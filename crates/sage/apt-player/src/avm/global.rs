use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::object::PropertyMap;
use super::value::Value;

#[derive(Default)]
struct Inner {
    members: Mutex<PropertyMap>,
    version: AtomicU64,
}

#[derive(Clone, Default)]
pub struct SharedGlobal(Arc<Inner>);

impl std::fmt::Debug for SharedGlobal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedGlobal")
            .field("version", &self.0.version.load(Ordering::Acquire))
            .finish()
    }
}

impl SharedGlobal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<Value> {
        self.0.members.lock().ok()?.get(name).cloned()
    }

    pub(crate) fn version(&self) -> u64 {
        self.0.version.load(Ordering::Acquire)
    }

    pub(crate) fn set(&self, name: &str, value: Value) -> Option<(u64, u64)> {
        let mut members = self.0.members.lock().ok()?;
        members.set(name, value);
        let before = self.0.version.fetch_add(1, Ordering::AcqRel);
        Some((before.wrapping_add(1), before))
    }

    pub(crate) fn snapshot(&self) -> Option<(u64, Vec<(Arc<str>, Value)>)> {
        let members = self.0.members.lock().ok()?;
        let version = self.0.version.load(Ordering::Acquire);
        let all = members
            .enumerable_names()
            .filter_map(|name| Some((Arc::clone(name), members.get(name)?.clone())))
            .collect();
        Some((version, all))
    }
}
