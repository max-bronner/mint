use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Tick};

use super::relations::Relation;
use super::{parameter_int, parameter_string, ConditionVerb, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

impl RotwkRules {
    fn last_attacker_of(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> Option<Entity> {
        let world = tick.state();
        let source = world.get(ids.last_attacker, victim)?.source;
        world.is_live(source).then_some(source)
    }

    fn attacker_of_types(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        victim: Entity,
        types: &str,
    ) -> bool {
        let Some(attacker) = self.last_attacker_of(tick, ids, victim) else {
            return false;
        };
        self.entity_in_types(tick, ids, state, attacker, types)
    }

    fn entity_in_types(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        entity: Entity,
        types: &str,
    ) -> bool {
        let names = if state.rt.lists.is_list(types) {
            state.rt.lists.types_of(types)
        } else {
            vec![types.to_owned()]
        };
        tick.state().get(ids.kind, entity).is_some_and(|kind| {
            names
                .iter()
                .any(|name| self.kinds.kind(name) == Some(*kind))
        })
    }

    fn script_vision(&self, tick: &Tick<'_>, ids: Ids, unit: Entity) -> f32 {
        let world = tick.state();
        let range = world
            .get(ids.kind, unit)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .filter(|range| range.is_finite() && *range > 0.0)
            .unwrap_or(0.0);
        let bonus = world
            .get(ids.bonuses, unit)
            .map_or(0.0, |held| held.0.vision);
        range * (1.0 + bonus).max(0.0)
    }

    pub(super) fn team_sees_enemy(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        members: &[Entity],
    ) -> bool {
        members.iter().any(|&member| {
            !tick.state().contains(ids.doomed, member)
                && self.unit_sees(
                    tick,
                    ids,
                    state,
                    member,
                    Sought::Relation(0),
                    super::masks::PlayerMask::ALL,
                )
        })
    }

    fn unit_sees(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        unit: Entity,
        sought: Sought<'_>,
        players: super::masks::PlayerMask,
    ) -> bool {
        let world = tick.state();
        let (Some(at), Some(own)) = (
            world.get(ids.body, unit).map(|body| body.0.position),
            world.get(ids.owner, unit).map(|owner| owner.0),
        ) else {
            return false;
        };
        let reach = self.script_vision(tick, ids, unit);
        let reach_squared = reach * reach;
        let off_map = self.off_map(world, ids, at);
        let Some(owners) = world.store(ids.owner) else {
            return false;
        };
        owners.iter().any(|(other, holder)| {
            if !players.contains(holder.0)
                || other == unit
                || !world.is_live(other)
                || world.contains(ids.doomed, other)
                || world.contains(ids.member, other)
                || world.contains(ids.contained, other)
                || self.hidden_undetected(world, ids, other)
            {
                return false;
            }
            let Some(there) = world.get(ids.body, other).map(|body| body.0.position) else {
                return false;
            };
            if self.off_map(world, ids, there) != off_map {
                return false;
            }
            let (dx, dy) = (there.x - at.x, there.y - at.y);
            if dx * dx + dy * dy > reach_squared {
                return false;
            }
            match sought {
                Sought::Relation(wanted) => {
                    Relation::from_param(wanted)
                        == Some(self.script_relation(&state.rt, own, holder.0))
                        && !self.kind_skipped(tick, ids, other, &["INERT", "MOVE_ONLY"])
                }
                Sought::Types(types) => self.entity_in_types(tick, ids, state, other, types),
            }
        })
    }
}

#[derive(Clone, Copy)]
enum Sought<'a> {
    Relation(i32),
    Types(&'a str),
}

enum Attacker {
    Player(String),
    Types(String),
}

struct ConditionNamedAttacked {
    unit: String,
    by: Attacker,
}

impl ConditionVerb for ConditionNamedAttacked {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let Some(unit) = rules.object_named_or_dying(tick, ids, &state.rt, &self.unit, "") else {
            return false;
        };
        match &self.by {
            Attacker::Player(text) => {
                let mask = rules.player_mask(&state.rt, text, player);
                state
                    .rt
                    .standing
                    .last_hitter(unit)
                    .is_some_and(|hitter| mask.contains(hitter))
                    || rules
                        .last_attacker_of(tick, ids, unit)
                        .and_then(|attacker| tick.state().get(ids.owner, attacker))
                        .is_some_and(|owner| mask.contains(owner.0))
            }
            Attacker::Types(types) => rules.attacker_of_types(tick, ids, state, unit, types),
        }
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ConditionTeamAttacked {
    team: String,
    side: String,
    by: Attacker,
}

impl ConditionVerb for ConditionTeamAttacked {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let members = rules.subject_members(tick, ids, state, &self.team, Some(&self.side));
        match &self.by {
            Attacker::Player(text) => {
                let wanted = rules.player_mask(&state.rt, text, player).single();
                members.into_iter().any(|member| {
                    wanted.is_some() && state.rt.standing.last_hitter(member) == wanted
                })
            }
            Attacker::Types(types) => members
                .into_iter()
                .any(|member| rules.attacker_of_types(tick, ids, state, member, types)),
        }
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

struct ConditionSighted {
    subject: String,
    side: Option<String>,
    relation: Option<i32>,
    types: Option<String>,
    player_text: String,
}

impl ConditionVerb for ConditionSighted {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let players = rules.player_mask(&state.rt, &self.player_text, player);
        let sought = match (&self.types, self.relation) {
            (Some(types), _) => Sought::Types(types),
            (None, Some(relation)) => Sought::Relation(relation),
            (None, None) => return false,
        };
        rules
            .subject_members(tick, ids, state, &self.subject, self.side.as_deref())
            .into_iter()
            .any(|unit| rules.unit_sees(tick, ids, state, unit, sought, players))
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.side.is_none().then_some(self.subject.as_str())
    }

    fn team_names(&self) -> Vec<String> {
        if self.side.is_some() {
            vec![self.subject.clone()]
        } else {
            Vec::new()
        }
    }
}

fn attacked(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    by: impl FnOnce(String) -> Attacker,
) -> Option<Arc<dyn ConditionVerb>> {
    let subject = parameter_string(params, 0)?;
    let by = by(parameter_string(params, 1)?);
    Some(if named {
        Arc::new(ConditionNamedAttacked { unit: subject, by })
    } else {
        Arc::new(ConditionTeamAttacked {
            team: subject,
            side: ctx.side.to_owned(),
            by,
        })
    })
}

pub(super) fn parse_named_attacked_by_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    attacked(params, ctx, true, Attacker::Player)
}

pub(super) fn parse_team_attacked_by_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    attacked(params, ctx, false, Attacker::Player)
}

pub(super) fn parse_named_attacked_by_types(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    attacked(params, ctx, true, Attacker::Types)
}

pub(super) fn parse_team_attacked_by_types(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    attacked(params, ctx, false, Attacker::Types)
}

fn sighted(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    typed: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionSighted {
        subject: parameter_string(params, 0)?,
        side: (!named).then(|| ctx.side.to_owned()),
        relation: if typed {
            None
        } else {
            Some(parameter_int(params, 1)?)
        },
        types: if typed {
            Some(parameter_string(params, 1)?)
        } else {
            None
        },
        player_text: parameter_string(params, 2)?,
    }))
}

pub(super) fn parse_enemy_sighted(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    sighted(params, ctx, true, false)
}

pub(super) fn parse_enemy_sighted_by_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    sighted(params, ctx, false, false)
}

pub(super) fn parse_type_sighted(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    sighted(params, ctx, true, true)
}
