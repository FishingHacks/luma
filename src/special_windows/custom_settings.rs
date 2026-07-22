use std::rc::Rc;

use iced::{Element, widget::text, window};

use crate::{
    State,
    config::{PluginSettings, PluginSettingsValue},
};

#[derive(Debug)]
pub struct CustomSettingsState {
    value: PluginSettingsValue,
    plugin: Box<str>,
    changed: bool,
}

pub enum StringOrUsize {
    Str(Rc<str>),
    Usize(usize),
}

pub enum CustomSettingsMessage {
    Change {
        path: Vec<StringOrUsize>,
        value: PluginSettingsValue,
    },
}

impl CustomSettingsState {
    pub fn new(value: &PluginSettingsValue, plugin: &str) -> Self {
        Self {
            value: value.clone(),
            plugin: plugin.into(),
            changed: false,
        }
    }

    pub fn view<'a>(&self, id: window::Id, state: &'a State) -> Element<'a, ()> {
        let Some(settings) = state.plugin_configs.get(&*self.plugin) else {
            return text("Error: Could not locate settings for plugin").into();
        };
        todo!()
    }

    pub fn update(&mut self, id: window::Id, state: &mut State, message: CustomSettingsMessage) {
        match message {
            CustomSettingsMessage::Change { path, value } => {
                let mut value_ref = Some(&mut self.value);
                for entry in path {
                    match entry {
                        StringOrUsize::Str(s) => match value_ref {
                            Some(PluginSettingsValue::Map(map)) => value_ref = map.get_mut(&*s),
                            _ => value_ref = None,
                        },
                        StringOrUsize::Usize(i) => match value_ref {
                            Some(PluginSettingsValue::List(list)) => value_ref = list.get_mut(i),
                            _ => value_ref = None,
                        },
                    }
                }
                if let Some(value_ref) = value_ref {
                    *value_ref = value;
                }
            }
        }
    }
}
