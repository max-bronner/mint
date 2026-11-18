use std::collections::BTreeMap;

use mint_sage_asset::KindOf;
use mint_sage_shroud::{CellVisibility, ObjectVisibility};
use mint_sim::PlayerId;

use crate::render_state::UnitView;

pub(crate) fn drawn_visibility(
    units: &mut Vec<UnitView>,
    shroud_drawn: bool,
    playable: &dyn Fn(f32, f32) -> bool,
) {
    if shroud_drawn {
        units.retain(|unit| unit.visibility != ObjectVisibility::Shrouded);
    } else {
        units.retain(|unit| playable(unit.position.x, unit.position.y));
        for unit in units.iter_mut() {
            unit.visibility = ObjectVisibility::Clear;
        }
    }
}

fn rememberable(unit: &UnitView) -> bool {
    unit.kind_of.contains(KindOf::IMMOBILE)
}

#[derive(Debug, Default, Clone)]
pub(crate) struct GhostMemory {
    viewer: Option<PlayerId>,
    seen: BTreeMap<u32, UnitView>,
}

impl GhostMemory {
    pub(crate) fn len(&self) -> usize {
        self.seen.len()
    }

    pub(crate) fn apply(
        &mut self,
        units: &mut Vec<UnitView>,
        viewer: PlayerId,
        shroud_drawn: bool,
        cell_at: &dyn Fn(f32, f32) -> CellVisibility,
    ) -> usize {
        if !shroud_drawn || self.viewer != Some(viewer) {
            self.seen.clear();
            self.viewer = Some(viewer);
            if !shroud_drawn {
                return 0;
            }
        }
        let mut ghosts = 0usize;
        for unit in units.iter_mut() {
            if !rememberable(unit) {
                continue;
            }
            let index = unit.entity.index();
            match unit.visibility {
                ObjectVisibility::Clear | ObjectVisibility::PartialClear => {
                    self.seen.insert(index, *unit);
                }
                ObjectVisibility::Fogged => {
                    if let Some(snapshot) =
                        self.seen.get(&index).filter(|s| s.entity == unit.entity)
                    {
                        *unit = UnitView {
                            visibility: ObjectVisibility::Fogged,
                            ..*snapshot
                        };
                        ghosts = ghosts.saturating_add(1);
                    }
                }
                ObjectVisibility::Shrouded => {}
            }
        }
        let mut listed: Vec<mint_sim::Entity> = units.iter().map(|unit| unit.entity).collect();
        listed.sort_unstable_by_key(|entity| (entity.index(), entity.generation()));
        let mut dead: Vec<UnitView> = Vec::new();
        self.seen.retain(|_, snapshot| {
            let entity = snapshot.entity;
            if listed
                .binary_search_by_key(&(entity.index(), entity.generation()), |held| {
                    (held.index(), held.generation())
                })
                .is_ok()
            {
                return true;
            }
            if cell_at(snapshot.position.x, snapshot.position.y) == CellVisibility::Explored {
                dead.push(UnitView {
                    visibility: ObjectVisibility::Fogged,
                    selected: false,
                    ..*snapshot
                });
                true
            } else {
                false
            }
        });
        ghosts = ghosts.saturating_add(dead.len());
        units.extend(dead);
        ghosts
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn view(
        world: &mut mint_sim::World,
        kind_of: KindOf,
        visibility: ObjectVisibility,
    ) -> UnitView {
        UnitView {
            entity: world.spawn().unwrap(),
            position: mint_movement::Vec2::new(100.0, 200.0),
            ground: 0.0,
            facing: 0.0,
            radius: 20.0,
            height: 40.0,
            kind_of,
            targetable: false,
            pick_type: mint_sage_asset::PickType::Selectable,
            owner: PlayerId::new(1),
            selected: false,
            moving: false,
            health: Some(1.0),
            dying: false,
            created: 0,
            visibility,
            speed: 0.0,
            weapon_cycle: 0.0,
            attack_cycle: 0,
            attack_slot: None,
            kind: Some(3),
            art_kind: Some(3),
            told_kind: Some(3),
            hover_kind: Some(3),
            conditions: mint_sage_scene::Conditions::NONE,
            build_elapsed: None,
            build_total: None,
            under_repair: false,
            held: false,
        }
    }

    const STRUCTURE: KindOf = KindOf::STRUCTURE.union(KindOf::IMMOBILE);

    #[test]
    fn with_the_reveal_on_nothing_is_shaded_by_the_shroud() {
        let mut world = mint_sim::World::new();
        let mut units = vec![
            view(&mut world, KindOf::SELECTABLE, ObjectVisibility::Shrouded),
            view(&mut world, STRUCTURE, ObjectVisibility::Fogged),
            view(&mut world, KindOf::SELECTABLE, ObjectVisibility::Clear),
        ];
        drawn_visibility(&mut units, false, &|_, _| true);
        assert_eq!(
            units.len(),
            3,
            "the reveal drops nothing in the playable area"
        );
        assert!(units
            .iter()
            .all(|unit| unit.visibility == ObjectVisibility::Clear));
        assert!(units
            .iter()
            .all(|unit| !crate::game::shroud_masked(unit.visibility)));
    }

    #[test]
    fn with_the_shroud_drawn_the_shrouded_are_gone_and_the_fogged_stay_fogged() {
        let mut world = mint_sim::World::new();
        let mut units = vec![
            view(&mut world, KindOf::SELECTABLE, ObjectVisibility::Shrouded),
            view(&mut world, STRUCTURE, ObjectVisibility::Fogged),
        ];
        drawn_visibility(&mut units, true, &|_, _| true);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].visibility, ObjectVisibility::Fogged);
    }

    #[test]
    fn the_reveal_leaves_objects_outside_the_boundary_undrawn() {
        let mut world = mint_sim::World::new();
        let mut outside = view(&mut world, KindOf::SELECTABLE, ObjectVisibility::Shrouded);
        outside.position = mint_movement::Vec2::new(-30.0, 200.0);
        let inside = view(&mut world, KindOf::SELECTABLE, ObjectVisibility::Shrouded);
        let mut units = vec![outside, inside];
        drawn_visibility(&mut units, false, &|x, y| x >= 0.0 && y >= 0.0);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].entity, inside.entity);
        assert_eq!(units[0].visibility, ObjectVisibility::Clear);
    }

    #[test]
    fn a_fogged_structure_shows_its_last_seen_state() {
        let mut world = mint_sim::World::new();
        let mut memory = GhostMemory::default();
        let fog = |_: f32, _: f32| CellVisibility::Explored;
        let mut seen = view(&mut world, STRUCTURE, ObjectVisibility::Clear);
        seen.health = Some(0.9);
        seen.conditions = mint_sage_scene::Conditions::PARTIALLY_CONSTRUCTED;
        let mut frame = vec![seen];
        memory.apply(&mut frame, PlayerId::new(0), true, &fog);

        let mut later = seen;
        later.visibility = ObjectVisibility::Fogged;
        later.health = Some(0.2);
        later.conditions = mint_sage_scene::Conditions::NONE;
        let mut frame = vec![later];
        let ghosts = memory.apply(&mut frame, PlayerId::new(0), true, &fog);
        assert_eq!(ghosts, 1);
        assert_eq!(frame[0].health, Some(0.9));
        assert_eq!(
            frame[0].conditions,
            mint_sage_scene::Conditions::PARTIALLY_CONSTRUCTED
        );
        assert_eq!(frame[0].visibility, ObjectVisibility::Fogged);
    }

    #[test]
    fn a_structure_destroyed_in_fog_is_remembered_until_its_ground_is_seen() {
        let mut world = mint_sim::World::new();
        let mut memory = GhostMemory::default();
        let seen = view(&mut world, STRUCTURE, ObjectVisibility::Clear);
        let mut frame = vec![seen];
        memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Visible
        });

        let mut frame: Vec<UnitView> = Vec::new();
        let ghosts = memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Explored
        });
        assert_eq!(ghosts, 1);
        assert_eq!(frame.len(), 1);
        assert_eq!(frame[0].entity, seen.entity);
        assert_eq!(frame[0].visibility, ObjectVisibility::Fogged);

        let mut frame: Vec<UnitView> = Vec::new();
        memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Visible
        });
        let mut after: Vec<UnitView> = Vec::new();
        let ghosts = memory.apply(&mut after, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Explored
        });
        assert_eq!(ghosts, 0);
        assert!(after.is_empty());
        assert_eq!(memory.len(), 0);
    }

    #[test]
    fn a_unit_is_never_a_ghost() {
        let mut world = mint_sim::World::new();
        let mut memory = GhostMemory::default();
        let mut frame = vec![view(
            &mut world,
            KindOf::SELECTABLE,
            ObjectVisibility::Clear,
        )];
        memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Visible
        });
        let mut frame: Vec<UnitView> = Vec::new();
        let ghosts = memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Explored
        });
        assert_eq!(ghosts, 0);
        assert!(frame.is_empty());
    }

    #[test]
    fn the_reveal_empties_the_memory() {
        let mut world = mint_sim::World::new();
        let mut memory = GhostMemory::default();
        let mut frame = vec![view(&mut world, STRUCTURE, ObjectVisibility::Clear)];
        memory.apply(&mut frame, PlayerId::new(0), true, &|_, _| {
            CellVisibility::Visible
        });
        assert_eq!(memory.len(), 1);
        let mut frame: Vec<UnitView> = Vec::new();
        memory.apply(&mut frame, PlayerId::new(0), false, &|_, _| {
            CellVisibility::Explored
        });
        assert_eq!(memory.len(), 0);
        assert!(frame.is_empty());
    }
}
