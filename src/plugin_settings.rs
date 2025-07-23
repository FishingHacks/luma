use std::collections::{BTreeMap, HashMap};
use std::fmt::Debug;
use std::ops::{Deref, Index};
use std::sync::OnceLock;

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
                Ok(PSV::Object(btree))
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
            PSV::Object(map) => serializer.collect_map(map.iter()),
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
            PluginSettingsValue::Object(map) => f.debug_map().entries(map.iter()).finish(),
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
    pub fn as_map(&self) -> Option<&BTreeMap<String, Self>> {
        match self {
            Self::Object(map) => Some(map),
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
            PSV::Object(map) => {
                let table = lua.create_table()?;
                for (k, v) in map {
                    table.set(k.as_str(), v)?;
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
    Object(BTreeMap<String, PluginSettingsValue>),
    Null,
}

#[derive(Debug)]
pub enum PluginSettings {
    Object {
        values: HashMap<String, PluginSettings>,
        label: Option<String>,
    },
    List {
        value_type: Box<PluginSettings>,
        max_entries: Option<usize>,
        label: Option<String>,
    },
    ParagraphInput {
        min: usize,
        max: Option<usize>,
        label: Option<String>,
    },
    StringInput {
        min: usize,
        max: Option<usize>,
        label: Option<String>,
    },
    Checkbox {
        label: Option<String>,
    },
    Toggle {
        label: Option<String>,
    },
    // PickList [[why are iced names this cursed lmao]]
    Dropdown {
        values: Vec<String>,
        label: Option<String>,
    },
    // PickList [[why are iced names this cursed lmao]]
    SearchableDropdown {
        values: Vec<String>,
        label: Option<String>,
    },
    IntSlider {
        min: i64,
        max: i64,
        step: i64,
        label: Option<String>,
    },
    IntInput {
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
        label: Option<String>,
    },
    Slider {
        min: f64,
        max: f64,
        step: Option<f64>,
        label: Option<String>,
    },
    NumInput {
        min: Option<f64>,
        max: Option<f64>,
        step: Option<f64>,
        label: Option<String>,
    },
}
