use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::orders::ScriptOrder;
use super::{
    parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::capture::Transfer;
use crate::rules::{Ids, RotwkRules};

const NOT_UNITS: [&str; 4] = ["STRUCTURE", "DOZER", "HARVESTER", "IGNORES_SELECT_ALL"];

impl RotwkRules {
    fn kind_flags(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, flags: &[&str]) -> bool {
        tick.state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| flags.iter().any(|flag| entry.record.has_kind(flag)))
    }

    pub(super) fn hunts_when_idle(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        world.is_live(entity)
            && world.contains(ids.health, entity)
            && !world.contains(ids.doomed, entity)
            && world.contains(ids.kind, entity)
            && !self.kind_flags(tick, ids, entity, &NOT_UNITS)
    }

    fn live_objects_of(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> Vec<Entity> {
        let world = tick.state();
        world
            .store(ids.owner)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, owner)| {
                        owner.0 == player
                            && world.is_live(*entity)
                            && world.contains(ids.kind, *entity)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn transfer_assets(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        from: PlayerId,
        to: PlayerId,
    ) -> Result<()> {
        for entity in self.live_objects_of(tick, ids, from) {
            self.change_owner(tick, ids, entity, to)?;
        }
        let money = crate::economy::purse_of(tick.state(), Self::economy_ids(ids), from).amount();
        let money = i32::try_from(money).unwrap_or(i32::MAX);
        self.script_give_money(tick, ids, from, money.saturating_neg())?;
        self.script_give_money(tick, ids, to, money)
    }
}

enum Op {
    Hunt,
    Idle,
    TransferAssets { to: String },
}

struct ActionPlayerOp {
    player_text: String,
    op: Op,
}

impl ActionVerb for ActionPlayerOp {
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
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        match &self.op {
            Op::Hunt => {
                for target in mask.players() {
                    state.rt.standing.start_hunting(target);
                    diag!("script: PLAYER_HUNT sends player {} hunting", target.get());
                }
            }
            Op::Idle => {
                for target in mask.players() {
                    let units: Vec<Entity> = rules
                        .live_objects_of(tick, ids, target)
                        .into_iter()
                        .filter(|&e| {
                            tick.state().contains(ids.health, e)
                                && !tick.state().contains(ids.doomed, e)
                                && !rules.kind_flags(tick, ids, e, &["STRUCTURE"])
                        })
                        .collect();
                    for unit in &units {
                        state.rt.hunters.forget(*unit);
                    }
                    diag!(
                        "script: IDLE_ALL_UNITS stops {} unit(s) of player {}",
                        units.len(),
                        target.get()
                    );
                    rules.issue_script_order(tick, ids, &units, ScriptOrder::Stop)?;
                }
            }
            Op::TransferAssets { to } => {
                let to = rules.player_mask(&state.rt, to, player).single();
                if let (Some(from), Some(to)) = (mask.single(), to) {
                    diag!(
                        "script: PLAYER_TRANSFER_OWNERSHIP_PLAYER player {} -> player {}",
                        from.get(),
                        to.get()
                    );
                    rules.transfer_assets(tick, ids, from, to)?;
                }
            }
        }
        Ok(())
    }
}

struct ActionNamedTransfer {
    unit: String,
    player_text: String,
    side: String,
}

impl ActionVerb for ActionNamedTransfer {
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
        let Some(to) = rules
            .player_mask(&state.rt, &self.player_text, player)
            .single()
        else {
            return Ok(());
        };
        if let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) {
            diag!(
                "script: NAMED_TRANSFER_OWNERSHIP_PLAYER {:?} -> player {}",
                self.unit,
                to.get()
            );
            rules.change_owner_scripted(tick, ids, entity, to, Transfer::NamedSetTeam)?;
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ActionTeamTransfer {
    team: String,
    player_text: String,
    side: String,
}

impl ActionVerb for ActionTeamTransfer {
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
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return Ok(());
        };
        let Some(to) = rules
            .player_mask(&state.rt, &self.player_text, player)
            .first()
        else {
            return Ok(());
        };
        diag!(
            "script: TEAM_TRANSFER_TO_PLAYER {:?} -> player {}",
            self.team,
            to.get()
        );
        let Some(proto) = state.rt.teams.get(team).map(|instance| instance.proto) else {
            return Ok(());
        };
        state.rt.teams.set_controller(proto, to);
        for other in state.rt.teams.instances_of(proto) {
            if other == team {
                continue;
            }
            for member in rules.live_members(tick, state, other) {
                rules.change_owner_scripted(tick, ids, member, to, Transfer::Prototype)?;
            }
        }
        for member in rules.live_members(tick, state, team) {
            rules.change_owner_scripted(tick, ids, member, to, Transfer::TeamController)?;
        }
        Ok(())
    }
}

struct ConditionLostObjectType {
    player_text: String,
    types: String,
}

impl ConditionLostObjectType {
    fn templates(&self, rules: &RotwkRules, state: &MapScriptState) -> Vec<String> {
        state
            .rt
            .lists
            .types_of(&self.types)
            .into_iter()
            .filter(|name| rules.kinds.kind(name).is_some())
            .collect()
    }
}

impl ConditionVerb for ConditionLostObjectType {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let templates = self.templates(rules, state);
        if templates.is_empty() {
            return false;
        }
        rules
            .player_mask(&state.rt, &self.player_text, player)
            .any(|p| {
                let count = rules.count_objects_of(tick, ids, p, &templates);
                count < i64::from(state.rt.standing.object_count(p, &self.types))
            })
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let templates = self.templates(rules, state);
        if templates.is_empty() {
            return false;
        }
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for p in mask.players() {
            let count = rules.count_objects_of(tick, ids, p, &templates);
            let count = i32::try_from(count).unwrap_or(i32::MAX);
            let stored = state.rt.standing.object_count(p, &self.types);
            if count != stored {
                state.rt.standing.set_object_count(p, &self.types, count);
            }
            if count < stored {
                return true;
            }
        }
        false
    }
}

struct ConditionAttackedBy {
    victims: String,
    attackers: String,
}

impl ConditionVerb for ConditionAttackedBy {
    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let attackers = rules.player_mask(&state.rt, &self.attackers, player);
        rules
            .player_mask(&state.rt, &self.victims, player)
            .any(|victim| attackers.any(|a| state.rt.standing.was_attacked_by(victim, a)))
    }
}

fn player_op(params: &[Parameter], op: Op) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerOp {
        player_text: parameter_string(params, 0).unwrap_or_default(),
        op,
    }))
}

pub(super) fn parse_player_hunt(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parameter_string(params, 0)?;
    player_op(params, Op::Hunt)
}

pub(super) fn parse_idle_all_units(
    _: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerOp {
        player_text: "<Local Player>".to_owned(),
        op: Op::Idle,
    }))
}

pub(super) fn parse_player_transfer(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let to = parameter_string(params, 1)?;
    parameter_string(params, 0)?;
    player_op(params, Op::TransferAssets { to })
}

pub(super) fn parse_named_transfer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNamedTransfer {
        unit: parameter_string(params, 0)?,
        player_text: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_team_transfer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamTransfer {
        team: parameter_string(params, 0)?,
        player_text: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_player_lost_object_type(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionLostObjectType {
        player_text: parameter_string(params, 0)?,
        types: parameter_string(params, 1)?,
    }))
}

pub(super) fn parse_attacked_by_player(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionAttackedBy {
        victims: parameter_string(params, 0)?,
        attackers: parameter_string(params, 1)?,
    }))
}
