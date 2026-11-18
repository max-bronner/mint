use std::sync::Arc;

use mint_sim::{Entity, Frame, PlayerId, Result, Tick};

use super::{
    parameter_real, parameter_string, ActionVerb, ConditionVerb, ForcedModelCondition,
    MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

struct ConditionNamedDestroyed(String);
impl ConditionVerb for ConditionNamedDestroyed {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules.named_entity_destroyed(tick, ids, state, &self.0)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.0)
    }
}

struct ActionKill(String);
impl ActionVerb for ActionKill {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.0, "") else {
            diag!(
                "script: NAMED_KILL names {:?}, which no live entity answers to",
                self.0
            );
            return Ok(());
        };
        if let Some(pair) =
            rules.kill(tick, ids, entity, None, crate::combat::DeathNaming::Normal)?
        {
            diag!("script: NAMED_KILL killed {:?}", self.0);
            kills.push(pair);
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.0)
    }
}

struct ActionSetModelCondition {
    unit_name: String,
    condition: String,
    frames: Option<i32>,
}
impl ActionVerb for ActionSetModelCondition {
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
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit_name, "") else {
            diag!(
                "script: UNIT_SET_MODELCONDITION_FOR_DURATION names {:?}, which no live entity \
                 answers to",
                self.unit_name
            );
            return Ok(());
        };
        let until = self.frames.map(|frames| {
            let add = u64::try_from(frames).unwrap_or(0);
            Frame::new(tick.frame().get().saturating_add(add))
        });
        tick.state_mut().insert(
            ids.forced_model_condition,
            entity,
            ForcedModelCondition {
                name: self.condition.clone(),
                until,
            },
        )?;
        diag!(
            "script: UNIT_SET_MODELCONDITION_FOR_DURATION set {:?} on {:?}, until {until:?}",
            self.condition,
            self.unit_name
        );
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit_name)
    }
}

pub(super) fn parse_named_destroyed(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionNamedDestroyed(parameter_string(
        params, 0,
    )?)))
}

pub(super) fn parse_kill(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionKill(parameter_string(params, 0)?)))
}

pub(super) fn parse_set_model_condition(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let unit_name = parameter_string(params, 0)?;
    let condition = parameter_string(params, 1)?;
    let seconds = parameter_real(params, 2)?;
    let frames = (seconds > 0.0).then(|| {
        #[allow(clippy::cast_possible_truncation)]
        let frames = (seconds * mint_sim::SIMULATION_HZ as f32).ceil() as i32;
        frames
    });
    Some(Arc::new(ActionSetModelCondition {
        unit_name,
        condition,
        frames,
    }))
}
