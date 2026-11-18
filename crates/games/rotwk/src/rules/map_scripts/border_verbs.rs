use std::sync::Arc;

use mint_movement::Cell;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::area::Area;
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::rules::border::HumanBarred;
use crate::rules::{Ids, RotwkRules};

struct ActionMapSwitchBorder {
    index: i32,
}

impl ActionVerb for ActionMapSwitchBorder {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Ok(index) = usize::try_from(self.index) else {
            return Ok(());
        };
        let Some(high) = rules.passability.boundary_high(index) else {
            return Ok(());
        };
        if index == crate::rules::border::active_index(tick.state(), ids) {
            return Ok(());
        }
        let (Some(clock), Ok(held)) =
            (crate::clock_entity(tick.state(), ids), u16::try_from(index))
        else {
            return Ok(());
        };
        if index == 0 {
            tick.state_mut().remove(ids.border, clock);
        } else {
            tick.state_mut()
                .insert(ids.border, clock, crate::rules::border::ActiveBorder(held))?;
        }
        crate::diag::diag!(
            "script: MAP_SWITCH_BORDER to {index}: playable up to ({}, {})",
            high.x,
            high.y
        );
        let window = (high != rules.passability.playable_high).then_some((high.x, high.y));
        crate::shroud::set_window(tick, RotwkRules::shroud_ids(ids), window);
        Ok(())
    }
}

pub(super) fn parse_map_switch_border(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionMapSwitchBorder {
        index: parameter_int(params, 0)?,
    }))
}

struct ActionHumanImpassableArea {
    area: Area,
    barred: bool,
}

impl ActionVerb for ActionHumanImpassableArea {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if !self.area.exists() {
            return Ok(());
        }
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(());
        };
        let extent = rules.passability.grid.extent();
        let Ok(cells) = usize::try_from(extent.cell_count()) else {
            return Ok(());
        };
        if tick.state().get(ids.human_barred, clock).is_none() {
            tick.state_mut().insert(
                ids.human_barred,
                clock,
                HumanBarred {
                    cells: vec![0; cells],
                    applied: Vec::new(),
                },
            )?;
        }
        let (low, high) = self.area.bounds();
        let value = if self.barred { 1 } else { 2 };
        let geometry = rules.passability.geometry;
        let Some(barred) = tick.state_mut().get_mut(ids.human_barred, clock) else {
            return Ok(());
        };
        let mut written = 0u32;
        for y in 0..extent.height {
            for x in 0..extent.width {
                let cell = Cell::new(x, y);
                let centre = geometry.centre_of(cell);
                if centre.x < low.x || centre.x > high.x || centre.y < low.y || centre.y > high.y {
                    continue;
                }
                if !self.area.contains(centre) {
                    continue;
                }
                if let Some(held) = extent
                    .index(cell)
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| barred.cells.get_mut(index))
                {
                    *held = value;
                    written = written.saturating_add(1);
                }
            }
        }
        let entry = (self.area.name.clone(), self.barred);
        if barred.applied.last() != Some(&entry) {
            barred.applied.push(entry);
        }
        crate::diag::diag!(
            "script: HUMAN_IMPASSABLE_AREA {:?} {}: {written} cell(s)",
            self.area.name,
            self.barred
        );
        Ok(())
    }
}

pub(super) fn parse_human_impassable_area(
    params: &[mint_sage_map::Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionHumanImpassableArea {
        area: Area::resolve(ctx, &parameter_string(params, 0)?),
        barred: parameter_int(params, 1)? != 0,
    }))
}
