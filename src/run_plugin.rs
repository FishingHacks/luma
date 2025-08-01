use std::{collections::HashSet, path::Path, process::Command, sync::Arc};

use freedesktop_file_parser::EntryType;
use iced::{
    Task,
    advanced::graphics::core::SmolStr,
    keyboard::{Key, Modifiers},
};

use crate::{
    Action, CustomData, Entry, Message, PluginContext, ResultBuilderRef, StructPlugin,
    matcher::MatcherInput, utils,
};

struct FileEntry {
    name: Arc<str>,
    terminal: bool,
    exec: Arc<str>,
    description: Arc<str>,
    path: Arc<Path>,
}

#[derive(Default)]
pub struct RunPlugin {
    files: Vec<FileEntry>,
}

impl StructPlugin for RunPlugin {
    fn prefix() -> &'static str {
        "run"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: PluginContext<'_>,
    ) {
        let iter = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                input.matches(&v.name)
                    || (input.matches(&v.description) && !v.description.is_empty())
            })
            .map(|(i, v)| Entry::new(v.name.clone(), v.description.clone(), CustomData::new(i)));
        builder.commit(iter).await;
    }

    async fn init(&mut self, _: PluginContext<'_>) {
        let mut file_entries = Vec::new();
        let mut programs = HashSet::new();
        for dir in utils::APPLICATION_DIRS.iter() {
            let Ok(mut dirent) = tokio::fs::read_dir(dir).await else {
                continue;
            };
            loop {
                let Ok(entry) = dirent.next_entry().await else {
                    continue;
                };
                let Some(entry) = entry else { break };
                let path = entry.path();
                let Ok(contents) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(parsed) = freedesktop_file_parser::parse(&contents) else {
                    continue;
                };
                if parsed.entry.no_display.unwrap_or(false) {
                    continue;
                }
                let EntryType::Application(application) = parsed.entry.entry_type else {
                    continue;
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
                    name: name.into(),
                    terminal: application.terminal.unwrap_or(false),
                    exec: exec.into(),
                    description: parsed
                        .entry
                        .comment
                        .map(|v| v.get_variant("en").into())
                        .unwrap_or_default(),
                    path: path.into(),
                });
            }
        }
        self.files = file_entries;
    }

    fn handle_pre(
        &self,
        thing: CustomData,
        action: &str,
        _: PluginContext<'_>,
    ) -> iced::Task<Message> {
        let file = &self.files[thing.into::<usize>()];

        if action == "run" {
            let mut split = file.exec.split(' ');
            let Some(command) = split.next() else {
                return Task::none();
            };
            let mut command = Command::new(command);
            command.args(split);
            if file.terminal {
                utils::run_in_terminal(&command);
            } else {
                utils::run_cmd(command);
            }
        } else {
            utils::open_file(&*file.path);
        }
        Task::none()
    }

    fn actions(&self) -> &'static [Action] {
        const {
            &[
                Action::default("Run Program", "run"),
                Action::new(
                    "Open Desktop Entry",
                    "open",
                    (Modifiers::CTRL, Key::Character(SmolStr::new_inline("o"))),
                ),
            ]
        }
    }
}
