use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

#[inline(always)]
fn def_false() -> bool {
    false
}

#[derive(Serialize, Deserialize)]
pub struct FileWatcherEntry {
    path: PathBuf,
    #[serde(default = "def_false")]
    watch: bool,
    reindex_every: Option<Duration>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Files {
    entries: Vec<FileWatcherEntry>,
    #[serde(default = "def_false")]
    reindex_at_startup: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlurAction {
    Refocus,
    Hide,
    #[default]
    None,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "Default::default")]
    files: Files,
    #[serde(default = "Default::default")]
    on_blur: BlurAction,
}
