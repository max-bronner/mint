use mint_movement::Vec2;
use mint_sage_modules::effects::{Effect, Name};
use mint_sim::{Component, Entity, Frame, PlayerId, Result, StateHasher, Tick};

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
    stamped: u32,
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
            stamped: u32::MAX,
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
        hasher.write_u32(self.stamped);
        hasher.write_u64(self.held_open.get());
        hasher.write_u8(match self.commanded {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GateButtons {
    pub(crate) may_operate: bool,
    pub(crate) open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DynamicPortalStamp {
    stamped: u32,
}

impl Component for DynamicPortalStamp {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.stamped);
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
        let movers = self.movers_for_gate_triggers(tick, ids);
        let now = tick.frame();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        for gate in gates {
            let Some(body) = tick.state().get(ids.body, gate).map(|body| body.0) else {
                continue;
            };
            let Some(owner) = tick.state().get(ids.owner, gate).map(|owner| owner.0) else {
                continue;
            };
            let Some(portal) = wiring.portal(tick.state(), gate) else {
                continue;
            };
            let admitted = self.admitted_askers(
                owner,
                true,
                portal.allow_enemies(),
                portal.allow_non_skirmish_ai_units(),
            );
            let cycle_data = wiring.gate_cycle(tick.state(), gate);
            let open_by_default = cycle_data
                .is_some_and(mint_sage_modules::GateOpenAndCloseBehavior::open_by_default);
            let reset = cycle_data.map_or(0, |cycle| cycle.reset_time().get());
            let percent = cycle_data.map_or(
                0.0,
                mint_sage_modules::GateOpenAndCloseBehavior::percent_open_for_pathing,
            );
            let triggered = wiring
                .gate_trigger(tick.state(), gate)
                .is_some_and(|trigger| {
                    let half = Vec2::new(
                        trigger.trigger_width_x() * 0.5,
                        trigger.trigger_width_y() * 0.5,
                    );
                    movers.iter().any(|(mover_owner, at)| {
                        admitted & crate::rules::gates::asker_bits(*mover_owner) != 0
                            && in_box(body.position, body.facing.radians(), half, *at)
                    })
                });
            let mut cycle = tick
                .state()
                .get(ids.gate_cycle, gate)
                .copied()
                .unwrap_or_else(|| GateCycle::at_rest(open_by_default, now));
            let box_asked = triggered || open_by_default;
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
            let wanted = if cycle.open_for_pathing { admitted } else { 0 };
            if wanted != cycle.stamped {
                self.stamp_portal(tick, ids, gate, wanted, PORTAL_KIND_WALK)?;
                cycle.stamped = wanted;
                diag!(
                    "gate: entity {} passage {} (askers {wanted:#x})",
                    gate.index(),
                    if wanted == 0 { "shut" } else { "open" }
                );
            }
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
        for entity in entities {
            let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
                continue;
            };
            let Some((active, allow_enemies, kind)) =
                wiring.dynamic_portal(tick.state(), entity).map(|module| {
                    (
                        module.is_active(now) || Self::siege_deployed(tick.state(), ids, entity),
                        module.allow_enemies(),
                        dynamic_portal_kind(module),
                    )
                })
            else {
                continue;
            };
            let admitted = if active {
                self.admitted_askers(owner, false, allow_enemies, false)
            } else {
                0
            };
            let stamped = tick
                .state()
                .get(ids.dynamic_portal_stamp, entity)
                .map(|s| s.stamped);
            if stamped != Some(admitted) {
                self.stamp_portal(tick, ids, entity, admitted, kind)?;
                tick.state_mut().insert(
                    ids.dynamic_portal_stamp,
                    entity,
                    DynamicPortalStamp { stamped: admitted },
                )?;
                diag!(
                    "portal: entity {} passage {} (askers {admitted:#x})",
                    entity.index(),
                    if admitted == 0 { "shut" } else { "open" }
                );
            }
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

    fn movers_for_gate_triggers(&self, tick: &Tick<'_>, ids: Ids) -> Vec<(PlayerId, Vec2)> {
        let Some(store) = tick.state().store(ids.body) else {
            return Vec::new();
        };
        let entities: Vec<Entity> = store.entities().to_vec();
        entities
            .into_iter()
            .filter_map(|entity| {
                let body = tick.state().get(ids.body, entity)?.0;
                let owner = tick.state().get(ids.owner, entity)?.0;
                let kind = tick.state().get(ids.kind, entity).copied()?;
                let entry = self.kinds.get(kind)?;
                (!entry.record.has_kind("STRUCTURE")).then_some((owner, body.position))
            })
            .collect()
    }

    fn admitted_askers(
        &self,
        owner: PlayerId,
        neutral: bool,
        allow_enemies: bool,
        allow_non_skirmish_ai: bool,
    ) -> u32 {
        let total = usize::from(self.players).saturating_add(self.map_players.len());
        let total = u8::try_from(total.min(32)).unwrap_or(32);
        let mut admitted = 0;
        for slot in 0..total {
            let player = PlayerId::new(slot);
            let stance = self.alliances.stance(owner, player);
            let passes = match stance {
                crate::alliance::Stance::Allied => true,
                crate::alliance::Stance::Neutral => neutral,
                crate::alliance::Stance::Hostile => false,
            };
            let non_skirmish_ai = self.ai_seeds.iter().all(|seed| seed.player != player);
            if passes || allow_enemies || (allow_non_skirmish_ai && non_skirmish_ai) {
                admitted |= asker_bits(player);
            }
        }
        admitted
    }

    fn stamp_portal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        gate: Entity,
        admitted: u32,
        kind: u8,
    ) -> Result<()> {
        let Some(clock_entity) = tick
            .state()
            .store(ids.portals)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut layer) = tick.state().get(ids.portals, clock_entity).cloned() else {
            return Ok(());
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        if admitted == 0 {
            layer.unstamp(geometry, extent, gate);
        } else {
            let Some(body) = tick.state().get(ids.body, gate).map(|body| body.0) else {
                return Ok(());
            };
            let records = tick
                .state()
                .get(ids.kind, gate)
                .copied()
                .and_then(|kind| self.kinds.get(kind))
                .map(|entry| crate::construction::footprint_of(&entry.record))
                .unwrap_or_default();
            if records.is_empty() {
                return Ok(());
            }
            layer.stamp(
                geometry,
                extent,
                crate::portals::PortalStamp {
                    entity: gate,
                    centre: body.position,
                    angle: body.facing.radians(),
                    records,
                    admitted,
                    kind,
                },
            );
        }
        tick.state_mut().insert(ids.portals, clock_entity, layer)?;
        Ok(())
    }
}

fn asker_bits(player: PlayerId) -> u32 {
    mint_sage_passability::asker_bit(mint_movement::Asker::new(player.get()))
}

const PORTAL_KIND_WALK: u8 = 2;

const PORTAL_KIND_CLIMB: u8 = 3;

fn dynamic_portal_kind(module: &mint_sage_modules::DynamicPortalBehaviour) -> u8 {
    let is_climb = module
        .waypoints()
        .iter()
        .any(|point| point.kind == mint_sage_modules::WayPointType::Climb);
    if is_climb {
        PORTAL_KIND_CLIMB
    } else {
        PORTAL_KIND_WALK
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

    #[test]
    fn an_open_portals_cells_pass_an_admitted_asker_and_stop_everyone_else() {
        use mint_movement::{Cell, CostGrid, GridExtent, MoverClass, Passability as _};
        use mint_sage_passability::{DynamicView, GatedView, GROUND, OBSTACLE, PORTAL};

        let extent = GridExtent::new(4, 4).unwrap();
        let mut grid = CostGrid::new(extent);
        let cell = Cell::new(2, 2);
        grid.add_flags(cell, OBSTACLE).unwrap();
        let mut masks = vec![0u32; 16];
        let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
        masks[index] = asker_bits(PlayerId::new(1));
        let view = GatedView {
            ground: DynamicView {
                grid: &grid,
                counts: &[],
                rubble: &[],
                wall_rim: &[],
                tags: &[],
                connect: &[],
                released: &[],
                closed: &[],
                obstacle_pinched: &[],
                ramp_side: &[],
                scaleable: &[],
                barred: &[],
            },
            portals: &masks,
            tags: &[],
        };
        let mover =
            |player: u8| MoverClass::asking(GROUND, PORTAL, mint_movement::Asker::new(player));
        assert!(view.is_passable(cell, mover(1)), "the owner passes");
        assert!(!view.is_passable(cell, mover(2)), "an enemy does not");
        assert!(
            !view.is_passable(cell, GROUND),
            "a query with no asker is stopped by the gate's own footprint"
        );
    }
}
