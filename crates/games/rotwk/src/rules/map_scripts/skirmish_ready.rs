use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_ini::CommandButtonDef;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, StateHasher, Tick, SIMULATION_HZ};

use super::count_cache::CacheKey;
use super::status::ScriptStatusBit;
use super::{parameter_string, ConditionVerb, MapScriptState, ParseCtx};
use crate::powers::is_ready;
use crate::rules::{Ids, RotwkRules};

const READY_CACHE_TICKS: u64 = 10 * SIMULATION_HZ as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cached {
    Never,
    Until(u64),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PowerReadyCache(BTreeMap<CacheKey, Cached>);

impl PowerReadyCache {
    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (key, cached) in &self.0 {
            hasher.write_u32(key.0);
            match cached {
                Cached::Never => hasher.write_u8(0),
                Cached::Until(frame) => {
                    hasher.write_u8(1);
                    hasher.write_u64(*frame);
                }
            }
        }
    }
}

struct ConditionPowerReady {
    player: String,
    power: String,
    cache: CacheKey,
}

impl ConditionVerb for ConditionPowerReady {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        current: Option<PlayerId>,
    ) -> bool {
        let mut cache = state.rt.power_ready.clone();
        rules.power_ready(tick, ids, state, current, self, &mut cache)
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        current: Option<PlayerId>,
    ) -> bool {
        let mut cache = std::mem::take(&mut state.rt.power_ready);
        let ready = rules.power_ready(tick, ids, state, current, self, &mut cache);
        state.rt.power_ready = cache;
        ready
    }
}

impl RotwkRules {
    fn power_ready(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        current: Option<PlayerId>,
        condition: &ConditionPowerReady,
        cache: &mut PowerReadyCache,
    ) -> bool {
        let now = tick.frame().get();
        match cache.0.get(&condition.cache) {
            Some(Cached::Never) => return false,
            Some(Cached::Until(frame)) if *frame > now => return false,
            _ => {}
        }
        if self.power_names.id(&condition.power).is_none() {
            cache.0.insert(condition.cache, Cached::Never);
            return false;
        }
        let world = tick.state();
        let mut next = now.saturating_add(READY_CACHE_TICKS);
        let mask = self.player_mask(&state.rt, &condition.player, current);
        for player in mask.players() {
            for entity in Self::owned_by(world, ids, player) {
                if world.contains(ids.construction, entity)
                    || world.contains(ids.doomed, entity)
                    || state.rt.statuses.has(entity, ScriptStatusBit::Disabled)
                    || !self.unit_has_power(tick, ids, entity, &condition.power)
                    || !self.power_science_met(world, ids, player, &condition.power)
                {
                    continue;
                }
                let Ok((paused, available_on)) =
                    self.power_availability(world, ids, entity, &condition.power)
                else {
                    continue;
                };
                if is_ready(paused, available_on, tick.frame()) {
                    return true;
                }
                next = next.min(available_on.get());
            }
        }
        cache.0.insert(condition.cache, Cached::Until(next));
        false
    }

    fn command_button_ready(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        member: Entity,
        def: &CommandButtonDef,
    ) -> bool {
        let world = tick.state();
        let upgrade = def
            .upgrade
            .as_deref()
            .and_then(|name| self.upgrade_bits.bit(name));
        let held =
            upgrade.is_some_and(|bit| self.effective_upgrades_in(world, ids, member).contains(bit));
        if Self::button_command(def) == "FIRE_WEAPON" {
            return upgrade.is_none() || held;
        }
        if let Some(power) = def.special_power.as_deref() {
            return self.power_is_ready(world, ids, tick.frame(), member, power);
        }
        upgrade.is_some_and(|bit| !held && self.affected_by_upgrade(world, ids, member, bit))
    }
}

struct ConditionButtonReady {
    team: String,
    side: String,
    button: String,
    all: bool,
}

impl ConditionVerb for ConditionButtonReady {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return false;
        };
        let Some(def) = rules.ai_power_buttons.get(&self.button) else {
            return false;
        };
        let is_fire = RotwkRules::button_command(def) == "FIRE_WEAPON";
        for member in rules.live_members(tick, state, team) {
            if let Some(power) = def.special_power.as_deref() {
                if !rules.unit_has_power(tick, ids, member, power) {
                    continue;
                }
            } else if def.upgrade.is_none() && !is_fire {
                continue;
            }
            let ready = rules.command_button_ready(tick, ids, member, def);
            if ready != self.all {
                return ready;
            }
        }
        self.all
    }
}

pub(super) fn parse_power_ready(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionPowerReady {
        player: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
        cache: ctx.next_cache_key(),
    }))
}

fn button_ready(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    all: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionButtonReady {
        team: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
        button: parameter_string(params, 2)?,
        all,
    }))
}

pub(super) fn parse_button_ready_all(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    button_ready(params, ctx, true)
}

pub(super) fn parse_button_ready_partial(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    button_ready(params, ctx, false)
}
