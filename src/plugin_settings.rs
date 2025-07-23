use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::ops::{BitOr, BitOrAssign, Deref, Index};
use std::sync::{Arc, OnceLock};
use tokio::sync::{RwLock, RwLockReadGuard};

use mlua::IntoLua;
use serde::{Deserialize, Serialize, de::Visitor};

impl<'de> Deserialize<'de> for PluginSettingsValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use PluginSettingsValue as PSV;
        struct PSVVisitor;
        impl<'de> Visitor<'de> for PSVVisitor {
            type Value = PSV;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("any valid toml value but a datetime")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::Boolean(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::Int(v))
            }

            fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_i64(i64::from(v))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::Number(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_string(v.to_string())
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::String(v))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::Null)
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                Deserialize::deserialize(deserializer)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PSV::Null)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut list = Vec::new();
                while let Some(v) = seq.next_element()? {
                    list.push(v);
                }
                Ok(PSV::List(list))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut btree = BTreeMap::new();
                while let Some((k, v)) = map.next_entry()? {
                    btree.insert(k, v);
                }
                Ok(PSV::Map(btree))
            }
        }
        deserializer.deserialize_any(PSVVisitor)
    }
}

impl Serialize for PluginSettingsValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use PluginSettingsValue as PSV;
        match self {
            PSV::String(s) => serializer.serialize_str(s),
            PSV::Number(n) => serializer.serialize_f64(*n),
            PSV::Int(i) => serializer.serialize_i64(*i),
            PSV::Boolean(b) => serializer.serialize_bool(*b),
            PSV::List(list) => serializer.collect_seq(list.iter()),
            PSV::Map(map) => serializer.collect_map(map.iter()),
            PSV::Null => serializer.serialize_none(),
        }
    }
}

impl Debug for PluginSettingsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginSettingsValue::String(v) => Debug::fmt(v, f),
            PluginSettingsValue::Number(v) => Debug::fmt(v, f),
            PluginSettingsValue::Int(v) => Debug::fmt(v, f),
            PluginSettingsValue::Boolean(v) => Debug::fmt(v, f),
            PluginSettingsValue::List(v) => Debug::fmt(v, f),
            PluginSettingsValue::Map(map) => f.debug_map().entries(map.iter()).finish(),
            PluginSettingsValue::Null => Ok(()),
        }
    }
}

impl PluginSettingsValue {
    pub fn as_str_default(&self) -> &str {
        match self {
            Self::String(s) => s,
            _ => "",
        }
    }
    pub fn as_number_default(&self) -> f64 {
        match self {
            Self::Number(n) => *n,
            _ => 0.0,
        }
    }
    pub fn as_boolean_default(&self) -> bool {
        match self {
            Self::Boolean(b) => *b,
            _ => false,
        }
    }
    pub fn as_int_default(&self) -> i64 {
        match self {
            Self::Int(n) => *n,
            _ => 0,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
    pub fn as_list(&self) -> &[PluginSettingsValue] {
        match self {
            Self::List(l) => l,
            _ => &[],
        }
    }
    pub fn as_map(&self) -> Option<&BTreeMap<Box<str>, Self>> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }
}

impl IntoLua for &PluginSettingsValue {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        use PluginSettingsValue as PSV;
        Ok(match self {
            PSV::String(s) => mlua::Value::String(lua.create_string(s)?),
            PSV::Number(n) => mlua::Value::Number(*n),
            PSV::Int(i) => mlua::Value::Integer(*i),
            PSV::Boolean(b) => mlua::Value::Boolean(*b),
            PSV::List(l) => {
                let table = lua.create_table()?;
                for (i, v) in l.iter().enumerate() {
                    table.set(i, v)?;
                }
                mlua::Value::Table(table)
            }
            PSV::Map(map) => {
                let table = lua.create_table()?;
                for (k, v) in map {
                    table.set(&**k, v)?;
                }
                mlua::Value::Table(table)
            }
            PSV::Null => todo!(),
        })
    }
}

impl Index<&str> for PluginSettingsValue {
    type Output = PluginSettingsValue;

    fn index(&self, index: &str) -> &Self::Output {
        self.as_map()
            .and_then(|map| map.get(index))
            .unwrap_or(&Self::Null)
    }
}
impl Index<usize> for PluginSettingsValue {
    type Output = PluginSettingsValue;

    fn index(&self, index: usize) -> &Self::Output {
        self.as_list().get(index).unwrap_or(&Self::Null)
    }
}

#[derive(Clone, Debug)]
pub struct PluginSettingsRoot {
    value: PluginSettingsValue,
    lua: OnceLock<mlua::Value>,
}

impl Deref for PluginSettingsRoot {
    type Target = PluginSettingsValue;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<T> Index<T> for PluginSettingsRoot
where
    PluginSettingsValue: Index<T>,
{
    type Output = <PluginSettingsValue as Index<T>>::Output;

    fn index(&self, index: T) -> &Self::Output {
        &self.value[index]
    }
}

impl PluginSettingsRoot {
    pub fn get_lua(&self, lua: &mlua::Lua) -> &mlua::Value {
        self.lua.get_or_init(|| match self.value.into_lua(lua) {
            Ok(v) => v,
            Err(e) => {
                log::error!("failed to turn plugin settings into lua value: {e}");
                mlua::Value::Nil
            }
        })
    }
}

impl<'de> Deserialize<'de> for PluginSettingsRoot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Deserialize::deserialize(deserializer).map(|value| Self {
            value,
            lua: OnceLock::new(),
        })
    }
}

impl Serialize for PluginSettingsRoot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

#[derive(Clone)]
pub enum PluginSettingsValue {
    String(String),
    Number(f64),
    Int(i64),
    Boolean(bool),
    List(Vec<PluginSettingsValue>),
    Map(BTreeMap<Box<str>, PluginSettingsValue>),
    Null,
}

#[derive(Debug)]
pub enum PluginSettings {
    Object {
        values: HashMap<Box<str>, PluginSettings>,
        label: Option<Box<str>>,
    },
    List {
        value_type: Box<PluginSettings>,
        max_entries: Option<usize>,
        label: Option<Box<str>>,
    },
    ParagraphInput {
        min: usize,
        max: Option<usize>,
        label: Option<Box<str>>,
        default: Box<str>,
    },
    StringInput {
        min: usize,
        max: Option<usize>,
        label: Option<Box<str>>,
        default: Box<str>,
    },
    Checkbox {
        label: Option<Box<str>>,
        default: bool,
    },
    Toggle {
        label: Option<Box<str>>,
        default: bool,
    },
    // PickList [[why are iced names this cursed lmao]]
    Dropdown {
        values: Vec<Box<str>>,
        label: Option<Box<str>>,
        default: usize,
    },
    // PickList [[why are iced names this cursed lmao]]
    SearchableDropdown {
        values: Vec<Box<str>>,
        label: Option<Box<str>>,
        default: usize,
    },
    IntSlider {
        min: i64,
        max: i64,
        step: i64,
        default: i64,
        label: Option<Box<str>>,
    },
    IntInput {
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
        default: i64,
        label: Option<Box<str>>,
    },
    Slider {
        min: f64,
        max: f64,
        step: Option<f64>,
        default: f64,
        label: Option<Box<str>>,
    },
    NumInput {
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        default: f64,
        label: Option<Box<str>>,
    },
}

impl<'de> Deserialize<'de> for PluginSettingsHolder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            settings: Arc::new(RwLock::new(map)),
        })
    }
}

impl Serialize for PluginSettingsHolder {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.settings.blocking_read().serialize(serializer)
    }
}

pub struct PluginSettingsHolderRef<'a> {
    settings: RwLockReadGuard<'a, HashMap<Box<str>, PluginSettingsRoot>>,
}

impl PluginSettingsHolderRef<'_> {
    pub fn get_lua(&self, id: &str, lua: &mlua::Lua) -> Option<mlua::Value> {
        Some(self.settings.get(id)?.get_lua(lua).clone())
    }
    pub fn get_root(&self, id: &str) -> Option<&PluginSettingsRoot> {
        self.settings.get(id)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Default)]
pub struct PluginSettingsHolder {
    settings: Arc<RwLock<HashMap<Box<str>, PluginSettingsRoot>>>,
}

impl PluginSettingsHolder {
    pub async fn as_ref_async(&self) -> PluginSettingsHolderRef<'_> {
        PluginSettingsHolderRef {
            settings: self.settings.read().await,
        }
    }
    pub fn as_ref(&self) -> PluginSettingsHolderRef<'_> {
        PluginSettingsHolderRef {
            settings: self.settings.blocking_read(),
        }
    }
    pub fn set(&self, plugin: &str, value: PluginSettingsValue) {
        if let Some(plugin) = self.settings.blocking_write().get_mut(plugin) {
            plugin.value = value;
            plugin.lua = OnceLock::new();
        }
    }
    /// applys default, returning if the config is malformed.
    pub fn apply_defaults(&self, plugin: &str, scheme: &PluginSettings) -> bool {
        let mut reader = self.settings.blocking_write();
        let value = reader.get_mut(plugin);
        match value {
            None => {
                reader.insert(
                    plugin.into(),
                    PluginSettingsRoot {
                        value: Self::default(scheme),
                        lua: OnceLock::new(),
                    },
                );
                false
            }
            Some(value) => match Self::apply_default(scheme, &mut value.value) {
                DefaultApplyResult::NoChanges => false,
                DefaultApplyResult::Changes => {
                    value.lua = OnceLock::new();
                    false
                }
                DefaultApplyResult::Error => true,
            },
        }
    }

    fn apply_default(
        scheme: &PluginSettings,
        value: &mut PluginSettingsValue,
    ) -> DefaultApplyResult {
        use PluginSettings as PS;
        use PluginSettingsValue as PSV;

        let mut result = DefaultApplyResult::NoChanges;
        match (scheme, value) {
            (PS::Object { values, .. }, PSV::Map(map)) => {
                for (k, scheme) in values {
                    let value = map.get_mut(k);
                    if let Some(v) = value {
                        result |= Self::apply_default(scheme, v);
                    } else {
                        result |= DefaultApplyResult::Changes;
                        map.insert(k.clone(), Self::default(scheme));
                    }
                }
            }
            (
                PS::List {
                    value_type,
                    max_entries,
                    ..
                },
                PSV::List(list),
            ) => {
                list.iter_mut()
                    .for_each(|v| result |= Self::apply_default(value_type, v));
                if let Some(len) = max_entries
                    && list.len() > *len
                {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::ParagraphInput { min, max, .. }, PSV::String(s)) => {
                if s.len() < *min {
                    return DefaultApplyResult::Error;
                }
                if let Some(max) = max
                    && s.len() > *max
                {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::StringInput { min, max, .. }, PSV::String(s)) => {
                if s.len() < *min {
                    return DefaultApplyResult::Error;
                }
                if let Some(max) = max
                    && s.len() > *max
                {
                    return DefaultApplyResult::Error;
                }
                if s.contains('\n') {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::Checkbox { .. } | PS::Toggle { .. }, PSV::Boolean(_)) => (),
            (
                PS::Dropdown { values, .. } | PS::SearchableDropdown { values, .. },
                PSV::String(s),
            ) if !values.iter().any(|v| **v == *s) => return DefaultApplyResult::Error,
            (PS::IntSlider { min, max, step, .. }, PSV::Int(i)) => {
                if *i < *min {
                    return DefaultApplyResult::Error;
                }
                if *i > *max {
                    return DefaultApplyResult::Error;
                }
                if *i % *step != 0 {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::IntInput { min, max, step, .. }, PSV::Int(i)) => {
                if let Some(min) = min
                    && *i < *min
                {
                    return DefaultApplyResult::Error;
                }
                if let Some(max) = max
                    && *i > *max
                {
                    return DefaultApplyResult::Error;
                }
                if *i % *step != 0 {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::Slider { min, max, step, .. }, PSV::Number(n)) => {
                if *n < *min {
                    return DefaultApplyResult::Error;
                }
                if *n > *max {
                    return DefaultApplyResult::Error;
                }
                if let Some(step) = step
                    && *n % *step != 0.0
                {
                    return DefaultApplyResult::Error;
                }
            }
            (PS::NumInput { min, max, step, .. }, PSV::Number(n)) => {
                if let Some(min) = min
                    && *n < *min
                {
                    return DefaultApplyResult::Error;
                }
                if let Some(max) = max
                    && *n > *max
                {
                    return DefaultApplyResult::Error;
                }
                if let Some(step) = step
                    && *n % *step != 0.0
                {
                    return DefaultApplyResult::Error;
                }
            }
            _ => return DefaultApplyResult::Error,
        }
        result
    }

    fn default(scheme: &PluginSettings) -> PluginSettingsValue {
        use PluginSettings as E;
        match scheme {
            E::Object { values, .. } => {
                let mut map = BTreeMap::new();
                for (k, v) in values {
                    map.insert(k.clone(), Self::default(v));
                }
                PluginSettingsValue::Map(map)
            }
            E::List { .. } => PluginSettingsValue::List(Vec::new()),
            E::ParagraphInput { default, .. } | E::StringInput { default, .. } => {
                PluginSettingsValue::String(default.to_string())
            }
            E::Checkbox { default, .. } | E::Toggle { default, .. } => {
                PluginSettingsValue::Boolean(*default)
            }
            E::Dropdown {
                values, default, ..
            }
            | E::SearchableDropdown {
                values, default, ..
            } => PluginSettingsValue::String(values[*default].to_string()),
            E::IntSlider { default, .. } | E::IntInput { default, .. } => {
                PluginSettingsValue::Int(*default)
            }
            E::Slider { default, .. } | E::NumInput { default, .. } => {
                PluginSettingsValue::Number(*default)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DefaultApplyResult {
    NoChanges = 0,
    Changes = 1,
    Error = 2,
}

impl BitOr for DefaultApplyResult {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        if self as u8 > rhs as u8 { self } else { rhs }
    }
}

impl BitOrAssign for DefaultApplyResult {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}
