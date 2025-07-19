#![warn(clippy::pedantic)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::unreadable_literal)]
use std::{
    borrow::Cow, collections::BTreeMap, ffi::OsStr, fmt::Debug, hash::Hash, sync::Arc,
    time::Duration,
};

use cache::HTTPCache;
use config::{BlurAction, Config};
use control_plugin::ControlPlugin;
use dice_plugin::DicePlugin;
use fend_plugin::FendPlugin;
use file_index::{FileIndex, FileIndexMessage, FileIndexResponse};
use file_plugin::FilePlugin;
use filter_service::{CollectorController, CollectorMessage, ResultBuilderRef};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use iced::{
    Border, Color, Element, Length, Point, Size, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    border::Radius,
    color,
    futures::{SinkExt, Stream, channel::mpsc::Sender},
    keyboard::{Key, Modifiers, key::Named},
    mouse::ScrollDelta,
    stream::channel,
    widget::{
        MouseArea, button, column, container, mouse_area, row, stack, text, text_input,
        vertical_space,
    },
    window::{self, Level, Position, Settings},
};
use mlua::Lua;
use notify::{EventKind, RecursiveMode, Watcher};
use run_plugin::RunPlugin;
use search_input::SearchInput;
use special_windows::{SpecialWindowMessage, SpecialWindowState};
use sqlite::SqliteContext;
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
use plugin::{AnyPlugin, GenericEntry, StringLike};
pub use plugin::{CustomData, Entry, Plugin};
use tokio::{
    sync::{
        RwLock,
        mpsc::{
            Sender as TokioSender, UnboundedSender, channel as bounded, error::TryRecvError,
            unbounded_channel,
        },
    },
    task::AbortHandle,
};
use utils::CONFIG_FILE;

// #[must_use]
// pub fn make_config() -> Config {
//     Config {
//         files: Files {
//             entries: vec![
//                 FileWatcherEntry {
//                     path: Path::new("/home/fishi").into(),
//                     watch: true,
//                     reindex_every: None,
//                     filter: ScanFilter::default(),
//                 },
//                 // FileWatcherEntry {
//                 //     path: Path::new("/").into(),
//                 //     watch: false,
//                 //     reindex_every: None,
//                 //     filter: ScanFilter {
//                 //         deny_paths: vec![
//                 //             Path::new("/dev").into(),
//                 //             Path::new("/proc").into(),
//                 //             Path::new("/srv").into(),
//                 //             Path::new("/sys").into(),
//                 //             Path::new("/lost+found").into(),
//                 //         ],
//                 //         ..Default::default()
//                 //     },
//                 // },
//             ],
//             reindex_at_startup: false,
//         },
//         on_blur: BlurAction::Refocus,
//         keybind: "Alt+P".into(),
//         enabled_plugins: vec![],
//     }
// }

#[derive(Clone, Debug)]
pub struct MessageSender(Arc<RwLock<UnboundedSender<Message>>>);

impl Hash for MessageSender {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

impl MessageSender {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(unbounded_channel().0)))
    }

    /// sends a message if the channel is open and ignores if it it's closed
    pub async fn send(&self, message: Message) {
        let _: Result<_, _> = self.0.read().await.send(message);
    }

    async fn replace(&self, new_sender: UnboundedSender<Message>) {
        *self.0.write().await = new_sender;
    }
}

impl Default for MessageSender {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Context {
    http_cache: Arc<RwLock<HTTPCache>>,
    file_index: Arc<RwLock<FileIndex>>,
    sqlite: SqliteContext,
    message_sender: MessageSender,
    config: Arc<Config>,
}

#[derive(Clone)]
pub struct SharedAnyPlugin(Arc<dyn AnyPlugin>);
impl Debug for SharedAnyPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[any plugin ")?;
        f.write_str(self.0.any_prefix())?;
        f.write_str("]")
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SpecialWindow(SpecialWindowMessage, window::Id),
    UpdateSearch(String),
    SetSearch(String),
    AddPlugin(SharedAnyPlugin),
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
    GetContext(TokioSender<Context>),
    UpdateConfig(Arc<Config>, bool),
    HideActions,
    Blurred(window::Id),
    OpenSpecial(SpecialWindowState),
    IndexerMessage(FileIndexResponse),
    HotkeyPressed(GlobalHotKeyEvent),
}

type PluginBuilder = Box<dyn FnMut() -> Box<dyn AnyPlugin>>;

pub struct State {
    hotkey: HotKey,
    search_query: String,
    results: Vec<GenericEntry>,
    selected: usize,
    offset: usize,
    text_input: text_input::Id,
    window: Option<window::Id>,
    plugins: Vec<Arc<dyn AnyPlugin>>,
    initializing_plugins: Vec<AbortHandle>,
    plugin_builder: Vec<(StringLike, PluginBuilder)>,
    theme: Theme,
    index_sender: Option<UnboundedSender<FileIndexMessage>>,
    collector_controller: Option<CollectorController>,
    showing_actions: bool,
    selected_action: usize,
    special_windows: BTreeMap<window::Id, SpecialWindowState>,
    lua: Lua,
    context: Context,
    manager: Arc<GlobalHotKeyManager>,
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
            s.push_str("Cmd");
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            s.push_str("Super");
        }
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

fn set_window_height(window_id: window::Id, new_height: f32, resize: bool) -> Task<Message> {
    if !resize {
        return Task::none();
    }
    window::get_size(window_id).then(move |size| {
        Task::batch([
            window::resize(window_id, Size::new(size.width, new_height)),
            Task::done(Message::None),
        ])
    })
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

        for entry_idx in 0..NUM_ENTRIES {
            let index = entry_idx + self.offset;
            if index >= self.results.len() {
                if !self.context.config.auto_resize {
                    col = col.push(
                        vertical_space()
                            .height(Length::Fixed(ENTRY_SIZE))
                            .width(Length::Fill),
                    );
                    continue;
                }
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
                self.plugins.as_slice().into(),
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
                plugin.any_handle_pre(entry.data.clone(), &action.id, self.context.clone()),
                Task::done(Message::HideMainWindow),
                Task::done(Message::HandleAction {
                    plugin: entry.plugin,
                    data: entry.data,
                    action: action.id.to_string(),
                }),
            ])
        } else {
            Task::batch([
                plugin.any_handle_pre(entry.data.clone(), &action.id, self.context.clone()),
                plugin.any_handle_post(entry.data.clone(), &action.id, self.context.clone()),
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
                        set_window_height(window_id, BASE_SIZE, self.context.config.auto_resize),
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
                    return set_window_height(
                        window_id,
                        BASE_SIZE,
                        self.context.config.auto_resize,
                    );
                }
            }
            Message::AddPlugin(plugin) => {
                self.plugins.push(plugin.0);
                self.update_matches();
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
                if self.selected >= self.offset + NUM_ENTRIES {
                    self.offset = self.selected + 1 - NUM_ENTRIES;
                }
                return self.run(index, 0);
            }
            Message::HideMainWindow => {
                self.search_query.clear();
                self.results.clear();
                self.hide_actions();
                self.initializing_plugins
                    .iter()
                    .for_each(AbortHandle::abort);
                self.initializing_plugins.clear();
                if let Some(v) = self.collector_controller.as_mut() {
                    v.stop();
                }
                self.window = None;
                return iced::window::close(window_id);
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
                    self.results.len().min(NUM_ENTRIES) as f32 * ENTRY_SIZE + BASE_SIZE;
                return set_window_height(window_id, new_height, self.context.config.auto_resize);
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
                    let new_height = if self.context.config.auto_resize {
                        self.results.len().min(NUM_ENTRIES) as f32 * ENTRY_SIZE + BASE_SIZE
                    } else {
                        NORESIZE_BASESIZE
                    };
                    let new_height = new_height + actions.len() as f32 * ACTION_SIZE;
                    return set_window_height(window_id, new_height, true);
                }
            }
            Message::HideActions => {
                self.hide_actions();
                let new_height = if self.context.config.auto_resize {
                    self.results.len().min(NUM_ENTRIES) as f32 * ENTRY_SIZE + BASE_SIZE
                } else {
                    NORESIZE_BASESIZE
                };
                return set_window_height(window_id, new_height, true);
            }
            Message::Blurred(id) if id == window_id => match self.context.config.on_blur {
                BlurAction::Refocus => return window::gain_focus(window_id),
                BlurAction::None => {}
            },
            Message::Blurred(_) => {}

            // daemon messages
            Message::Show
            | Message::OpenSpecial(_)
            | Message::Hide(_)
            | Message::HandleAction { .. }
            | Message::None
            | Message::Exit
            | Message::IndexerMessage(_)
            | Message::GetContext(_)
            | Message::UpdateConfig(..)
            | Message::HotkeyPressed(_)
            | Message::SpecialWindow(..)
            | Message::CollectorMessage(CollectorMessage::Ready(_)) => unreachable!(),
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + NUM_ENTRIES {
            self.offset = self.selected + 1 - NUM_ENTRIES;
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

    pub fn add_plugin_instance<T: Plugin + Clone + 'static>(
        &mut self,
        value: T,
        id: impl Into<StringLike>,
    ) {
        self.plugin_builder
            .push((id.into(), Box::new(move || Box::new(value.clone()))));
    }
    pub fn add_plugin<T: Plugin + Default + 'static>(&mut self, id: impl Into<StringLike>) {
        self.plugin_builder
            .push((id.into(), Box::new(|| Box::new(T::default()))));
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
                Ok(v) => self.add_plugin_instance(v, stem),
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
        self.plugins.clear();
        for plugin_builder in self.plugin_builder.iter_mut().map(|(_, v)| v) {
            let mut plugin = plugin_builder();
            let prefix = plugin.any_prefix();
            if prefix != "control"
                && !self
                    .context
                    .config
                    .enabled_plugins
                    .iter()
                    .any(|v| v == prefix)
            {
                continue;
            }
            let context = self.context.clone();
            let sender = context.message_sender.clone();
            self.initializing_plugins.push(
                tokio::spawn(async move {
                    plugin.any_init(context).await;
                    sender
                        .send(Message::AddPlugin(SharedAnyPlugin(plugin.into())))
                        .await;
                })
                .abort_handle(),
            );
        }
    }

    pub fn save_config(&self) {
        let s = match toml::to_string_pretty(&*self.context.config) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed to save the config: {e}");
                return;
            }
        };
        if std::fs::create_dir_all(
            #[allow(clippy::missing_panics_doc)]
            CONFIG_FILE
                .parent()
                .expect("the config file has to have a parent"),
        )
        .is_err()
        {
            log::error!(
                "Failed to save the config: Failed to create path {}",
                CONFIG_FILE.display()
            );
            return;
        }
        if let Err(e) = std::fs::write(&*CONFIG_FILE, s) {
            log::error!(
                "Failed to save config: Failed to write {}: {e}",
                CONFIG_FILE.display()
            );
        }
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
const NUM_ENTRIES: usize = 10;
const NORESIZE_BASESIZE: f32 = BASE_SIZE + NUM_ENTRIES as f32 * ENTRY_SIZE;

fn daemon_view(state: &State, id: window::Id) -> Element<'_, Message> {
    if let Some(main_window_id) = state.window
        && id == main_window_id
    {
        return state.view().into();
    }
    if let Some(window_state) = state.special_windows.get(&id) {
        return window_state.view(id, state);
    }
    text(format!("No state was found for this window. {id:?}")).into()
}

fn daemon_update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::SpecialWindow(msg, id) => {
            let Some(mut window_state) = state.special_windows.remove(&id) else {
                return Task::none();
            };
            let task = window_state.update(id, state, msg);
            state.special_windows.insert(id, window_state);
            task
        }
        Message::Show => {
            let mut settings = Settings {
                resizable: false,
                decorations: false,
                level: Level::AlwaysOnTop,
                position: Position::Centered,
                ..Default::default()
            };
            settings.size.height = NORESIZE_BASESIZE;
            if state.context.config.auto_resize {
                settings.position = Position::SpecificWith(|winsize, resolution| {
                    Point::new(
                        (resolution.width - winsize.width).max(0.0) / 2.0,
                        (resolution.height - BASE_SIZE - 12.0 * ENTRY_SIZE).max(0.0) / 2.0,
                    )
                });
                settings.size.height = BASE_SIZE;
            }
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
            if let Some(id) = state.window
                && window_id == id
            {
                return Task::done(Message::HideMainWindow);
            }
            state.special_windows.remove(&window_id);
            window::close(window_id)
        }
        Message::HandleAction {
            plugin,
            data,
            action,
        } => state.plugins.get(plugin).map_or_else(Task::none, |plugin| {
            plugin.any_handle_post(data, &action, state.context.clone())
        }),
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
                .send(FileIndexMessage::SetFileIndex(
                    state.context.file_index.clone(),
                ))
                .expect("this should never fail :3");
            sender
                .send(FileIndexMessage::SetConfig(state.context.config.clone()))
                .expect("this should never fail :3");
            state.index_sender = Some(sender);
            Task::none()
        }
        Message::UpdateConfig(cfg, save) => {
            let Some(hotkey) =
                keybind::key_and_modifiers_from_str(&cfg.keybind).and_then(keybind::iced_to_hotkey)
            else {
                log::error!(
                    "failed to load config: {:?} is not a valid keybind",
                    cfg.keybind
                );
                return Task::none();
            };
            state.context.config = cfg;
            if save {
                state.save_config();
            }
            if let Some(sender) = state.index_sender.as_ref() {
                // it is fine to ignore this result because if the file indexing stopped, some
                // error occurred and there's no need to spam the console for no reason, the error
                // will already have produced an error message
                _ = sender.send(FileIndexMessage::SetConfig(state.context.config.clone()));
            }
            if let Err(e) = state.manager.unregister(state.hotkey) {
                log::error!("failed to unregister hotkey: {e}");
            }
            if let Err(e) = state.manager.register(hotkey) {
                log::error!("failed to register hotkey: {e}");
            }
            state.hotkey = hotkey;
            let Some(id) = state.window else {
                return Task::none();
            };
            if state.context.config.auto_resize {
                let mut new_height =
                    state.results.len().min(NUM_ENTRIES) as f32 * ENTRY_SIZE + BASE_SIZE;
                if state.showing_actions {
                    new_height += state.get_actions().len() as f32 * ACTION_SIZE;
                }
                set_window_height(id, new_height, true)
            } else {
                set_window_height(id, NORESIZE_BASESIZE, true)
            }
        }
        Message::GetContext(sender) => {
            // it is fine to ignore the error, because it's either full or disconnected.
            // in the case of full, the sender already got the context
            // in the case of disconnected, the sender is no longer interested.
            _ = sender.try_send(state.context.clone());
            Task::none()
        }
        Message::CollectorMessage(CollectorMessage::Ready(mut controller)) => {
            controller.init(state.context.clone());
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
        Message::HotkeyPressed(ev) => {
            if ev.state() == HotKeyState::Pressed && ev.id == state.hotkey.id {
                Task::done(Message::Show)
            } else {
                Task::none()
            }
        }
        _ if state.window.is_none() => Task::none(),
        _ => state.update(message),
    }
}

// static HOTKEY: HotKey = make_hotkey(HKModifiers::ALT, Code::KeyP);
const DEFAULT_CONFIG: &str = "keybind = \"ctrl+space\"";

fn load_config() -> Option<Config> {
    let content = match std::fs::read_to_string(&*CONFIG_FILE) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // default config :3
            _ = std::fs::create_dir_all(CONFIG_FILE.parent().unwrap());
            _ = std::fs::write(&*CONFIG_FILE, DEFAULT_CONFIG);
            DEFAULT_CONFIG.to_string()
        }
        Err(e) => {
            log::error!("failed to load config: {e}");
            return None;
        }
    };
    match toml::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            log::error!("failed to load config: {e}");
            None
        }
    }
}

fn main() -> iced::Result {
    logging::init();
    log::info!("--- New Run ---");
    let Some(config) = load_config() else {
        return Ok(());
    };
    let config = Arc::new(config);
    let Some(hotkey) =
        keybind::key_and_modifiers_from_str(&config.keybind).and_then(keybind::iced_to_hotkey)
    else {
        log::error!(
            "failed to load hotkey: {:?} is not a valid keybind",
            config.keybind
        );
        return Ok(());
    };
    let (sqlite, sqlite_deinitializer) = sqlite::init().expect("failed to initialize sqlite");
    let lua = match lua::setup_runtime() {
        Ok(v) => v,
        Err(e) => {
            log::error!("{e}");
            panic!("failed to setup lua");
        }
    };
    let manager = GlobalHotKeyManager::new().expect("failed to start the hotkey manager");
    manager
        .register(hotkey)
        .expect("failed to register the hotkey");
    let manager = Arc::new(manager);
    let message_sender = MessageSender::new();
    let message_sender_subscription = message_sender.clone();

    iced::daemon(
        move || {
            let text_input_id = text_input::Id::unique();
            let mut state = State {
                search_query: String::new(),
                results: Vec::new(),
                selected: 0,
                text_input: text_input_id.clone(),
                offset: 0,
                window: None,
                plugins: Vec::new(),
                plugin_builder: Vec::new(),
                theme: Theme::Dracula,
                index_sender: None,
                collector_controller: None,
                showing_actions: false,
                selected_action: 0,
                special_windows: BTreeMap::new(),
                lua: lua.clone(),
                context: Context {
                    http_cache: Arc::new(HTTPCache::new().into()),
                    file_index: Arc::new(RwLock::new(FileIndex::new())),
                    sqlite: sqlite.clone(),
                    message_sender: message_sender.clone(),
                    config: config.clone(),
                },
                hotkey,
                manager: manager.clone(),
                initializing_plugins: Vec::new(),
            };
            state.add_plugin::<ControlPlugin>(ControlPlugin.prefix());
            state.add_plugin::<ThemePlugin>(ThemePlugin.prefix());
            state.add_plugin::<DicePlugin>(DicePlugin.prefix());
            state.add_plugin::<FendPlugin>(FendPlugin::PREFIX);
            state.add_plugin::<RunPlugin>(RunPlugin::PREFIX);
            state.add_lua_plugins();
            state.add_plugin::<FilePlugin>(FilePlugin.prefix());
            let focus_task = text_input::focus(text_input_id);
            let http_cache = state.context.http_cache.clone();
            let sqlite = sqlite.clone();
            let http_cache_init_task = Task::perform(
                async move { http_cache.read().await.init(sqlite).await },
                |_| Message::None,
            );
            (state, Task::batch([focus_task, http_cache_init_task]))
        },
        daemon_update,
        daemon_view,
    )
    .theme(|s, _| s.theme.clone())
    .subscription(move |_| {
        Subscription::batch([
            window::events().map(|ev| match ev.1 {
                window::Event::Unfocused => Message::Blurred(ev.0),
                window::Event::Closed => Message::Hide(ev.0),
                _ => Message::None,
            }),
            hotkey_sub().map(Message::HotkeyPressed),
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
            watch_config(),
            Subscription::run_with(message_sender_subscription.clone(), message_sender_handler),
        ])
    })
    .run()?;
    drop(sqlite_deinitializer);
    Ok(())
}

fn message_sender_handler(message_sender: &MessageSender) -> impl Stream<Item = Message> + use<> {
    let message_sender = message_sender.clone();
    channel(32, |mut sender: Sender<_>| async move {
        let (tx, mut rx) = unbounded_channel();
        message_sender.replace(tx).await;
        loop {
            let Some(v) = rx.recv().await else { return };
            match sender.send(v).await {
                Ok(()) => {}
                Err(e) if e.is_full() => {
                    log::warn!("Failed to submit message {e:?}: channel is full");
                }
                Err(_) => (),
            }
        }
    })
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
        channel(32, |mut output: Sender<_>| async move {
            let (sender, mut receiver) = bounded(1);
            if output.send(Message::GetContext(sender)).await.is_err() {
                // the main loop exited
                return;
            }
            // the main loop exited
            let Some(context) = receiver.recv().await else {
                return;
            };
            loop {
                cache::clean_caches(&context).await;
                tokio::time::sleep(Duration::from_secs(10 * 60)).await;
            }
        })
    })
}

fn watch_config() -> Subscription<Message> {
    Subscription::run(|| {
        channel(32, |mut output: Sender<_>| async move {
            let (sender, mut receiver) = unbounded_channel();
            let mut watcher =
                match notify::recommended_watcher(move |ev: Result<notify::Event, _>| {
                    if let Ok(v) = ev
                        && matches!(v.kind, EventKind::Modify(_) | EventKind::Create(_))
                        && v.paths.contains(&*CONFIG_FILE)
                    {
                        _ = sender.send(v);
                    }
                }) {
                    Ok(v) => v,
                    Err(e) => {
                        log::error!("failed to watch the config: {e}");
                        return;
                    }
                };
            if let Err(e) =
                watcher.watch(CONFIG_FILE.parent().unwrap(), RecursiveMode::NonRecursive)
            {
                log::error!("failed to watch the config: {e}");
                return;
            }
            loop {
                let Some(_) = receiver.recv().await else {
                    break;
                };
                tokio::time::sleep(Duration::from_secs(2)).await;
                loop {
                    match receiver.try_recv() {
                        Ok(_) => {}
                        Err(TryRecvError::Empty) => break,
                        Err(_) => return,
                    }
                }
                let Some(cfg) = load_config() else { continue };
                _ = output.send(Message::UpdateConfig(cfg.into(), false)).await;
            }
            drop(watcher);
        })
    })
}
