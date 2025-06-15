// File plugin to search and index the entire drive (except a few directories)

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    env::home_dir,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Instant, SystemTime},
};

use iced::{
    Task,
    futures::{
        Stream, StreamExt,
        channel::mpsc::{self, Sender},
    },
    stream::channel,
};
use smol::{lock::RwLock, stream::StreamExt as _};

use crate::{
    Action, CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput, utils,
};

static IGNORED_DIRECTORIES: &[&str] = &["node_modules", "target"];
static FILE_INDEX: RwLock<FileIndex> = RwLock::new(FileIndex {
    directories: Vec::new(),
    files: Vec::new(),
});

#[derive(Debug, Clone)]
pub enum IndexerMessage {
    Ready(Sender<()>),
    IndexingFinished,
}

/// starts an indexing thread and returns a sender, that can be used to initiate another indexing
/// or close the thread by closing the senders.
pub fn start_indexer() -> impl Stream<Item = IndexerMessage> {
    channel(2, |mut output| async move {
        let (sender, mut receiver) = mpsc::channel(1);
        match output.try_send(IndexerMessage::Ready(sender)) {
            Ok(_) => (),
            Err(e) if e.is_full() => unreachable!("this channel can't be full"),
            Err(e) => {
                log::debug!("stopping the file indexer: {e:?}");
                return;
            }
        }

        loop {
            if StreamExt::next(&mut receiver).await.is_none() {
                log::debug!("sender was dropped, stopping file indexer");
                return;
            }
            log::debug!("Indexing files...");

            let mut indexer = Indexer::new();
            let now = Instant::now();
            while indexer.cycle().await {}
            let elapsed = now.elapsed();
            let index = indexer.finish();
            log::info!(
                "Indexed {} File(s) and {} Directorie(s) in {elapsed:?}",
                index.files.len(),
                index.directories.len(),
            );
            let mut writer = FILE_INDEX.write().await;
            *writer = index;
            drop(writer);

            match output.try_send(IndexerMessage::IndexingFinished) {
                Ok(_) => (),
                // if the channel is full, that means there are 2 unhandled finishes. It is fine to
                // not send one in this case because the state is gonna work with the new data
                // anwyay.
                Err(e) if e.is_full() => (),
                Err(e) => {
                    log::debug!("stopping the file indexer: {e:?}");
                    return;
                }
            }
        }
    })
}

pub struct FileIndex {
    pub directories: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

pub struct Indexer {
    directory_queue: Vec<PathBuf>,
    files: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
}

fn should_scan(path: &Path) -> bool {
    match path.file_name().and_then(OsStr::to_str) {
        Some(name) => !name.starts_with('.') && !IGNORED_DIRECTORIES.contains(&name),
        _ => true,
    }
}

impl Indexer {
    pub fn new() -> Self {
        let Some(homedir) = home_dir() else {
            panic!("Home directory not set")
        };
        Self {
            directory_queue: vec!["/mnt".into(), "/run/media".into(), "/media".into(), homedir],
            files: HashSet::new(),
            directories: HashSet::new(),
        }
    }

    pub async fn cycle(&mut self) -> bool {
        let Some(directory) = self.directory_queue.pop() else {
            return false;
        };
        if !should_scan(&directory) {
            return true;
        }
        let mut dirent = match smol::fs::read_dir(&directory).await {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Failed to read {}: {e}", directory.display());
                return true;
            }
        };
        self.directories.insert(directory);
        loop {
            let entry = dirent.try_next().await;
            let entry = match entry {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => continue,
            };
            let Ok(ftype) = entry.file_type().await else {
                continue;
            };
            if ftype.is_dir() && self.directories.insert(entry.path()) {
                self.directory_queue.push(entry.path());
            }
            if ftype.is_file() {
                self.files.insert(entry.path());
            }
        }
        true
    }

    pub fn finish(self) -> FileIndex {
        assert!(self.directory_queue.is_empty());
        let mut index = FileIndex {
            directories: self.directories.into_iter().collect(),
            files: self.files.into_iter().collect(),
        };
        index.directories.sort_unstable();
        index.files.sort_unstable();
        index
    }
}

#[derive(Default)]
pub struct FilePlugin;

impl Plugin for FilePlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "file"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let reader = FILE_INDEX.read_blocking();
        let dirs = reader
            .directories
            .iter()
            .enumerate()
            .filter(|(_, path)| path_matches(input, path))
            .filter_map(|(i, v)| Some((i, v.as_os_str().to_str()?)))
            .map(|(i, v)| Entry {
                name: v.to_string(),
                subtitle: Cow::Borrowed(""),
                plugin: self.prefix(),
                data: CustomData::new((false, i)),
            });
        let files = reader
            .files
            .iter()
            .enumerate()
            .filter(|(_, path)| path_matches(input, path))
            .filter_map(|(i, v)| Some((i, v.as_os_str().to_str()?)))
            .map(|(i, v)| Entry {
                name: v.to_string(),
                subtitle: Cow::Borrowed(""),
                plugin: self.prefix(),
                data: CustomData::new((true, i)),
            });
        builder.commit(dirs.chain(files)).await
    }

    fn init(&mut self) {}

    fn handle(&self, thing: CustomData, _: &str) -> Task<Message> {
        let (is_file, index) = thing.into::<(bool, usize)>();
        let reader = FILE_INDEX.read_blocking();
        let arr = if is_file {
            &reader.files
        } else {
            &reader.directories
        };
        if arr.len() <= index {
            return Task::none();
        }
        utils::open_file(&arr[index]);
        drop(reader);
        Task::none()
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Open", "")] }
    }
}

fn path_matches(input: &MatcherInput, path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(|v| input.matches(v))
        .unwrap_or(false)
}
