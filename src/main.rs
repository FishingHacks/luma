use std::{borrow::Cow, fmt::Debug, sync::Arc, time::Duration};

use control_plugin::ControlPlugin;
use convert_plugin::ConvertPlugin;
use dice_plugin::DicePlugin;
use file_plugin::{FilePlugin, IndexerMessage};
use filter_service::{CollectorController, CollectorMessage};
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use iced::{
    Color, Element, Length, Subscription, Task, Theme,
    alignment::{Horizontal, Vertical},
    futures::{SinkExt, StreamExt, channel::mpsc::Sender, future::BoxFuture},
    keyboard::{self, Key},
    mouse::{Interaction, ScrollDelta},
    stream::channel,
    widget::{
        MouseArea, button, column, horizontal_space, mouse_area, row, stack, text, text_input,
    },
    window::{self, Level, Settings},
};
use matcher::MatcherInput;
use run_plugin::RunPlugin;
use search_input::SearchInput;
use theme_plugin::ThemePlugin;

mod config;
mod control_plugin;
mod convert_plugin;
mod dice_plugin;
mod file_index;
mod file_plugin;
mod filter_service;
mod matcher;
mod run_plugin;
mod search_input;
mod theme_plugin;
mod utils;
pub use filter_service::ResultBuilder;

pub trait CustomDataCompatible: std::any::Any + Send + Sync + 'static {
    fn clone_custom_data(&self) -> Box<dyn CustomDataCompatible>;
}
impl<T: std::any::Any + Clone + Send + Sync> CustomDataCompatible for T {
    fn clone_custom_data(&self) -> Box<dyn CustomDataCompatible> {
        Box::new(self.clone())
    }
}

pub trait AsCustomData {
    fn from_custom_data(custom_data: CustomData) -> Self;
    fn to_custom_data(&self) -> Self;
}

pub trait Plugin: Send + Sync {
    fn prefix(&self) -> &'static str;
    fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: &ResultBuilder,
    ) -> impl std::future::Future<Output = ()> + std::marker::Send;
    fn init(&mut self);
    fn handle(&self, thing: CustomData) -> Task<Message>;
    fn should_close(&self) -> bool {
        true
    }
}

pub trait AnyPlugin: Send + Sync {
    fn as_any_ref(&self) -> &dyn std::any::Any;
    fn any_prefix(&self) -> &'static str;
    fn any_get_for_values<'future, 'a: 'future, 'b: 'future, 'c: 'future, 'd: 'future>(
        &'a self,
        input: &'b MatcherInput<'c>,
        builder: &'d ResultBuilder,
    ) -> BoxFuture<'future, ()>;
    fn any_init(&mut self);
    fn any_handle(&self, thing: CustomData) -> Task<Message>;
    fn any_should_close(&self) -> bool;
}
impl<T: Plugin + 'static> AnyPlugin for T {
    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn any_prefix(&self) -> &'static str {
        self.prefix()
    }

    fn any_get_for_values<'future, 'a: 'future, 'b: 'future, 'c: 'future, 'd: 'future>(
        &'a self,
        input: &'b MatcherInput<'c>,
        builder: &'d ResultBuilder,
    ) -> BoxFuture<'future, ()> {
        Box::pin(self.get_for_values(input, builder))
    }

    fn any_init(&mut self) {
        self.init();
    }

    fn any_handle(&self, thing: CustomData) -> Task<Message> {
        self.handle(thing)
    }

    fn any_should_close(&self) -> bool {
        self.should_close()
    }
}

impl Debug for CustomData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<custom user data>")
    }
}

impl Clone for Box<dyn CustomDataCompatible> {
    fn clone(&self) -> Self {
        (**self).clone_custom_data()
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdateSearch(String),
    GoUp,
    GoDown,
    Go10Up,
    Go10Down,
    Submit,
    Click(usize),
    Hide,
    Show,
    WindowOpened(window::Id),
    ChangeTheme(Theme),
    HandleAction {
        plugin: &'static str,
        data: CustomData,
    },
    None,
    InputPress,
    Exit,
    Reindex,
    FileIndexMessage(IndexerMessage),
    CollectorMessage(CollectorMessage),
    ResultsUpdated,
    KeyPressed(Key, keyboard::Modifiers),
    ShowActions,
    HideActions,
}

#[derive(Clone)]
pub struct CustomData(Box<dyn CustomDataCompatible>);

impl CustomData {
    pub fn new<T: CustomDataCompatible>(value: T) -> Self {
        Self(Box::new(value))
    }

    pub fn into<T: CustomDataCompatible>(self) -> T {
        *(self.0 as Box<dyn std::any::Any>)
            .downcast()
            .expect("this should never fail")
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    name: String,
    subtitle: Cow<'static, str>,
    plugin: &'static str,
    data: CustomData,
}

impl Entry {
    pub fn new(
        name: impl Into<String>,
        subtitle: impl Into<Cow<'static, str>>,
        plugin: &'static str,
        data: CustomData,
    ) -> Self {
        Self {
            name: name.into(),
            subtitle: subtitle.into(),
            plugin,
            data,
        }
    }
}

pub struct State {
    search_query: String,
    results: Vec<Entry>,
    selected: usize,
    offset: usize,
    num_entries: usize,
    text_input: text_input::Id,
    window: Option<window::Id>,
    plugins: Arc<Vec<Box<dyn AnyPlugin>>>,
    plugin_builder: Vec<Box<dyn FnMut() -> Box<dyn AnyPlugin>>>,
    theme: Theme,
    index_sender: Option<Sender<()>>,
    collector_controller: Option<CollectorController>,
    showing_actions: bool,
    selected_action: usize,
    num_actions: usize,
}

const ALLOWED_ACTION_MODIFIERS: keyboard::Modifiers = keyboard::Modifiers::COMMAND
    .union(keyboard::Modifiers::ALT)
    .union(keyboard::Modifiers::CTRL)
    .union(keyboard::Modifiers::LOGO);

static ACTIONS: &[&str] = &["open (enter)", "suggest (tab)", "delete (ctrl+d)"];

impl State {
    pub fn view(&self, _: window::Id) -> MouseArea<'_, Message> {
        let search_field = SearchInput::new(&self.search_query, self.text_input.clone());
        let mut col = column![stack([
            search_field.into(),
            mouse_area(horizontal_space().width(Length::Fill).height(Length::Fill))
                .on_press(Message::InputPress)
                .interaction(Interaction::Idle)
                .into(),
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
                text(entry.plugin).size(16).into()
            } else {
                row![
                    text(entry.plugin).size(16).style(text::default),
                    text(" • ").size(16),
                    text(entry.subtitle.as_ref())
                        .size(16)
                        .wrapping(text::Wrapping::None),
                ]
                .height(20)
                .width(Length::Fill)
                .into()
            };
            let inner_col = column![
                text(&entry.name)
                    .size(20)
                    .height(25)
                    .wrapping(text::Wrapping::None),
                subtitle
            ];
            col = col.push(
                button(inner_col)
                    .width(Length::Fill)
                    .style(if selected {
                        button::primary
                    } else {
                        button::text
                    })
                    .on_press(Message::Click(entry_idx + self.offset)),
            );
        }

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

    fn update_matches(&mut self) {
        if self.search_query.is_empty() {
            self.results.clear();
            return;
        }
        if let Some(controller) = &mut self.collector_controller {
            controller.start(self.plugins.clone(), self.search_query.to_lowercase());
        } else {
            eprintln!("Failed to query: no collector controller present");
        }
    }

    fn run(&mut self, index: usize) -> iced::Task<Message> {
        if self.results.is_empty() {
            return Task::none();
        }
        let entry = &self.results[index];
        for plugin in self.plugins.iter() {
            if !plugin.any_should_close() && plugin.any_prefix() == entry.plugin {
                return plugin.any_handle(entry.data.clone());
            }
        }
        let entry = self.results.remove(index);
        Task::done(Message::Hide).chain(Task::done(Message::HandleAction {
            plugin: entry.plugin,
            data: entry.data,
        }))
    }

    fn handle_go_up(&mut self, amount: usize) {
        if self.showing_actions {
            self.selected_action = self.selected_action.saturating_sub(amount);
        } else {
            self.selected = self.selected.saturating_sub(amount);
        }
    }

    fn handle_go_down(&mut self, amount: usize) {
        if self.showing_actions {
            self.selected_action = (self.selected_action + amount).min(self.num_actions);
        } else {
            self.selected = (self.selected + amount).min(self.results.len() - 1);
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::UpdateSearch(q) => {
                self.search_query = q;
                self.update_matches();
                self.selected = 0;
            }
            Message::KeyPressed(key, modifiers) => println!("{modifiers:?} {key:?}"),
            Message::ResultsUpdated => self.update_matches(),
            Message::GoUp => self.handle_go_up(1),
            Message::Go10Up => self.handle_go_up(10),
            Message::GoDown => self.handle_go_down(1),
            Message::Go10Down => self.handle_go_down(10),
            Message::Submit => return self.run(self.selected),
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
                return self.run(index);
            }
            Message::Hide => {
                self.search_query.clear();
                self.results.clear();
                self.showing_actions = false;
                self.selected_action = 0;
                self.num_actions = 0;
                if let Some(v) = self.collector_controller.as_mut() {
                    v.stop()
                }
                if let Some(window) = self.window.take() {
                    return iced::window::close(window);
                }
            }
            Message::WindowOpened(id) => self.window = Some(id),
            Message::ChangeTheme(theme) => self.theme = theme,
            Message::InputPress => {
                let Some(window) = self.window else {
                    return text_input::focus(self.text_input.clone());
                };
                return text_input::focus(self.text_input.clone()).chain(window::drag(window));
            }
            Message::CollectorMessage(CollectorMessage::Finished(results)) => {
                self.results = results;
            }
            Message::ShowActions => {
                self.showing_actions = true;
                self.selected_action = 0;
                self.num_actions = ACTIONS.len();
            }
            Message::HideActions => {
                self.showing_actions = false;
                self.selected_action = 0;
                self.num_actions = 0;
            }

            // daemon messages
            Message::Show
            | Message::HandleAction { .. }
            | Message::None
            | Message::Exit
            | Message::Reindex
            | Message::FileIndexMessage(_)
            | Message::CollectorMessage(CollectorMessage::Ready(_)) => unreachable!(),
        }
        if self.selected >= self.results.len() && !self.results.is_empty() {
            self.selected = self.results.len() - 1;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + self.num_entries {
            self.offset = self.selected + 1 - self.num_entries;
        }
        Task::none()
    }

    pub fn add_plugin<T: Plugin + Default + 'static>(&mut self) {
        self.plugin_builder
            .push(Box::new(|| Box::new(T::default())));
    }

    pub fn init_plugins(&mut self) {
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

const SEARCH_SIZE: usize = 31;
const ENTRY_SIZE: usize = 56;

fn daemon_update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Show => {
            let mut settings = Settings {
                resizable: false,
                decorations: false,
                level: Level::AlwaysOnTop,
                ..Default::default()
            };
            settings.size.height = (SEARCH_SIZE + ENTRY_SIZE * state.num_entries + 200) as f32;
            let (id, task) = window::open(settings);
            let old_window = state.window.replace(id);
            state.init_plugins();
            let task = task
                .chain(text_input::focus(state.text_input.clone()))
                .map(|_| Message::None);
            match old_window {
                Some(id) => window::close(id).chain(task),
                None => task,
            }
        }
        Message::HandleAction {
            plugin: plugin_prefix,
            data,
        } => {
            for plugin in state.plugins.iter() {
                if plugin.any_prefix() == plugin_prefix {
                    return plugin.any_handle(data);
                }
            }
            println!("Err: No plugin called '{plugin_prefix}' found!");
            Task::none()
        }
        Message::Exit => iced::exit(),
        Message::None => Task::none(),
        Message::Reindex => match &mut state.index_sender {
            None => Task::none(),
            Some(v) => match v.try_send(()) {
                Ok(_) => Task::none(),
                Err(e) if e.is_full() => Task::none(),
                Err(e) => {
                    eprintln!("Faiiled to request a file reindex: {e:?}");
                    state.index_sender = None;
                    Task::none()
                }
            },
        },
        Message::FileIndexMessage(msg) => match msg {
            IndexerMessage::Ready(sender) => {
                state.index_sender = Some(sender);
                Task::done(Message::Reindex)
            }
            IndexerMessage::IndexingFinished if state.window.is_none() => Task::none(),
            IndexerMessage::IndexingFinished => Task::done(Message::ResultsUpdated),
        },
        Message::CollectorMessage(CollectorMessage::Ready(controller)) => {
            state.collector_controller = Some(controller);
            Task::none()
        }
        _ => state.update(message),
    }
}

const fn make_hotkey(mods: Modifiers, key: Code) -> HotKey {
    HotKey {
        mods,
        key,
        id: (mods.bits() << 16) | key as u32,
    }
}

static HOTKEY: HotKey = make_hotkey(Modifiers::ALT, Code::KeyP);

fn main() -> iced::Result {
    let manager = GlobalHotKeyManager::new().expect("failed to start the hotkey manager");
    manager
        .register(HOTKEY)
        .expect("failed to register the hotkey");

    iced::daemon("spotlight", daemon_update, State::view)
        .theme(|s, _| s.theme.clone())
        .subscription(|_| {
            Subscription::batch([
                window::open_events().map(Message::WindowOpened),
                hotkey_sub().map(|ev| {
                    if ev.state() == HotKeyState::Pressed && ev.id == HOTKEY.id {
                        Message::Show
                    } else {
                        Message::None
                    }
                }),
                // re-index every 10 minutes
                Subscription::run(|| smol::Timer::interval(Duration::from_secs(10 * 60)).boxed())
                    .map(|_| Message::Reindex),
                // Subscription::run(file_plugin::start_indexer).map(Message::FileIndexMessage),
                Subscription::run(filter_service::collector).map(Message::CollectorMessage),
            ])
        })
        .run_with(move || {
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
                num_actions: 0,
            };
            state.add_plugin::<RunPlugin>();
            state.add_plugin::<ConvertPlugin>();
            state.add_plugin::<ThemePlugin>();
            state.add_plugin::<ControlPlugin>();
            state.add_plugin::<FilePlugin>();
            state.add_plugin::<DicePlugin>();
            (state, text_input::focus(text_input_id))
        })?;
    drop(manager);
    Ok(())
}

fn hotkey_sub() -> Subscription<GlobalHotKeyEvent> {
    Subscription::run(|| {
        channel(32, |mut sender| async move {
            let receiver = GlobalHotKeyEvent::receiver();
            loop {
                if let Ok(event) = receiver.try_recv() {
                    sender.send(event).await.unwrap();
                }
                smol::Timer::interval(Duration::from_millis(50)).await;
            }
        })
    })
}
