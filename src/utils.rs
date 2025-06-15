use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{LazyLock, RwLock},
    time::Duration,
};

use freedesktop_file_parser::{DesktopFile, EntryType};

use crate::cache::Cache;

pub static DATA_DIRS: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Some(mut application_path) = std::env::home_dir() {
        application_path.push(".local");
        application_path.push("share");
        application_path.push("applications");
        dirs.push(application_path);
    }
    if let Some(data_dir_var) = std::env::var_os("XDG_DATA_DIRS") {
        std::env::split_paths(&data_dir_var)
            .filter_map(|v| v.canonicalize().ok())
            .for_each(|d| {
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            });
    }
    dirs
});

pub static EXECUTABLE_PATHS: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    std::env::var_os("PATH")
        .as_deref()
        .map(std::env::split_paths)
        .map(|v| v.collect())
        .unwrap_or_default()
});

pub static TERMINAL: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    if let Some(path) = std::env::var_os("TERMINAL") {
        let path = PathBuf::from(path);
        if path.exists() && path.is_file() {
            return Some(path);
        }
        if let Some(path) = lookup_executable(path.as_os_str()) {
            return Some(path);
        }
    }
    if let Some(path) = std::env::var_os("TERM") {
        let path = PathBuf::from(path);
        if path.exists() && path.is_file() {
            return Some(path);
        }
        return lookup_executable(path.as_os_str());
    }
    None
});

pub fn lookup_executable(executable: &OsStr) -> Option<PathBuf> {
    EXECUTABLE_PATHS
        .iter()
        .map(|v| v.join(executable))
        .find(|path| path.exists() && path.is_file())
}

pub fn run_cmd(mut cmd: Command) {
    match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => log::trace!("Running {cmd:?}"),
        Err(e) => log::warn!("Failed to run {cmd:?}: {e:?}"),
    }
}

pub fn run(cmd: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) {
    let mut cmd = Command::new(cmd);
    cmd.args(args);
    run_cmd(cmd)
}

pub fn locate_desktop_file(name: impl AsRef<Path> + Copy) -> Option<PathBuf> {
    DATA_DIRS
        .iter()
        .map(|path| path.join(name))
        .find(|v| v.exists() && v.is_file())
}

pub fn run_in_terminal(cmd: Command) {
    if let Some(terminal) = TERMINAL.deref() {
        let mut command = Command::new(terminal);
        command
            .arg("-e")
            .arg(cmd.get_program())
            .args(cmd.get_args());
        run_cmd(command);
    } else {
        log::warn!("cannot run {cmd:?} in terminal because none is set.");
    }
}

pub fn open_file(file: impl Into<PathBuf>) {
    let file = file.into();
    let mut cmd = Command::new("xdg-mime");
    cmd.arg("query").arg("filetype").arg(&file);
    std::thread::spawn(move || {
        let output = match cmd.output() {
            Ok(output) if output.status.success() => output.stdout,
            _ => return,
        };
        let Ok(output) = str::from_utf8(&output) else {
            return;
        };
        let output = output.lines().next().unwrap_or_default();
        cmd = Command::new("xdg-mime");
        cmd.arg("query").arg("default").arg(output);
        let output = match cmd.output() {
            Ok(output) if output.status.success() => output.stdout,
            _ => return,
        };
        let Ok(output) = str::from_utf8(&output) else {
            return;
        };
        let output = output.lines().next().unwrap_or_default();
        with_desktop_file_info(Path::new(output), |desktop_file| {
            run_desktop_file(desktop_file, &file)
        });
    });
}

pub fn run_desktop_file(file: &DesktopFile, path: &Path) {
    let application = match &file.entry.entry_type {
        EntryType::Application(application) => application,
        _ => return,
    };
    let Some(ref exec) = application.exec.clone() else {
        return;
    };
    let (exec, rest) = exec.split_once(' ').unwrap_or((exec, ""));
    let mut cmd = Command::new(exec);
    for entry in rest.split(' ').filter(|v| !v.is_empty()) {
        if entry == "%u" || entry == "%f" || entry == "%F" || entry == "%U" {
            cmd.arg(path);
        } else {
            cmd.arg(entry);
        }
    }
    if application.terminal == Some(true) {
        run_in_terminal(cmd);
    } else {
        run_cmd(cmd);
    }
}

pub fn with_desktop_file_info<R>(
    executable: &Path,
    func: impl FnOnce(&DesktopFile) -> R,
) -> Option<R> {
    let executable = match executable.is_relative() {
        false => Cow::Borrowed(executable),
        true => Cow::Owned(locate_desktop_file(executable)?),
    };
    let Ok(mut cache) = DESKTOP_FILE_INFO_CACHE.write() else {
        log::info!("failed to write to the desktop file cache");
        return None;
    };
    match cache.get_cow(executable) {
        Ok(Some(file)) => Some(func(file)),
        _ => None,
    }
}

type DesktopFileCache =
    Cache<PathBuf, DesktopFile, (), fn(PathBuf) -> Result<(PathBuf, DesktopFile), ()>>;

static DESKTOP_FILE_INFO_CACHE: LazyLock<RwLock<DesktopFileCache>> = LazyLock::new(|| {
    RwLock::new(Cache::new(
        |file| {
            let Ok(result) = std::fs::read_to_string(&file) else {
                return Err(());
            };
            let Ok(result) = freedesktop_file_parser::parse(&result) else {
                return Err(());
            };
            Ok((file, result))
        },
        Duration::from_secs(5 * 60),
    ))
});
