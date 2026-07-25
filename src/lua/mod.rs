use std::{
    borrow::Cow,
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use iced::{
    Task, clipboard,
    futures::StreamExt as _,
    keyboard::{Key, Modifiers},
    widget::operation::{focus_next, focus_previous},
};
use mlua::{
    AnyUserData, AsChunk, FromLua, FromLuaMulti, Function, Lua, LuaOptions, MaybeSend, StdLib,
    Table, UserData, Value,
};

use crate::{
    Action, CustomData, Entry, Message, Plugin, PluginContext, config::PluginSettings,
    filter_service::ResultBuilderRef, matcher::MatcherInput, plugin::InstancePlugin,
    plugin_settings::PluginWidget,
};

pub struct LuaEntry {
    name: String,
    subtitle: String,
    data: Value,
    perfect_match: bool,
}

impl FromLua for LuaEntry {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let table = Table::from_lua(value, lua)?;
        Ok(Self {
            name: table.get("name")?,
            subtitle: table.get::<Option<String>>("subtitle")?.unwrap_or_default(),
            data: table.get("data")?,
            perfect_match: table.get::<Option<bool>>("perfect_match")?.unwrap_or(false),
        })
    }
}

pub struct LuaPlugin {
    actions: Arc<[Action]>,
    config: Option<PluginSettings>,
    prefixes: Arc<[Cow<'static, str>]>,
    get_for_values: Function,
    init: Option<Function>,
    handle_pre: Option<Function>,
    handle_post: Option<Function>,
    table: Table,
    lua: Lua,
}

impl Clone for LuaPlugin {
    fn clone(&self) -> Self {
        Self {
            actions: self.actions.clone(),
            config: None,
            prefixes: self.prefixes.clone(),
            get_for_values: self.get_for_values.clone(),
            init: self.init.clone(),
            handle_pre: self.handle_pre.clone(),
            handle_post: self.handle_post.clone(),
            table: self.table.clone(),
            lua: self.lua.clone(),
        }
    }
}

impl LuaPlugin {
    fn from_lua(value: Value, lua: &Lua, prefix: Cow<'static, str>) -> mlua::Result<Self> {
        let table: Table = FromLua::from_lua(value, lua)?;
        let actions_data: Vec<AnyUserData> = table.get("actions")?;
        let mut actions = Vec::with_capacity(actions_data.len());
        for action in actions_data {
            actions.push(action.take()?);
        }

        let lua_prefixes = table
            .get::<Vec<String>>("extra_prefixes")
            .unwrap_or_else(|_| Vec::new());
        let mut prefixes = Vec::with_capacity(lua_prefixes.len() + 1);
        prefixes.push(prefix);
        prefixes.extend(lua_prefixes.into_iter().map(Cow::Owned));

        let config = table.get::<Option<Table>>("config")?.map(|table| {
            let mut values = HashMap::new();
            for (k, v) in table
                .pairs::<Box<str>, PluginSettings>()
                .filter_map(Result::ok)
            {
                values.insert(k, v);
            }
            PluginSettings {
                label: Some((&*prefixes[0]).into()),
                description: Box::default(),
                widget: PluginWidget::Object { values },
            }
        });
        Ok(Self {
            get_for_values: table.get("get_for_values")?,
            init: table.get("init")?,
            handle_pre: table.get("handle_pre")?,
            handle_post: table.get("handle_post")?,
            config,
            actions: actions.into(),
            prefixes: prefixes.into(),
            table,
            lua: lua.clone(),
        })
    }

    async fn get_for_values(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
        context: PluginContext<'_>,
    ) -> mlua::Result<()> {
        let thread = self
            .lua
            .create_thread(self.get_for_values.clone())?
            .into_async::<Option<LuaEntry>>((
                &self.table,
                MatcherInputUserData(input),
                ContextUserData::new(context, &self.lua),
            ));
        thread
            .filter_map(async |v| match v {
                Ok(v) => v,
                Err(e) => {
                    log::error!(
                        "lua: failed to get values for plugin `{}`: {e}",
                        self.prefixes[0]
                    );
                    None
                }
            })
            .for_each(|v| async move {
                builder
                    .add(
                        Entry::new(v.name, v.subtitle, CustomData::new(v.data))
                            .perfect(v.perfect_match),
                    )
                    .await;
            })
            .await;
        Ok(())
    }
}

impl InstancePlugin for LuaPlugin {
    fn config(&mut self) -> Option<PluginSettings> {
        self.config.take()
    }
}

impl Plugin for LuaPlugin {
    fn prefixes(&self) -> &[Cow<'static, str>] {
        &self.prefixes
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    async fn get_for_values_arc(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
        context: PluginContext<'_>,
    ) {
        if let Err(e) = LuaPlugin::get_for_values(self, input, builder, context).await {
            log::error!("In {}.lua: {e}", self.prefixes[0]);
        }
    }
    async fn get_for_values(
        &self,
        _: &MatcherInput,
        _: ResultBuilderRef<'_>,
        _: PluginContext<'_>,
    ) {
        unreachable!()
    }

    async fn init(&mut self, context: PluginContext<'_>) {
        if let Some(ref f) = self.init
            && let Err(e) = f
                .call_async::<Value>((&self.table, ContextUserData::new(context, &self.lua)))
                .await
        {
            log::error!("In {}.lua: {e}", self.prefixes[0]);
        }
    }

    fn handle_pre(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext<'_>,
    ) -> Task<Message> {
        let thing = thing.into::<Value>();
        if let Some(ref f) = self.handle_pre {
            match f.call::<TaskWrapper>((
                &self.table,
                thing,
                action,
                ContextUserData::new(context, &self.lua),
            )) {
                Err(e) => log::error!("In {}.lua: {e}", self.prefixes[0]),
                Ok(v) => return v.0,
            }
        }
        Task::none()
    }
    fn handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext<'_>,
    ) -> Task<Message> {
        let thing = thing.into::<Value>();
        if let Some(ref f) = self.handle_post {
            match f.call::<TaskWrapper>((
                &self.table,
                thing,
                action,
                ContextUserData::new(context, &self.lua),
            )) {
                Err(e) => log::error!("In {}.lua: {e}", self.prefixes[0]),
                Ok(v) => return v.0,
            }
        }
        Task::none()
    }
}

// TODO: add context
#[repr(transparent)]
pub struct ContextUserData(mlua::Value);
impl ContextUserData {
    pub fn new(ctx: PluginContext, lua: &Lua) -> Self {
        let value = ctx
            .config
            .map(|v| v.get_lua(lua).clone())
            .unwrap_or_default();
        // TODO: add context
        drop(ctx);
        Self(value)
    }
}

impl UserData for ContextUserData {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("config", |_, me| Ok(me.0.clone()));
    }
}

#[repr(transparent)]
pub struct MatcherInputUserData(Arc<MatcherInput>);

impl UserData for MatcherInputUserData {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("has_prefix", |_, me| Ok(me.0.has_prefix()));
        fields.add_field_method_get("input", |_, me| Ok(me.0.input().to_string()));
        fields.add_field_method_get("words", |_, me| {
            Ok(me.0.words().iter().map(Clone::clone).collect::<Vec<_>>())
        });
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("matches", |_, me, v: String| Ok(me.0.matches(&v)));
    }
}

impl UserData for Action {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("name", |_, me| Ok(me.name.to_string()));
        fields.add_field_method_get("id", |_, me| Ok(me.id.to_string()));
        fields.add_field_method_get("closes", |_, me| Ok(me.closes));
    }
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("keep_open", |_, me: AnyUserData| {
            me.borrow_mut::<Self>()?.closes = false;
            Ok(Value::UserData(me))
        });
    }
}

pub struct TaskWrapper(Task<Message>);
impl FromLua for TaskWrapper {
    fn from_lua(value: Value, _: &mlua::Lua) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(TaskWrapper(Task::none())),
            Value::Table(table) => Ok(TaskWrapper(Task::batch(
                table
                    .pairs()
                    .filter_map(Result::ok)
                    .map(|(_, v): (Value, TaskWrapper)| v.0),
            ))),
            Value::UserData(any_user_data) => any_user_data.take(),
            v => Err(mlua::Error::FromLuaConversionError {
                from: v.type_name(),
                to: "Task".into(),
                message: Some("Expected a task, nil or table of tasks".into()),
            }),
        }
    }
}
impl UserData for TaskWrapper {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_function("chain", |_, (me, other): (TaskWrapper, TaskWrapper)| {
            Ok(TaskWrapper(me.0.chain(other.0)))
        });
    }
}
pub struct KeybindWrapper(Modifiers, Key);
impl FromLua for KeybindWrapper {
    fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Nil => Ok(Self(Modifiers::empty(), Key::Unidentified)),
            Value::String(ref s) => {
                let (modifiers, key) = crate::keybind::key_and_modifiers_from_str(&s.to_str()?)
                    .ok_or_else(|| match s.to_str() {
                        Err(e) => e,
                        Ok(s) => mlua::Error::FromLuaConversionError {
                            from: value.type_name(),
                            to: "Keybind".into(),
                            message: Some(format!("{s:?} is not a valid keybind!")),
                        },
                    })?;
                Ok(Self(modifiers, key))
            }
            Value::Table(table) => {
                let mut pairs = table.pairs::<usize, String>().peekable();
                let mut modifiers = Modifiers::empty();
                loop {
                    let Some(v) = pairs.next() else { break };
                    let v = v?.1;
                    if pairs.peek().is_some() {
                        modifiers |= crate::keybind::modifier_from_str(&v).ok_or_else(|| {
                            mlua::Error::FromLuaConversionError {
                                from: "String",
                                to: "Modifier".into(),
                                message: Some(format!("{v:?} is not a valid modifier")),
                            }
                        })?;
                    } else {
                        return Ok(Self(modifiers, crate::keybind::key_from_str(&v)));
                    }
                }
                Ok(Self(Modifiers::empty(), Key::Unidentified))
            }
            v => Err(mlua::Error::FromLuaConversionError {
                from: v.type_name(),
                to: "Keybind".into(),
                message: Some("Expected a task, nil or table of tasks".into()),
            }),
        }
    }
}

pub fn luma_module(lua: &Lua) -> mlua::Result<Table> {
    fn task_fn<V: FromLuaMulti>(
        lua: &Lua,
        f: impl Fn(&Lua, V) -> Task<Message> + 'static + MaybeSend,
    ) -> mlua::Result<Value> {
        let func = lua.create_function(move |lua, v| Ok(TaskWrapper(f(lua, v))))?;
        Ok(Value::Function(func))
    }
    fn message<V: FromLuaMulti>(
        lua: &Lua,
        f: impl Fn(V) -> Message + 'static + MaybeSend,
    ) -> mlua::Result<Value> {
        task_fn(lua, move |_, v| Task::done(f(v)))
    }
    fn action_fn<V: FromLuaMulti>(
        lua: &Lua,
        f: impl Fn(&Lua, V) -> Action + 'static + MaybeSend,
    ) -> mlua::Result<Value> {
        let func = lua.create_function(move |lua, v| Ok(f(lua, v)))?;
        Ok(Value::Function(func))
    }

    let root = lua.create_table()?;

    // ┌───────┐
    // │ Tasks │
    // └───────┘
    let task = lua.create_table()?;
    task.set("none", task_fn(lua, |_, ()| Task::none())?)?;

    // messages
    task.set("set_search", message(lua, Message::SetSearch)?)?;
    task.set("update_search", message(lua, Message::UpdateSearch)?)?;
    task.set("show", message(lua, |()| Message::Show)?)?;
    task.set("hide", message(lua, |()| Message::HideMainWindow)?)?;
    task.set("exit", message(lua, |()| Message::Exit)?)?;

    // widgets
    task.set("focus_next", task_fn(lua, |_, ()| focus_next())?)?;
    task.set("focus_prev", task_fn(lua, |_, ()| focus_previous())?)?;

    // clipboard
    task.set(
        "write_clipboard",
        task_fn(lua, |_, s: String| clipboard::write(s))?,
    )?;
    root.set("task", task)?;

    // ┌─────────┐
    // │ Actions │
    // └─────────┘
    let action = lua.create_table()?;
    action.set(
        "default",
        action_fn(lua, |_, (name, id): (String, String)| {
            Action::default_owned(name, id)
        })?,
    )?;
    action.set(
        "suggest",
        action_fn(lua, |_, (name, id): (String, String)| {
            Action::suggest_owned(name, id)
        })?,
    )?;
    action.set(
        "without_shortcut",
        action_fn(lua, |_, (name, id): (String, String)| {
            Action::without_shortcut_owned(name, id)
        })?,
    )?;
    action.set(
        "new",
        action_fn(
            lua,
            |_, (name, id, key): (String, String, KeybindWrapper)| {
                Action::new_owned(name, id, (key.0, key.1))
            },
        )?,
    )?;
    root.set("action", action)?;

    Ok(root)
}

pub fn proxy(lua: &Lua, proxied_value: Table) -> mlua::Result<Table> {
    let env = lua.create_table()?;
    let metatable = lua.create_table()?;
    metatable.raw_set(
        "__index",
        lua.create_function(move |_, (table, key): (Value, Value)| {
            let res: Value = proxied_value.get(&key)?;
            let res = match res {
                Value::Table(ref v) if *v == proxied_value => table,
                v => v,
            };
            Ok(res)
        })?,
    )?;
    metatable.set("__metatable", Value::Nil)?;
    env.set_metatable(Some(metatable));
    Ok(env)
}

pub fn setup_runtime() -> mlua::Result<Lua> {
    let libs = StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH;
    let lua = Lua::new_with(libs, LuaOptions::new())?;
    let luma_module = luma_module(&lua)?;
    lua.globals().set("luma", luma_module)?;
    Ok(lua)
}

pub fn load_lua_plugin<'a>(
    lua: &Lua,
    src: impl AsChunk<'a>,
    prefix: impl Into<Cow<'static, str>>,
) -> mlua::Result<LuaPlugin> {
    let value = lua
        .load(src)
        .set_environment(proxy(lua, lua.globals())?)
        .call(())?;
    LuaPlugin::from_lua(value, lua, prefix.into())
}

pub static LUA_PLUGIN_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::current_dir().unwrap().join("lua_plugins"));

impl FromLua for PluginSettings {
    // fine cuz we're forced to do this by the trait.
    #[allow(clippy::only_used_in_recursion)]
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        use crate::plugin_settings::PluginWidget as PW;

        fn needs_more_cx(s: &'static str) -> mlua::Result<PluginSettings> {
            Err(mlua::Error::FromLuaConversionError {
                from: "string",
                to: "PluginWidget".to_string(),
                message: Some(format!(
                    "Widget of type {s:?} needs more context (i.e. you need a table)"
                )),
            })
        }

        let label: Option<Box<str>>;
        let description: Box<str>;

        let widget = match value {
            Value::String(s) => {
                label = None;
                description = Box::default();

                let ty = s.to_str()?;
                match &*ty {
                    "section" => return needs_more_cx("section"),
                    "list" => return needs_more_cx("list"),
                    "dropdown" => return needs_more_cx("dropdown"),
                    "searchable_dropdown" => return needs_more_cx("searchable_dropdown"),
                    "intslider" => return needs_more_cx("intslider"),
                    "int_slider" => return needs_more_cx("int_slider"),
                    "slider" => return needs_more_cx("slider"),

                    "paragraph" | "paragraph_input" => PW::ParagraphInput {
                        min: 0,
                        max: None,
                        default: Box::default(),
                    },
                    "string" | "input" | "string_input" => PW::StringInput {
                        min: 0,
                        max: None,
                        default: Box::default(),
                    },
                    "checkbox" | "checkmark" => PW::Checkbox { default: false },
                    "toggle" | "switch" => PW::Toggle { default: false },
                    "intinput" | "int_input" => PW::IntInput {
                        min: None,
                        max: None,
                        step: 1,
                        default: 0,
                    },
                    "numinput" | "num_input" => PW::NumInput {
                        min: None,
                        max: None,
                        step: None,
                        default: 0.0,
                    },
                    _ => {
                        return Err(mlua::Error::FromLuaConversionError {
                            from: "table",
                            to: "PluginWidget".to_string(),
                            message: Some(format!("No widget type {ty:?}")),
                        });
                    }
                }
            }
            Value::Table(t) => {
                label = t.get("label")?;
                description = t.get::<Option<Box<str>>>("label")?.unwrap_or_default();
                let ty = t.get::<mlua::String>("type")?;
                let ty = ty.to_str()?;
                match &*ty {
                    "section" => {
                        let mut values = HashMap::new();
                        for (k, v) in t
                            .pairs::<Box<str>, Value>()
                            .filter_map(Result::ok)
                            .filter(|(k, _)| **k != *"type" && **k != *"label")
                        {
                            let v = Self::from_lua(v, lua)?;
                            values.insert(k, v);
                        }
                        PW::Object { values }
                    }
                    "list" => PW::List {
                        max_entries: t.get("max_entries")?,
                        value_type: Box::new(t.get("value_type")?),
                    },
                    "paragraph" | "paragraph_input" => PW::ParagraphInput {
                        min: t.get::<Option<_>>("min")?.unwrap_or(0),
                        max: t.get("max")?,
                        default: t.get::<Option<_>>("default")?.unwrap_or_default(),
                    },
                    "string" | "input" | "string_input" => PW::StringInput {
                        min: t.get::<Option<_>>("min")?.unwrap_or(0),
                        max: t.get("max")?,
                        default: t.get::<Option<_>>("default")?.unwrap_or_default(),
                    },
                    "checkbox" | "checkmark" => PW::Checkbox {
                        default: t.get::<Option<_>>("default")?.unwrap_or(false),
                    },
                    "toggle" | "switch" => PW::Toggle {
                        default: t.get::<Option<_>>("default")?.unwrap_or(false),
                    },
                    "dropdown" => {
                        let values: Vec<Box<str>> = t.get("values")?;
                        PW::Dropdown {
                            default: t
                                .get::<Option<Box<str>>>("default")?
                                .and_then(|v| values.iter().position(|el| *el == v))
                                .unwrap_or(0),
                            values,
                        }
                    }
                    "searchable_dropdown" => {
                        let values: Vec<Box<str>> = t.get("values")?;
                        PW::SearchableDropdown {
                            default: t
                                .get::<Option<Box<str>>>("default")?
                                .and_then(|v| values.iter().position(|el| *el == v))
                                .unwrap_or(0),
                            values,
                        }
                    }
                    "intslider" | "int_slider" => {
                        let min = t.get("min")?;
                        PW::IntSlider {
                            min,
                            max: t.get("max")?,
                            step: t.get::<Option<_>>("step")?.unwrap_or(1),
                            default: t.get::<Option<_>>("default")?.unwrap_or(min),
                        }
                    }
                    "intinput" | "int_input" => {
                        let min = t.get("min")?;
                        PW::IntInput {
                            min,
                            max: t.get("max")?,
                            step: t.get::<Option<_>>("step")?.unwrap_or(1),
                            default: t.get::<Option<i64>>("default")?.and(min).unwrap_or(0),
                        }
                    }
                    "slider" => {
                        let min = t.get("min")?;
                        PW::Slider {
                            min,
                            max: t.get("max")?,
                            step: t.get("step")?,
                            default: t.get::<Option<_>>("default")?.unwrap_or(min),
                        }
                    }
                    "numinput" | "num_input" => {
                        let min = t.get("min")?;
                        PW::NumInput {
                            min,
                            max: t.get("max")?,
                            step: t.get("step")?,
                            default: t.get::<Option<f64>>("default")?.and(min).unwrap_or(0.0),
                        }
                    }
                    _ => {
                        return Err(mlua::Error::FromLuaConversionError {
                            from: "table",
                            to: "PluginWidget".to_string(),
                            message: Some(format!("No widget type {ty:?}")),
                        });
                    }
                }
            }
            _ => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: value.type_name(),
                    to: "PluginSettings".to_string(),
                    message: Some("Expected either a table or a string".to_string()),
                });
            }
        };
        Ok(Self {
            label,
            description,
            widget,
        })
    }
}
