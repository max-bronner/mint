use std::sync::Arc;

use mint_sage_economy::Money;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};

struct ActionPlayerSetMoney {
    player_text: String,
    amount: i64,
}

impl ActionVerb for ActionPlayerSetMoney {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let economy = RotwkRules::economy_ids(ids);
        for target in rules
            .player_mask(&state.rt, &self.player_text, player)
            .players()
        {
            if let Some(entity) = crate::economy::player_entity(tick.state(), economy, target) {
                tick.state_mut().insert(
                    ids.purse,
                    entity,
                    crate::economy::Purse(Money::new(self.amount)),
                )?;
            }
        }
        Ok(())
    }
}

struct ActionOverrideCommandPoints {
    player_text: String,
    base: i32,
    cap: i32,
}

impl ActionVerb for ActionOverrideCommandPoints {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let economy = RotwkRules::economy_ids(ids);
        for target in rules
            .player_mask(&state.rt, &self.player_text, player)
            .players()
        {
            if let Some(entity) = crate::economy::player_entity(tick.state(), economy, target) {
                let rule = crate::spell_state::PointRule::overridden(self.base, self.cap);
                tick.state_mut().insert(ids.point_rule, entity, rule)?;
                crate::diag::diag!(
                    "script: OVERRIDE_PLAYER_COMMAND_POINTS player {} base {} cap {}: limit {},                      used {}",
                    target.get(),
                    self.base,
                    self.cap,
                    rule.limit(0),
                    crate::economy::points_of(tick.state(), economy, target).used()
                );
            }
        }
        Ok(())
    }
}

struct ActionAllowDisallowBuilding {
    player_text: String,
    template: String,
    allow: bool,
}

impl ActionVerb for ActionAllowDisallowBuilding {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(kind) = rules.kinds.kind(&self.template) else {
            return Ok(());
        };
        let economy = RotwkRules::economy_ids(ids);
        for target in rules
            .player_mask(&state.rt, &self.player_text, player)
            .players()
        {
            let Some(entity) = crate::economy::player_entity(tick.state(), economy, target) else {
                continue;
            };
            let mut set = tick
                .state()
                .get(ids.disallowed_buildings, entity)
                .cloned()
                .unwrap_or_default();
            set.set(kind, !self.allow);
            tick.state_mut()
                .insert(ids.disallowed_buildings, entity, set)?;
        }
        Ok(())
    }
}

struct ActionAllowDisallowAllBuildings {
    player_text: String,
    allow: bool,
}

impl ActionVerb for ActionAllowDisallowAllBuildings {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let economy = RotwkRules::economy_ids(ids);
        for target in rules
            .player_mask(&state.rt, &self.player_text, player)
            .players()
        {
            let Some(entity) = crate::economy::player_entity(tick.state(), economy, target) else {
                continue;
            };
            let mut set = crate::economy::DisallowedBuildings::default();
            if !self.allow {
                let side = rules
                    .player_sides()
                    .iter()
                    .find(|&&(p, _)| p == target)
                    .map(|(_, side)| side.as_str());
                if let Some(side) = side {
                    for index in 0..rules.kinds.len() {
                        let Some(kind) = u16::try_from(index).ok().map(crate::unit::Kind) else {
                            continue;
                        };
                        let Some(entry) = rules.kinds.get(kind) else {
                            continue;
                        };
                        if entry.record.has_kind("STRUCTURE")
                            && entry.record.side.as_deref() == Some(side)
                        {
                            set.set(kind, true);
                        }
                    }
                }
            }
            tick.state_mut()
                .insert(ids.disallowed_buildings, entity, set)?;
        }
        Ok(())
    }
}

pub(super) fn parse_player_set_money(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerSetMoney {
        player_text: parameter_string(params, 0)?,
        amount: i64::from(parameter_int(params, 1)?),
    }))
}

pub(super) fn parse_override_command_points(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionOverrideCommandPoints {
        player_text: parameter_string(params, 0)?,
        base: parameter_int(params, 1)?,
        cap: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_allow_disallow_one_building(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionAllowDisallowBuilding {
        player_text: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        allow: parameter_int(params, 2)? != 0,
    }))
}

pub(super) fn parse_allow_disallow_all_buildings(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionAllowDisallowAllBuildings {
        player_text: parameter_string(params, 0)?,
        allow: parameter_int(params, 1)? != 0,
    }))
}
