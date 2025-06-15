use iced::{Element, Size, Task, window};

use crate::Message;

pub mod error_popup;
pub mod warning_popup;

#[derive(Debug)]
pub enum SpecialWindowState {
    ErrorPopup(error_popup::State),
    WarnPopup(warning_popup::State),
}

impl Clone for SpecialWindowState {
    fn clone(&self) -> Self {
        unreachable!()
    }
}

impl SpecialWindowState {
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match self {
            SpecialWindowState::ErrorPopup(s) => s.update(message),
            SpecialWindowState::WarnPopup(s) => s.update(message),
        }
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        match self {
            SpecialWindowState::ErrorPopup(state) => state.view(id),
            SpecialWindowState::WarnPopup(state) => state.view(id),
        }
    }

    pub fn size(&self) -> Option<Size> {
        match self {
            SpecialWindowState::ErrorPopup(_) | SpecialWindowState::WarnPopup(_) => Some(Size {
                width: 250.0,
                height: 125.0,
            }),
        }
    }

    pub fn new_error_popup(message: String) -> Self {
        Self::ErrorPopup(error_popup::State { message })
    }
    pub fn new_warning_popup(message: String) -> Self {
        Self::WarnPopup(warning_popup::State { message })
    }
}
