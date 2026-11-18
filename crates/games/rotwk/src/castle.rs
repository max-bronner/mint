use std::collections::BTreeMap;

use mint_movement::{Facing, Vec2};
use mint_sage_map::BaseLayout;
use mint_sage_modules::Castle;
use mint_sim::{Component, ComponentId, Entity, StateHasher, UpgradeMask, World};

use crate::combat::Doomed;
use crate::templates::KindTable;
use crate::unit::Kind;

pub(crate) type CastleLayouts = BTreeMap<String, BaseLayout>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Unpacked {
    pub(crate) kind: Kind,
    pub(crate) at: Vec2,
    pub(crate) facing: Facing,
    pub(crate) owned: bool,
}

pub(crate) fn layout_for<'a>(
    layouts: &'a CastleLayouts,
    castle: &'a Castle,
    side: &str,
) -> Option<(&'a str, &'a BaseLayout)> {
    let name = castle.for_side(side)?;
    layouts
        .get(&name.to_ascii_lowercase())
        .map(|layout| (name, layout))
}

pub(crate) fn unpack_plan(
    kinds: &KindTable,
    castle: &Castle,
    layout: &BaseLayout,
    origin: Vec2,
    facing: Facing,
) -> Vec<Unpacked> {
    layout
        .entries
        .iter()
        .filter_map(|entry| {
            let kind = kinds.kind(&entry.template_name)?;
            let owned = kinds
                .get(kind)
                .is_some_and(|found| castle.keeps(&found.record.kind_of));
            Some(Unpacked {
                kind,
                at: crate::templates::rotate(origin, facing, entry.offset),
                facing: Facing::from_radians(facing.radians() + entry.angle),
                owned,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CastleMember {
    pub(crate) castle: Entity,
    pub(crate) castle_kind: Kind,
}

impl Component for CastleMember {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.castle.hash_into(hasher);
        self.castle_kind.hash_into(hasher);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CastleUpgrades(pub(crate) UpgradeMask);

impl Component for CastleUpgrades {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PadOccupant(pub(crate) Entity);

impl Component for PadOccupant {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

pub(crate) fn pad_is_occupied(
    world: &World,
    occupants: ComponentId<PadOccupant>,
    doomed: ComponentId<Doomed>,
    pad: Entity,
) -> bool {
    world
        .get(occupants, pad)
        .is_some_and(|occupant| world.is_live(occupant.0) && !world.contains(doomed, occupant.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_is_free_once_its_occupant_is_doomed_or_gone() {
        let mut world = World::new();
        let occupants = world.register::<PadOccupant>();
        let doomed = world.register::<Doomed>();
        let pad = world.spawn().unwrap();
        let tower = world.spawn().unwrap();
        assert!(!pad_is_occupied(&world, occupants, doomed, pad));
        world.insert(occupants, pad, PadOccupant(tower)).unwrap();
        assert!(pad_is_occupied(&world, occupants, doomed, pad));
        world
            .insert(
                doomed,
                tower,
                Doomed {
                    destroy_at: mint_sim::Frame::FOREVER,
                    death_index: 0,
                    death_conditions: mint_sage_scene::Conditions::NONE,
                },
            )
            .unwrap();
        assert!(!pad_is_occupied(&world, occupants, doomed, pad));
        world.remove(doomed, tower);
        assert!(pad_is_occupied(&world, occupants, doomed, pad));
        world.despawn(tower);
        assert!(!pad_is_occupied(&world, occupants, doomed, pad));
    }
}
