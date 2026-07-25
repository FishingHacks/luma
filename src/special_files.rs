use std::{path::PathBuf, sync::LazyLock};

pub use crate::utils::{CONFIG_DIR, DATA_DIR, RUNTIME_DIR, STATE_DIR};

pub static LOG_FILE: LazyLock<PathBuf> = LazyLock::new(|| STATE_DIR.join("latest.log"));
pub static SQLITE_FILE: LazyLock<PathBuf> = LazyLock::new(|| DATA_DIR.join("cache.sqlite"));
pub static FILE_INDEX_FILE: LazyLock<PathBuf> = LazyLock::new(|| DATA_DIR.join("file_index.toml"));
pub static CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| CONFIG_DIR.join("config.toml"));

pub static SOCKET_FILE: LazyLock<PathBuf> = LazyLock::new(|| RUNTIME_DIR.join("socket"));
