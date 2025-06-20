use iced::{Element, Size, window};

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
    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        match self {
            SpecialWindowState::ErrorPopup(state) => state.view(id),
            SpecialWindowState::WarnPopup(state) => state.view(id),
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn size(&self) -> Option<Size> {
        match self {
            SpecialWindowState::ErrorPopup(_) | SpecialWindowState::WarnPopup(_) => Some(Size {
                width: 400.0,
                height: 150.0,
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
