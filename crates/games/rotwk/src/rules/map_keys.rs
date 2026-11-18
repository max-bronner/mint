use mint_sage_ini::Stance;
use mint_sim::{Component, Entity, Result, StateHasher, Tick, UpgradeMask};

use super::map_scripts::{ForcedModelCondition, ScriptStatusBit};
use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::map_keys::ObjectKeys;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventsList(pub(crate) String);

impl Component for EventsList {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.0.as_bytes());
    }
}

const WEATHER_SNOW: i32 = 2;

#[derive(Default)]
struct Applied {
    health: usize,
    trimmed: usize,
    experience: usize,
    attitude: usize,
    stance: usize,
    selectable: usize,
    statuses: usize,
    weather: usize,
    targetable: usize,
    unrecruitable: usize,
    upgrades: usize,
}

impl RotwkRules {
    pub(super) fn place_events_list(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        keys: &ObjectKeys,
    ) -> Result<()> {
        if let Some(list) = &keys.events_list {
            tick.state_mut()
                .insert(ids.events_list, entity, EventsList(list.clone()))?;
        }
        Ok(())
    }

    pub(super) fn apply_placed_keys(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        anchor: Entity,
        keyed: &[(Entity, ObjectKeys)],
    ) -> Result<()> {
        if keyed.is_empty() {
            return Ok(());
        }
        let mut applied = Applied::default();
        for (entity, keys) in keyed {
            let entity = *entity;
            if let Some(percent) = keys.initial_health {
                self.apply_initial_health(tick, ids, entity, percent, &mut applied)?;
            }
            if let Some(rank) = keys.experience {
                self.apply_experience_level(tick, ids, entity, rank, &mut applied)?;
            }
            if let Some(value) = keys.aggressiveness {
                if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
                    let stamp = state.rt.teams.stamp();
                    if state.rt.attitudes.set(entity, value, stamp) {
                        applied.attitude = applied.attitude.saturating_add(1);
                    }
                }
            }
            if let Some(stance) = keys
                .initial_stance
                .and_then(Stance::from_engine_ordinal)
                .filter(|stance| *stance != Stance::Uninitialized)
            {
                if self.holds_a_stance(tick.state(), ids, entity) {
                    self.apply_stance_change(tick, ids, entity, stance)?;
                    applied.stance = applied.stance.saturating_add(1);
                }
            }
            if keys.recruitable == Some(false) {
                self.set_placed_status(
                    tick,
                    ids,
                    anchor,
                    entity,
                    ScriptStatusBit::NotRecruitable,
                    true,
                )?;
                applied.unrecruitable = applied.unrecruitable.saturating_add(1);
            }
            if let Some(on) = keys.selectable {
                let unselectable = self.modules.as_ref().is_some_and(|wiring| {
                    wiring.carries_status(tick.state(), entity, "UNSELECTABLE")
                });
                if on == unselectable {
                    self.set_selectable(tick, ids, entity, on)?;
                    applied.selectable = applied.selectable.saturating_add(1);
                }
            }
            for (bit, on) in [
                (ScriptStatusBit::Disabled, keys.enabled.map(|on| !on)),
                (ScriptStatusBit::Unpowered, keys.powered.map(|on| !on)),
                (ScriptStatusBit::Unsellable, keys.unsellable),
                (ScriptStatusBit::Targetable, keys.targetable.then_some(true)),
            ] {
                let Some(on) = on else { continue };
                self.set_placed_status(tick, ids, anchor, entity, bit, on)?;
                if bit == ScriptStatusBit::Targetable {
                    applied.targetable = applied.targetable.saturating_add(1);
                } else {
                    applied.statuses = applied.statuses.saturating_add(1);
                }
            }
            if keys.weather == Some(WEATHER_SNOW) {
                tick.state_mut().insert(
                    ids.forced_model_condition,
                    entity,
                    ForcedModelCondition {
                        name: "SNOW".to_owned(),
                        until: None,
                    },
                )?;
                applied.weather = applied.weather.saturating_add(1);
            }
            if keys.time.is_some() {
                diag!(
                    "script: placed object {}: objectTime is not modelled (no NIGHT bit)",
                    entity.index()
                );
            }
        }
        for (entity, keys) in keyed {
            for name in &keys.upgrades {
                if self.apply_placed_upgrade(tick, ids, *entity, name)? {
                    applied.upgrades = applied.upgrades.saturating_add(1);
                }
            }
        }
        println!(
            "map keys: {} object(s) carry keys: initialHealth {} ({} horde member(s) removed), \
             experienceLevel {}, aggressiveness {}, stance {}, selectable {}, enabled/powered/unsellable {}, \
             snow {}, targetable {}, not recruitable {}, upgrades {}",
            keyed.len(),
            applied.health,
            applied.trimmed,
            applied.experience,
            applied.attitude,
            applied.stance,
            applied.selectable,
            applied.statuses,
            applied.weather,
            applied.targetable,
            applied.unrecruitable,
            applied.upgrades
        );
        Ok(())
    }

    fn set_placed_status(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        anchor: Entity,
        entity: Entity,
        bit: ScriptStatusBit,
        on: bool,
    ) -> Result<()> {
        let changed = tick
            .state_mut()
            .get_mut(ids.map_script_state, anchor)
            .is_some_and(|state| state.rt.statuses.set(entity, bit, on));
        if changed && bit == ScriptStatusBit::Disabled {
            self.pause_special_powers(tick, ids, entity, on)?;
        }
        Ok(())
    }

    fn apply_initial_health(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        percent: i32,
        applied: &mut Applied,
    ) -> Result<()> {
        if let Some(horde) = tick.state().get(ids.horde, entity) {
            let members = horde.members.clone();
            let count = f64::from(u32::try_from(members.len()).unwrap_or(u32::MAX));
            let share = (f64::from(100_i32.saturating_sub(percent)) * 0.01 * count).trunc();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let removed = if share >= 1.0 { share as usize } else { 0 };
            if removed == 0 || removed >= members.len() {
                return Ok(());
            }
            let doomed: Vec<Entity> = members
                .into_iter()
                .filter(|member| tick.state().contains(ids.health, *member))
                .take(removed)
                .collect();
            for member in &doomed {
                tick.state_mut().despawn(*member);
            }
            applied.trimmed = applied.trimmed.saturating_add(doomed.len());
            applied.health = applied.health.saturating_add(1);
            crate::horde::prune_hordes(tick, Self::horde_ids(ids), &self.formations)?;
            return Ok(());
        }
        let Some(mut health) = tick.state().get(ids.health, entity).copied() else {
            return Ok(());
        };
        #[allow(clippy::cast_precision_loss)]
        health.0.set_initial_percent(percent as f32);
        tick.state_mut().insert(ids.health, entity, health)?;
        applied.health = applied.health.saturating_add(1);
        Ok(())
    }

    fn apply_experience_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        rank: i32,
        applied: &mut Applied,
    ) -> Result<()> {
        let world = tick.state();
        let Some(current) = self
            .experience_of(world, ids, entity)
            .and_then(|(index, _)| self.level_table_of(world, ids, entity)?.level(index))
            .map(|level| level.rank)
        else {
            return Ok(());
        };
        let delta = rank.saturating_sub(i32::from(current));
        let Ok(levels) = u32::try_from(delta) else {
            return Ok(());
        };
        if levels == 0 {
            return Ok(());
        }
        self.gain_levels(tick, ids, entity, levels, None)?;
        applied.experience = applied.experience.saturating_add(1);
        Ok(())
    }

    fn apply_placed_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<bool> {
        let Some(bit) = self.upgrade_bits.bit(name) else {
            return Ok(false);
        };
        let mut granted = self.upgrade_bits.sub_mask(bit);
        if granted.is_empty() {
            granted = UpgradeMask::EMPTY;
            granted.set(bit);
        }
        let held = tick
            .state()
            .get(ids.object_upgrades, entity)
            .is_some_and(|held| held.0.contains_all(&granted));
        if held && tick.state().contains(ids.horde, entity) {
            return Ok(false);
        }
        self.grant_object_upgrades(tick, ids, entity, &granted)?;
        Ok(true)
    }
}
