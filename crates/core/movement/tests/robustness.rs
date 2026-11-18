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

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, limit: u32) -> u32 {
        if limit == 0 {
            0
        } else {
            self.next() % limit
        }
    }
}

fn recompute(grid: &CostGrid, cells: &[Cell]) -> Option<u32> {
    let mut cost = 0u32;
    for pair in cells.windows(2) {
        let (a, b) = (pair.first().copied()?, pair.last().copied()?);
        let dx = a.x.abs_diff(b.x);
        let dy = a.y.abs_diff(b.y);
        if a.layer != b.layer || dx > 1 || dy > 1 || (dx == 0 && dy == 0) {
            return None;
        }
        if !grid.is_passable(b, MOVER) {
            return None;
        }
        let diagonal = dx == 1 && dy == 1;
        if diagonal {
            let x_open = grid.is_passable(Cell::on_layer(b.x, a.y, a.layer), MOVER);
            let y_open = grid.is_passable(Cell::on_layer(a.x, b.y, a.layer), MOVER);
            if !x_open && !y_open {
                return None;
            }
        }
        let step = if diagonal {
            COST_DIAGONAL
        } else {
            COST_ORTHOGONAL
        };
        cost = cost.saturating_add(step).saturating_add(grid.extra_cost(b));
    }
    Some(cost)
}

#[test]
fn degenerate_grids_and_requests_are_answered_rather_than_survived() {
    let one_cell = CostGrid::new(GridExtent::new(1, 1).unwrap());
    let mut all_blocked = CostGrid::new(GridExtent::new(3, 3).unwrap());
    for y in 0..3 {
        for x in 0..3 {
            all_blocked.set_flags(Cell::new(x, y), WALL).unwrap();
        }
    }
    let strip = CostGrid::new(GridExtent::new(1, 64).unwrap());
    let mut search = AStar::octile();

    let cases: [(&CostGrid, Cell, Cell, u32, &str); 5] = [
        (&one_cell, Cell::new(0, 0), Cell::new(0, 0), 4000, "found"),
        (
            &all_blocked,
            Cell::new(0, 0),
            Cell::new(2, 2),
            4000,
            "unreachable",
        ),
        (&strip, Cell::new(0, 0), Cell::new(0, 63), 4000, "found"),
        (&strip, Cell::new(0, 0), Cell::new(0, 63), 0, "budget"),
        (&one_cell, Cell::new(0, 0), Cell::new(9, 9), 4000, "error"),
    ];

    for (grid, start, goal, budget, expected) in cases {
        let request = PathRequest {
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
            start,
            goal,
            mover: MOVER,
            goal_id: GoalId(0),
            origin: None,
        };
        let result = search.find(grid, &request, &mut NodeBudget::new(budget));
        let actual = match &result {
            Err(_) => "error",
            Ok(r) => match r.outcome {
                SearchOutcome::Found(_) => "found",
                SearchOutcome::Unreachable => "unreachable",
                SearchOutcome::BudgetExhausted => "budget",
                _ => "unknown outcome",
            },
        };
        assert_eq!(actual, expected, "{start:?} -> {goal:?} budget {budget}");
    }
}

#[test]
fn a_blocked_start_or_goal_is_reported_unreachable_not_searched_for() {
    let mut grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
    grid.set_flags(Cell::new(7, 7), WALL).unwrap();
    let mut search = AStar::octile();
    let result = search
        .find(
            &grid,
            &PathRequest {
                corner_radius: 0.0,
                departure: None,
                approach: None,
                prune: false,
                final_goal: true,
                start: Cell::new(0, 0),
                goal: Cell::new(7, 7),
                mover: MOVER,
                goal_id: GoalId(0),
                origin: None,
            },
            &mut NodeBudget::unlimited(),
        )
        .unwrap();
    assert_eq!(result.outcome, SearchOutcome::Unreachable);
    assert_eq!(
        result.expanded, 0,
        "a blocked goal should cost no expansions"
    );
}

#[test]
fn every_path_a_random_sweep_produces_is_a_walk_the_grid_permits() {
    let mut rng = Lcg(0x5EED_1234_ABCD_0001);
    let mut search = AStar::octile();
    let geometry = CellGeometry::new(10.0, Vec2::ZERO).unwrap();
    let mut found = 0u32;
    let mut smoothed_shorter = 0u32;

    for case in 0..400u32 {
        let width = 4 + u16::try_from(rng.below(20)).unwrap();
        let height = 4 + u16::try_from(rng.below(20)).unwrap();
        let mut grid = CostGrid::new(GridExtent::new(width, height).unwrap());
        let blocked = rng.below(u32::from(width) * u32::from(height) / 2);
        for _ in 0..blocked {
            let cell = Cell::new(
                u16::try_from(rng.below(u32::from(width))).unwrap(),
                u16::try_from(rng.below(u32::from(height))).unwrap(),
            );
            grid.set_flags(cell, WALL).unwrap();
            grid.set_extra_cost(cell, u16::try_from(rng.below(64)).unwrap())
                .unwrap();
        }
        let request = PathRequest {
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
            start: Cell::new(
                u16::try_from(rng.below(u32::from(width))).unwrap(),
                u16::try_from(rng.below(u32::from(height))).unwrap(),
            ),
            goal: Cell::new(
                u16::try_from(rng.below(u32::from(width))).unwrap(),
                u16::try_from(rng.below(u32::from(height))).unwrap(),
            ),
            mover: MOVER,
            goal_id: GoalId(u64::from(case)),
            origin: None,
        };

        let result = search
            .find(&grid, &request, &mut NodeBudget::new(50_000))
            .unwrap();
        let SearchOutcome::Found(path) = result.outcome else {
            continue;
        };
        found = found.saturating_add(1);
        assert_eq!(path.cells.first(), Some(&request.start), "case {case}");
        assert_eq!(path.cells.last(), Some(&request.goal), "case {case}");
        assert_eq!(
            recompute(&grid, &path.cells),
            Some(path.cost),
            "case {case}: reported cost disagrees with the grid"
        );

        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert_eq!(pulled.cells.first(), Some(&request.start), "case {case}");
        assert_eq!(pulled.cells.last(), Some(&request.goal), "case {case}");
        if pulled.cells.len() < path.cells.len() {
            smoothed_shorter = smoothed_shorter.saturating_add(1);
        }
        for pair in pulled.cells.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.last()) else {
                continue;
            };
            assert!(
                mint_movement::smoothing::line_is_passable(&grid, MOVER, *a, *b),
                "case {case}: smoothed segment {a:?} -> {b:?} is not walkable"
            );
        }
        let route = Route::from_path(&pulled, geometry);
        assert!(route
            .points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    assert!(found > 100, "only {found} of 400 cases found a path at all");
    assert!(
        smoothed_shorter > found / 4,
        "string-pull shortened only {smoothed_shorter} of {found} paths"
    );
}
