use mint_movement::Vec2;
use mint_sim::{CommandKey, Entity, PlayerId, Result, Rules, Tick, World};

use super::orders::ScriptOrder;
use super::status::ScriptStatusBit;
use super::team::TeamId;
use super::team_build::WorkOrder;
use super::team_build_step::creation_order;
use super::{MapScriptEngine, MapScriptState};
use crate::diag::diag;
use crate::rules::{Ids, RotwkCommand, RotwkRules};
use crate::unit::Kind;

const UNBOUNDED_RADIUS: f32 = 100_000.0;

impl RotwkRules {
    pub(super) fn queue_units(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
    ) -> Result<()> {
        let queued = state
            .rt
            .builders
            .get(player)
            .map_or(0, |builder| builder.build.len());
        for at in 0..queued {
            let Some(entry) = state
                .rt
                .builders
                .get(player)
                .and_then(|builder| builder.build.get(at))
                .cloned()
            else {
                continue;
            };
            let home = self.team_home(engine, state, entry.team);
            let (origin, radius) = home.map_or((Vec2::new(0.0, 0.0), UNBOUNDED_RADIUS), |at| {
                (at, self.ai_timing.max_recruit_radius)
            });
            let mut orders = entry.orders;
            for order in &mut orders {
                while order.completed < order.required {
                    let Some(unit) = self.try_to_recruit(
                        tick.state(),
                        ids,
                        engine,
                        state,
                        player,
                        entry.team,
                        order.kind,
                        origin,
                        radius,
                    ) else {
                        break;
                    };
                    order.completed = order.completed.saturating_add(1);
                    diag!(
                        "script: team-build player {} recruits {:?} into team {}",
                        player.get(),
                        order.kind,
                        entry.team.get()
                    );
                    state.rt.teams.add_member(entry.team, unit);
                    state.rt.stand_ground.set(unit, false);
                    state.rt.builders.remember_slot(unit, order);
                    let command = home.map_or(ScriptOrder::Stop, |at| ScriptOrder::Move(at, false));
                    self.issue_script_order(tick, ids, &[unit], command)?;
                    if let Some(instance) = state.rt.teams.get_mut(entry.team) {
                        instance.set_active();
                    }
                }
                if order.completed < order.required && !order.optional {
                    self.start_training(tick, ids, player, order, entry.priority)?;
                } else {
                    self.validate_factory(tick.state(), ids, player, order);
                }
            }
            if let Some(slot) = state
                .rt
                .builders
                .get_mut(player)
                .and_then(|builder| builder.build.get_mut(at))
            {
                slot.orders = orders;
            }
        }
        Ok(())
    }

    pub(super) fn team_home(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        team: TeamId,
    ) -> Option<Vec2> {
        let proto = engine.team_protos.get(state.rt.teams.get(team)?.proto)?;
        engine.waypoint(proto.template.home.as_deref()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_to_recruit(
        &self,
        world: &World,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        player: PlayerId,
        team: TeamId,
        kind: Kind,
        origin: Vec2,
        radius: f32,
    ) -> Option<Entity> {
        let mine = engine
            .team_protos
            .get(state.rt.teams.get(team)?.proto)?
            .template
            .production_priority;
        let owners = world.store(ids.owner)?;
        let walk: Vec<(Entity, ())> = owners.iter().map(|(entity, _)| (entity, ())).collect();
        let mut best = radius * radius;
        let mut recruit = None;
        for (entity, ()) in creation_order(walk) {
            if world.get(ids.owner, entity).is_none_or(|o| o.0 != player)
                || !world.is_live(entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.member, entity)
                || state
                    .rt
                    .statuses
                    .has(entity, ScriptStatusBit::NotRecruitable)
                || world
                    .get(ids.kind, entity)
                    .is_none_or(|have| !self.same_template(*have, kind))
            {
                continue;
            }
            let default = match state.rt.teams.team_of(entity) {
                None => true,
                Some(other) => {
                    let Some(instance) = state.rt.teams.get(other) else {
                        continue;
                    };
                    if !instance.active {
                        continue;
                    }
                    let default = self.is_default_team(engine, state, other);
                    if !default {
                        let Some(template) =
                            engine.team_protos.get(instance.proto).map(|p| &p.template)
                        else {
                            continue;
                        };
                        if template.production_priority >= mine || !template.ai_recruitable {
                            continue;
                        }
                    }
                    default
                }
            };
            let Some(at) = world.get(ids.body, entity).map(|body| body.0.position) else {
                continue;
            };
            let (dx, dy) = (origin.x - at.x, origin.y - at.y);
            let distance = dx * dx + dy * dy;
            if default && recruit.is_none() {
                best = distance;
            } else if distance > best {
                continue;
            } else {
                best = distance;
            }
            recruit = Some(entity);
        }
        recruit
    }

    fn start_training(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        order: &mut WorkOrder,
        busy_ok: bool,
    ) -> Result<()> {
        if order.factory.is_some() {
            return Ok(());
        }
        let Some(factory) = self.find_factory(tick.state(), ids, player, order.kind, busy_ok)
        else {
            return Ok(());
        };
        if !factory.idle {
            return Ok(());
        }
        let Some(button) = self.button_for(tick.state(), ids, factory.entity, order.kind) else {
            return Ok(());
        };
        let now = tick.frame();
        self.apply(
            tick,
            CommandKey::new(now, player, 0),
            &RotwkCommand::SetSelection {
                replace: true,
                units: vec![factory.entity],
            },
        )?;
        self.apply(
            tick,
            CommandKey::new(now, player, 1),
            &RotwkCommand::QueueUnit { button },
        )?;
        let queued = tick
            .state()
            .get(ids.producer, factory.entity)
            .is_some_and(|producer| !producer.queue.entries().is_empty());
        if queued {
            order.factory = Some(factory.entity);
        }
        Ok(())
    }

    fn button_for(&self, world: &World, ids: Ids, factory: Entity, kind: Kind) -> Option<u8> {
        let entry = self.kinds.get(*world.get(ids.kind, factory)?)?;
        entry
            .builds
            .iter()
            .position(|built| *built == kind)
            .or_else(|| {
                entry
                    .builds
                    .iter()
                    .position(|built| self.same_template(*built, kind))
            })
            .and_then(|position| u8::try_from(position).ok())
    }

    fn validate_factory(&self, world: &World, ids: Ids, player: PlayerId, order: &mut WorkOrder) {
        if let Some(factory) = order.factory {
            let ours = world.is_live(factory)
                && world
                    .get(ids.owner, factory)
                    .is_some_and(|owner| owner.0 == player);
            if !ours {
                order.factory = None;
            }
        }
    }

    pub(in crate::rules) fn team_builder_unit_produced(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        factory: Entity,
        unit: Entity,
        kind: Kind,
    ) -> Result<()> {
        let (Some(engine), Some((anchor, state))) = (
            self.map_scripts.as_ref(),
            tick.state()
                .store(ids.map_script_state)
                .and_then(|store| store.iter().next()),
        ) else {
            return Ok(());
        };
        if state.rt.builders.get(owner).is_none() {
            return Ok(());
        }
        let mut state = state.clone();
        let found = state.rt.builders.get(owner).and_then(|builder| {
            builder.build.iter().enumerate().find_map(|(at, entry)| {
                entry
                    .orders
                    .iter()
                    .position(|order| {
                        order.factory == Some(factory)
                            && order.completed < order.required
                            && self.same_template(order.kind, kind)
                    })
                    .map(|slot| (at, slot))
            })
        });
        let mut home = None;
        if let Some((at, slot)) = found {
            let taken = state
                .rt
                .builders
                .get_mut(owner)
                .and_then(|builder| builder.build.get_mut(at))
                .and_then(|entry| {
                    let order = entry.orders.get_mut(slot)?;
                    order.completed = order.completed.saturating_add(1);
                    order.factory = None;
                    Some((entry.team, order.clone()))
                });
            if let Some((team, order)) = taken {
                diag!(
                    "script: team-build player {} trained {:?} into team {}",
                    owner.get(),
                    order.kind,
                    team.get()
                );
                state.rt.teams.add_member(team, unit);
                state.rt.builders.remember_slot(unit, &order);
                home = self.team_home(engine, &state, team);
            }
        }
        if let Some(builder) = state.rt.builders.get_mut(owner) {
            builder.team_delay = 0;
        }
        tick.state_mut()
            .insert(ids.map_script_state, anchor, state)?;
        if let Some(at) = home {
            self.issue_script_order(tick, ids, &[unit], ScriptOrder::Move(at, false))?;
        }
        Ok(())
    }
}
