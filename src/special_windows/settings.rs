use iced::{
    Element, Length, Task,
    alignment::Vertical,
    widget::{button, checkbox, column, horizontal_space, row, text, vertical_space},
    window,
};

use crate::{
    Message, State,
    config::{BlurAction, Config},
    plugin::StringLike,
};

#[derive(Debug)]
pub struct SettingsState {
    pub(super) config: Config,
}

impl From<(SettingsMessage, window::Id)> for Message {
    fn from(value: (SettingsMessage, window::Id)) -> Self {
        Message::SpecialWindow(super::SpecialWindowMessage::Settings(value.0), value.1)
    }
}

#[derive(Clone, Debug)]
pub enum SettingsMessage {
    SetAutoResize(bool),
    SetForceFocus(bool),
    SetPluginEnabled(StringLike, bool),
    Save,
    Discard,
}

impl SettingsState {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn view<'a>(&self, id: window::Id, state: &'a State) -> Element<'a, Message> {
        let mut col = column![
            text("Luma Settings").size(25).width(Length::Fill).center(),
            vertical_space()
                .width(Length::Fill)
                .height(Length::Fixed(10.0))
        ]
        .padding(10.0);
        col = col.push(
            checkbox("Auto Resize", self.config.auto_resize)
                .on_toggle(move |v| (SettingsMessage::SetAutoResize(v), id).into()),
        );
        col = col.push(
            checkbox(
                "Force focus when the launcher is opened",
                matches!(self.config.on_blur, BlurAction::Refocus),
            )
            .on_toggle(move |v| (SettingsMessage::SetForceFocus(v), id).into()),
        );
        col = col.push(text("Plugins").size(18).width(Length::Fill).center());
        for plugin in state
            .plugin_builder
            .iter()
            .map(|v| &v.0)
            .filter(|v| **v != "control")
        {
            let mut row = row![
                checkbox(
                    plugin.clone(),
                    self.config.enabled_plugins.contains(plugin.to_str()),
                )
                .on_toggle(move |v| {
                    (SettingsMessage::SetPluginEnabled(plugin.clone(), v), id).into()
                }),
            ];
            if state.plugin_configs.contains_key(plugin) {
                row = row
                    .push(horizontal_space().width(Length::Fixed(20.0)))
                    .push(button("Edit Plugin Config"))
                    .align_y(Vertical::Center);
            }
            col = col.push(row);
        }
        col = col
            .push(vertical_space().width(Length::Fill).height(Length::Fill))
            .push(row![
                button("Save").on_press((SettingsMessage::Save, id).into()),
                button("Discard").on_press((SettingsMessage::Discard, id).into())
            ]);
        col.into()
    }

    pub fn update(
        &mut self,
        id: window::Id,
        _: &mut State,
        message: SettingsMessage,
    ) -> Task<Message> {
        match message {
            SettingsMessage::Discard => return window::close(id),
            SettingsMessage::Save => {
                return Task::batch([
                    window::close(id),
                    // it is fine to take here because we close the window, meaning we will no
                    // longer use the state, as such an incorrect config should never be drawn.
                    Task::done(Message::UpdateConfig(
                        std::mem::take(&mut self.config).into(),
                        true,
                    )),
                ]);
            }
            SettingsMessage::SetAutoResize(v) => self.config.auto_resize = v,
            SettingsMessage::SetForceFocus(true) => self.config.on_blur = BlurAction::Refocus,
            SettingsMessage::SetForceFocus(false) => self.config.on_blur = BlurAction::None,
            SettingsMessage::SetPluginEnabled(plugin, true) => {
                if !self.config.enabled_plugins.contains(&*plugin) {
                    self.config.enabled_plugins.insert(plugin.into());
                }
            }
            SettingsMessage::SetPluginEnabled(plugin, false) => {
                self.config.enabled_plugins.retain(|v| v != &*plugin);
            }
        }
        Task::none()
    }
}
