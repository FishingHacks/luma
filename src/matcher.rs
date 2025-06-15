#[derive(Debug)]
pub enum MatcherInput<'a> {
    Words(Vec<&'a str>),
}

#[inline(always)]
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

impl<'a> MatcherInput<'a> {
    pub fn new(s: &'a str) -> Self {
        let s = s.trim();
        if s.is_empty() {
            return Self::Words(Vec::new());
        }
        Self::Words(
            s.split_terminator(is_terminator)
                .map(|v| v.trim_matches(is_terminator))
                .filter(|v| !v.is_empty())
                .collect(),
        )
    }

    pub fn matches(&self, pattern: &str) -> bool {
        match self {
            MatcherInput::Words(words) => matches_words(pattern, words),
        }
    }

    pub fn words(&self) -> &[&'a str] {
        match self {
            MatcherInput::Words(items) => items,
        }
    }
}

fn matches_words(pattern: &str, mut words: &[&str]) -> bool {
    if words.is_empty() {
        return true;
    }
    let mut current_str = words[0];
    words = &words[1..];
    let mut last_terminator = true;

    for c in pattern.chars() {
        if current_str.is_empty() {
            if is_terminator(c) {
                if last_terminator {
                    continue;
                }
                if words.is_empty() {
                    return current_str.is_empty();
                }
                last_terminator = true;
                current_str = words[0];
                words = &words[1..];
            }
        } else if is_terminator(c) {
            return false;
        } else {
            last_terminator = false;
            let next_char = current_str.chars().next();
            if let Some(next) = next_char {
                if c.to_ascii_lowercase() != next {
                    return false;
                }
                current_str = &current_str[next.len_utf8()..];
            }
        }
    }

    words.is_empty() && current_str.is_empty()
}

#[cfg(test)]
mod test {
    use crate::matcher::matches_words;

    #[test]
    fn test() {
        assert!(matches_words("anathema-tb", &["ana", "tb"]));
        assert!(!matches_words("anathema-tb", &["ana", "ma", "tb"]));
        assert!(matches_words("quit", &["qu"]));
        assert!(!matches_words("quit", &["qu", "t"]));
        assert!(!matches_words("quit", &["qut"]));
    }
}
