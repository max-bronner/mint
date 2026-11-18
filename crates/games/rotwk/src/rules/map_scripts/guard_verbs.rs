use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::area::Area;
use super::orders::ScriptOrder;
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::guard::{GuardArea, GuardMode, GuardTarget};
use crate::rules::{Ids, RotwkRules};

enum What {
    GuardHere,
    GuardPoint(Option<Vec2>),
    GuardArea {
        area: Area,
        from: Option<Option<Vec2>>,
    },
    StandGround(bool),
    AttackArea(Arc<Area>),
    GuardObject(String),
    GuardTeam(String),
}

struct ActionGuard {
    name: String,
    side: Option<String>,
    what: What,
    verb: &'static str,
}

impl RotwkRules {
    fn script_guard(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        point: Option<Vec2>,
        area: Option<GuardArea>,
        target: GuardTarget,
    ) -> Result<()> {
        self.leave_guard(tick, ids, unit);
        let Some(mut guard) = self.guard_here(tick, ids, unit, GuardMode::Normal, tick.frame())
        else {
            return Ok(());
        };
        if let Some(point) = point {
            guard.point = point;
        }
        if target != GuardTarget::Point {
            guard.point = Vec2::new(0.0, 0.0);
        }
        guard.target = target;
        guard.scripted = true;
        guard.area = area;
        if let Some(goal) = self.position_to_guard(tick, ids, unit, &guard, false) {
            guard.goal = goal;
        }
        tick.state_mut().insert(ids.guard, unit, guard)?;
        Ok(())
    }
}

impl ActionVerb for ActionGuard {
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
        let mut members = rules.subject_members(tick, ids, state, &self.name, self.side.as_deref());
        members.retain(|&member| {
            let world = tick.state();
            !world.contains(ids.member, member) && !world.contains(ids.construction, member)
        });
        if members.is_empty() {
            return Ok(());
        }
        if !matches!(self.what, What::StandGround(_) | What::AttackArea(_)) {
            for &member in &members {
                state.rt.attack_areas.forget(member);
            }
        }
        match &self.what {
            What::StandGround(on) => {
                for &member in &members {
                    let horde = tick
                        .state()
                        .get(ids.horde, member)
                        .map(|horde| horde.members.clone())
                        .unwrap_or_default();
                    for held in std::iter::once(member).chain(horde) {
                        state.rt.stand_ground.set(held, *on);
                    }
                }
            }
            What::AttackArea(area) => {
                if area.exists() {
                    let attackers: Vec<Entity> = members
                        .iter()
                        .copied()
                        .filter(|&member| {
                            rules.move_refused(tick, ids, member).is_none()
                                && !rules.has_kind_bit(tick.state(), ids, member, "PROJECTILE")
                        })
                        .collect();
                    rules.begin_attack_area(tick, state, &attackers, area);
                }
            }
            What::GuardObject(_) | What::GuardTeam(_) => {
                let target = match &self.what {
                    What::GuardObject(ward) => rules
                        .subject_members(tick, ids, state, ward, None)
                        .first()
                        .map(|&ward| GuardTarget::Object(ward)),
                    What::GuardTeam(ward) => rules
                        .peek_team(state, ward, self.side.as_deref().unwrap_or_default())
                        .map(GuardTarget::Team),
                    _ => None,
                };
                let Some(target) = target else {
                    return Ok(());
                };
                let guarders: Vec<Entity> = members
                    .iter()
                    .copied()
                    .filter(|&member| {
                        rules.move_refused(tick, ids, member).is_none()
                            && !rules.has_kind_bit(tick.state(), ids, member, "PROJECTILE")
                    })
                    .collect();
                rules.issue_script_order(tick, ids, &guarders, ScriptOrder::Stop)?;
                for &member in &guarders {
                    rules.script_guard(tick, ids, member, None, None, target)?;
                }
            }
            What::GuardHere | What::GuardPoint(_) | What::GuardArea { .. } => {
                let (point, area) = match &self.what {
                    What::GuardHere => (None, None),
                    What::GuardPoint(at) => {
                        let Some(at) = *at else {
                            return Ok(());
                        };
                        (Some(at), None)
                    }
                    What::GuardArea { area, from } => {
                        let Some((centre, radius)) = area.centre_and_radius() else {
                            return Ok(());
                        };
                        (
                            Some(from.flatten().unwrap_or(centre)),
                            Some(GuardArea { centre, radius }),
                        )
                    }
                    _ => return Ok(()),
                };
                rules.issue_script_order(tick, ids, &members, ScriptOrder::Stop)?;
                for &member in &members {
                    rules.script_guard(tick, ids, member, point, area, GuardTarget::Point)?;
                }
            }
        }
        diag!("script: {} orders {} unit(s)", self.verb, members.len());
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.side.is_none().then_some(self.name.as_str())
    }
}

fn build(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    verb: &'static str,
    what: impl FnOnce(&[Parameter], &ParseCtx<'_>) -> Option<What>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionGuard {
        name: parameter_string(params, 0)?,
        side: (!named).then(|| ctx.side.to_owned()),
        what: what(params, ctx)?,
        verb,
    }))
}

fn waypoint(params: &[Parameter], ctx: &ParseCtx<'_>, index: usize) -> Option<Vec2> {
    let name = parameter_string(params, index)?;
    ctx.waypoints.get(&name).map(|(position, _)| *position)
}

fn area(params: &[Parameter], ctx: &ParseCtx<'_>, from: bool) -> Option<What> {
    Some(What::GuardArea {
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        from: from.then(|| waypoint(params, ctx, 2)),
    })
}

fn attack_area(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<What> {
    Some(What::AttackArea(Arc::new(Area::resolve(
        ctx,
        &parameter_string(params, 1)?,
    ))))
}

macro_rules! rows {
    ($($fn_name:ident, $named:expr, $verb:literal, $what:expr;)*) => {
        $(pub(super) fn $fn_name(
            params: &[Parameter],
            ctx: &ParseCtx<'_>,
        ) -> Option<Arc<dyn ActionVerb>> {
            build(params, ctx, $named, $verb, $what)
        })*
    };
}

rows! {
    parse_team_guard, false, "TEAM_GUARD", |_, _| Some(What::GuardHere);
    parse_named_guard, true, "NAMED_GUARD", |_, _| Some(What::GuardHere);
    parse_team_guard_object, false, "TEAM_GUARD_OBJECT",
        |p, _| parameter_string(p, 1).map(What::GuardObject);
    parse_team_guard_team, false, "TEAM_GUARD_TEAM",
        |p, _| parameter_string(p, 1).map(What::GuardTeam);
    parse_team_guard_position, false, "TEAM_GUARD_POSITION",
        |p, c| Some(What::GuardPoint(waypoint(p, c, 1)));
    parse_team_guard_area, false, "TEAM_GUARD_AREA", |p, c| area(p, c, false);
    parse_team_guard_area_from, false, "TEAM_GUARD_AREA_FROM_POSITION", |p, c| area(p, c, true);
    parse_unit_guard_area, true, "UNIT_GUARD_AREA", |p, c| area(p, c, false);
    parse_unit_guard_area_from, true, "UNIT_GUARD_AREA_FROM_POSITION", |p, c| area(p, c, true);
    parse_team_stand_ground, false, "TEAM_STAND_GROUND",
        |p, _| Some(What::StandGround(parameter_int(p, 1)? != 0));
    parse_unit_stand_ground, true, "UNIT_STAND_GROUND",
        |p, _| Some(What::StandGround(parameter_int(p, 1)? != 0));
    parse_team_attack_area, false, "TEAM_ATTACK_AREA", attack_area;
    parse_named_attack_area, true, "NAMED_ATTACK_AREA", attack_area;
}
