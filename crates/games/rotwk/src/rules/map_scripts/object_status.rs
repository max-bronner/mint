use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sage_modules::effects::{Effect, Names};
use mint_sim::{Entity, PlayerId, Result, Tick, World};

use super::status::{status_name, ScriptStatusBit};
use super::{
    parameter_int, parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState,
    ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

const MAX_CONTAIN_DEPTH: u8 = 8;

#[derive(Clone)]
enum Subject {
    Unit(String),
    Team(String),
}

#[derive(Clone)]
pub(super) struct Target {
    subject: Subject,
    pub(super) side: String,
}

impl Target {
    pub(super) fn parse(params: &[Parameter], ctx: &ParseCtx<'_>, team: bool) -> Option<Self> {
        let name = parameter_string(params, 0)?;
        Some(Self {
            subject: if team {
                Subject::Team(name)
            } else {
                Subject::Unit(name)
            },
            side: ctx.side.to_owned(),
        })
    }

    pub(super) fn entities(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
    ) -> Vec<Entity> {
        match &self.subject {
            Subject::Unit(name) => rules
                .object_named(tick, ids, &state.rt, name, &self.side)
                .into_iter()
                .collect(),
            Subject::Team(name) => rules
                .peek_team(state, name, &self.side)
                .map(|team| rules.live_members(tick, state, team))
                .unwrap_or_default(),
        }
    }

    pub(super) fn unit_name(&self) -> Option<&str> {
        match &self.subject {
            Subject::Unit(name) => Some(name),
            Subject::Team(_) => None,
        }
    }

    pub(super) fn team_names(&self) -> Vec<String> {
        match &self.subject {
            Subject::Team(name) => vec![name.clone()],
            Subject::Unit(_) => Vec::new(),
        }
    }
}

fn names_of(status: &str) -> Names {
    vec![Arc::<str>::from(status)].into()
}

impl RotwkRules {
    fn script_set_status(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        entity: Entity,
        index: i32,
        on: bool,
    ) -> Result<()> {
        let Some(name) = status_name(index) else {
            return Ok(());
        };
        match name {
            "STAND_GROUND" => state.rt.stand_ground.set(entity, on),
            "REPULSOR" => self.set_repulsor(tick, ids, entity, on)?,
            "HUNT_WHEN_IDLE" if on => state.rt.hunters.insert(entity),
            "HUNT_WHEN_IDLE" => state.rt.hunters.forget(entity),
            "DESTROYED" | "UNDER_CONSTRUCTION" | "IS_ATTACKING" | "HORDE_MEMBER" => {
                diag!("script: object status {name} is derived from other state and is not set");
            }
            _ => {
                let (set, clear) = if on {
                    (names_of(name), Vec::new().into())
                } else {
                    (Vec::new().into(), names_of(name))
                };
                self.apply_effects(tick, ids, vec![(entity, Effect::StatusBits { set, clear })])?;
            }
        }
        Ok(())
    }

    fn script_has_status(
        &self,
        world: &World,
        ids: Ids,
        state: &MapScriptState,
        entity: Entity,
        index: i32,
    ) -> bool {
        let Some(name) = status_name(index) else {
            return false;
        };
        match name {
            "STAND_GROUND" => state.rt.stand_ground.contains(entity),
            "HUNT_WHEN_IDLE" => state.rt.hunters.contains(entity),
            "IS_ATTACKING" => Self::is_attacking(world, ids, entity),
            "UNDER_CONSTRUCTION" => world.contains(ids.construction, entity),
            "HORDE_MEMBER" => world.contains(ids.member, entity),
            _ => self
                .modules
                .as_ref()
                .is_some_and(|wiring| wiring.carries_status(world, entity, name)),
        }
    }

    pub(crate) fn script_status_bit(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        bit: ScriptStatusBit,
    ) -> bool {
        entity_has_script_status(world, ids, entity, bit)
    }

    fn contained_count(world: &World, ids: Ids, container: Entity) -> usize {
        world.get(ids.passengers, container).map_or(0, |held| {
            held.0
                .iter()
                .map(|&inside| {
                    world
                        .get(ids.horde, inside)
                        .map_or(1, |horde| horde.members.len())
                })
                .sum()
        })
    }

    pub(in crate::rules) fn set_indestructible_with_contents(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        on: bool,
        depth: u8,
    ) -> Result<()> {
        if let Some(mut health) = tick.state().get(ids.health, entity).copied() {
            health.0.set_indestructible(on);
            tick.state_mut().insert(ids.health, entity, health)?;
        }
        if depth >= MAX_CONTAIN_DEPTH {
            return Ok(());
        }
        let world = tick.state();
        let mut inside: Vec<Entity> = world
            .get(ids.passengers, entity)
            .map(|held| held.0.clone())
            .unwrap_or_default();
        if let Some(horde) = world.get(ids.horde, entity) {
            inside.extend(horde.members.iter().copied());
        }
        for held in inside {
            Self::set_indestructible_with_contents(tick, ids, held, on, depth.saturating_add(1))?;
        }
        Ok(())
    }

    pub(in crate::rules) fn set_selectable(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        on: bool,
    ) -> Result<()> {
        let (set, clear) = if on {
            (Vec::new().into(), names_of("UNSELECTABLE"))
        } else {
            (names_of("UNSELECTABLE"), Vec::new().into())
        };
        self.apply_effects(tick, ids, vec![(entity, Effect::StatusBits { set, clear })])
    }

    fn script_panel_flag(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        entity: Entity,
        flag: &str,
        on: bool,
    ) -> Result<()> {
        match flag {
            "Enabled" => {
                if state
                    .rt
                    .statuses
                    .set(entity, ScriptStatusBit::Disabled, !on)
                {
                    self.pause_special_powers(tick, ids, entity, !on)?;
                }
            }
            "Powered" => {
                state
                    .rt
                    .statuses
                    .set(entity, ScriptStatusBit::Unpowered, !on);
            }
            "Indestructible" => Self::set_indestructible_with_contents(tick, ids, entity, on, 0)?,
            "Unsellable" => {
                state
                    .rt
                    .statuses
                    .set(entity, ScriptStatusBit::Unsellable, on);
            }
            "Selectable" => self.set_selectable(tick, ids, entity, on)?,
            "Player Targetable" => {
                state
                    .rt
                    .statuses
                    .set(entity, ScriptStatusBit::Targetable, on);
            }
            _ => {}
        }
        Ok(())
    }
}

enum Op {
    Status { index: i32, on: bool },
    Panel { flag: String, on: bool },
    Stealth { enabled: bool },
}

struct ActionObjects {
    target: Target,
    op: Op,
}

impl ActionVerb for ActionObjects {
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
        for entity in self.target.entities(rules, tick, ids, state) {
            match &self.op {
                Op::Status { index, on } => {
                    rules.script_set_status(tick, ids, state, entity, *index, *on)?;
                }
                Op::Panel { flag, on } => {
                    rules.script_panel_flag(tick, ids, state, entity, flag, *on)?;
                }
                Op::Stealth { enabled } => {
                    state
                        .rt
                        .statuses
                        .set(entity, ScriptStatusBit::Unstealthed, !*enabled);
                }
            }
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.target.unit_name()
    }
}

enum Check {
    Status(i32),
    Empty,
    TotallyDead,
}

struct ConditionObjects {
    target: Target,
    check: Check,
    any_of_team: bool,
}

impl ConditionVerb for ConditionObjects {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let world = tick.state();
        if matches!(self.check, Check::TotallyDead) {
            return match self.target.unit_name() {
                Some(name) => {
                    rules
                        .object_named(tick, ids, &state.rt, name, &self.target.side)
                        .is_none()
                        && state.ever_existed.contains(name)
                }
                None => false,
            };
        }
        let members = self.target.entities(rules, tick, ids, state);
        let holds = |entity: Entity| match self.check {
            Check::Status(index) => rules.script_has_status(world, ids, state, entity, index),
            Check::Empty => {
                rules.contain_view(world, entity).is_some()
                    && RotwkRules::contained_count(world, ids, entity) == 0
            }
            Check::TotallyDead => false,
        };
        if self.any_of_team {
            members.into_iter().any(holds)
        } else {
            members.first().is_some_and(|&entity| holds(entity))
        }
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.target.unit_name()
    }

    fn team_names(&self) -> Vec<String> {
        self.target.team_names()
    }
}

fn action(target: Option<Target>, op: Option<Op>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionObjects {
        target: target?,
        op: op?,
    }))
}

fn status_op(params: &[Parameter]) -> Option<Op> {
    Some(Op::Status {
        index: parameter_int(params, 1)?,
        on: parameter_int(params, 2)? != 0,
    })
}

fn panel_op(params: &[Parameter]) -> Option<Op> {
    Some(Op::Panel {
        flag: parameter_string(params, 1)?,
        on: parameter_int(params, 2)? != 0,
    })
}

pub(super) fn parse_unit_change_status(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, false), status_op(params))
}

pub(super) fn parse_team_change_status(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, true), status_op(params))
}

fn repulsor_op(params: &[Parameter]) -> Option<Op> {
    Some(Op::Status {
        index: super::status::status_index("REPULSOR"),
        on: parameter_int(params, 1)? != 0,
    })
}

pub(super) fn parse_named_set_repulsor(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, false), repulsor_op(params))
}

pub(super) fn parse_team_set_repulsor(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, true), repulsor_op(params))
}

pub(super) fn parse_unit_panel_flags(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, false), panel_op(params))
}

pub(super) fn parse_team_panel_flags(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    action(Target::parse(params, ctx, true), panel_op(params))
}

pub(super) fn parse_set_stealth_enabled(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let enabled = Op::Stealth {
        enabled: parameter_int(params, 1)? != 0,
    };
    action(Target::parse(params, ctx, false), Some(enabled))
}

fn condition(
    target: Option<Target>,
    check: Check,
    any_of_team: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionObjects {
        target: target?,
        check,
        any_of_team,
    }))
}

pub(super) fn parse_unit_has_status(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let index = parameter_int(params, 1)?;
    condition(
        Target::parse(params, ctx, false),
        Check::Status(index),
        false,
    )
}

pub(super) fn parse_team_some_have_status(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let index = parameter_int(params, 1)?;
    condition(Target::parse(params, ctx, true), Check::Status(index), true)
}

pub(super) fn parse_building_is_empty(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    condition(Target::parse(params, ctx, false), Check::Empty, false)
}

pub(super) fn parse_totally_dead(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    condition(Target::parse(params, ctx, false), Check::TotallyDead, false)
}

pub(crate) fn entity_has_script_status(
    world: &World,
    ids: Ids,
    entity: Entity,
    bit: ScriptStatusBit,
) -> bool {
    world
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next())
        .is_some_and(|(_, state)| state.rt.statuses.has(entity, bit))
}
