use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::orders::ScriptOrder;
use super::status::ScriptStatusBit;
use super::{parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

const WANDER_SET: &str = "WANDER";
const PANIC_SET: &str = "PANIC";

enum Order {
    StopAndDisband,
    WanderInPlace,
    Wander { path: String },
    Panic { path: String },
    MoveToNearest { types: String },
}

struct ActionTeamOrder {
    team: String,
    side: String,
    order: Order,
}

impl RotwkRules {
    fn release_script_orders(state: &mut MapScriptState, members: &[Entity]) {
        for &member in members {
            state.rt.walks.forget(member);
            state.rt.attack_areas.forget(member);
            state.rt.wanderers.forget(member);
        }
    }

    fn stop_and_disband(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        team: super::team::TeamId,
        members: &[Entity],
    ) -> Result<()> {
        Self::release_script_orders(state, members);
        self.issue_script_order(tick, ids, members, ScriptOrder::Stop)?;
        for &member in members {
            state
                .rt
                .statuses
                .set(member, ScriptStatusBit::NotRecruitable, false);
        }
        match self.default_team_of(engine, state, team) {
            Some(default) if default != team => {
                for member in self.live_members(tick, state, team) {
                    self.move_to_team(tick, ids, engine, state, member, default)?;
                }
                if let Some(instance) = state.rt.teams.get_mut(default) {
                    instance.set_active();
                }
            }
            Some(_) => {}
            None => state.rt.teams.disband(team, None),
        }
        Ok(())
    }

    fn wander_in_place(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        members: &[Entity],
    ) -> Result<()> {
        for &member in members {
            if !self.has_ai(tick.state(), ids, member) {
                continue;
            }
            let Some(origin) = self.position_of_entity(tick, ids, member) else {
                continue;
            };
            self.apply_locomotor_override(tick, ids, member, Some(WANDER_SET))?;
            state.rt.walks.forget(member);
            state.rt.attack_areas.forget(member);
            state.rt.wanderers.begin(member, origin);
        }
        Ok(())
    }

    fn wander_along(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        members: &[Entity],
        path: &str,
    ) -> Result<()> {
        for &member in members {
            if !self.has_ai(tick.state(), ids, member) {
                continue;
            }
            let Some(from) = self.position_of_entity(tick, ids, member) else {
                continue;
            };
            let Some(start) = Self::nearest_on_path(engine, path, from) else {
                break;
            };
            Self::release_script_orders(state, &[member]);
            self.apply_locomotor_override(tick, ids, member, Some(WANDER_SET))?;
            state.rt.walks.begin_own(member, path);
            state.rt.wanderers.begin_path(member, path, start, false);
        }
        Ok(())
    }

    fn panic_along(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        members: &[Entity],
        path: &str,
    ) -> Result<()> {
        for &member in members {
            if !self.has_ai(tick.state(), ids, member) {
                continue;
            }
            let Some(from) = self.position_of_entity(tick, ids, member) else {
                continue;
            };
            let Some(start) = Self::nearest_on_path(engine, path, from) else {
                break;
            };
            Self::release_script_orders(state, &[member]);
            self.apply_locomotor_override(tick, ids, member, Some(PANIC_SET))?;
            self.set_model_condition(tick, ids, member, "PANICKING", true)?;
            state.rt.walks.begin_own(member, path);
            state.rt.wanderers.begin_path(member, path, start, true);
        }
        Ok(())
    }
}

impl ActionVerb for ActionTeamOrder {
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
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return Ok(());
        };
        let members = rules.subject_members(tick, ids, state, &self.team, Some(&self.side));
        match &self.order {
            Order::StopAndDisband => {
                rules.stop_and_disband(tick, ids, engine, state, team, &members)
            }
            Order::WanderInPlace => rules.wander_in_place(tick, ids, state, &members),
            Order::Wander { path } => rules.wander_along(tick, ids, engine, state, &members, path),
            Order::Panic { path } => rules.panic_along(tick, ids, engine, state, &members, path),
            Order::MoveToNearest { types } => {
                if members.is_empty() {
                    return Ok(());
                }
                let Some(centre) = rules.team_position(tick, ids, state, team) else {
                    return Ok(());
                };
                let names = state.rt.lists.types_of(types);
                let Some(found) = rules.nearest_of_types(tick, ids, &names, centre, None, false)
                else {
                    return Ok(());
                };
                let Some(to) = rules.position_of_entity(tick, ids, found) else {
                    return Ok(());
                };
                RotwkRules::release_script_orders(state, &members);
                diag!(
                    "script: {} unit(s) head for the nearest {types:?}",
                    members.len()
                );
                rules.issue_script_order(tick, ids, &members, ScriptOrder::Move(to, false))
            }
        }
    }
}

fn order(params: &[Parameter], ctx: &ParseCtx<'_>, order: Order) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamOrder {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        order,
    }))
}

pub(super) fn parse_stop_and_disband(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    order(params, ctx, Order::StopAndDisband)
}

pub(super) fn parse_wander_in_place(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    order(params, ctx, Order::WanderInPlace)
}

pub(super) fn parse_wander(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let path = parameter_string(params, 1)?;
    order(params, ctx, Order::Wander { path })
}

pub(super) fn parse_panic(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    let path = parameter_string(params, 1)?;
    order(params, ctx, Order::Panic { path })
}

pub(super) fn parse_move_to_nearest(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let types = parameter_string(params, 1)?;
    order(params, ctx, Order::MoveToNearest { types })
}
