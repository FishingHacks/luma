use std::{
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

use crate::{AnyPlugin, Entry, matcher::MatcherInput};

#[derive(Default)]
pub struct ResultBuilder {
    results: RwLock<Vec<Entry>>,
    should_stop: Arc<AtomicBool>,
}

impl ResultBuilder {
    pub async fn commit(&self, iter: impl Iterator<Item = Entry>) {
        if self.should_stop.load(Ordering::Relaxed) {
            return;
        }
        let mut writer = self.results.write().await;
        for entry in iter {
            writer.push(entry);
            if self.should_stop.load(Ordering::Relaxed) {
                return;
            }
        }
    }

    pub fn to_inner(self) -> Vec<Entry> {
        self.results.into_inner()
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
    Finished(Vec<Entry>),
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
                eprintln!("Failed to start a collection cycle: {e:?}");
                return false;
            }
            _ => {}
        }
        true
    }

    pub fn stop(&mut self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            match self.sender.try_send(Action::Stop) {
                Err(e) if e.is_disconnected() => {
                    eprintln!("Failed to start a collection cycle: {e:?}");
                }
                Err(e) => eprintln!(
                    "failed to stop the collection cycle: {e:?} (this is **extremely** bad)"
                ),
                _ => {}
            }
        }
    }
}

pub fn collector() -> impl Stream<Item = CollectorMessage> {
    channel(32, |mut output| async move {
        let (sender, mut receiver) = mpsc::channel(20);
        match output.try_send(CollectorMessage::Ready(CollectorController {
            sender,
            stop: Arc::default(),
        })) {
            Ok(_) => (),
            Err(e) if e.is_full() => unreachable!("this channel can't be full"),
            Err(e) => {
                eprintln!("stopping the file indexer: {e:?}");
                return;
            }
        }

        std::thread::spawn(move || {
            smol::future::block_on(async {
                'main: loop {
                    let (plugins, query, should_stop) = match StreamExt::next(&mut receiver).await {
                        Some(Action::Stop) => continue,
                        Some(Action::Start(plugins, query, stop_bool)) => {
                            (plugins, query, stop_bool)
                        }
                        None => {
                            return eprintln!(
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
                    for plugin in plugins.iter() {
                        if query.starts_with(plugin.any_prefix()) {
                            let input =
                                MatcherInput::new(query[plugin.any_prefix().len()..].trim());
                            let future = Either(
                                plugin.any_get_for_values(&input, &result_builder),
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

                    let input = MatcherInput::new(query.trim());
                    let futures = Joinall(
                        plugins
                            .iter()
                            .map(|v| v.any_get_for_values(&input, &result_builder))
                            .collect(),
                        next_msg,
                    );
                    if futures.await {
                        let res = output
                            .send(CollectorMessage::Finished(result_builder.to_inner()))
                            .await;
                        if handle_send_result(res) {
                            return;
                        }
                    }
                }
            })
        });
    })
}

fn handle_send_result(res: Result<(), SendError>) -> bool {
    match res {
        Ok(_) => false,
        Err(e) if e.is_full() => {
            eprintln!("Error: Frontend is not responding: {e:?}");
            false
        }
        Err(e) if e.is_disconnected() => {
            eprintln!("collector receiver is disconnected, exiting: {e:?}");
            true
        }
        Err(e) => {
            eprintln!("Collector Error: {e:?}");
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
