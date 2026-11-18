#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::time::Instant;

use mint_movement::*;

const WALL: CellFlags = CellFlags::from_bits(1);
const MOVER: MoverClass = MoverClass::new(WALL);
const TICK: f32 = 1.0 / 30.0;
const BUDGET_MS: f64 = 11.0;

fn limits() -> MotionLimits {
    MotionLimits {
        turn_pivot: 0.0,
        backs: false,
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
        turn_slowdown: mint_movement::TurnSlowdown::PROPORTIONAL,
    }
}

fn build_grid(width: u16, height: u16) -> (CostGrid, u32) {
    let mut grid = CostGrid::new(GridExtent::new(width, height).unwrap());
    let mut blocked = 0u32;
    let mut y = 12;
    while y < height.saturating_sub(12) {
        let mut x = 12;
        while x < width.saturating_sub(12) {
            for dy in 0..8u16 {
                for dx in 0..8u16 {
                    let cell = Cell::new(x + dx, y + dy);
                    grid.set_flags(cell, WALL).unwrap();
                    blocked += 1;
                }
            }
            x += 24;
        }
        y += 24;
    }
    (grid, blocked)
}

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "timing harness; run with --release -- --ignored --nocapture"]
fn movement_at_the_design_target() {
    let (width, height) = (512u16, 512u16);
    let (grid, blocked) = build_grid(width, height);
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let context = MoveContext {
        grid: &grid,
        geometry,
        mover: MOVER,
        layer: 0,
    };
    println!(
        "grid={width}x{height} cells={} blocked={blocked} ({:.1}%)",
        grid.extent().cell_count(),
        f64::from(blocked) * 100.0 / f64::from(grid.extent().cell_count())
    );
    println!("budget={BUDGET_MS:.1} ms per tick for all five stages");

    let mut routes: Vec<Route> = Vec::new();
    let mut expanded_total = 0u64;
    let mut cells_total = 0u64;
    let mut search_ms = 0.0f64;
    let mut smooth_ms = 0.0f64;
    let mut search = AStar::octile();
    let crossings = 16u16;
    for i in 0..crossings {
        let lane = 4 + i.saturating_mul(30);
        let request = PathRequest {
            corner_radius: 0.0,
            departure: None,
            start: Cell::new(2, lane),
            goal: Cell::new(width - 3, lane.saturating_add(9)),
            mover: MOVER,
            goal_id: GoalId(u64::from(i)),
            origin: None,
        };
        let start = Instant::now();
        let result = search
            .find(&grid, &request, &mut NodeBudget::new(400_000))
            .unwrap();
        search_ms += millis(start);
        let SearchOutcome::Found(path) = result.outcome else {
            panic!("crossing {i} did not plan: {:?}", result.outcome);
        };
        expanded_total += u64::from(result.expanded);
        cells_total += path.cells.len() as u64;
        let start = Instant::now();
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        smooth_ms += millis(start);
        routes.push(Route::from_path(&pulled, geometry));
    }
    let searches = f64::from(crossings);
    println!(
        "search  a*: {crossings} full-map crossings, {:.3} ms/search, \
         {:.0} expansions and {:.0} cells per path, {:.1} searches per {BUDGET_MS:.0} ms",
        search_ms / searches,
        expanded_total as f64 / searches,
        cells_total as f64 / searches,
        BUDGET_MS / (search_ms / searches)
    );
    println!(
        "smooth  string-pull: {:.3} ms/path, {:.1}x the search it follows",
        smooth_ms / searches,
        smooth_ms / search_ms
    );

    for movers in [100_000usize, 150_000usize] {
        for config in [PipelineConfig::default(), PipelineConfig::straight_line()] {
            let pipeline = Pipeline::new(config);
            let mut states: Vec<(MoverState, Route)> = Vec::with_capacity(movers);
            for i in 0..movers {
                let route = routes
                    .get(i % routes.len())
                    .cloned()
                    .unwrap_or_else(|| Route::to_point(Vec2::new(100.0, 100.0)));
                let origin = route.points.first().copied().unwrap_or(Vec2::ZERO);
                let jitter = (i % 17) as f32;
                states.push((
                    MoverState::new(Vec2::new(origin.x + jitter, origin.y), Facing::EAST),
                    route,
                ));
            }

            let start = Instant::now();
            let mut moving = 0u32;
            for (state, route) in &mut states {
                let motion = pipeline.follow(&context, *state, route, &limits(), TICK);
                *state = motion.state;
                if motion.status == MotionStatus::Moving {
                    moving += 1;
                }
            }
            let follow_ms = millis(start);

            let mut scratch: Vec<Neighbour> = Vec::new();
            let start = Instant::now();
            for (index, (state, route)) in states.iter().enumerate() {
                let motion = pipeline.follow(&context, *state, route, &limits(), TICK);
                let agent = Agent {
                    id: MoverId(index as u32),
                    position: motion.state.position,
                    radius: 5.0,
                };
                let _ = pipeline.avoid(
                    &context,
                    &agent,
                    state.position,
                    motion,
                    &NoNeighbours,
                    &mut scratch,
                    TICK,
                );
            }
            let with_avoid_ms = millis(start);

            println!(
                "follow  {movers:>6} movers [{config}]: {follow_ms:>7.2} ms/tick \
                 ({:>4.2}x budget); with the avoidance seam {with_avoid_ms:>7.2} ms; \
                 {moving} still moving",
                follow_ms / BUDGET_MS
            );
        }
    }

    let samples = 100_000usize;
    let start = Instant::now();
    let mut sink = 0.0f32;
    for i in 0..samples {
        let direction = Vec2::new((i % 211) as f32 - 105.0, (i % 37) as f32 - 18.0);
        let facing = Facing::from_vector(direction).unwrap_or(Facing::EAST);
        sink += facing.to_vector().x;
    }
    let trig_ms = millis(start);
    println!("trig    {samples} atan2+sin_cos pairs: {trig_ms:.2} ms (sink {sink:.3})");

    assert!(!routes.is_empty());
}
