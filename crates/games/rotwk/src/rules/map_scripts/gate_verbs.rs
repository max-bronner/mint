use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{GateAction, Ids, RotwkRules};

struct ActionGate {
    unit: String,
    side: String,
    action: GateAction,
}

impl ActionVerb for ActionGate {
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
        let Some(gate) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) else {
            return Ok(());
        };
        let took = rules.command_gate_entity(tick, ids, gate, self.action)?;
        diag!(
            "script: GATE_{} on {:?} - {}",
            if self.action == GateAction::Open {
                "OPEN"
            } else {
                "CLOSE"
            },
            self.unit,
            if took { "commanded" } else { "refused" }
        );
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ConditionGateIsOpen {
    unit: String,
    side: String,
}

impl ConditionVerb for ConditionGateIsOpen {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .object_named(tick, ids, &state.rt, &self.unit, &self.side)
            .and_then(|gate| rules.gate_is_open(tick.state(), ids, gate))
            .unwrap_or(false)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

fn action(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    action: GateAction,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionGate {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        action,
    }))
}

pub(super) fn parse_open(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    action(params, ctx, GateAction::Open)
}

pub(super) fn parse_close(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    action(params, ctx, GateAction::Close)
}

pub(super) fn parse_is_open(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionGateIsOpen {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
    }))
}
