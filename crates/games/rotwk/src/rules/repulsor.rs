use mint_movement::Vec2;
use mint_sage_modules::effects::{names, Effect};
use mint_sim::{Component, Entity, Frame, Result, StateHasher, Tick, World};

use super::emotion_states::walk;
use super::emotions::TICKS_PER_FRAME;
use super::safe_path::{SafeRequest, ABSENT_REPULSOR};
use super::{Ids, RotwkRules};
use crate::unit::PathKind;

const REPULSOR: &str = "REPULSOR";
const CAN_BE_REPULSED: &str = "CAN_BE_REPULSED";

const REPULSOR_DURATION_MS: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RepulsorClear(pub(crate) Frame);

impl Component for RepulsorClear {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Flee,
    Wander,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Repulsed {
    phase: Phase,
    goal: Vec2,
    origin: Vec2,
    timer: i32,
}

impl Component for Repulsed {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(match self.phase {
            Phase::Flee => 0,
            Phase::Wander => 1,
        });
        for value in [self.goal.x, self.goal.y, self.origin.x, self.origin.y] {
            hasher.write_f32(value);
        }
        hasher.write_u32(u32::from_ne_bytes(self.timer.to_ne_bytes()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct RepulsorPair {
    current: Option<Entity>,
    prior: Option<Entity>,
}

impl RepulsorPair {
    fn request(&mut self, repulsor: Entity) {
        if self.current != Some(repulsor) {
            self.prior = self.current;
        }
        self.current = Some(repulsor);
    }

    #[cfg(test)]
    pub(crate) fn prior(&self) -> Option<Entity> {
        self.prior
    }
}

impl Component for RepulsorPair {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for entity in [self.current, self.prior] {
            hasher.write_u32(entity.map_or(u32::MAX, Entity::index));
        }
    }
}

#[cfg(test)]
impl Repulsed {
    pub(crate) fn is_fleeing(&self) -> bool {
        self.phase == Phase::Flee
    }

    pub(crate) fn goal(&self) -> Vec2 {
        self.goal
    }
}

impl RotwkRules {
    pub(super) fn can_be_repulsed(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.has_kind_named(world, ids, entity, CAN_BE_REPULSED)
    }

    fn has_kind_named(&self, world: &World, ids: Ids, entity: Entity, name: &str) -> bool {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry
                    .record
                    .kind_of
                    .iter()
                    .any(|k| k.eq_ignore_ascii_case(name))
            })
    }

    pub(super) fn set_repulsor(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        on: bool,
    ) -> Result<()> {
        let held = self.has_status_bit(tick.state(), entity, REPULSOR);
        if held == on {
            return Ok(());
        }
        let bit = names(vec![REPULSOR.to_owned()]);
        let none = names(Vec::new());
        let effect = if on {
            Effect::StatusBits {
                set: bit,
                clear: none,
            }
        } else {
            Effect::StatusBits {
                set: none,
                clear: bit,
            }
        };
        self.apply_effects(tick, ids, vec![(entity, effect)])?;
        if !on {
            tick.state_mut().remove(ids.repulsor_clear, entity);
        } else if self.can_be_repulsed(tick.state(), ids, entity) {
            let now = tick.frame();
            let due = now
                .checked_add(mint_sage_modules::retail_duration_ticks(
                    REPULSOR_DURATION_MS,
                ))
                .unwrap_or(now);
            tick.state_mut()
                .insert(ids.repulsor_clear, entity, RepulsorClear(due))?;
        }
        Ok(())
    }

    pub(super) fn note_repulsor_damage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
    ) -> Result<()> {
        if self.guard_ranges.enable_repulsors && self.can_be_repulsed(tick.state(), ids, victim) {
            self.set_repulsor(tick, ids, victim, true)?;
        }
        Ok(())
    }

    pub(super) fn step_repulsor_helpers(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.repulsor_clear)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, clear)| clear.0 <= now)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in due {
            if tick.state().is_live(entity) {
                self.set_repulsor(tick, ids, entity, false)?;
            } else {
                tick.state_mut().remove(ids.repulsor_clear, entity);
            }
        }
        Ok(())
    }

    pub(super) fn live_vision(&self, world: &World, ids: Ids, entity: Entity) -> f32 {
        let base = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .filter(|vision| vision.is_finite() && *vision > 0.0)
            .unwrap_or(0.0);
        let bonus = world
            .get(ids.bonuses, entity)
            .map_or(0.0, |bonuses| bonuses.0.vision);
        base * (1.0 + bonus).max(0.0)
    }

    pub(super) fn find_closest_repulsor(
        &self,
        world: &World,
        ids: Ids,
        me: Entity,
        range: f32,
    ) -> Option<Entity> {
        if !self.guard_ranges.enable_repulsors {
            return None;
        }
        let at = world.get(ids.body, me)?.0.position;
        let mut best: Option<(f32, Entity)> = None;
        for (other, body) in world.store(ids.body)?.iter() {
            if other == me {
                continue;
            }
            let (dx, dy) = (body.0.position.x - at.x, body.0.position.y - at.y);
            let reach = (dx * dx + dy * dy).sqrt();
            if reach > range || best.is_some_and(|(near, _)| reach >= near) {
                continue;
            }
            if self.repulses(world, ids, me, other) {
                best = Some((reach, other));
            }
        }
        best.map(|(_, entity)| entity)
    }

    fn repulses(&self, world: &World, ids: Ids, me: Entity, other: Entity) -> bool {
        if self.has_status_bit(world, other, REPULSOR) {
            return true;
        }
        let dead = world.get(ids.health, other).is_some_and(|h| h.0.is_dead())
            || world.contains(ids.doomed, other);
        if dead || self.object_relation(world, me, other) != super::map_scripts::Relation::Enemies {
            return false;
        }
        if self.has_kind_named(world, ids, other, "INERT")
            && !self.has_kind_named(world, ids, other, "STRUCTURE")
        {
            return false;
        }
        world.contains(ids.armed, other)
            || world.get(ids.horde, other).is_some_and(|horde| {
                horde
                    .members
                    .iter()
                    .any(|member| world.contains(ids.armed, *member))
            })
    }

    pub(super) fn step_repulsed(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let frame = tick.frame().get();
        let stride = u64::from(TICKS_PER_FRAME.max(1));
        let due = |entity: Entity| frame.wrapping_add(u64::from(entity.index())) % stride == 0;
        let running: Vec<Entity> = tick
            .state()
            .store(ids.repulsed)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in running {
            if due(entity) {
                self.advance_repulsed(tick, ids, entity)?;
            }
        }
        if frame % stride != 0 || !self.guard_ranges.enable_repulsors {
            return Ok(());
        }
        let idle: Vec<Entity> = tick
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, _)| entity)
                    .filter(|entity| self.idle_repulsable(tick.state(), ids, *entity))
                    .collect()
            })
            .unwrap_or_default();
        for entity in idle {
            let vision = self.live_vision(tick.state(), ids, entity);
            if self
                .find_closest_repulsor(tick.state(), ids, entity, vision)
                .is_some()
            {
                self.begin_flee(tick, ids, entity)?;
            }
        }
        Ok(())
    }

    pub(super) fn repulsor_check(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        timer: &mut Option<i32>,
    ) -> bool {
        if !self.can_be_repulsed(world, ids, entity) {
            return false;
        }
        let left = timer.unwrap_or(0).saturating_sub(1);
        if left >= 0 {
            *timer = Some(left);
            return false;
        }
        *timer = Some(wander_interval(entity));
        let vision = self.live_vision(world, ids, entity);
        self.find_closest_repulsor(world, ids, entity, vision)
            .is_some()
    }

    fn idle_gates_pass(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        if self
            .script_state(world, ids)
            .is_some_and(|state| state.rt.hunters.contains(entity))
        {
            return false;
        }
        let Some(member) = world.get(ids.member, entity) else {
            return true;
        };
        if self
            .script_state(world, ids)
            .is_some_and(|state| state.rt.hunters.contains(member.horde))
        {
            return false;
        }
        let walking = !world.contains(ids.idle_ask, entity);
        let melee = world.get(ids.armed, entity).is_some_and(|armed| {
            self.combat
                .weapons
                .get(armed.weapon)
                .is_some_and(|weapon| weapon.melee)
        });
        !walking && !melee
    }

    fn idle_repulsable(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.can_be_repulsed(world, ids, entity)
            && self.idle_gates_pass(world, ids, entity)
            && world.contains(ids.body, entity)
            && !world.contains(ids.path, entity)
            && !world.contains(ids.attack_order, entity)
            && !world.contains(ids.emotion, entity)
            && !world.contains(ids.repulsed, entity)
            && !world.contains(ids.doomed, entity)
            && !world.contains(ids.contained, entity)
            && !world.get(ids.health, entity).is_some_and(|h| h.0.is_dead())
            && !self
                .script_state(world, ids)
                .is_some_and(|state| state.rt.wanderers.contains(entity))
    }

    pub(super) fn begin_flee(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let vision = self.live_vision(tick.state(), ids, entity);
        let Some(repulsor) = self.find_closest_repulsor(tick.state(), ids, entity, vision) else {
            return self.begin_wander(tick, ids, entity);
        };
        let Some(at) = tick.state().get(ids.body, entity).map(|b| b.0.position) else {
            return Ok(());
        };
        drop_orders(tick, ids, entity);
        let mut state = Repulsed {
            phase: Phase::Flee,
            goal: at,
            origin: at,
            timer: 0,
        };
        self.apply_locomotor_override(tick, ids, entity, Some("PANIC"))?;
        self.set_model_condition(tick, ids, entity, "PANICKING", true)?;
        if let Some(goal) = self.request_safe_goal(tick, ids, entity, repulsor, 0.0)? {
            state.goal = goal;
            let goal = self.adjust_to_possible_destination(tick, ids, entity, goal);
            walk(tick, ids, entity, goal)?;
            tick.state_mut().insert(ids.repulsed, entity, state)?;
            return Ok(());
        }
        self.set_model_condition(tick, ids, entity, "PANICKING", false)?;
        self.begin_wander(tick, ids, entity)
    }

    pub(super) fn request_safe_goal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        repulsor: Entity,
        flee_bonus: f32,
    ) -> Result<Option<Vec2>> {
        let mut pair = tick
            .state()
            .get(ids.repulsor_pair, entity)
            .copied()
            .unwrap_or_default();
        pair.request(repulsor);
        tick.state_mut().insert(ids.repulsor_pair, entity, pair)?;
        let world = tick.state();
        let position = |held: Option<Entity>| {
            held.and_then(|e| world.get(ids.body, e))
                .map(|body| body.0.position)
        };
        let current = position(pair.current).unwrap_or(ABSENT_REPULSOR);
        let prior = position(pair.prior).unwrap_or(current);
        let radius = self.safe_radius(world, ids, entity, flee_bonus);
        Ok(self.find_safe_goal(
            tick,
            ids,
            entity,
            SafeRequest {
                repulsor: current,
                prior,
                radius,
            },
        ))
    }

    pub(super) fn safe_radius(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        flee_bonus: f32,
    ) -> f32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let bonus = (flee_bonus as i32) as f32;
        self.live_vision(world, ids, entity) + self.guard_ranges.repulsed_distance + bonus
    }

    fn begin_wander(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(at) = tick.state().get(ids.body, entity).map(|b| b.0.position) else {
            return Ok(());
        };
        drop_orders(tick, ids, entity);
        self.apply_locomotor_override(tick, ids, entity, Some("WANDER"))?;
        let state = Repulsed {
            phase: Phase::Wander,
            goal: at,
            origin: at,
            timer: 0,
        };
        tick.state_mut().insert(ids.repulsed, entity, state)?;
        self.wander_goal(tick, ids, entity, at)
    }

    fn wander_goal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        origin: Vec2,
    ) -> Result<()> {
        let cells = self.wander_cells(tick, ids, entity);
        let span = u32::try_from(cells.saturating_mul(2).saturating_add(1)).unwrap_or(1);
        let mut offset = [0.0_f32; 2];
        for axis in &mut offset {
            let drawn = i32::try_from(tick.rng().below(span.max(1))).unwrap_or(0);
            let steps = i16::try_from(drawn.saturating_sub(cells)).unwrap_or(0);
            *axis = f32::from(steps) * WANDER_CELL;
        }
        let goal = Vec2::new(origin.x + offset[0], origin.y + offset[1]);
        let goal = self.adjust_to_possible_destination(tick, ids, entity, goal);
        if let Some(state) = tick.state_mut().get_mut(ids.repulsed, entity) {
            state.goal = goal;
        }
        walk(tick, ids, entity, goal)
    }

    fn advance_repulsed(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(mut state) = tick.state().get(ids.repulsed, entity).copied() else {
            return Ok(());
        };
        let world = tick.state();
        if !world.is_live(entity) || world.contains(ids.doomed, entity) {
            tick.state_mut().remove(ids.repulsed, entity);
            return Ok(());
        }
        if !world.contains(ids.body, entity) {
            return Ok(());
        }
        let walking = world
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Emotion);
        match state.phase {
            Phase::Flee => {
                if walking {
                    return Ok(());
                }
                self.set_model_condition(tick, ids, entity, "PANICKING", false)?;
                self.begin_wander(tick, ids, entity)
            }
            Phase::Wander => {
                if self.can_be_repulsed(world, ids, entity) {
                    state.timer = state.timer.saturating_sub(1);
                    if state.timer < 0 {
                        state.timer = wander_interval(entity);
                        tick.state_mut().insert(ids.repulsed, entity, state)?;
                        let vision = self.live_vision(tick.state(), ids, entity);
                        if self
                            .find_closest_repulsor(tick.state(), ids, entity, vision)
                            .is_some()
                        {
                            return self.begin_flee(tick, ids, entity);
                        }
                    }
                }
                tick.state_mut().insert(ids.repulsed, entity, state)?;
                if !walking {
                    self.wander_goal(tick, ids, entity, state.origin)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn end_repulsed(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let Some(state) = tick.state_mut().remove(ids.repulsed, entity) else {
            return Ok(());
        };
        if state.phase == Phase::Flee {
            self.set_model_condition(tick, ids, entity, "PANICKING", false)?;
        }
        self.apply_locomotor_override(tick, ids, entity, None)
    }
}

pub(super) fn wander_interval(entity: Entity) -> i32 {
    10 + i32::try_from(entity.index() & 7).unwrap_or(0)
}

const WANDER_CELL: f32 = 10.0;

fn drop_orders(tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
    tick.state_mut().remove(ids.path, entity);
    tick.state_mut().remove(ids.attack_order, entity);
}
