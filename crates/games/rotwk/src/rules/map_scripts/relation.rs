use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::relations::{Relation, ATTITUDE_PEACEFUL};
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};

enum Op {
    PlayerToPlayer { from: String, to: String },
    TeamToTeam { from: String, to: String },
    TeamToPlayer { team: String, player: String },
    PlayerToTeam { player: String, team: String },
    RemoveAll { team: String },
    NamedAttitude { unit: String },
    TeamAttitude { team: String },
}

struct ActionRelation {
    op: Op,
    side: String,
    value: i32,
}

impl ActionVerb for ActionRelation {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let side = self.side.as_str();
        let relation = Relation::from_param(self.value);
        match &self.op {
            Op::PlayerToPlayer { from, to } => {
                let from = rules.player_mask(&state.rt, from, player).single();
                let to = rules.player_mask(&state.rt, to, player).single();
                if let (Some(from), Some(to), Some(relation)) = (from, to, relation) {
                    state.rt.relations.set_player_player(from, to, relation);
                    crate::diag::diag!(
                        "script: PLAYER_RELATES_PLAYER {from:?} -> {to:?} = {relation:?}"
                    );
                }
            }
            Op::TeamToTeam { from, to } => {
                let from = rules.get_team(state, from, side, false);
                let to = rules.get_team(state, to, side, false);
                if let (Some(from), Some(to), Some(relation)) = (from, to, relation) {
                    state.rt.relations.set_team_team(from, to, relation);
                    crate::diag::diag!("script: TEAM_SET_OVERRIDE_RELATION_TO_TEAM {from:?} -> {to:?} = {relation:?}");
                }
            }
            Op::TeamToPlayer { team, player } => {
                let team = rules.get_team(state, team, side, false);
                let player = rules.resolve_side_player(player);
                if let (Some(team), Some(player), Some(relation)) = (team, player, relation) {
                    state.rt.relations.set_team_player(team, player, relation);
                    crate::diag::diag!("script: TEAM_SET_OVERRIDE_RELATION_TO_PLAYER {team:?} -> {player:?} = {relation:?}");
                }
            }
            Op::PlayerToTeam { player, team } => {
                let player = rules.resolve_side_player(player);
                let team = rules.get_team(state, team, side, false);
                if let (Some(player), Some(team), Some(relation)) = (player, team, relation) {
                    state.rt.relations.set_player_team(player, team, relation);
                    crate::diag::diag!("script: PLAYER_SET_OVERRIDE_RELATION_TO_TEAM {player:?} -> {team:?} = {relation:?}");
                }
            }
            Op::RemoveAll { team } => {
                if let Some(team) = rules.get_team(state, team, side, false) {
                    state.rt.relations.remove_team_overrides(team, None);
                }
            }
            Op::NamedAttitude { unit } => {
                if let Some(entity) = rules.object_named(tick, ids, &state.rt, unit, side) {
                    if set_attitude(tick, ids, state, entity, self.value)
                        && self.value == i32::from(ATTITUDE_PEACEFUL)
                    {
                        rules.stop_unit(tick, ids, entity);
                    }
                }
            }
            Op::TeamAttitude { team } => {
                if let Some(team) = rules.get_team(state, team, side, false) {
                    for member in rules.live_members(tick, state, team) {
                        if set_attitude(tick, ids, state, member, self.value)
                            && self.value == i32::from(ATTITUDE_PEACEFUL)
                        {
                            rules.stop_unit(tick, ids, member);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.op {
            Op::NamedAttitude { unit } => Some(unit),
            _ => None,
        }
    }
}

fn set_attitude(
    tick: &mut Tick<'_>,
    ids: Ids,
    state: &mut MapScriptState,
    entity: Entity,
    value: i32,
) -> bool {
    let stamp = state.rt.teams.stamp();
    if !state.rt.attitudes.set(entity, value, stamp) {
        return false;
    }
    let anchor = tick
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(anchor, _)| anchor));
    if let Some(committed) =
        anchor.and_then(|anchor| tick.state_mut().get_mut(ids.map_script_state, anchor))
    {
        let stamp = committed.rt.teams.stamp();
        committed.rt.attitudes.set(entity, value, stamp);
    }
    true
}

fn relation_verb(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    op: impl FnOnce(String, String) -> Op,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRelation {
        op: op(parameter_string(params, 0)?, parameter_string(params, 1)?),
        side: ctx.side.to_owned(),
        value: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_player_relates_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    relation_verb(params, ctx, |from, to| Op::PlayerToPlayer { from, to })
}

pub(super) fn parse_team_relation_to_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    relation_verb(params, ctx, |from, to| Op::TeamToTeam { from, to })
}

pub(super) fn parse_team_relation_to_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    relation_verb(params, ctx, |team, player| Op::TeamToPlayer {
        team,
        player,
    })
}

pub(super) fn parse_player_relation_to_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    relation_verb(params, ctx, |player, team| Op::PlayerToTeam {
        player,
        team,
    })
}

pub(super) fn parse_team_remove_all_relations(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRelation {
        op: Op::RemoveAll {
            team: parameter_string(params, 0)?,
        },
        side: ctx.side.to_owned(),
        value: 0,
    }))
}

pub(super) fn parse_named_set_attitude(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRelation {
        op: Op::NamedAttitude {
            unit: parameter_string(params, 0)?,
        },
        side: ctx.side.to_owned(),
        value: parameter_int(params, 1)?,
    }))
}

pub(super) fn parse_team_set_attitude(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRelation {
        op: Op::TeamAttitude {
            team: parameter_string(params, 0)?,
        },
        side: ctx.side.to_owned(),
        value: parameter_int(params, 1)?,
    }))
}
