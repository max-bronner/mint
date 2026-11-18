use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use crate::error::{EvalError, EvalErrorKind};
use crate::limits::MAX_VARIABLES;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f32),
    Str(Arc<str>),
    Handle(u64),
}

impl Value {
    pub fn string(text: impl AsRef<str>) -> Self {
        Value::Str(Arc::from(text.as_ref()))
    }

    #[must_use]
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::Str(_) => "string",
            Value::Handle(_) => "object",
        }
    }

    #[must_use]
    pub fn as_number(&self) -> Option<f32> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Nil => f.write_str("nil"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Str(s) => f.write_str(s),
            Value::Handle(bits) => write!(f, "object:{bits:#x}"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Env {
    slots: Vec<(Arc<str>, Value)>,
}

impl Env {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), EvalError> {
        self.set_at(name, value, 0)
    }

    pub(crate) fn set_at(&mut self, name: &str, value: Value, line: u32) -> Result<(), EvalError> {
        match self.slots.binary_search_by(|(key, _)| (**key).cmp(name)) {
            Ok(at) => {
                if let Some(slot) = self.slots.get_mut(at) {
                    slot.1 = value;
                }
                Ok(())
            }
            Err(at) => {
                if self.slots.len() >= MAX_VARIABLES {
                    return Err(EvalError {
                        line,
                        kind: EvalErrorKind::TooManyVariables {
                            limit: MAX_VARIABLES,
                        },
                    });
                }
                self.slots.insert(at, (Arc::from(name), value));
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.slots
            .binary_search_by(|(key, _)| (**key).cmp(name))
            .ok()
            .and_then(|at| self.slots.get(at).map(|(_, value)| value))
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.slots.iter().map(|(name, value)| (&**name, value))
    }
}
