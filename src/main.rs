#![warn(clippy::pedantic)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unreadable_literal)]
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsStr,
    fmt::Debug,
    path::Path,
    sync::{Arc, LazyLock},
    time::Duration,
};

use config::{BlurAction, Config, FileWatcherEntry, Files, ScanFilter};
use control_plugin::ControlPlugin;
use dice_plugin::DicePlugin;
use fend_plugin::FendPlugin;
use file_index::{FileIndexMessage, FileIndexResponse};
use file_plugin::FilePlugin;
use filter_service::{CollectorController, CollectorMessage, ResultBuilderRef};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers as HKModifiers},
};
use iced::{
    Border, Color, Element, Length, Point, Size, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    border::Radius,
    color,
    futures::{SinkExt, channel::mpsc::Sender},
    keyboard::{Key, Modifiers, key::Named},
    mouse::ScrollDelta,
    stream::channel,
    widget::{MouseArea, button, column, container, mouse_area, row, stack, text, text_input},
    window::{self, Level, Position, Settings},
};
use mlua::Lua;
use run_plugin::RunPlugin;
use search_input::SearchInput;
use special_windows::SpecialWindowState;
use theme_plugin::ThemePlugin;

mod cache;
mod config;
mod control_plugin;
mod dice_plugin;
mod fend_plugin;
mod file_index;
mod file_plugin;
mod filter_service;
mod keybind;
mod logging;
mod lua;
mod matcher;
mod plugin;
mod run_plugin;
mod search_input;
mod special_windows;
mod sqlite;
mod theme_plugin;
mod utils;
pub use filter_service::ResultBuilder;
use plugin::{AnyPlugin, GenericEntry};
pub use plugin::{CustomData, Entry, Plugin};
use tokio::sync::mpsc::UnboundedSender;

pub static CONFIG: LazyLock<Arc<Config>> = LazyLock::new(|| {
    Config {
        files: Files {
            entries: vec![FileWatcherEntry {
                path: Path::new("/home/fishi").into(),
                watch: true,
                reindex_every: None,
                filter: ScanFilter::default(),
            }],
            reindex_at_startup: false,
        },
        on_blur: BlurAction::Refocus,
        keybind: "Alt+P".into(),
        enabled_plugins: vec![],
    }
    .into()
});

#[derive(Debug, Clone)]
pub enum Message {
    UpdateSearch(String),
    SetSearch(String),
    GoUp,
    GoDown,
    Go10Up,
    Go10Down,
    Submit,
    Click(usize),
    HideMainWindow,
    Hide(window::Id),
    Show,
    ChangeTheme(Theme),
    HandleAction {
        plugin: usize,
        data: CustomData,
        action: String,
    },
    None,
    InputPress,
    Exit,
    CollectorMessage(CollectorMessage),
    ResultsUpdated,
    KeyPressed(Key, Modifiers),
    ShowActions,
    HideActions,
    Blurred,
    OpenSpecial(SpecialWindowState),
    IndexerMessage(FileIndexResponse),
}

pub struct State {
    search_query: String,
    results: Vec<GenericEntry>,
    selected: usize,
    offset: usize,
    num_entries: usize,
    text_input: text_input::Id,
    window: Option<window::Id>,
    plugins: Arc<Vec<Box<dyn AnyPlugin>>>,
    plugin_builder: Vec<Box<dyn FnMut() -> Box<dyn AnyPlugin>>>,
    theme: Theme,
    index_sender: Option<UnboundedSender<FileIndexMessage>>,
    collector_controller: Option<CollectorController>,
    showing_actions: bool,
    selected_action: usize,
    on_blur: BlurAction,
    special_windows: BTreeMap<window::Id, SpecialWindowState>,
    lua: Lua,
}

const ALLOWED_ACTION_MODIFIERS: Modifiers = Modifiers::COMMAND
    .union(Modifiers::ALT)
    .union(Modifiers::CTRL)
    .union(Modifiers::LOGO);

pub struct Action {
    name: Cow<'static, str>,
    shortcut: (Modifiers, Key),
    id: Cow<'static, str>,
    closes: bool,
}

impl Action {
    #[must_use]
    pub const fn new(name: &'static str, id: &'static str, shortcut: (Modifiers, Key)) -> Self {
        Self {
            name: Cow::Borrowed(name),
            shortcut,
            id: Cow::Borrowed(id),
            closes: true,
        }
    }

    #[must_use]
    pub const fn without_shortcut(name: &'static str, id: &'static str) -> Self {
        Self::new(name, id, (Modifiers::empty(), Key::Unidentified))
    }

    /// Constructs the suggest action (tab)
    #[must_use]
    pub const fn suggest(name: &'static str, id: &'static str) -> Self {
        Self::new(name, id, (Modifiers::empty(), Key::Named(Named::Tab))).keep_open()
    }

    /// Constructs the default action. This should always be the first entry.
    #[must_use]
    pub const fn default(name: &'static str, id: &'static str) -> Self {
        Self::new(name, id, (Modifiers::empty(), Key::Named(Named::Enter)))
    }

    #[must_use]
    pub const fn keep_open(mut self) -> Self {
        self.closes = false;
        self
    }

    #[must_use]
    pub const fn new_owned(name: String, id: String, shortcut: (Modifiers, Key)) -> Self {
        Self {
            name: Cow::Owned(name),
            shortcut,
            id: Cow::Owned(id),
            closes: true,
        }
    }

    #[must_use]
    pub const fn without_shortcut_owned(name: String, id: String) -> Self {
        Self::new_owned(name, id, (Modifiers::empty(), Key::Unidentified))
    }

    /// Constructs the suggest action (tab)
    #[must_use]
    pub const fn suggest_owned(name: String, id: String) -> Self {
        Self::new_owned(name, id, (Modifiers::empty(), Key::Named(Named::Tab)))
    }

    /// Constructs the default action. This should always be the first entry.
    #[must_use]
    pub const fn default_owned(name: String, id: String) -> Self {
        Self::new_owned(name, id, (Modifiers::empty(), Key::Named(Named::Enter)))
    }
}

pub fn format_key(key: &Key, modifiers: Modifiers, s: &mut String) {
    use std::fmt::Write;

    if matches!(key, Key::Unidentified) {
        return;
    }
    if Modifiers::CTRL.intersects(modifiers) {
        s.push_str("Ctrl + ");
    }
    if Modifiers::ALT.intersects(modifiers) {
        #[cfg(target_os = "macos")]
        s.push_str("Alt + ");
        #[cfg(not(target_os = "macos"))]
        s.push_str("Alt + ");
    }
    if Modifiers::LOGO.intersects(modifiers) {
        #[cfg(target_os = "windows")]
        s.push_str("Win + ");
        #[cfg(target_os = "macos")]
        s.push_str("Cmd + ");
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        s.push_str("Super + ");
    }
    match key {
        Key::Named(Named::Super) => {
            #[cfg(target_os = "windows")]
            s.push_str("Win");
            #[cfg(target_os = "macos")]
            s.push_str(" Cmd");
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            s.push_str("Super");
        }
        Key::Named(Named::Enter) => s.push_str("↵  Enter"),
        Key::Named(Named::Backspace) => s.push_str("← Backspace"),
        Key::Named(named) => write!(s, "{named:?}").expect("write-to-str-fail"),
        Key::Character(c) => s.push_str(c.as_str()),
        Key::Unidentified => s.push_str("unknown key"),
    }
}

#[must_use]
pub fn key_element(s: Cow<'_, str>) -> Element<'_, Message> {
    container(text(s).size(16))
        .style(|theme| {
            container::dark(theme)
                .background(color!(0x44403b))
                .border(Border {
                    color: color!(0x292524),
                    width: 1.5,
                    radius: 7.0.into(),
                })
        })
        .padding([0, 10])
        .into()
}

fn button_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = if selected {
            button::primary(theme, status)
        } else {
            button::text(theme, status)
        };
        style.border = Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(0.0),
        };
        style
    }
}

impl State {
    pub fn view(&self) -> MouseArea<'_, Message> {
        let search_field = SearchInput::new(&self.search_query, self.text_input.clone());
        let mut col = column![stack([
            search_field.into(),
            text(format!("{} / {}  ", self.selected + 1, self.results.len()))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Right)
                .align_y(Vertical::Center)
                .color(Color::from_rgb8(0x60, 0x60, 0x60))
                .size(13)
                .into()
        ])];

        for entry_idx in 0..self.num_entries {
            let index = entry_idx + self.offset;
            if index >= self.results.len() {
                break;
            }
            let selected = index == self.selected;
            let entry = &self.results[entry_idx + self.offset];
            let subtitle: Element<'_, Message> = if entry.subtitle.is_empty() {
                text(
                    self.plugins
                        .get(entry.plugin)
                        .map(|v| v.any_prefix())
                        .unwrap_or_default(),
                )
                .size(16)
                .into()
            } else {
                row![
                    text(
                        self.plugins
                            .get(entry.plugin)
                            .map(|v| v.any_prefix())
                            .unwrap_or_default()
                    )
                    .size(16)
                    .style(text::default),
                    text(" • ").size(16),
                    text(&*entry.subtitle)
                        .size(16)
                        .wrapping(text::Wrapping::None),
                ]
                .height(20)
                .width(Length::Fill)
                .into()
            };
            let inner_col = column![
                text(&*entry.name)
                    .size(20)
                    .height(25)
                    .wrapping(text::Wrapping::None),
                subtitle
            ];
            col = col.push(
                button(inner_col)
                    .width(Length::Fill)
                    .height(Length::Fixed(ENTRY_SIZE))
                    .style(button_style(selected))
                    .on_press(Message::Click(entry_idx + self.offset)),
            );
        }
        if self.showing_actions {
            for (i, action) in self.get_actions().iter().enumerate() {
                let description = if matches!(action.shortcut.1, Key::Unidentified) {
                    row![text(&action.name).size(16).style(text::default)].spacing(10)
                } else {
                    let mut s = String::new();
                    format_key(&action.shortcut.1, action.shortcut.0, &mut s);
                    row![
                        text(&action.name).size(16).style(text::default),
                        key_element(s.into())
                    ]
                    .spacing(10)
                };
                col = col.push(
                    button(
                        container(description)
                            .width(Length::Fill)
                            .align_x(Horizontal::Center),
                    )
                    .width(Length::Fill)
                    .style(button_style(self.selected_action == i))
                    .height(ACTION_SIZE)
                    .on_press(Message::None),
                );
            }
        }

        let (action_text, action_key, action_seperator) = match self
            .results
            .get(self.selected)
            .and_then(|v| self.plugins.get(v.plugin))
            .and_then(|v| v.any_actions().first())
        {
            None => (None, None, None),
            Some(action) => {
                let mut s = String::new();
                format_key(&action.shortcut.1, action.shortcut.0, &mut s);
                (
                    Some(text(&action.name).size(16)),
                    Some(key_element(s.into())),
                    Some(text("•").size(16)),
                )
            }
        };
        col = col.push(
            container(
                row::Row::new()
                    .push_maybe(action_text)
                    .push_maybe(action_key)
                    .push_maybe(action_seperator)
                    .push(text("Actions").size(16))
                    .push(key_element("Alt".into()))
                    .push(text("•").size(16))
                    .push(
                        text(utils::CRATE_NAME.to_string() + " v" + utils::CRATE_VERSION).size(16),
                    )
                    .spacing(10)
                    .width(Length::Fill)
                    .height(ACTION_BAR_SIZE)
                    .align_y(Vertical::Center),
            )
            .height(ACTION_BAR_SIZE + 1.0)
            .padding([0, 7])
            .style(|_| container::background(color!(0x79716b)).color(Color::WHITE)),
        );

        mouse_area(col).on_scroll(|delta| {
            let delta = match delta {
                ScrollDelta::Lines { y, .. } | ScrollDelta::Pixels { y, .. } => y,
            };
            if delta > 0.0 {
                Message::GoUp
            } else {
                Message::GoDown
            }
        })
    }
    fn get_actions(&self) -> &[Action] {
        if self.showing_actions {
            self.results
                .get(self.selected)
                .and_then(|res| self.plugins.get(res.plugin))
                .map(|v| v.any_actions())
                .unwrap_or_default()
        } else {
            &[]
        }
    }

    fn update_matches(&mut self) {
        if self.search_query.is_empty() {
            self.results.clear();
            return;
        }
        if let Some(controller) = &mut self.collector_controller {
            controller.start(
                self.plugins.clone(),
                self.search_query.trim().to_lowercase(),
            );
        } else {
            log::error!("Failed to query: no collector controller present");
        }
    }

    fn run(&mut self, index: usize, selected_action: usize) -> iced::Task<Message> {
        if self.results.len() <= self.selected {
            return Task::none();
        }
        let entry = &self.results[index];
        if entry.plugin >= self.plugins.len() {
            return Task::none();
        }
        let plugin = &self.plugins[entry.plugin];
        let Some(action) = plugin.any_actions().get(selected_action) else {
            return Task::none();
        };
        if action.closes {
            let entry = self.results.remove(index);
            Task::batch([
                plugin.any_handle_pre(entry.data.clone(), &action.id),
                Task::done(Message::HideMainWindow),
                Task::done(Message::HandleAction {
                    plugin: entry.plugin,
                    data: entry.data,
                    action: action.id.to_string(),
                }),
            ])
        } else {
            Task::batch([
                plugin.any_handle_pre(entry.data.clone(), &action.id),
                plugin.any_handle_post(entry.data.clone(), &action.id),
            ])
        }
    }

    fn handle_go_up(&mut self, amount: usize) {
        if self.showing_actions {
            self.selected_action = self.selected_action.saturating_sub(amount);
        } else {
            self.selected = self.selected.saturating_sub(amount);
        }
    }

    fn handle_go_down(&mut self, amount: usize) {
        let actions = self.get_actions();
        if self.showing_actions && !actions.is_empty() {
            self.selected_action = (self.selected_action + amount).min(actions.len() - 1);
        } else if !self.results.is_empty() {
            self.selected = (self.selected + amount).min(self.results.len() - 1);
        }
    }

    fn hide_actions(&mut self) {
        self.showing_actions = false;
        self.selected_action = 0;
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let Some(window_id) = self.window else {
            unreachable!("the window update should always have a window")
        };
        match message {
            Message::SetSearch(q) => {
                self.search_query = q;
                self.update_matches();
                self.selected = 0;
                self.hide_actions();
                let task = text_input::move_cursor_to_end(self.text_input.clone());
                if self.search_query.is_empty() {
                    return Task::batch([
                        task,
                        window::get_size(window_id).then(move |size| {
                            window::resize(window_id, Size::new(size.width, BASE_SIZE))
                        }),
                    ]);
                }
                return task;
            }
            Message::UpdateSearch(q) => {
                self.search_query = q;
                self.update_matches();
                self.selected = 0;
                self.hide_actions();
                if self.search_query.is_empty() {
                    return window::get_size(window_id).then(move |size| {
                        window::resize(window_id, Size::new(size.width, BASE_SIZE))
                    });
                }
            }
            Message::KeyPressed(key, modifiers) => {
                if let Some(action) = self
                    .results
                    .get(self.selected)
                    .and_then(|v| self.plugins.get(v.plugin))
                    .and_then(|plugin| {
                        plugin
                            .any_actions()
                            .iter()
                            .position(|v| v.shortcut.0 == modifiers && v.shortcut.1 == key)
                    })
                {
                    return self.run(self.selected, action);
                }
            }
            Message::ResultsUpdated => self.update_matches(),
            Message::GoUp => self.handle_go_up(1),
            Message::Go10Up => self.handle_go_up(10),
            Message::GoDown => self.handle_go_down(1),
            Message::Go10Down => self.handle_go_down(10),
            Message::Submit => {
                return self.run(
                    self.selected,
                    if self.showing_actions {
                        self.selected_action
                    } else {
                        0
                    },
                );
            }
            Message::Click(index) => {
                self.selected = index;
                if self.selected >= self.results.len() && !self.results.is_empty() {
                    self.selected = self.results.len() - 1;
                }
                if self.selected < self.offset {
                    self.offset = self.selected;
                }
                if self.selected >= self.offset + self.num_entries {
                    self.offset = self.selected + 1 - self.num_entries;
                }
                return self.run(index, 0);
            }
            Message::HideMainWindow => {
                self.search_query.clear();
                self.results.clear();
                self.hide_actions();
                if let Some(v) = self.collector_controller.as_mut() {
                    v.stop();
                }
                if let Some(window) = self.window.take() {
                    return iced::window::close(window);
                }
            }
            Message::ChangeTheme(theme) => self.theme = theme,
            Message::InputPress => {
                let Some(window) = self.window else {
                    return text_input::focus(self.text_input.clone());
                };
                return Task::batch([
                    text_input::focus(self.text_input.clone()),
                    window::drag(window),
                ]);
            }
            Message::CollectorMessage(CollectorMessage::Finished(results)) => {
                self.hide_actions();
                self.results = results;
                let new_height =
                    self.results.len().min(self.num_entries) as f32 * ENTRY_SIZE + BASE_SIZE;
                return window::get_size(window_id).then(move |size| {
                    window::resize(window_id, Size::new(size.width, new_height))
                });
            }
            Message::ShowActions => {
                if self.results.is_empty() {
                    return Task::none();
                }
                let Some(plugin) = self.plugins.get(self.results[self.selected].plugin) else {
                    return Task::none();
                };
                let actions = plugin.any_actions();
                if !self.results.is_empty() {
                    self.showing_actions = true;
                    self.selected_action = 0;
                    let new_height = self.results.len().min(self.num_entries) as f32 * ENTRY_SIZE
                        + BASE_SIZE
                        + actions.len() as f32 * ACTION_SIZE;
                    return window::get_size(window_id).then(move |size| {
                        window::resize(window_id, Size::new(size.width, new_height))
                    });
                }
            }
            Message::HideActions => {
                self.hide_actions();
                let new_height =
                    self.results.len().min(self.num_entries) as f32 * ENTRY_SIZE + BASE_SIZE;
                return window::get_size(window_id).then(move |size| {
                    window::resize(window_id, Size::new(size.width, new_height))
                });
            }
            Message::Blurred => match self.on_blur {
                BlurAction::Refocus => return window::gain_focus(window_id),
                BlurAction::Hide => return Task::done(Message::HideMainWindow),
                BlurAction::None => {}
            },

            // daemon messages
            Message::Show
            | Message::OpenSpecial(_)
            | Message::Hide(_)
            | Message::HandleAction { .. }
            | Message::None
            | Message::Exit
            | Message::IndexerMessage(_)
            | Message::CollectorMessage(CollectorMessage::Ready(_)) => unreachable!(),
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + self.num_entries {
            self.offset = self.selected + 1 - self.num_entries;
        }
        Task::none()
    }

    #[must_use]
    pub fn get_plugin(&self, s: &str) -> Option<&dyn AnyPlugin> {
        self.plugins
            .iter()
            .find(|v| v.any_prefix() == s)
            .map(|v| &**v)
    }

    pub fn add_plugin_instance<T: Plugin + Clone + 'static>(&mut self, value: T) {
        self.plugin_builder
            .push(Box::new(move || Box::new(value.clone())));
    }
    pub fn add_plugin<T: Plugin + Default + 'static>(&mut self) {
        self.plugin_builder
            .push(Box::new(|| Box::new(T::default())));
    }
    pub fn add_lua_plugins(&mut self) {
        log::debug!("Loading lua plugins...");
        let Ok(dirent) = std::fs::read_dir(&*lua::LUA_PLUGIN_DIR) else {
            return;
        };
        for ent in dirent.filter_map(Result::ok) {
            let path = ent.path();
            let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "lua" {
                continue;
            }
            let stem = Arc::<str>::from(stem);
            match lua::load_lua_plugin(&self.lua, path, stem.clone()) {
                Ok(v) => self.add_plugin_instance(v),
                Err(e) => {
                    log::error!("Failed to load plugin {stem:?}: {e}");
                }
            }
        }
    }

    pub fn init_plugins(&mut self) {
        if let Some(controller) = &mut self.collector_controller {
            controller.stop();
        }
        self.results.clear();
        self.plugins = Arc::new(
            self.plugin_builder
                .iter_mut()
                .map(|v| {
                    let mut plugin = v();
                    plugin.any_init();
                    plugin
                })
                .collect(),
        );
    }
}

pub fn change_theme(new_theme: Theme) -> Task<Message> {
    Task::done(Message::ChangeTheme(new_theme))
}

const SEARCH_SIZE: f32 = 31.0;
const ENTRY_SIZE: f32 = 56.0;
const ACTION_SIZE: f32 = 31.0;
const ACTION_BAR_SIZE: f32 = 31.0;
const BASE_SIZE: f32 = SEARCH_SIZE + ACTION_BAR_SIZE;

fn daemon_view(state: &State, id: window::Id) -> Element<'_, Message> {
    if let Some(main_window_id) = state.window {
        if id == main_window_id {
            return state.view().into();
        }
    }
    if let Some(state) = state.special_windows.get(&id) {
        return state.view(id);
    }
    text(format!("No state was found for this window. {id:?}")).into()
}

fn daemon_update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Show => {
            let mut settings = Settings {
                resizable: false,
                decorations: false,
                level: Level::AlwaysOnTop,
                position: Position::SpecificWith(|winsize, resolution| {
                    Point::new(
                        (resolution.width - winsize.width).max(0.0) / 2.0,
                        (resolution.height - BASE_SIZE - 12.0 * ENTRY_SIZE).max(0.0) / 2.0,
                    )
                }),
                ..Default::default()
            };
            settings.size.height = BASE_SIZE;
            let (id, open_window_task) = window::open(settings);
            let open_window_task = open_window_task.map(|_| Message::None);
            log::trace!("opened main window with id {id:?}");
            let old_window = state.window.replace(id);
            state.init_plugins();
            let focus_task = text_input::focus(state.text_input.clone()).map(|()| Message::None);
            match old_window {
                Some(id) => Task::batch([window::close(id), open_window_task, focus_task]),
                None => Task::batch([open_window_task, focus_task]),
            }
        }
        Message::Hide(window_id) => {
            state.special_windows.remove(&window_id);
            window::close(window_id)
        }
        Message::HandleAction {
            plugin,
            data,
            action,
        } => state
            .plugins
            .get(plugin)
            .map_or_else(Task::none, |plugin| plugin.any_handle_post(data, &action)),
        Message::Exit => iced::exit(),
        Message::None => Task::none(),
        Message::IndexerMessage(FileIndexResponse::IndexFinished) if state.window.is_none() => {
            Task::none()
        }
        Message::IndexerMessage(FileIndexResponse::IndexFinished) => {
            Task::done(Message::ResultsUpdated)
        }
        Message::IndexerMessage(FileIndexResponse::Starting(sender)) => {
            sender
                .send(FileIndexMessage::SetConfig(CONFIG.clone()))
                .expect("this should never fail :3");
            state.index_sender = Some(sender);
            Task::none()
        }
        Message::CollectorMessage(CollectorMessage::Ready(controller)) => {
            state.collector_controller = Some(controller);
            Task::none()
        }
        Message::OpenSpecial(window_state) => {
            let (id, task) = if let Some(size) = window_state.size() {
                window::open(Settings {
                    size,
                    resizable: false,
                    level: Level::AlwaysOnTop,
                    ..Default::default()
                })
            } else {
                window::open(Settings::default())
            };
            log::trace!("Opened special window {window_state:?} {id:?}");
            state.special_windows.insert(id, window_state);
            task.map(|_| Message::None)
        }
        _ if state.window.is_none() => Task::none(),
        _ => state.update(message),
    }
}

const fn make_hotkey(mods: HKModifiers, key: Code) -> HotKey {
    HotKey {
        mods,
        key,
        id: (mods.bits() << 16) | key as u32,
    }
}

static HOTKEY: HotKey = make_hotkey(HKModifiers::ALT, Code::KeyP);

fn main() -> iced::Result {
    logging::init();
    log::info!("--- New Run ---");
    let sqlite_deinitializer = sqlite::init();
    let lua = match lua::setup_runtime() {
        Ok(v) => v,
        Err(e) => {
            log::error!("{e}");
            panic!("failed to setup lua");
        }
    };
    let manager = GlobalHotKeyManager::new().expect("failed to start the hotkey manager");
    manager
        .register(HOTKEY)
        .expect("failed to register the hotkey");

    iced::daemon(
        move || {
            let text_input_id = text_input::Id::unique();
            let mut state = State {
                search_query: String::new(),
                results: Vec::new(),
                selected: 0,
                text_input: text_input_id.clone(),
                num_entries: 10,
                offset: 0,
                window: None,
                plugins: Vec::new().into(),
                plugin_builder: Vec::new(),
                theme: Theme::Dracula,
                index_sender: None,
                collector_controller: None,
                showing_actions: false,
                selected_action: 0,
                on_blur: BlurAction::Refocus,
                special_windows: BTreeMap::new(),
                lua: lua.clone(),
            };
            state.add_plugin::<ControlPlugin>();
            state.add_plugin::<ThemePlugin>();
            state.add_plugin::<DicePlugin>();
            state.add_plugin::<FendPlugin>();
            state.add_plugin::<RunPlugin>();
            state.add_lua_plugins();
            state.add_plugin::<FilePlugin>();
            let focus_task = text_input::focus(text_input_id);
            let http_cache_init_task = Task::perform(utils::HTTP_CACHE.init(), |_| Message::None);
            (state, Task::batch([focus_task, http_cache_init_task]))
        },
        daemon_update,
        daemon_view,
    )
    .theme(|s, _| s.theme.clone())
    .subscription(|_| {
        Subscription::batch([
            window::events().map(|ev| match ev.1 {
                window::Event::Unfocused => Message::Blurred,
                window::Event::Closed => Message::Hide(ev.0),
                _ => Message::None,
            }),
            hotkey_sub().map(|ev| {
                if ev.state() == HotKeyState::Pressed && ev.id == HOTKEY.id {
                    Message::Show
                } else {
                    Message::None
                }
            }),
            Subscription::run(file_index::file_index_service).map(Message::IndexerMessage),
            Subscription::run(filter_service::collector).map(Message::CollectorMessage),
            Subscription::run(|| {
                channel(100, |mut sender: Sender<_>| async move {
                    logging::register_message_sender(move |message| {
                        _ = sender.try_send(message);
                    });
                })
            }),
            cache_clear_sub(),
        ])
    })
    .run()?;
    drop(manager);
    drop(sqlite_deinitializer);
    Ok(())
}

fn hotkey_sub() -> Subscription<GlobalHotKeyEvent> {
    Subscription::run(|| {
        channel(32, |mut sender: Sender<_>| async move {
            let receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = receiver.try_recv() {
                    sender.send(event).await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    })
}

fn cache_clear_sub() -> Subscription<Message> {
    Subscription::run(|| {
        channel(32, |_: Sender<_>| async move {
            loop {
                cache::clean_caches().await;
                tokio::time::sleep(Duration::from_secs(10 * 60)).await;
            }
        })
    })
}
