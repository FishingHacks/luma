use iced::{Element, Size, Task, window};
use settings::SettingsMessage;

use crate::{Message, State};

pub mod error_popup;
pub mod settings;
pub mod warning_popup;

#[derive(Debug)]
pub enum SpecialWindowState {
    ErrorPopup(error_popup::State),
    WarnPopup(warning_popup::State),
    Settings(settings::SettingsState),
}

#[derive(Clone, Debug)]
pub enum SpecialWindowMessage {
    Settings(SettingsMessage),
}

impl Clone for SpecialWindowState {
    fn clone(&self) -> Self {
        unreachable!()
    }
}

impl SpecialWindowState {
    pub fn view(&self, id: window::Id, parent_state: &State) -> Element<'_, Message> {
        match self {
            SpecialWindowState::ErrorPopup(state) => state.view(id),
            SpecialWindowState::WarnPopup(state) => state.view(id),
            SpecialWindowState::Settings(state) => state.view(id, parent_state),
        }
    }

    pub fn update(
        &mut self,
        id: window::Id,
        parent_state: &mut State,
        message: SpecialWindowMessage,
    ) -> Task<Message> {
        match (self, message) {
            (SpecialWindowState::Settings(state), SpecialWindowMessage::Settings(message)) => {
                state.update(id, parent_state, message)
            }
            _ => Task::none(),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn size(&self) -> Option<Size> {
        match self {
            SpecialWindowState::ErrorPopup(_) | SpecialWindowState::WarnPopup(_) => Some(Size {
                width: 400.0,
                height: 150.0,
            }),
            SpecialWindowState::Settings(_) => None,
        }
    }

    pub fn new_error_popup(message: String) -> Self {
        Self::ErrorPopup(error_popup::State { message })
    }
    pub fn new_warning_popup(message: String) -> Self {
        Self::WarnPopup(warning_popup::State { message })
    }

    pub(crate) fn settings() -> Self {
        Self::Settings(settings::SettingsState)
    }
}
