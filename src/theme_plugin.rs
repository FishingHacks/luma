use std::sync::LazyLock;

use iced::{Task, Theme};

use crate::{
    Action, Context, CustomData, Entry, Message, Plugin, ResultBuilderRef, matcher::MatcherInput,
    plugin::StringLike,
};

static THEMES: LazyLock<Vec<(String, Theme)>> = LazyLock::new(|| {
    Theme::ALL
        .iter()
        .map(|theme| (format!("{theme}"), theme.clone()))
        .collect()
});

#[derive(Default)]
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn prefix(&self) -> &'static str {
        "theme"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: Context,
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

    fn init(&mut self, _: Context) {}

    fn handle_pre(&self, thing: CustomData, _: &str, _: Context) -> iced::Task<Message> {
        Task::done(Message::ChangeTheme(thing.into::<Theme>()))
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Apply Theme", "").keep_open()] }
    }
}
