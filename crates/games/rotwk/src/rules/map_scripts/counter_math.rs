use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::counters::SideKey;
use super::{parameter_int, side_key, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

struct ActionCounterMath {
    dest: SideKey,
    op: i32,
    source: SideKey,
}

impl ActionVerb for ActionCounterMath {
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
        let rhs = state.counters.get(&self.source).map_or(0, |c| c.value);
        let dest = state.counters.entry(self.dest.clone()).or_default();
        let lhs = dest.value;
        let result = match self.op {
            0 => Some(lhs.wrapping_add(rhs)),
            1 => Some(lhs.wrapping_sub(rhs)),
            2 => Some(lhs.wrapping_mul(rhs)),
            3 => lhs.checked_div(rhs),
            _ => None,
        };
        if let Some(value) = result {
            dest.value = value;
        }
        Ok(())
    }
}

pub(super) fn parse_counter_math(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionCounterMath {
        dest: side_key(params, 0, ctx)?,
        op: parameter_int(params, 1)?,
        source: side_key(params, 2, ctx)?,
    }))
}
