use mint_movement::{CellRect, MoverClass, Vec2};
use mint_sim::{Component, ComponentId, PlayerId, StateHasher, World};

use super::{Ids, RotwkRules};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveBorder(pub(crate) u16);

impl Component for ActiveBorder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.0);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HumanBarred {
    pub(crate) cells: Vec<u8>,
    pub(crate) applied: Vec<(String, bool)>,
}

impl Component for HumanBarred {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.applied.len());
        for (name, barred) in &self.applied {
            hasher.write_usize(name.len());
            hasher.write_bytes(name.as_bytes());
            hasher.write_bool(*barred);
        }
    }
}

pub(crate) fn barred_of(world: &World, id: ComponentId<HumanBarred>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[], |barred| barred.cells.as_slice())
}

pub(crate) fn active_index(world: &World, ids: Ids) -> usize {
    crate::clock_entity(world, ids)
        .and_then(|clock| world.get(ids.border, clock))
        .map_or(0, |border| usize::from(border.0))
}

#[derive(Clone, Copy)]
pub(crate) struct Restriction<'a> {
    rules: &'a RotwkRules,
    confine: Option<CellRect>,
}

impl Restriction<'_> {
    pub(crate) fn apply(self, class: MoverClass, owner: Option<PlayerId>) -> MoverClass {
        if owner.is_some_and(|owner| self.rules.is_human(owner)) {
            let flier = class.blocked_by == mint_sage_passability::BOUNDARY;
            let barred = if flier {
                mint_movement::CellFlags::NONE
            } else {
                mint_sage_passability::IMPASSABLE_TO_PLAYERS
            };
            class.restricted(barred, self.confine)
        } else {
            class
        }
    }
}

impl RotwkRules {
    pub(crate) fn active_high(&self, world: &World, ids: Ids) -> Vec2 {
        self.passability
            .boundary_high(active_index(world, ids))
            .unwrap_or(self.passability.playable_high)
    }

    pub(crate) fn restriction(&self, world: &World, ids: Ids) -> Restriction<'_> {
        let index = active_index(world, ids);
        let narrower = self
            .passability
            .boundary_high(index)
            .is_some_and(|high| high != self.passability.playable_high);
        Restriction {
            rules: self,
            confine: narrower
                .then(|| self.passability.boundary_rect(index))
                .flatten(),
        }
    }
}
