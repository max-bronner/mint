use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::masks::PlayerMask;
use super::{
    parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

const MAX_RECORDS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TriggeredPowers(Vec<(PlayerId, String, Entity)>);

impl TriggeredPowers {
    pub(super) fn push(&mut self, player: PlayerId, power: &str, source: Entity) {
        if self.0.len() >= MAX_RECORDS {
            self.0.remove(0);
        }
        self.0.push((player, power.to_owned(), source));
    }

    fn find(&self, player: PlayerId, power: &str) -> Option<usize> {
        self.0
            .iter()
            .position(|(who, name, _)| *who == player && name.eq_ignore_ascii_case(power))
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (player, power, source) in &self.0 {
            hasher.write_u8(player.get());
            hasher.write_bytes(power.as_bytes());
            hasher.write_u8(0);
            source.hash_into(hasher);
        }
    }
}

pub(crate) fn note_special_power_triggered(
    rules: &RotwkRules,
    tick: &mut Tick<'_>,
    ids: Ids,
    player: PlayerId,
    power: &str,
    source: Entity,
) {
    if !rules
        .map_scripts
        .as_ref()
        .is_some_and(|engine| engine.reads_triggered_powers())
    {
        return;
    }
    let Some(anchor) = tick
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(entity, _)| entity))
    else {
        return;
    };
    if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
        state.rt.triggered_powers.push(player, power, source);
    }
}

impl RotwkRules {
    fn power_caster(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        power: &str,
    ) -> Option<Entity> {
        let world = tick.state();
        world
            .store(ids.owner)?
            .iter()
            .filter(|(entity, owner)| {
                owner.0 == player && world.is_live(*entity) && !world.contains(ids.doomed, *entity)
            })
            .map(|(entity, _)| entity)
            .find(|&entity| {
                world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| entry.record.power(power).is_some())
            })
    }

    pub(super) fn script_fire_power_at(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        mask: PlayerMask,
        power: &str,
        at: Vec2,
    ) -> Result<bool> {
        for player in mask.players() {
            let Some(caster) = self.power_caster(tick, ids, player, power) else {
                continue;
            };
            self.fire_power_scripted(tick, ids, caster, power, None, Some((at.x, at.y)))?;
            state.rt.triggered_powers.push(player, power, caster);
            diag!(
                "script: {power} fired for player {} from entity {} at ({:.0}, {:.0})",
                player.get(),
                caster.index(),
                at.x,
                at.y
            );
            return Ok(true);
        }
        diag!("script: {power} has no caster among the named players");
        Ok(false)
    }
}

struct FireAtWaypoint {
    player_text: String,
    power: String,
    at: Option<Vec2>,
}

impl ActionVerb for FireAtWaypoint {
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
        let Some(at) = self.at else {
            return Ok(());
        };
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        rules.script_fire_power_at(tick, ids, state, mask, &self.power, at)?;
        Ok(())
    }
}

struct FireOnTeam {
    player_text: String,
    power: String,
    team: String,
    side: String,
}

impl ActionVerb for FireOnTeam {
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
        let Some(at) = rules
            .peek_team(state, &self.team, &self.side)
            .filter(|&team| rules.team_has_units(tick, ids, state, team))
            .and_then(|team| rules.team_position(tick, ids, state, team))
        else {
            return Ok(());
        };
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        rules.script_fire_power_at(tick, ids, state, mask, &self.power, at)?;
        Ok(())
    }
}

struct FireOnNearest {
    player_text: String,
    power: String,
    types: String,
    team: String,
    side: String,
    owners_text: Option<String>,
}

impl ActionVerb for FireOnNearest {
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
        let Some(origin) = rules
            .peek_team(state, &self.team, &self.side)
            .filter(|&team| rules.team_has_units(tick, ids, state, team))
            .and_then(|team| rules.team_position(tick, ids, state, team))
        else {
            return Ok(());
        };
        let names = state.rt.lists.types_of(&self.types);
        let found = match &self.owners_text {
            Some(owners_text) => rules
                .player_mask(&state.rt, owners_text, player)
                .players()
                .find_map(|owner| {
                    rules.nearest_of_types(tick, ids, &names, origin, Some(owner), false)
                }),
            None => rules.nearest_of_types(tick, ids, &names, origin, None, false),
        };
        let Some(at) = found.and_then(|object| rules.position_of_entity(tick, ids, object)) else {
            return Ok(());
        };
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        rules.script_fire_power_at(tick, ids, state, mask, &self.power, at)?;
        Ok(())
    }
}

struct ConditionPlayerTriggered {
    player_text: String,
    power: String,
}

impl ConditionVerb for ConditionPlayerTriggered {
    fn reads_triggered_powers(&self) -> bool {
        true
    }

    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        rules
            .player_mask(&state.rt, &self.player_text, player)
            .any(|who| state.rt.triggered_powers.find(who, &self.power).is_some())
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for who in mask.players() {
            if let Some(index) = state.rt.triggered_powers.find(who, &self.power) {
                state.rt.triggered_powers.0.remove(index);
                return true;
            }
        }
        false
    }
}

pub(super) fn parse_fire_at_waypoint(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 2)?;
    Some(Arc::new(FireAtWaypoint {
        player_text: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
        at: ctx.waypoints.get(&waypoint).map(|(position, _)| *position),
    }))
}

pub(super) fn parse_fire_on_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(FireOnTeam {
        player_text: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
        team: parameter_string(params, 2)?,
        side: ctx.side.to_owned(),
    }))
}

fn parse_fire_on_nearest_with(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    owners_text: Option<String>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(FireOnNearest {
        player_text: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
        types: parameter_string(params, 2)?,
        team: parameter_string(params, 3)?,
        side: ctx.side.to_owned(),
        owners_text,
    }))
}

pub(super) fn parse_fire_on_nearest(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_fire_on_nearest_with(params, ctx, None)
}

pub(super) fn parse_fire_on_nearest_by_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let owners_text = parameter_string(params, 4)?;
    parse_fire_on_nearest_with(params, ctx, Some(owners_text))
}

pub(super) fn parse_player_triggered(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionPlayerTriggered {
        player_text: parameter_string(params, 0)?,
        power: parameter_string(params, 1)?,
    }))
}
