use super::*;
use crate::avoidance::{MoverId, NoNeighbours, SeparationAvoidance};
use crate::geom::{Facing, Vec2};
use crate::grid::{Cell, CellFlags, CostGrid, GridExtent, MoverClass};
use crate::search::GoalId;

const WALL: CellFlags = CellFlags::from_bits(1);
const MOVER: MoverClass = MoverClass::new(WALL);

fn walled_grid() -> CostGrid {
    let mut grid = CostGrid::new(GridExtent::new(24, 24).unwrap());
    for y in 0..20 {
        grid.set_flags(Cell::new(10, y), WALL).unwrap();
    }
    grid
}

fn request() -> PathRequest {
    PathRequest {
        corner_radius: 0.0,
        departure: None,
        start: Cell::new(1, 1),
        goal: Cell::new(22, 5),
        mover: MOVER,
        goal_id: GoalId(1),
        origin: None,
    }
}

fn ready(outcome: PlanOutcome) -> Plan {
    match outcome {
        PlanOutcome::Ready(plan) => plan,
        other => panic!("expected a plan, got {other:?}"),
    }
}

#[test]
fn both_first_configurations_are_constructible_and_produce_a_route() {
    let grid = walled_grid();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    for config in [PipelineConfig::straight_line(), PipelineConfig::default()] {
        let mut pipeline = Pipeline::new(config);
        let plan = ready(
            pipeline
                .plan(&grid, geometry, &request(), &mut NodeBudget::unlimited())
                .unwrap(),
        );
        assert!(!plan.route.is_empty(), "{config} produced no waypoints");
        assert_eq!(plan.cells.cells.first(), Some(&request().start));
        assert_eq!(plan.cells.cells.last(), Some(&request().goal));
    }
}

#[test]
fn the_null_search_walks_into_the_wall_and_a_star_walks_round_it() {
    let grid = walled_grid();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 40.0,
        acceleration: 80.0,
        braking: 80.0,
        turn_rate: 8.0,
        arrival_radius: 3.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    };
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };

    let mut outcomes = Vec::new();
    for config in [PipelineConfig::straight_line(), PipelineConfig::default()] {
        let mut pipeline = Pipeline::new(config);
        let plan = ready(
            pipeline
                .plan(&grid, geometry, &request(), &mut NodeBudget::unlimited())
                .unwrap(),
        );
        let mut state = MoverState::new(geometry.centre_of(request().start), Facing::EAST);
        let mut status = MotionStatus::Moving;
        for _ in 0..600 {
            let motion = pipeline.follow(&context, state, &plan.route, &limits, 1.0 / 30.0);
            state = motion.state;
            status = motion.status;
            if status != MotionStatus::Moving {
                break;
            }
        }
        outcomes.push((config.search, status, state.position));
    }
    assert_eq!(
        outcomes.first().map(|o| o.1),
        Some(MotionStatus::Blocked),
        "the null search should walk into the wall: {outcomes:?}"
    );
    assert_eq!(
        outcomes.last().map(|o| o.1),
        Some(MotionStatus::Arrived),
        "a* should get there: {outcomes:?}"
    );
}

#[test]
fn swapping_the_smoother_changes_the_waypoint_count_and_nothing_else() {
    let grid = walled_grid();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let mut pipeline = Pipeline::new(PipelineConfig::default());
    let pulled = ready(
        pipeline
            .plan(&grid, geometry, &request(), &mut NodeBudget::unlimited())
            .unwrap(),
    );
    pipeline.set_smoothing(SmoothingMode::Off);
    let raw = ready(
        pipeline
            .plan(&grid, geometry, &request(), &mut NodeBudget::unlimited())
            .unwrap(),
    );
    assert!(
        pulled.route.points.len() < raw.route.points.len(),
        "string-pull kept {} of {} waypoints",
        pulled.route.points.len(),
        raw.route.points.len()
    );
    assert_eq!(pulled.cells.cost, raw.cells.cost);
    assert_eq!(pipeline.config().smoothing, SmoothingMode::Off);
}

#[test]
fn null_avoidance_leaves_the_followers_motion_bit_identical() {
    let grid = walled_grid();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let pipeline = Pipeline::new(PipelineConfig::default());
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 40.0,
        acceleration: 80.0,
        braking: 80.0,
        turn_rate: 8.0,
        arrival_radius: 3.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    };
    let route = Route::to_point(Vec2::new(200.0, 55.0));
    let state = MoverState::new(Vec2::new(15.0, 15.0), Facing::EAST);
    let motion = pipeline.follow(&context, state, &route, &limits, 1.0 / 30.0);
    let agent = Agent {
        id: MoverId(0),
        position: motion.state.position,
        radius: 5.0,
    };
    let mut scratch = Vec::new();
    let avoided = pipeline.avoid(
        &context,
        &agent,
        state.position,
        motion,
        &NoNeighbours,
        &mut scratch,
        1.0 / 30.0,
    );
    assert_eq!(
        avoided.state.position.x.to_bits(),
        motion.state.position.x.to_bits()
    );
    assert_eq!(avoided, motion);
}

struct OneNeighbour(Neighbour);

impl NeighbourQuery for OneNeighbour {
    fn neighbours(&self, _centre: Vec2, _radius: f32, out: &mut Vec<Neighbour>) {
        out.push(self.0);
    }
}

#[test]
fn avoid_reconstructs_position_from_before_the_follower_stepped_not_after() {
    let grid = CostGrid::new(GridExtent::new(64, 64).unwrap());
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let pipeline = Pipeline::new(PipelineConfig {
        search: SearchMode::StraightLine,
        smoothing: SmoothingMode::Off,
        avoidance: AvoidanceMode::Separation,
    });
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 40.0,
        acceleration: 60.0,
        braking: 60.0,
        turn_rate: 8.0,
        arrival_radius: 3.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    };
    let dt = 1.0 / 30.0;
    let origin = Vec2::new(100.0, 100.0);
    let state = MoverState {
        ramp: Default::default(),
        position: origin,
        facing: Facing::EAST,
        speed: 40.0,
        waypoint: 0,
    };
    let route = Route::to_point(Vec2::new(10_100.0, 100.0));
    let motion = pipeline.follow(&context, state, &route, &limits, dt);
    assert_eq!(motion.status, MotionStatus::Moving);
    let post_follow = motion.state.position;
    assert!(
        (post_follow.x - (origin.x + 40.0 * dt)).abs() < 1e-4,
        "the fixture's own premise: an unturning, already-at-speed follower travels \
             exactly max_speed * dt this tick, got {post_follow:?}"
    );

    let neighbour = Neighbour {
        id: MoverId(9),
        position: Vec2::new(post_follow.x + 6.0, post_follow.y),
        velocity: Vec2::ZERO,
        radius: 5.0,
    };
    let agent = Agent {
        id: MoverId(1),
        position: post_follow,
        radius: 5.0,
    };
    let mut scratch = Vec::new();
    let adjusted = SeparationAvoidance::default().adjust(
        &agent,
        motion.velocity,
        &OneNeighbour(neighbour),
        &mut scratch,
    );
    assert_ne!(
        adjusted, motion.velocity,
        "the fixture's own premise: a push must fire"
    );

    let mut scratch = Vec::new();
    let avoided = pipeline.avoid(
        &context,
        &agent,
        origin,
        motion,
        &OneNeighbour(neighbour),
        &mut scratch,
        dt,
    );

    let correct = origin.x + adjusted.x * dt;
    let pre_fix_bug = post_follow.x + adjusted.x * dt;
    assert!(
        (avoided.state.position.x - correct).abs() < 1e-3,
        "expected {correct} (origin + adjusted*dt), got {}; the pre-fix formula would give \
             {pre_fix_bug} (post-follow position + adjusted*dt again)",
        avoided.state.position.x
    );
    assert!(
        (correct - pre_fix_bug).abs() > 0.5,
        "the fixture's own premise: the two formulas must actually disagree, or this test \
             cannot tell them apart"
    );
}

#[test]
fn avoid_refuses_to_push_a_mover_into_a_blocked_cell() {
    let grid = walled_grid();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    let pipeline = Pipeline::new(PipelineConfig {
        search: SearchMode::StraightLine,
        smoothing: SmoothingMode::Off,
        avoidance: AvoidanceMode::Separation,
    });
    let limits = MotionLimits {
        turn_pivot: 0.0,
        backs: false,
        desired_speed: f32::INFINITY,
        stops_dead: false,
        frame: 0.2,
        turn_while_moving: true,
        walk_distance: 0.0,
        turn_phases_at_speed: false,
        corner_radius: 0.0,
        max_speed: 40.0,
        acceleration: 60.0,
        braking: 60.0,
        turn_rate: 8.0,
        arrival_radius: 3.0,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: crate::following::TurnSlowdown::PROPORTIONAL,
    };
    let dt = 1.0 / 30.0;
    let origin = Vec2::new(99.9, 55.0);
    let state = MoverState {
        ramp: Default::default(),
        position: origin,
        facing: Facing::EAST,
        speed: 0.0,
        waypoint: 0,
    };
    let route = Route::to_point(origin);
    let motion = pipeline.follow(&context, state, &route, &limits, dt);
    assert_eq!(motion.status, MotionStatus::Arrived);

    let neighbour = Neighbour {
        id: MoverId(9),
        position: Vec2::new(origin.x - 6.0, origin.y),
        velocity: Vec2::ZERO,
        radius: 5.0,
    };
    let agent = Agent {
        id: MoverId(1),
        position: motion.state.position,
        radius: 5.0,
    };
    let mut scratch = Vec::new();
    let adjusted = SeparationAvoidance::default().adjust(
        &agent,
        motion.velocity,
        &OneNeighbour(neighbour),
        &mut scratch,
    );
    assert!(
        adjusted.x > 0.0,
        "the fixture's own premise: the push must point east, into the wall: {adjusted:?}"
    );

    let mut scratch = Vec::new();
    let avoided = pipeline.avoid(
        &context,
        &agent,
        origin,
        motion,
        &OneNeighbour(neighbour),
        &mut scratch,
        dt,
    );
    assert_eq!(
        avoided.status,
        MotionStatus::Blocked,
        "a push into a blocked cell must refuse the step, not cross into it"
    );
    assert_eq!(
        avoided.state.position, origin,
        "a refused push must leave the mover exactly where it was"
    );
}

#[test]
fn the_configuration_prints_in_a_form_that_pastes_into_a_report() {
    assert_eq!(
        PipelineConfig::straight_line().to_string(),
        "search=straight-line smoothing=off avoidance=off"
    );
    assert_eq!(
        PipelineConfig::default().to_string(),
        "search=a* smoothing=string-pull avoidance=off"
    );
}
