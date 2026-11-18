use super::*;
use crate::grid::{Cell, CellFlags, CostGrid, GridExtent};

#[test]
fn scaled_speed_clamps_a_non_finite_result_and_never_goes_negative() {
    let mover = MoverState {
        speed: 10.0,
        ..MoverState::new(Vec2::ZERO, Facing::from_radians(0.0))
    };
    assert_eq!(mover.scaled_speed(0.5), 5.0);
    assert_eq!(mover.scaled_speed(0.0), 0.0);
    assert_eq!(
        mover.scaled_speed(-1.0),
        0.0,
        "a negative factor never reverses a speed"
    );
    assert_eq!(
        mover.scaled_speed(f32::INFINITY),
        10.0,
        "a non-finite result keeps the original speed"
    );
}

#[test]
fn velocity_is_speed_along_facing() {
    let mover = MoverState {
        speed: 4.0,
        ..MoverState::new(Vec2::ZERO, Facing::from_radians(0.0))
    };
    let v = mover.velocity();
    assert!(v.minus(mover.facing.to_vector().scale(4.0)).length() < 1e-6);
}

const WALL: CellFlags = CellFlags::from_bits(1);
const MOVER: MoverClass = MoverClass::new(WALL);

fn limits() -> MotionLimits {
    MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        backing_speed: None,
        wheel_backing: None,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 30.0,
        acceleration: 60.0,
        braking: 60.0,
        turn_rate: 6.0,
        arrival_radius: 2.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    }
}

fn geometry() -> CellGeometry {
    CellGeometry::new(10.0, Vec2::ZERO).unwrap()
}

fn run(grid: &CostGrid, route: &Route, ticks: usize) -> (MoverState, MotionStatus) {
    let geometry = geometry();
    let context = MoveContext {
        grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let mut state = MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST);
    let mut status = MotionStatus::Moving;
    for _ in 0..ticks {
        let motion = SteeringFollower.step(&context, state, route, &limits(), 1.0 / 30.0);
        state = motion.state;
        status = motion.status;
        if status != MotionStatus::Moving {
            break;
        }
    }
    (state, status)
}

#[test]
fn a_mover_reaches_its_destination_and_stops() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let route = Route::to_point(geometry().centre_of(Cell::new(9, 0)));
    let (state, status) = run(&grid, &route, 200);
    assert_eq!(status, MotionStatus::Arrived);
    assert_eq!(state.speed, 0.0);
    assert!(
        state.position.minus(Vec2::new(95.0, 5.0)).length() <= limits().arrival_radius,
        "stopped at {:?}",
        state.position
    );
}

#[test]
fn a_pathless_route_creeps_at_the_floor_speed_even_with_a_stalled_ramp() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let stalled = MotionLimits {
        acceleration: 0.0,
        desired_speed: 30.0,
        ..limits()
    };
    let geometry = geometry();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let start = geometry.centre_of(Cell::new(0, 0));
    let state = MoverState::new(start, Facing::EAST);
    let route = Route::to_point(Vec2::new(start.x + 100.0, start.y));
    let motion = SteeringFollower.step(&context, state, &route, &stalled, 1.0 / 30.0);
    assert_eq!(
        motion.state.speed, 0.0,
        "the ramp itself never accelerates with Acceleration = 0"
    );
    assert!(
        motion.state.position.minus(start).length() > 0.0,
        "a pathless step still moves while the ramp is stalled, at {:?}",
        motion.state.position
    );
}

#[test]
fn a_mover_stops_at_an_obstacle_rather_than_walking_through_it() {
    let mut grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    for y in 0..16 {
        grid.set_flags(Cell::new(4, y), WALL).unwrap();
    }
    let route = Route::to_point(geometry().centre_of(Cell::new(9, 0)));
    let (state, status) = run(&grid, &route, 200);
    assert_eq!(status, MotionStatus::Blocked);
    assert!(state.position.x < 40.0, "stopped at {:?}", state.position);
}

#[test]
fn a_mover_may_leave_a_blocked_cell_but_not_enter_a_new_one() {
    let mut grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    for x in 0..4u16 {
        grid.set_flags(Cell::new(x, 0), WALL).unwrap();
    }
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let route = Route::to_point(geometry().centre_of(Cell::new(9, 0)));
    let mut state = MoverState::new(geometry().centre_of(Cell::new(1, 0)), Facing::EAST);
    let mut status = MotionStatus::Moving;
    for _ in 0..200 {
        let motion = SteeringFollower.step(&context, state, &route, &limits(), 1.0 / 30.0);
        state = motion.state;
        status = motion.status;
        if status != MotionStatus::Moving {
            break;
        }
    }
    assert_eq!(
        status,
        MotionStatus::Arrived,
        "stuck at {:?}",
        state.position
    );
}

#[test]
fn a_mover_on_clear_ground_still_refuses_to_enter_a_blocked_cell() {
    let mut grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    grid.set_flags(Cell::new(2, 0), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let route = Route::to_point(geometry().centre_of(Cell::new(9, 0)));
    let mut state = MoverState::new(geometry().centre_of(Cell::new(0, 0)), Facing::EAST);
    let mut status = MotionStatus::Moving;
    for _ in 0..200 {
        let motion = SteeringFollower.step(&context, state, &route, &limits(), 1.0 / 30.0);
        state = motion.state;
        status = motion.status;
        if status != MotionStatus::Moving {
            break;
        }
    }
    assert_eq!(status, MotionStatus::Blocked);
    assert!(state.position.x < 25.0, "stopped at {:?}", state.position);
}

#[test]
fn an_unsearched_route_crosses_terrain_but_stops_at_an_object_footprint() {
    const TERRAIN: CellFlags = CellFlags::from_bits(2);
    const OBJECT: CellFlags = CellFlags::from_bits(4);
    let mover = MoverClass::new(TERRAIN.union(OBJECT)).object_stamped(OBJECT);
    let run = |flag: CellFlags| {
        let mut grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
        for x in 4..16u16 {
            grid.set_flags(Cell::new(x, 0), flag).unwrap();
        }
        let context = MoveContext {
            grid: &grid,
            geometry: geometry(),
            mover,
            layer: 0,
        };
        let mut route = Route::to_point(geometry().centre_of(Cell::new(9, 0)));
        route.unsearched = true;
        let mut state = MoverState::new(geometry().centre_of(Cell::new(0, 0)), Facing::EAST);
        let mut status = MotionStatus::Moving;
        for _ in 0..400 {
            let motion = SlidingFollower.step(&context, state, &route, &limits(), 1.0 / 30.0);
            state = motion.state;
            status = motion.status;
            if status != MotionStatus::Moving {
                break;
            }
        }
        (status, state.position)
    };
    let (status, at) = run(TERRAIN);
    assert_eq!(status, MotionStatus::Arrived, "stopped at {at:?}");
    let (status, at) = run(OBJECT);
    assert_eq!(status, MotionStatus::Blocked);
    assert!(at.x < 45.0, "walked into the footprint: {at:?}");
}

#[test]
fn a_diagonal_corner_clip_slides_along_the_clear_axis_instead_of_stopping() {
    let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
    grid.set_flags(Cell::new(1, 1), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let heading = Vec2::new(1.0, 1.0).normalised().unwrap();
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(9.9, 9.9),
        facing: Facing::from_vector(heading).unwrap(),
        speed: limits.max_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(1000.0, 1000.0));
    let motion = SlidingFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(
        motion.status,
        MotionStatus::Moving,
        "a diagonal corner clip should slide, not stop: {motion:?}"
    );
    let travel = limits.max_speed / 30.0;
    let expected = Vec2::new(9.9 + heading.x * travel, 9.9);
    assert!(
        (motion.state.position.x - expected.x).abs() < 1e-4
            && (motion.state.position.y - expected.y).abs() < 1e-4,
        "expected to slide along x to {expected:?}, got {:?}",
        motion.state.position
    );
    assert_eq!(
        motion.velocity.y, 0.0,
        "the y component must be shed by the slide"
    );
    assert!(
        motion.velocity.x > 0.0,
        "the x component must survive the slide"
    );
}

#[test]
fn a_diagonal_corner_clip_falls_back_to_the_y_axis_when_x_is_also_blocked() {
    let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
    grid.set_flags(Cell::new(1, 1), WALL).unwrap();
    grid.set_flags(Cell::new(1, 0), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let heading = Vec2::new(1.0, 1.0).normalised().unwrap();
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(9.9, 9.9),
        facing: Facing::from_vector(heading).unwrap(),
        speed: limits.max_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(1000.0, 1000.0));
    let motion = SlidingFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(motion.status, MotionStatus::Moving, "{motion:?}");
    assert!(
        motion.state.position.x - 9.9 < 1e-4,
        "x must not have advanced (that axis is walled): {:?}",
        motion.state.position
    );
    assert!(
        motion.state.position.y > 9.9,
        "y should have advanced instead: {:?}",
        motion.state.position
    );
    assert_eq!(motion.velocity.x, 0.0);
    assert!(motion.velocity.y > 0.0);
}

#[test]
fn the_strict_follower_never_slides_even_at_a_genuine_corner_clip() {
    let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
    grid.set_flags(Cell::new(1, 1), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let heading = Vec2::new(1.0, 1.0).normalised().unwrap();
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(9.9, 9.9),
        facing: Facing::from_vector(heading).unwrap(),
        speed: limits.max_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(1000.0, 1000.0));
    let motion = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(
        motion.status,
        MotionStatus::Blocked,
        "the strict follower must not have picked up the slide: {motion:?}"
    );
    assert_eq!(motion.state.position, state.position);
}

#[test]
fn a_diagonal_step_boxed_in_on_both_axes_still_stops() {
    let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
    grid.set_flags(Cell::new(1, 1), WALL).unwrap();
    grid.set_flags(Cell::new(1, 0), WALL).unwrap();
    grid.set_flags(Cell::new(0, 1), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let heading = Vec2::new(1.0, 1.0).normalised().unwrap();
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(9.9, 9.9),
        facing: Facing::from_vector(heading).unwrap(),
        speed: limits.max_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(1000.0, 1000.0));
    let motion = SlidingFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(motion.status, MotionStatus::Blocked, "{motion:?}");
    assert_eq!(motion.state.position, state.position);
}

#[test]
fn a_shallow_slide_along_a_wall_does_not_crawl() {
    let mut grid = CostGrid::new(GridExtent::new(40, 10).unwrap());
    for y in 3..10u16 {
        grid.set_flags(Cell::new(1, y), WALL).unwrap();
    }
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let start = Vec2::new(9.5, 95.0);
    let goal = Vec2::new(300.0, 5.0);
    let route = Route::to_point(goal);
    let mut state = MoverState::new(start, Facing::from_vector(goal.minus(start)).unwrap());
    for _ in 0..900 {
        state = SlidingFollower
            .step(&context, state, &route, &limits, 1.0 / 30.0)
            .state;
    }
    assert!(
        state.position.x > 20.0,
        "still pinned against the wall after 30 s: {state:?}"
    );
}

#[test]
fn a_straight_run_into_a_flat_wall_has_no_axis_to_slide_along() {
    let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
    for y in 0..4u16 {
        grid.set_flags(Cell::new(1, y), WALL).unwrap();
    }
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = limits();
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(9.9, 5.0),
        facing: Facing::EAST,
        speed: limits.max_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(1000.0, 5.0));
    let motion = SlidingFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(motion.status, MotionStatus::Blocked, "{motion:?}");
    assert_eq!(motion.state.position, state.position);
}

#[test]
fn a_mover_turns_at_its_turn_rate_rather_than_snapping() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let route = Route::to_point(Vec2::new(5.0, 155.0));
    let state = MoverState::new(Vec2::new(5.0, 5.0), Facing::EAST);
    let dt = 1.0 / 30.0;
    let motion = SteeringFollower.step(&context, state, &route, &limits(), dt);
    let turned = motion.state.facing.radians();
    assert!(
        (turned - limits().turn_rate * dt).abs() < 1e-5,
        "turned {turned} in one tick"
    );
}

#[test]
fn a_mover_turns_at_half_rate_and_publishes_no_turn_pose_at_a_portal() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let route = Route {
        points: vec![Vec2::new(5.0, 155.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![Some(Portal::new(1, 2))],
    };
    let state = MoverState::new(Vec2::new(5.0, 5.0), Facing::EAST);
    let dt = 1.0 / 30.0;
    let motion = SteeringFollower.step(&context, state, &route, &limits(), dt);
    let turned = motion.state.facing.radians();
    assert!(
        (turned - 0.5 * limits().turn_rate * dt).abs() < 1e-5,
        "turned {turned} in one tick"
    );
    assert!(!motion.phase.contains(MotionPhase::TURNING_LEFT));
    assert!(!motion.phase.contains(MotionPhase::TURNING_RIGHT));
}

fn orbiting_limits() -> MotionLimits {
    MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        backing_speed: None,
        wheel_backing: None,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 40.0,
        acceleration: 60.0,
        braking: 80.0,
        turn_rate: 6.0,
        arrival_radius: 4.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    }
}

fn open_grid() -> CostGrid {
    CostGrid::new(GridExtent::new(64, 64).unwrap())
}

fn drive(
    grid: &CostGrid,
    mut state: MoverState,
    route: &Route,
    limits: &MotionLimits,
    ticks: usize,
) -> (MoverState, MotionStatus, usize) {
    let context = MoveContext {
        grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let mut status = MotionStatus::Moving;
    for tick in 0..ticks {
        let motion = SteeringFollower.step(&context, state, route, limits, 1.0 / 30.0);
        state = motion.state;
        status = motion.status;
        if status != MotionStatus::Moving {
            return (state, status, tick);
        }
    }
    (state, status, ticks)
}

#[test]
fn an_intermediate_waypoint_inside_the_turning_circle_is_not_orbited() {
    let grid = open_grid();
    let limits = orbiting_limits();
    assert!(
        limits.turning_radius() > limits.arrival_radius,
        "this test is meaningless unless the waypoint is unreachable by turning"
    );
    let route = Route {
        points: vec![Vec2::new(100.0, 105.0), Vec2::new(400.0, 105.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(100.0, 100.0),
        facing: Facing::EAST,
        speed: limits.max_speed,
        waypoint: 0,
    };
    let (state, status, ticks) = drive(&grid, state, &route, &limits, 450);
    assert_eq!(status, MotionStatus::Arrived, "gave up at {state:?}");
    assert!(
        state.position.minus(Vec2::new(400.0, 105.0)).length() <= limits.arrival_radius,
        "arrived after {ticks} ticks at {:?}",
        state.position
    );
}

#[test]
fn a_waypoint_the_mover_has_gone_past_is_retired_rather_than_turned_back_to() {
    let grid = open_grid();
    let limits = orbiting_limits();
    let route = Route {
        points: vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(200.0, 100.0),
            Vec2::new(300.0, 100.0),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(240.0, 130.0),
        facing: Facing::EAST,
        speed: limits.max_speed,
        waypoint: 1,
    };
    assert!(
        state.position.minus(Vec2::new(200.0, 100.0)).length() > limits.capture_radius(),
        "the point of this test is that no distance test could retire it"
    );
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let motion = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(motion.state.waypoint, 2);
}

#[test]
fn a_hard_turn_costs_speed_and_a_straight_run_does_not() {
    let grid = open_grid();
    let limits = orbiting_limits();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(100.0, 100.0),
        facing: Facing::EAST,
        speed: limits.max_speed,
        waypoint: 0,
    };
    let ahead = Route::to_point(Vec2::new(400.0, 100.0));
    let sideways = Route::to_point(Vec2::new(100.0, 400.0));
    let dt = 1.0 / 30.0;
    let straight = SteeringFollower.step(&context, state, &ahead, &limits, dt);
    let turning = SteeringFollower.step(&context, state, &sideways, &limits, dt);
    assert_eq!(straight.state.speed, limits.max_speed);
    assert!(
        turning.state.speed < straight.state.speed,
        "a right-angle turn cost nothing: {} vs {}",
        turning.state.speed,
        straight.state.speed
    );
    assert!((turning.state.speed - 37.333_332).abs() < 1e-4);
}

#[test]
fn the_destination_keeps_its_declared_arrival_radius() {
    let grid = open_grid();
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        backing_speed: None,
        wheel_backing: None,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 45.0,
        acceleration: 60.0,
        braking: 80.0,
        turn_rate: 8.0,
        arrival_radius: 2.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    };
    assert!(limits.capture_radius() > limits.arrival_radius);
    let goal = Vec2::new(300.0, 160.0);
    let state = MoverState::new(Vec2::new(100.0, 100.0), Facing::EAST);
    let (state, status, _) = drive(&grid, state, &Route::to_point(goal), &limits, 600);
    assert_eq!(status, MotionStatus::Arrived);
    assert!(
        state.position.minus(goal).length() <= limits.arrival_radius,
        "stopped {:?} short of {goal:?}",
        state.position.minus(goal).length()
    );
}

#[test]
fn an_extra_distance_between_one_and_two_frames_short_arrives_early() {
    let grid = open_grid();
    let mut limits = limits();
    limits.desired_speed = 20.0;
    limits.max_speed = 30.0;
    limits.arrival_radius = 1.0;
    let goal = Vec2::new(250.0, 100.0);
    let start = MoverState::new(Vec2::new(100.0, 100.0), Facing::EAST);

    let mut with_extra = Route::to_point(goal);
    with_extra.extra_distance = 6.0;
    let (with_extra_state, with_extra_status, _) = drive(&grid, start, &with_extra, &limits, 600);
    assert_eq!(with_extra_status, MotionStatus::Arrived);
    let with_extra_gap = with_extra_state.position.minus(goal).length();
    assert!(
        with_extra_gap > limits.arrival_radius,
        "arrived {with_extra_gap} short of the goal, expected past the plain radius"
    );

    let plain = Route::to_point(goal);
    let (plain_state, plain_status, _) = drive(&grid, start, &plain, &limits, 600);
    assert_eq!(plain_status, MotionStatus::Arrived);
    let plain_gap = plain_state.position.minus(goal).length();
    assert!(plain_gap <= limits.arrival_radius);
    assert!(
        with_extra_gap > plain_gap,
        "extra distance {with_extra_gap} should arrive earlier than the plain route {plain_gap}"
    );
}

#[test]
fn a_capture_radius_never_sits_inside_the_turning_circle() {
    let mut limits = orbiting_limits();
    assert!((limits.turning_radius() - 40.0 / 6.0).abs() < 1e-5);
    assert!((limits.capture_radius() - 40.0 / 6.0).abs() < 1e-5);
    assert!(!limits.clears_turning_circle());
    limits.arrival_radius = 20.0;
    assert_eq!(limits.capture_radius(), 20.0);
    assert!(limits.clears_turning_circle());
    limits.turn_rate = 0.0;
    assert_eq!(limits.turning_radius(), f32::INFINITY);
    assert_eq!(limits.capture_radius(), 20.0);
    limits.turn_rate = f32::NAN;
    assert_eq!(limits.capture_radius(), 20.0);
    limits.turn_rate = 6.0;
    limits.arrival_radius = f32::NAN;
    assert!((limits.capture_radius() - 40.0 / 6.0).abs() < 1e-5);
}

#[test]
fn the_same_tick_computed_twice_gives_bit_identical_state() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let route = Route::to_point(Vec2::new(120.0, 87.0));
    let first = run(&grid, &route, 40);
    let second = run(&grid, &route, 40);
    assert_eq!(first.0.position.x.to_bits(), second.0.position.x.to_bits());
    assert_eq!(first.0.position.y.to_bits(), second.0.position.y.to_bits());
    assert_eq!(
        first.0.facing.radians().to_bits(),
        second.0.facing.radians().to_bits()
    );
}

#[test]
fn motion_phase_reports_accelerating_decelerating_and_the_turn_sign() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let geometry = geometry();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let dt = 1.0 / 30.0;

    let at_rest = MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST);
    let far_ahead = Route::to_point(geometry.centre_of(Cell::new(9, 0)));
    let motion = SteeringFollower.step(&context, at_rest, &far_ahead, &limits(), dt);
    assert!(motion.phase.contains(MotionPhase::ACCELERATING));
    assert!(!motion.phase.contains(MotionPhase::DECELERATING));

    let braking = MotionLimits {
        acc_dec_trigger: 1.0,
        min_speed: 0.0,
        ..limits()
    };
    let braking_distance = 30.0 * 30.0 / (2.0 * 60.0);
    let near_stop = MoverState {
        speed: 30.0,
        ramp: Ramp {
            has_run: true,
            ..Ramp::default()
        },
        ..MoverState::new(
            geometry.centre_of(Cell::new(0, 0)),
            Facing::from_radians(core::f32::consts::PI),
        )
    };
    let close = Route::to_point(
        near_stop
            .position
            .plus(near_stop.facing.to_vector().scale(braking_distance * 0.5)),
    );
    let motion = SteeringFollower.step(&context, near_stop, &close, &braking, dt);
    assert!(motion.phase.contains(MotionPhase::DECELERATING));

    let facing_east = MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST);
    let to_the_left = Route::to_point(facing_east.position.plus(Vec2::new(1.0, 10.0)));
    let motion = SteeringFollower.step(&context, facing_east, &to_the_left, &limits(), dt);
    assert!(motion.phase.contains(MotionPhase::TURNING_LEFT));
    assert!(!motion.phase.contains(MotionPhase::TURNING_RIGHT));

    let to_the_right = Route::to_point(facing_east.position.plus(Vec2::new(1.0, -10.0)));
    let motion = SteeringFollower.step(&context, facing_east, &to_the_right, &limits(), dt);
    assert!(motion.phase.contains(MotionPhase::TURNING_RIGHT));
    assert!(!motion.phase.contains(MotionPhase::TURNING_LEFT));
}

#[test]
fn a_heading_error_under_the_turn_threshold_sets_no_turn_bit() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let geometry = geometry();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let dt = 1.0 / 30.0;
    let thresholded = MotionLimits {
        turn_threshold: 0.0873,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        ..limits()
    };
    let facing_east = MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST);

    let barely_off = Route::to_point(facing_east.position.plus(Vec2::new(100.0, 3.0)));
    let motion = SteeringFollower.step(&context, facing_east, &barely_off, &thresholded, dt);
    assert_eq!(motion.phase, MotionPhase::ACCELERATING);

    let well_off = Route::to_point(facing_east.position.plus(Vec2::new(100.0, 20.0)));
    let motion = SteeringFollower.step(&context, facing_east, &well_off, &thresholded, dt);
    assert!(motion.phase.contains(MotionPhase::TURNING_LEFT));
}

#[test]
fn a_mover_oscillating_by_a_small_error_never_enters_a_turn_phase() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let geometry = geometry();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let thresholded = MotionLimits {
        turn_threshold: 0.0873,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        ..limits()
    };
    let mut state = MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST);
    let route = Route::to_point(state.position.plus(Vec2::new(1000.0, 30.0)));
    for _ in 0..90 {
        let motion = SteeringFollower.step(&context, state, &route, &thresholded, 1.0 / 30.0);
        assert!(!motion.phase.contains(MotionPhase::TURNING_LEFT));
        assert!(!motion.phase.contains(MotionPhase::TURNING_RIGHT));
        state = motion.state;
        if motion.status != MotionStatus::Moving {
            break;
        }
    }
}

#[test]
fn a_mover_above_its_acc_dec_trigger_publishes_neither_speed_phase() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let geometry = geometry();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let at_speed = |speed| MoverState {
        speed,
        ..MoverState::new(geometry.centre_of(Cell::new(0, 0)), Facing::EAST)
    };
    let route = Route::to_point(at_speed(0.0).position.plus(Vec2::new(1000.0, 30.0)));
    for speed in [20.0, 30.0] {
        let motion =
            SteeringFollower.step(&context, at_speed(speed), &route, &limits(), 1.0 / 30.0);
        assert!(!motion.phase.contains(MotionPhase::ACCELERATING), "{speed}");
        assert!(!motion.phase.contains(MotionPhase::DECELERATING), "{speed}");
    }
}

#[test]
fn a_min_speed_mover_never_slows_below_its_floor_for_a_hard_turn() {
    let grid = open_grid();
    let limits = MotionLimits {
        min_speed: 5.0,
        ..orbiting_limits()
    };
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let state = MoverState {
        ramp: Default::default(),
        position: Vec2::new(100.0, 100.0),
        facing: Facing::EAST,
        speed: limits.min_speed,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(-400.0, 100.0));
    let motion = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
    assert_eq!(motion.state.speed, limits.min_speed);
}

fn winged_limits() -> MotionLimits {
    MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        backing_speed: None,
        wheel_backing: None,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 60.0,
        acceleration: 60.0,
        braking: 60.0,
        turn_rate: 1.8,
        arrival_radius: 2.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: Some(TurnCircle {
            slow_radius: 32.0,
            fast_radius: 48.0,
        }),
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    }
}

#[test]
fn a_mover_with_a_turn_circle_comes_round_on_a_half_circle_rather_than_pivoting() {
    let grid = open_grid();
    let limits = winged_limits();
    let start = Vec2::new(300.0, 300.0);
    let route = Route::to_point(Vec2::new(200.0, 300.0));
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let mut state = MoverState::new(start, Facing::EAST);
    let mut widest = 0.0f32;
    for _ in 0..(30 * 10) {
        let motion = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
        state = motion.state;
        widest = widest.max((state.position.y - start.y).abs());
        if state.facing.to_vector().x < -0.99 {
            assert!(
                widest >= 2.0 * limits.turn_circle.unwrap().slow_radius * 0.9,
                "came round within {widest} of its line - a pivot, not a half circle"
            );
            return;
        }
    }
    panic!("never came round");
}

#[test]
fn a_mover_with_a_turn_circle_reaches_a_point_behind_it() {
    let grid = open_grid();
    let limits = winged_limits();
    let state = MoverState {
        speed: limits.max_speed,
        ..MoverState::new(Vec2::new(300.0, 300.0), Facing::EAST)
    };
    let goal = Vec2::new(150.0, 300.0);
    let route = Route::to_point(goal);
    let (end, status, _) = drive(&grid, state, &route, &limits, 30 * 30);
    assert_eq!(
        status,
        MotionStatus::Arrived,
        "still going at {:?}",
        end.position
    );
    assert!(end.position.minus(goal).length() <= limits.capture_radius() + 1.0);
}

fn off_polyline(points: &[Vec2], point: Vec2) -> f32 {
    points
        .windows(2)
        .map(|leg| {
            let (a, b) = (leg[0], leg[1]);
            let ab = b.minus(a);
            let t = (point.minus(a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            point.minus(a.plus(ab.scale(t))).length()
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn a_mover_placed_on_its_route_stays_on_the_polyline_through_a_corner() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let points = vec![
        Vec2::new(100.0, 100.0),
        Vec2::new(300.0, 100.0),
        Vec2::new(300.0, 300.0),
    ];
    let route = Route {
        points: points.clone(),
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let mut state = MoverState::new(points[0], Facing::EAST);
    let mut worst = 0.0f32;
    for _ in 0..600 {
        let motion = SteeringFollower.step(&context, state, &route, &orbiting_limits(), 1.0 / 30.0);
        state = motion.state;
        worst = worst.max(off_polyline(&points, state.position));
        if motion.status != MotionStatus::Moving {
            break;
        }
    }
    assert!(worst < 1e-3, "strayed {worst} from the route");
    assert!(state.position.minus(points[2]).length() <= orbiting_limits().arrival_radius);
}

#[test]
fn a_mover_brakes_to_one_step_a_frame_then_stops_dead_at_close_enough() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = MotionLimits {
        arrival_radius: 1.0,
        ..limits()
    };
    let end = Vec2::new(205.0, 5.0);
    let route = Route::to_point(end);
    let mut state = MoverState::new(Vec2::new(5.0, 5.0), Facing::EAST);
    let mut slowest_braking = f32::INFINITY;
    let mut motion = None;
    for _ in 0..600 {
        let step = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
        if step.status == MotionStatus::Moving && step.state.ramp.braking {
            slowest_braking = slowest_braking.min(step.state.speed);
        }
        state = step.state;
        motion = Some(step);
        if step.status != MotionStatus::Moving {
            break;
        }
    }
    let motion = motion.unwrap();
    assert_eq!(motion.status, MotionStatus::Arrived);
    assert_eq!(motion.state.speed, 0.0);
    assert!(motion.state.position.minus(end).length() <= 1.0);
    let floor = limits.braking * limits.frame;
    assert!(
        (slowest_braking - floor).abs() < 1e-3,
        "braked down to {slowest_braking}, the floor is {floor}"
    );
}

#[test]
fn a_mover_that_does_not_turn_while_moving_keeps_its_facing_round_a_bend() {
    let grid = open_grid();
    let limits = MotionLimits {
        turn_while_moving: false,
        ..orbiting_limits()
    };
    let route = Route {
        points: vec![
            Vec2::new(100.0, 100.0),
            Vec2::new(200.0, 100.0),
            Vec2::new(300.0, 157.7),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let start = MoverState::new(Vec2::new(100.0, 100.0), Facing::EAST);
    let (end, status, _) = drive(&grid, start, &route, &limits, 600);
    assert_eq!(status, MotionStatus::Arrived);
    assert_eq!(end.facing, Facing::EAST);
}

#[test]
fn the_gait_phases_follow_the_ramp() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let gaits = |limits: &MotionLimits, length: f32| {
        let route = Route::to_point(Vec2::new(5.0 + length, 5.0));
        let mut state = MoverState::new(Vec2::new(5.0, 5.0), Facing::EAST);
        let mut seen: Vec<MotionPhase> = Vec::new();
        for _ in 0..600 {
            let motion = SteeringFollower.step(&context, state, &route, limits, 1.0 / 30.0);
            state = motion.state;
            if motion.status != MotionStatus::Moving {
                break;
            }
            if seen.last() != Some(&motion.phase) {
                seen.push(motion.phase);
            }
        }
        seen
    };
    assert_eq!(
        gaits(&limits(), 300.0),
        [
            MotionPhase::ACCELERATING,
            MotionPhase::NONE,
            MotionPhase::DECELERATING
        ]
    );
    let walker = MotionLimits {
        walk_distance: 50.0,
        ..limits()
    };
    assert_eq!(gaits(&walker, 20.0), [MotionPhase::WALKING]);
}

#[test]
fn a_right_angle_corner_is_rounded_by_a_tangent_arc() {
    let mut route = Route {
        points: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 100.0),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    round_corners(&mut route, 20.0, |_| true);
    assert_eq!(route.points.len(), 9);
    let centre = Vec2::new(80.0, 20.0);
    let arc = &route.points[1..8];
    assert!(arc[0].minus(Vec2::new(80.0, 0.0)).length() < 1e-3);
    assert!(arc[6].minus(Vec2::new(100.0, 20.0)).length() < 1e-3);
    assert!(arc
        .iter()
        .all(|point| (point.minus(centre).length() - 20.0).abs() < 1e-3));
}

#[test]
fn the_corner_angle_decides_whether_and_how_widely_it_is_rounded() {
    let bend = |degrees: f32| {
        let (sin, cos) = mint_math::f32::sin_cos(degrees.to_radians());
        let mut route = Route {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(200.0, 0.0),
                Vec2::new(200.0 + 200.0 * cos, 200.0 * sin),
            ],
            extra_distance: 0.0,
            unsearched: false,
            portals: Vec::new(),
        };
        round_corners(&mut route, 20.0, |_| true);
        route.points
    };
    assert_eq!(bend(100.0).len(), 3, "kept sharp");
    let rounded = bend(30.0);
    assert!(rounded.len() > 3);
    assert!((rounded[1].x - (200.0 - 35.0 * mint_math::f32::tan(15f32.to_radians()))).abs() < 1e-2);
}

#[test]
fn a_portal_corner_is_never_rounded() {
    let mut route = Route {
        points: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 100.0),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![None, Some(Portal::new(7, 3)), None],
    };
    round_corners(&mut route, 20.0, |_| true);
    assert_eq!(route.points.len(), 3, "the portal's corner stayed sharp");
    assert_eq!(route.portals, vec![None, Some(Portal::new(7, 3)), None]);
}

#[test]
fn asking_for_no_speed_stops_a_walker_dead_and_brakes_anything_else() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let route = Route::to_point(Vec2::new(900.0, 300.0));
    let state = MoverState {
        speed: 30.0,
        ..MoverState::new(Vec2::new(300.0, 300.0), Facing::EAST)
    };
    for (stops_dead, want) in [(true, 0.0), (false, 28.0)] {
        let limits = MotionLimits {
            desired_speed: 0.0,
            stops_dead,
            ..limits()
        };
        let motion = SteeringFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
        assert!(
            (motion.state.speed - want).abs() < 1e-4,
            "stops_dead {stops_dead}: speed {}",
            motion.state.speed
        );
    }
}

#[test]
fn a_route_curves_out_of_the_facing_only_between_the_two_gates() {
    let start = Vec2::new(100.0, 100.0);
    let first = Vec2::new(100.0, 200.0);
    for (heading, max_turn, curves) in [
        (Facing::EAST, core::f32::consts::PI, true),
        (Facing::EAST, 1.0, false),
        (Facing::from_radians(1.5), core::f32::consts::PI, false),
    ] {
        let mut route = Route {
            points: vec![start, first],
            extra_distance: 0.0,
            unsearched: false,
            portals: Vec::new(),
        };
        let departure = crate::Departure {
            heading,
            radius: 20.0,
            max_turn,
            pass_first: false,
            backing: None,
        };
        crate::depart(&mut route, departure, |_| true, |_, _| true);
        assert_eq!(route.points.len() > 2, curves, "heading {heading:?}");
        if curves {
            let out = route.points[1].minus(start);
            assert!(out.x > 0.0 && out.y > 0.0, "left along {out:?}");
            let last = route.points[route.points.len() - 2];
            assert!(
                (last.minus(Vec2::new(100.0, 120.0)).length() - 20.0).abs() < 1e-3,
                "the arc ends off its circle at {last:?}"
            );
        }
    }
}

#[test]
fn the_departure_arc_tops_up_its_sweep_to_the_true_tangent() {
    let start = Vec2::new(100.0, 100.0);
    let first = Vec2::new(100.0, 200.0);
    let mut route = Route {
        points: vec![start, first],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let departure = crate::Departure {
        heading: Facing::EAST,
        radius: 20.0,
        max_turn: core::f32::consts::PI,
        pass_first: false,
        backing: None,
    };
    crate::depart(&mut route, departure, |_| true, |_, _| true);
    let last = route.points[route.points.len() - 2];
    let uncorrected = Vec2::new(120.0, 120.0);
    assert!(
        last.minus(uncorrected).length() > 1.0,
        "the top-up term left the arc at the plain 90-degree point {last:?}"
    );
    assert!(
        (last.minus(Vec2::new(100.0, 120.0)).length() - 20.0).abs() < 1e-3,
        "still on the turning circle at {last:?}"
    );
}

#[test]
fn n_less_than_two_takes_no_curve_rather_than_retrying_at_a_bigger_radius() {
    let start = Vec2::new(100.0, 100.0);
    let first = Vec2::new(100.0, 200.0);
    let mut route = Route {
        points: vec![start, first],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let departure = crate::Departure {
        heading: Facing::EAST,
        radius: 2.0,
        max_turn: core::f32::consts::PI,
        pass_first: false,
        backing: None,
    };
    crate::depart(&mut route, departure, |_| true, |_, _| true);
    assert_eq!(
        route.points,
        vec![start, first],
        "a sub-two-step sweep should take no curve at all"
    );
}

#[test]
fn a_backing_mover_faces_away_from_where_it_goes() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = MotionLimits {
        backs: true,
        backing_speed: None,
        wheel_backing: None,
        ..limits()
    };
    let route = Route::to_point(Vec2::new(600.0, 300.0));
    let mut state = MoverState::new(
        Vec2::new(300.0, 300.0),
        Facing::from_radians(core::f32::consts::PI),
    );
    for _ in 0..30 {
        state = SteeringFollower
            .step(&context, state, &route, &limits, 1.0 / 30.0)
            .state;
    }
    assert!(
        state.position.x > 310.0,
        "went only to {:?}",
        state.position
    );
    assert!(
        state.facing.to_vector().x < -0.99,
        "turned to {:?}",
        state.facing
    );
}

#[test]
fn a_backing_mover_is_held_to_its_backing_speed() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let top = |backs: bool| {
        let limits = MotionLimits {
            backs,
            backing_speed: Some(limits().max_speed / 3.0),
            wheel_backing: None,
            ..limits()
        };
        let route = Route::to_point(Vec2::new(600.0, 300.0));
        let mut state = MoverState::new(
            Vec2::new(300.0, 300.0),
            Facing::from_radians(core::f32::consts::PI),
        );
        let mut fastest = 0.0_f32;
        for _ in 0..60 {
            state = SteeringFollower
                .step(&context, state, &route, &limits, 1.0 / 30.0)
                .state;
            fastest = fastest.max(state.speed);
        }
        (fastest, limits.max_speed)
    };
    let (backing, max) = top(true);
    assert!(backing <= max / 3.0 + 0.01, "{backing} of {max}");
    assert!(backing > 0.0);
    let (forward, _) = top(false);
    assert!(forward > max / 3.0 + 1.0, "{forward}");
}

fn walk_with_flag(mut state: MoverState, stops_dead: bool, backs: bool, ticks: u32) -> MoverState {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let limits = MotionLimits {
        backs,
        stops_dead,
        ..limits()
    };
    let route = Route::to_point(Vec2::new(600.0, 300.0));
    for _ in 0..ticks {
        state = SteeringFollower
            .step(&context, state, &route, &limits, 1.0 / 30.0)
            .state;
    }
    state
}

#[test]
fn only_a_legs_mover_starts_backing_mid_move() {
    for (stops_dead, backs_after) in [(true, true), (false, false)] {
        let walking = walk_with_flag(
            MoverState::new(Vec2::new(300.0, 300.0), Facing::EAST),
            stops_dead,
            false,
            15,
        );
        assert!(walking.speed > 0.0);
        let after = walk_with_flag(walking, stops_dead, true, 30);
        assert_eq!(
            after.facing.to_vector().x < 0.0,
            backs_after,
            "stops_dead {stops_dead}: facing {:?}",
            after.facing
        );
    }
}

#[test]
fn backing_ends_the_tick_its_flag_fails() {
    for stops_dead in [true, false] {
        let backing = walk_with_flag(
            MoverState::new(
                Vec2::new(300.0, 300.0),
                Facing::from_radians(core::f32::consts::PI),
            ),
            stops_dead,
            true,
            20,
        );
        assert!(backing.speed > 0.0 && backing.facing.to_vector().x < -0.99);
        let after = walk_with_flag(backing, stops_dead, false, 30);
        assert!(
            after.facing.to_vector().x > 0.0,
            "stops_dead {stops_dead}: facing {:?}",
            after.facing
        );
    }
}

#[test]
fn a_wheeled_mover_backs_only_when_the_path_is_short_or_the_order_is_fresh() {
    let grid = open_grid();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let backs_towards = |goal_x: f32, moved_since_order: f32| {
        let limits = MotionLimits {
            wheel_backing: Some(WheelBacking {
                distance_min: 50.0,
                distance_max: 400.0,
                angle: core::f32::consts::FRAC_PI_2,
                stop_when_turning: false,
                moved_since_order,
                turning: false,
            }),
            ..limits()
        };
        let route = Route::to_point(Vec2::new(goal_x, 300.0));
        let mut motion = SteeringFollower.step(
            &context,
            MoverState::new(
                Vec2::new(300.0, 300.0),
                Facing::from_radians(core::f32::consts::PI),
            ),
            &route,
            &limits,
            1.0 / 30.0,
        );
        for _ in 0..4 {
            motion = SteeringFollower.step(&context, motion.state, &route, &limits, 1.0 / 30.0);
        }
        motion.phase.contains(MotionPhase::BACKING_UP)
    };
    assert!(backs_towards(600.0, 100.0), "300 to go backs");
    assert!(
        !backs_towards(1300.0, 100.0),
        "1000 to go, 100 from the order, turns"
    );
    assert!(
        backs_towards(1300.0, 10.0),
        "1000 to go, 10 from the order, backs"
    );
}

#[test]
fn a_climb_leg_scales_the_step_by_its_slope() {
    let mut grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    grid.set_flags(Cell::new(0, 0), WALL).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let climb = Portal {
        id: 1,
        kind: 3,
        leg: Leg::Climb,
        rise: 40.0,
    };
    let route = Route {
        points: vec![Vec2::new(5.0, 5.0), Vec2::new(15.0, 5.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![None, Some(climb)],
    };
    let mut state = MoverState::new(Vec2::new(5.0, 5.0), Facing::EAST);
    state.waypoint = 1;
    let dt = 1.0 / 30.0;
    let motion = SteeringFollower.step(&context, state, &route, &limits(), dt);
    let step = limits().acceleration * dt * dt;
    let expected = step * (10.0 / 41.0) * 0.85;
    let moved = motion.state.position.x - 5.0;
    assert!(
        (moved - expected).abs() < 1e-5,
        "moved {moved}, expected {expected}"
    );
    assert!(motion.phase.contains(MotionPhase::WALKING));
    assert!(motion.phase.contains(MotionPhase::CLIMBING));
    assert_ne!(motion.status, MotionStatus::Blocked);
}

#[test]
fn a_steep_descent_backs_down_the_slope() {
    let grid = CostGrid::new(GridExtent::new(16, 16).unwrap());
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let climb = Portal {
        id: 1,
        kind: 3,
        leg: Leg::Climb,
        rise: -40.0,
    };
    let route = Route {
        points: vec![Vec2::new(5.0, 5.0), Vec2::new(5.0, 15.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![None, Some(climb)],
    };
    let mut state = MoverState::new(
        Vec2::new(5.0, 5.0),
        Facing::from_radians(core::f32::consts::FRAC_PI_2),
    );
    state.waypoint = 1;
    state.speed = 5.0;
    let motion = SteeringFollower.step(&context, state, &route, &limits(), 1.0 / 30.0);
    assert!(motion.phase.contains(MotionPhase::BACKING_UP));
    assert!(motion.phase.contains(MotionPhase::CLIMBING));
    let turned = state.facing.delta_to(motion.state.facing).abs();
    assert!(turned > 0.0, "it turns away from the slope it descends");
}

#[test]
fn a_climb_leg_with_no_run_does_not_strand_the_mover() {
    let grid = CostGrid::new(GridExtent::new(200, 200).unwrap());
    let context = MoveContext {
        grid: &grid,
        geometry: geometry(),
        mover: MOVER,
        layer: 0,
    };
    let tag = |rise: f32, leg: Leg| {
        Some(Portal {
            id: 1,
            kind: 3,
            leg,
            rise,
        })
    };
    let route = Route {
        points: vec![
            Vec2::new(930.0, 2000.0),
            Vec2::new(955.0, 2005.0),
            Vec2::new(952.2, 2000.1),
            Vec2::new(970.5, 2000.0),
            Vec2::new(970.5, 2000.0),
            Vec2::new(992.3, 1994.7),
            Vec2::new(1000.0, 2000.0),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![
            None,
            None,
            None,
            tag(0.0, Leg::Walk),
            tag(53.0, Leg::Climb),
            tag(-0.2, Leg::Climb),
            tag(0.0, Leg::Climb),
        ],
    };
    let mut state = MoverState::new(route.points[4], Facing::from_radians(-2.63));
    state.speed = 50.0;
    state.waypoint = 4;
    let mut limits = limits();
    limits.max_speed = 50.0;
    limits.desired_speed = 50.0;
    for tick in 0..400 {
        let motion = SlidingFollower.step(&context, state, &route, &limits, 1.0 / 30.0);
        state = motion.state;
        if motion.status == MotionStatus::Arrived {
            return;
        }
        assert!(
            tick < 399,
            "stranded at {:?} on waypoint {}",
            state.position,
            state.waypoint
        );
    }
}

#[test]
fn a_steep_climb_leg_is_crossed_in_rise_over_step_frames() {
    let climb = Portal {
        id: 1,
        kind: 3,
        leg: Leg::Climb,
        rise: 53.0,
    };
    let route = Route {
        points: vec![Vec2::new(1000.0, 2000.0), Vec2::new(1000.1, 2000.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![None, Some(climb)],
    };
    let mut at = route.points[0];
    let mut frames = 0;
    while at != route.points[1] && frames < 100 {
        let look = super::path::walk(&route, at, 1, 5.0);
        at = look.point;
        frames += 1;
    }
    assert_eq!(frames, 13);
}

#[test]
fn a_wheeled_mover_doubles_its_error_before_it_ignores_it_at_speed() {
    let wheels = crate::following::TurnSlowdown {
        cap: 1.0,
        unhindered_above: Some(0.25),
        pivot_below: 1.0,
        ..crate::following::TurnSlowdown::PROPORTIONAL
    };
    let error = 15.0_f32.to_radians();
    let kept = wheels.keep(error, 10.0, 10.0, 1.0);
    assert!((kept - (1.0 - 2.0 * error / core::f32::consts::FRAC_PI_4)).abs() < 1e-5);
    let three_frames = crate::following::TurnSlowdown {
        pivot_below: 1.0 / 3.0,
        ..wheels
    };
    assert!((three_frames.keep(error, 10.0, 10.0, 1.0) - 1.0).abs() < 1e-6);
    assert!((three_frames.keep(error, 3.0, 10.0, 1.0) - kept).abs() < 1e-5);
}

#[test]
fn a_ships_slowdown_follows_the_current_turn_rate() {
    let ship = crate::following::TurnSlowdown {
        cap: 0.9,
        full_at_turn_steps: 3.5,
        turn_step_seconds: 0.2,
        ..crate::following::TurnSlowdown::PROPORTIONAL
    };
    let fast = ship.keep(0.35, 1.0, 10.0, 1.0);
    let slow = ship.keep(0.35, 1.0, 10.0, 0.5);
    assert!(slow < fast);
}

fn departing(first: Vec2, pass_first: bool, clear: impl Fn(Vec2) -> bool) -> Vec<Vec2> {
    let start = Vec2::new(100.0, 100.0);
    let mut route = Route {
        points: vec![start, first],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let departure = crate::Departure {
        heading: Facing::EAST,
        radius: 20.0,
        max_turn: core::f32::consts::PI,
        pass_first,
        backing: None,
    };
    crate::depart(&mut route, departure, clear, |_, _| true);
    route.points
}

#[test]
fn the_radius_five_arc_is_committed_untested() {
    let start = Vec2::new(100.0, 100.0);
    let route = departing(Vec2::new(100.0, 200.0), false, |p| {
        p.minus(start).length() < 6.0
    });
    assert!(route.len() > 2, "started straight: {route:?}");
    assert!(route[1].minus(start).length() < 10.0, "{route:?}");
}

#[test]
fn a_sharp_turn_tries_the_other_way_round_before_radius_five() {
    let route = departing(Vec2::new(60.0, 110.0), false, |p| p.y <= 100.5);
    assert!(route.len() > 2, "started straight: {route:?}");
    assert!(route[1].y < 100.0, "first point {:?}", route[1]);
}

#[test]
fn a_backing_departure_starts_behind_the_mover() {
    let start = Vec2::new(100.0, 100.0);
    let mut route = Route {
        points: vec![start, Vec2::new(-200.0, 100.0)],
        extra_distance: 0.0,
        unsearched: false,
        portals: Vec::new(),
    };
    let departure = crate::Departure {
        heading: Facing::EAST,
        radius: 5.0,
        max_turn: core::f32::consts::PI,
        pass_first: false,
        backing: Some(20.0),
    };
    crate::depart(&mut route, departure, |_| false, |_, _| false);
    assert!(route.points.len() > 3, "{:?}", route.points);
    assert!(route.points[1].x < 100.0, "{:?}", route.points);
}

#[test]
fn a_short_backing_departure_uses_the_ordinary_radius() {
    let start = Vec2::new(100.0, 100.0);
    let leg_to = Vec2::new(60.0, 160.0);
    let build = |heading, backing| {
        let mut route = Route {
            points: vec![start, leg_to],
            extra_distance: 0.0,
            unsearched: false,
            portals: Vec::new(),
        };
        let departure = crate::Departure {
            heading,
            radius: 10.0,
            max_turn: core::f32::consts::PI,
            pass_first: false,
            backing,
        };
        crate::depart(&mut route, departure, |_| true, |_, _| true);
        route.points
    };
    let backing = build(Facing::EAST, Some(100.0));
    let ordinary = build(Facing::from_radians(core::f32::consts::PI), None);
    assert!(ordinary.len() > 2);
    assert_eq!(backing, ordinary);
}

#[test]
fn a_floating_departure_commits_its_first_arc_untested() {
    let start = Vec2::new(100.0, 100.0);
    let refused = |_: Vec2| false;
    let asked = departing(Vec2::new(100.0, 200.0), true, refused);
    let retry = departing(Vec2::new(100.0, 200.0), false, refused);
    assert!(asked[1].minus(start).length() > retry[1].minus(start).length());
}
