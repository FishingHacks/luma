use std::{
    ffi::OsStr,
    ops::Deref,
    path::PathBuf,
    process::{Command, Stdio},
    sync::LazyLock,
};

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
        Ok(_) => println!("Running {cmd:?}"),
        Err(e) => eprintln!("Failed to run {cmd:?}: {e:?}"),
    }
}

pub fn run(cmd: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) {
    let mut cmd = Command::new(cmd);
    cmd.args(args);
    run_cmd(cmd)
}

pub fn locate_desktop_file(name: &str) -> Option<PathBuf> {
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
        eprintln!("canno run {cmd:?} in terminal because none is set.");
    }
}

// pub fn run_desktop_file(file: &Path) {
//     let contents = match std::fs::read_to_string(file) {
//         Ok(v) => v,
//         Err(e) => {
//             eprintln!("failed to load desktop file from {}: {e:?}", file.display());
//             return;
//         }
//     };
//     let parsed = match freedesktop_file_parser::parse(&contents) {
//         Ok(v) => v,
//         Err(e) => {
//             eprintln!("failed to load desktop file from {}: {e:?}", file.display());
//             return;
//         }
//     };
//     parsed.
// }
