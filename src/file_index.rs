use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    pin::pin,
    sync::{Arc, LazyLock, OnceLock},
    task::Poll,
    time::{Duration, Instant, SystemTime},
};

use iced::futures::{
    FutureExt as _, SinkExt, Stream,
    channel::mpsc::{self},
};
use notify::{
    ErrorKind, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, RemoveKind},
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{
        RwLock,
        mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel},
    },
    time::sleep,
};

use crate::{
    config::{ArcPath, Config, FileWatcherEntry, ScanFilter},
    utils::{self, CONFIG_FILE},
};

#[derive(Clone)]
pub enum FileIndexMessage {
    SetConfig(Arc<Config>),
    Reindex(Arc<Path>),
}

#[derive(Debug, Clone)]
pub enum FileIndexResponse {
    Starting(UnboundedSender<FileIndexMessage>),
    IndexFinished,
}

pub fn file_index_service() -> impl Stream<Item = FileIndexResponse> {
    iced::stream::channel(100, async |mut output: mpsc::Sender<_>| {
        let (sender, mut receiver) = unbounded_channel();
        let (event_sender, event_receiver) = unbounded_channel();
        let file_index = load_fileindex(move |ev| {
            if let Ok(ev) = ev {
                if !matches!(ev.kind, EventKind::Create(_) | EventKind::Remove(_)) {
                    return;
                }
                match event_sender.send(ev) {
                    Ok(()) => {}
                    Err(_) => log::error!(
                        "the file indexing is stopped, but events are still received. this should never happen. you might want to restart the application and reindex directories you're watching."
                    ),
                }
            }
        });
        let Some(mut file_index) = file_index.await else {
            log::debug!("Stopping file indexing");
            return;
        };
        output
            .send(FileIndexResponse::Starting(sender))
            .await
            .expect("the application exited but this is somehow still running");
        let config = loop {
            match receiver.recv().await {
                Some(FileIndexMessage::SetConfig(config)) => break config,
                Some(FileIndexMessage::Reindex(_)) => {}
                None => {
                    log::debug!(
                        "Stopping file indexing: main thread didn't send a config before quitting"
                    );
                    return;
                }
            }
        };
        let files = &config.files;
        let mut queue = HashSet::new();
        for entry in &files.entries {
            if file_index.config.contains_key(&*entry.path) {
                log::error!(
                    "The config contains multiple entries for {}.\nPlease edit the config at {}",
                    entry.path.display(),
                    CONFIG_FILE.display()
                );
                return;
            }
            let path = entry.path.clone();
            if files.reindex_at_startup || !file_index.children.contains_key(&path) {
                queue.insert(path.clone());
            }
            file_index.config.insert(path.0, entry.clone());
        }
        run_thread(file_index, receiver, event_receiver, output, queue);
    })
}

fn run_thread(
    mut file_index: FileIndex,
    mut receiver: UnboundedReceiver<FileIndexMessage>,
    mut event_receiver: UnboundedReceiver<notify::Event>,
    mut output: iced::futures::channel::mpsc::Sender<FileIndexResponse>,
    mut queue: HashSet<ArcPath>,
) {
    std::thread::spawn(move || {
        let mut watcher = file_index.watcher.blocking_write();
        log::debug!("Starting to watch directories...");
        file_index
            .children
            .values_mut()
            .for_each(|v| v.start_watching(&mut watcher));
        log::debug!("All directories are being watched...");
        drop(watcher);
        FILE_INDEX
            .set(Arc::new(file_index.into()))
            .ok()
            .expect("the file indexing service was started multiple times");
        let mut prev_file_msg = None;
        let mut prev_event = None;
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("this should never fail");
        rt.block_on(async move {
            loop {
                let res = main_loop(
                    &mut receiver,
                    &mut event_receiver,
                    &mut output,
                    &mut queue,
                    prev_file_msg.take(),
                    prev_event.take(),
                )
                .await;
                match res {
                    MainLoopResult::Stop => break,
                    MainLoopResult::Working => {}
                    MainLoopResult::Idle => {
                        let fut1 = receiver.recv().map(Ok);
                        let fut2 = event_receiver.recv().map(Err);
                        let mut fut1 = pin!(fut1);
                        let mut fut2 = pin!(fut2);
                        let fut = std::future::poll_fn(|cx| {
                            if let Poll::Ready(v) = fut1.as_mut().poll(cx) {
                                return Poll::Ready(v);
                            }
                            if let Poll::Ready(v) = fut2.as_mut().poll(cx) {
                                return Poll::Ready(v);
                            }
                            Poll::Pending
                        });
                        match fut.await {
                            Ok(Some(v)) => prev_file_msg = Some(v),
                            Err(Some(v)) => prev_event = Some(v),
                            _ => {}
                        }
                    }
                }
            }
        });
        rt.shutdown_timeout(Duration::from_secs(10));
        log::debug!("Shutting down file indexer");
    });
}

enum MainLoopResult {
    Stop,
    Working,
    Idle,
}

async fn main_loop(
    receiver: &mut UnboundedReceiver<FileIndexMessage>,
    event_receiver: &mut UnboundedReceiver<notify::Event>,
    output: &mut mpsc::Sender<FileIndexResponse>,
    queue: &mut HashSet<ArcPath>,
    mut prev_file_idx_msg: Option<FileIndexMessage>,
    mut prev_event: Option<notify::Event>,
) -> MainLoopResult {
    // deal with any requests. this is because we do the queue next, and it'd be really stupid to
    // reindex a directory just to add it back to the reindexing queue immediately afterwards.
    loop {
        match prev_file_idx_msg
            .take()
            .map_or_else(|| receiver.try_recv(), Ok)
        {
            Ok(FileIndexMessage::Reindex(path)) => _ = queue.insert(ArcPath(path)),
            Ok(FileIndexMessage::SetConfig(_)) => todo!(),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return MainLoopResult::Stop,
        }
    }
    let index = FILE_INDEX.get().expect("file index should be initialized");
    let notify = if let Some(path) = queue.iter().next().cloned() {
        queue.remove(&path);
        log::info!("Indexing {}", path.display());
        FileIndex::index(index.clone(), &path).await;
        true
    } else {
        false
    };
    let result = if queue.is_empty() {
        MainLoopResult::Idle
    } else {
        MainLoopResult::Working
    };
    if event_receiver.is_empty() && prev_event.is_none() {
        if notify {
            match output.send(FileIndexResponse::IndexFinished).await {
                Ok(()) => {}
                // don't care about full
                Err(e) if e.is_full() => {}
                Err(e) => {
                    update_file_index(index).await;
                    log::debug!("Shutting down indexer: {e:?}");
                    return MainLoopResult::Stop;
                }
            }
            update_file_index(index).await;
        }
        return result;
    }
    if event_receiver.is_closed() && prev_event.is_none() {
        return result;
    }
    if event_receiver.is_empty() {
        // wait 10 seconds and collect all events, so we don't get overwhelmed.
        sleep(Duration::from_secs(10)).await;
    }

    let mut writer = index.write().await;
    let mut watcher = writer.watcher.clone().write_owned().await;
    log::debug!("got watch events");
    while !event_receiver.is_empty() || prev_event.is_some() {
        let ev = match prev_event.take() {
            Some(e) => e,
            None => match event_receiver.recv().await {
                Some(ev) => ev,
                None => break,
            },
        };
        if ev.need_rescan() {
            log::info!("Note: deal with need_rescan");
        }
        match ev.kind {
            EventKind::Create(kind @ (CreateKind::File | CreateKind::Folder)) => {
                for path in &ev.paths {
                    let Some(data) = writer.get_file_data(path) else {
                        continue;
                    };
                    let path = ArcPath((&**path).into());
                    if data.paths.insert(path.clone()) && kind == CreateKind::Folder {
                        if data.watched {
                            if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
                                log::debug!("cannot watch path {}: {e:?}", path.display());
                            }
                        }
                        data.directories.insert(path);
                    }
                }
            }
            EventKind::Remove(RemoveKind::File | RemoveKind::Folder) => {
                for path in &ev.paths {
                    let Some(data) = writer.get_file_data(path) else {
                        continue;
                    };
                    if !data.paths.remove(&**path) {
                        continue;
                    }
                    if !data.directories.remove(&**path) {
                        continue;
                    }
                    if let Err(e) = watcher.unwatch(path) {
                        if !matches!(e.kind, ErrorKind::WatchNotFound) {
                            log::debug!("Failed to unwatch {}: {e:?}", path.display());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    drop(watcher);
    drop(writer);

    match output.send(FileIndexResponse::IndexFinished).await {
        Ok(()) => {}
        // don't care about full
        Err(e) if e.is_full() => {}
        Err(e) => {
            update_file_index(index).await;
            log::debug!("Shutting down indexer: {e:?}");
            return MainLoopResult::Stop;
        }
    }
    update_file_index(index).await;
    result
}

pub static FILE_INDEX: OnceLock<Arc<RwLock<FileIndex>>> = OnceLock::new();

pub static INDEX_FILE_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| utils::DATA_DIR.join("file_index.toml"));

async fn load_fileindex(
    event_handler: impl Fn(Result<notify::Event, notify::Error>) + Send + 'static,
) -> Option<FileIndex> {
    let children = if let Ok(data) = tokio::fs::read_to_string(&*INDEX_FILE_DIR).await {
        match toml::from_str(&data) {
            Ok(v) => v,
            Err(e) => {
                log::error!(
                    "Failed to read the file index: {e:?}. you can either delete or fix up the file index at {}.",
                    INDEX_FILE_DIR.display()
                );
                return None;
            }
        }
    } else {
        HashMap::new()
    };
    let watcher = match notify::recommended_watcher(event_handler) {
        Ok(v) => Arc::new(v.into()),
        Err(e) => {
            log::error!("Failed to start the watcher: {e:?}");
            return None;
        }
    };
    Some(FileIndex {
        children,
        watcher,
        config: HashMap::new(),
    })
}

async fn update_file_index(index: &RwLock<FileIndex>) -> bool {
    let reader = index.read().await;
    let string = match toml::to_string(&reader.children) {
        Ok(v) => v,
        Err(e) => {
            log::error!("Failed to write the file index: {e:?}");
            return false;
        }
    };
    let parent = INDEX_FILE_DIR
        .parent()
        .expect("A file should always have a parent");
    if let Err(e) = tokio::fs::create_dir_all(parent).await {
        log::error!("Failed to create the path {}: {e:?}", parent.display());
        return false;
    }
    if let Err(e) = tokio::fs::write(&*INDEX_FILE_DIR, string).await {
        log::error!("Failed to write the file index: {e:?}");
        return false;
    }
    true
}

impl ScanFilter {
    pub fn is_allowed(&self, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            return true;
        };
        if self.ignore_hidden && file_name.starts_with('.') {
            return false;
        }
        for entry in &self.deny_if_starts {
            if file_name.starts_with(&**entry) {
                return false;
            }
        }
        for entry in &self.deny_if_ends {
            if file_name.ends_with(&**entry) {
                return false;
            }
        }
        for entry in &self.deny_if_is {
            if *file_name == **entry {
                return false;
            }
        }
        for entry in &self.deny_if_contains {
            if file_name.contains(&**entry) {
                return false;
            }
        }
        for entry in &self.deny_paths {
            if *path == **entry {
                return false;
            }
        }
        true
    }
}

pub struct FileIndex {
    pub children: HashMap<ArcPath, FileIndexData>,
    watcher: Arc<RwLock<RecommendedWatcher>>,
    config: HashMap<Arc<Path>, FileWatcherEntry>,
}

impl FileIndex {
    pub fn get_file_data(&mut self, path: &Path) -> Option<&mut FileIndexData> {
        let mut iter = self
            .children
            .iter_mut()
            .filter(|(k, _)| path.starts_with(&***k));
        let mut result = iter.next()?;
        // get the most fitting path (e.g. for /, ~/ and
        // ~/.config/rust-analyzer if the path is ~/.config/rust-analyzer/config.toml,
        // this would return the FileIndexData associated with ~/.config/rust-analyzer.)
        for value in iter {
            if value.0.as_os_str().len() > result.0.as_os_str().len() {
                result = value;
            }
        }
        Some(result.1)
    }

    pub async fn index(me: Arc<RwLock<Self>>, path: &Path) -> bool {
        let now = Instant::now();
        let reader = me.read().await;
        let Some((path, config)) = reader
            .config
            .get_key_value(path)
            .map(|(path, config)| (path.clone(), config.clone()))
        else {
            return false;
        };
        let mut indexer = FileIndexer::new(
            path.clone(),
            reader.config.keys(),
            config.filter,
            config.watch.then(|| reader.watcher.clone()),
        )
        .await;
        drop(reader);
        FileIndex::remove(&me, &path).await;
        while indexer.cycle().await {}
        let next_scan = config.reindex_every.map(|v| SystemTime::now() + v);
        let file_index_data = indexer.into_data(next_scan);
        let amount = file_index_data.paths.len();
        let mut writer = me.write().await;
        writer
            .children
            .insert(ArcPath(path.clone()), file_index_data);
        let remove = !writer.config.contains_key(&path);
        drop(writer);
        if remove {
            Self::remove(&me, &path).await;
        }
        log::info!(
            "Indexed {amount} files and directories in {:.3?}",
            now.elapsed()
        );
        true
    }

    async fn remove(me: &RwLock<Self>, path: &Path) {
        let mut writer = me.write().await;
        let Some(indexed_data) = writer.children.remove(path) else {
            return;
        };
        let watcher = writer.watcher.clone();
        drop(writer);
        let mut watcher = watcher.write().await;
        let mut did_popup = false;
        for dir in &indexed_data.directories {
            let Err(e) = watcher.unwatch(dir) else {
                continue;
            };
            if did_popup {
                log::debug!(
                    "Failed to unwatch the {} and potentially more: {e:?}",
                    dir.display()
                );
            } else {
                log::error!(
                    "Failed to unwatch the {} and potentially more: {e:?}",
                    dir.display()
                );
                did_popup = true;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct FileIndexData {
    pub paths: HashSet<ArcPath>,
    directories: HashSet<ArcPath>,
    next_scan: Option<SystemTime>,
    watched: bool,
}

impl FileIndexData {
    pub fn start_watching(&mut self, watcher: &mut RecommendedWatcher) {
        let mut did_err = false;
        self.directories.retain(|dir| {
            if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
                match e.kind {
                    ErrorKind::PathNotFound | ErrorKind::WatchNotFound => return false,
                    ErrorKind::Io(e) if e.kind() == std::io::ErrorKind::NotFound => return false,
                    _ => {}
                }
                if did_err {
                    return true;
                }
                did_err = true;
                match e.kind {
                    ErrorKind::Generic(e) => {
                        log::error!("While watching {}: {e}", dir.display());
                    }
                    ErrorKind::Io(e) => log::error!("While watching {}: {e}", dir.display()),
                    ErrorKind::PathNotFound | ErrorKind::WatchNotFound => return false,
                    ErrorKind::InvalidConfig(_) => log::error!(
                        "An invalid config was passed onto the watcher. This should never happen."
                    ),
                    ErrorKind::MaxFilesWatch => {
                        log::error!(
                            "max files watchable reached. Increase the limit or stop {} from being watched.\nFurther directories of this or parent paths may not be watched and will not register changes.",
                            dir.display()
                        );
                    }
                }
                return true;
            }
            true
        });
    }
}

pub struct FileIndexer {
    entries: HashSet<ArcPath>,
    dirs: HashSet<ArcPath>,
    queue: Vec<Arc<Path>>,
    denied: HashSet<Arc<Path>>,
    other_indexed_dirs: HashSet<Arc<Path>>,
    watcher: Option<Arc<RwLock<RecommendedWatcher>>>,
    scanfilter: ScanFilter,
}

impl FileIndexer {
    pub async fn new<'a>(
        root: Arc<Path>,
        indexed_dirs: impl Iterator<Item = &'a Arc<Path>>,
        scanfilter: ScanFilter,
        mut watcher: Option<Arc<RwLock<RecommendedWatcher>>>,
    ) -> Self {
        let other_indexed_dirs = indexed_dirs
            .filter(|v| **v != root)
            .map(Clone::clone)
            .collect();

        if let Some(watcher_ref) = &watcher {
            let res = watcher_ref
                .write()
                .await
                .watch(&root, RecursiveMode::NonRecursive);
            if let Err(e) = res {
                watcher = None;
                match e.kind {
                    ErrorKind::Generic(e) => {
                        log::error!("While watching {}: {e}", root.display());
                    }
                    ErrorKind::Io(e) => log::error!("While watching {}: {e}", root.display()),
                    ErrorKind::PathNotFound => {
                        log::error!("While watching {}: path not found", root.display());
                    }
                    ErrorKind::WatchNotFound => unreachable!(),
                    ErrorKind::InvalidConfig(_) => log::error!(
                        "An invalid config was passed onto the watcher. This should never happen."
                    ),
                    ErrorKind::MaxFilesWatch => {
                        log::error!(
                            "max files watchable reached. Increase the limit or stop {} from being watched.\nFurther directories of this or parent paths may not be watched and will not register changes.",
                            root.display()
                        );
                    }
                }
            }
        }
        Self {
            entries: HashSet::new(),
            queue: vec![root.clone()],
            denied: HashSet::new(),
            other_indexed_dirs,
            watcher,
            scanfilter,
            dirs: [ArcPath(root)].into_iter().collect(),
        }
    }

    pub fn into_data(self, next_scan: Option<SystemTime>) -> FileIndexData {
        assert!(self.queue.is_empty());
        FileIndexData {
            paths: self.entries,
            directories: self.dirs,
            next_scan,
            watched: self.watcher.is_some(),
        }
    }

    pub async fn cycle(&mut self) -> bool {
        let Some(directory) = self.queue.pop() else {
            return false;
        };
        if self.other_indexed_dirs.contains(&directory) {
            return true;
        }
        let mut dirent = match tokio::fs::read_dir(&directory).await {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Failed to read {}: {e}", directory.display());
                return true;
            }
        };
        self.entries.insert(ArcPath(directory));
        loop {
            let entry = dirent.next_entry().await;
            let entry = match entry {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => continue,
            };
            let path: Arc<_> = entry.path().into();
            if self.entries.contains(&*path) || self.other_indexed_dirs.contains(&*path) {
                continue;
            }
            if self.denied.contains(&path) || !self.scanfilter.is_allowed(&path) {
                self.denied.insert(path);
                continue;
            }
            if !self.entries.insert(ArcPath(path.clone())) {
                continue;
            }
            let Ok(ftype) = entry.file_type().await else {
                continue;
            };
            if !ftype.is_dir() {
                continue;
            }
            self.dirs.insert(ArcPath(path.clone()));
            if let Some(watcher) = &self.watcher {
                let res = watcher
                    .write()
                    .await
                    .watch(&path, RecursiveMode::NonRecursive);
                if let Err(e) = res {
                    self.watcher = None;
                    match e.kind {
                        ErrorKind::Generic(e) => {
                            log::error!("While watching {}: {e}", path.display());
                        }
                        ErrorKind::Io(e) => log::error!("While watching {}: {e}", path.display()),
                        ErrorKind::PathNotFound | ErrorKind::WatchNotFound => unreachable!(),
                        ErrorKind::InvalidConfig(_) => log::error!(
                            "An invalid config was passed onto the watcher. This should never happen."
                        ),
                        ErrorKind::MaxFilesWatch => {
                            log::error!(
                                "max files watchable reached. Increase the limit or stop {} from being watched.\nFurther directories of this or parent paths may not be watched and will not register changes.",
                                path.display()
                            );
                        }
                    }
                }
            }
            self.queue.push(path);
        }
        true
    }
}
