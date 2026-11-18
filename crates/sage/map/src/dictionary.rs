use crate::container::NameTable;
use crate::reader::Reader;
use crate::{Error, Result};

impl Value {
    fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Str(_) => "string",
            Self::WideStr(_) => "wide string",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i32),
    Float(f32),
    Str(String),
    WideStr(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dictionary(Vec<(String, Value)>);

impl Dictionary {
    #[must_use]
    pub fn from_pairs(pairs: Vec<(String, Value)>) -> Self {
        Self(pairs)
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn typed<T>(
        &self,
        chunk: &'static str,
        key: &str,
        expected: &'static str,
        pick: impl FnOnce(&Value) -> Option<T>,
    ) -> Result<Option<T>> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        pick(value).map(Some).ok_or_else(|| Error::WrongValueType {
            chunk,
            key: key.to_string(),
            expected,
            found: value.type_name(),
        })
    }

    pub(crate) fn get_bool(&self, chunk: &'static str, key: &str) -> Result<Option<bool>> {
        self.typed(chunk, key, "bool", |v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
    }

    pub(crate) fn get_int(&self, chunk: &'static str, key: &str) -> Result<Option<i32>> {
        self.typed(chunk, key, "int", |v| match v {
            Value::Int(i) => Some(*i),
            _ => None,
        })
    }

    pub(crate) fn get_float(&self, chunk: &'static str, key: &str) -> Result<Option<f32>> {
        self.typed(chunk, key, "float", |v| match v {
            Value::Float(f) => Some(*f),
            _ => None,
        })
    }

    pub(crate) fn get_str(&self, chunk: &'static str, key: &str) -> Result<Option<&str>> {
        match self.get(key) {
            None => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s.as_str())),
            Some(other) => Err(Error::WrongValueType {
                chunk,
                key: key.to_string(),
                expected: "string",
                found: other.type_name(),
            }),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub(crate) fn read_dictionary(r: &mut Reader<'_>, names: &NameTable) -> Result<Dictionary> {
    let count = r.u16()?;
    let mut pairs = Vec::new();
    for _ in 0..count {
        let offset = r.position();
        let packed = r.u32()?;
        let kind = u8::try_from(packed & 0xff).unwrap_or(0);
        let index = packed >> 8;
        let key = names.resolve(index, offset)?;
        let value = match kind {
            0 => Value::Bool(r.u8()? != 0),
            1 => Value::Int(r.i32()?),
            2 => Value::Float(r.f32()?),
            3 => Value::Str(r.ascii_string()?),
            4 => Value::WideStr(r.wide_string()?),
            _ => return Err(Error::UnknownValueType { offset, kind }),
        };
        pairs.push((key, value));
    }
    Ok(Dictionary(pairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::MapBuilder;

    #[test]
    fn round_trips_every_value_type() {
        let mut b = MapBuilder::new();
        let entries = vec![
            b.dict_bool("flag", true),
            b.dict_int("count", -7),
            b.dict_float("scale", 1.5),
            b.dict_str("name", "hi"),
            b.dict_wide("label", "hi"),
        ];
        let payload = b.dict(entries);
        let names = b.name_table();

        let mut r = Reader::new(&payload);
        let dict = read_dictionary(&mut r, &names).expect("valid dictionary");

        assert_eq!(dict.len(), 5);
        assert_eq!(dict.get("flag"), Some(&Value::Bool(true)));
        assert_eq!(dict.get("count"), Some(&Value::Int(-7)));
        assert_eq!(dict.get("scale"), Some(&Value::Float(1.5)));
        assert_eq!(dict.get("name"), Some(&Value::Str("hi".to_string())));
        assert_eq!(dict.get("label"), Some(&Value::WideStr("hi".to_string())));
        assert_eq!(dict.get("missing"), None);
    }

    #[test]
    fn unknown_value_type_is_an_error() {
        let mut b = MapBuilder::new();
        let index = b.intern("k");
        let mut payload = 1u16.to_le_bytes().to_vec();
        payload.extend_from_slice(&((index << 8) | 9).to_le_bytes());
        let names = b.name_table();
        let mut r = Reader::new(&payload);
        let err = read_dictionary(&mut r, &names).unwrap_err();
        assert!(matches!(err, Error::UnknownValueType { kind: 9, .. }));
    }
}
