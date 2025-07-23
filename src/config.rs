use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    fmt::{Debug, Display, Write},
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ScanFilter {
    pub ignore_hidden: bool,
    pub deny_paths: Vec<ArcPath>,
    pub deny_if_contains: Vec<ArcStr>,
    pub deny_if_starts: Vec<ArcStr>,
    pub deny_if_ends: Vec<ArcStr>,
    pub deny_if_is: Vec<ArcStr>,
}

impl Display for ScanFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ignore_hidden {
            f.write_str("any hidden files or directories or ones that start with a `.`\n")?;
        }
        for deny_path in &self.deny_paths {
            Display::fmt(&deny_path.display(), f)?;
            f.write_char('\n')?;
        }
        for value in &self.deny_if_contains {
            f.write_str("any file or directory that contains ")?;
            f.write_str(value)?;
            f.write_char('\n')?;
        }
        for value in &self.deny_if_ends {
            f.write_str("any file or directory that ends in ")?;
            f.write_str(value)?;
            f.write_char('\n')?;
        }
        for value in &self.deny_if_starts {
            f.write_str("any file or directory that starts with ")?;
            f.write_str(value)?;
            f.write_char('\n')?;
        }
        for value in &self.deny_if_is {
            f.write_str("any file or directory whose filename is ")?;
            f.write_str(value)?;
            f.write_char('\n')?;
        }
        Ok(())
    }
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

fn def_true() -> bool {
    true
}

fn def_false() -> bool {
    false
}

fn default_keybind() -> String {
    "Ctrl+Space".into()
}

fn none<T>() -> Option<T> {
    None
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FileWatcherEntry {
    pub path: ArcPath,
    #[serde(default = "def_false")]
    pub watch: bool,
    #[serde(default = "none")]
    pub reindex_every: Option<Duration>,
    #[serde(default = "<_>::default")]
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
        &self.0
    }
}
impl Deref for ArcStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
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

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Files {
    pub entries: Vec<FileWatcherEntry>,
    #[serde(default = "def_false")]
    pub reindex_at_startup: bool,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum BlurAction {
    Refocus,
    #[default]
    None,
}

impl Display for BlurAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "Default::default")]
    pub files: Files,
    #[serde(default = "Default::default")]
    pub on_blur: BlurAction,
    #[serde(default = "default_keybind")]
    pub keybind: String,
    #[serde(default = "HashSet::new")]
    pub enabled_plugins: HashSet<String>,
    #[serde(default = "def_true")]
    pub auto_resize: bool,
    #[serde(default = "Default::default", rename = "plugin")]
    pub plugin_settings: HashMap<String, PluginSettingsRoot>,
}

use crate::plugin_settings::PluginSettingsRoot;
#[allow(unused_imports)]
pub use crate::plugin_settings::{PluginSettings, PluginSettingsValue};
