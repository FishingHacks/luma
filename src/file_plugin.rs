// File plugin to search and index the entire drive (except a few directories)

use std::{collections::HashSet, env::home_dir, ffi::OsStr, path::Path, sync::Arc, time::Instant};

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
    Action, CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput,
    plugin::StringLike, utils,
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
                "Indexed {} File(s) and {} Directorie(s) in {elapsed:.3?}",
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
    pub directories: Vec<Arc<Path>>,
    pub files: Vec<Arc<Path>>,
}

pub struct Indexer {
    directory_queue: Vec<Arc<Path>>,
    files: HashSet<Arc<Path>>,
    directories: HashSet<Arc<Path>>,
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
            directory_queue: vec![
                Path::new("/mnt").into(),
                Path::new("/run/media").into(),
                Path::new("/media").into(),
                homedir.into(),
            ],
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
            let path: Arc<Path> = entry.path().into();
            if ftype.is_dir() && self.directories.insert(path.clone()) {
                self.directory_queue.push(path);
                continue;
            }
            if ftype.is_file() {
                self.files.insert(path);
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

fn iter<'a>(
    input: &MatcherInput,
    iter: impl Iterator<Item = (usize, &'a Arc<Path>)>,
) -> impl Iterator<Item = Entry> {
    iter.filter(|(_, path)| path_matches(input, path))
        .map(|(i, v)| (i, v.clone(), v.file_name().map(|v| v.len()).unwrap_or(0)))
        .map(|(i, v, filename_len)| {
            let mut name = StringLike::from(v.clone());
            name.substr((name.len() - filename_len) as u16..);
            let mut subtitle = StringLike::from(v);
            subtitle.substr(..(subtitle.len() - filename_len) as u16);
            Entry {
                name,
                subtitle,
                plugin: FilePlugin.prefix(),
                data: CustomData::new((true, i)),
            }
        })
}

impl Plugin for FilePlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "file"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let reader = FILE_INDEX.read_blocking();
        let files = iter(input, reader.files.iter().enumerate());
        let iter = iter(input, reader.directories.iter().enumerate()).chain(files);
        builder.commit(iter).await
    }

    fn init(&mut self) {}

    fn handle_pre(&self, thing: CustomData, _: &str) -> Task<Message> {
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
        utils::open_file(&*arr[index]);
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
