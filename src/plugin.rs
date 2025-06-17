use std::fmt::Display;
use std::ops::{Bound, Deref, Range, RangeBounds};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum StringLike {
    Static(&'static str),
    StaticPath(&'static Path),
    Owned(String),
    SharedStr(Arc<str>, Range<u16>),
    SharedPath(Arc<Path>, Range<u16>),
    Empty,
}

impl PartialEq for StringLike {
    fn eq(&self, other: &Self) -> bool {
        self.to_str().eq(other.to_str())
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
