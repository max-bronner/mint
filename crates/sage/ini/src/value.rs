use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::tree::{Block, Define, Field};

pub const NONE_VALUE: &str = "NONE";

#[must_use]
pub fn is_none(token: &str) -> bool {
    token.eq_ignore_ascii_case(NONE_VALUE)
}

#[must_use]
pub fn parse_bool(token: &str) -> Option<bool> {
    let token = token.trim();
    if token.eq_ignore_ascii_case("Yes") || token == "1" {
        Some(true)
    } else if token.eq_ignore_ascii_case("No") || token == "0" {
        Some(false)
    } else {
        None
    }
}

#[must_use]
pub fn parse_int(token: &str) -> Option<i64> {
    let text = token.trim_start();
    longest_prefix(
        text,
        |c| c.is_ascii_digit() || c == '+' || c == '-',
        |t| t.parse::<i64>().ok(),
    )
}

#[must_use]
pub fn parse_float(token: &str) -> Option<f32> {
    let text = token.trim_start();
    longest_prefix(
        text,
        |c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'),
        |t| t.parse::<f32>().ok().filter(|v| v.is_finite()),
    )
}

fn longest_prefix<T>(
    text: &str,
    charset: impl Fn(char) -> bool,
    scan: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    let end = text.find(|c: char| !charset(c)).unwrap_or(text.len());
    let mut candidate = text.get(..end)?;
    while !candidate.is_empty() {
        if let Some(value) = scan(candidate) {
            return Some(value);
        }
        candidate = candidate.get(..candidate.len().saturating_sub(1))?;
    }
    None
}

fn scan_number(token: &str) -> Option<f32> {
    let text = token.trim();
    parse_float(text.strip_suffix('%').unwrap_or(text))
}

#[must_use]
pub fn parse_percent(token: &str) -> Option<f32> {
    scan_number(token).map(|v| v / 100.0)
}

#[must_use]
pub fn split_tag(token: &str) -> Option<(&str, &str)> {
    token.split_once(':')
}

#[must_use]
pub fn tagged_value<'a>(args: &'a [String], tag: &str) -> Option<&'a str> {
    for (i, token) in args.iter().enumerate() {
        let Some((found, value)) = split_tag(token) else {
            continue;
        };
        if !found.eq_ignore_ascii_case(tag) {
            continue;
        }
        if !value.is_empty() {
            return Some(value);
        }
        return args.get(i.saturating_add(1)).map(String::as_str);
    }
    None
}

#[must_use]
pub fn identifier_list(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitEdit<'a> {
    Clear,
    Set(&'a str),
    Unset(&'a str),
}

pub fn bit_string_edits(args: &[String]) -> Result<Vec<BitEdit<'_>>, &str> {
    let mut edits = Vec::with_capacity(args.len().saturating_add(1));
    let mut bare = false;
    let mut signed = false;
    for token in args {
        if is_none(token) {
            if bare || signed {
                return Err(token);
            }
            edits.push(BitEdit::Clear);
            break;
        }
        if let Some(name) = token.strip_prefix('+') {
            if bare {
                return Err(token);
            }
            signed = true;
            edits.push(BitEdit::Set(name));
        } else if let Some(name) = token.strip_prefix('-') {
            if bare {
                return Err(token);
            }
            signed = true;
            edits.push(BitEdit::Unset(name));
        } else {
            if signed {
                return Err(token);
            }
            if !bare {
                edits.push(BitEdit::Clear);
                bare = true;
            }
            edits.push(BitEdit::Set(token));
        }
    }
    Ok(edits)
}

pub const BIT_STRING_EXPECTED: &str = "bare names, or +/- edits, not both on one line";

#[must_use]
pub fn is_unexpanded_macro_call(token: &str) -> bool {
    token.starts_with('#')
}

#[derive(Debug, Clone, Default)]
pub struct MacroTable {
    numbers: BTreeMap<String, f32>,
    lists: BTreeMap<String, Vec<String>>,
    seen: BTreeSet<String>,
    diagnostics: Vec<MacroDiagnostic>,
    percent_bodied: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MacroDiagnostic {
    #[error("{file}:{line}: macro name `{name}` is not all upper case")]
    NotUpperCase {
        name: String,
        file: Arc<str>,
        line: u32,
    },
    #[error("{file}:{line}: macro name `{name}` is defined more than once")]
    Duplicate {
        name: String,
        file: Arc<str>,
        line: u32,
    },
    #[error("{file}:{line}: macro `{name}` has no value")]
    NoValue {
        name: String,
        file: Arc<str>,
        line: u32,
    },
}

impl MacroTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_defines(&mut self, defines: &[Define]) {
        for define in defines {
            if define.name != define.name.to_ascii_uppercase() {
                self.diagnostics.push(MacroDiagnostic::NotUpperCase {
                    name: define.name.clone(),
                    file: Arc::clone(&define.file),
                    line: define.line,
                });
            }
            let key = define.name.to_ascii_uppercase();
            if !self.seen.insert(key.clone()) {
                self.diagnostics.push(MacroDiagnostic::Duplicate {
                    name: define.name.clone(),
                    file: Arc::clone(&define.file),
                    line: define.line,
                });
            }
            let tokens: Vec<&str> = define.body.split_whitespace().collect();
            let numeric = match tokens.as_slice() {
                [only] => scan_number(only).map(|v| (v, only.trim().ends_with('%'))),
                _ => None,
            };
            match numeric {
                Some((value, had_percent)) => {
                    self.lists.remove(&key);
                    self.numbers.insert(key.clone(), value);
                    if had_percent {
                        self.percent_bodied.insert(key);
                    } else {
                        self.percent_bodied.remove(&key);
                    }
                }
                None if !tokens.is_empty() => {
                    self.numbers.remove(&key);
                    self.percent_bodied.remove(&key);
                    self.lists
                        .insert(key, tokens.into_iter().map(str::to_owned).collect());
                }
                None => self.diagnostics.push(MacroDiagnostic::NoValue {
                    name: define.name.clone(),
                    file: Arc::clone(&define.file),
                    line: define.line,
                }),
            }
        }
    }

    #[must_use]
    pub fn tokens(&self, name: &str) -> Option<&[String]> {
        self.lists.get(key(name).as_ref()).map(Vec::as_slice)
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<f32> {
        self.numbers.get(key(name).as_ref()).copied()
    }

    #[must_use]
    pub fn is_percent_bodied(&self, name: &str) -> bool {
        self.percent_bodied.contains(key(name).as_ref())
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[MacroDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.numbers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty()
    }
}

fn key(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_lowercase()) {
        Cow::Owned(name.to_ascii_uppercase())
    } else {
        Cow::Borrowed(name)
    }
}

const MAX_MACRO_EXPANSION_PASSES: usize = 8;

const MAX_EXPANDED_TOKENS: usize = 4096;

pub fn expand_token_list_macros(args: &mut Vec<String>, macros: &MacroTable) {
    if macros.lists.is_empty() || !args.iter().any(|token| macros.tokens(token).is_some()) {
        return;
    }
    for _ in 0..MAX_MACRO_EXPANSION_PASSES {
        let mut changed = false;
        let mut expanded = Vec::with_capacity(args.len());
        for token in args.drain(..) {
            match macros.tokens(&token) {
                Some(list) if expanded.len().saturating_add(list.len()) <= MAX_EXPANDED_TOKENS => {
                    expanded.extend(list.iter().cloned());
                    changed = true;
                }
                _ => expanded.push(token),
            }
        }
        *args = expanded;
        if !changed {
            break;
        }
    }
}

#[must_use]
pub fn parse_float_or_macro(token: &str, macros: &MacroTable) -> Option<f32> {
    macros.get(token).or_else(|| parse_float(token))
}

#[must_use]
pub fn parse_float_or_expression(token: &str, macros: &MacroTable) -> Option<f32> {
    if let Some(value) = parse_float_or_macro(token, macros) {
        return Some(value);
    }
    let call = token.trim().strip_prefix('#')?;
    let (name, rest) = call.split_once('(')?;
    let inner = rest.trim_end().strip_suffix(')')?;
    let mut operands = inner.split_whitespace();
    let (Some(a), Some(b), None) = (operands.next(), operands.next(), operands.next()) else {
        return None;
    };
    let a = parse_float_or_macro(a, macros)?;
    let b = parse_float_or_macro(b, macros)?;
    let value = match name.trim().to_ascii_uppercase().as_str() {
        "ADD" => a + b,
        "SUBTRACT" => a - b,
        "MULTIPLY" => a * b,
        "DIVIDE" if b != 0.0 => a / b,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

#[must_use]
pub fn field_expression(field: &Field, macros: &MacroTable) -> Option<f32> {
    let first = field.value()?;
    if first.starts_with('#') {
        parse_float_or_expression(&field.args.join(" "), macros)
    } else {
        parse_float_or_macro(first, macros)
    }
}

#[must_use]
pub fn resolve_percent(args: &[String], macros: &MacroTable) -> Option<f32> {
    let first = args.first()?;
    let raw = if first.starts_with('#') {
        parse_float_or_expression(&args.join(" "), macros)
    } else {
        scan_number(first).or_else(|| macros.get(first.as_str()))
    };
    raw.map(|v| v / 100.0)
}

#[must_use]
pub fn parse_percent_shaped(token: &str, macros: &MacroTable) -> Option<f32> {
    match token.strip_suffix('%') {
        Some(text) => parse_float(text).map(|v| v / 100.0),
        None => match parse_float(token) {
            Some(v) => Some(v),
            None => macros.get(token).map(|v| {
                if macros.is_percent_bodied(token) {
                    v / 100.0
                } else {
                    v
                }
            }),
        },
    }
}

pub fn keyed_pairs<'a>(
    fields: impl Iterator<Item = &'a Field>,
    keyword: &str,
) -> Vec<(String, Vec<String>)> {
    fields
        .filter(|f| f.keyword.eq(keyword))
        .filter_map(|f| {
            let (bucket, rest) = f.args.split_first()?;
            Some((bucket.clone(), rest.to_vec()))
        })
        .collect()
}

#[must_use]
pub fn field_value<'a>(block: &'a Block, keyword: &str) -> Option<&'a str> {
    block.field(keyword).and_then(Field::value)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Duration {
    Frames(f32),
    Milliseconds(f32),
    Seconds(f32),
    Unknown(f32),
}

impl Duration {
    #[must_use]
    pub fn raw(self) -> f32 {
        match self {
            Duration::Frames(v)
            | Duration::Milliseconds(v)
            | Duration::Seconds(v)
            | Duration::Unknown(v) => v,
        }
    }

    #[must_use]
    pub fn parse_seconds_field(token: &str) -> Option<Duration> {
        parse_float(token).map(|seconds| Duration::Milliseconds(seconds * 1000.0))
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
