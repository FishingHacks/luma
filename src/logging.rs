use std::{
    fs::OpenOptions,
    path::PathBuf,
    process::Command,
    sync::{LazyLock, OnceLock, RwLock},
};

use env_logger::{Target, WriteStyle};
use log::{Level, LevelFilter, Log, Metadata, Record};

use crate::{
    Message,
    special_windows::SpecialWindowState,
    utils::{self, CRATE_NAME},
};

pub struct Logger {
    stderr: env_logger::Logger,
    file: env_logger::Logger,
}

#[allow(clippy::type_complexity)]
static SENDER: OnceLock<RwLock<Box<dyn Send + Sync + FnMut(Message)>>> = OnceLock::new();
pub static LOG_FILE: LazyLock<PathBuf> = LazyLock::new(|| utils::DATA_DIR.join("latest.log"));

pub fn register_message_sender(sender: impl FnMut(Message) + Send + Sync + 'static) {
    SENDER
        .set(RwLock::new(Box::new(sender)))
        .ok()
        .expect("sender is already set");
}

pub fn init() {
    let stderr_logger = env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .filter_module("wgpu_hal", LevelFilter::Error)
        .filter_module("wgpu_core", LevelFilter::Info)
        .filter_module("naga", LevelFilter::Info)
        .filter_module("cosmic_text", LevelFilter::Info)
        .filter_module("iced_winit", LevelFilter::Warn)
        .filter_module("iced_wgpu", LevelFilter::Warn)
        .parse_default_env()
        .build();
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&*LOG_FILE)
        .unwrap();
    let file_logger = env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .filter_module("wgpu_hal", LevelFilter::Error)
        .filter_module("wgpu_core", LevelFilter::Info)
        .filter_module("naga", LevelFilter::Info)
        .filter_module("cosmic_text", LevelFilter::Info)
        .filter_module("iced_winit", LevelFilter::Warn)
        .filter_module("iced_wgpu", LevelFilter::Warn)
        .target(Target::Pipe(Box::new(file)))
        .parse_default_env()
        .write_style(WriteStyle::Never)
        .build();
    let max_level = stderr_logger
        .filter()
        .max(file_logger.filter())
        .max(LevelFilter::Debug);
    let logger = Logger {
        stderr: stderr_logger,
        file: file_logger,
    };
    log::set_boxed_logger(Box::new(logger)).expect("failed to setup the logger");
    log::set_max_level(max_level);
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.stderr.enabled(metadata)
            || self.file.enabled(metadata)
            || metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.stderr.enabled(record.metadata()) {
            self.stderr.log(record);
        }
        if self.file.enabled(record.metadata()) {
            self.file.log(record);
        }
        let fmt = record.args();
        match record.level() {
            Level::Error => {
                let Some(sender) = SENDER.get() else { return };
                let msg = match record.module_path() {
                    Some(v) if v.starts_with(CRATE_NAME) => format!("{fmt}"),
                    None => format!("{fmt}"),
                    Some(v) => format!("[{v}]: {fmt}"),
                };
                (sender.write().expect("failed to write"))(Message::OpenSpecial(
                    SpecialWindowState::new_error_popup(msg),
                ));
            }
            Level::Warn => {
                let Some(sender) = SENDER.get() else { return };
                let msg = match record.module_path() {
                    Some(v) if v.starts_with(CRATE_NAME) => format!("{fmt}"),
                    _ => return,
                };
                (sender.write().expect("failed to write"))(Message::OpenSpecial(
                    SpecialWindowState::new_warning_popup(msg),
                ));
            }
            Level::Info => {
                let Some(path) = record.module_path() else {
                    return;
                };
                if path.starts_with(CRATE_NAME) {
                    let mut cmd = Command::new("notify-send");
                    cmd.arg(format!("{fmt}"));
                    utils::run_cmd(cmd);
                }
            }
            _ => {}
        }
    }

    fn flush(&self) {
        self.stderr.flush();
        self.file.flush();
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        self.flush();
    }
}
