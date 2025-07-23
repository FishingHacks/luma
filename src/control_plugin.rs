use iced::Task;

use crate::{
    CustomData, Entry, Message, ResultBuilderRef, matcher::MatcherInput, plugin::StructPlugin,
    special_windows::SpecialWindowState, utils,
};

#[derive(Clone, Copy)]
pub enum Action {
    Quit,
    Hide,
    ShowLogs,
    OpenSettings,
}

impl Action {
    pub const fn get_name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Hide => "hide",
            Action::ShowLogs => "logs",
            Action::OpenSettings => "settings",
        }
    }
    pub const fn get_description(self) -> &'static str {
        match self {
            Action::Quit => {
                "Exit the runner (This will exit it entirely, not just hide the window)."
            }
            Action::Hide => "Hides the window",
            Action::ShowLogs => "Open the latest application logs",
            Action::OpenSettings => "Open the settings",
        }
    }
}

static ACTIONS: &[Action] = &[
    Action::Quit,
    Action::Hide,
    Action::ShowLogs,
    Action::OpenSettings,
];

#[derive(Default)]
pub struct ControlPlugin;

impl StructPlugin for ControlPlugin {
    fn prefix() -> &'static str {
        "control"
    }

    async fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        _: crate::PluginContext<'_>,
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

    async fn init(&mut self, _: crate::PluginContext<'_>) {}

    fn handle_pre(
        &self,
        thing: CustomData,
        _: &str,
        ctx: crate::PluginContext<'_>,
    ) -> iced::Task<Message> {
        match thing.into::<Action>() {
            Action::Quit => Task::done(Message::Exit),
            Action::Hide => Task::none(),
            Action::ShowLogs => {
                utils::open_file(&**crate::logging::LOG_FILE);
                Task::none()
            }
            Action::OpenSettings => Task::done(Message::OpenSpecial(SpecialWindowState::settings(
                Clone::clone(&*ctx.global_config),
            ))),
        }
    }

    fn actions(&self) -> &'static [crate::Action] {
        const { &[crate::Action::default("Execute Action", "")] }
    }
}
