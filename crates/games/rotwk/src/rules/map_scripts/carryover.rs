use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::game::LOCAL_PLAYER;
use crate::rules::{Ids, RotwkRules};

const LOCAL_PLAYER_TEXT: &str = "<Local Player>";

struct ConditionHasDelayed(String);

impl ConditionVerb for ConditionHasDelayed {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        state
            .carryover
            .as_ref()
            .is_some_and(|run| run.carry.has_delayed(&self.0))
    }
}

struct ActionCreateDelayed {
    unit_type: String,
    player: String,
    at: Option<(Vec2, f32)>,
    unit_name: Option<String>,
}

impl ActionVerb for ActionCreateDelayed {
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
        let Some((at, angle)) = self.at else {
            diag!(
                "script: CREATE_DELAYED_CARRYOVER_UNIT_AT_WAYPOINT {:?} names a waypoint this map \
                 does not have - nothing spawned",
                self.unit_type
            );
            return Ok(());
        };
        let owner = if self.player.eq_ignore_ascii_case(LOCAL_PLAYER_TEXT) {
            Some(LOCAL_PLAYER)
        } else {
            rules.resolve_side_player(&self.player)
        };
        let (Some(owner), Some(run)) = (owner, state.carryover.as_mut()) else {
            return Ok(());
        };
        let made =
            rules.create_delayed_carryover(tick, ids, run, &self.unit_type, owner, at, angle)?;
        if let (Some(_), Some(name)) = (made, &self.unit_name) {
            if let Some(entity) = made {
                tick.state_mut().insert(
                    ids.script_name,
                    entity,
                    super::ScriptObjectName(name.clone()),
                )?;
            }
            state.ever_existed.insert(name.clone());
        }
        Ok(())
    }

    fn referenced_template(&self) -> Option<&str> {
        Some(&self.unit_type)
    }
}

struct ActionRevivalEntry {
    unit_type: String,
}

impl ActionVerb for ActionRevivalEntry {
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
        let Some(run) = state.carryover.as_mut() else {
            return Ok(());
        };
        rules.revival_entry_from_carryover(tick, ids, run, &self.unit_type)
    }

    fn referenced_template(&self) -> Option<&str> {
        Some(&self.unit_type)
    }
}

pub(super) fn parse_has_delayed(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionHasDelayed(parameter_string(params, 0)?)))
}

pub(super) fn parse_create_delayed(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 2)?;
    Some(Arc::new(ActionCreateDelayed {
        unit_type: parameter_string(params, 0)?,
        player: parameter_string(params, 1)?,
        at: ctx.waypoints.get(&waypoint).copied(),
        unit_name: parameter_string(params, 3).filter(|name| !name.is_empty()),
    }))
}

pub(super) fn parse_revival_entry(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRevivalEntry {
        unit_type: parameter_string(params, 0)?,
    }))
}
