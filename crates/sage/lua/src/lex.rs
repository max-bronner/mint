use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::error::{ParseError, ParseErrorKind};
use crate::limits::{MAX_SOURCE_BYTES, MAX_TOKENS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comments {
    HostFile,
    LuaFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kw {
    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    False,
    For,
    Function,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,
}

impl Kw {
    fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "and" => Kw::And,
            "break" => Kw::Break,
            "do" => Kw::Do,
            "else" => Kw::Else,
            "elseif" => Kw::Elseif,
            "end" => Kw::End,
            "false" => Kw::False,
            "for" => Kw::For,
            "function" => Kw::Function,
            "if" => Kw::If,
            "in" => Kw::In,
            "local" => Kw::Local,
            "nil" => Kw::Nil,
            "not" => Kw::Not,
            "or" => Kw::Or,
            "repeat" => Kw::Repeat,
            "return" => Kw::Return,
            "then" => Kw::Then,
            "true" => Kw::True,
            "until" => Kw::Until,
            "while" => Kw::While,
            _ => return None,
        })
    }

    pub(crate) fn word(self) -> &'static str {
        match self {
            Kw::And => "and",
            Kw::Break => "break",
            Kw::Do => "do",
            Kw::Else => "else",
            Kw::Elseif => "elseif",
            Kw::End => "end",
            Kw::False => "false",
            Kw::For => "for",
            Kw::Function => "function",
            Kw::If => "if",
            Kw::In => "in",
            Kw::Local => "local",
            Kw::Nil => "nil",
            Kw::Not => "not",
            Kw::Or => "or",
            Kw::Repeat => "repeat",
            Kw::Return => "return",
            Kw::Then => "then",
            Kw::True => "true",
            Kw::Until => "until",
            Kw::While => "while",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sym {
    LParen,
    RParen,
    Comma,
    Assign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Concat,
    Plus,
    Minus,
    Star,
    Slash,
    Semi,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Dot,
    Colon,
    Percent,
    Caret,
    Hash,
    Ellipsis,
}

impl Sym {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Sym::LParen => "(",
            Sym::RParen => ")",
            Sym::Comma => ",",
            Sym::Assign => "=",
            Sym::Eq => "==",
            Sym::Ne => "~=",
            Sym::Lt => "<",
            Sym::Le => "<=",
            Sym::Gt => ">",
            Sym::Ge => ">=",
            Sym::Concat => "..",
            Sym::Plus => "+",
            Sym::Minus => "-",
            Sym::Star => "*",
            Sym::Slash => "/",
            Sym::Semi => ";",
            Sym::LBrace => "{",
            Sym::RBrace => "}",
            Sym::LBracket => "[",
            Sym::RBracket => "]",
            Sym::Dot => ".",
            Sym::Colon => ":",
            Sym::Percent => "%",
            Sym::Caret => "^",
            Sym::Hash => "#",
            Sym::Ellipsis => "...",
        }
    }

    pub(crate) fn unsupported(self) -> Option<&'static str> {
        Some(match self {
            Sym::LBrace | Sym::RBrace => "a table constructor",
            Sym::LBracket | Sym::RBracket => "an index expression `[]`",
            Sym::Dot => "field access `.`",
            Sym::Colon => "a method call `:`",
            Sym::Percent => "the modulo operator `%`",
            Sym::Caret => "the exponent operator `^`",
            Sym::Hash => "the length operator `#`",
            Sym::Ellipsis => "varargs `...`",
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Kind<'a> {
    Name(&'a str),
    Number(f32),
    Str(Arc<str>),
    Kw(Kw),
    Sym(Sym),
}

impl Kind<'_> {
    pub(crate) fn describe(&self) -> String {
        match self {
            Kind::Name(name) => (*name).to_string(),
            Kind::Number(n) => {
                let mut text = String::new();
                let _ = core::fmt::Write::write_fmt(&mut text, format_args!("{n}"));
                text
            }
            Kind::Str(_) => "a string".to_string(),
            Kind::Kw(kw) => kw.word().to_string(),
            Kind::Sym(sym) => sym.text().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token<'a> {
    pub(crate) kind: Kind<'a>,
    pub(crate) line: u32,
}

pub(crate) fn lex(source: &str, comments: Comments) -> Result<Vec<Token<'_>>, ParseError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ParseError {
            line: 1,
            kind: ParseErrorKind::SourceTooLong {
                bytes: source.len(),
                limit: MAX_SOURCE_BYTES,
            },
        });
    }

    let bytes = source.as_bytes();
    let mut out: Vec<Token<'_>> = Vec::new();
    let mut at = 0usize;
    let mut line = 1u32;

    while at < bytes.len() {
        let byte = byte_at(bytes, at);
        if byte == b'\n' {
            line = line.saturating_add(1);
            at = at.saturating_add(1);
            continue;
        }
        if byte.is_ascii_whitespace() {
            at = at.saturating_add(1);
            continue;
        }
        if starts_comment(bytes, at, comments) {
            if starts_with(bytes, at, b"--[[") || starts_with(bytes, at, b"--[=") {
                return Err(ParseError {
                    line,
                    kind: ParseErrorKind::Unsupported("a long comment `--[[ ]]`"),
                });
            }
            at = end_of_line(bytes, at);
            continue;
        }
        if byte == b'[' && (starts_with(bytes, at, b"[[") || starts_with(bytes, at, b"[=")) {
            return Err(ParseError {
                line,
                kind: ParseErrorKind::Unsupported("a long string `[[ ]]`"),
            });
        }

        let (kind, next) = if byte == b'"' || byte == b'\'' {
            let (text, next) = read_string(source, at, line)?;
            (Kind::Str(text), next)
        } else if byte.is_ascii_digit() || (byte == b'.' && peek_digit(bytes, at)) {
            read_number(source, at, line)?
        } else if byte == b'_' || byte.is_ascii_alphabetic() {
            let end = end_of_word(bytes, at);
            let word = slice(source, at, end);
            let kind = match Kw::from_word(word) {
                Some(kw) => Kind::Kw(kw),
                None => Kind::Name(word),
            };
            (kind, end)
        } else {
            let (sym, next) = read_symbol(bytes, at, line)?;
            (Kind::Sym(sym), next)
        };

        if out.len() >= MAX_TOKENS {
            return Err(ParseError {
                line,
                kind: ParseErrorKind::TooManyTokens { limit: MAX_TOKENS },
            });
        }
        out.push(Token { kind, line });
        at = next;
    }

    Ok(out)
}

fn byte_at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

fn slice(source: &str, from: usize, to: usize) -> &str {
    source.get(from..to).unwrap_or("")
}

fn starts_with(bytes: &[u8], at: usize, prefix: &[u8]) -> bool {
    bytes.get(at..).is_some_and(|rest| rest.starts_with(prefix))
}

fn starts_comment(bytes: &[u8], at: usize, comments: Comments) -> bool {
    if starts_with(bytes, at, b"--") {
        return true;
    }
    comments == Comments::HostFile && (byte_at(bytes, at) == b';' || starts_with(bytes, at, b"//"))
}

fn end_of_line(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while end < bytes.len() && byte_at(bytes, end) != b'\n' {
        end = end.saturating_add(1);
    }
    end
}

fn end_of_word(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while end < bytes.len() {
        let byte = byte_at(bytes, end);
        if byte == b'_' || byte.is_ascii_alphanumeric() {
            end = end.saturating_add(1);
        } else {
            break;
        }
    }
    end
}

fn peek_digit(bytes: &[u8], at: usize) -> bool {
    byte_at(bytes, at.saturating_add(1)).is_ascii_digit()
}

fn read_number(source: &str, at: usize, line: u32) -> Result<(Kind<'_>, usize), ParseError> {
    let bytes = source.as_bytes();
    if starts_with(bytes, at, b"0x") || starts_with(bytes, at, b"0X") {
        return Err(ParseError {
            line,
            kind: ParseErrorKind::Unsupported("a hexadecimal number"),
        });
    }
    let mut end = at;
    let mut seen_exponent = false;
    while end < bytes.len() {
        let byte = byte_at(bytes, end);
        if byte.is_ascii_digit() || byte == b'.' {
            end = end.saturating_add(1);
        } else if (byte == b'e' || byte == b'E') && !seen_exponent {
            seen_exponent = true;
            end = end.saturating_add(1);
            let next = byte_at(bytes, end);
            if next == b'+' || next == b'-' {
                end = end.saturating_add(1);
            }
        } else {
            break;
        }
    }
    let text = slice(source, at, end);
    match text.parse::<f32>() {
        Ok(number) => Ok((Kind::Number(number), end)),
        Err(_) => Err(ParseError {
            line,
            kind: ParseErrorKind::MalformedNumber(text.to_string()),
        }),
    }
}

fn read_string(source: &str, at: usize, line: u32) -> Result<(Arc<str>, usize), ParseError> {
    let bytes = source.as_bytes();
    let quote = byte_at(bytes, at);
    let mut cursor = at.saturating_add(1);
    let mut text = String::new();
    loop {
        if cursor >= bytes.len() {
            return Err(ParseError {
                line,
                kind: ParseErrorKind::UnterminatedString,
            });
        }
        let byte = byte_at(bytes, cursor);
        cursor = cursor.saturating_add(1);
        match byte {
            b'\n' => {
                return Err(ParseError {
                    line,
                    kind: ParseErrorKind::UnterminatedString,
                })
            }
            b'\\' => {
                let escape = byte_at(bytes, cursor);
                cursor = cursor.saturating_add(1);
                let decoded = match escape {
                    b'n' => '\n',
                    b't' => '\t',
                    b'r' => '\r',
                    b'a' => '\u{7}',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'v' => '\u{b}',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    _ => {
                        let mut seq = String::from("\\");
                        seq.push(char::from(escape));
                        return Err(ParseError {
                            line,
                            kind: ParseErrorKind::UnknownEscape(seq),
                        });
                    }
                };
                text.push(decoded);
            }
            _ if byte == quote => return Ok((Arc::from(text.as_str()), cursor)),
            _ => {
                let start = cursor.saturating_sub(1);
                let end = next_char_boundary(source, start);
                text.push_str(slice(source, start, end));
                cursor = end;
            }
        }
    }
}

fn next_char_boundary(source: &str, at: usize) -> usize {
    let mut end = at.saturating_add(1);
    while end < source.len() && !source.is_char_boundary(end) {
        end = end.saturating_add(1);
    }
    end
}

fn read_symbol(bytes: &[u8], at: usize, line: u32) -> Result<(Sym, usize), ParseError> {
    let two = [
        (&b"=="[..], Sym::Eq),
        (&b"~="[..], Sym::Ne),
        (&b"<="[..], Sym::Le),
        (&b">="[..], Sym::Ge),
        (&b"..."[..], Sym::Ellipsis),
        (&b".."[..], Sym::Concat),
    ];
    for (text, sym) in two {
        if starts_with(bytes, at, text) {
            return Ok((sym, at.saturating_add(text.len())));
        }
    }
    let sym = match byte_at(bytes, at) {
        b'(' => Sym::LParen,
        b')' => Sym::RParen,
        b',' => Sym::Comma,
        b'=' => Sym::Assign,
        b'<' => Sym::Lt,
        b'>' => Sym::Gt,
        b'+' => Sym::Plus,
        b'-' => Sym::Minus,
        b'*' => Sym::Star,
        b'/' => Sym::Slash,
        b';' => Sym::Semi,
        b'{' => Sym::LBrace,
        b'}' => Sym::RBrace,
        b'[' => Sym::LBracket,
        b']' => Sym::RBracket,
        b'.' => Sym::Dot,
        b':' => Sym::Colon,
        b'%' => Sym::Percent,
        b'^' => Sym::Caret,
        b'#' => Sym::Hash,
        other => {
            let mut found = String::new();
            found.push(char::from(other));
            return Err(ParseError {
                line,
                kind: ParseErrorKind::Expected {
                    expected: "a statement",
                    found,
                },
            });
        }
    };
    Ok((sym, at.saturating_add(1)))
}
