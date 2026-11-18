use std::collections::BTreeSet;

use mint_sim::{Entity, StateHasher, World};

use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct StandGround(BTreeSet<Entity>);

impl StandGround {
    pub(super) fn set(&mut self, entity: Entity, on: bool) {
        if on {
            self.0.insert(entity);
        } else {
            self.0.remove(&entity);
        }
    }

    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.0.contains(&entity)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for entity in &self.0 {
            entity.hash_into(hasher);
        }
    }
}

impl RotwkRules {
    pub(crate) fn stands_ground(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .is_some_and(|(_, state)| state.rt.stand_ground.0.contains(&entity))
    }
}
