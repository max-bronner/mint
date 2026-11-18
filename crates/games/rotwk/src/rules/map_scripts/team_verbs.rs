use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::counters::SideKey;
use super::orders::ScriptOrder;
use super::team::{TeamId, THIS_TEAM};
use super::{
    parameter_real, parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState,
    ParseCtx,
};
use crate::combat::DeathNaming;
use crate::diag::diag;
use crate::rules::capture::Transfer;
use crate::rules::{Ids, RotwkRules};

const REINFORCEMENT_SPACING: f32 = 2.25;
const REINFORCEMENT_ROW: f32 = 2.0;

impl RotwkRules {
    pub(crate) fn seat_placed_team_members(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        anchor: Entity,
        seats: &[(Entity, String)],
    ) {
        let Some(engine) = self.map_scripts.as_ref() else {
            return;
        };
        let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) else {
            return;
        };
        let protos = engine.team_prototypes();
        for (entity, raw) in seats {
            let (entity, Some((owner, name))) = (*entity, raw.split_once('/')) else {
                continue;
            };
            let Some(proto) = protos.find(owner, name) else {
                continue;
            };
            let team = match state.rt.teams.instances_of(proto).first() {
                Some(&team) => team,
                None => state.rt.teams.create_instance(proto, false),
            };
            state.rt.teams.add_member(team, entity);
            if let Some(instance) = state.rt.teams.get_mut(team) {
                instance.set_active();
            }
        }
    }

    fn team_named_has_units(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        name: &str,
        side: &str,
    ) -> bool {
        let Some(engine) = self.map_scripts.as_ref() else {
            return false;
        };
        let protos = engine.team_prototypes();
        let bound = [state.rt.calling_team, state.rt.condition_team]
            .into_iter()
            .flatten()
            .next();
        if name == THIS_TEAM {
            return bound.is_some_and(|team| self.team_has_units(tick, ids, state, team));
        }
        let key = SideKey::qualify(name, side);
        for team in [state.rt.calling_team, state.rt.condition_team]
            .into_iter()
            .flatten()
        {
            let named = state
                .rt
                .teams
                .get(team)
                .and_then(|instance| protos.get(instance.proto))
                .is_some_and(|proto| proto.owner == key.side && proto.name == key.name);
            if named {
                return self.team_has_units(tick, ids, state, team);
            }
        }
        protos.find(&key.side, &key.name).is_some_and(|proto| {
            state
                .rt
                .teams
                .instances_of(proto)
                .into_iter()
                .any(|team| self.team_has_units(tick, ids, state, team))
        })
    }

    pub(super) fn team_owner(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        team: TeamId,
    ) -> Option<PlayerId> {
        let proto = state.rt.teams.get(team)?.proto;
        if let Some(controller) = state.rt.teams.controller(proto) {
            return Some(controller);
        }
        self.resolve_side_player(&engine.team_prototypes().get(proto)?.owner)
    }

    pub(super) fn spawn_script_unit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        template: &str,
        position: Vec2,
        angle: f32,
        owner: PlayerId,
    ) -> Result<Option<(Entity, f32)>> {
        let Some(kind) = self.kinds.kind(template) else {
            diag!("script: {template:?} has no Kind in this install - nothing spawned");
            return Ok(None);
        };
        let Some(entity) = self.spawn_kind_placed(
            tick,
            ids,
            kind,
            crate::rules::spawning::Placement::Exact(position),
            Facing::from_radians(angle),
            owner,
        )?
        else {
            return Ok(None);
        };
        let radius = tick
            .state()
            .get(ids.shape, entity)
            .map_or(0.0, |shape| shape.radius);
        Ok(Some((entity, radius)))
    }

    pub(super) fn grant_script_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        let Some(bit) = self.upgrade_bits.bit(name) else {
            return Ok(());
        };
        if self.upgrade_bits.is_player(bit) {
            if let Some(owner) = tick.state().get(ids.owner, entity).map(|o| o.0) {
                self.grant_player_upgrade(tick, ids, owner, bit)?;
            }
            return Ok(());
        }
        self.grant_object_upgrade(tick, ids, entity, bit)?;
        let members = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        for member in members {
            self.grant_object_upgrade(tick, ids, member, bit)?;
        }
        Ok(())
    }

    fn destroy_silently(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        let members = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        for member in members {
            tick.state_mut().despawn(member);
        }
        tick.state_mut().despawn(entity);
    }

    pub(super) fn live_members(
        &self,
        tick: &Tick<'_>,
        state: &MapScriptState,
        team: TeamId,
    ) -> Vec<Entity> {
        state
            .rt
            .teams
            .get(team)
            .map(|instance| {
                instance
                    .members()
                    .iter()
                    .copied()
                    .filter(|&m| tick.state().is_live(m))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn move_to_team(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        entity: Entity,
        team: TeamId,
    ) -> Result<()> {
        state.rt.teams.add_member(team, entity);
        let owner = tick.state().get(ids.owner, entity).map(|o| o.0);
        if let Some(new) = self.team_owner(engine, state, team) {
            if owner != Some(new) {
                self.change_owner_scripted(tick, ids, entity, new, Transfer::NamedSetTeam)?;
            }
        }
        Ok(())
    }
}

struct ConditionTeam {
    team: String,
    side: String,
    test: TeamTest,
}

#[derive(Clone, Copy)]
enum TeamTest {
    HasUnits,
    Destroyed,
    Created,
}

impl ConditionVerb for ConditionTeam {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        match self.test {
            TeamTest::HasUnits => {
                rules.team_named_has_units(tick, ids, state, &self.team, &self.side)
            }
            TeamTest::Destroyed => rules
                .peek_team(state, &self.team, &self.side)
                .is_some_and(|team| rules.team_destroyed(tick, ids, state, team)),
            TeamTest::Created => rules
                .peek_team(state, &self.team, &self.side)
                .and_then(|team| state.rt.teams.get(team))
                .is_some_and(|instance| instance.created),
        }
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

struct ConditionNamedNotDestroyed(String);
impl ConditionVerb for ConditionNamedNotDestroyed {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules
            .object_named(tick, ids, &state.rt, &self.0, "")
            .is_some()
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.0)
    }
}

enum TeamOp {
    Delete { ignore_dead: bool },
    Kill,
    SetHealth(f32),
    Upgrade(String),
}

struct ActionTeamOp {
    team: String,
    side: String,
    op: TeamOp,
}

impl ActionVerb for ActionTeamOp {
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
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return Ok(());
        };
        let members = rules.live_members(tick, state, team);
        for member in members {
            let dying = tick.state().contains(ids.doomed, member);
            match &self.op {
                TeamOp::Delete { ignore_dead } => {
                    if !(*ignore_dead && dying) {
                        rules.destroy_silently(tick, ids, member);
                    }
                }
                TeamOp::Kill => {
                    if !dying {
                        if let Some(pair) =
                            rules.kill(tick, ids, member, None, DeathNaming::Normal)?
                        {
                            kills.push(pair);
                        }
                    }
                }
                TeamOp::SetHealth(percent) => {
                    if let Some(health) = tick.state_mut().get_mut(ids.health, member) {
                        let to = health.0.max() * percent / 100.0;
                        health.0.set_current(to);
                    }
                }
                TeamOp::Upgrade(name) => rules.grant_script_upgrade(tick, ids, member, name)?,
            }
        }
        Ok(())
    }
}

enum NamedOp {
    Delete,
    Upgrade(String),
}

struct ActionNamedOp {
    unit: String,
    op: NamedOp,
}

impl ActionVerb for ActionNamedOp {
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
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, "") else {
            return Ok(());
        };
        match &self.op {
            NamedOp::Delete => rules.destroy_silently(tick, ids, entity),
            NamedOp::Upgrade(name) => rules.grant_script_upgrade(tick, ids, entity, name)?,
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ActionUnitSetTeam {
    unit: String,
    team: String,
    side: String,
}

impl ActionVerb for ActionUnitSetTeam {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, "") else {
            return Ok(());
        };
        let Some(team) = rules.get_team(state, &self.team, &self.side, true) else {
            return Ok(());
        };
        rules.move_to_team(tick, ids, engine, state, entity, team)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

struct ActionMergeTeams {
    from: String,
    into: String,
    side: String,
}

impl ActionVerb for ActionMergeTeams {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(from) = rules.get_team(state, &self.from, &self.side, false) else {
            return Ok(());
        };
        let Some(into) = rules.get_team(state, &self.into, &self.side, true) else {
            return Ok(());
        };
        if from == into {
            return Ok(());
        }
        for member in rules.live_members(tick, state, from) {
            rules.move_to_team(tick, ids, engine, state, member, into)?;
        }
        if let Some(instance) = state.rt.teams.get_mut(into) {
            instance.set_active();
        }
        Ok(())
    }
}

struct ActionCreateUnnamed {
    template: String,
    team: String,
    side: String,
    at: Option<(Vec2, f32)>,
    place: String,
}

impl ActionVerb for ActionCreateUnnamed {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(team) = rules.get_team(state, &self.team, &self.side, true) else {
            return Ok(());
        };
        let Some((position, angle)) = self.at.or_else(|| {
            let names = state.rt.lists.types_of(&self.place);
            let from = rules.team_position(tick, ids, state, team)?;
            let found = rules.nearest_of_types(tick, ids, &names, from, None, false)?;
            Some((rules.position_of_entity(tick, ids, found)?, 0.0))
        }) else {
            diag!(
                "script: CREATE_UNNAMED_ON_TEAM_AT_WAYPOINT finds no waypoint or object type {:?}",
                self.place
            );
            return Ok(());
        };
        let Some(owner) = rules.team_owner(engine, state, team) else {
            return Ok(());
        };
        if let Some((entity, _)) =
            rules.spawn_script_unit(tick, ids, &self.template, position, angle, owner)?
        {
            state.rt.teams.add_member(team, entity);
        }
        Ok(())
    }

    fn referenced_template(&self) -> Option<&str> {
        Some(&self.template)
    }
}

struct ActionReinforce {
    team: String,
    side: String,
    destination: Option<Vec2>,
}

impl ActionVerb for ActionReinforce {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(destination) = self.destination else {
            return Ok(());
        };
        let key = SideKey::qualify(&self.team, &self.side);
        let protos = engine.team_prototypes();
        let Some(index) = protos.find(&key.side, &key.name) else {
            return Ok(());
        };
        let Some(proto) = protos.get(index) else {
            return Ok(());
        };
        let Some(owner) = state
            .rt
            .teams
            .controller(index)
            .or_else(|| rules.resolve_side_player(&proto.owner))
        else {
            return Ok(());
        };
        let mut origin = proto
            .template
            .reinforcement_origin
            .as_deref()
            .and_then(|name| engine.waypoint(name))
            .unwrap_or(destination);
        let moves = origin.x != destination.x || origin.y != destination.y;
        let team = match proto
            .singleton
            .then(|| state.rt.teams.instances_of(index).first().copied())
            .flatten()
        {
            Some(team) => team,
            None => state.rt.teams.create_instance(index, false),
        };
        let row_start = origin.x;
        for slot in &proto.template.units {
            let mut radius = 0.0;
            for j in 0..slot.max {
                let at = Vec2::new(
                    row_start + REINFORCEMENT_SPACING * j as f32 * radius,
                    origin.y,
                );
                let Some((entity, found)) =
                    rules.spawn_script_unit(tick, ids, &slot.template, at, 0.0, owner)?
                else {
                    break;
                };
                radius = found;
                for upgrade in &slot.upgrades {
                    rules.grant_script_upgrade(tick, ids, entity, upgrade)?;
                }
                if let Ok(levels) = u32::try_from(slot.experience_level) {
                    if levels > 0 {
                        rules.gain_levels(tick, ids, entity, levels, None)?;
                    }
                }
                state.rt.teams.add_member(team, entity);
            }
            origin.y += REINFORCEMENT_ROW * radius;
        }
        if let Some(instance) = state.rt.teams.get_mut(team) {
            instance.set_active();
        }
        if moves {
            let members = rules.live_members(tick, state, team);
            rules.issue_script_order(tick, ids, &members, ScriptOrder::Move(destination, false))?;
        }
        diag!(
            "script: CREATE_REINFORCEMENT_TEAM {:?} created {} unit(s)",
            self.team,
            state.rt.teams.get(team).map_or(0, |t| t.members().len())
        );
        Ok(())
    }
}

fn team_param(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<(String, String)> {
    Some((parameter_string(params, 0)?, ctx.side.to_owned()))
}

fn team_condition(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    test: TeamTest,
) -> Option<Arc<dyn ConditionVerb>> {
    let (team, side) = team_param(params, ctx)?;
    Some(Arc::new(ConditionTeam { team, side, test }))
}

pub(super) fn parse_team_has_units(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_condition(params, ctx, TeamTest::HasUnits)
}

pub(super) fn parse_team_destroyed(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_condition(params, ctx, TeamTest::Destroyed)
}

pub(super) fn parse_team_created(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_condition(params, ctx, TeamTest::Created)
}

pub(super) fn parse_named_not_destroyed(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionNamedNotDestroyed(parameter_string(
        params, 0,
    )?)))
}

fn team_op(params: &[Parameter], ctx: &ParseCtx<'_>, op: TeamOp) -> Option<Arc<dyn ActionVerb>> {
    let (team, side) = team_param(params, ctx)?;
    Some(Arc::new(ActionTeamOp { team, side, op }))
}

pub(super) fn parse_team_delete(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_op(params, ctx, TeamOp::Delete { ignore_dead: false })
}

pub(super) fn parse_team_delete_living(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_op(params, ctx, TeamOp::Delete { ignore_dead: true })
}

pub(super) fn parse_team_kill(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_op(params, ctx, TeamOp::Kill)
}

pub(super) fn parse_team_set_health(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let percent = parameter_real(params, 1)?.clamp(0.0, 100.0);
    team_op(params, ctx, TeamOp::SetHealth(percent))
}

pub(super) fn parse_team_upgrade(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_op(params, ctx, TeamOp::Upgrade(parameter_string(params, 1)?))
}

pub(super) fn parse_named_receive_upgrade(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNamedOp {
        unit: parameter_string(params, 0)?,
        op: NamedOp::Upgrade(parameter_string(params, 1)?),
    }))
}

pub(super) fn parse_named_delete(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNamedOp {
        unit: parameter_string(params, 0)?,
        op: NamedOp::Delete,
    }))
}

pub(super) fn parse_unit_set_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionUnitSetTeam {
        unit: parameter_string(params, 0)?,
        team: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_merge_teams(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionMergeTeams {
        from: parameter_string(params, 0)?,
        into: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_create_unnamed(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 2)?;
    Some(Arc::new(ActionCreateUnnamed {
        template: parameter_string(params, 0)?,
        team: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
        at: ctx.waypoints.get(&waypoint).copied(),
        place: waypoint,
    }))
}

pub(super) fn parse_reinforce(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 1)?;
    Some(Arc::new(ActionReinforce {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        destination: ctx.waypoints.get(&waypoint).map(|(position, _)| *position),
    }))
}
