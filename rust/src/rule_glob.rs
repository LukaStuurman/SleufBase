//! Unicode fnmatch tokens. Class/range normalization follows Python 3.11.
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};
#[derive(Debug)]
enum Token {
    Star,
    Any,
    Literal(char),
    Class {
        negated: bool,
        ranges: Vec<(char, char)>,
    },
}
impl Token {
    fn matches(&self, ch: char) -> bool {
        match self {
            Self::Any => true,
            Self::Literal(c) => *c == ch,
            Self::Class { negated, ranges } => {
                ranges.iter().any(|(a, b)| *a <= ch && ch <= *b) != *negated
            }
            Self::Star => false,
        }
    }
}
#[derive(Debug)]
struct Glob {
    tokens: Vec<Token>,
}
impl Glob {
    fn compile(pattern: &str) -> Self {
        let chars: Vec<_> = crate::python_values::uppercase(pattern).chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            i += 1;
            match ch {
                '*' => {
                    if !matches!(tokens.last(), Some(Token::Star)) {
                        tokens.push(Token::Star);
                    }
                }
                '?' => tokens.push(Token::Any),
                '[' => {
                    let start = i;
                    let mut end = i;
                    if chars.get(end) == Some(&'!') {
                        end += 1;
                    }
                    if chars.get(end) == Some(&']') {
                        end += 1;
                    }
                    while end < chars.len() && chars[end] != ']' {
                        end += 1;
                    }
                    if end == chars.len() {
                        tokens.push(Token::Literal('['));
                        continue;
                    }
                    // Python normalizes ranges before interpreting a leading
                    // '!'. Removing an invalid range can expose a new '!'.
                    let body = &chars[start..end];
                    let mut chunks: Vec<Vec<char>> = Vec::new();
                    let mut cursor = 0;
                    let mut k = if body.first() == Some(&'!') { 2 } else { 1 };
                    while k < body.len() {
                        let Some(offset) = body[k..].iter().position(|c| *c == '-') else {
                            break;
                        };
                        k += offset;
                        chunks.push(body[cursor..k].to_vec());
                        cursor = k + 1;
                        k += 3;
                    }
                    let tail = body[cursor..].to_vec();
                    if !tail.is_empty() {
                        chunks.push(tail);
                    } else if let Some(last) = chunks.last_mut() {
                        last.push('-');
                    }
                    for k in (1..chunks.len()).rev() {
                        if chunks[k - 1].last() > chunks[k].first() {
                            chunks[k - 1].pop();
                            let rest = chunks.remove(k);
                            chunks[k - 1].extend_from_slice(&rest[1..]);
                        }
                    }
                    let mut set = Vec::new();
                    for (part, chunk) in chunks.iter().enumerate() {
                        if part > 0 {
                            set.push(('-', true));
                        }
                        set.extend(chunk.iter().map(|c| (*c, false)));
                    }
                    let negated = set.first() == Some(&('!', false));
                    let mut ranges = Vec::new();
                    let mut at = usize::from(negated);
                    while at < set.len() {
                        if at + 2 < set.len() && set[at + 1] == ('-', true) {
                            ranges.push((set[at].0, set[at + 2].0));
                            at += 3;
                        } else {
                            ranges.push((set[at].0, set[at].0));
                            at += 1;
                        }
                    }
                    tokens.push(Token::Class { negated, ranges });
                    i = end + 1;
                }
                c => tokens.push(Token::Literal(c)),
            }
        }
        Self { tokens }
    }
    fn matches(&self, text: &str) -> bool {
        let (mut p, mut at) = (0, 0);
        let mut star = None;
        let mut retry = 0;
        while at < text.len() {
            let ch = text[at..].chars().next().unwrap();
            if self.tokens.get(p).is_some_and(|token| token.matches(ch)) {
                p += 1;
                at += ch.len_utf8();
            } else if matches!(self.tokens.get(p), Some(Token::Star)) {
                star = Some(p);
                retry = at;
                p += 1;
            } else if let Some(previous) = star {
                let Some(ch) = text[retry..].chars().next() else {
                    return false;
                };
                retry += ch.len_utf8();
                at = retry;
                p = previous + 1;
            } else {
                return false;
            }
        }
        while matches!(self.tokens.get(p), Some(Token::Star)) {
            p += 1;
        }
        p == self.tokens.len()
    }
}
// Names never enter the cache. Large/extra patterns still work without retention.
static CACHE: LazyLock<Mutex<HashMap<String, Arc<Glob>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
pub(crate) fn matches(pattern: &str, uppercase_name: &str) -> bool {
    let cached = CACHE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(pattern)
        .cloned();
    let compiled = cached.unwrap_or_else(|| {
        let compiled = Arc::new(Glob::compile(pattern));
        if pattern.len() <= 1024 {
            let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
            if cache.len() < 64 {
                cache.insert(pattern.into(), compiled.clone());
            }
        }
        compiled
    });
    compiled.matches(uppercase_name)
}
