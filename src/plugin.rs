use std::fmt::{Debug, Display};
use std::hash::{Hash, Hasher};
use std::ops::{Bound, Deref, Range, RangeBounds};
use std::path::Path;
use std::sync::Arc;

use iced::Task;
use iced::futures::future::BoxFuture;
use rusqlite::ToSql;

use crate::config::PluginSettings;
use crate::filter_service::ResultBuilderRef;
use crate::matcher::MatcherInput;
use crate::{Action, Message, PluginContext, ResultBuilder};

#[derive(Clone, Debug, Eq)]
pub enum StringLike {
    Static(&'static str),
    StaticPath(&'static Path),
    Owned(String),
    SharedStr(Arc<str>, Range<u16>),
    SharedPath(Arc<Path>, Range<u16>),
    Empty,
}

impl ToSql for StringLike {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.to_str().to_sql()
    }
}

impl Hash for StringLike {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_str().hash(state);
    }
}

impl PartialEq for StringLike {
    fn eq(&self, other: &Self) -> bool {
        self.to_str().eq(other.to_str())
    }
}

impl PartialEq<StringLike> for String {
    fn eq(&self, other: &StringLike) -> bool {
        other.eq(self)
    }
}
impl PartialEq<String> for StringLike {
    fn eq(&self, other: &String) -> bool {
        self.to_str() == other
    }
}
impl PartialEq<StringLike> for &str {
    fn eq(&self, other: &StringLike) -> bool {
        other.eq(self)
    }
}
impl<'a> PartialEq<&'a str> for StringLike {
    fn eq(&self, other: &&'a str) -> bool {
        self.to_str() == *other
    }
}

impl Display for StringLike {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_str())
    }
}

impl StringLike {
    pub fn is_empty(&self) -> bool {
        match self {
            StringLike::Static(v) => v.is_empty(),
            StringLike::StaticPath(path) => path.as_os_str().is_empty(),
            StringLike::Owned(v) => v.is_empty(),
            StringLike::SharedStr(v, range) => v.len() <= range.start as usize,
            StringLike::SharedPath(path, range) => path.as_os_str().len() <= range.start as usize,
            StringLike::Empty => true,
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            StringLike::Static(v) => v,
            StringLike::Owned(v) => v,
            StringLike::StaticPath(path) => path.to_str().unwrap_or_default(),
            StringLike::SharedStr(s, range) => &s[range.start as usize..=range.end as usize],
            StringLike::SharedPath(path, range) => path
                .to_str()
                .map(|v| &v[range.start as usize..=range.end as usize])
                .unwrap_or_default(),
            StringLike::Empty => "",
        }
    }

    pub fn substr(&mut self, range: impl RangeBounds<u16>) {
        if matches!(self, StringLike::Empty) {
            return;
        }
        let new_range = Range {
            start: match range.start_bound() {
                Bound::Included(v) => *v,
                Bound::Excluded(v) => *v + 1,
                Bound::Unbounded => 0,
            },
            end: match range.end_bound() {
                Bound::Included(v) => *v,
                Bound::Excluded(v) => *v - 1,
                Bound::Unbounded => u16::MAX,
            },
        };
        match self {
            StringLike::Static(v) => {
                if new_range.start as usize >= v.len() {
                    *self = Self::Empty;
                } else {
                    *v = &v[new_range.start as usize..=(new_range.end as usize).min(v.len() - 1)];
                }
            }
            StringLike::StaticPath(path) => {
                let s = if let Some(s) = path.to_str() {
                    &s[(new_range.start as usize).min(s.len() - 1)
                        ..=(new_range.end as usize).min(s.len() - 1)]
                } else {
                    ""
                };
                *path = Path::new(s);
                if s.is_empty() {
                    *self = Self::Empty;
                }
            }
            StringLike::Owned(s) => {
                if new_range.start as usize >= s.len() {
                    s.clear();
                } else {
                    s.drain(..new_range.start as usize);
                    s.drain((new_range.end as usize + 1).min(s.len() - 1)..);
                }
            }
            StringLike::SharedStr(v, range) => {
                range.start = range
                    .start
                    .saturating_add(new_range.start)
                    .min(v.len() as u16);
                range.end = range
                    .end
                    .saturating_add(new_range.end)
                    .min((v.len() - 1) as u16);
                range.start = range.start.saturating_add(new_range.start);
                if range.start == range.end {
                    *self = StringLike::Empty;
                }
            }
            StringLike::SharedPath(v, range) => {
                let len = (v.as_os_str().len() - 1) as u16;
                range.start = range.start.saturating_add(new_range.start).min(len);
                range.end = range.end.saturating_add(new_range.end).min(len);
                if range.start == range.end {
                    *self = StringLike::Empty;
                }
            }
            StringLike::Empty => {}
        }
    }

    fn correct(mut self) -> Self {
        let me = match self {
            StringLike::SharedStr(v, range) if v.len() <= range.start as usize => Self::Empty,
            StringLike::SharedStr(ref v, ref mut range) => {
                if range.end as usize >= v.len() {
                    range.end = (v.len() - 1) as u16;
                }
                self
            }
            StringLike::SharedPath(ref path, ref mut range) => {
                let Some(s) = path.to_str() else {
                    return Self::Empty;
                };
                if range.start as usize >= s.len() {
                    return Self::Empty;
                }
                if range.end as usize >= s.len() {
                    range.end = (s.len() - 1) as u16;
                }
                self
            }
            _ => self,
        };
        if me.to_str().is_empty() {
            Self::Empty
        } else {
            me
        }
    }
}

impl Deref for StringLike {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.to_str()
    }
}

impl From<&'static str> for StringLike {
    fn from(value: &'static str) -> Self {
        Self::Static(value).correct()
    }
}

impl From<&'static Path> for StringLike {
    fn from(value: &'static Path) -> Self {
        Self::StaticPath(value).correct()
    }
}

impl From<Arc<str>> for StringLike {
    fn from(value: std::sync::Arc<str>) -> Self {
        Self::SharedStr(value, 0..u16::MAX).correct()
    }
}

impl From<Arc<Path>> for StringLike {
    fn from(value: Arc<Path>) -> Self {
        Self::SharedPath(value, 0..u16::MAX).correct()
    }
}

impl From<String> for StringLike {
    fn from(value: String) -> Self {
        Self::Owned(value).correct()
    }
}

impl From<StringLike> for String {
    fn from(value: StringLike) -> Self {
        match value {
            StringLike::Owned(s) => s,
            StringLike::Empty => String::new(),
            v => v.to_string(),
        }
    }
}

impl<T> From<Option<T>> for StringLike
where
    StringLike: From<T>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            None => Self::Empty,
            Some(v) => Self::from(v),
        }
    }
}

pub trait CustomDataCompatible: std::any::Any + Send + Sync + 'static {
    fn clone_custom_data(&self) -> Box<dyn CustomDataCompatible>;
}
impl<T: std::any::Any + Clone + Send + Sync> CustomDataCompatible for T {
    fn clone_custom_data(&self) -> Box<dyn CustomDataCompatible> {
        Box::new(self.clone())
    }
}

pub trait Plugin: Send + Sync {
    fn actions(&self) -> &[Action] {
        const { &[Action::default("Default Action", "")] }
    }
    fn prefix(&self) -> &str;
    fn get_for_values_arc(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send {
        async move { self.get_for_values(&input, builder, context).await }
    }
    fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send;
    fn init(&mut self, context: PluginContext) -> impl Future<Output = ()> + Send;
    #[allow(unused_variables)]
    fn handle_pre(&self, thing: CustomData, action: &str, context: PluginContext) -> Task<Message> {
        Task::none()
    }
    #[allow(unused_variables)]
    fn handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message> {
        Task::none()
    }
}

pub struct Entry {
    pub name: StringLike,
    pub subtitle: StringLike,
    pub perfect_match: bool,
    pub data: CustomData,
}
impl Entry {
    pub fn new(
        name: impl Into<StringLike>,
        subtitle: impl Into<StringLike>,
        data: CustomData,
    ) -> Self {
        Self {
            name: name.into(),
            subtitle: subtitle.into(),
            data,
            perfect_match: false,
        }
    }

    /// this function pins this entry to the top of the list.
    ///
    /// Effectively this is the same as [`Entry::perfect`] called with true
    #[must_use]
    pub fn pin(mut self) -> Self {
        // this effectively pins it to the top
        self.perfect_match = true;
        self
    }
    #[must_use]
    pub fn perfect(mut self, perfect: bool) -> Self {
        self.perfect_match = perfect;
        self
    }
}

pub trait InstancePlugin: Plugin + Clone + 'static {
    /// This function will only ever be called once.
    fn config(&mut self) -> Option<PluginSettings>;
}
impl<T: StructPlugin> Plugin for T {
    fn prefix(&self) -> &str {
        Self::prefix()
    }

    fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send {
        StructPlugin::get_for_values(self, input, builder, context)
    }

    fn init(&mut self, context: PluginContext) -> impl Future<Output = ()> + Send {
        StructPlugin::init(self, context)
    }

    fn actions(&self) -> &[Action] {
        StructPlugin::actions(self)
    }

    fn get_for_values_arc(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send {
        StructPlugin::get_for_values_arc(self, input, builder, context)
    }

    fn handle_pre(&self, thing: CustomData, action: &str, context: PluginContext) -> Task<Message> {
        StructPlugin::handle_pre(self, thing, action, context)
    }

    fn handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message> {
        StructPlugin::handle_post(self, thing, action, context)
    }
}
pub trait StructPlugin: Send + Sync + Default + 'static {
    fn prefix() -> &'static str;
    fn config() -> Option<PluginSettings> {
        None
    }

    fn actions(&self) -> &[Action] {
        const { &[Action::default("Default Action", "")] }
    }
    fn get_for_values_arc(
        &self,
        input: Arc<MatcherInput>,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send {
        async move { self.get_for_values(&input, builder, context).await }
    }
    fn get_for_values(
        &self,
        input: &MatcherInput,
        builder: ResultBuilderRef<'_>,
        context: PluginContext,
    ) -> impl Future<Output = ()> + Send;
    fn init(&mut self, context: PluginContext) -> impl Future<Output = ()> + Send;
    #[allow(unused_variables)]
    fn handle_pre(&self, thing: CustomData, action: &str, context: PluginContext) -> Task<Message> {
        Task::none()
    }
    #[allow(unused_variables)]
    fn handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message> {
        Task::none()
    }
}

pub trait AnyPlugin: Send + Sync {
    fn as_any_ref(&self) -> &dyn std::any::Any;
    fn any_actions(&self) -> &[Action];
    fn any_prefix(&self) -> &str;
    fn any_get_for_values<'fut>(
        &'fut self,
        input: Arc<MatcherInput>,
        builder: &'fut ResultBuilder,
        plugin_id: usize,
        context: PluginContext<'fut>,
    ) -> BoxFuture<'fut, ()>;
    fn any_init<'a>(&'a mut self, context: PluginContext<'a>) -> BoxFuture<'a, ()>;
    fn any_handle_pre(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message>;
    fn any_handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message>;
}
impl<T: Plugin + 'static> AnyPlugin for T {
    fn as_any_ref(&self) -> &dyn std::any::Any {
        self
    }

    fn any_actions(&self) -> &[Action] {
        self.actions()
    }

    fn any_prefix(&self) -> &str {
        self.prefix()
    }

    fn any_get_for_values<'fut>(
        &'fut self,
        input: Arc<MatcherInput>,
        builder: &'fut ResultBuilder,
        plugin_id: usize,
        context: PluginContext<'fut>,
    ) -> BoxFuture<'fut, ()> {
        let builder = ResultBuilderRef::create(plugin_id, builder);
        Box::pin(self.get_for_values_arc(input, builder, context))
    }

    fn any_init<'a>(&'a mut self, context: PluginContext<'a>) -> BoxFuture<'a, ()> {
        Box::pin(self.init(context))
    }

    fn any_handle_pre(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message> {
        self.handle_pre(thing, action, context)
    }
    fn any_handle_post(
        &self,
        thing: CustomData,
        action: &str,
        context: PluginContext,
    ) -> Task<Message> {
        self.handle_post(thing, action, context)
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

#[derive(Clone)]
pub struct CustomData(Box<dyn CustomDataCompatible>);

impl CustomData {
    pub fn new<T: CustomDataCompatible>(value: T) -> Self {
        Self(Box::new(value))
    }

    /// # Panics
    ///
    /// Panics when T is not the same value as the one stored in this [`CustomData`]
    #[must_use]
    pub fn into<T: CustomDataCompatible>(self) -> T {
        *(self.0 as Box<dyn std::any::Any>)
            .downcast()
            .expect("this should never fail")
    }
}

#[derive(Debug, Clone)]
pub struct GenericEntry {
    pub(crate) name: StringLike,
    pub(crate) subtitle: StringLike,
    /// the plugin index into the state
    pub(crate) plugin: usize,
    pub(crate) data: CustomData,
    pub(crate) perfect_match: bool,
}

impl GenericEntry {
    pub fn new(
        name: impl Into<StringLike>,
        subtitle: impl Into<StringLike>,
        plugin: usize,
        data: CustomData,
    ) -> Self {
        Self {
            name: name.into(),
            subtitle: subtitle.into(),
            plugin,
            data,
            perfect_match: false,
        }
    }

    #[must_use]
    pub fn perfect(mut self, perfect: bool) -> Self {
        self.perfect_match = perfect;
        self
    }
}
