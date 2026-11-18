use std::sync::Arc;

use mint_sage_ini::Stance;
use mint_sage_map::Parameter;
use mint_sim::{PlayerId, Tick};

use super::cinematic::simple;
use super::{parameter_int, parameter_real, parameter_string, ConditionVerb, MapScriptState};
use super::{ActionVerb, ParseCtx};
use crate::rules::command::UiPanel;
use crate::rules::{Ids, RotwkRules};

type Action = Option<Arc<dyn ActionVerb>>;
type Condition = Option<Arc<dyn ConditionVerb>>;

pub(super) fn parse_enable_spell_store(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_spell_store_enabled(true))
}

pub(super) fn parse_disable_spell_store(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_spell_store_enabled(false))
}

pub(super) fn parse_enable_objectives(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_objectives_enabled(true))
}

pub(super) fn parse_disable_objectives(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_objectives_enabled(false))
}

pub(super) fn parse_close_objectives(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.close_objectives())
}

pub(super) fn parse_enable_planning(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_planning_allowed(true))
}

pub(super) fn parse_disable_planning(_: &[Parameter], _: &ParseCtx<'_>) -> Action {
    simple(|_, _, _, state| state.ui.set_planning_allowed(false))
}

struct PanelOpen(UiPanel);

impl ConditionVerb for PanelOpen {
    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .script_local_player
            .is_some_and(|local| state.ui.panel_open(local, self.0))
    }
}

pub(super) fn parse_spell_store_open(_: &[Parameter], _: &ParseCtx<'_>) -> Condition {
    Some(Arc::new(PanelOpen(UiPanel::SpellStore)))
}

pub(super) fn parse_objectives_open(_: &[Parameter], _: &ParseCtx<'_>) -> Condition {
    Some(Arc::new(PanelOpen(UiPanel::Objectives)))
}

pub(super) fn parse_stance_submenu_open(_: &[Parameter], _: &ParseCtx<'_>) -> Condition {
    Some(Arc::new(PanelOpen(UiPanel::StanceSubMenu)))
}

struct ScrollDistance(f32);

impl ConditionVerb for ScrollDistance {
    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .script_local_player
            .is_some_and(|local| state.ui.scroll_distance(local) > self.0)
    }
}

pub(super) fn parse_camera_scroll_distance(params: &[Parameter], _: &ParseCtx<'_>) -> Condition {
    Some(Arc::new(ScrollDistance(parameter_real(params, 0)?)))
}

struct FormationOrderIssued;

impl ConditionVerb for FormationOrderIssued {
    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .script_local_player
            .is_some_and(|local| state.ui.formation_order_issued(local))
    }
}

pub(super) fn parse_formation_order_issued(_: &[Parameter], _: &ParseCtx<'_>) -> Condition {
    Some(Arc::new(FormationOrderIssued))
}

struct UnitUsingStance {
    unit: String,
    side: String,
    stance: Stance,
}

impl ConditionVerb for UnitUsingStance {
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
            .and_then(|entity| rules.stance_of(tick.state(), ids, entity))
            .is_some_and(|held| held == self.stance)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

pub(super) fn parse_unit_using_stance(params: &[Parameter], ctx: &ParseCtx<'_>) -> Condition {
    let name = parameter_string(params, 1)?;
    let stance = Stance::from_name(&name)
        .or_else(|| Stance::from_engine_ordinal(parameter_int(params, 1)?))?;
    Some(Arc::new(UnitUsingStance {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        stance,
    }))
}
