use std::borrow::Cow;

use iced::Task;

use crate::{CustomData, Entry, Message, Plugin, ResultBuilder, matcher::MatcherInput};

#[derive(Clone, Copy)]
pub enum Action {
    Quit,
    Hide,
}

impl Action {
    pub const fn get_name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Hide => "hide",
        }
    }
    pub const fn get_description(self) -> &'static str {
        match self {
            Action::Quit => {
                "Exit the runner (This will exit it entirely, not just hide the window)."
            }
            Action::Hide => "Hides the window",
        }
    }
}

static ACTIONS: &[Action] = &[Action::Quit, Action::Hide];

#[derive(Default)]
pub struct ControlPlugin;

impl Plugin for ControlPlugin {
    #[inline(always)]
    fn prefix(&self) -> &'static str {
        "control"
    }

    async fn get_for_values(&self, input: &MatcherInput<'_>, builder: &ResultBuilder) {
        let iter = ACTIONS
            .iter()
            .filter(|&action| input.matches(action.get_name()))
            .map(|action| Entry {
                name: action.get_name().to_string(),
                subtitle: Cow::Borrowed(action.get_description()),
                plugin: self.prefix(),
                data: CustomData::new(*action),
            });
        builder.commit(iter).await;
    }

    fn init(&mut self) {}

    fn handle(&self, thing: CustomData) -> iced::Task<Message> {
        match thing.into::<Action>() {
            Action::Quit => Task::done(Message::Exit),
            Action::Hide => Task::done(Message::Hide),
        }
    }
}
