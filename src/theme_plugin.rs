use std::{borrow::Cow, sync::LazyLock};

use iced::{Task, Theme};

use crate::{Action, CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput};

static THEMES: LazyLock<Vec<(String, Theme)>> = LazyLock::new(|| {
    Theme::ALL
        .iter()
        .map(|theme| (format!("{theme}"), theme.clone()))
        .collect()
});

#[derive(Default)]
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "theme"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let iter = THEMES
            .iter()
            .filter(|&v| input.matches(&v.0))
            .map(|v| Entry {
                name: v.0.clone(),
                subtitle: Cow::Borrowed(""),
                plugin: self.prefix(),
                data: CustomData::new(v.1.clone()),
            });
        builder.commit(iter).await;
    }

    fn init(&mut self) {}

    fn handle(&self, thing: CustomData, _: &str) -> iced::Task<Message> {
        Task::done(Message::ChangeTheme(thing.into()))
    }

    fn actions(&self) -> &'static [Action] {
        const { &[Action::default("Apply Theme", "").keep_open()] }
    }
}
