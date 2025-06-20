use std::{
    cmp,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
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
use smol::{future::FutureExt, lock::RwLock};

use crate::{AnyPlugin, Entry, GenericEntry, matcher::MatcherInput};

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

    pub fn to_inner(self) -> Vec<GenericEntry> {
        self.results.into_inner()
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

        std::thread::spawn(move || {
            smol::future::block_on(async {
                'main: loop {
                    let (plugins, mut query, should_stop) = match StreamExt::next(&mut receiver)
                        .await
                    {
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
                    let next_msg = pin!(next_message_fn());
                    let result_builder = ResultBuilder {
                        results: RwLock::default(),
                        should_stop,
                    };
                    for (id, plugin) in plugins.iter().enumerate() {
                        if query.starts_with(plugin.any_prefix()) {
                            query.drain(..plugin.any_prefix().len());
                            let input = Arc::new(MatcherInput::new(query, true));
                            let future = Either(
                                plugin.any_get_for_values(input, &result_builder, id),
                                next_msg,
                            );
                            if future.await {
                                let res = output
                                    .send(CollectorMessage::Finished(result_builder.to_inner()))
                                    .await;
                                if handle_send_result(res) {
                                    return;
                                }
                            }
                            continue 'main;
                        }
                    }

                    let input = Arc::new(MatcherInput::new(query, false));
                    let futures = Joinall(
                        plugins
                            .iter()
                            .enumerate()
                            .map(|(id, plugin)| {
                                plugin.any_get_for_values(input.clone(), &result_builder, id)
                            })
                            .collect(),
                        next_msg,
                    );
                    if futures.await {
                        let mut entries = result_builder.to_inner();
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
        });
    })
}

fn handle_send_result(res: Result<(), SendError>) -> bool {
    match res {
        Ok(()) => false,
        Err(e) if e.is_full() => {
            log::debug!("Error: Frontend is not responding: {e:?}");
            false
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

struct Joinall<'a, F: Future<Output = ()>>(Vec<BoxFuture<'a, ()>>, F);

impl<F: Future<Output = ()> + Unpin> Future for Joinall<'_, F> {
    type Output = bool;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.0
            .retain_mut(|fut| matches!(fut.poll(cx), Poll::Pending));
        if self.0.is_empty() {
            Poll::Ready(true)
        } else if self.1.poll(cx).is_ready() {
            return Poll::Ready(false);
        } else {
            Poll::Pending
        }
    }
}

struct Either<F1: Future<Output = ()> + Unpin, F2: Future<Output = ()> + Unpin>(F1, F2);

impl<F1: Future<Output = ()> + Unpin, F2: Future<Output = ()> + Unpin> Future for Either<F1, F2> {
    type Output = bool;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.0.poll(cx).is_ready() {
            return Poll::Ready(true);
        }
        if self.1.poll(cx).is_ready() {
            return Poll::Ready(false);
        }
        Poll::Pending
    }
}
