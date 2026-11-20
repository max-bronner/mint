use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_int, parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState,
    ParseCtx,
};
use crate::rules::terrain_fire::{singleton_of, FireLighting, FireLogicKind, DEFAULT_DAMAGE_ARC};
use crate::rules::{Ids, RotwkRules};

struct ActionChangeBurnRate {
    at: Option<Vec2>,
    radius: f32,
    amount: i32,
}

impl ActionVerb for ActionChangeBurnRate {
    fn run(
        &self,
        _: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(at) = self.at else {
            return Ok(());
        };
        let Some(grid) = singleton_of(tick.state(), ids.fire_grid) else {
            return Ok(());
        };
        #[allow(clippy::cast_precision_loss)]
        let damage = self.amount as f32;
        if let Some(grid) = tick.state_mut().get_mut(ids.fire_grid, grid) {
            grid.light(
                &FireLighting {
                    logic_type: FireLogicKind::IncreaseBurnRate {
                        existing_only: false,
                        arc: false,
                    },
                    radius: self.radius,
                    damage,
                    damage_arc: DEFAULT_DAMAGE_ARC,
                    min_max_burn_rate: None,
                    min_decay: None,
                    max_resistance: u16::MAX,
                },
                (at.x, at.y),
                None,
                &|_, _| false,
            );
        }
        Ok(())
    }
}

pub(super) fn parse_change_burn_rate(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 0)?;
    Some(Arc::new(ActionChangeBurnRate {
        at: ctx.waypoints.get(&waypoint).map(|(position, _)| *position),
        radius: parameter_real(params, 1)?,
        amount: parameter_int(params, 2)?,
    }))
}
