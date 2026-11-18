use std::sync::Arc;

use crate::arena::Handle;

use super::object::Object;

pub type ObjectId = Handle<Object>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Arc<str>),
    Object(ObjectId),
}

impl Value {
    #[must_use]
    pub fn str(text: &str) -> Self {
        Self::String(Arc::from(text))
    }

    #[must_use]
    pub fn as_object(&self) -> Option<ObjectId> {
        match self {
            Self::Object(id) => Some(*id),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_number(&self) -> f64 {
        match self {
            Self::Undefined | Self::Object(_) => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(b) => f64::from(u8::from(*b)),
            Self::Number(n) => *n,
            Self::String(s) => parse_number(s),
        }
    }

    #[must_use]
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::String(s) => {
                let n = parse_number(s);
                n != 0.0 && !n.is_nan()
            }
            Self::Object(_) => true,
        }
    }

    #[must_use]
    pub fn to_i32(&self) -> i32 {
        to_i32(self.to_number())
    }

    #[must_use]
    pub fn primitive_string(&self) -> Option<Arc<str>> {
        Some(match self {
            Self::Undefined => Arc::from("undefined"),
            Self::Null => Arc::from("null"),
            Self::Bool(b) => Arc::from(if *b { "true" } else { "false" }),
            Self::Number(n) => Arc::from(format_number(*n)),
            Self::String(s) => Arc::clone(s),
            Self::Object(_) => return None,
        })
    }
}

#[must_use]
pub(crate) fn to_i32(n: f64) -> i32 {
    if !n.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let wide = n.trunc() as i64;
    #[allow(clippy::cast_possible_truncation)]
    let wrapped = wide as i32;
    wrapped
}

#[must_use]
pub(crate) fn parse_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return i64::from_str_radix(hex, 16).map_or(f64::NAN, |v| {
            #[allow(clippy::cast_precision_loss)]
            let f = v as f64;
            f
        });
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

#[must_use]
pub(crate) fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_owned();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if n == n.trunc() && n.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        let i = n as i64;
        return i.to_string();
    }
    format!("{n}")
}

#[must_use]
pub(crate) fn loose_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
        (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Object(_), _) | (_, Value::Object(_)) => false,
        _ => {
            let (x, y) = (a.to_number(), b.to_number());
            !x.is_nan() && !y.is_nan() && x == y
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swf6_truthiness_converts_strings_to_numbers() {
        assert!(!Value::str("abc").to_bool(), "NaN is false at SWF 6");
        assert!(!Value::str("0").to_bool());
        assert!(Value::str("2").to_bool());
        assert!(!Value::str("").to_bool());
    }

    #[test]
    fn numbers_print_without_a_trailing_point_and_int32_wraps() {
        assert_eq!(format_number(3.0), "3");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(to_i32(4_294_967_297.0), 1);
        assert_eq!(parse_number("0x10"), 16.0);
        assert!(Value::Undefined.to_number().is_nan());
    }
}
