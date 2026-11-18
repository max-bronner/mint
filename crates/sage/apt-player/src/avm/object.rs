use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_apt::Block;

use crate::instance::InstanceId;
use crate::library::MovieId;

use super::heap::ScopeId;
use super::natives::NativeFn;
use super::value::Value;

#[derive(Debug, Clone)]
pub(crate) struct Prop {
    pub(crate) name: Arc<str>,
    pub(crate) value: Value,
    pub(crate) dont_enum: bool,
}

const INDEX_ABOVE: usize = 12;

#[derive(Debug, Clone, Default)]
pub(crate) struct PropertyMap {
    entries: Vec<Prop>,
    index: BTreeMap<u64, Vec<u32>>,
}

fn fold_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b.to_ascii_lowercase());
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl PropertyMap {
    fn position(&self, name: &str) -> Option<usize> {
        if self.entries.len() > INDEX_ABOVE {
            let bucket = self.index.get(&fold_hash(name))?;
            bucket.iter().map(|i| *i as usize).find(|i| {
                self.entries
                    .get(*i)
                    .is_some_and(|p| p.name.eq_ignore_ascii_case(name))
            })
        } else {
            self.entries
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(name))
        }
    }

    fn reindex(&mut self) {
        self.index.clear();
        if self.entries.len() <= INDEX_ABOVE {
            return;
        }
        for (i, p) in self.entries.iter().enumerate() {
            if let Ok(i) = u32::try_from(i) {
                self.index.entry(fold_hash(&p.name)).or_default().push(i);
            }
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.position(name)
            .and_then(|i| self.entries.get(i))
            .map(|p| &p.value)
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    pub(crate) fn set(&mut self, name: &str, value: Value) {
        self.define(name, value, false);
    }

    pub(crate) fn define(&mut self, name: &str, value: Value, dont_enum: bool) {
        if let Some(entry) = self.position(name).and_then(|i| self.entries.get_mut(i)) {
            entry.value = value;
            return;
        }
        self.entries.push(Prop {
            name: Arc::from(name),
            value,
            dont_enum,
        });
        if self.entries.len() == INDEX_ABOVE.saturating_add(1) {
            self.reindex();
        } else if self.entries.len() > INDEX_ABOVE {
            let i = self.entries.len().saturating_sub(1);
            if let Ok(i) = u32::try_from(i) {
                self.index.entry(fold_hash(name)).or_default().push(i);
            }
        }
    }

    pub(crate) fn remove(&mut self, name: &str) -> bool {
        let Some(i) = self.position(name) else {
            return false;
        };
        self.entries.remove(i);
        self.reindex();
        true
    }

    pub(crate) fn enumerable_names(&self) -> impl Iterator<Item = &Arc<str>> {
        self.entries
            .iter()
            .filter(|p| !p.dont_enum)
            .map(|p| &p.name)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().map(|p| &p.value)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BytecodeFn {
    pub(crate) block: Arc<Block>,
    pub(crate) index: usize,
    pub(crate) movie: MovieId,
    pub(crate) scope: ScopeId,
    pub(crate) pool: Arc<[Value]>,
    pub(crate) base_clip: Option<InstanceId>,
}

#[derive(Debug, Clone)]
pub(crate) enum Native {
    Plain,
    Array,
    Function(Box<BytecodeFn>),
    Builtin(NativeFn),
    Display(InstanceId),
}

#[derive(Debug, Clone)]
pub struct Object {
    pub(crate) props: PropertyMap,
    pub(crate) proto: Option<super::value::ObjectId>,
    pub(crate) native: Native,
}

impl Object {
    pub(crate) fn new(proto: Option<super::value::ObjectId>, native: Native) -> Self {
        Self {
            props: PropertyMap::default(),
            proto,
            native,
        }
    }

    pub(crate) fn display(&self) -> Option<InstanceId> {
        match self.native {
            Native::Display(id) => Some(id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_ignores_ascii_case_and_keeps_the_first_spelling() {
        let mut map = PropertyMap::default();
        map.set("OpenButton", Value::Number(1.0));
        assert_eq!(map.get("openbutton"), Some(&Value::Number(1.0)));
        map.set("OPENBUTTON", Value::Number(2.0));
        assert_eq!(map.len(), 1);
        let names: Vec<_> = map.enumerable_names().map(|n| n.to_string()).collect();
        assert_eq!(names, vec!["OpenButton".to_owned()]);
    }

    #[test]
    fn a_large_map_indexes_and_still_finds_every_name_after_a_removal() {
        let mut map = PropertyMap::default();
        for i in 0..40 {
            map.set(&format!("Name{i}"), Value::Number(f64::from(i)));
        }
        assert!(map.remove("name7"));
        assert_eq!(map.get("NAME39"), Some(&Value::Number(39.0)));
        assert_eq!(map.get("name7"), None);
        assert_eq!(map.len(), 39);
    }
}
