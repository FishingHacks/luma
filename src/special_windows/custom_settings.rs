use std::collections::HashMap;

use iced::{
    Element, Length, Task,
    keyboard::Key,
    widget::{
        self, button, checkbox, column, combo_box, container, pick_list, row, scrollable, slider,
        space, text,
        text_editor::{Action, Content},
        text_input, toggler,
    },
    window,
};

use crate::{
    Message, State,
    config::{PluginSettings, PluginSettingsValue},
    format_key, key_element,
    plugin_settings::{PluginSettingsHolder, PluginWidget},
    special_windows::SpecialWindowMessage,
};

#[derive(Debug, Default)]
struct WidgetStates {
    text_editor: HashMap<Box<[StringOrUsize]>, Content>,
    combo_box: HashMap<Box<[StringOrUsize]>, widget::combo_box::State<Box<str>>>,
}

#[derive(Debug)]
pub struct CustomSettingsState {
    value: PluginSettingsValue,
    plugin: Box<str>,
    changed: bool,
    widget_states: WidgetStates,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StringOrUsize {
    Str(Box<str>),
    Usize(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum ListOp {
    Remove(usize),
    AddNew,
    MoveUp(usize),
    MoveDown(usize),
}

#[derive(Debug, Clone)]
pub enum CustomSettingsMessage {
    Change {
        path: Box<[StringOrUsize]>,
        value: PluginSettingsValue,
    },
    TextEditorAction {
        path: Box<[StringOrUsize]>,
        action: Action,
    },
    ListOp {
        path: Box<[StringOrUsize]>,
        op: ListOp,
    },
    Save,
}

impl CustomSettingsState {
    pub fn new(value: PluginSettingsValue, plugin: Box<str>, scheme: &PluginWidget) -> Self {
        let mut me = Self {
            value,
            plugin,
            changed: false,
            widget_states: WidgetStates::default(),
        };
        Self::compute_widget_states(&me.value, scheme, &mut Vec::new(), &mut me.widget_states);
        me
    }

    fn compute_widget_states(
        value: &PluginSettingsValue,
        scheme: &PluginWidget,
        path: &mut Vec<StringOrUsize>,
        widget_states: &mut WidgetStates,
    ) {
        match scheme {
            PluginWidget::Object { values } => {
                let map = if let PluginSettingsValue::Map(v) = value {
                    Some(v)
                } else {
                    None
                };
                for (k, scheme) in values {
                    let val = if let Some(v) = map.and_then(|v| v.get(k)) {
                        v
                    } else {
                        PluginSettingsValue::DEFAULT
                    };
                    path.push(StringOrUsize::Str(k.clone()));
                    Self::compute_widget_states(val, &scheme.widget, path, widget_states);
                    path.pop();
                }
            }
            PluginWidget::List { value_type, .. } => {
                let list = if let PluginSettingsValue::List(v) = value {
                    v.as_slice()
                } else {
                    &[]
                };
                for (i, v) in list.iter().enumerate() {
                    path.push(StringOrUsize::Usize(i));
                    Self::compute_widget_states(v, &value_type.widget, path, widget_states);
                    path.pop();
                }
            }

            PluginWidget::ParagraphInput { .. } => {
                let v = if let PluginSettingsValue::String(s) = value {
                    s
                } else {
                    ""
                };
                widget_states
                    .text_editor
                    .insert(path.as_slice().into(), Content::with_text(v));
            }

            PluginWidget::SearchableDropdown { values, .. } => {
                widget_states.combo_box.insert(
                    path.as_slice().into(),
                    widget::combo_box::State::new(values.clone()),
                );
            }

            _ => (),
        }
    }

    pub fn view<'a>(&'a self, id: window::Id, state: &'a State) -> Element<'a, Message> {
        let Some(settings) = state.plugin_configs.get(&*self.plugin) else {
            return text("Error: Could not locate settings for plugin").into();
        };

        let valid = PluginSettingsHolder::is_valid(&settings.widget, &self.value);

        let w = settings.view(&self.value, id, Box::new([]), &self.widget_states);

        let mut col = column([
            text(format!("Settings for {}", self.plugin))
                .size(25)
                .into(),
            space().height(5).into(),
        ]);

        if let Some(plugin) = state.get_plugin(&self.plugin) {
            let prefixes = row![text("Prefixes:")].spacing(5).extend(
                plugin
                    .any_prefixes()
                    .iter()
                    .map(|v| (&**v).into())
                    .map(key_element),
            );

            col =
                col.push(prefixes)
                    .push(text("Actions:"))
                    .extend(plugin.any_actions().iter().map(|action| {
                        let mut action_text = String::from("    ");
                        action_text.push_str(&action.name);
                        if action.shortcut.1 == Key::Unidentified {
                            text(action_text).into()
                        } else {
                            row![
                                text(action_text),
                                key_element(
                                    format_key(&action.shortcut.1, action.shortcut.0).into()
                                )
                            ]
                            .spacing(10)
                            .into()
                        }
                    }));
        }

        let save_msg = CustomSettingsMessage::Save;
        let save_msg = Message::SpecialWindow(SpecialWindowMessage::CustomSettings(save_msg), id);
        scrollable(
            container(
                col.push(space().height(20))
                    .push(w)
                    .push(space().height(20))
                    .push(button(text("Save")).on_press_maybe(valid.then_some(save_msg))),
            )
            .padding(10)
            .style(container::transparent),
        )
        .into()
    }

    pub fn update(
        &mut self,
        id: window::Id,
        state: &mut State,
        message: CustomSettingsMessage,
    ) -> Task<Message> {
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
            CustomSettingsMessage::TextEditorAction { path, action } => {
                let Some(contents) = self.widget_states.text_editor.get_mut(&path) else {
                    log::debug!("Got TextEditorAction for a field without a text editor :/");
                    return Task::none();
                };
                let needs_change = match action {
                    // Actions that don't actually effect the value of the editor
                    Action::Move(_)
                    | Action::Select(_)
                    | Action::SelectWord
                    | Action::SelectLine
                    | Action::SelectAll
                    | Action::Scroll { .. }
                    | Action::Drag(_)
                    | Action::Click(_) => false,

                    Action::Edit(_) => true,
                };
                contents.perform(action);

                if !needs_change {
                    return Task::none();
                }

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
                    *value_ref = PluginSettingsValue::String(contents.text());
                }
            }
            CustomSettingsMessage::ListOp { path, op } => {
                let holder = state
                    .plugin_configs
                    .get(&*self.plugin)
                    .expect("there should be a config schema");
                let mut scheme = &holder.widget;

                let mut value_ref = Some(&mut self.value);
                for entry in path {
                    match entry {
                        StringOrUsize::Str(s) => {
                            match value_ref {
                                Some(PluginSettingsValue::Map(map)) => value_ref = map.get_mut(&*s),
                                _ => value_ref = None,
                            }
                            match scheme {
                                PluginWidget::Object { values } if let Some(v) = values.get(&s) => {
                                    scheme = &v.widget;
                                }
                                _ => return Task::none(),
                            }
                        }
                        StringOrUsize::Usize(i) => {
                            match value_ref {
                                Some(PluginSettingsValue::List(list)) => {
                                    value_ref = list.get_mut(i);
                                }
                                _ => value_ref = None,
                            }
                            match scheme {
                                PluginWidget::List { value_type, .. } => {
                                    scheme = &value_type.widget;
                                }
                                _ => return Task::none(),
                            }
                        }
                    }
                }
                let Some(PluginSettingsValue::List(v)) = value_ref else {
                    return Task::none();
                };

                match op {
                    ListOp::Remove(idx) => {
                        if idx >= v.len() {
                            return Task::none();
                        }
                        v.remove(idx);
                    }
                    ListOp::MoveUp(idx) => {
                        if idx == 0 || idx >= v.len() {
                            return Task::none();
                        }
                        v.swap(idx, idx - 1);
                    }
                    ListOp::MoveDown(idx) => {
                        if idx >= v.len() - 1 {
                            return Task::none();
                        }
                        v.swap(idx, idx + 1);
                    }
                    ListOp::AddNew => {
                        let PluginWidget::List { value_type, .. } = scheme else {
                            return Task::none();
                        };
                        v.push(PluginSettingsHolder::default(&value_type.widget));
                    }
                }

                self.widget_states = WidgetStates::default();
                Self::compute_widget_states(
                    &self.value,
                    &holder.widget,
                    &mut Vec::new(),
                    &mut self.widget_states,
                );
            }
            CustomSettingsMessage::Save => {
                if self.changed {
                    state.context.config.plugin_settings.set(
                        &self.plugin,
                        std::mem::replace(&mut self.value, PluginSettingsValue::Null),
                    );
                    state.save_config();
                }
                return window::close(id);
            }
        }

        self.changed = true;
        Task::none()
    }
}

impl PluginSettings {
    fn view<'a>(
        &'a self,
        value: &PluginSettingsValue,
        id: window::Id,
        path: Box<[StringOrUsize]>,
        widget_states: &'a WidgetStates,
    ) -> Element<'a, Message> {
        let desc = self.description.trim();
        let title = match (&self.label, path.last()) {
            (Some(v), _) => text(&**v),
            (None, None) => text(""),
            (None, Some(StringOrUsize::Str(s))) => text(s.to_string()),
            (None, Some(StringOrUsize::Usize(i))) => text(format!("#{i}")),
        }
        .size(20);

        let left_widget = if desc.is_empty() {
            title.into()
        } else {
            column([title.into(), text(desc).size(12).into()]).into()
        };

        match &self.widget {
            PluginWidget::Object { values } => {
                let map = if let PluginSettingsValue::Map(v) = value {
                    Some(v)
                } else {
                    None
                };

                let col = column([left_widget, space().height(5).into()])
                    .extend(values.iter().map(|(k, v)| {
                        let value = map
                            .and_then(|v| v.get(k))
                            .unwrap_or(PluginSettingsValue::DEFAULT);

                        let p = path_with(&path, StringOrUsize::Str(k.clone()));
                        v.view(value, id, p, widget_states)
                    }))
                    .spacing(7);

                container(col)
                    .style(container::bordered_box)
                    .padding(12)
                    .into()
            }
            PluginWidget::List { value_type, .. } => {
                let v = if let PluginSettingsValue::List(v) = value {
                    v
                } else {
                    &Vec::new()
                };

                let add_msg = CustomSettingsMessage::ListOp {
                    path: path.clone(),
                    op: ListOp::AddNew,
                };
                let add_msg =
                    Message::SpecialWindow(SpecialWindowMessage::CustomSettings(add_msg), id);
                let left_widget = row![
                    left_widget,
                    button(text("+")).on_press(add_msg),
                    space().width(Length::Fill)
                ]
                .spacing(12)
                .into();

                let len = v.len();
                let col = column([left_widget, space().height(5).into()])
                    .extend(v.iter().enumerate().map(|(i, value)| {
                        let mut row = row![];

                        if i != 0 {
                            let up_msg = CustomSettingsMessage::ListOp {
                                path: path.clone(),
                                op: ListOp::MoveUp(i),
                            };
                            let up_msg = Message::SpecialWindow(
                                SpecialWindowMessage::CustomSettings(up_msg),
                                id,
                            );

                            row = row.push(button(text("▲")).on_press(up_msg));
                        }

                        if i < len - 1 {
                            let down_msg = CustomSettingsMessage::ListOp {
                                path: path.clone(),
                                op: ListOp::MoveDown(i),
                            };
                            let down_msg = Message::SpecialWindow(
                                SpecialWindowMessage::CustomSettings(down_msg),
                                id,
                            );

                            row = row.push(button(text("▼")).on_press(down_msg));
                        }

                        let remove_msg = CustomSettingsMessage::ListOp {
                            path: path.clone(),
                            op: ListOp::Remove(i),
                        };
                        let remove_msg = Message::SpecialWindow(
                            SpecialWindowMessage::CustomSettings(remove_msg),
                            id,
                        );

                        row.push(button(text("remove")).on_press(remove_msg))
                            .push(value_type.view(
                                value,
                                id,
                                path_with(&path, StringOrUsize::Usize(i)),
                                widget_states,
                            ))
                            .spacing(12)
                            .into()
                    }))
                    .spacing(7);

                container(col)
                    .style(container::bordered_box)
                    .padding(12)
                    .into()
            }

            _ => {
                let widget = self.widget.view(value, id, path, widget_states);

                row([left_widget, space().width(Length::Fill).into(), widget]).into()
            }
        }
    }
}

fn path_with(p: &[StringOrUsize], w: StringOrUsize) -> Box<[StringOrUsize]> {
    let mut v = Vec::with_capacity(p.len() + 1);
    v.extend_from_slice(p);
    v.push(w);
    v.into_boxed_slice()
}

impl PluginWidget {
    fn view<'a>(
        &'a self,
        value: &PluginSettingsValue,
        id: window::Id,
        path: Box<[StringOrUsize]>,
        widget_states: &'a WidgetStates,
    ) -> Element<'a, Message> {
        match self {
            Self::ParagraphInput { .. } => {
                let Some(content) = widget_states.text_editor.get(&path) else {
                    return space().into();
                };
                widget::text_editor(content)
                    .on_action(move |action| {
                        let msg = CustomSettingsMessage::TextEditorAction {
                            action,
                            path: path.clone(),
                        };
                        Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                    })
                    .into()
            }
            Self::StringInput { .. } => {
                let v = if let PluginSettingsValue::String(s) = value {
                    s.as_str()
                } else {
                    ""
                };

                text_input("", v)
                    .on_input(move |s| {
                        let msg = CustomSettingsMessage::Change {
                            path: path.clone(),
                            value: PluginSettingsValue::String(s),
                        };
                        Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                    })
                    .into()
            }
            Self::Toggle { .. } | Self::Checkbox { .. } => {
                let v = if let PluginSettingsValue::Boolean(v) = value {
                    *v
                } else {
                    false
                };
                let on_toggle = move |new_state| {
                    let path = path.clone();
                    let value = PluginSettingsValue::Boolean(new_state);
                    let msg = CustomSettingsMessage::Change { path, value };
                    Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                };
                if matches!(self, Self::Toggle { .. }) {
                    toggler(v).on_toggle(on_toggle).into()
                } else {
                    checkbox(v).on_toggle(on_toggle).into()
                }
            }
            Self::Dropdown { values, default } => {
                let v = if let PluginSettingsValue::String(s) = value {
                    s.as_str()
                } else {
                    &*values[*default]
                };
                pick_list(values.as_slice(), Some(Box::<str>::from(v)), move |v| {
                    let msg = CustomSettingsMessage::Change {
                        path: path.clone(),
                        value: PluginSettingsValue::String(v.into_string()),
                    };
                    Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                })
                .into()
            }
            Self::SearchableDropdown { values, default } => {
                let Some(state) = widget_states.combo_box.get(&path) else {
                    return space().into();
                };
                let v = if let PluginSettingsValue::String(s) = value {
                    s.as_str()
                } else {
                    &*values[*default]
                };

                combo_box(
                    state,
                    "",
                    state.options().iter().find(|opt| ***opt == *v),
                    move |v| {
                        let msg = CustomSettingsMessage::Change {
                            path: path.clone(),
                            value: PluginSettingsValue::String(v.into_string()),
                        };
                        Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                    },
                )
                .into()
            }
            &Self::IntSlider {
                min,
                max,
                step,
                default,
            } => {
                let v = if let &PluginSettingsValue::Int(v) = value {
                    v
                } else {
                    default
                };

                let slider = slider(min as f64..=max as f64, v as f64, move |v| {
                    let msg = CustomSettingsMessage::Change {
                        path: path.clone(),
                        value: PluginSettingsValue::Int(v as i64),
                    };
                    Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                })
                .step(step as f64);
                row![text(format!("{v}")), slider].spacing(4).into()
            }
            &Self::Slider {
                min,
                max,
                step,
                default,
            } => {
                let v = if let &PluginSettingsValue::Number(v) = value {
                    v
                } else {
                    default
                };

                let mut s = slider(min..=max, v, move |v| {
                    let msg = CustomSettingsMessage::Change {
                        path: path.clone(),
                        value: PluginSettingsValue::Number(v),
                    };
                    Message::SpecialWindow(SpecialWindowMessage::CustomSettings(msg), id)
                });
                if let Some(step) = step {
                    s = s.step(step);
                }
                row![text(format!("{v}")), s].spacing(4).into()
            }

            &Self::IntInput { default, .. } => {
                let v = if let &PluginSettingsValue::Int(v) = value {
                    v
                } else {
                    default
                };

                text_input("", &format!("{v}"))
                    .on_input(move |v| {
                        let v = v.parse::<i64>();
                        match v {
                            Ok(v) => {
                                let msg = CustomSettingsMessage::Change {
                                    path: path.clone(),
                                    value: PluginSettingsValue::Int(v),
                                };
                                Message::SpecialWindow(
                                    SpecialWindowMessage::CustomSettings(msg),
                                    id,
                                )
                            }
                            Err(_) => Message::None,
                        }
                    })
                    .into()
            }
            &Self::NumInput { default, .. } => {
                let v = if let &PluginSettingsValue::Number(v) = value {
                    v
                } else {
                    default
                };

                text_input("", &format!("{v}"))
                    .on_input(move |v| {
                        let v = v.parse::<f64>();
                        match v {
                            Ok(v) => {
                                let msg = CustomSettingsMessage::Change {
                                    path: path.clone(),
                                    value: PluginSettingsValue::Number(v),
                                };
                                Message::SpecialWindow(
                                    SpecialWindowMessage::CustomSettings(msg),
                                    id,
                                )
                            }
                            Err(_) => Message::None,
                        }
                    })
                    .into()
            }

            Self::Object { .. } | Self::List { .. } => unreachable!(),
        }
    }
}
