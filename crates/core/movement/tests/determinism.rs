#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use mint_movement::*;

const WALL: CellFlags = CellFlags::from_bits(1);
const MOVER: MoverClass = MoverClass::new(WALL);

fn obstacles() -> Vec<Cell> {
    let mut cells = Vec::new();
    for y in 0..28u16 {
        if y != 9 && y != 21 {
            cells.push(Cell::new(12, y));
        }
    }
    for x in 4..26u16 {
        if x != 7 && x != 19 {
            cells.push(Cell::new(x, 16));
        }
    }
    cells
}

fn grid_from(order: &[Cell]) -> CostGrid {
    let mut grid = CostGrid::new(GridExtent::new(32, 32).unwrap());
    for cell in order {
        grid.set_flags(*cell, WALL).unwrap();
    }
    grid
}

fn plan(grid: &CostGrid) -> Plan {
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let mut pipeline = Pipeline::new(PipelineConfig::default());
    let request = PathRequest {
        corner_radius: 0.0,
        departure: None,
        start: Cell::new(1, 1),
        goal: Cell::new(30, 30),
        mover: MOVER,
        goal_id: GoalId(7),
        origin: None,
    };
    match pipeline
        .plan(grid, geometry, &request, &mut NodeBudget::unlimited())
        .unwrap()
    {
        PlanOutcome::Ready(plan) => plan,
        other => panic!("expected a plan, got {other:?}"),
    }
}

#[test]
fn the_same_map_built_in_a_different_order_gives_the_identical_path() {
    let forwards = obstacles();
    let mut backwards = forwards.clone();
    backwards.reverse();
    let mut shuffled = forwards.clone();
    shuffled.sort_by_key(|c| (u32::from(c.x).wrapping_mul(7) % 11, c.y, c.x));

    let expected = plan(&grid_from(&forwards));
    for order in [backwards, shuffled] {
        let actual = plan(&grid_from(&order));
        assert_eq!(actual.cells, expected.cells);
        assert_eq!(actual.expanded, expected.expanded);
    }
}

#[test]
fn a_planned_and_followed_run_repeats_bit_for_bit() {
    let grid = grid_from(&obstacles());
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
        max_speed: 35.0,
        acceleration: 70.0,
        braking: 70.0,
        turn_rate: 7.0,
        arrival_radius: 2.5,
        turn_threshold: 0.0,
        acc_dec_trigger: 0.5,
        min_speed: 0.0,
        turn_circle: None,
        turn_slowdown: mint_movement::TurnSlowdown::PROPORTIONAL,
    };
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };

    let trace = || -> Vec<(u32, u32, u32)> {
        let plan = plan(&grid);
        let pipeline = Pipeline::new(PipelineConfig::default());
        let mut state = MoverState::new(geometry.centre_of(Cell::new(1, 1)), Facing::EAST);
        let mut samples = Vec::new();
        for _ in 0..400 {
            let motion = pipeline.follow(&context, state, &plan.route, &limits, 1.0 / 30.0);
            state = motion.state;
            samples.push((
                state.position.x.to_bits(),
                state.position.y.to_bits(),
                state.facing.radians().to_bits(),
            ));
            if motion.status != MotionStatus::Moving {
                break;
            }
        }
        samples
    };

    assert_eq!(trace(), trace());
}
