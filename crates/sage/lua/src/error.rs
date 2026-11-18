use alloc::string::String;
use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub kind: ParseErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Unsupported(&'static str),
    Expected {
        expected: &'static str,
        found: String,
    },
    UnexpectedEnd(&'static str),
    UnterminatedString,
    UnknownEscape(String),
    MalformedNumber(String),
    SourceTooLong {
        bytes: usize,
        limit: usize,
    },
    TooManyTokens {
        limit: usize,
    },
    TooDeep {
        limit: u32,
    },
    TooManyArguments {
        limit: usize,
    },
    DuplicateFunction(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: ", self.line)?;
        match &self.kind {
            ParseErrorKind::Unsupported(what) => {
                write!(f, "{what} is outside the supported subset")
            }
            ParseErrorKind::Expected { expected, found } => {
                write!(f, "expected {expected}, found `{found}`")
            }
            ParseErrorKind::UnexpectedEnd(what) => write!(f, "source ended inside {what}"),
            ParseErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            ParseErrorKind::UnknownEscape(seq) => write!(f, "unknown string escape `{seq}`"),
            ParseErrorKind::MalformedNumber(text) => write!(f, "malformed number `{text}`"),
            ParseErrorKind::SourceTooLong { bytes, limit } => {
                write!(f, "source is {bytes} bytes, limit {limit}")
            }
            ParseErrorKind::TooManyTokens { limit } => write!(f, "more than {limit} tokens"),
            ParseErrorKind::TooDeep { limit } => write!(f, "nested deeper than {limit}"),
            ParseErrorKind::TooManyArguments { limit } => write!(f, "more than {limit} arguments"),
            ParseErrorKind::DuplicateFunction(name) => {
                write!(f, "function `{name}` is declared twice")
            }
        }
    }
}

impl core::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError {
    pub message: String,
}

impl HostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl core::error::Error for HostError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub line: u32,
    pub kind: EvalErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalErrorKind {
    Host {
        name: String,
        error: HostError,
    },
    UnknownVariable(String),
    TypeMismatch {
        op: &'static str,
        found: &'static str,
    },
    IncomparableTypes {
        left: &'static str,
        right: &'static str,
    },
    StringTooLong {
        limit: usize,
    },
    TooManyVariables {
        limit: usize,
    },
    CallTooDeep {
        limit: u32,
    },
    WrongArity {
        name: String,
        expected: usize,
        found: usize,
    },
    BudgetExhausted,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: ", self.line)?;
        match &self.kind {
            EvalErrorKind::Host { name, error } => write!(f, "call to `{name}` failed: {error}"),
            EvalErrorKind::UnknownVariable(name) => write!(f, "`{name}` was never assigned"),
            EvalErrorKind::TypeMismatch { op, found } => {
                write!(f, "`{op}` cannot take a {found}")
            }
            EvalErrorKind::IncomparableTypes { left, right } => {
                write!(f, "cannot order a {left} against a {right}")
            }
            EvalErrorKind::StringTooLong { limit } => {
                write!(f, "concatenated string longer than {limit} bytes")
            }
            EvalErrorKind::TooManyVariables { limit } => write!(f, "more than {limit} variables"),
            EvalErrorKind::CallTooDeep { limit } => write!(f, "calls nested deeper than {limit}"),
            EvalErrorKind::WrongArity {
                name,
                expected,
                found,
            } => write!(f, "`{name}` takes {expected} arguments, given {found}"),
            EvalErrorKind::BudgetExhausted => f.write_str("instruction budget exhausted"),
        }
    }
}

impl core::error::Error for EvalError {}
