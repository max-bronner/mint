use mint_movement::Vec2;
use mint_sage_modules::effects::{Effect, Name};
use mint_sim::{Component, Entity, Frame, PlayerId, Result, StateHasher, Tick};

use super::map_scripts::Relation;
use super::{Ids, RotwkRules};
use crate::diag::diag;

const DOOR_OPENING: &str = "DOOR_1_OPENING";
const DOOR_CLOSING: &str = "DOOR_1_CLOSING";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GatePhase {
    Idle,
    Opening,
    Closing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateAction {
    Open,
    Close,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GateCycle {
    open: bool,
    phase: GatePhase,
    ends_at: Frame,
    pathing_at: Frame,
    open_for_pathing: bool,
    held_open: Frame,
    commanded: Option<bool>,
}

impl GateCycle {
    fn at_rest(open: bool, now: Frame) -> Self {
        Self {
            open,
            phase: GatePhase::Idle,
            ends_at: now,
            pathing_at: now,
            open_for_pathing: open,
            held_open: Frame::new(0),
            commanded: None,
        }
    }

    pub(crate) fn may_operate(&self, under_construction: bool, dead: bool) -> bool {
        self.phase == GatePhase::Idle && !under_construction && !dead
    }

    pub(crate) fn fully_open(&self) -> bool {
        self.open && self.phase == GatePhase::Idle
    }
}

impl Component for GateCycle {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(u8::from(self.open));
        hasher.write_u8(match self.phase {
            GatePhase::Idle => 0,
            GatePhase::Opening => 1,
            GatePhase::Closing => 2,
        });
        hasher.write_u64(self.ends_at.get());
        hasher.write_u64(self.pathing_at.get());
        hasher.write_u8(u8::from(self.open_for_pathing));
        hasher.write_u64(self.held_open.get());
        hasher.write_u8(match self.commanded {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GateBox {
    allies: u32,
    enemies: u32,
    inside: Vec<Entity>,
}

impl GateBox {
    fn observe(&mut self, object: Entity, inside_now: bool, relation: Relation) {
        let at = self.inside.binary_search(&object);
        if inside_now == at.is_ok() {
            return;
        }
        let counter = match relation {
            Relation::Allies => Some(&mut self.allies),
            Relation::Enemies => Some(&mut self.enemies),
            Relation::Neutral => None,
        };
        match at {
            Err(place) => {
                self.inside.insert(place, object);
                if let Some(counter) = counter {
                    *counter = counter.saturating_add(1);
                }
            }
            Ok(place) => {
                self.inside.remove(place);
                if let Some(counter) = counter {
                    *counter = counter.saturating_sub(1);
                }
            }
        }
    }

    fn ally_inside(&self) -> bool {
        self.allies > 0
    }
}

impl Component for GateBox {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.allies);
        hasher.write_u32(self.enemies);
        hasher.write_usize(self.inside.len());
        for entity in &self.inside {
            entity.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoxSeen(i32, i32);

impl BoxSeen {
    #[allow(clippy::cast_possible_truncation)]
    fn of(at: Vec2) -> Self {
        Self(at.x as i32, at.y as i32)
    }

    #[allow(clippy::cast_precision_loss)]
    fn point(self) -> Vec2 {
        Vec2::new(self.0 as f32, self.1 as f32)
    }
}

impl Component for BoxSeen {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0);
        hasher.write_i32(self.1);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateButtons {
    pub(crate) may_operate: bool,
    pub(crate) open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DynamicPortalStamp {
    active: bool,
    centre: Vec2,
    angle: f32,
}

impl Component for DynamicPortalStamp {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(u8::from(self.active));
        hasher.write_f32(self.centre.x);
        hasher.write_f32(self.centre.y);
        hasher.write_f32(self.angle);
    }
}

impl RotwkRules {
    pub(super) fn run_gates(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        self.run_wall_gates(tick, ids)?;
        self.run_dynamic_portals(tick, ids)
    }

    fn run_wall_gates(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let gates = wiring.portal_entities(tick.state());
        if gates.is_empty() {
            return Ok(());
        }
        self.track_gate_boxes(tick, ids, &gates)?;
        let now = tick.frame();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for gate in gates {
            if !tick.state().contains(ids.body, gate) {
                continue;
            }
            let Some(owner) = tick.state().get(ids.owner, gate).map(|owner| owner.0) else {
                continue;
            };
            if wiring.portal(tick.state(), gate).is_none() {
                continue;
            }
            let cycle_data = wiring.gate_cycle(tick.state(), gate);
            let open_by_default = cycle_data
                .is_some_and(mint_sage_modules::GateOpenAndCloseBehavior::open_by_default);
            let reset = cycle_data.map_or(0, |cycle| cycle.reset_time().get());
            let percent = cycle_data.map_or(
                0.0,
                mint_sage_modules::GateOpenAndCloseBehavior::percent_open_for_pathing,
            );
            let triggered = tick
                .state()
                .get(ids.gate_box, gate)
                .is_some_and(GateBox::ally_inside);
            let mut cycle = tick
                .state()
                .get(ids.gate_cycle, gate)
                .copied()
                .unwrap_or_else(|| GateCycle::at_rest(open_by_default, now));
            let box_asked = triggered;
            let is_skirmish_ai = self.ai_seeds.iter().any(|seed| seed.player == owner);
            let asked = if is_skirmish_ai {
                box_asked
            } else {
                cycle.commanded.unwrap_or(open_by_default)
            };
            let target = asked || now < cycle.held_open;
            let under_construction = tick.state().contains(ids.construction, gate);
            let dead = tick
                .state()
                .get(ids.health, gate)
                .is_some_and(|health| health.0.is_dead());
            if cycle.may_operate(under_construction, dead) && target != cycle.open {
                cycle.phase = if target {
                    GatePhase::Opening
                } else {
                    GatePhase::Closing
                };
                cycle.ends_at = Frame::new(now.get().saturating_add(u64::from(reset)));
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let to_pathing = (percent.clamp(0.0, 1.0) * reset as f32) as u32;
                cycle.pathing_at = Frame::new(now.get().saturating_add(u64::from(to_pathing)));
                diag!(
                    "gate: entity {} starts {} over {reset} frame(s)",
                    gate.index(),
                    if target { "opening" } else { "closing" }
                );
            }
            let was_open_for_pathing = cycle.open_for_pathing;
            if cycle.phase != GatePhase::Idle {
                if now.get() >= cycle.pathing_at.get() {
                    cycle.open_for_pathing = cycle.phase == GatePhase::Opening;
                }
                if now.get() >= cycle.ends_at.get() {
                    cycle.open = cycle.phase == GatePhase::Opening;
                    cycle.phase = GatePhase::Idle;
                }
            }
            let (add, remove) = if cycle.phase == GatePhase::Opening
                || (cycle.phase == GatePhase::Idle && cycle.open)
            {
                (DOOR_OPENING, DOOR_CLOSING)
            } else {
                (DOOR_CLOSING, DOOR_OPENING)
            };
            let resting = tick.state().get(wiring.state, gate).is_some_and(|state| {
                state.has_model_condition(add) && !state.has_model_condition(remove)
            });
            if !resting {
                effects.push((
                    gate,
                    Effect::ModelConditions {
                        add: vec![Name::from(add)].into(),
                        remove: vec![Name::from(remove)].into(),
                    },
                ));
            }
            if cycle.open_for_pathing != was_open_for_pathing {
                self.swap_gate_geometry(tick, ids, gate, cycle.open_for_pathing)?;
                diag!(
                    "gate: entity {} passage {}",
                    gate.index(),
                    if cycle.open_for_pathing {
                        "open"
                    } else {
                        "shut"
                    }
                );
            }
            self.set_gate_link(
                tick,
                ids,
                gate,
                is_skirmish_ai && !cycle.fully_open() && !dead,
            )?;
            tick.state_mut().insert(ids.gate_cycle, gate, cycle)?;
        }
        self.apply_effects(tick, ids, effects)
    }

    fn run_dynamic_portals(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let entities = wiring.dynamic_portal_entities(tick.state());
        if entities.is_empty() {
            return Ok(());
        }
        let now = tick.frame();
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let mut changes: Vec<(Entity, DynamicPortalStamp, Vec<mint_movement::PortalLink>)> =
            Vec::new();
        for entity in entities {
            let world = tick.state();
            let Some(module) = wiring.dynamic_portal(world, entity) else {
                continue;
            };
            let Some(body) = world.get(ids.body, entity).map(|body| body.0) else {
                continue;
            };
            let bones = world
                .get(ids.kind, entity)
                .and_then(|kind| self.portal_bones.get(usize::from(kind.0)))
                .map_or(&[][..], Vec::as_slice);
            let active = (module.is_active(now) || Self::siege_deployed(world, ids, entity))
                && !bones.is_empty();
            let want = DynamicPortalStamp {
                active,
                centre: body.position,
                angle: body.facing.radians(),
            };
            if world.get(ids.dynamic_portal_stamp, entity) == Some(&want) {
                continue;
            }
            let links = if active {
                super::portal_links::PortalSite {
                    entity,
                    waypoints: module.waypoints(),
                    links: module.links(),
                    bones,
                    centre: want.centre,
                    angle: want.angle,
                    geometry,
                    extent,
                }
                .links()
            } else {
                Vec::new()
            };
            changes.push((entity, want, links));
        }
        for (entity, stamp, links) in changes {
            let clock = tick
                .state()
                .store(ids.portals)
                .and_then(|store| store.entities().first().copied());
            if let Some(layer) =
                clock.and_then(|clock| tick.state_mut().get_mut(ids.portals, clock))
            {
                for link in &links {
                    diag!(
                        "portal: entity {} link from cell ({}, {}) to ({}, {}), {} node(s)",
                        entity.index(),
                        link.entry.x,
                        link.entry.y,
                        link.exit.x,
                        link.exit.y,
                        link.nodes.len()
                    );
                }
                if links.is_empty() {
                    layer.unregister(entity);
                } else {
                    layer.register(entity, links);
                }
                diag!(
                    "portal: entity {} {} ({} link(s) on the layer)",
                    entity.index(),
                    if stamp.active {
                        "routes"
                    } else {
                        "stands down"
                    },
                    layer.links().len()
                );
            }
            tick.state_mut()
                .insert(ids.dynamic_portal_stamp, entity, stamp)?;
        }
        Ok(())
    }

    pub(super) fn open_gate_on_hit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        gate: Entity,
    ) -> Result<bool> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(false);
        };
        let Some(reset) = wiring
            .gate_cycle(tick.state(), gate)
            .map(|cycle| cycle.reset_time().get())
        else {
            return Ok(false);
        };
        let Some(mut cycle) = tick.state().get(ids.gate_cycle, gate).copied() else {
            return Ok(false);
        };
        let now = tick.frame();
        cycle.held_open = Frame::new(now.get().saturating_add(u64::from(reset)));
        tick.state_mut().insert(ids.gate_cycle, gate, cycle)?;
        diag!(
            "gate: entity {} held open until frame {}",
            gate.index(),
            cycle.held_open.get()
        );
        Ok(true)
    }

    pub(super) fn command_gate(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        action: GateAction,
    ) -> Result<()> {
        for gate in self.selection(tick, ids, player) {
            self.command_gate_entity(tick, ids, gate, action)?;
        }
        Ok(())
    }

    pub(super) fn command_gate_entity(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        gate: Entity,
        action: GateAction,
    ) -> Result<bool> {
        let Some(data) = self
            .modules
            .as_ref()
            .and_then(|wiring| wiring.gate_cycle(tick.state(), gate))
        else {
            return Ok(false);
        };
        let open_by_default = data.open_by_default();
        let now = tick.frame();
        let mut cycle = tick
            .state()
            .get(ids.gate_cycle, gate)
            .copied()
            .unwrap_or_else(|| GateCycle::at_rest(open_by_default, now));
        let under_construction = tick.state().contains(ids.construction, gate);
        let dead = tick
            .state()
            .get(ids.health, gate)
            .is_some_and(|health| health.0.is_dead());
        if !cycle.may_operate(under_construction, dead) {
            return Ok(false);
        }
        let commanded = match action {
            GateAction::Open => true,
            GateAction::Close => false,
            GateAction::Toggle => !cycle.open,
        };
        cycle.commanded = Some(commanded);
        tick.state_mut().insert(ids.gate_cycle, gate, cycle)?;
        diag!(
            "gate: entity {} commanded to {}",
            gate.index(),
            if commanded { "open" } else { "close" }
        );
        Ok(true)
    }

    pub(super) fn gate_is_open(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        gate: Entity,
    ) -> Option<bool> {
        let data = self.modules.as_ref()?.gate_cycle(world, gate)?;
        Some(
            world
                .get(ids.gate_cycle, gate)
                .map_or_else(|| data.open_by_default(), |cycle| cycle.fully_open()),
        )
    }

    pub(crate) fn gate_buttons(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        gate: Entity,
    ) -> Option<GateButtons> {
        let cycle_data = self.modules.as_ref()?.gate_cycle(world, gate)?;
        let under_construction = world.contains(ids.construction, gate);
        let dead = world
            .get(ids.health, gate)
            .is_some_and(|health| health.0.is_dead());
        let (may_operate, open) = match world.get(ids.gate_cycle, gate) {
            Some(cycle) => (cycle.may_operate(under_construction, dead), cycle.open),
            None => (!under_construction && !dead, cycle_data.open_by_default()),
        };
        Some(GateButtons { may_operate, open })
    }

    fn track_gate_boxes(&self, tick: &mut Tick<'_>, ids: Ids, gates: &[Entity]) -> Result<()> {
        let mut boxes: Vec<(Entity, Vec2, f32, Vec2)> = Vec::new();
        if let Some(wiring) = self.modules.as_ref() {
            for &gate in gates {
                let world = tick.state();
                let (Some(body), Some(owner), Some(trigger)) = (
                    world.get(ids.body, gate).map(|body| body.0),
                    world.get(ids.owner, gate).map(|owner| owner.0),
                    wiring.gate_trigger(world, gate),
                ) else {
                    continue;
                };
                if !self.ai_seeds.iter().any(|seed| seed.player == owner) {
                    continue;
                }
                let half = Vec2::new(
                    trigger.trigger_width_x() * 0.5,
                    trigger.trigger_width_y() * 0.5,
                );
                boxes.push((gate, body.position, body.facing.radians(), half));
            }
        }
        if boxes.is_empty() {
            return Ok(());
        }
        let primed = tick
            .state()
            .store(ids.box_seen)
            .is_some_and(|store| !store.is_empty());
        for &(gate, ..) in &boxes {
            if !tick.state().contains(ids.gate_box, gate) {
                tick.state_mut()
                    .insert(ids.gate_box, gate, GateBox::default())?;
            }
        }
        let objects: Vec<Entity> = tick
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for object in objects {
            let world = tick.state();
            let Some(at) = world.get(ids.body, object).map(|body| body.0.position) else {
                continue;
            };
            let seen = world.get(ids.box_seen, object).copied();
            let here = BoxSeen::of(at);
            if seen == Some(here)
                || !world.is_live(object)
                || world.contains(ids.doomed, object)
                || world.contains(ids.contained, object)
                || world
                    .get(ids.health, object)
                    .is_some_and(|health| health.0.is_dead())
                || world
                    .get(ids.kind, object)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| {
                        entry.record.has_kind("PROJECTILE") || entry.record.has_kind("INERT")
                    })
            {
                continue;
            }
            tick.state_mut().insert(ids.box_seen, object, here)?;
            if seen.is_none() && !primed {
                continue;
            }
            for &(gate, centre, angle, half) in &boxes {
                if gate == object {
                    continue;
                }
                let inside = in_box(centre, angle, half, here.point());
                let world = tick.state();
                let relation = self.object_relation(world, object, gate);
                if let Some(mut counted) = world.get(ids.gate_box, gate).cloned() {
                    counted.observe(object, inside, relation);
                    tick.state_mut().insert(ids.gate_box, gate, counted)?;
                }
            }
        }
        Ok(())
    }

    fn swap_gate_geometry(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        gate: Entity,
        open: bool,
    ) -> Result<()> {
        let (Some(cycle_data), Some(kind), Some(body)) = (
            self.modules
                .as_ref()
                .and_then(|wiring| wiring.gate_cycle(tick.state(), gate)),
            tick.state().get(ids.kind, gate).copied(),
            tick.state().get(ids.body, gate).map(|body| body.0),
        ) else {
            return Ok(());
        };
        let Some(entry) = self.kinds.get(kind) else {
            return Ok(());
        };
        let overrides: Vec<(Name, bool)> = cycle_data
            .geometry_for_open()
            .iter()
            .map(|name| (Name::from(name.as_str()), open))
            .chain(
                cycle_data
                    .geometry_for_closed()
                    .iter()
                    .map(|name| (Name::from(name.as_str()), !open)),
            )
            .collect();
        let records = crate::construction::shown_footprint(&entry.record, &overrides);
        let Some(clock) = tick
            .state()
            .store(ids.obstacles)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.obstacles, clock).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        if layer.cells_of(geometry, extent, gate).is_none() {
            return Ok(());
        }
        if let Err(message) = layer.stamp(
            &self.passability.grid,
            geometry,
            extent,
            gate,
            body.position,
            body.facing.radians(),
            records,
            false,
        ) {
            diag!("obstacles: {message}");
        }
        tick.state_mut().insert(ids.obstacles, clock, layer)?;
        Ok(())
    }

    fn set_gate_link(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        gate: Entity,
        wanted: bool,
    ) -> Result<()> {
        let Some(clock) = tick
            .state()
            .store(ids.portals)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let held = tick
            .state()
            .get(ids.portals, clock)
            .is_some_and(|layer| layer.holds(gate));
        if held == wanted {
            return Ok(());
        }
        let links = if wanted {
            let (Some(body), Some(kind)) = (
                tick.state().get(ids.body, gate).map(|body| body.0),
                tick.state().get(ids.kind, gate).copied(),
            ) else {
                return Ok(());
            };
            let Some(entry) = self.kinds.get(kind) else {
                return Ok(());
            };
            let geometry = &entry.record.geometry;
            let reach = geometry
                .major_radius
                .filter(|reach| *reach > 0.0)
                .or(geometry.minor_radius)
                .unwrap_or(0.0);
            super::portal_links::fake_gate_links(
                gate,
                body.position,
                body.facing.radians(),
                reach,
                self.passability.geometry,
                self.passability.grid.extent(),
            )
        } else {
            Vec::new()
        };
        if let Some(layer) = tick.state_mut().get_mut(ids.portals, clock) {
            if links.is_empty() {
                layer.unregister(gate);
            } else {
                layer.register(gate, links);
            }
        }
        diag!(
            "gate: entity {} ai link {}",
            gate.index(),
            if wanted { "registered" } else { "removed" }
        );
        Ok(())
    }
}

fn in_box(centre: Vec2, angle: f32, half: Vec2, point: Vec2) -> bool {
    let (sin, cos) = (mint_math::f32::sin(angle), mint_math::f32::cos(angle));
    let to = point.minus(centre);
    let along = to.x * cos + to.y * sin;
    let across = to.y * cos - to.x * sin;
    along.abs() <= half.x && across.abs() <= half.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_sees_whole_units_truncated_toward_zero() {
        assert_eq!(
            BoxSeen::of(Vec2::new(1.9, -1.9)),
            BoxSeen::of(Vec2::new(1.1, -1.2))
        );
        assert_eq!(
            BoxSeen::of(Vec2::new(1.9, -1.9)).point(),
            Vec2::new(1.0, -1.0)
        );
    }

    #[test]
    fn the_box_counts_allies_only_and_a_death_inside_stays_counted() {
        let mut counted = GateBox::default();
        let (neutral, enemy, ally) = (
            Entity::from_parts(1, 0),
            Entity::from_parts(2, 0),
            Entity::from_parts(3, 0),
        );
        counted.observe(neutral, true, Relation::Neutral);
        counted.observe(enemy, true, Relation::Enemies);
        assert!(!counted.ally_inside());
        counted.observe(ally, true, Relation::Allies);
        assert!(counted.ally_inside());
        counted.observe(ally, true, Relation::Allies);
        assert!(counted.ally_inside(), "a repeat entry counts once");
        counted.observe(ally, false, Relation::Allies);
        assert!(!counted.ally_inside(), "its own exit uncounts it");
        counted.observe(ally, true, Relation::Allies);
        assert!(
            counted.ally_inside(),
            "no exit is ever observed for the dead"
        );
    }

    #[test]
    fn a_trigger_box_is_read_in_the_gates_own_frame() {
        let centre = Vec2::new(100.0, 100.0);
        let half = Vec2::new(150.0, 75.0);
        assert!(in_box(centre, 0.0, half, Vec2::new(240.0, 100.0)));
        assert!(!in_box(
            centre,
            core::f32::consts::FRAC_PI_2,
            half,
            Vec2::new(240.0, 100.0)
        ));
        assert!(in_box(
            centre,
            core::f32::consts::FRAC_PI_2,
            half,
            Vec2::new(100.0, 240.0)
        ));
    }
}
