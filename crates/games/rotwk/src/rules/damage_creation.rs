use mint_movement::Vec2;
use mint_sage_combat::{FxTrigger, WeaponId};
use mint_sage_modules::body::DestroyedSide;
use mint_sage_modules::effects::{names, Effect};
use mint_sim::{Entity, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{DamageThresholds, Launched};
use crate::diag::diag;

impl RotwkRules {
    pub(super) fn launched_by(&self, launcher: WeaponId, from: Vec2) -> Option<Launched> {
        let trigger = self.combat.weapons.get(launcher)?.fx_trigger;
        (trigger != FxTrigger::None).then_some(Launched { trigger, from })
    }

    pub(super) fn landing_debris(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        launched: Launched,
        at: Vec2,
        struck: &[Entity],
    ) -> Result<()> {
        for &body in struck {
            let lists: Vec<String> = self
                .damage_creations(tick.state(), ids, body)
                .iter()
                .filter(|line| line.trigger == launched.trigger && line.side.is_none())
                .map(|line| line.list.clone())
                .collect();
            for list in lists {
                let made = self.create_for(tick, ids, body, &list, at)?;
                diag!(
                    "damage-creation: frame {} entity {} {} struck by {:?} at ({:.1},{:.1}): {list} made {made} object(s)",
                    tick.frame().get(),
                    body.index(),
                    self.kind_name(tick.state().get(ids.kind, body).copied()),
                    launched.trigger,
                    at.x,
                    at.y
                );
            }
        }
        Ok(())
    }

    pub(super) fn knock_side_off(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        body: Entity,
        launched: Launched,
        before: f32,
        after: f32,
    ) -> Result<()> {
        let world = tick.state();
        if world.contains(ids.construction, body) {
            return Ok(());
        }
        let Some(place) = world.get(ids.body, body).map(|b| b.0) else {
            return Ok(());
        };
        let thresholds = self.thresholds_of(world, ids, body);
        let (from, to) = (thresholds.rung(before), thresholds.rung(after));
        if to <= from {
            return Ok(());
        }
        let side = side_struck(
            place.facing.to_vector(),
            launched.from.minus(place.position),
        );
        let Some(list) = self
            .damage_creations(world, ids, body)
            .iter()
            .find(|line| line.trigger == launched.trigger && line.side == Some(side))
            .map(|line| line.list.clone())
        else {
            return Ok(());
        };
        let condition = side.model_condition();
        let off = self.modules.as_ref().is_some_and(|wiring| {
            world.get(wiring.state, body).is_some_and(|state| {
                state
                    .model_conditions
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(condition))
            })
        });
        if off {
            return Ok(());
        }
        let made = self.create_for(tick, ids, body, &list, place.position)?;
        self.apply_effects(
            tick,
            ids,
            vec![(
                body,
                Effect::ModelConditions {
                    add: names(vec![condition.to_owned()]),
                    remove: names(Vec::new()),
                },
            )],
        )?;
        diag!(
            "damage-creation: frame {} entity {} {} {from:?} -> {to:?}, struck on its {side:?} from ({:.1},{:.1}): {list} made {made} object(s), {condition} raised",
            tick.frame().get(),
            body.index(),
            self.kind_name(tick.state().get(ids.kind, body).copied()),
            launched.from.x,
            launched.from.y
        );
        Ok(())
    }

    fn damage_creations(
        &self,
        world: &World,
        ids: Ids,
        body: Entity,
    ) -> &[mint_sage_modules::body::DamageCreation] {
        world
            .get(ids.kind, body)
            .and_then(|kind| self.kinds.get(*kind))
            .map_or(&[], |entry| entry.record.damage_creations.as_slice())
    }

    pub(super) fn thresholds_of(&self, world: &World, ids: Ids, body: Entity) -> DamageThresholds {
        DamageThresholds::of(
            world
                .get(ids.kind, body)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record),
            self.damage_thresholds,
        )
    }

    fn create_for(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        body: Entity,
        list: &str,
        at: Vec2,
    ) -> Result<usize> {
        let world = tick.state();
        let Some(owner) = world.get(ids.owner, body).map(|owner| owner.0) else {
            return Ok(0);
        };
        self.run_creation_list(
            tick,
            ids,
            body,
            list,
            at,
            super::spells::CreationVariant::Object,
            owner,
            None,
            None,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DieOclStamp(pub(crate) mint_sim::Frame);

impl mint_sim::Component for DieOclStamp {
    fn hash_into(&self, hasher: &mut mint_sim::StateHasher) {
        self.0.hash_into(hasher);
    }
}

fn side_struck(facing: Vec2, towards: Vec2) -> DestroyedSide {
    let ahead = facing.x * towards.x + facing.y * towards.y;
    let left = facing.x * towards.y - facing.y * towards.x;
    if ahead.abs() >= left.abs() {
        if ahead >= 0.0 {
            DestroyedSide::Front
        } else {
            DestroyedSide::Back
        }
    } else if left > 0.0 {
        DestroyedSide::Left
    } else {
        DestroyedSide::Right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blow_strikes_the_quadrant_it_arrives_from_relative_to_the_facing() {
        let north = Vec2::new(0.0, 1.0);
        let cases = [
            (Vec2::new(0.2, 5.0), DestroyedSide::Front),
            (Vec2::new(-0.2, -5.0), DestroyedSide::Back),
            (Vec2::new(-5.0, 1.0), DestroyedSide::Left),
            (Vec2::new(5.0, -1.0), DestroyedSide::Right),
            (Vec2::new(3.0, 3.0), DestroyedSide::Front),
        ];
        for (towards, side) in cases {
            assert_eq!(side_struck(north, towards), side, "from {towards:?}");
        }
    }
}
