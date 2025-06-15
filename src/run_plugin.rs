use std::{collections::HashSet, process::Command};

use freedesktop_file_parser::EntryType;
use iced::Task;

use crate::{CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput, utils};

struct FileEntry {
    name: String,
    terminal: bool,
    exec: String,
    description: String,
}

#[derive(Default)]
pub struct RunPlugin {
    files: Vec<FileEntry>,
}

impl Plugin for RunPlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "run"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let iter = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                input.matches(&v.name)
                    || (input.matches(&v.description) && !v.description.is_empty())
            })
            .map(|(i, v)| Entry {
                name: v.name.clone(),
                subtitle: v.description.clone().into(),
                plugin: self.prefix(),
                data: CustomData::new(i),
            });
        builder.commit(iter).await;
    }

    fn init(&mut self) {
        let mut file_entries = Vec::new();
        let mut programs = HashSet::new();
        for dir in utils::DATA_DIRS.iter() {
            let Ok(dirent) = dir.read_dir() else { continue };
            for entry in dirent {
                let Ok(entry) = entry else { continue };
                let Ok(contents) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                let Ok(parsed) = freedesktop_file_parser::parse(&contents) else {
                    continue;
                };
                let application = match parsed.entry.entry_type {
                    EntryType::Application(application) => application,
                    _ => continue,
                };
                let name = parsed.entry.name.get_variant("en");
                if programs.contains(name) {
                    continue;
                }
                programs.insert(name.to_string());
                let Some(mut exec) = application.exec else {
                    continue;
                };
                if let Some(pos) = exec.find("%u") {
                    exec.replace_range(pos..pos + 2, "");
                }
                if let Some(pos) = exec.find("%U") {
                    exec.replace_range(pos..pos + 2, "");
                }
                if let Some(pos) = exec.find("%f") {
                    exec.replace_range(pos..pos + 2, "");
                }
                if let Some(pos) = exec.find("%F") {
                    exec.replace_range(pos..pos + 2, "");
                }
                file_entries.push(FileEntry {
                    name: name.to_string(),
                    terminal: application.terminal.unwrap_or(false),
                    exec,
                    description: parsed
                        .entry
                        .comment
                        .map(|v| v.get_variant("en").to_string())
                        .unwrap_or_default(),
                });
            }
        }
        self.files = file_entries;
    }

    fn handle(&self, thing: CustomData) -> iced::Task<Message> {
        let file = &self.files[thing.into::<usize>()];
        let mut split = file.exec.split(' ');
        let Some(command) = split.next() else {
            return Task::none();
        };
        let mut command = Command::new(command);
        command.args(split);
        if file.terminal {
            utils::run_in_terminal(command);
        } else {
            utils::run_cmd(command);
        }
        Task::none()
    }
}
