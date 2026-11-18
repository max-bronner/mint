use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::object_status::Target;
use super::{
    compare, parameter_int, parameter_string, ActionVerb, ConditionVerb, MapScriptEngine,
    MapScriptState, ParseCtx, COMPARE_GREATER_EQUAL,
};
use crate::experience::{scaled, Experience};
use crate::rules::{Ids, RotwkRules};

impl RotwkRules {
    fn script_give_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
        amount: i32,
    ) -> Result<()> {
        let Ok(amount) = u32::try_from(amount) else {
            return Ok(());
        };
        let world = tick.state();
        let receiver = self.sink_of(world, ids, member);
        if !self
            .level_table_of(world, ids, receiver)
            .is_some_and(|table| table.trainable)
        {
            return Ok(());
        }
        let scalar = self.xp_scalar(world, ids, receiver) * self.level_scalar(world, ids, receiver);
        let gain = scaled(amount, scalar);
        if gain > 0 {
            self.add_experience(tick, ids, receiver, gain)?;
        }
        Ok(())
    }

    fn script_set_experience(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        amount: i32,
    ) -> Result<()> {
        let world = tick.state();
        let receiver = self.sink_of(world, ids, entity);
        let Some(table) = self.level_table_of(world, ids, receiver) else {
            return Ok(());
        };
        if !table.trainable {
            return Ok(());
        }
        let points = u32::try_from(amount).unwrap_or(0);
        let new = table.index_for(points);
        let Some((old, _)) = self.experience_of(world, ids, receiver) else {
            return Ok(());
        };
        self.set_experience(tick, ids, receiver, points, old, new)
    }

    fn script_set_max_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        cap: i32,
    ) -> Result<()> {
        let world = tick.state();
        let Some((level, points)) = self.experience_of(world, ids, entity) else {
            return Ok(());
        };
        let base_rank = self.base_rank_of(world, ids, entity);
        let max_level = u16::try_from(cap.max(0)).unwrap_or(u16::MAX);
        tick.state_mut().insert(
            ids.experience,
            entity,
            Experience {
                points,
                level: u16::try_from(level).unwrap_or(u16::MAX),
                base_rank,
                max_level,
            },
        )?;
        Ok(())
    }

    fn script_give_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        let world = tick.state();
        let receiver = self.sink_of(world, ids, entity);
        let Some(table) = self.level_table_of(world, ids, receiver) else {
            return Ok(());
        };
        let Some(target) = table
            .levels
            .iter()
            .position(|level| level.name.eq_ignore_ascii_case(name))
        else {
            return Ok(());
        };
        let Some((old, points)) = self.experience_of(world, ids, receiver) else {
            return Ok(());
        };
        let required = table.level(target).map_or(points, |level| level.required);
        let points = if target > old {
            points.max(required)
        } else {
            required
        };
        self.set_experience(tick, ids, receiver, points, old, target)
    }

    fn level_name_of(&self, world: &World, ids: Ids, entity: Entity) -> Option<&str> {
        let receiver = self.sink_of(world, ids, entity);
        let (index, _) = self.experience_of(world, ids, receiver)?;
        Some(
            self.level_table_of(world, ids, receiver)?
                .level(index)?
                .name
                .as_str(),
        )
    }

    fn level_rank_of(&self, world: &World, ids: Ids, entity: Entity) -> Option<u16> {
        let receiver = self.sink_of(world, ids, entity);
        let (index, _) = self.experience_of(world, ids, receiver)?;
        Some(
            self.level_table_of(world, ids, receiver)?
                .level(index)?
                .rank,
        )
    }

    fn script_set_rank_cap(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        points: i32,
    ) -> Result<()> {
        let Some(entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        let Some(mut state) = tick.state().get(ids.player_science, entity).cloned() else {
            return Ok(());
        };
        let wanted = u32::try_from(points).unwrap_or(0);
        state.rank_cap = (1..=u16::MAX)
            .map_while(|rank| self.ranks.needed(rank).map(|needed| (rank, needed)))
            .find(|&(_, needed)| needed >= wanted)
            .map_or(0, |(rank, _)| rank);
        tick.state_mut().insert(ids.player_science, entity, state)?;
        Ok(())
    }

    fn script_revival_at_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        template: &str,
        rank: i32,
    ) -> Result<()> {
        let Some(kind) = self.kinds.kind(template) else {
            return Ok(());
        };
        let Some(table) = self.level_tables.get(kind) else {
            return Ok(());
        };
        let level = if rank < 0 {
            table.level(table.created_at)
        } else {
            u16::try_from(rank)
                .ok()
                .and_then(|rank| table.index_at_rank(rank))
                .and_then(|index| table.level(index))
        };
        let Some(points) = level.map(|level| level.required) else {
            return Ok(());
        };
        let world = tick.state();
        let Some(slot) = self
            .roster_of(world, ids, player)
            .and_then(|roster| roster.0.iter().position(|slot| slot.kind == Some(kind)))
        else {
            return Ok(());
        };
        let Some(holder) = crate::economy::player_entity(world, Self::economy_ids(ids), player)
        else {
            return Ok(());
        };
        self.set_hero_state(
            tick,
            ids,
            holder,
            slot,
            crate::heroes::HeroState::Dead { points },
        )
    }
}

enum Op {
    Give(i32),
    Set(i32),
    MaxLevel(i32),
    GainLevel,
    GiveLevel(String),
}

struct ActionExperience {
    target: Target,
    op: Op,
}

impl ActionVerb for ActionExperience {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        for entity in self.target.entities(rules, tick, ids, state) {
            match &self.op {
                Op::Give(amount) => rules.script_give_experience(tick, ids, entity, *amount)?,
                Op::Set(amount) => rules.script_set_experience(tick, ids, entity, *amount)?,
                Op::MaxLevel(cap) => rules.script_set_max_level(tick, ids, entity, *cap)?,
                Op::GainLevel => rules.gain_levels(tick, ids, entity, 1, None)?,
                Op::GiveLevel(name) => rules.script_give_level(tick, ids, entity, name)?,
            }
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.target.unit_name()
    }
}

struct ActionPlayerExperience {
    player: String,
    op: PlayerOp,
}

enum PlayerOp {
    MaxSpellPoints(i32),
    SetRankLevel(i32),
    RevivalAtLevel { template: String, rank: i32 },
}

impl ActionVerb for ActionPlayerExperience {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        current: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mask = rules.player_mask(&state.rt, &self.player, current);
        match &self.op {
            PlayerOp::MaxSpellPoints(points) => {
                for player in mask.players() {
                    rules.script_set_rank_cap(tick, ids, player, *points)?;
                }
            }
            PlayerOp::SetRankLevel(level) => {
                for player in mask.players() {
                    rules.script_set_rank_level(tick, ids, player, *level)?;
                }
            }
            PlayerOp::RevivalAtLevel { template, rank } => {
                if let Some(player) = mask.first() {
                    rules.script_revival_at_level(tick, ids, player, template, *rank)?;
                }
            }
        }
        Ok(())
    }

    fn referenced_template(&self) -> Option<&str> {
        match &self.op {
            PlayerOp::RevivalAtLevel { template, .. } => Some(template),
            PlayerOp::MaxSpellPoints(_) | PlayerOp::SetRankLevel(_) => None,
        }
    }
}

fn action(target: Option<Target>, op: Op) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionExperience {
        target: target?,
        op,
    }))
}

pub(super) fn parse_team_give_points(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(
        Target::parse(params, ctx, true),
        Op::Give(parameter_int(params, 1)?),
    )
}

pub(super) fn parse_unit_give_points(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(
        Target::parse(params, ctx, false),
        Op::Give(parameter_int(params, 1)?),
    )
}

pub(super) fn parse_unit_set_points(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(
        Target::parse(params, ctx, false),
        Op::Set(parameter_int(params, 1)?),
    )
}

pub(super) fn parse_unit_set_max_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(
        Target::parse(params, ctx, false),
        Op::MaxLevel(parameter_int(params, 1)?),
    )
}

pub(super) fn parse_unit_gain_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, false), Op::GainLevel)
}

pub(super) fn parse_unit_give_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(
        Target::parse(params, ctx, false),
        Op::GiveLevel(parameter_string(params, 1)?),
    )
}

pub(super) fn parse_player_max_spellpoints(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerExperience {
        player: parameter_string(params, 0)?,
        op: PlayerOp::MaxSpellPoints(parameter_int(params, 1)?),
    }))
}

pub(super) fn parse_player_set_ranklevel(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerExperience {
        player: parameter_string(params, 0)?,
        op: PlayerOp::SetRankLevel(parameter_int(params, 1)?),
    }))
}

pub(super) fn parse_revival_at_level(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerExperience {
        player: parameter_string(params, 1)?,
        op: PlayerOp::RevivalAtLevel {
            template: parameter_string(params, 0)?,
            rank: parameter_int(params, 2)?,
        },
    }))
}

enum Check {
    AtLevel(String),
    HasGained,
    RankAtLeast(i32),
}

struct ConditionLevel {
    target: Target,
    check: Check,
}

impl ConditionVerb for ConditionLevel {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let world = tick.state();
        let Some(entity) = self
            .target
            .entities(rules, tick, ids, state)
            .first()
            .copied()
        else {
            return false;
        };
        match &self.check {
            Check::AtLevel(name) => rules
                .level_name_of(world, ids, entity)
                .is_some_and(|held| held.eq_ignore_ascii_case(name)),
            Check::HasGained => {
                let receiver = rules.sink_of(world, ids, entity);
                rules
                    .experience_of(world, ids, receiver)
                    .is_some_and(|(index, _)| index > 0)
            }
            Check::RankAtLeast(rank) => rules
                .level_rank_of(world, ids, entity)
                .is_some_and(|held| compare(i32::from(held), COMPARE_GREATER_EQUAL, *rank)),
        }
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.target.unit_name()
    }
}

fn condition(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    check: Check,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionLevel {
        target: Target::parse(params, ctx, false)?,
        check,
    }))
}

pub(super) fn parse_unit_is_at_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    condition(params, ctx, Check::AtLevel(parameter_string(params, 1)?))
}

pub(super) fn parse_unit_has_gained_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    condition(params, ctx, Check::HasGained)
}

pub(super) fn parse_rank_level(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    condition(params, ctx, Check::RankAtLeast(parameter_int(params, 1)?))
}
