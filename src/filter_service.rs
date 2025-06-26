use std::{
    cmp,
    pin::{Pin, pin},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
    time::Duration,
};

use iced::futures::{
    SinkExt,
    channel::mpsc::{self, SendError, Sender},
    future::BoxFuture,
};
use iced::{
    futures::{Stream, StreamExt},
    stream::channel,
};
use tokio::sync::RwLock;

use crate::{AnyPlugin, Context, Entry, GenericEntry, matcher::MatcherInput};

#[derive(Clone, Copy)]
pub struct ResultBuilderRef<'a> {
    plugin_id: usize,
    builder: &'a ResultBuilder,
}

impl<'a> ResultBuilderRef<'a> {
    pub(crate) fn create(plugin_id: usize, builder: &'a ResultBuilder) -> Self {
        Self { plugin_id, builder }
    }

    /// returns false if you should stop adding entries.
    pub async fn add(&self, entry: Entry) -> bool {
        self.builder
            .commit(std::iter::once(GenericEntry {
                name: entry.name,
                subtitle: entry.subtitle,
                plugin: self.plugin_id,
                data: entry.data,
                perfect_match: entry.perfect_match,
            }))
            .await
    }

    /// returns false if you should stop adding entries.
    pub async fn commit(&self, iter: impl Iterator<Item = Entry>) -> bool {
        self.builder
            .commit(iter.map(|entry| GenericEntry {
                name: entry.name,
                subtitle: entry.subtitle,
                plugin: self.plugin_id,
                data: entry.data,
                perfect_match: entry.perfect_match,
            }))
            .await
    }

    pub fn should_stop(&self) -> bool {
        self.builder.should_stop()
    }
}

#[derive(Default)]
pub struct ResultBuilder {
    results: RwLock<Vec<GenericEntry>>,
    should_stop: Arc<AtomicBool>,
}

impl ResultBuilder {
    /// returns false if you should stop adding entries.
    pub async fn commit(&self, iter: impl Iterator<Item = GenericEntry>) -> bool {
        if self.should_stop.load(Ordering::Relaxed) {
            return false;
        }
        let mut writer = self.results.write().await;
        for entry in iter {
            writer.push(entry);
            if self.should_stop.load(Ordering::Relaxed) {
                return false;
            }
        }
        true
    }

    pub fn to_inner(&self) -> &RwLock<Vec<GenericEntry>> {
        &self.results
    }

    pub fn should_stop(&self) -> bool {
        self.should_stop.load(Ordering::Relaxed)
    }

    pub fn get_should_stop(&self) -> Arc<AtomicBool> {
        self.should_stop.clone()
    }
}

enum Action {
    Stop,
    Start(Arc<Vec<Box<dyn AnyPlugin>>>, String, Arc<AtomicBool>),
    Context(Context),
}

#[derive(Debug, Clone)]
pub enum CollectorMessage {
    Ready(CollectorController),
    Finished(Vec<GenericEntry>),
}

#[derive(Debug, Clone)]
pub struct CollectorController {
    sender: Sender<Action>,
    stop: Arc<AtomicBool>,
}

impl CollectorController {
    pub fn start(&mut self, plugins: Arc<Vec<Box<dyn AnyPlugin>>>, query: String) -> bool {
        self.stop();
        self.stop = Arc::default();
        match self
            .sender
            .try_send(Action::Start(plugins, query, self.stop.clone()))
        {
            Err(e) if e.is_disconnected() => {
                log::debug!("Failed to start a collection cycle: {e:?}");
                return false;
            }
            Err(e) if e.is_full() => {
                log::error!("failed to start a collection cycle: {e:?} (this is very bad)");
            }
            _ => {}
        }
        true
    }

    pub fn stop(&mut self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            match self.sender.try_send(Action::Stop) {
                Err(e) if e.is_disconnected() => {
                    log::debug!("Failed to stop a collection cycle: {e:?}");
                }
                Err(e) => log::error!(
                    "failed to stop the collection cycle: {e:?} (this is **extremely** bad)"
                ),
                _ => {}
            }
        }
    }

    pub(crate) fn init(&mut self, ctx: Context) {
        match self.sender.try_send(Action::Context(ctx)) {
            Ok(()) => {}
            Err(e) if e.is_full() => {
                unreachable!("the channel should never be full at the first message")
            }
            Err(e) => log::error!("the collector exited early during init: {e:?}"),
        }
    }
}

pub fn collector() -> impl Stream<Item = CollectorMessage> {
    channel(32, |mut output: mpsc::Sender<_>| async move {
        let (sender, mut receiver) = mpsc::channel(20);
        match output.try_send(CollectorMessage::Ready(CollectorController {
            sender,
            stop: Arc::default(),
        })) {
            Ok(()) => (),
            Err(e) if e.is_full() => unreachable!("this channel can't be full"),
            Err(e) => {
                log::debug!("stopping the collector: {e:?}");
                return;
            }
        }
        let context = loop {
            match receiver.next().await {
                Some(Action::Context(ctx)) => break ctx,
                Some(_) => {}
                None => {
                    log::debug!("stopping collector: main thread exited");
                    return;
                }
            }
        };

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("failed to run tokio collector runtime");
            rt.block_on(async {
                loop {
                    let (plugins, mut query, should_stop) = match StreamExt::next(&mut receiver)
                        .await
                    {
                        Some(Action::Context(_)) => unreachable!(),
                        Some(Action::Stop) => continue,
                        Some(Action::Start(plugins, query, stop_bool)) => {
                            (plugins, query, stop_bool)
                        }
                        None => {
                            return log::debug!(
                                "action sender was dropped, stopping the search result collection."
                            );
                        }
                    };
                    let mut next_message_fn = async || _ = receiver.next().await;
                    let result_builder = ResultBuilder {
                        results: RwLock::default(),
                        should_stop,
                    };

                    let mut futures = 'block: {
                        for (id, plugin) in plugins.iter().enumerate() {
                            if query.starts_with(plugin.any_prefix()) {
                                query.drain(..plugin.any_prefix().len());
                                let input = Arc::new(MatcherInput::new(query, true));
                                break 'block vec![plugin.any_get_for_values(
                                    input,
                                    &result_builder,
                                    id,
                                    context.clone(),
                                )];
                            }
                        }

                        let input = Arc::new(MatcherInput::new(query, false));
                        plugins
                            .iter()
                            .enumerate()
                            .map(|(id, plugin)| {
                                plugin.any_get_for_values(
                                    input.clone(),
                                    &result_builder,
                                    id,
                                    context.clone(),
                                )
                            })
                            .collect::<Vec<_>>()
                    };

                    let mut sent_previously = usize::MAX;
                    loop {
                        if futures.is_empty() {
                            break;
                        }
                        let next_msg = pin!(next_message_fn());
                        // https://preview.redd.it/7nv2i903ezba1.png?width=320&crop=smart&auto=webp&s=8c198937d80657b642b857b9a49346f48f49a0d9
                        let the_eeper = pin!(tokio::time::sleep(Duration::from_millis(200)));
                        let res = Joinall(futures, the_eeper, next_msg).await;
                        match res {
                            JoinAllResult::Abort => break,
                            JoinAllResult::Done(moved_futures) => futures = moved_futures,
                        }
                        let mut writer = result_builder.to_inner().write().await;
                        if writer.len() == sent_previously {
                            continue;
                        }
                        sent_previously = writer.len();
                        let mut entries = if futures.is_empty() {
                            std::mem::take(&mut *writer)
                        } else {
                            writer.clone()
                        };
                        drop(writer);
                        entries.sort_by(|a, b| {
                            if a.perfect_match == b.perfect_match {
                                cmp::Ordering::Equal
                            } else if a.perfect_match {
                                cmp::Ordering::Less
                            } else {
                                cmp::Ordering::Greater
                            }
                        });
                        let res = output.send(CollectorMessage::Finished(entries)).await;
                        if handle_send_result(res) {
                            return;
                        }
                    }
                }
            });
            rt.shutdown_timeout(Duration::from_secs(10));
        });
    })
}

fn handle_send_result(res: Result<(), SendError>) -> bool {
    match res {
        Ok(()) => false,
        Err(e) if e.is_full() => {
            log::debug!("Error: Frontend is not responding: {e:?}");
            true
        }
        Err(e) if e.is_disconnected() => {
            log::debug!("collector receiver is disconnected, exiting: {e:?}");
            true
        }
        Err(e) => {
            log::info!("Collector Error: {e:?}");
            true
        }
    }
}

struct Joinall<'a, 'b, Eeper: Future, F: Future>(Vec<BoxFuture<'a, ()>>, Pin<&'b mut Eeper>, F);

enum JoinAllResult<'a> {
    Done(Vec<BoxFuture<'a, ()>>),
    Abort,
}

impl<'a, Eeper: Future, F: Future + Unpin> Future for Joinall<'a, '_, Eeper, F> {
    type Output = JoinAllResult<'a>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0
            .retain_mut(|fut| matches!(fut.as_mut().poll(cx), Poll::Pending));
        if self.0.is_empty() || self.1.as_mut().poll(cx).is_ready() {
            Poll::Ready(JoinAllResult::Done(std::mem::take(&mut self.0)))
        } else if pin!(&mut self.2).poll(cx).is_ready() {
            return Poll::Ready(JoinAllResult::Abort);
        } else {
            Poll::Pending
        }
    }
}
