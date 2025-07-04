use iced::Task;

use crate::{CustomData, Entry, Message, Plugin, ResultBuilderRef, matcher::MatcherInput, utils};

#[derive(Clone, Copy)]
pub enum Action {
    Quit,
    Hide,
    ShowLogs,
}

impl Action {
    pub const fn get_name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Hide => "hide",
            Action::ShowLogs => "logs",
        }
    }
    pub const fn get_description(self) -> &'static str {
        match self {
            Action::Quit => {
                "Exit the runner (This will exit it entirely, not just hide the window)."
            }
            Action::Hide => "Hides the window",
            Action::ShowLogs => "Open the latest application logs",
        }
    }
}

static ACTIONS: &[Action] = &[Action::Quit, Action::Hide, Action::ShowLogs];

#[derive(Default)]
pub struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn prefix(&self) -> &'static str {
        "control"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: crate::Context,
    ) {
        let iter = ACTIONS
            .iter()
            .filter(|&action| input.matches(action.get_name()))
            .map(|action| {
                Entry::new(
                    action.get_name(),
                    action.get_description(),
                    CustomData::new(*action),
                )
            });
        builder.commit(iter).await;
    }

    fn init(&mut self, _: crate::Context) {}

    fn handle_pre(&self, thing: CustomData, _: &str, _: crate::Context) -> iced::Task<Message> {
        match thing.into::<Action>() {
            Action::Quit => Task::done(Message::Exit),
            Action::Hide => Task::none(),
            Action::ShowLogs => {
                utils::open_file(&**crate::logging::LOG_FILE);
                Task::none()
            }
        }
    }

    fn actions(&self) -> &'static [crate::Action] {
        const { &[crate::Action::default("Execute Action", "")] }
    }
}
