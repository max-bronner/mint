use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
    ScriptObjectName,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

enum Place {
    Coord(Vec2, f32),
    Beside {
        waypoint: Option<Vec2>,
        object: String,
    },
}

struct ActionSpawn {
    name: String,
    template: String,
    team: String,
    side: String,
    place: Place,
    verb: &'static str,
}

impl ActionVerb for ActionSpawn {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let named = !self.name.is_empty();
        if named
            && rules
                .object_named(tick, ids, &state.rt, &self.name, &self.side)
                .is_some_and(|old| !tick.state().contains(ids.doomed, old))
        {
            diag!(
                "script: {} {:?} already exists - nothing made",
                self.verb,
                self.name
            );
            return Ok(());
        }
        let Some(team) = rules.get_team(state, &self.team, &self.side, true) else {
            diag!(
                "script: {} names team {:?}, which does not exist",
                self.verb,
                self.team
            );
            return Ok(());
        };
        let Some(owner) = rules.team_owner(engine, state, team) else {
            return Ok(());
        };
        let (position, angle) = match &self.place {
            Place::Coord(at, angle) => (*at, *angle),
            Place::Beside { waypoint, object } => {
                let beside = waypoint.or_else(|| {
                    let origin = rules.team_position(tick, ids, state, team)?;
                    let found = rules.nearest_of_types(
                        tick,
                        ids,
                        std::slice::from_ref(object),
                        origin,
                        None,
                        false,
                    )?;
                    rules.position_of_entity(tick, ids, found)
                });
                let Some(at) = beside else {
                    diag!("script: {} found no place beside {object:?}", self.verb);
                    return Ok(());
                };
                (at, 0.0)
            }
        };
        let Some((entity, _)) =
            rules.spawn_script_unit(tick, ids, &self.template, position, angle, owner)?
        else {
            return Ok(());
        };
        if named {
            tick.state_mut().insert(
                ids.script_name,
                entity,
                ScriptObjectName(self.name.clone()),
            )?;
            state.ever_existed.insert(self.name.clone());
        }
        state.rt.teams.add_member(team, entity);
        diag!(
            "script: {} made {:?} for player {}",
            self.verb,
            self.template,
            owner.get()
        );
        Ok(())
    }

    fn referenced_template(&self) -> Option<&str> {
        Some(&self.template)
    }
}

fn coord(params: &[Parameter], index: usize) -> Option<Vec2> {
    match params.get(index)? {
        Parameter::Coord3D { x, y, .. } => Some(Vec2::new(*x, *y)),
        Parameter::Value { .. } => None,
    }
}

fn spawn(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    name_at: Option<usize>,
    verb: &'static str,
) -> Option<Arc<dyn ActionVerb>> {
    let shift = usize::from(name_at.is_some());
    let name = match name_at {
        Some(index) => parameter_string(params, index)?,
        None => String::new(),
    };
    Some(Arc::new(ActionSpawn {
        name,
        template: parameter_string(params, shift)?,
        team: parameter_string(params, shift + 1)?,
        side: ctx.side.to_owned(),
        place: Place::Coord(
            coord(params, shift + 2)?,
            parameter_real(params, shift + 3)?,
        ),
        verb,
    }))
}

pub(super) fn parse_create_object(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    spawn(params, ctx, None, "CREATE_OBJECT")
}

pub(super) fn parse_spawn_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    spawn(
        params,
        ctx,
        Some(0),
        "UNIT_SPAWN_NAMED_LOCATION_ORIENTATION",
    )
}

pub(super) fn parse_create_beside_object(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let place = parameter_string(params, 3)?;
    Some(Arc::new(ActionSpawn {
        name: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        team: parameter_string(params, 2)?,
        side: ctx.side.to_owned(),
        place: Place::Beside {
            waypoint: ctx.waypoints.get(&place).map(|(at, _)| *at),
            object: place,
        },
        verb: "CREATE_NAMED_ON_TEAM_AT_OBJECTTYPE",
    }))
}

struct SetRefToNearestOfTeamsPlayer {
    types: String,
    team: String,
    reference: String,
    side: String,
}

impl ActionVerb for SetRefToNearestOfTeamsPlayer {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            diag!("script: {:?} names no team {:?}", self.reference, self.team);
            return Ok(());
        };
        let (Some(origin), Some(owner)) = (
            rules.team_position(tick, ids, state, team),
            rules.team_owner(engine, state, team),
        ) else {
            diag!(
                "script: {:?} - team {:?} has no live member or no player",
                self.reference,
                self.team
            );
            return Ok(());
        };
        let names = state.rt.lists.types_of(&self.types);
        let found = rules.nearest_of_types(tick, ids, &names, origin, Some(owner), false);
        diag!(
            "script: reference {:?} -> {:?} (nearest {:?} of player {} to {origin:?})",
            self.reference,
            found.map(Entity::index),
            self.types,
            owner.get()
        );
        if let Some(found) = found {
            state.rt.objects.set(&self.reference, &self.side, found);
        }
        Ok(())
    }
}

pub(super) fn parse_set_ref_nearest_of_teams_player(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(SetRefToNearestOfTeamsPlayer {
        types: parameter_string(params, 0)?,
        team: parameter_string(params, 1)?,
        reference: parameter_string(params, 2)?,
        side: ctx.side.to_owned(),
    }))
}
