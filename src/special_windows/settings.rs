use iced::{
    Element, Length, Task,
    widget::{checkbox, column, text, vertical_space},
    window,
};

use crate::{Message, State};

#[derive(Debug)]
pub struct SettingsState;

impl From<(SettingsMessage, window::Id)> for Message {
    fn from(value: (SettingsMessage, window::Id)) -> Self {
        Message::SpecialWindow(super::SpecialWindowMessage::Settings(value.0), value.1)
    }
}

#[derive(Copy, Clone, Debug)]
pub enum SettingsMessage {
    SetAutoResize(bool),
}

impl SettingsState {
    pub fn view(&self, id: window::Id, parent_state: &State) -> Element<'_, Message> {
        _ = id;
        _ = self;
        let mut col = column![
            text("Luma Settings").size(25).width(Length::Fill).center(),
            vertical_space()
                .width(Length::Fill)
                .height(Length::Fixed(10.0))
        ]
        .padding(10.0);
        col = col.push(
            checkbox("Auto Resize", parent_state.config.auto_resize)
                .on_toggle(move |v| (SettingsMessage::SetAutoResize(v), id).into()),
        );
        col.into()
    }

    pub fn update(
        &mut self,
        id: window::Id,
        parent_state: &mut State,
        message: SettingsMessage,
    ) -> Task<Message> {
        Task::none()
    }
}
