use std::sync::LazyLock;

use iced::{Task, Theme};

use crate::{
    Action, CustomData, Entry, Message, PluginContext, ResultBuilderRef,
    matcher::MatcherInput,
    plugin::{StringLike, StructPlugin},
};

static THEMES: LazyLock<Vec<(String, Theme)>> = LazyLock::new(|| {
    Theme::ALL
        .iter()
        .map(|theme| (format!("{theme}"), theme.clone()))
        .collect()
});

#[derive(Default)]
pub struct ThemePlugin;

impl StructPlugin for ThemePlugin {
    fn prefix() -> &'static str {
        "theme"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: PluginContext<'_>,
    ) {
        let iter = THEMES.iter().filter(|&v| input.matches(&v.0)).map(|v| {
            Entry::new(
                v.0.clone(),
                StringLike::Empty,
                CustomData::new::<Theme>(v.1.clone()),
            )
        });
        builder.commit(iter).await;
    }

    async fn init(&mut self, _: PluginContext<'_>) {}

    fn handle_pre(&self, thing: CustomData, _: &str, _: PluginContext<'_>) -> iced::Task<Message> {
        Task::done(Message::ChangeTheme(thing.into::<Theme>()))
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Apply Theme", "").keep_open()] }
    }
}
