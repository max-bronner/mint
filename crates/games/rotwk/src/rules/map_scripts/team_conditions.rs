use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Tick};

use super::area::Area;
use super::{
    compare, parameter_int, parameter_real, parameter_string, ConditionVerb, MapScriptState,
    ParseCtx, COMPARE_EQUAL, COMPARE_GREATER, COMPARE_GREATER_EQUAL, COMPARE_LESS_EQUAL,
    COMPARE_LESS_THAN, COMPARE_NOT_EQUAL,
};
use crate::rules::{Ids, RotwkRules};

const STRUCTURE_THREAT: f32 = 150.0;

impl RotwkRules {
    fn script_threat(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> f32 {
        let world = tick.state();
        let level = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.threat_level)
            .unwrap_or(0.0);
        let health = world
            .get(ids.health, entity)
            .map_or(1.0, |held| held.0.fraction());
        let value = world
            .get(ids.object_value, entity)
            .copied()
            .unwrap_or_default();
        #[allow(clippy::cast_precision_loss)]
        let (paid, spent) = (value.paid_cost as f32, value.upgrade_spend as f32);
        if self.has_kind_bit(world, ids, entity, "STRUCTURE") {
            if !self.has_kind_bit(world, ids, entity, "CAN_ATTACK") {
                return 0.0;
            }
            if self.has_kind_bit(world, ids, entity, "FS_BASE_DEFENSE") {
                return paid + spent;
            }
            let value = if level <= 100.0 {
                STRUCTURE_THREAT
            } else {
                level
            };
            return value * health;
        }
        let veterancy = world
            .get(ids.experience, entity)
            .map_or(0.0, |held| f32::from(held.level));
        let multiplier = 0.8 + 0.2 * veterancy;
        let worth = paid * multiplier + spent;
        let worth = if worth == 0.0 {
            level * multiplier
        } else {
            worth
        };
        worth * health
    }
}

fn compare_real(current: f32, op: i32, value: f32) -> bool {
    match op {
        COMPARE_LESS_THAN => current < value,
        COMPARE_LESS_EQUAL => current <= value,
        COMPARE_EQUAL => current == value,
        COMPARE_GREATER_EQUAL => current >= value,
        COMPARE_GREATER => current > value,
        COMPARE_NOT_EQUAL => current != value,
        _ => false,
    }
}

struct ConditionTeamHealth {
    team: String,
    side: String,
    comparison: i32,
    percent: i32,
}

impl ConditionVerb for ConditionTeamHealth {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let world = tick.state();
        let (mut total, mut alive) = (0_i64, 0_i64);
        for member in rules.subject_members(tick, ids, state, &self.team, Some(&self.side)) {
            if world.contains(ids.member, member)
                || rules.kind_skipped(tick, ids, member, &["INERT", "IGNORED_IN_GUI"])
            {
                continue;
            }
            match world.get(ids.horde, member) {
                Some(horde) => {
                    total += i64::try_from(horde.slots.len()).unwrap_or(0);
                    alive += i64::try_from(horde.members.len()).unwrap_or(0);
                }
                None => {
                    total += 1;
                    alive += 1;
                }
            }
        }
        let share = if total == 0 { 0 } else { alive * 100 / total };
        compare(
            i32::try_from(share).unwrap_or(0),
            self.comparison,
            self.percent,
        )
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

struct ConditionUnitHealth {
    unit: String,
    side: String,
    comparison: i32,
    percent: i32,
}

impl ConditionVerb for ConditionUnitHealth {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) else {
            return false;
        };
        let world = tick.state();
        let Some(body) = world.get(ids.health, entity) else {
            return false;
        };
        let share = match world.get(ids.horde, entity) {
            Some(horde) => {
                let (alive, total) = (horde.members.len(), horde.slots.len());
                if total == 0 {
                    0
                } else {
                    i32::try_from(alive.saturating_mul(100) / total).unwrap_or(0)
                }
            }
            None => {
                let max = body.0.max();
                if max > 0.0 {
                    #[allow(clippy::cast_possible_truncation)]
                    let truncated = (body.0.current() * 100.0 / max) as i32;
                    truncated
                } else {
                    0
                }
            }
        };
        compare(share, self.comparison, self.percent)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ConditionTeamMoreThan {
    team: String,
    side: String,
    limit: i32,
    kind_of: String,
}

impl ConditionVerb for ConditionTeamMoreThan {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let world = tick.state();
        let count = rules
            .subject_members(tick, ids, state, &self.team, Some(&self.side))
            .into_iter()
            .filter(|&member| {
                rules.has_kind_bit(world, ids, member, &self.kind_of)
                    && !world.contains(ids.construction, member)
            })
            .count();
        i64::try_from(count).unwrap_or(i64::MAX) > i64::from(self.limit)
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

struct ConditionValueInArea {
    player_text: String,
    comparison: i32,
    value: f32,
    area: Area,
}

impl ConditionVerb for ConditionValueInArea {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        let world = tick.state();
        let mut total = 0.0_f32;
        if self.area.exists() {
            if let Some(owners) = world.store(ids.owner) {
                for (entity, owner) in owners.iter() {
                    if !mask.contains(owner.0)
                        || !world.is_live(entity)
                        || world.contains(ids.doomed, entity)
                        || world.contains(ids.member, entity)
                    {
                        continue;
                    }
                    let inside = world
                        .get(ids.body, entity)
                        .is_some_and(|body| self.area.contains(body.0.position));
                    if inside && !rules.kind_skipped(tick, ids, entity, &["INERT"]) {
                        total += rules.script_threat(tick, ids, entity);
                    }
                }
            }
        }
        compare_real(total, self.comparison, self.value)
    }
}

struct ConditionTeamReachedPathEnd {
    team: String,
    side: String,
    path: String,
}

impl ConditionVerb for ConditionTeamReachedPathEnd {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .subject_members(tick, ids, state, &self.team, Some(&self.side))
            .into_iter()
            .any(|member| state.rt.walks.finished(member, &self.path))
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

pub(super) fn parse_eval_team_health(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTeamHealth {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        comparison: parameter_int(params, 1)?,
        percent: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_unit_health(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionUnitHealth {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        comparison: parameter_int(params, 1)?,
        percent: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_team_more_than(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTeamMoreThan {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        limit: parameter_int(params, 1)?,
        kind_of: parameter_string(params, 2)?,
    }))
}

pub(super) fn parse_value_in_area(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionValueInArea {
        player_text: parameter_string(params, 0)?,
        comparison: parameter_int(params, 1)?,
        value: parameter_real(params, 2)?,
        area: Area::resolve(ctx, &parameter_string(params, 3)?),
    }))
}

pub(super) fn parse_team_reached_path_end(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTeamReachedPathEnd {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        path: parameter_string(params, 1)?,
    }))
}
