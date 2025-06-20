#[derive(Debug)]
pub struct MatcherInput {
    split_words: Vec<String>,
    input: String,
    has_prefix: bool,
}

fn is_terminator(c: char) -> bool {
    matches!(
        c,
        '!' | '#'
            | '*'
            | '('
            | ')'
            | '_'
            | '+'
            | '='
            | '/'
            | '?'
            | '.'
            | ','
            | '<'
            | '>'
            | ';'
            | ':'
            | '\''
            | '"'
            | '['
            | ']'
            | '{'
            | '}'
            | '-'
            | ' '
    )
}

macro_rules! _try {
    ($expr:expr) => {
        match $expr {
            Some(v) => v,
            _ => return false,
        }
    };
}

impl MatcherInput {
    pub fn new(s: String, has_prefix: bool) -> Self {
        if s.is_empty() {
            return Self {
                split_words: Vec::new(),
                input: s,
                has_prefix,
            };
        }
        Self {
            split_words: s
                .split_terminator(is_terminator)
                .map(|v| v.trim_matches(is_terminator))
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .collect(),
            input: s,
            has_prefix,
        }
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn has_prefix(&self) -> bool {
        self.has_prefix
    }

    pub fn matches(&self, pattern: &str) -> bool {
        matches_words(pattern, &self.split_words).is_matching()
    }

    pub fn matches_perfect(&self, pattern: &str) -> Option<bool> {
        let res = matches_words(pattern, &self.split_words);
        res.is_matching()
            .then_some(matches!(res, MatchResult::PerfectMatch))
    }

    pub fn words(&self) -> &[String] {
        &self.split_words
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchResult {
    DoesNotMatch,
    Matches,
    PerfectMatch,
}

impl MatchResult {
    pub fn is_matching(self) -> bool {
        matches!(self, Self::PerfectMatch | Self::Matches)
    }

    pub fn from_match(perfect: bool) -> Self {
        if perfect {
            Self::PerfectMatch
        } else {
            Self::Matches
        }
    }
    pub fn new(matches: bool, perfect: bool) -> Self {
        if matches {
            Self::from_match(perfect)
        } else {
            Self::DoesNotMatch
        }
    }
}

fn matches_words(pattern: &str, mut words: &[impl AsRef<str>]) -> MatchResult {
    if words.is_empty() {
        return MatchResult::from_match(pattern.trim().is_empty());
    }
    let mut current_str: &str = words[0].as_ref();
    let mut last_current_str = current_str;
    words = &words[1..];
    let mut last_terminator = true;
    let last_i_perfect_val = pattern.len().saturating_sub(1);

    let mut perfect = true;
    for (i, c) in pattern.char_indices() {
        if current_str.is_empty() {
            if is_terminator(c) {
                if last_terminator {
                    continue;
                }
                if words.is_empty() {
                    return MatchResult::new(
                        current_str.is_empty(),
                        i == last_i_perfect_val && perfect,
                    );
                }
                last_terminator = true;
                current_str = words[0].as_ref();
                last_current_str = current_str;
                words = &words[1..];
            } else {
                perfect = false;
            }
        } else if is_terminator(c) {
            current_str = last_current_str;
        } else {
            last_terminator = false;
            let next_char = current_str.chars().next();
            if let Some(next) = next_char {
                if c.to_ascii_lowercase() == next {
                    current_str = &current_str[next.len_utf8()..];
                } else {
                    perfect = false;
                    current_str = last_current_str;
                }
            } else {
                perfect = false;
            }
        }
    }

    MatchResult::new(words.is_empty() && current_str.is_empty(), perfect)
}

#[cfg(test)]
mod test {
    use crate::matcher::{MatchResult, matches_words};

    #[test]
    fn test() {
        assert_eq!(
            MatchResult::Matches,
            matches_words("luma-dev", &["lum", "dev"])
        );
        assert_eq!(
            MatchResult::DoesNotMatch,
            matches_words("luma-dev", &["lu", "ma", "dev"])
        );
        assert_eq!(
            MatchResult::PerfectMatch,
            matches_words("luma-dev", &["luma", "dev"])
        );
        assert_eq!(
            MatchResult::Matches,
            matches_words("convert_plugin.rs", &["plugin", "rs"])
        );
        assert_eq!(
            MatchResult::Matches,
            matches_words("convert_plugin.rs", &["rs"])
        );
        assert_eq!(MatchResult::Matches, matches_words("quit", &["qu"]));
        assert_eq!(MatchResult::PerfectMatch, matches_words("quit", &["quit"]));
        assert_eq!(
            MatchResult::DoesNotMatch,
            matches_words("quit", &["qu", "t"])
        );
        assert_eq!(MatchResult::DoesNotMatch, matches_words("quit", &["qut"]));
    }
}
