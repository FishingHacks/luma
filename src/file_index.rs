use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, mpsc::channel},
    time::SystemTime,
};

use notify::{ErrorKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use smol::{lock::RwLock, stream::StreamExt as _};

pub struct ScanFilter {
    ignore_hidden: bool,
    deny_paths: Vec<PathBuf>,
    deny_if_contains: Vec<String>,
    deny_if_starts: Vec<String>,
    deny_if_ends: Vec<String>,
    deny_if_is: Vec<String>,
}

impl ScanFilter {
    pub fn is_allowed(&self, path: &Path) -> bool {
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            return true;
        };
        if self.ignore_hidden && file_name.starts_with('.') {
            return false;
        }
        for entry in self.deny_if_starts.iter() {
            if file_name.starts_with(entry) {
                return false;
            }
        }
        for entry in self.deny_if_ends.iter() {
            if file_name.ends_with(entry) {
                return false;
            }
        }
        for entry in self.deny_if_is.iter() {
            if file_name == entry {
                return false;
            }
        }
        for entry in self.deny_if_contains.iter() {
            if file_name.contains(entry) {
                return false;
            }
        }
        for entry in self.deny_paths.iter() {
            if path == entry {
                return false;
            }
        }
        true
    }
}

pub struct NewFileIndex {
    children: HashMap<PathBuf, FileIndexData>,
    watcher: Arc<RwLock<RecommendedWatcher>>,
}

#[derive(Serialize, Deserialize)]
pub struct FileIndexData {
    paths: Vec<PathBuf>,
    next_scan: Option<SystemTime>,
}

pub fn meow() {
    let (tx, rx) = channel();
    let watcher = notify::recommended_watcher(tx).unwrap();
}

pub struct FileIndexer {
    entries: HashSet<PathBuf>,
    queue: Vec<PathBuf>,
    denied: HashSet<PathBuf>,
    other_indexed_dirs: HashSet<PathBuf>,
    watcher: Option<Arc<RwLock<RecommendedWatcher>>>,
    scanfilter: ScanFilter,
}

impl FileIndexer {
    pub async fn new<'a>(
        root: PathBuf,
        indexed_dirs: impl Iterator<Item = &'a Path>,
        scanfilter: ScanFilter,
        mut watcher: Option<Arc<RwLock<RecommendedWatcher>>>,
    ) -> Self {
        let other_indexed_dirs = indexed_dirs
            .filter(|v| *v != root)
            .map(Into::into)
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
                        log::error!("While watching {}: {e}", root.display())
                    }
                    ErrorKind::Io(e) => log::error!("While watching {}: {e}", root.display()),
                    ErrorKind::PathNotFound | ErrorKind::WatchNotFound => unreachable!(),
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
            queue: vec![root],
            denied: HashSet::new(),
            other_indexed_dirs,
            watcher,
            scanfilter,
        }
    }

    pub fn into_inner(self) -> Vec<PathBuf> {
        assert!(self.queue.is_empty());
        let mut entries: Vec<_> = self.entries.into_iter().collect();
        entries.sort_unstable();
        entries
    }

    pub async fn cycle(&mut self) -> bool {
        let Some(directory) = self.queue.pop() else {
            return false;
        };
        let mut dirent = match smol::fs::read_dir(&directory).await {
            Ok(v) => v,
            Err(e) => {
                log::debug!("Failed to read {}: {e}", directory.display());
                return true;
            }
        };
        self.entries.insert(directory);
        loop {
            let entry = dirent.try_next().await;
            let entry = match entry {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(_) => continue,
            };
            let path = entry.path();
            if self.entries.contains(&path) {
                continue;
            } else if self.denied.contains(&path) || !self.scanfilter.is_allowed(&path) {
                self.denied.insert(path);
                continue;
            } else if !self.entries.insert(path) {
                continue;
            }
            let Ok(ftype) = entry.file_type().await else {
                continue;
            };
            if !ftype.is_dir() {
                continue;
            }
            let path = entry.path();
            if let Some(watcher) = &self.watcher {
                let res = watcher
                    .write()
                    .await
                    .watch(&path, RecursiveMode::NonRecursive);
                if let Err(e) = res {
                    self.watcher = None;
                    match e.kind {
                        ErrorKind::Generic(e) => {
                            log::error!("While watching {}: {e}", path.display())
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
