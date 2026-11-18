use std::sync::Arc;

use mint_sage_campaign::MissionEnd;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

struct ActionMissionEnd(MissionEnd);

impl ActionVerb for ActionMissionEnd {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state.mission_end.get_or_insert(self.0);
        Ok(())
    }
}

struct ActionVictoryScreen;

impl ActionVerb for ActionVictoryScreen {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state.cine.input_disabled = true;
        state.cine.input_seq = state.cine.input_seq.wrapping_add(1);
        state.victory_screen_seq = state.victory_screen_seq.wrapping_add(1);
        Ok(())
    }
}

pub(super) fn parse_victory(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionMissionEnd(MissionEnd::Victory)))
}

pub(super) fn parse_defeat(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionMissionEnd(MissionEnd::Defeat)))
}

pub(super) fn parse_victory_screen(
    _: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionVictoryScreen))
}
