// File plugin to search and index the entire drive (except a few directories)

use std::{ffi::OsStr, path::Path, sync::Arc};

use iced::Task;

use crate::{
    Action, CustomData, Entry, Message, Plugin, ResultBuilder, file_index::FILE_INDEX,
    matcher::MatcherInput, plugin::StringLike, utils,
};

#[derive(Default)]
pub struct FilePlugin;

fn iter<'a>(
    input: &MatcherInput,
    iter: impl Iterator<Item = &'a Arc<Path>>,
) -> impl Iterator<Item = Entry> {
    iter.filter(|path| path_matches(input, path))
        .map(|v| (v.clone(), v.file_name().map(|v| v.len()).unwrap_or(0)))
        .map(|(v, filename_len)| {
            let mut name = StringLike::from(v.clone());
            name.substr((name.len() - filename_len) as u16..);
            let mut subtitle = StringLike::from(v.clone());
            subtitle.substr(..(subtitle.len() - filename_len) as u16);
            Entry {
                name,
                subtitle,
                plugin: FilePlugin.prefix(),
                data: CustomData::new(v),
            }
        })
}

impl Plugin for FilePlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "file"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let Some(index) = FILE_INDEX.get() else {
            return;
        };
        let reader = index.read().await;
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

    fn init(&mut self) {}

    fn handle_pre(&self, thing: CustomData, _: &str) -> Task<Message> {
        let path = thing.into::<Arc<Path>>();
        utils::open_file(path);
        Task::none()
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Open", "")] }
    }
}

fn path_matches(input: &MatcherInput, path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(|v| input.matches(v))
        .unwrap_or(false)
}
