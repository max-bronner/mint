use std::collections::BTreeMap;

use mint_sim::{Entity, StateHasher, Tick};

use super::MapScriptState;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Walk {
    label: String,
    walking: bool,
    done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Walks(BTreeMap<Entity, Walk>);

impl Walks {
    pub(super) fn begin(&mut self, entity: Entity, label: &str) {
        self.0.insert(
            entity,
            Walk {
                label: label.to_owned(),
                walking: false,
                done: false,
            },
        );
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.0.remove(&entity);
    }

    pub(super) fn finished(&self, entity: Entity, label: &str) -> bool {
        self.0
            .get(&entity)
            .is_some_and(|walk| walk.done && walk.label.eq_ignore_ascii_case(label))
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity, _| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (entity, walk) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_bytes(walk.label.as_bytes());
            hasher.write_bool(walk.walking);
            hasher.write_bool(walk.done);
        }
    }
}

impl RotwkRules {
    pub(super) fn step_walks(&self, tick: &Tick<'_>, ids: Ids, state: &mut MapScriptState) {
        let world = tick.state();
        for (&entity, walk) in &mut state.rt.walks.0 {
            if walk.done {
                continue;
            }
            if world.contains(ids.path, entity) {
                walk.walking = true;
            } else if walk.walking {
                walk.done = true;
            }
        }
    }
}
