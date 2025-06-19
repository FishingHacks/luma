use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::Instant,
};

use iced::{
    Task, clipboard,
    futures::StreamExt as _,
    keyboard::{Key, Modifiers},
    widget,
};
use mlua::{
    AnyUserData, AsChunk, FromLua, FromLuaMulti, Function, Lua, LuaOptions, MaybeSend, StdLib,
    Table, UserData, Value,
};

use crate::{
    Action, CustomData, Entry, Message, Plugin, filter_service::ResultBuilderRef,
    matcher::MatcherInput,
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

#[derive(Clone)]
pub struct LuaPlugin {
    actions: Arc<[Action]>,
    prefix: Arc<str>,
    get_for_values: Function,
    init: Option<Function>,
    handle_pre: Option<Function>,
    handle_post: Option<Function>,
    me: Table,
    lua: Lua,
}

impl LuaPlugin {
    fn from_lua(value: Value, lua: &Lua, prefix: impl Into<Arc<str>>) -> mlua::Result<Self> {
        let table: Table = FromLua::from_lua(value, lua)?;
        let actions_data: Vec<AnyUserData> = table.get("actions")?;
        let mut actions = Vec::with_capacity(actions_data.len());
        for action in actions_data {
            actions.push(action.take()?);
        }
        Ok(Self {
            actions: actions.into(),
            prefix: prefix.into(),
            get_for_values: table.get("get_for_values")?,
            init: table.get("init")?,
            handle_pre: table.get("handle_pre")?,
            handle_post: table.get("handle_post")?,
            me: table,
            lua: lua.clone(),
        })
    }
}

impl LuaPlugin {
    async fn get_for_values(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
    ) -> mlua::Result<()> {
        let thread = self
            .lua
            .create_thread(self.get_for_values.clone())?
            .into_async::<Option<LuaEntry>>((&self.me, MatcherInputUserData(input)));
        thread
            .filter_map(async |v| v.ok().flatten())
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

impl Plugin for LuaPlugin {
    fn prefix(&self) -> &str {
        &self.prefix
    }

    fn actions(&self) -> &[Action] {
        &self.actions
    }

    async fn get_for_values_arc(&self, input: Arc<MatcherInput>, builder: ResultBuilderRef<'_>) {
        if let Err(e) = LuaPlugin::get_for_values(self, input, builder).await {
            log::error!("In {}.lua: {e}", self.prefix);
        }
    }
    async fn get_for_values(&self, _: &MatcherInput, _: ResultBuilderRef<'_>) {
        unreachable!()
    }

    fn init(&mut self) {
        if let Some(ref f) = self.init {
            if let Err(e) = f.call::<Value>(&self.me) {
                log::error!("In {}.lua: {e}", self.prefix);
            }
        }
    }

    fn handle_pre(&self, thing: CustomData, action: &str) -> Task<Message> {
        let thing = thing.into::<Value>();
        if let Some(ref f) = self.handle_pre {
            match f.call::<TaskWrapper>((&self.me, thing, action)) {
                Err(e) => log::error!("In {}.lua: {e}", self.prefix),
                Ok(v) => return v.0,
            }
        }
        Task::none()
    }
    fn handle_post(&self, thing: CustomData, action: &str) -> Task<Message> {
        let thing = thing.into::<Value>();
        if let Some(ref f) = self.handle_post {
            match f.call::<TaskWrapper>((&self.me, thing, action)) {
                Err(e) => log::error!("In {}.lua: {e}", self.prefix),
                Ok(v) => return v.0,
            }
        }
        Task::none()
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
                    .filter_map(|v| v.ok())
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
                            message: Some(format!("{:?} is not a valid keybind!", s)),
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
    let root = lua.create_table()?;

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

    // ┌───────┐
    // │ Tasks │
    // └───────┘
    let task = lua.create_table()?;
    task.set("none", task_fn(lua, |_, _: ()| Task::none())?)?;

    // messages
    task.set("set_search", message(lua, Message::SetSearch)?)?;
    task.set("update_search", message(lua, Message::UpdateSearch)?)?;
    task.set("show", message(lua, |_: ()| Message::Show)?)?;
    task.set("hide", message(lua, |_: ()| Message::HideMainWindow)?)?;
    task.set("exit", message(lua, |_: ()| Message::Exit)?)?;

    // widgets
    task.set("focus_next", task_fn(lua, |_, _: ()| widget::focus_next())?)?;
    task.set(
        "focus_prev",
        task_fn(lua, |_, _: ()| widget::focus_previous())?,
    )?;

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
    prefix: impl Into<Arc<str>>,
) -> mlua::Result<LuaPlugin> {
    let value = lua
        .load(src)
        .set_environment(proxy(lua, lua.globals())?)
        .call(())?;
    LuaPlugin::from_lua(value, lua, prefix)
}

pub static LUA_PLUGIN_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::current_dir().unwrap().join("lua_plugins"));
