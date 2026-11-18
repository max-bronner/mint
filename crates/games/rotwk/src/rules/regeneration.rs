use mint_sage_modules::effects::{Name, Names};
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::spell_state::{HealedAt, LastHurt, PointBonus, SoleHealer};

pub(crate) const AUTO_HEAL_PULSE: LogicFrames = LogicFrames::from_engine_milliseconds(400);

const RECENT_DAMAGE_WINDOW: LogicFrames = LogicFrames::from_engine_milliseconds(4_000);

pub(super) struct HealPulse<'a> {
    pub(super) amount: f32,
    pub(super) radius: f32,
    pub(super) kinds: &'a Names,
    pub(super) forbidden: &'a Names,
    pub(super) skip_self: bool,
    pub(super) quiet: LogicFrames,
    pub(super) fx: Option<&'a Name>,
    pub(super) sole_for: LogicFrames,
    pub(super) as_percent: bool,
    pub(super) nonstackable: bool,
    pub(super) skip_recently_damaged: bool,
    pub(super) fx_always: bool,
}

impl RotwkRules {
    pub(super) fn heal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        hp: f32,
        as_percent: bool,
        now: Frame,
    ) -> Result<bool> {
        let Some(health) = tick.state_mut().get_mut(ids.health, entity) else {
            return Ok(false);
        };
        if health.0.is_dead() || !hp.is_finite() || hp <= 0.0 {
            return Ok(false);
        }
        let hp = if as_percent { hp * health.0.max() } else { hp };
        let before = health.0.current();
        health.0.set_current(before + hp);
        let landed = health.0.current() > before;
        if landed {
            tick.state_mut()
                .insert(ids.healed_at, entity, HealedAt(Some(now)))?;
        }
        Ok(landed)
    }

    fn hurt(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        !tick.state().contains(ids.doomed, entity)
            && tick
                .state()
                .get(ids.health, entity)
                .is_some_and(|health| !health.0.is_dead() && health.0.current() < health.0.max())
    }

    fn kinds_admit(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        pulse: &HealPulse<'_>,
    ) -> bool {
        let Some(entry) = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return pulse.kinds.is_empty() && pulse.forbidden.is_empty();
        };
        let any = pulse.kinds.is_empty() || pulse.kinds.iter().any(|k| entry.record.has_kind(k));
        any && !pulse.forbidden.iter().any(|k| entry.record.has_kind(k))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn burst_heal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        amount: f32,
        radius: f32,
        kinds: &[String],
        skip_self: bool,
        fx: Option<&str>,
    ) -> Result<usize> {
        let now = tick.frame();
        let world = tick.state();
        let (Some(_), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(0);
        };
        let reach = radius * radius;
        let targets: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|entity| !(skip_self && *entity == source))
            .filter(|entity| {
                world.contains(ids.owner, *entity) && self.object_allied(world, source, *entity)
            })
            .filter(|entity| {
                world.get(ids.body, *entity).is_some_and(|body| {
                    let (dx, dy) = (body.0.position.x - centre.x, body.0.position.y - centre.y);
                    dx * dx + dy * dy <= reach
                })
            })
            .filter(|entity| {
                kinds.is_empty()
                    || world
                        .get(ids.kind, *entity)
                        .and_then(|kind| self.kinds.get(*kind))
                        .is_some_and(|entry| kinds.iter().any(|kind| entry.record.has_kind(kind)))
            })
            .collect();
        let mut healed = 0usize;
        for entity in targets {
            if self.hurt(tick, ids, entity) && self.heal(tick, ids, entity, amount, false, now)? {
                healed = healed.saturating_add(1);
                if let Some(list) = fx {
                    let at = tick.state().get(ids.body, entity).map(|b| b.0.position);
                    if let Some(at) = at {
                        let power = crate::fx::FxSource::Power;
                        self.emit_fx_object_named(tick, ids, list, at, power, entity, None);
                    }
                }
            }
        }
        Ok(healed)
    }

    pub(super) fn pulse_auto_heal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        pulse: &HealPulse<'_>,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let now = tick.frame();
        let world = tick.state();
        if world.contains(ids.doomed, source)
            || world.get(ids.health, source).is_some_and(|h| h.0.is_dead())
            || world.contains(ids.construction, source)
        {
            return Ok(());
        }
        if pulse.radius <= 0.0 {
            let quiet = world
                .get(ids.last_hurt, source)
                .copied()
                .unwrap_or_default()
                .quiet_for(pulse.quiet, now);
            if quiet
                && self.hurt(tick, ids, source)
                && self.heal(tick, ids, source, pulse.amount, pulse.as_percent, now)?
            {
                self.heal_fx(tick, ids, source, pulse.fx);
            }
            return Ok(());
        }
        let (Some(owner), Some(centre)) = (
            world.get(ids.owner, source).map(|o| o.0),
            world.get(ids.body, source).map(|body| body.0.position),
        ) else {
            return Ok(());
        };
        let mut near: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(centre.x),
            grid_cell(centre.y),
            grid_radius_cells(pulse.radius),
            &mut near,
        );
        let reach = pulse.radius * pulse.radius;
        let mut targets: Vec<Entity> = near
            .iter()
            .filter_map(|index| living.get(usize::try_from(*index).ok()?))
            .filter(|(foe, player)| {
                let (dx, dy) = (foe.position.x - centre.x, foe.position.y - centre.y);
                dx * dx + dy * dy <= reach
                    && (*player == owner || self.object_allied(world, source, foe.entity))
            })
            .map(|(foe, _)| foe.entity)
            .filter(|entity| !pulse.skip_self || *entity != source)
            .collect();
        targets.sort_unstable_by_key(|entity| entity.index());
        targets.dedup();
        let until = now.checked_add(pulse.sole_for).unwrap_or(Frame::FOREVER);
        let bypasses_sole = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("SWARM_DOZER"));
        for entity in targets {
            if pulse.skip_recently_damaged
                && !tick
                    .state()
                    .get(ids.last_hurt, entity)
                    .copied()
                    .unwrap_or_default()
                    .quiet_for(RECENT_DAMAGE_WINDOW, now)
            {
                continue;
            }
            if !self.hurt(tick, ids, entity) || !self.kinds_admit(tick, ids, entity, pulse) {
                continue;
            }
            if pulse.nonstackable
                && !tick
                    .state()
                    .get(ids.healed_at, entity)
                    .copied()
                    .unwrap_or_default()
                    .quiet_for(pulse.sole_for, now)
            {
                continue;
            }
            if !bypasses_sole {
                let held = tick.state().get(ids.sole_healer, entity).copied();
                if !SoleHealer::admits(held, source, now) {
                    continue;
                }
                tick.state_mut().insert(
                    ids.sole_healer,
                    entity,
                    SoleHealer { by: source, until },
                )?;
            }
            let landed = self.heal(tick, ids, entity, pulse.amount, pulse.as_percent, now)?;
            if landed || pulse.fx_always {
                self.heal_fx(tick, ids, entity, pulse.fx);
            }
        }
        Ok(())
    }

    fn heal_fx(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity, fx: Option<&Name>) {
        let Some(list) = fx else {
            return;
        };
        if let Some(at) = tick.state().get(ids.body, entity).map(|b| b.0.position) {
            let power = crate::fx::FxSource::Power;
            self.emit_fx_object_named(tick, ids, list, at, power, entity, None);
        }
    }

    pub(super) fn pulse_modifier_heals(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        if tick.frame().get() % u64::from(AUTO_HEAL_PULSE.get()) != 0 {
            return Ok(());
        }
        let now = tick.frame();
        let healing: Vec<(Entity, f32)> = tick
            .state()
            .store(ids.bonuses)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, bonuses)| bonuses.0.auto_heal > 0.0)
                    .map(|(entity, bonuses)| (entity, bonuses.0.auto_heal))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, hp) in healing {
            if self.hurt(tick, ids, entity) {
                self.heal(tick, ids, entity, hp, false, now)?;
            }
        }
        Ok(())
    }

    pub(super) fn note_hurt(&self, tick: &mut Tick<'_>, ids: Ids, victim: Entity) {
        let now = tick.frame();
        if let Some(last) = tick.state_mut().get_mut(ids.last_hurt, victim) {
            *last = LastHurt(Some(now));
        }
    }

    pub(super) fn attach_regeneration(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        kind: crate::unit::Kind,
    ) -> Result<()> {
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(());
        };
        tick.state_mut()
            .insert(ids.last_hurt, entity, LastHurt(None))?;
        let bonus = entry
            .record
            .command_point_bonus
            .filter(|v| v.is_finite() && v.abs() <= 1.0e6)
            .map(|v| {
                #[allow(clippy::cast_possible_truncation)]
                let whole = v.round() as i32;
                whole
            })
            .unwrap_or(0);
        if bonus != 0 {
            tick.state_mut()
                .insert(ids.point_bonus, entity, PointBonus(bonus))?;
        }
        Ok(())
    }

    pub(super) fn recount_point_limits(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let counts = |entity: Entity| {
            !world.contains(ids.doomed, entity)
                && world
                    .get(ids.health, entity)
                    .is_none_or(|health| !health.0.is_dead())
        };
        let mut bonus: Vec<(PlayerId, i32)> = Vec::new();
        let mut add = |player: PlayerId, points: i32| match bonus
            .iter_mut()
            .find(|(held, _)| *held == player)
        {
            Some((_, sum)) => *sum = sum.saturating_add(points),
            None => bonus.push((player, points)),
        };
        if let Some(store) = world.store(ids.point_bonus) {
            for (entity, points) in store.iter() {
                if let (true, Some(owner)) = (counts(entity), world.get(ids.owner, entity)) {
                    add(owner.0, points.0);
                }
            }
        }
        if let Some(store) = world.store(ids.bonuses) {
            for (entity, bonuses) in store.iter() {
                let points = bonuses.0.command_point_bonus();
                if points == 0 {
                    continue;
                }
                if let (true, Some(owner)) = (counts(entity), world.get(ids.owner, entity)) {
                    add(owner.0, points);
                }
            }
        }
        if let Some(store) = world.store(ids.player_point_bonus) {
            for (entity, points) in store.iter() {
                if let Some(slot) = world.get(ids.slot, entity) {
                    add(slot.0, points.0);
                }
            }
        }
        let players: Vec<(Entity, PlayerId, crate::spell_state::PointRule)> = world
            .store(ids.point_rule)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(entity, rule)| {
                        Some((entity, world.get(ids.slot, entity)?.0, *rule))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (entity, player, rule) in players {
            let extra = bonus
                .iter()
                .find(|(held, _)| *held == player)
                .map_or(0, |(_, sum)| *sum);
            if let Some(points) = tick.state_mut().get_mut(ids.points, entity) {
                points.0.set_limit(rule.limit(extra));
            }
        }
        Ok(())
    }
}
