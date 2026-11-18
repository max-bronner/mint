use std::collections::BTreeMap;
use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::counters::TICKS_PER_SCRIPT_FRAME;
use super::orders::ScriptOrder;
use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Goal {
    Point(Vec2),
    Object(Entity),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Facings(BTreeMap<Entity, Goal>);

impl Eq for Facings {}

impl Facings {
    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.0.contains_key(&entity)
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.0.remove(&entity);
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity, _| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (entity, goal) in &self.0 {
            entity.hash_into(hasher);
            match goal {
                Goal::Point(at) => {
                    hasher.write_u8(0);
                    hasher.write_f32(at.x);
                    hasher.write_f32(at.y);
                }
                Goal::Object(target) => {
                    hasher.write_u8(1);
                    target.hash_into(hasher);
                }
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn step_facings(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let turning: Vec<(Entity, Goal)> =
            state.rt.facings.0.iter().map(|(&e, &g)| (e, g)).collect();
        for (entity, goal) in turning {
            let world = tick.state();
            let busy = world.contains(ids.path, entity) || world.contains(ids.attack_order, entity);
            let target = match goal {
                Goal::Point(at) => Some(at),
                Goal::Object(target) if world.is_live(target) => {
                    world.get(ids.body, target).map(|body| body.0.position)
                }
                Goal::Object(_) => None,
            };
            let body = world.get(ids.body, entity).copied();
            let (false, Some(target), Some(mut body)) = (busy, target, body) else {
                state.rt.facings.0.remove(&entity);
                continue;
            };
            let Some(desired) = Facing::from_vector(target.minus(body.0.position)) else {
                state.rt.facings.0.remove(&entity);
                continue;
            };
            let limits = self.mover_limits(tick, ids, entity);
            let step = limits.turn_rate * Self::tick_seconds() * TICKS_PER_SCRIPT_FRAME as f32;
            body.0.facing = body.0.facing.turned_towards(desired, step);
            let arrived = body.0.facing.delta_to(desired).abs() <= f32::EPSILON;
            tick.state_mut().insert(ids.body, entity, body)?;
            if let Some(horde) = tick.state_mut().get_mut(ids.horde, entity) {
                horde.dirty = true;
            }
            if arrived {
                state.rt.facings.0.remove(&entity);
            }
        }
        Ok(())
    }

    pub(in crate::rules) fn script_teleport(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        to: Vec2,
    ) -> Result<()> {
        let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        body.0.position = to;
        body.0.waypoint = 0;
        tick.state_mut().insert(ids.body, entity, body)?;
        tick.state_mut().remove(ids.path, entity);
        if tick.state().contains(ids.horde, entity) {
            self.snap_horde_members_to_slots(tick, ids, entity)?;
        }
        Ok(())
    }
}

enum What {
    FaceWaypoint(Option<Vec2>),
    FaceNamed(String),
    Teleport(Option<Vec2>),
}

struct ActionFaceOrTeleport {
    name: String,
    side: Option<String>,
    what: What,
    verb: &'static str,
}

impl ActionVerb for ActionFaceOrTeleport {
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
        let members = rules.subject_members(tick, ids, state, &self.name, self.side.as_deref());
        if members.is_empty() {
            return Ok(());
        }
        let goal = match &self.what {
            What::FaceWaypoint(at) => at.map(Goal::Point),
            What::FaceNamed(unit) => rules
                .object_named(tick, ids, &state.rt, unit, "")
                .map(Goal::Object),
            What::Teleport(to) => {
                let Some(to) = *to else {
                    return Ok(());
                };
                for &member in &members {
                    rules.script_teleport(tick, ids, member, to)?;
                }
                diag!("script: {} sets {} unit(s) down", self.verb, members.len());
                return Ok(());
            }
        };
        let Some(goal) = goal else {
            return Ok(());
        };
        rules.issue_script_order(tick, ids, &members, ScriptOrder::Stop)?;
        for &member in &members {
            state.rt.walks.forget(member);
            state.rt.facings.0.insert(member, goal);
        }
        diag!("script: {} turns {} unit(s)", self.verb, members.len());
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match (&self.side, &self.what) {
            (None, _) => Some(&self.name),
            (Some(_), What::FaceNamed(unit)) => Some(unit),
            _ => None,
        }
    }
}

fn build(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    verb: &'static str,
    what: impl FnOnce(&[Parameter], &ParseCtx<'_>) -> Option<What>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionFaceOrTeleport {
        name: parameter_string(params, 0)?,
        side: (!named).then(|| ctx.side.to_owned()),
        what: what(params, ctx)?,
        verb,
    }))
}

fn waypoint(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Option<Vec2>> {
    let name = parameter_string(params, 1)?;
    Some(ctx.waypoints.get(&name).map(|(position, _)| *position))
}

pub(super) fn parse_named_face_waypoint(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, true, "NAMED_FACE_WAYPOINT", |p, c| {
        waypoint(p, c).map(What::FaceWaypoint)
    })
}

pub(super) fn parse_team_face_waypoint(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, false, "TEAM_FACE_WAYPOINT", |p, c| {
        waypoint(p, c).map(What::FaceWaypoint)
    })
}

pub(super) fn parse_named_face_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, true, "NAMED_FACE_NAMED", |p, _| {
        parameter_string(p, 1).map(What::FaceNamed)
    })
}

pub(super) fn parse_team_face_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, false, "TEAM_FACE_NAMED", |p, _| {
        parameter_string(p, 1).map(What::FaceNamed)
    })
}

pub(super) fn parse_named_teleport(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, true, "UNIT_TELEPORT_TO_WAYPOINT", |p, c| {
        waypoint(p, c).map(What::Teleport)
    })
}

pub(super) fn parse_team_teleport(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(params, ctx, false, "TEAM_TELEPORT_TO_WAYPOINT", |p, c| {
        waypoint(p, c).map(What::Teleport)
    })
}
