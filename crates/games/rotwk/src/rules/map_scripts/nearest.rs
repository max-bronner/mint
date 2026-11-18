use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::cinematic::mean_position;
use super::team::TeamId;
use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TeamWard {
    Gone,
    Empty,
    At(Vec2),
}

impl RotwkRules {
    pub(crate) fn team_ward(&self, world: &mint_sim::World, ids: Ids, team: TeamId) -> TeamWard {
        let Some(instance) = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().find_map(|(_, state)| state.rt.teams.get(team)))
        else {
            return TeamWard::Gone;
        };
        let positions: Vec<Vec2> = instance
            .members()
            .iter()
            .filter(|&&member| world.is_live(member))
            .filter_map(|&member| world.get(ids.body, member).map(|body| body.0.position))
            .collect();
        mean_position(&positions).map_or(TeamWard::Empty, TeamWard::At)
    }

    pub(super) fn team_position(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> Option<Vec2> {
        let positions: Vec<Vec2> = self
            .live_members(tick, state, team)
            .into_iter()
            .filter_map(|member| self.position_of_entity(tick, ids, member))
            .collect();
        mean_position(&positions)
    }

    pub(super) fn nearest_of_types(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        names: &[String],
        from: Vec2,
        owner: Option<PlayerId>,
        unnamed_only: bool,
    ) -> Option<Entity> {
        let wanted: Vec<_> = names
            .iter()
            .filter_map(|name| self.kinds.kind(name))
            .collect();
        let world = tick.state();
        world
            .store(ids.kind)?
            .iter()
            .filter(|(entity, kind)| {
                world.is_live(*entity)
                    && !world.contains(ids.doomed, *entity)
                    && owner.is_none_or(|owner| {
                        world.get(ids.owner, *entity).is_some_and(|o| o.0 == owner)
                    })
                    && !(unnamed_only && world.contains(ids.script_name, *entity))
                    && wanted
                        .iter()
                        .any(|&want| self.kinds.equivalent(want, **kind))
            })
            .filter_map(|(entity, _)| {
                let at = self.position_of_entity(tick, ids, entity)?;
                let (dx, dy) = (at.x - from.x, at.y - from.y);
                Some((dx * dx + dy * dy, entity))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.index().cmp(&b.1.index())))
            .map(|(_, entity)| entity)
    }
}

struct ActionSetRefToNearestTeam {
    types: String,
    player_text: String,
    team: String,
    reference: String,
    side: String,
    unnamed_only: bool,
}

impl ActionVerb for ActionSetRefToNearestTeam {
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
        let Some(owner) = rules
            .player_mask(&state.rt, &self.player_text, player)
            .single()
        else {
            return Ok(());
        };
        let Some(origin) = rules
            .peek_team(state, &self.team, &self.side)
            .and_then(|team| rules.team_position(tick, ids, state, team))
        else {
            return Ok(());
        };
        let names = state.rt.lists.types_of(&self.types);
        let Some(found) =
            rules.nearest_of_types(tick, ids, &names, origin, Some(owner), self.unnamed_only)
        else {
            return Ok(());
        };
        state.rt.objects.set(&self.reference, &self.side, found);
        diag!(
            "script: {:?} now names entity {}",
            self.reference,
            found.index()
        );
        Ok(())
    }
}

fn parse(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    unnamed_only: bool,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetRefToNearestTeam {
        types: parameter_string(params, 0)?,
        player_text: parameter_string(params, 1)?,
        team: parameter_string(params, 2)?,
        reference: parameter_string(params, 3)?,
        side: ctx.side.to_owned(),
        unnamed_only,
    }))
}

pub(super) fn parse_of_type(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse(params, ctx, false)
}

pub(super) fn parse_of_unnamed_type(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse(params, ctx, true)
}
