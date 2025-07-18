use std::{
    ffi::OsStr,
    iter::Iterator,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use freedesktop_file_parser::EntryType;

use crate::cache::Cache;

pub static CRATE_NAME: &str = env!("CARGO_PKG_NAME");
pub static CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub static HOME_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::home_dir().expect("no homedir was found!"));
pub static APPLICATION_DIRS: LazyLock<Vec<PathBuf>> = LazyLock::new(|| {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    let mut application_path = HOME_DIR.clone();
    application_path.push(".local");
    application_path.push("share");
    application_path.push("applications");
    dirs.push(application_path);
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
        .map(Iterator::collect)
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
        Ok(_) => log::debug!("Running {cmd:?}"),
        Err(e) => log::warn!("Failed to run {cmd:?}: {e:?}"),
    }
}

pub fn locate_desktop_file(name: impl AsRef<Path> + Copy) -> Option<PathBuf> {
    APPLICATION_DIRS
        .iter()
        .map(|path| path.join(name))
        .find(|v| v.exists() && v.is_file())
}

pub fn run_in_terminal(cmd: &Command) {
    if let Some(terminal) = &*TERMINAL {
        let mut command = Command::new(terminal);
        command
            .arg("-e")
            .arg(cmd.get_program())
            .args(cmd.get_args());
        if let Some(curdir) = cmd.get_current_dir() {
            command.current_dir(curdir);
        }
        for (k, v) in cmd.get_envs() {
            match v {
                Some(v) => command.env(k, v),
                None => command.env_remove(k),
            };
        }
        run_cmd(command);
    } else {
        log::warn!("cannot run {cmd:?} in terminal because none is set.");
    }
}

pub fn open_link(file: impl AsRef<OsStr>) {
    let mut cmd = Command::new("xdg-open");
    cmd.arg(file);
    run_cmd(cmd);
}
pub fn open_file(file: impl Into<Arc<Path>>) {
    let file = file.into();
    log::debug!("opening {}", file.display());
    let mut cmd = Command::new("xdg-mime");
    cmd.arg("query").arg("filetype").arg(&*file);
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
            run_desktop_file(desktop_file, &file);
        });
    });
}

pub fn run_desktop_file(file: &DesktopFile, path: &Path) {
    let (exec, rest) = file.exec.split_once(' ').unwrap_or((&file.exec, ""));
    let mut cmd = Command::new(exec);
    for entry in rest.split(' ').filter(|v| !v.is_empty()) {
        if entry == "%u" || entry == "%f" || entry == "%F" || entry == "%U" {
            cmd.arg(path);
        } else {
            cmd.arg(entry);
        }
    }
    if let Some(cwd) = &file.cwd {
        cmd.current_dir(Path::new(&**cwd));
    }
    if file.terminal {
        run_in_terminal(&cmd);
    } else {
        run_cmd(cmd);
    }
}

pub fn with_desktop_file_info<R>(
    executable: &Path,
    func: impl FnOnce(&DesktopFile) -> R,
) -> Option<R> {
    if executable.is_relative() {
        let file = locate_desktop_file(executable)?.into();
        let Ok(mut cache) = DESKTOP_FILE_INFO_CACHE.write() else {
            log::info!("failed to write to the desktop file cache");
            return None;
        };
        return match cache.get_owned(file) {
            Ok(Some(file)) => Some(func(file)),
            _ => None,
        };
    }
    let Ok(mut cache) = DESKTOP_FILE_INFO_CACHE.write() else {
        log::info!("failed to write to the desktop file cache");
        return None;
    };
    match cache.get(executable, |v| v.into()) {
        Ok(Some(file)) => Some(func(file)),
        _ => None,
    }
}

pub struct DesktopFile {
    exec: Arc<str>,
    cwd: Option<Arc<str>>,
    terminal: bool,
}

impl TryFrom<freedesktop_file_parser::DesktopFile> for DesktopFile {
    type Error = ();

    fn try_from(value: freedesktop_file_parser::DesktopFile) -> Result<Self, Self::Error> {
        let EntryType::Application(app) = value.entry.entry_type else {
            return Err(());
        };
        Ok(Self {
            exec: match (app.exec, app.try_exec) {
                (Some(v), _) | (None, Some(v)) => v.into(),
                (None, None) => return Err(()),
            },
            terminal: app.terminal.unwrap_or_default(),
            cwd: app.path.map(Into::into),
        })
    }
}

type DesktopFileCache =
    Cache<Arc<Path>, DesktopFile, (), fn(Arc<Path>) -> Result<(Arc<Path>, DesktopFile), ()>>;

pub static DESKTOP_FILE_INFO_CACHE: LazyLock<RwLock<DesktopFileCache>> = LazyLock::new(|| {
    RwLock::new(Cache::new(
        |file| {
            let Ok(result) = std::fs::read_to_string(&file) else {
                return Err(());
            };
            let Ok(result) = freedesktop_file_parser::parse(&result) else {
                return Err(());
            };
            Ok((file, result.try_into()?))
        },
        Duration::from_secs(5 * 60),
    ))
});

pub static CONFIG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut buf = if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(value)
    } else {
        let mut buf = HOME_DIR.clone();
        buf.push(".config");
        buf
    };
    buf.push(CRATE_NAME);
    buf
});

pub static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut buf = if let Some(value) = std::env::var_os("XDG_DATA_HOME") {
        PathBuf::from(value)
    } else {
        let mut buf = HOME_DIR.clone();
        buf.push(".local");
        buf.push("share");
        buf
    };
    buf.push(CRATE_NAME);
    buf
});

pub static CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| CONFIG_DIR.join("config.toml"));
