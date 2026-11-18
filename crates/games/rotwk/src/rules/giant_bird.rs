use mint_movement::{Facing, FlightPath, MotionLimits, Vec2, Vec3};
use mint_sage_combat::WeaponStatus;
use mint_sage_modules::FollowThrough;
use mint_sim::{Component, Entity, Result, StateHasher, Tick, World};

use super::contact::nearest_labelled;
use super::{Ids, RotwkRules};
use crate::diag::diag;
use crate::unit::PathKind;

const CLIMBING: &str = "CLIMBING";
const FREEFALL: &str = "FREEFALL";
const ABOUT_TO_HIT: &str = "ABOUT_TO_HIT";
const SWOOP_SET: &str = "SUPERSONIC";
const DUEL_SET: &str = "WANDER";
const POWER_SET: &str = "PANIC";
const SWOOP_POWER: &str = "FellBeastSwoopPower";
const DUEL_ROLLS: u32 = 13;
const DUEL_WANDER_ROLLS: u32 = 7;
const POWER_DIVE_HEIGHT: f32 = 1000.0;

const LEAD_IN: f32 = 3.0;
const MAX_LEAD_IN: f32 = 1500.0;
const LEAD_OUT: f32 = 0.25;
const MIN_LEAD_OUT: f32 = 50.0;
const MAX_LEAD_OUT: f32 = 150.0;

const SLOPE_DRAG: f32 = 0.4;
const HARD_TURN_SLOPE: f32 = 1.5;
const HARD_TURN_FRAMES: f32 = 3.0;
const LEVEL_SHARE: f32 = 0.25;
const EASE_IN_STEPS: f32 = 4.0;
const CLIMBING_SLOPE: f32 = 0.3;
const FREEFALL_SLOPE: f32 = -0.6;
const TURN_SPEED: f32 = 0.4;
const GROUND_CLEARANCE: f32 = 0.25;

const SWOOP_BRAKE: f32 = 0.4;
const SWOOP_BRAKE_DECAY: f32 = 0.8;
const REAIM_DISTANCE: f32 = 10.0;
const REAIM_STEPS: f32 = 8.0;
const REAIM_NEAR: f32 = 1.0;
const CLAWING_STEPS: f32 = 4.0;
const STRIKE_STEPS: f32 = 3.0;
const MIN_STRIKE: f32 = 8.5;
const STRIKE_FRAMES: u32 = 5;
const CLIP_FRAMES: u32 = 10;
const FOLLOW_THROUGH_FRAMES: u32 = 40;
const FOLLOW_THROUGH_CAP: f32 = 1.25;
const FOLLOW_THROUGH_LOW: f32 = 0.25;
const MAX_FOLLOW_THROUGH_CHECKS: u32 = 256;

const PROBE_CLEARANCE: f32 = 2.5;
const NUDGE_UP: f32 = 5.0;
const NUDGE_DOWN: f32 = 1.0;
const TOO_HIGH: f32 = 2.0;

const CENTRE_CLEARANCE: f32 = 0.5;
const EDGE_CLEARANCE: f32 = 0.3885;
const SEPARATION_NEAR: f32 = 1.2;
const FAN_STEP: f32 = 0.2;
const FAN_RINGS: [f32; 3] = [1.0, 2.0, 4.0];
const ESCAPE_MARGIN: f32 = 1.05;
const MAX_LIFTS: u32 = 5;
const MAX_CLEARANCE_SAMPLES: u32 = 200;

const BOB_RATE: f32 = 0.1;
const BOB: f32 = 0.2;
const CLIMB_GAIN: f32 = 0.25;
const CLIMB_LIMIT: f32 = 0.04;
const CLIMBING_CLIMB: f32 = 0.3;
const DRIFT: f32 = 0.0628;
const PUSH_REACH: f32 = 2.0;
const PUSH: f32 = 2.0;
const PUSH_JITTER: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    start: Start,
    descend: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    Along,
    Climb,
    Dive,
}

impl Shape {
    const PLAIN: Self = Self {
        start: Start::Along,
        descend: false,
    };
    const CLIMB_START: Self = Self {
        start: Start::Climb,
        descend: false,
    };

    fn ending(descend: bool) -> Self {
        Self {
            start: Start::Along,
            descend,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Strike,
    Duel,
    Power,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwoopSet {
    Supersonic,
    Wander,
    Panic,
}

impl SwoopSet {
    fn name(self) -> &'static str {
        match self {
            Self::Supersonic => SWOOP_SET,
            Self::Wander => DUEL_SET,
            Self::Panic => POWER_SET,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct BirdKind {
    obstacle: bool,
    pushes: bool,
    immobile: bool,
    salt_lick: bool,
}

impl BirdKind {
    pub(super) fn from_kind_of(kind_of: &[String]) -> Self {
        let has = |name: &str| kind_of.iter().any(|flag| flag.eq_ignore_ascii_case(name));
        Self {
            obstacle: has("STRUCTURE") || has("MONSTER") || has("MACHINE"),
            pushes: !(has("INFANTRY") || has("CAVALRY") || has("HORDE")),
            immobile: has("IMMOBILE"),
            salt_lick: has("SALT_LICK"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Pusher {
    at: Vec3,
    radius: f32,
}

#[derive(Clone, Copy)]
struct Scene<'a> {
    world: &'a World,
    ids: Ids,
    entity: Entity,
    victim: Option<Entity>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Hover,
    Cruise {
        goal: Vec2,
    },
    Swoop {
        target: Entity,
        goal: Vec3,
        clip: bool,
    },
    Attack {
        target: Entity,
        clip: bool,
        fired: bool,
    },
    FollowThrough {
        target: Entity,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GiantBird {
    phase: Phase,
    path: Option<FlightPath>,
    travelled: f32,
    step: f32,
    slope: f32,
    hard_turn: bool,
    brake: f32,
    climb: f32,
    drift: f32,
    push: Vec2,
    frames: u32,
    lifted: f32,
    mode: Mode,
    set: SwoopSet,
    horde: Option<Entity>,
}

impl GiantBird {
    fn new() -> Self {
        Self {
            phase: Phase::Hover,
            path: None,
            travelled: 0.0,
            step: 0.0,
            slope: 0.0,
            hard_turn: false,
            brake: 0.0,
            climb: 0.0,
            drift: 0.0,
            push: Vec2::ZERO,
            frames: 0,
            lifted: 0.0,
            mode: Mode::Strike,
            set: SwoopSet::Supersonic,
            horde: None,
        }
    }

    #[cfg(test)]
    pub(super) fn phase_name(&self) -> &'static str {
        match self.phase {
            Phase::Hover => "hover",
            Phase::Cruise { .. } => "cruise",
            Phase::Swoop { .. } => "swoop",
            Phase::Attack { .. } => "attack",
            Phase::FollowThrough { .. } => "follow-through",
        }
    }

    #[cfg(test)]
    pub(super) fn swoop_name(&self) -> (&'static str, &'static str) {
        let mode = match self.mode {
            Mode::Strike => "strike",
            Mode::Duel => "duel",
            Mode::Power => "power",
        };
        (mode, self.set.name())
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.frames = 0;
        if matches!(phase, Phase::Hover | Phase::Cruise { .. }) {
            self.mode = Mode::Strike;
            self.set = SwoopSet::Supersonic;
        }
    }

    fn victim(&self) -> Option<Entity> {
        match self.phase {
            Phase::Swoop { target, .. }
            | Phase::Attack { target, .. }
            | Phase::FollowThrough { target } => Some(target),
            Phase::Hover | Phase::Cruise { .. } => None,
        }
    }

    fn finished(&self) -> bool {
        self.path
            .as_ref()
            .is_none_or(|path| self.travelled >= path.length())
    }

    fn remaining(&self) -> f32 {
        self.path
            .as_ref()
            .map_or(0.0, |path| (path.length() - self.travelled).max(0.0))
    }
}

impl Component for GiantBird {
    fn hash_into(&self, hasher: &mut StateHasher) {
        let vec3 = |hasher: &mut StateHasher, v: Vec3| {
            hasher.write_f32(v.x);
            hasher.write_f32(v.y);
            hasher.write_f32(v.z);
        };
        match self.phase {
            Phase::Hover => hasher.write_u8(0),
            Phase::Cruise { goal } => {
                hasher.write_u8(1);
                hasher.write_f32(goal.x);
                hasher.write_f32(goal.y);
            }
            Phase::Swoop { target, goal, clip } => {
                hasher.write_u8(2);
                target.hash_into(hasher);
                vec3(hasher, goal);
                hasher.write_bool(clip);
            }
            Phase::Attack {
                target,
                clip,
                fired,
            } => {
                hasher.write_u8(3);
                target.hash_into(hasher);
                hasher.write_bool(clip);
                hasher.write_bool(fired);
            }
            Phase::FollowThrough { target } => {
                hasher.write_u8(4);
                target.hash_into(hasher);
            }
        }
        match &self.path {
            None => hasher.write_usize(0),
            Some(path) => {
                hasher.write_usize(path.points().len());
                for point in path.points() {
                    vec3(hasher, *point);
                }
            }
        }
        for value in [
            self.travelled,
            self.step,
            self.slope,
            self.brake,
            self.climb,
            self.drift,
            self.push.x,
            self.push.y,
            self.lifted,
        ] {
            hasher.write_f32(value);
        }
        hasher.write_bool(self.hard_turn);
        hasher.write_u32(self.frames);
        hasher.write_u8(self.mode as u8);
        hasher.write_u8(self.set as u8);
        hasher.write_u32(self.horde.map_or(u32::MAX, Entity::index));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Engagement {
    Free,
    Busy { target: Option<Entity> },
    Strike(Entity),
}

#[derive(Debug, Clone, Copy)]
struct Flight {
    position: Vec3,
    facing: Facing,
    radius: f32,
    preferred: f32,
    look: f32,
    look_mult: f32,
    lift: f32,
    limits: MotionLimits,
    hz: f32,
    ticks: f32,
}

impl Flight {
    fn max_step(&self) -> f32 {
        (self.limits.max_speed / self.hz).max(0.0)
    }

    fn accel_step(&self) -> f32 {
        (self.limits.acceleration / (self.hz * self.hz)).max(0.0)
    }

    fn brake_step(&self) -> f32 {
        (self.limits.braking / (self.hz * self.hz)).max(0.0)
    }

    fn min_step(&self) -> f32 {
        (self.limits.min_speed / self.hz)
            .max(0.0)
            .min(self.max_step())
    }

    fn turn(&self) -> f32 {
        (self.limits.turn_rate / self.hz).max(0.0)
    }
}

impl RotwkRules {
    pub(super) fn is_giant_bird(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.follow_through_of(world, ids, entity).is_some()
    }

    fn follow_through_of(&self, world: &World, ids: Ids, entity: Entity) -> Option<FollowThrough> {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.giant_birds.get(usize::from(kind.0)))
            .copied()
            .flatten()
    }

    pub(super) fn flies_itself(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.giant_bird, entity)
    }

    pub(super) fn fly_giant_birds(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        if self.giant_birds.iter().all(Option::is_none) {
            return Ok(());
        }
        let birds: Vec<Entity> = tick
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, kind)| {
                        self.giant_birds
                            .get(usize::from(kind.0))
                            .is_some_and(Option::is_some)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let hz = self.logic_hz().clamp(1.0, Self::ticks_per_second());
        let think = self.logic_frame_starts(tick.frame().get(), hz);
        for entity in birds {
            self.fly_giant_bird(tick, ids, entity, hz, think)?;
        }
        Ok(())
    }

    fn logic_frame_starts(&self, frame: u64, hz: f32) -> bool {
        Self::logic_frame_index(frame, hz) != Self::logic_frame_index(frame.saturating_sub(1), hz)
            || frame == 0
    }

    fn logic_frame_index(frame: u64, hz: f32) -> u64 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hz = hz.round().max(1.0) as u64;
        let per_second = u64::from(super::SIMULATION_HZ);
        frame
            .saturating_mul(hz)
            .checked_div(per_second)
            .unwrap_or(0)
    }

    fn fly_giant_bird(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        hz: f32,
        think: bool,
    ) -> Result<()> {
        let dead = tick.state().contains(ids.doomed, entity)
            || tick
                .state()
                .get(ids.health, entity)
                .is_none_or(|health| health.0.is_dead());
        if dead || tick.state().contains(ids.contained, entity) {
            tick.state_mut().remove(ids.giant_bird, entity);
            return Ok(());
        }
        let Some(spec) = self.follow_through_of(tick.state(), ids, entity) else {
            return Ok(());
        };
        if !tick.state().contains(ids.body, entity) {
            return Ok(());
        }
        let mut bird = tick
            .state()
            .get(ids.giant_bird, entity)
            .cloned()
            .unwrap_or_else(GiantBird::new);
        let Some(mut flight) = self.flight(tick, ids, entity, &bird, hz) else {
            return Ok(());
        };
        if think {
            bird.frames = bird.frames.saturating_add(1);
            self.decide(tick, ids, entity, &mut bird, &mut flight, spec)?;
        }
        self.move_bird(tick, ids, entity, &mut bird, &flight)?;
        tick.state_mut().insert(ids.giant_bird, entity, bird)?;
        Ok(())
    }

    fn flight(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &GiantBird,
        hz: f32,
    ) -> Option<Flight> {
        let body = tick.state().get(ids.body, entity)?.0;
        let ground = self.ground_at(body.position)?;
        let profile = self.flight_profile(tick, ids, entity);
        let preferred = profile
            .and_then(|profile| profile.preferred_height)
            .filter(|height| height.is_finite())
            .unwrap_or(0.0)
            .max(0.0);
        let radius = tick
            .state()
            .get(ids.shape, entity)
            .map_or(1.0, |shape| shape.radius)
            .max(1.0);
        let z = tick
            .state()
            .get(ids.elevation, entity)
            .map_or(ground + preferred, |elevation| elevation.0);
        let look_mult = profile.map_or(1.0, |profile| profile.look_ahead_mult);
        let (limits, set_look) = self.phase_limits(tick, ids, entity, bird);
        Some(Flight {
            position: Vec3::new(body.position.x, body.position.y, z),
            facing: body.facing,
            radius,
            preferred,
            look: 0.5 * radius * set_look.unwrap_or(look_mult),
            look_mult,
            lift: profile.map_or(1.0, |profile| profile.lift),
            limits,
            hz,
            ticks: (Self::ticks_per_second() / hz).max(1.0),
        })
    }

    fn phase_limits(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &GiantBird,
    ) -> (MotionLimits, Option<f32>) {
        if matches!(bird.phase, Phase::Swoop { .. }) {
            if let Some((limits, look)) = self.set_limits(tick, ids, entity, bird.set.name()) {
                return (limits, Some(look));
            }
        }
        (self.mover_limits(tick, ids, entity), None)
    }

    fn set_limits(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        set: &str,
    ) -> Option<(MotionLimits, f32)> {
        let entry = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))?;
        let kinematics = entry
            .locomotor_sets
            .iter()
            .find(|(condition, _)| condition.eq_ignore_ascii_case(set))
            .map(|(_, kinematics)| *kinematics)?;
        let health = tick
            .state()
            .get(ids.health, entity)
            .map(|health| health.0.fraction());
        let limits = crate::experience::with_speed(
            kinematics.at_health(health, entry.movement_penalty),
            tick.state().get(ids.bonuses, entity),
        );
        Some((limits, kinematics.look_ahead_mult))
    }

    fn bird_kind(&self, world: &World, ids: Ids, entity: Entity) -> BirdKind {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.bird_kinds.get(usize::from(kind.0)))
            .copied()
            .unwrap_or_default()
    }

    fn ground_at(&self, at: Vec2) -> Option<f32> {
        let heights = &self.passability.heights;
        let origin = heights.geometry().origin();
        let size = heights.geometry().cell_size();
        let extent = heights.extent();
        let far = |cells: u16| f32::from(cells.saturating_sub(1)) * size;
        let x = at.x.min(origin.x + far(extent.width)).max(origin.x);
        let y = at.y.min(origin.y + far(extent.height)).max(origin.y);
        heights.height_at(x, y)
    }

    fn decide(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &mut Flight,
        spec: FollowThrough,
    ) -> Result<()> {
        match bird.phase {
            Phase::Hover | Phase::Cruise { .. } => {
                self.decide_idle(tick, ids, entity, bird, flight)
            }
            Phase::Swoop { .. } => self.decide_swoop(tick, ids, entity, bird, flight, spec)?,
            Phase::Attack { .. } => self.decide_attack(tick, ids, entity, bird, flight, spec)?,
            Phase::FollowThrough { .. } => {
                if bird.finished() || bird.frames > FOLLOW_THROUGH_FRAMES {
                    diag!(
                        "bird: frame {} entity {} ends its follow-through ({})",
                        tick.frame().get(),
                        entity.index(),
                        if bird.finished() {
                            "arrived"
                        } else {
                            "out of time"
                        }
                    );
                    self.end_follow_through(tick, ids, entity, bird, flight)?;
                }
            }
        }
        let (limits, set_look) = self.phase_limits(tick, ids, entity, bird);
        flight.limits = limits;
        flight.look = 0.5 * flight.radius * set_look.unwrap_or(flight.look_mult);
        match bird.phase {
            Phase::Hover => self.hover(tick, ids, entity, bird, flight)?,
            Phase::Attack { .. } => self.pose(tick, ids, entity, false, false)?,
            Phase::Cruise { .. } | Phase::Swoop { .. } | Phase::FollowThrough { .. } => {
                let probe = !self.licks_still_victim(tick, ids, entity, bird);
                let scene = Scene {
                    world: tick.state(),
                    ids,
                    entity,
                    victim: bird.victim(),
                };
                self.nudge(scene, bird, flight, probe);
                self.speed(bird, flight);
                self.pose(
                    tick,
                    ids,
                    entity,
                    bird.slope > CLIMBING_SLOPE,
                    bird.slope < FREEFALL_SLOPE,
                )?;
            }
        }
        Ok(())
    }

    fn end_follow_through(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &mut Flight,
    ) -> Result<()> {
        let Phase::FollowThrough { target } = bird.phase else {
            return Ok(());
        };
        let ordered_move = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Ordered);
        let again = if self.target_is_live(tick, ids, target) {
            Some(target)
        } else {
            bird.horde
                .filter(|horde| self.swoop_victim_live(tick, ids, *horde))
        };
        bird.enter(Phase::Hover);
        let Some(again) = again.filter(|_| !ordered_move) else {
            bird.horde = None;
            self.decide_idle(tick, ids, entity, bird, flight);
            return Ok(());
        };
        tick.state_mut()
            .insert(ids.giant_bird, entity, bird.clone())?;
        self.start_swoop(tick, ids, entity, again, false)?;
        if let Some(swooping) = tick.state().get(ids.giant_bird, entity).cloned() {
            *bird = swooping;
        }
        if let Some(refreshed) = self.flight(tick, ids, entity, bird, flight.hz) {
            *flight = refreshed;
        }
        Ok(())
    }

    fn decide_idle(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
    ) {
        let order = tick.state().get(ids.path, entity).map(|path| path.goal);
        match (order, bird.phase) {
            (Some(goal), Phase::Cruise { goal: flying }) if goal == flying => {
                if bird.finished() {
                    tick.state_mut().remove(ids.path, entity);
                    diag!(
                        "bird: frame {} entity {} arrives at ({:.0}, {:.0})",
                        tick.frame().get(),
                        entity.index(),
                        goal.x,
                        goal.y
                    );
                    bird.enter(Phase::Hover);
                }
            }
            (Some(goal), _) => {
                let height = self
                    .ground_at(goal)
                    .map_or(flight.position.z, |ground| ground + flight.preferred);
                let to = Vec3::new(goal.x, goal.y, height);
                let scene = Scene {
                    world: tick.state(),
                    ids,
                    entity,
                    victim: None,
                };
                self.plan(scene, bird, flight, flight.position, to, Shape::PLAIN, true);
                bird.enter(Phase::Cruise { goal });
            }
            (None, Phase::Cruise { .. }) => bird.enter(Phase::Hover),
            (None, _) => {}
        }
    }

    fn decide_swoop(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
        spec: FollowThrough,
    ) -> Result<()> {
        let Phase::Swoop {
            mut target,
            goal,
            clip,
        } = bird.phase
        else {
            return Ok(());
        };
        let ordered_move = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Ordered);
        if ordered_move {
            self.set_conditions(tick, ids, entity, &[], &[ABOUT_TO_HIT])?;
            bird.enter(Phase::Hover);
            self.decide_idle(tick, ids, entity, bird, flight);
            return Ok(());
        }
        if let Some(ordered) = Self::attack_goal(tick, ids, entity).filter(|ordered| {
            !Self::stands_for(tick, ids, *ordered, target)
                && self.swoop_victim_live(tick, ids, *ordered)
        }) {
            target = self.swoop_victim(tick, ids, entity, ordered, flight.position.horizontal());
            bird.horde = self.horde_of_victim(tick, ids, ordered, target);
            if bird.mode != Mode::Power {
                self.choose_mode(tick, ids, entity, bird, target);
            }
        } else if !self.swoop_victim_live(tick, ids, target) {
            return self.enter_follow_through(tick, ids, entity, bird, flight, spec, target);
        }
        let Some(aim) = self.aim(tick, ids, entity, bird.mode, target, flight) else {
            return self.enter_follow_through(tick, ids, entity, bird, flight, spec, target);
        };
        let distance = aim.minus(flight.position).length();
        if distance <= CLAWING_STEPS * bird.step
            && !self.holds_model_condition(tick, entity, ABOUT_TO_HIT)
        {
            self.set_conditions(tick, ids, entity, &[ABOUT_TO_HIT], &[])?;
        }
        let retargeted = !matches!(bird.phase, Phase::Swoop { target: t, .. } if t == target);
        let moved = aim.minus(goal).length();
        if retargeted
            || moved > REAIM_DISTANCE
            || (distance <= REAIM_STEPS * bird.step && moved > REAIM_NEAR)
        {
            let scene = Scene {
                world: tick.state(),
                ids,
                entity,
                victim: Some(target),
            };
            let shape = Shape::ending(clip);
            self.plan(scene, bird, flight, flight.position, aim, shape, false);
        }
        bird.phase = Phase::Swoop {
            target,
            goal: aim,
            clip,
        };
        if bird.finished() || distance <= bird.step {
            if distance <= (STRIKE_STEPS * bird.step).max(MIN_STRIKE) {
                diag!(
                    "bird: frame {} entity {} strikes at entity {} from {distance:.1}",
                    tick.frame().get(),
                    entity.index(),
                    target.index()
                );
                bird.enter(Phase::Attack {
                    target,
                    clip,
                    fired: false,
                });
            } else {
                diag!(
                    "bird: frame {} entity {} misses entity {} by {distance:.1}",
                    tick.frame().get(),
                    entity.index(),
                    target.index()
                );
                self.enter_follow_through(tick, ids, entity, bird, flight, spec, target)?;
            }
        }
        Ok(())
    }

    fn decide_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
        spec: FollowThrough,
    ) -> Result<()> {
        let Phase::Attack {
            target,
            clip,
            fired,
        } = bird.phase
        else {
            return Ok(());
        };
        let status = tick
            .state()
            .get(ids.armed, entity)
            .copied()
            .map(|mut armed| (armed.state.status(tick.frame()), armed.winding));
        let over = if clip {
            !self.swoop_victim_live(tick, ids, target)
                || bird.frames > CLIP_FRAMES
                || matches!(
                    status,
                    None | Some((WeaponStatus::ReloadingClip | WeaponStatus::OutOfAmmo, _))
                )
        } else {
            let can_strike = matches!(
                status,
                Some((WeaponStatus::ReadyToFire | WeaponStatus::PreAttack, _) | (_, true))
            );
            fired || !can_strike || bird.frames > STRIKE_FRAMES
        };
        if !over {
            return Ok(());
        }
        self.reload_after_strike(tick, ids, entity)?;
        self.enter_follow_through(tick, ids, entity, bird, flight, spec, target)
    }

    fn reload_after_strike(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        let now = tick.frame();
        if let Some(mut armed) = tick.state().get(ids.armed, entity).copied() {
            if let Some(template) = self.combat.weapons.get(armed.weapon) {
                let rate = self.fire_rate(tick.state(), ids, entity, template);
                armed.state.reload_at_rate(template, now, rate, tick.rng());
                tick.state_mut().insert(ids.armed, entity, armed)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_follow_through(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
        spec: FollowThrough,
        target: Entity,
    ) -> Result<()> {
        self.set_conditions(tick, ids, entity, &[], &[ABOUT_TO_HIT])?;
        let heading = flight.facing.to_vector();
        let from = flight.position;
        let distance = spec.distance;
        let step = spec.check_step.max(f32::MIN_POSITIVE);
        let mut clear = from;
        let mut reached = 0.0f32;
        let mut checks = 0u32;
        loop {
            checks = checks.saturating_add(1);
            let along = step * f32::from(u16::try_from(checks).unwrap_or(u16::MAX));
            if along > distance || checks > MAX_FOLLOW_THROUGH_CHECKS {
                break;
            }
            let at = from.horizontal().plus(heading.scale(along));
            if !self.follow_through_clear(tick, ids, entity, at, from.z + along * spec.gradient) {
                break;
            }
            clear = Vec3::new(at.x, at.y, from.z);
            reached = along;
        }
        let blocked = reached < 0.5 * distance;
        let ground = self.ground_at(clear.horizontal()).unwrap_or(from.z);
        let cap = ground + FOLLOW_THROUGH_CAP * flight.preferred;
        let mut height = clear.z + flight.preferred;
        if height > cap {
            height = (clear.z + FOLLOW_THROUGH_LOW * flight.preferred).max(cap);
        }
        let goal = if bird.mode == Mode::Duel {
            let middle = clear.horizontal().plus(from.horizontal()).scale(0.5);
            Vec3::new(middle.x, middle.y, ground + flight.preferred)
        } else {
            Vec3::new(clear.x, clear.y, height)
        };
        let shape = if blocked {
            Shape::CLIMB_START
        } else {
            Shape::PLAIN
        };
        diag!(
            "bird: frame {} entity {} follows through to ({:.0}, {:.0}, {:.0}){}",
            tick.frame().get(),
            entity.index(),
            goal.x,
            goal.y,
            goal.z,
            if blocked { " climbing" } else { "" }
        );
        let scene = Scene {
            world: tick.state(),
            ids,
            entity,
            victim: Some(target),
        };
        self.plan(scene, bird, flight, from, goal, shape, true);
        bird.brake = 0.0;
        bird.enter(Phase::FollowThrough { target });
        Ok(())
    }

    fn follow_through_clear(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        at: Vec2,
        ceiling: f32,
    ) -> bool {
        if self.ground_at(at).is_some_and(|ground| ground >= ceiling) {
            return false;
        }
        let radius = tick
            .state()
            .get(ids.shape, entity)
            .map_or(0.0, |shape| shape.radius);
        self.nearest_obstacle(tick.state(), ids, entity, at, radius)
            .is_none_or(|(_, _, top)| top < ceiling)
    }

    fn nearest_obstacle(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        at: Vec2,
        reach: f32,
    ) -> Option<(Vec2, f32, f32)> {
        let store = world.store(ids.body)?;
        store
            .iter()
            .filter(|(other, _)| *other != entity)
            .filter(|(other, _)| self.bird_kind(world, ids, *other).obstacle)
            .filter_map(|(other, body)| {
                let shape = world.get(ids.shape, other)?;
                let distance = body.0.position.minus(at).length();
                (distance <= reach + shape.radius).then(|| {
                    (
                        distance,
                        body.0.position,
                        shape.radius,
                        Self::feet(world, ids, other) + shape.height,
                    )
                })
            })
            .fold(
                None,
                |best: Option<(f32, Vec2, f32, f32)>, found| match best {
                    Some(best) if best.0 <= found.0 => Some(best),
                    _ => Some(found),
                },
            )
            .map(|(_, position, radius, top)| (position, radius, top))
    }

    fn hover(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
    ) -> Result<()> {
        bird.step = (bird.step - flight.brake_step()).max(0.0);
        let ground = self
            .ground_at(flight.position.horizontal())
            .unwrap_or(flight.position.z);
        let amplitude = flight.look;
        #[allow(clippy::cast_possible_truncation)]
        let phase = (Self::logic_frame_index(tick.frame().get(), flight.hz) as u32)
            .wrapping_add(entity.index());
        #[allow(clippy::cast_precision_loss)]
        let bob = mint_math::f32::sin(phase as f32 * BOB_RATE) * amplitude * BOB;
        let desired = flight.preferred + bob;
        let above = flight.position.z - ground;
        if desired + BOB * amplitude > above || desired * 1.5 < above {
            bird.climb += desired - above;
        }
        let limit = CLIMB_LIMIT * flight.lift * flight.max_step();
        bird.climb = (bird.climb * CLIMB_GAIN).clamp(-limit, limit);
        let turn = (tick.rng().next_f32() * 2.0 - 1.0) * DRIFT;
        bird.drift = (bird.drift + turn).clamp(-DRIFT, DRIFT);
        bird.push = match self.nearest_obstacle(
            tick.state(),
            ids,
            entity,
            flight.position.horizontal(),
            PUSH_REACH * flight.radius,
        ) {
            Some((at, radius, _))
                if at.minus(flight.position.horizontal()).length()
                    <= PUSH_REACH * flight.radius + radius =>
            {
                let away = flight
                    .position
                    .horizontal()
                    .minus(at)
                    .normalised()
                    .unwrap_or_else(|| flight.facing.to_vector());
                let jitter =
                    |tick: &mut Tick<'_>| (tick.rng().next_f32() * 2.0 - 1.0) * PUSH_JITTER;
                let (jx, jy) = (jitter(tick), jitter(tick));
                away.scale(PUSH).plus(Vec2::new(jx, jy))
            }
            _ => Vec2::ZERO,
        };
        self.set_conditions(tick, ids, entity, &[], &[ABOUT_TO_HIT])?;
        self.pose(tick, ids, entity, bird.climb > CLIMBING_CLIMB, false)
    }

    fn nudge(&self, scene: Scene<'_>, bird: &mut GiantBird, flight: &Flight, probe: bool) {
        let Some(path) = bird.path.as_ref() else {
            return;
        };
        let end = path.end();
        let ahead = bird.travelled + 2.0 * bird.step;
        let probes: Vec<Vec3> = [ahead - flight.radius * 0.5, ahead]
            .into_iter()
            .filter(|at| *at > bird.travelled && *at < path.length() - flight.radius)
            .filter_map(|at| path.sample(at))
            .collect();
        let pushers = if probe && !probes.is_empty() {
            self.pushers(
                scene,
                flight,
                flight.position.horizontal(),
                ahead - bird.travelled,
            )
            .into_iter()
            .filter(|pusher| separation(end, flight.radius, &[*pusher]).length_squared() == 0.0)
            .collect()
        } else {
            Vec::new()
        };
        let here = separation(flight.position, flight.radius, &pushers).length();
        let blocked = probe
            && probes.into_iter().any(|point| {
                self.ground_at(point.horizontal())
                    .is_some_and(|ground| ground >= point.z - PROBE_CLEARANCE)
                    || separation(point, flight.radius, &pushers).length() > here
            });
        let descend = matches!(bird.phase, Phase::Swoop { clip: true, .. });
        if blocked {
            let from = flight.position.plus(Vec3::new(0.0, 0.0, NUDGE_UP));
            let shape = Shape {
                start: Start::Climb,
                descend,
            };
            self.plan(scene, bird, flight, from, end, shape, false);
            return;
        }
        let ground = self
            .ground_at(flight.position.horizontal())
            .unwrap_or(flight.position.z);
        let rising = path
            .sample(bird.travelled + 10.0)
            .is_some_and(|point| point.z - 2.0 > flight.position.z);
        if flight.preferred > 0.0
            && flight.position.z - ground > TOO_HIGH * flight.preferred
            && rising
        {
            let from = flight.position.minus(Vec3::new(0.0, 0.0, NUDGE_DOWN));
            self.plan(
                scene,
                bird,
                flight,
                from,
                end,
                Shape::ending(descend),
                false,
            );
        }
    }

    fn licks_still_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &GiantBird,
    ) -> bool {
        let Phase::Swoop { target, .. } = bird.phase else {
            return false;
        };
        self.bird_kind(tick.state(), ids, entity).salt_lick
            && tick
                .state()
                .get(ids.body, target)
                .is_some_and(|body| body.0.speed == 0.0)
    }

    fn speed(&self, bird: &mut GiantBird, flight: &Flight) {
        let max = flight.max_step();
        let min = flight.min_step();
        let mut slope = bird.slope;
        if bird.remaining() <= EASE_IN_STEPS * bird.step && slope < 0.0 {
            slope = -slope;
        } else if bird.hard_turn {
            slope += HARD_TURN_SLOPE;
        }
        bird.slope = slope;
        let target = (max * (1.0 - SLOPE_DRAG * slope)).clamp(min, max);
        let mut step = if bird.step < target {
            (bird.step + flight.accel_step()).min(target)
        } else {
            (bird.step - flight.brake_step()).max(target)
        };
        if matches!(bird.phase, Phase::Swoop { .. }) {
            step *= 1.0 - bird.brake;
            bird.brake *= SWOOP_BRAKE_DECAY;
        }
        bird.step = if step.is_finite() {
            step.clamp(min, max)
        } else {
            min
        };
    }

    fn move_bird(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        flight: &Flight,
    ) -> Result<()> {
        let share = bird.step / flight.ticks;
        let turn = flight.turn();
        let mut facing = flight.facing;
        let mut position = flight.position;
        let flying_path = matches!(
            bird.phase,
            Phase::Cruise { .. } | Phase::Swoop { .. } | Phase::FollowThrough { .. }
        ) && !bird.finished();
        if let (true, Some(path)) = (flying_path, bird.path.as_ref()) {
            bird.travelled = (bird.travelled + share).min(path.length());
            let at = path.sample(bird.travelled).unwrap_or_else(|| path.end());
            let behind = path
                .sample((bird.travelled - flight.look).max(0.0))
                .unwrap_or(at);
            let ahead = path
                .sample((bird.travelled + flight.look).min(path.length()))
                .unwrap_or(at);
            let chord = ahead.minus(behind);
            let length = chord.length();
            let level = chord.horizontal().length() >= LEVEL_SHARE * length;
            match Facing::from_vector(chord.horizontal()).filter(|_| level) {
                Some(desired) => {
                    bird.hard_turn = facing.delta_to(desired).abs() > HARD_TURN_FRAMES * turn;
                    facing = facing.turned_towards(desired, turn / flight.ticks);
                }
                None => bird.hard_turn = false,
            }
            bird.slope = if length > 0.0 { chord.z / length } else { 0.0 };
            position = at.plus(Vec3::new(0.0, 0.0, bird.lifted));
        } else {
            let ahead = facing.to_vector().scale(share);
            let mut next = position.horizontal().plus(ahead);
            let mut z = position.z;
            if matches!(bird.phase, Phase::Hover) {
                next = next.plus(bird.push.scale(1.0 / flight.ticks));
                z += bird.climb / flight.ticks;
                facing = Facing::from_radians(facing.radians() + bird.drift / flight.ticks);
            }
            bird.slope = 0.0;
            bird.hard_turn = false;
            position = Vec3::new(next.x, next.y, z);
        }
        let ease = NUDGE_UP.max(bird.lifted.abs()) / flight.ticks;
        bird.lifted = if bird.lifted > 0.0 {
            (bird.lifted - ease).max(0.0)
        } else {
            (bird.lifted + ease).min(0.0)
        };
        if let Some(ground) = self.ground_at(position.horizontal()) {
            position.z = position.z.max(ground + GROUND_CLEARANCE * flight.radius);
        }
        if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
            body.0.position = position.horizontal();
            body.0.facing = facing;
            body.0.speed = bird.step * flight.hz;
            tick.state_mut().insert(ids.body, entity, body)?;
        }
        match tick.state_mut().get_mut(ids.elevation, entity) {
            Some(elevation) => elevation.0 = position.z,
            None => {
                tick.state_mut().insert(
                    ids.elevation,
                    entity,
                    crate::unit::Elevation::new(position.z),
                )?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn plan(
        &self,
        scene: Scene<'_>,
        bird: &mut GiantBird,
        flight: &Flight,
        from: Vec3,
        to: Vec3,
        shape: Shape,
        fan: bool,
    ) {
        let to = if fan {
            self.fan_search(scene, flight, from, to)
        } else {
            to
        };
        let heading = flight.facing.to_vector();
        let forward = Vec3::new(heading.x, heading.y, 0.0);
        let mut inner = match self.turn_point(bird, flight, from, to) {
            Some(via) => vec![from, via, to],
            None => vec![from, to],
        };
        let Some(mut path) = path_through(&inner, forward, shape) else {
            bird.path = None;
            return;
        };
        let pushers = self.pushers(scene, flight, from.horizontal(), path.length());
        for _ in 0..MAX_LIFTS {
            let Some((along, lifted)) = self.dip(&path, flight, &pushers) else {
                break;
            };
            let Some(segment) = path.segment_at(along) else {
                break;
            };
            let last = inner.len().saturating_sub(1);
            match (1..last).find(|at| {
                inner
                    .get(*at)
                    .is_some_and(|point| point.minus(lifted).length() <= flight.radius)
            }) {
                Some(at) => {
                    if let Some(point) = inner.get_mut(at) {
                        *point = lifted;
                    }
                }
                None => inner.insert(segment.saturating_add(1).min(last), lifted),
            }
            match path_through(&inner, forward, shape) {
                Some(next) => path = next,
                None => break,
            }
        }
        bird.path = Some(path);
        bird.travelled = 0.0;
        bird.lifted = flight.position.z - from.z;
    }

    fn turn_point(&self, bird: &GiantBird, flight: &Flight, from: Vec3, to: Vec3) -> Option<Vec3> {
        let heading = flight.facing.to_vector();
        let toward = to.minus(from).horizontal();
        if heading.dot(toward) >= 0.0 || flight.limits.turn_rate <= 0.0 {
            return None;
        }
        let speed = (bird.step * flight.hz).max(TURN_SPEED * flight.limits.max_speed);
        let radius = speed / flight.limits.turn_rate;
        if !radius.is_finite() || toward.length() <= 2.0 * radius {
            return None;
        }
        let left = Vec2::new(-heading.y, heading.x);
        let side = if left.dot(toward) >= 0.0 { 1.0 } else { -1.0 };
        let at = from
            .horizontal()
            .plus(heading.scale(radius))
            .plus(left.scale(side * radius));
        Some(Vec3::new(at.x, at.y, from.z))
    }

    fn dip(&self, path: &FlightPath, flight: &Flight, pushers: &[Pusher]) -> Option<(f32, Vec3)> {
        let spacing = (0.5 * flight.radius).max(1.0);
        let most = f32::from(u16::try_from(MAX_CLEARANCE_SAMPLES).unwrap_or(u16::MAX));
        let count = (path.length() / spacing).floor().clamp(0.0, most);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = count as u16;
        let mut seen_clear = false;
        let mut run: Option<(f32, f32)> = None;
        for index in 0..=count {
            let along = (spacing * f32::from(index)).min(path.length());
            let point = path.sample(along)?;
            let blocked = self.correction(point, flight, pushers).length_squared() > 0.0;
            match (blocked, run) {
                (true, None) if seen_clear => run = Some((along, along)),
                (true, Some((start, _))) => run = Some((start, along)),
                (false, Some((start, end))) => {
                    let middle = 0.5 * (start + end);
                    let point = path.sample(middle)?;
                    let lifted = point.plus(self.correction(point, flight, pushers));
                    return Some((middle, lifted));
                }
                _ => {}
            }
            seen_clear |= !blocked;
        }
        None
    }

    fn correction(&self, point: Vec3, flight: &Flight, pushers: &[Pusher]) -> Vec3 {
        Vec3::new(0.0, 0.0, self.clearance(point, flight)).plus(separation(
            point,
            flight.radius,
            pushers,
        ))
    }

    fn pushers(&self, scene: Scene<'_>, flight: &Flight, centre: Vec2, reach: f32) -> Vec<Pusher> {
        let Scene {
            world,
            ids,
            entity,
            victim,
        } = scene;
        let Some(store) = world.store(ids.body) else {
            return Vec::new();
        };
        let near = SEPARATION_NEAR * flight.radius;
        store
            .iter()
            .filter(|(other, _)| *other != entity && Some(*other) != victim)
            .filter(|(other, _)| !world.contains(ids.contained, *other))
            .filter_map(|(other, body)| {
                let kind = self.bird_kind(world, ids, other);
                if !kind.pushes
                    || world
                        .get(ids.health, other)
                        .is_some_and(|health| health.0.is_dead())
                {
                    return None;
                }
                let radius = world.get(ids.shape, other)?.radius;
                let position = body.0.position;
                let at = Vec3::new(position.x, position.y, Self::feet(world, ids, other));
                if !kind.immobile && at.minus(flight.position).length() > near {
                    return None;
                }
                let within = reach + flight.radius + 2.0 * radius;
                (position.minus(centre).length() <= within).then_some(Pusher { at, radius })
            })
            .collect()
    }

    fn fan_search(&self, scene: Scene<'_>, flight: &Flight, from: Vec3, to: Vec3) -> Vec3 {
        let pushing = self.pushers(scene, flight, to.horizontal(), 0.0);
        let escape = pushing
            .iter()
            .filter(|pusher| separation(to, flight.radius, &[**pusher]).length_squared() > 0.0)
            .map(|pusher| {
                pusher.at.minus(to).horizontal().length() + 2.0 * pusher.radius + flight.radius
            })
            .fold(0.0f32, f32::max)
            * ESCAPE_MARGIN;
        let widest = FAN_RINGS
            .iter()
            .map(|ring| ring * flight.radius)
            .fold(escape, f32::max);
        let pushers = self.pushers(scene, flight, to.horizontal(), widest);
        let clear = |at: Vec3| {
            self.ground_at(at.horizontal())
                .is_none_or(|ground| ground < at.z)
                && separation(at, flight.radius, &pushers).length_squared() == 0.0
        };
        if clear(to) {
            return to;
        }
        let back = from.minus(to).horizontal();
        let facing = mint_math::f32::atan2(back.y, back.x);
        let rings = FAN_RINGS.map(|ring| ring * flight.radius);
        for radius in rings.into_iter().chain((escape > 0.0).then_some(escape)) {
            let mut turns = 0u16;
            loop {
                let off = FAN_STEP * f32::from(turns);
                if off > core::f32::consts::PI {
                    break;
                }
                for angle in [facing + off, facing - off] {
                    let (sin, cos) = mint_math::f32::sin_cos(angle);
                    let at = Vec3::new(to.x + radius * cos, to.y + radius * sin, to.z);
                    if clear(at) {
                        return at;
                    }
                }
                turns = turns.saturating_add(1);
            }
        }
        to
    }

    fn clearance(&self, point: Vec3, flight: &Flight) -> f32 {
        let centre = point.horizontal();
        let r = flight.radius;
        let edges = [
            Vec2::new(r, 0.0),
            Vec2::new(-r, 0.0),
            Vec2::new(0.0, r),
            Vec2::new(0.0, -r),
        ];
        let over = |at: Vec2, share: f32| {
            self.ground_at(at)
                .map_or(0.0, |ground| ground - (point.z - share * flight.preferred))
        };
        edges
            .into_iter()
            .map(|edge| over(centre.plus(edge), EDGE_CLEARANCE))
            .fold(over(centre, CENTRE_CLEARANCE), f32::max)
            .max(0.0)
    }

    fn pose(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        climbing: bool,
        falling: bool,
    ) -> Result<()> {
        let mut add: Vec<&str> = Vec::new();
        let mut remove: Vec<&str> = Vec::new();
        for (name, wanted) in [(CLIMBING, climbing), (FREEFALL, falling)] {
            match (wanted, self.holds_model_condition(tick, entity, name)) {
                (true, false) => add.push(name),
                (false, true) => remove.push(name),
                _ => {}
            }
        }
        if add.is_empty() && remove.is_empty() {
            return Ok(());
        }
        self.set_conditions(tick, ids, entity, &add, &remove)
    }

    fn swoop_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        bird: Entity,
        target: Entity,
        from: Vec2,
    ) -> Entity {
        let whole = tick
            .state()
            .get(ids.member, target)
            .map_or(target, |member| member.horde);
        let Some(horde) = tick.state().get(ids.horde, whole) else {
            return target;
        };
        let claimed: Vec<Entity> = tick
            .state()
            .store(ids.giant_bird)
            .map(|store| {
                store
                    .iter()
                    .filter(|(other, _)| *other != bird)
                    .filter(|(_, other)| {
                        matches!(other.phase, Phase::Swoop { .. } | Phase::Attack { .. })
                    })
                    .filter_map(|(_, other)| other.victim())
                    .collect()
            })
            .unwrap_or_default();
        let nearest = |free: bool| {
            horde
                .members
                .iter()
                .filter(|member| self.target_is_live(tick, ids, **member))
                .filter(|member| !free || !claimed.contains(member))
                .filter_map(|member| {
                    let at = tick.state().get(ids.body, *member)?.0.position;
                    Some((at.minus(from).length(), *member))
                })
                .fold(None, |best: Option<(f32, Entity)>, found| match best {
                    Some(best) if best.0 <= found.0 => Some(best),
                    _ => Some(found),
                })
                .map(|(_, member)| member)
        };
        nearest(true).or_else(|| nearest(false)).unwrap_or(target)
    }

    fn horde_of_victim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        target: Entity,
        victim: Entity,
    ) -> Option<Entity> {
        let world = tick.state();
        if world.contains(ids.horde, target) {
            return Some(target);
        }
        world
            .get(ids.member, victim)
            .map(|member| member.horde)
            .filter(|horde| world.contains(ids.horde, *horde))
    }

    fn stands_for(tick: &Tick<'_>, ids: Ids, target: Entity, victim: Entity) -> bool {
        target == victim
            || tick
                .state()
                .get(ids.horde, target)
                .is_some_and(|horde| horde.members.contains(&victim))
    }

    pub(super) fn swoop_victim_live(&self, tick: &Tick<'_>, ids: Ids, target: Entity) -> bool {
        match tick.state().get(ids.horde, target) {
            Some(horde) => horde
                .members
                .iter()
                .any(|member| self.target_is_live(tick, ids, *member)),
            None => self.target_is_live(tick, ids, target),
        }
    }

    fn swoop_point(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        bird: Entity,
        victim: Entity,
        from: Vec2,
    ) -> Option<Vec3> {
        let body = tick.state().get(ids.body, victim)?.0;
        let feet = Self::feet(tick.state(), ids, victim);
        let labels = self
            .ai_table_of(tick, ids, bird)
            .map(|ai| ai.special_contact_points.tags())
            .unwrap_or_default();
        let points = self.contact_points_of(tick, ids, victim);
        if let Some((at, height)) =
            nearest_labelled(points, labels, body.position, body.facing, from)
        {
            return Some(Vec3::new(at.x, at.y, feet + height));
        }
        let height = tick
            .state()
            .get(ids.shape, victim)
            .map_or(0.0, |shape| shape.height);
        Some(Vec3::new(
            body.position.x,
            body.position.y,
            feet + 0.5 * height,
        ))
    }

    pub(super) fn giant_bird_engagement(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<Engagement> {
        if !self.is_giant_bird(tick.state(), ids, entity) {
            return None;
        }
        if self.carries_catch(tick, ids, entity) {
            return Some(Engagement::Busy { target: None });
        }
        let phase = tick
            .state()
            .get(ids.giant_bird, entity)
            .map_or(Phase::Hover, |bird| bird.phase);
        Some(match phase {
            Phase::Hover | Phase::Cruise { .. } => Engagement::Free,
            Phase::Swoop { target, .. } | Phase::FollowThrough { target } => Engagement::Busy {
                target: Some(target),
            },
            Phase::Attack {
                target,
                clip: false,
                fired: true,
            } => Engagement::Busy {
                target: Some(target),
            },
            Phase::Attack { target, .. } => Engagement::Strike(target),
        })
    }

    pub(super) fn begin_giant_bird_swoop(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        target: Entity,
    ) -> Result<()> {
        self.start_swoop(tick, ids, entity, target, false)
    }

    pub(super) fn giant_bird_power_edge(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        source: Entity,
        power: &str,
        target: Option<Entity>,
        edge: mint_sage_modules::CastEdge,
    ) -> Result<()> {
        let Some(target) = target.filter(|_| edge == mint_sage_modules::CastEdge::Trigger) else {
            return Ok(());
        };
        if !self.is_giant_bird(tick.state(), ids, source) {
            return Ok(());
        }
        let swoops = tick
            .state()
            .get(ids.kind, source)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry.record.declared_modules.iter().any(|module| {
                    module.name.eq_ignore_ascii_case(SWOOP_POWER)
                        && module
                            .field("SpecialPowerTemplate")
                            .and_then(|field| field.args.first())
                            .is_some_and(|named| named.eq_ignore_ascii_case(power))
                })
            });
        if !swoops || !self.swoop_victim_live(tick, ids, target) {
            return Ok(());
        }
        self.start_swoop(tick, ids, source, target, true)
    }

    fn start_swoop(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        target: Entity,
        power: bool,
    ) -> Result<()> {
        let Some(mut bird) = tick.state().get(ids.giant_bird, entity).cloned() else {
            return Ok(());
        };
        let free = if power {
            !matches!(bird.phase, Phase::Attack { .. })
        } else {
            matches!(bird.phase, Phase::Hover | Phase::Cruise { .. })
        };
        if !free {
            return Ok(());
        }
        let hz = self.logic_hz().clamp(1.0, Self::ticks_per_second());
        bird.phase = Phase::Swoop {
            target,
            goal: Vec3::ZERO,
            clip: false,
        };
        let from = match self.flight(tick, ids, entity, &bird, hz) {
            Some(flight) => flight.position.horizontal(),
            None => return Ok(()),
        };
        let victim = self.swoop_victim(tick, ids, entity, target, from);
        bird.horde = self.horde_of_victim(tick, ids, target, victim);
        if power {
            bird.mode = Mode::Power;
            bird.set = SwoopSet::Panic;
        } else {
            self.choose_mode(tick, ids, entity, &mut bird, victim);
        }
        let Some(flight) = self.flight(tick, ids, entity, &bird, hz) else {
            return Ok(());
        };
        let Some(goal) = self.aim(tick, ids, entity, bird.mode, victim, &flight) else {
            return Ok(());
        };
        let clip = bird.mode == Mode::Strike
            && tick
                .state()
                .get(ids.armed, entity)
                .and_then(|armed| self.combat.weapons.get(armed.weapon))
                .is_some_and(|weapon| weapon.clip_size > 1)
            && self.airborne_category(tick.state(), ids, victim).is_none();
        if tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| power || path.kind != PathKind::Ordered)
        {
            tick.state_mut().remove(ids.path, entity);
        }
        let shape = Shape {
            start: if power { Start::Dive } else { Start::Along },
            descend: clip,
        };
        let scene = Scene {
            world: tick.state(),
            ids,
            entity,
            victim: Some(victim),
        };
        self.plan(
            scene,
            &mut bird,
            &flight,
            flight.position,
            goal,
            shape,
            false,
        );
        bird.brake = SWOOP_BRAKE;
        bird.enter(Phase::Swoop {
            target: victim,
            goal,
            clip,
        });
        diag!(
            "bird: frame {} entity {} {} entity {} at ({:.0}, {:.0}, {:.0}) on {}{}",
            tick.frame().get(),
            entity.index(),
            match bird.mode {
                Mode::Strike => "swoops on",
                Mode::Duel => "duels",
                Mode::Power => "power-swoops on",
            },
            victim.index(),
            goal.x,
            goal.y,
            goal.z,
            bird.set.name(),
            if clip { " with its clip" } else { "" }
        );
        tick.state_mut().insert(ids.giant_bird, entity, bird)?;
        Ok(())
    }

    fn choose_mode(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        bird: &mut GiantBird,
        victim: Entity,
    ) {
        if !self.fights_back(tick, ids, entity, victim) {
            bird.mode = Mode::Strike;
            bird.set = SwoopSet::Supersonic;
            return;
        }
        if bird.mode != Mode::Duel {
            bird.set = if tick.rng().below(DUEL_ROLLS) < DUEL_WANDER_ROLLS {
                SwoopSet::Wander
            } else {
                SwoopSet::Supersonic
            };
        }
        bird.mode = Mode::Duel;
    }

    fn fights_back(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, victim: Entity) -> bool {
        if self.airborne_category(tick.state(), ids, victim).is_none() {
            return false;
        }
        let weapon = tick
            .state()
            .get(ids.armed, victim)
            .and_then(|armed| armed.target)
            .is_some_and(|target| Self::stands_for(tick, ids, target, entity));
        let swoop = tick
            .state()
            .get(ids.giant_bird, victim)
            .and_then(GiantBird::victim)
            .is_some_and(|target| target == entity);
        weapon || swoop
    }

    fn aim(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        mode: Mode,
        victim: Entity,
        flight: &Flight,
    ) -> Option<Vec3> {
        if mode == Mode::Duel {
            let at = tick.state().get(ids.body, victim)?.0.position;
            let ground = self.ground_at(at)?;
            return Some(Vec3::new(at.x, at.y, ground + flight.preferred));
        }
        self.swoop_point(tick, ids, entity, victim, flight.position.horizontal())
    }

    pub(super) fn giant_bird_fired(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(mut bird) = tick.state().get(ids.giant_bird, entity).cloned() else {
            return Ok(());
        };
        if let Phase::Attack { target, clip, .. } = bird.phase {
            bird.phase = Phase::Attack {
                target,
                clip,
                fired: true,
            };
            tick.state_mut().insert(ids.giant_bird, entity, bird)?;
        }
        Ok(())
    }

    fn carries_catch(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        tick.state()
            .get(ids.passengers, entity)
            .is_some_and(|list| {
                list.0.iter().any(|held| {
                    tick.state()
                        .get(ids.contained, *held)
                        .is_some_and(|held| held.grabbed_at.is_some())
                })
            })
    }
}

fn path_through(inner: &[Vec3], forward: Vec3, shape: Shape) -> Option<FlightPath> {
    let from = *inner.first()?;
    let to = *inner.last()?;
    let second = *inner.get(1)?;
    let before_last = *inner.get(inner.len().checked_sub(2)?)?;
    let lead_in = (LEAD_IN * second.minus(from).length()).min(MAX_LEAD_IN);
    let first = match shape.start {
        Start::Along => from.minus(forward.scale(lead_in)),
        Start::Climb => from.minus(forward.scale(lead_in).plus(Vec3::new(0.0, 0.0, lead_in))),
        Start::Dive => Vec3::new(from.x, from.y, POWER_DIVE_HEIGHT.max(from.z + lead_in)),
    };
    let last_leg = to.minus(before_last);
    let lead_out = (LEAD_OUT * last_leg.length()).clamp(MIN_LEAD_OUT, MAX_LEAD_OUT);
    let mut last = to.plus(last_leg.normalised().unwrap_or(forward).scale(lead_out));
    if shape.descend {
        last.z -= (LEAD_IN * last_leg.length()).min(MAX_LEAD_IN);
    }
    let mut points = Vec::with_capacity(inner.len().saturating_add(2));
    points.push(first);
    points.extend_from_slice(inner);
    points.push(last);
    FlightPath::new(points)
}

fn separation(point: Vec3, radius: f32, pushers: &[Pusher]) -> Vec3 {
    pushers.iter().fold(Vec3::ZERO, |push, pusher| {
        let away = point.minus(pusher.at);
        if away.horizontal().length() > radius + pusher.radius {
            return push;
        }
        let overlap = 2.0 * pusher.radius + radius - away.length();
        if overlap <= 0.0 {
            return push;
        }
        let unit = away.normalised().unwrap_or(Vec3::new(0.0, 0.0, 1.0));
        push.plus(unit.scale(overlap))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_at_five_logic_frames_a_second_decides_every_sixth_tick() {
        let starts: Vec<u64> = (0..20)
            .filter(|frame| {
                RotwkRules::logic_frame_index(*frame, 5.0)
                    != RotwkRules::logic_frame_index(frame.saturating_sub(1), 5.0)
                    || *frame == 0
            })
            .collect();
        assert_eq!(starts, vec![0, 6, 12, 18]);
        assert!((1..20).all(|frame| RotwkRules::logic_frame_index(frame, 30.0) == frame));
    }
}
