// File plugin to search and index the entire drive (except a few directories)

use std::{ffi::OsStr, path::Path, process::Command, sync::Arc};

use iced::{
    Task,
    keyboard::{Key, Modifiers, key::Named},
};

use crate::{
    Action, CustomData, Entry, Message, PluginContext, ResultBuilderRef, StructPlugin,
    matcher::MatcherInput, plugin::StringLike, utils,
};

#[derive(Default)]
pub struct FilePlugin;

fn iter<'a>(
    input: &MatcherInput,
    iter: impl Iterator<Item = &'a Arc<Path>>,
) -> impl Iterator<Item = Entry> {
    iter.filter_map(|path| path_matches(input, path).map(|v| (path, v)))
        .map(|(v, perfect_match)| {
            (
                v.clone(),
                v.file_name().map_or(0, OsStr::len),
                perfect_match,
            )
        })
        .map(|(v, filename_len, perfect_match)| {
            let mut name = StringLike::from(v.clone());
            name.substr((name.len() - filename_len) as u16..);
            let mut subtitle = StringLike::from(v.clone());
            subtitle.substr(..(subtitle.len() - filename_len) as u16);
            Entry {
                name,
                subtitle,
                data: CustomData::new(v),
                perfect_match,
            }
        })
}

impl StructPlugin for FilePlugin {
    fn prefix() -> &'static str {
        "file"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        context: PluginContext<'_>,
    ) {
        let reader = context.file_index.read().await;
        let iter = iter(
            input,
            reader
                .children
                .values()
                .flat_map(|v| v.paths.iter())
                .map(|v| &v.0),
        );
        builder.commit(iter).await;
    }

    async fn init(&mut self, _: PluginContext<'_>) {}

    fn handle_pre(&self, thing: CustomData, action: &str, _: PluginContext<'_>) -> Task<Message> {
        let path = thing.into::<Arc<Path>>();
        if action == "open" {
            utils::open_file(path);
        } else if let Some(terminal) = &*utils::TERMINAL {
            let mut cmd = Command::new(terminal);
            cmd.current_dir(path);
            utils::run_cmd(cmd);
        }
        Task::none()
    }

    fn actions(&self) -> &'static [Action] {
        const {
            &[
                Action::default("Open", "open"),
                Action::new(
                    "Open in terminal",
                    "terminal",
                    (Modifiers::CTRL, Key::Named(Named::Enter)),
                ),
            ]
        }
    }
}

fn path_matches(input: &MatcherInput, path: &Path) -> Option<bool> {
    path.file_name()
        .and_then(OsStr::to_str)
        .and_then(|v| input.matches_perfect(v))
}
