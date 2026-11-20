use crate::geom::{CellGeometry, Facing, Vec2};
use crate::grid::{Leg, MoverClass, Passability, Portal};
use crate::search::CellPath;

mod path;

pub use path::{depart, round_corners, Departure};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Route {
    pub points: Vec<Vec2>,
    pub extra_distance: f32,
    pub portals: Vec<Option<Portal>>,
    pub unsearched: bool,
}

impl Route {
    #[must_use]
    pub fn from_path(path: &CellPath, geometry: CellGeometry) -> Self {
        let points: Vec<Vec2> = path.cells.iter().map(|c| geometry.centre_of(*c)).collect();
        let portals = vec![None; points.len()];
        Self {
            points,
            extra_distance: 0.0,
            unsearched: false,
            portals,
        }
    }

    #[must_use]
    pub fn crossing_at(&self, waypoint: u32) -> bool {
        path::crossing(self, waypoint).is_some()
    }

    #[must_use]
    pub fn tagged_at(&self, waypoint: u32) -> bool {
        usize::try_from(waypoint)
            .ok()
            .and_then(|at| self.portals.get(at))
            .is_some_and(Option::is_some)
    }

    #[must_use]
    pub fn place_along(&self, from: Vec2, waypoint: u32, distance: f32) -> (Vec2, u32) {
        let placed = path::walk(self, from, waypoint, distance);
        (placed.point, placed.waypoint)
    }

    #[must_use]
    pub fn past_crossing(&self, waypoint: u32) -> u32 {
        let last = u32::try_from(self.points.len().saturating_sub(1)).unwrap_or(u32::MAX);
        let mut at = waypoint;
        while at < last && self.crossing_at(at) {
            at = at.saturating_add(1);
        }
        at.min(last)
    }

    #[must_use]
    pub fn to_point(point: Vec2) -> Self {
        Self {
            points: vec![point],
            extra_distance: 0.0,
            unsearched: false,
            portals: vec![None],
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionLimits {
    pub max_speed: f32,
    pub desired_speed: f32,
    pub stops_dead: bool,
    pub acceleration: f32,
    pub braking: f32,
    pub turn_rate: f32,
    pub arrival_radius: f32,
    pub turn_threshold: f32,
    pub acc_dec_trigger: f32,
    pub min_speed: f32,
    pub turn_circle: Option<TurnCircle>,
    pub turn_slowdown: TurnSlowdown,
    pub frame: f32,
    pub turn_while_moving: bool,
    pub walk_distance: f32,
    pub turn_phases_at_speed: bool,
    pub corner_radius: f32,
    pub turn_pivot: f32,
    pub backs: bool,
    pub wheel_backing: Option<WheelBacking>,
    pub backing_speed: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelBacking {
    pub distance_min: f32,
    pub distance_max: f32,
    pub angle: f32,
    pub stop_when_turning: bool,
    pub moved_since_order: f32,
    pub turning: bool,
}

impl WheelBacking {
    #[must_use]
    pub fn backs(self, heading_error: f32, path_left: f32) -> bool {
        let refused = self.moved_since_order > self.distance_min && path_left > self.distance_max;
        !refused && heading_error.abs() > self.angle
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnSlowdown {
    pub cap: f32,
    pub full_at: f32,
    pub full_at_turn_steps: f32,
    pub turn_step_seconds: f32,
    pub unhindered_above: Option<f32>,
    pub pivot_below: f32,
}

impl TurnSlowdown {
    pub const PROPORTIONAL: Self = Self {
        cap: 1.0,
        full_at: core::f32::consts::FRAC_PI_4,
        full_at_turn_steps: 0.0,
        turn_step_seconds: 0.0,
        unhindered_above: None,
        pivot_below: 0.0,
    };

    #[must_use]
    pub fn keep(self, heading_error: f32, speed: f32, max_speed: f32, turn_rate: f32) -> f32 {
        let full_at = if self.full_at_turn_steps > 0.0 {
            self.full_at_turn_steps * turn_rate * self.turn_step_seconds
        } else {
            self.full_at
        };
        let error = if self.pivot_below > 0.0 && speed <= self.pivot_below * max_speed {
            heading_error * 2.0
        } else if self
            .unhindered_above
            .is_some_and(|above| speed > above * max_speed)
        {
            0.0
        } else {
            heading_error
        };
        if full_at <= 0.0 || !full_at.is_finite() {
            return 1.0 - self.cap.clamp(0.0, 1.0);
        }
        1.0 - (error / full_at).min(self.cap.clamp(0.0, 1.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurnCircle {
    pub slow_radius: f32,
    pub fast_radius: f32,
}

impl TurnCircle {
    #[must_use]
    pub fn radius_at(self, speed: f32, max_speed: f32) -> f32 {
        let fraction = if max_speed > 0.0 && speed.is_finite() {
            (speed / max_speed).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let slow = self.slow_radius.max(0.0);
        let fast = self.fast_radius.max(0.0);
        let radius = slow + (fast - slow) * fraction;
        if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        }
    }
}

impl MotionLimits {
    #[must_use]
    pub fn turning_radius(self) -> f32 {
        if !self.turn_rate.is_finite() || self.turn_rate <= 0.0 {
            return f32::INFINITY;
        }
        let radius = self.max_speed.max(0.0) / self.turn_rate;
        let radius = match self.turn_circle {
            Some(circle) => radius.max(circle.radius_at(self.max_speed, self.max_speed)),
            None => radius,
        };
        if radius.is_finite() {
            radius
        } else {
            f32::INFINITY
        }
    }

    #[must_use]
    pub fn capture_radius(self) -> f32 {
        let arrival = self.arrival_radius.max(0.0);
        let radius = self.turning_radius();
        if radius.is_finite() {
            arrival.max(radius)
        } else {
            arrival
        }
    }

    #[must_use]
    pub fn clears_turning_circle(self) -> bool {
        self.arrival_radius > self.turning_radius()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoverState {
    pub position: Vec2,
    pub facing: Facing,
    pub speed: f32,
    pub waypoint: u32,
    pub ramp: Ramp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ramp {
    pub braking: bool,
    pub has_run: bool,
    pub walking: bool,
    pub backing: bool,
}

impl Ramp {
    #[must_use]
    pub fn bits(self) -> u8 {
        u8::from(self.braking)
            | (u8::from(self.has_run) << 1)
            | (u8::from(self.walking) << 2)
            | (u8::from(self.backing) << 3)
    }
}

impl MoverState {
    #[must_use]
    pub const fn new(position: Vec2, facing: Facing) -> Self {
        Self {
            position,
            facing,
            speed: 0.0,
            waypoint: 0,
            ramp: Ramp {
                braking: false,
                has_run: false,
                walking: false,
                backing: false,
            },
        }
    }

    #[must_use]
    pub fn velocity(self) -> Vec2 {
        self.facing.to_vector().scale(self.speed)
    }

    #[must_use]
    pub fn scaled_speed(self, factor: f32) -> f32 {
        let out = self.speed * factor;
        if out.is_finite() {
            out.max(0.0)
        } else {
            self.speed
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionStatus {
    Moving,
    Arrived,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MotionPhase(u8);

impl MotionPhase {
    pub const NONE: Self = Self(0);
    pub const ACCELERATING: Self = Self(1 << 0);
    pub const DECELERATING: Self = Self(1 << 1);
    pub const TURNING_LEFT: Self = Self(1 << 2);
    pub const TURNING_RIGHT: Self = Self(1 << 3);
    pub const WALKING: Self = Self(1 << 4);
    pub const HIGH_SPEED: Self = Self(1 << 5);
    pub const BACKING_UP: Self = Self(1 << 6);
    pub const CLIMBING: Self = Self(1 << 7);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub state: MoverState,
    pub velocity: Vec2,
    pub status: MotionStatus,
    pub phase: MotionPhase,
}

#[derive(Clone, Copy)]
pub struct MoveContext<'a> {
    pub grid: &'a dyn Passability,
    pub geometry: CellGeometry,
    pub mover: MoverClass,
    pub layer: u8,
}

pub trait Follower {
    fn step(
        &self,
        context: &MoveContext<'_>,
        state: MoverState,
        route: &Route,
        limits: &MotionLimits,
        dt: f32,
    ) -> Motion;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SteeringFollower;

const MIN_SLIDE_COMPONENT: f32 = 0.05;

fn passed(route: &Route, index: usize, point: Vec2, from_waypoint: Vec2) -> bool {
    let Some(previous) = index.checked_sub(1).and_then(|i| route.points.get(i)) else {
        return false;
    };
    from_waypoint.dot(point.minus(*previous)) >= 0.0
}

fn stopped(status: MotionStatus, state: MoverState) -> Motion {
    Motion {
        state: MoverState {
            speed: 0.0,
            ..state
        },
        velocity: Vec2::ZERO,
        status,
        phase: MotionPhase::NONE,
    }
}

struct Proposed {
    state: MoverState,
    heading: Vec2,
    travel: f32,
    speed: f32,
    velocity: Vec2,
    position: Vec2,
    waypoint: u32,
    passable: bool,
    from_blocked: bool,
    arrived: bool,
    phase: MotionPhase,
}

impl Proposed {
    fn taken(&self, position: Vec2, velocity: Vec2) -> Motion {
        if self.arrived {
            return Motion {
                state: MoverState {
                    position,
                    speed: 0.0,
                    waypoint: self.waypoint,
                    ramp: Ramp::default(),
                    ..self.state
                },
                velocity: Vec2::ZERO,
                status: MotionStatus::Arrived,
                phase: MotionPhase::NONE,
            };
        }
        Motion {
            state: MoverState {
                position,
                speed: self.speed,
                waypoint: self.waypoint,
                ..self.state
            },
            velocity,
            status: MotionStatus::Moving,
            phase: self.phase,
        }
    }
}

const LOOK_AHEAD_MIN: f32 = 0.1;

const TURN_DEAD_ZONE: f32 = 0.1;

const SHORT_LEG_THRESHOLD_DIVISOR: f32 = 16.0;

const STRAIGHT_BELOW: f32 = 10.0;

const ARRIVAL_END_RANGE: f32 = 40.0;

const PATHLESS_FLOOR_SPEED: f32 = 10.0;

fn distance_to_go(route: &Route, position: Vec2, waypoint: u32, travel: f32) -> f32 {
    let Some(end) = route.points.last() else {
        return 0.0;
    };
    let straight = end.minus(position).length();
    let ahead = path::walk(route, position, waypoint, travel.max(LOOK_AHEAD_MIN));
    let left = path::length_left(route, ahead.point, ahead.waypoint, path::octagonal);
    if left < STRAIGHT_BELOW {
        straight
    } else {
        left.max(straight)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookAhead {
    pub point: Vec2,
    pub path_left: f32,
    pub waypoint: u32,
}

#[must_use]
pub fn look_ahead(route: &Route, state: MoverState, limits: &MotionLimits) -> Option<LookAhead> {
    let waypoint = path::cursor(route, state.position, state.waypoint);
    if usize::try_from(waypoint).map_or(true, |at| at >= route.points.len()) {
        return None;
    }
    let distance = (state.speed.max(0.0) * frame_of(limits, 0.0)).max(LOOK_AHEAD_MIN);
    let ahead = path::walk(route, state.position, waypoint, distance);
    Some(LookAhead {
        point: ahead.point,
        path_left: path::length_left(route, ahead.point, ahead.waypoint, path::octagonal),
        waypoint: ahead.waypoint,
    })
}

fn frame_of(limits: &MotionLimits, fallback: f32) -> f32 {
    if limits.frame.is_finite() && limits.frame > 0.0 {
        limits.frame
    } else {
        fallback
    }
}

fn braking_distance(speed: f32, min_speed: f32, braking: f32, frame: f32) -> f32 {
    let over = speed - min_speed;
    if over <= 0.0 || braking <= 0.0 || frame <= 0.0 {
        return 0.0;
    }
    1.05 * (over / (braking * frame) + 1.0) * (min_speed + over / 2.0) * frame
}

fn propose(
    context: &MoveContext<'_>,
    state: MoverState,
    route: &Route,
    limits: &MotionLimits,
    dt: f32,
) -> Result<Proposed, Motion> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(stopped(MotionStatus::Moving, state));
    }
    if limits.turn_circle.is_some() {
        propose_flying(context, state, route, limits, dt)
    } else {
        propose_on_path(context, state, route, limits, dt)
    }
}

fn propose_on_path(
    context: &MoveContext<'_>,
    state: MoverState,
    route: &Route,
    limits: &MotionLimits,
    dt: f32,
) -> Result<Proposed, Motion> {
    let waypoint = path::cursor(route, state.position, state.waypoint);
    let state = MoverState { waypoint, ..state };
    if usize::try_from(waypoint).map_or(true, |at| at >= route.points.len()) {
        return Err(stopped(MotionStatus::Arrived, state));
    }
    let at_portal = path::at_portal(route, waypoint);
    let frame = frame_of(limits, dt);
    let max_speed = limits.max_speed.max(0.0);
    let old = state.speed.max(0.0);
    let mut ramp = state.ramp;
    if old <= 0.0 {
        ramp.has_run = false;
    }
    if limits.stops_dead || old <= 0.0 {
        ramp.backing = limits.backs;
    } else if !limits.backs {
        ramp.backing = false;
    }

    let look = path::walk(
        route,
        state.position,
        waypoint,
        (old * frame).max(LOOK_AHEAD_MIN),
    );
    let on_path = path::length_left(route, look.point, look.waypoint, path::octagonal)
        + route.extra_distance.max(0.0);
    let crossing = path::crossing(route, look.waypoint);
    if crossing.is_some_and(|tag| tag.leg == Leg::Halt) {
        return Err(stopped(MotionStatus::Arrived, state));
    }
    let steep = crossing
        .filter(|tag| tag.leg == Leg::Climb)
        .and_then(|tag| {
            let start = usize::try_from(look.waypoint)
                .ok()
                .and_then(|at| at.checked_sub(1))
                .and_then(|i| route.points.get(i))
                .copied()
                .unwrap_or(state.position);
            let end = usize::try_from(look.waypoint)
                .ok()
                .and_then(|at| route.points.get(at))
                .copied()?;
            path::steep(end.minus(start).length(), tag.rise)
        });
    let backing_down = steep == Some(false);
    if let Some(wheels) = limits.wheel_backing {
        let toward = look.point.minus(
            state
                .position
                .plus(state.facing.to_vector().scale(limits.turn_pivot)),
        );
        let error = Facing::from_vector(toward).map_or(0.0, |aim| state.facing.delta_to(aim));
        ramp.backing = wheels.backs(error, on_path);
    }
    let max_speed = match limits.backing_speed.filter(|_| ramp.backing) {
        Some(backing) => backing.max(0.0),
        None => max_speed,
    };
    let aim_at = if ramp.backing || backing_down {
        state.position.scale(2.0).minus(look.point)
    } else {
        look.point
    };
    let pivot = state
        .position
        .plus(state.facing.to_vector().scale(limits.turn_pivot));
    let to_look = aim_at.minus(pivot);
    let at_end = route.points.last() == Some(&look.point);
    let turn_delta =
        if at_end && to_look.x.abs() < TURN_DEAD_ZONE && to_look.y.abs() < TURN_DEAD_ZONE {
            0.0
        } else {
            Facing::from_vector(to_look).map_or(0.0, |aim| state.facing.delta_to(aim))
        };
    let turn_step = limits.turn_rate.max(0.0) * dt * if at_portal { 0.5 } else { 1.0 };
    let facing = if limits.turn_while_moving && crossing.is_none_or(|tag| tag.leg != Leg::Bare) {
        Facing::from_radians(state.facing.radians() + turn_delta.clamp(-turn_step, turn_step))
    } else {
        state.facing
    };

    let braking = limits.braking.max(0.0);
    let min_speed = limits.min_speed.max(0.0).min(max_speed);
    let desired = limits.desired_speed.max(0.0).min(max_speed);
    let stop_dead = limits.stops_dead && desired <= 0.0;
    let old = if stop_dead { 0.0 } else { old };
    let wheels_backing = limits.wheel_backing.filter(|_| ramp.backing);
    let mut goal = match wheels_backing {
        Some(wheels) if wheels.stop_when_turning && wheels.turning => 0.0,
        Some(_) => desired,
        None => {
            desired
                * limits
                    .turn_slowdown
                    .keep(turn_delta.abs(), old, max_speed, limits.turn_rate)
        }
    }
    .max(min_speed);
    if old - braking * frame > goal {
        goal = (old - braking * dt).max(braking * frame);
    }
    let slow = braking_distance(old, min_speed, braking, frame);
    if 3.0 * slow < on_path {
        ramp.braking = false;
    }
    if on_path < slow - 0.5 * old * frame {
        ramp.braking = true;
    }
    if ramp.braking {
        goal = (old - braking * dt).max(braking * frame);
    }
    let mut speed = old;
    if goal > speed && !ramp.braking {
        speed += limits.acceleration.max(0.0) * dt;
    }
    if speed > goal {
        speed = (speed - braking * dt).max(goal);
    }
    let speed = if speed.is_finite() {
        speed.max(0.0)
    } else {
        0.0
    };

    let speed = if crossing.is_some() {
        let ceiling = limits.desired_speed.max(0.0).min(max_speed).max(min_speed);
        ramp.braking = false;
        if ceiling > old {
            (old + limits.acceleration.max(0.0) * dt).min(ceiling)
        } else {
            ceiling
        }
    } else {
        speed
    };
    let mut distance = speed * dt;
    if route.points.len() == 1 {
        let remaining = route
            .points
            .last()
            .map_or(0.0, |end| end.minus(state.position).length());
        distance = distance.max(PATHLESS_FLOOR_SPEED * dt).min(remaining);
    }
    let placed = path::walk(route, state.position, waypoint, distance);
    let step = placed.point.minus(state.position);
    let travel = step.length();
    let heading = step.normalised().unwrap_or_else(|| facing.to_vector());
    let distance_to_go = distance_to_go(route, placed.point, placed.waypoint, speed * frame);
    let cur = speed * frame;
    let extra = route.extra_distance.max(0.0);
    let arrived = (distance_to_go <= limits.arrival_radius.max(0.0)
        || (extra > cur && 2.0 * cur > distance_to_go))
        && route
            .points
            .last()
            .is_some_and(|end| end.minus(placed.point).length() <= ARRIVAL_END_RANGE);

    let mut phase = gait(limits, &mut ramp, old, speed, on_path, dt);
    if ramp.backing && speed > 0.0 {
        phase = phase.union(MotionPhase::BACKING_UP);
    }
    if let Some(tag) = crossing {
        phase = match tag.leg {
            Leg::Bare | Leg::Through => MotionPhase::NONE,
            _ => MotionPhase::WALKING,
        };
        if steep.is_some() {
            phase = phase.union(MotionPhase::CLIMBING);
        }
        if backing_down {
            phase = phase.union(MotionPhase::BACKING_UP);
        }
    }
    if limits.turn_while_moving && !at_portal {
        let short_leg = usize::try_from(waypoint).ok().is_some_and(|at| {
            let start = at
                .checked_sub(1)
                .and_then(|i| route.points.get(i))
                .copied()
                .unwrap_or(state.position);
            route
                .points
                .get(at)
                .is_some_and(|end| end.minus(start).length() < context.geometry.cell_size())
        });
        let threshold = if short_leg {
            limits.turn_threshold.max(0.0) / SHORT_LEG_THRESHOLD_DIVISOR
        } else {
            limits.turn_threshold.max(0.0)
        };
        let half = 0.5 * max_speed;
        if turn_delta.abs() > threshold && (old <= half || limits.turn_phases_at_speed) {
            phase = phase.union(if turn_delta > 0.0 {
                MotionPhase::TURNING_LEFT
            } else {
                MotionPhase::TURNING_RIGHT
            });
            if old > half {
                phase = phase.union(MotionPhase::HIGH_SPEED);
            }
        }
    }

    let position = placed.point;
    let passable = crossing.is_some() || passable_at(context, route, position);
    let from_blocked = crossing.is_none()
        && context
            .geometry
            .cell_at(state.position, context.layer, context.grid.extent())
            .is_none_or(|cell| !context.grid.is_passable(cell, context.mover));
    Ok(Proposed {
        state: MoverState {
            facing,
            ramp,
            ..state
        },
        heading,
        travel,
        speed,
        velocity: step.scale(1.0 / dt),
        position,
        waypoint: placed.waypoint,
        passable,
        from_blocked,
        arrived,
        phase,
    })
}

fn passable_at(context: &MoveContext<'_>, route: &Route, point: Vec2) -> bool {
    let cell = context
        .geometry
        .cell_at(point, context.layer, context.grid.extent());
    if route.unsearched {
        let mover = context.mover.objects_only();
        return cell.is_none_or(|cell| context.grid.is_passable(cell, mover));
    }
    cell.is_some_and(|cell| context.grid.is_passable(cell, context.mover))
}

fn gait(
    limits: &MotionLimits,
    ramp: &mut Ramp,
    old: f32,
    speed: f32,
    on_path: f32,
    dt: f32,
) -> MotionPhase {
    let trigger = limits.max_speed.max(0.0) * limits.acc_dec_trigger.clamp(0.0, 1.0);
    let walk = limits.walk_distance.max(0.0);
    if speed - old > 0.4 * limits.acceleration.max(0.0) * dt && old < trigger {
        ramp.walking = on_path < walk;
        return if ramp.walking {
            MotionPhase::WALKING
        } else {
            MotionPhase::ACCELERATING
        };
    }
    if ramp.braking && speed < trigger {
        ramp.walking = !ramp.has_run;
        return if ramp.walking {
            MotionPhase::WALKING
        } else {
            MotionPhase::DECELERATING
        };
    }
    if old > trigger {
        if on_path > walk {
            ramp.walking = false;
        }
        if !ramp.walking {
            ramp.has_run = true;
        }
    }
    if ramp.walking {
        MotionPhase::WALKING
    } else {
        MotionPhase::NONE
    }
}

fn propose_flying(
    context: &MoveContext<'_>,
    state: MoverState,
    route: &Route,
    limits: &MotionLimits,
    dt: f32,
) -> Result<Proposed, Motion> {
    let mut state = state;
    let count = u32::try_from(route.points.len()).unwrap_or(u32::MAX);
    let capture = limits.capture_radius();
    let arrival = limits.arrival_radius.max(0.0);
    let mut target = Vec2::ZERO;
    let mut remaining = 0.0f32;
    let mut last = true;
    let mut found = false;
    for _ in 0..count {
        let index = usize::try_from(state.waypoint).unwrap_or(usize::MAX);
        let Some(point) = route.points.get(index).copied() else {
            break;
        };
        let final_waypoint = state.waypoint.saturating_add(1) >= count;
        let from_waypoint = state.position.minus(point);
        let distance = from_waypoint.length();
        let reach = if final_waypoint { arrival } else { capture };
        let flown_past = final_waypoint
            && limits.turn_circle.is_some()
            && distance <= capture
            && state.facing.to_vector().dot(from_waypoint) > 0.0;
        if distance <= reach
            || flown_past
            || (!final_waypoint && passed(route, index, point, from_waypoint))
        {
            state.waypoint = state.waypoint.saturating_add(1);
            continue;
        }
        target = point;
        remaining = distance;
        last = final_waypoint;
        found = true;
        break;
    }
    if !found {
        if limits.turn_circle.is_some() {
            let velocity = state.facing.to_vector().scale(state.speed.max(0.0));
            let mut state = state;
            state.position = state.position.plus(velocity.scale(dt));
            return Err(Motion {
                state,
                velocity,
                status: MotionStatus::Arrived,
                phase: MotionPhase::NONE,
            });
        }
        return Err(stopped(MotionStatus::Arrived, state));
    }
    let mut on_path = remaining + route.extra_distance.max(0.0);
    if !last {
        let mut previous = target;
        let from = usize::try_from(state.waypoint).unwrap_or(usize::MAX);
        for point in route.points.iter().skip(from.saturating_add(1)) {
            on_path += point.minus(previous).length();
            previous = *point;
        }
    }

    let Some(desired) = Facing::from_vector(target.minus(state.position)) else {
        return Err(stopped(MotionStatus::Arrived, state));
    };
    let turn_delta = state.facing.delta_to(desired);
    let facing = state
        .facing
        .turned_towards(desired, max_turn(*limits, state, target, turn_delta, dt));
    let heading_error = turn_delta.abs();

    let alignment = if limits.turn_circle.is_some() {
        1.0
    } else {
        limits.turn_slowdown.keep(
            heading_error,
            state.speed,
            limits.max_speed.max(0.0),
            limits.turn_rate,
        )
    };

    let braking_distance = if limits.braking > 0.0 {
        state.speed * state.speed / (2.0 * limits.braking)
    } else {
        0.0
    };
    let min_speed = limits.min_speed.max(0.0).min(limits.max_speed.max(0.0));
    let target_speed = if on_path <= braking_distance {
        min_speed
    } else {
        (limits.desired_speed.max(0.0).min(limits.max_speed.max(0.0)) * alignment).max(min_speed)
    };
    let mut phase = MotionPhase::NONE;
    let acc_dec_ceiling = limits.max_speed.max(0.0) * limits.acc_dec_trigger.clamp(0.0, 1.0);
    if state.speed <= acc_dec_ceiling {
        if target_speed > state.speed {
            phase = phase.union(MotionPhase::ACCELERATING);
        } else if target_speed < state.speed {
            phase = phase.union(MotionPhase::DECELERATING);
        }
    }
    if heading_error > limits.turn_threshold.max(0.0) {
        if turn_delta > 0.0 {
            phase = phase.union(MotionPhase::TURNING_LEFT);
        } else if turn_delta < 0.0 {
            phase = phase.union(MotionPhase::TURNING_RIGHT);
        }
    }
    let speed = if target_speed > state.speed {
        f32::min(
            target_speed,
            state.speed + limits.acceleration.max(0.0) * dt,
        )
    } else {
        f32::max(target_speed, state.speed - limits.braking.max(0.0) * dt)
    }
    .clamp(0.0, limits.max_speed.max(0.0));

    let travel = f32::min(speed * dt, remaining);
    let heading = facing.to_vector();
    let velocity = heading.scale(speed);
    let position = state.position.plus(heading.scale(travel));

    let passable = passable_at(context, route, position);
    let from_blocked = context
        .geometry
        .cell_at(state.position, context.layer, context.grid.extent())
        .is_none_or(|cell| !context.grid.is_passable(cell, context.mover));

    Ok(Proposed {
        state: MoverState {
            facing,
            waypoint: state.waypoint,
            ..state
        },
        heading,
        travel,
        speed,
        velocity,
        position,
        waypoint: state.waypoint,
        passable,
        from_blocked,
        arrived: false,
        phase,
    })
}

fn max_turn(
    limits: MotionLimits,
    state: MoverState,
    target: Vec2,
    turn_delta: f32,
    dt: f32,
) -> f32 {
    let rate = limits.turn_rate.max(0.0) * dt;
    let Some(circle) = limits.turn_circle else {
        return rate;
    };
    let radius = circle.radius_at(state.speed, limits.max_speed);
    if radius <= 0.0 {
        return rate;
    }
    let heading = state.facing.to_vector();
    let side = if turn_delta >= 0.0 { 1.0 } else { -1.0 };
    let centre = state
        .position
        .plus(Vec2::new(-heading.y * side, heading.x * side).scale(radius));
    if target.minus(centre).length() < radius {
        return 0.0;
    }
    rate.min(state.speed.max(0.0) * dt / radius)
}

impl Follower for SteeringFollower {
    fn step(
        &self,
        context: &MoveContext<'_>,
        state: MoverState,
        route: &Route,
        limits: &MotionLimits,
        dt: f32,
    ) -> Motion {
        let proposed = match propose(context, state, route, limits, dt) {
            Ok(proposed) => proposed,
            Err(motion) => return motion,
        };
        if !proposed.passable && !proposed.from_blocked {
            return stopped(MotionStatus::Blocked, proposed.state);
        }
        proposed.taken(proposed.position, proposed.velocity)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlidingFollower;

impl Follower for SlidingFollower {
    fn step(
        &self,
        context: &MoveContext<'_>,
        state: MoverState,
        route: &Route,
        limits: &MotionLimits,
        dt: f32,
    ) -> Motion {
        let proposed = match propose(context, state, route, limits, dt) {
            Ok(proposed) => proposed,
            Err(motion) => return motion,
        };
        if !proposed.passable && !proposed.from_blocked {
            let original_position = proposed.state.position;
            let is_passable_at = |point: Vec2| passable_at(context, route, point);
            let diagonal = proposed.heading.x.abs() >= MIN_SLIDE_COMPONENT
                && proposed.heading.y.abs() >= MIN_SLIDE_COMPONENT;
            let slide_x = diagonal.then(|| {
                original_position.plus(Vec2::new(proposed.heading.x * proposed.travel, 0.0))
            });
            let slide_y = diagonal.then(|| {
                original_position.plus(Vec2::new(0.0, proposed.heading.y * proposed.travel))
            });
            let slid = if slide_x.is_some_and(is_passable_at) {
                slide_x.map(|position| (position, Vec2::new(proposed.velocity.x, 0.0)))
            } else if slide_y.is_some_and(is_passable_at) {
                slide_y.map(|position| (position, Vec2::new(0.0, proposed.velocity.y)))
            } else {
                None
            };
            let Some((position, velocity)) = slid else {
                return stopped(MotionStatus::Blocked, proposed.state);
            };
            return proposed.taken(position, velocity);
        }
        proposed.taken(proposed.position, proposed.velocity)
    }
}

#[cfg(test)]
mod tests;
