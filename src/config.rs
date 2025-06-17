use std::{
    borrow::Borrow,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct ScanFilter {
    pub ignore_hidden: bool,
    pub deny_paths: Vec<ArcPath>,
    pub deny_if_contains: Vec<ArcStr>,
    pub deny_if_starts: Vec<ArcStr>,
    pub deny_if_ends: Vec<ArcStr>,
    pub deny_if_is: Vec<ArcStr>,
}

impl Default for ScanFilter {
    fn default() -> Self {
        Self {
            ignore_hidden: true,
            deny_paths: Vec::new(),
            deny_if_contains: Vec::new(),
            deny_if_starts: Vec::new(),
            deny_if_ends: Vec::new(),
            deny_if_is: vec!["target".into(), "node_modules".into()],
        }
    }
}

#[inline(always)]
fn def_false() -> bool {
    false
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileWatcherEntry {
    pub path: ArcPath,
    #[serde(default = "def_false")]
    pub watch: bool,
    pub reindex_every: Option<Duration>,
    pub filter: ScanFilter,
}

#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArcStr(pub Arc<str>);

impl<'de> Deserialize<'de> for ArcStr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Into::into).map(Self)
    }
}

impl Serialize for ArcStr {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        str::serialize(&self.0, serializer)
    }
}

#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArcPath(pub Arc<Path>);

impl Borrow<Path> for ArcPath {
    fn borrow(&self) -> &Path {
        self.0.borrow()
    }
}

impl<'de> Deserialize<'de> for ArcPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        PathBuf::deserialize(deserializer).map(Into::into).map(Self)
    }
}

impl Serialize for ArcPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        Path::serialize(&self.0, serializer)
    }
}

impl Deref for ArcPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}
impl Deref for ArcStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl From<&Path> for ArcPath {
    fn from(value: &Path) -> Self {
        Self(value.into())
    }
}

impl From<&str> for ArcStr {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Files {
    pub entries: Vec<FileWatcherEntry>,
    #[serde(default = "def_false")]
    pub reindex_at_startup: bool,
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
    pub files: Files,
    #[serde(default = "Default::default")]
    pub on_blur: BlurAction,
}
