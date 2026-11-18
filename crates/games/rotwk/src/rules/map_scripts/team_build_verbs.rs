use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::counters::SideKey;
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};

struct ActionBuildTeam {
    team: String,
    side: String,
}

impl ActionVerb for ActionBuildTeam {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let key = SideKey::qualify(&self.team, &self.side);
        let Some(proto) = engine.team_prototypes().find(&key.side, &key.name) else {
            return Ok(());
        };
        let Some(player) = rules.proto_controller(engine, state, proto) else {
            return Ok(());
        };
        if state.rt.builders.get(player).is_none() {
            return Ok(());
        }
        rules.build_specific_ai_team(tick, ids, engine, state, player, proto, true, kills)
    }
}

pub(super) fn parse_build_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionBuildTeam {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
    }))
}

struct ActionTeamSeconds {
    player_text: String,
    seconds: i32,
}

impl ActionVerb for ActionTeamSeconds {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for target in mask.players() {
            if let Some(builder) = state.rt.builders.get_mut(target) {
                builder.team_seconds = self.seconds;
            }
        }
        Ok(())
    }
}

pub(super) fn parse_set_base_construction_speed(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamSeconds {
        player_text: parameter_string(params, 0)?,
        seconds: parameter_int(params, 1)?,
    }))
}
