use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, mpsc::channel},
    time::SystemTime,
};

use notify::{RecommendedWatcher, Watcher};
use serde::{Deserialize, Serialize};

pub struct NewFileIndex {
    children: HashMap<PathBuf, FileIndexData>,
    watcher: Arc<RecommendedWatcher>,
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
