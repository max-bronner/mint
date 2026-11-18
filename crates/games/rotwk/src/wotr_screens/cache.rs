use std::collections::BTreeMap;

use crate::wotr_movies::Kind;

#[derive(Debug, Default)]
pub(super) struct StateCache {
    last: BTreeMap<(Kind, String, String), String>,
}

impl StateCache {
    pub(super) fn seed(&mut self, kind: Kind, path: &str, function: &str, state: &str) {
        self.last
            .insert(key(kind, path, function), state.to_owned());
    }

    pub(super) fn changed(&mut self, kind: Kind, path: &str, function: &str, state: &str) -> bool {
        let slot = self.last.entry(key(kind, path, function)).or_default();
        if slot == state {
            return false;
        }
        state.clone_into(slot);
        true
    }

    pub(super) fn forget(&mut self, kind: Kind, path: &str, function: &str) {
        self.last.remove(&key(kind, path, function));
    }

    pub(super) fn clear_under(&mut self, kind: Kind, prefix: &str) {
        self.last
            .retain(|(k, path, _), _| *k != kind || !path.starts_with(prefix));
    }

    pub(super) fn clear(&mut self, kind: Kind) {
        self.last.retain(|(k, _, _), _| *k != kind);
    }
}

fn key(kind: Kind, path: &str, function: &str) -> (Kind, String, String) {
    (kind, path.to_owned(), function.to_owned())
}
