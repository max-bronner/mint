use crate::grid::{Cell, MoverClass, Passability};
use crate::search::CellPath;

pub trait Smoothing {
    fn smooth(&self, grid: &dyn Passability, mover: MoverClass, path: &CellPath) -> CellPath;

    fn smooth_toward(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        _path_end: Cell,
    ) -> CellPath {
        self.smooth(grid, mover, path)
    }

    fn smooth_by_portal(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        path_end: Cell,
        _edge: PortalEdge,
    ) -> CellPath {
        self.smooth_toward(grid, mover, path, path_end)
    }

    fn smooth_backward(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        _heading: (f32, f32),
        _gated: bool,
    ) -> CellPath {
        self.smooth(grid, mover, path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PortalEdge {
    pub into_portal: bool,
    pub from_portal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmoothingMode {
    Off,
    #[default]
    StringPull,
}

impl SmoothingMode {
    pub const ALL: [Self; 2] = [Self::Off, Self::StringPull];

    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::StringPull,
            Self::StringPull => Self::Off,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "smoothing=off",
            Self::StringPull => "smoothing=string-pull",
        }
    }

    #[must_use]
    pub fn build(self) -> Box<dyn Smoothing> {
        match self {
            Self::Off => Box::new(NoSmoothing),
            Self::StringPull => Box::new(StringPull),
        }
    }
}

impl core::fmt::Display for SmoothingMode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoSmoothing;

impl Smoothing for NoSmoothing {
    fn smooth(&self, _grid: &dyn Passability, _mover: MoverClass, path: &CellPath) -> CellPath {
        path.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StringPull;

impl Smoothing for StringPull {
    fn smooth(&self, grid: &dyn Passability, mover: MoverClass, path: &CellPath) -> CellPath {
        match path.cells.last() {
            Some(end) => self.smooth_toward(grid, mover, path, *end),
            None => path.clone(),
        }
    }

    fn smooth_toward(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        path_end: Cell,
    ) -> CellPath {
        self.smooth_by_portal(grid, mover, path, path_end, PortalEdge::default())
    }

    fn smooth_by_portal(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        path_end: Cell,
        edge: PortalEdge,
    ) -> CellPath {
        let widened = widen_corner_cuts(grid, mover, &path.cells);
        let last = Some(path_end);
        let count = widened.len();
        let cells = pull(grid, &widened, |(anchor_index, anchor), (index, next)| {
            (edge.into_portal && index.saturating_add(PORTAL_LOOKAHEAD_CELLS) >= count)
                || (edge.from_portal && anchor_index == 0 && (1..=2).contains(&index))
                || line_is_passable(grid, mover, anchor, next)
                || (mover.straight_tail > 0
                    && last.is_some_and(|last| {
                        u32::from(next.x.abs_diff(last.x))
                            .saturating_add(u32::from(next.y.abs_diff(last.y)))
                            < u32::from(mover.straight_tail)
                    })
                    && walk_line(anchor, next, &|cell| grid.tail_clear(cell, mover)))
        });
        CellPath {
            cells,
            cost: path.cost,
            hops: Vec::new(),
        }
    }

    fn smooth_backward(
        &self,
        grid: &dyn Passability,
        mover: MoverClass,
        path: &CellPath,
        heading: (f32, f32),
        gated: bool,
    ) -> CellPath {
        let mut widened = widen_corner_cuts(grid, mover, &path.cells);
        widened.reverse();
        let mut cells = pull_refused(
            grid,
            &widened,
            |(anchor_index, anchor), (index, candidate)| {
                line_is_passable(grid, mover, anchor, candidate)
                    && (!gated
                        || anchor_index != 0
                        || index <= BACKWARD_FREE_NODES
                        || leg_follows_heading(anchor, candidate, heading))
            },
        );
        cells.reverse();
        CellPath {
            cells,
            cost: path.cost,
            hops: Vec::new(),
        }
    }
}

const PORTAL_LOOKAHEAD_CELLS: usize = 3;

const BACKWARD_FREE_NODES: usize = 5;

const BACKWARD_ALIGNMENT: f32 = 0.9;

const HEADING_LINKS: usize = 10;

fn leg_follows_heading(from: Cell, to: Cell, heading: (f32, f32)) -> bool {
    let (dx, dy) = (
        f32::from(to.x) - f32::from(from.x),
        f32::from(to.y) - f32::from(from.y),
    );
    let length = (dx * dx + dy * dy).sqrt();
    length <= f32::EPSILON
        || ((dx * heading.0 + dy * heading.1) / length).abs() >= BACKWARD_ALIGNMENT
}

#[must_use]
pub fn final_heading(cells: &[Cell], fallback: (f32, f32)) -> (f32, f32) {
    let mut sum = (0.0_f32, 0.0_f32);
    let mut weight = 1.0_f32;
    for pair in cells.windows(2).rev().take(HEADING_LINKS) {
        let (Some(a), Some(b)) = (pair.first(), pair.last()) else {
            continue;
        };
        let (dx, dy) = (
            f32::from(b.x) - f32::from(a.x),
            f32::from(b.y) - f32::from(a.y),
        );
        let length = (dx * dx + dy * dy).sqrt();
        if length > f32::EPSILON {
            sum = (sum.0 + weight * dx / length, sum.1 + weight * dy / length);
        }
        weight *= 0.5;
    }
    let length = (sum.0 * sum.0 + sum.1 * sum.1).sqrt();
    if length <= f32::EPSILON {
        fallback
    } else {
        (sum.0 / length, sum.1 / length)
    }
}

fn pull(
    grid: &dyn Passability,
    cells: &[Cell],
    sees: impl Fn((usize, Cell), (usize, Cell)) -> bool,
) -> Vec<Cell> {
    let count = cells.len();
    if count <= 2 {
        return cells.to_vec();
    }
    let Some(first) = cells.first().copied() else {
        return Vec::new();
    };
    let mut kept = Vec::with_capacity(count);
    kept.push(first);
    let mut anchor = (0usize, first);
    for index in 1..count.saturating_sub(1) {
        let next_index = index.saturating_add(1);
        let (Some(current), Some(next)) =
            (cells.get(index).copied(), cells.get(next_index).copied())
        else {
            break;
        };
        if grid.portal_at(current).is_some() || !sees(anchor, (next_index, next)) {
            kept.push(current);
            anchor = (index, current);
        }
    }
    if let Some(last) = cells.last().copied() {
        kept.push(last);
    }
    kept
}

fn pull_refused(
    grid: &dyn Passability,
    cells: &[Cell],
    sees: impl Fn((usize, Cell), (usize, Cell)) -> bool,
) -> Vec<Cell> {
    let count = cells.len();
    if count <= 2 {
        return cells.to_vec();
    }
    let Some(first) = cells.first().copied() else {
        return Vec::new();
    };
    let mut kept = Vec::with_capacity(count);
    kept.push(first);
    let mut anchor = (0usize, first);
    for index in 1..count.saturating_sub(1) {
        let Some(candidate) = cells.get(index).copied() else {
            break;
        };
        if grid.portal_at(candidate).is_some() || !sees(anchor, (index, candidate)) {
            kept.push(candidate);
            anchor = (index, candidate);
        }
    }
    if let Some(last) = cells.last().copied() {
        kept.push(last);
    }
    kept
}

fn widen_corner_cuts(grid: &dyn Passability, mover: MoverClass, cells: &[Cell]) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::with_capacity(cells.len().saturating_mul(2));
    for pair in cells.windows(2) {
        let (Some(a), Some(b)) = (pair.first().copied(), pair.last().copied()) else {
            continue;
        };
        out.push(a);
        if a.layer != b.layer || a.x.abs_diff(b.x) != 1 || a.y.abs_diff(b.y) != 1 {
            continue;
        }
        let along_x = Cell::on_layer(b.x, a.y, a.layer);
        let along_y = Cell::on_layer(a.x, b.y, a.layer);
        let x_open = grid.is_passable(along_x, mover);
        let y_open = grid.is_passable(along_y, mover);
        if x_open && y_open {
            continue;
        }
        if x_open {
            out.push(along_x);
        } else if y_open {
            out.push(along_y);
        }
    }
    if let Some(last) = cells.last().copied() {
        out.push(last);
    }
    out
}

#[must_use]
pub fn line_is_passable(grid: &dyn Passability, mover: MoverClass, from: Cell, to: Cell) -> bool {
    walk_line(from, to, &|cell| grid.line_clear(cell, mover))
}

#[must_use]
pub fn straight_is_passable(
    grid: &dyn Passability,
    mover: MoverClass,
    from: Cell,
    to: Cell,
) -> bool {
    walk_line(from, to, &|cell| grid.straight_clear(cell, mover))
}

fn walk_line(from: Cell, to: Cell, clear: &dyn Fn(Cell) -> bool) -> bool {
    if from.layer != to.layer {
        return false;
    }
    let mut x = i32::from(from.x);
    let mut y = i32::from(from.y);
    let target_x = i32::from(to.x);
    let target_y = i32::from(to.y);
    let dx = target_x.saturating_sub(x).abs();
    let dy = target_y.saturating_sub(y).abs();
    let step_x = if target_x > x { 1 } else { -1 };
    let step_y = if target_y > y { 1 } else { -1 };
    let mut error = dx.saturating_sub(dy);
    let cap = dx.saturating_add(dy).saturating_add(1);

    let at = |x: i32, y: i32| -> bool {
        let (Ok(cx), Ok(cy)) = (u16::try_from(x), u16::try_from(y)) else {
            return false;
        };
        clear(Cell::on_layer(cx, cy, from.layer))
    };

    if !at(x, y) {
        return false;
    }
    for _ in 0..cap {
        if x == target_x && y == target_y {
            return true;
        }
        let doubled = error.saturating_mul(2);
        let steps_x = doubled > dy.saturating_neg();
        let steps_y = doubled < dx;
        if steps_x
            && steps_y
            && !(at(x.saturating_add(step_x), y) && at(x, y.saturating_add(step_y)))
        {
            return false;
        }
        if steps_x {
            error = error.saturating_sub(dy);
            x = x.saturating_add(step_x);
        }
        if steps_y {
            error = error.saturating_add(dx);
            y = y.saturating_add(step_y);
        }
        if !at(x, y) {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{CellFlags, CostGrid, GridExtent, Portal};
    use crate::search::{AStar, GoalId, NodeBudget, PathRequest, Search, SearchOutcome};

    const WALL: CellFlags = CellFlags::from_bits(1);
    const MOVER: MoverClass = MoverClass::new(WALL);

    fn path_over(grid: &CostGrid, start: Cell, goal: Cell) -> CellPath {
        let mut search = AStar::octile();
        let result = search
            .find(
                grid,
                &PathRequest {
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
                },
                &mut NodeBudget::unlimited(),
            )
            .unwrap();
        match result.outcome {
            SearchOutcome::Found(path) => path,
            other => panic!("expected a path, got {other:?}"),
        }
    }

    #[test]
    fn a_straight_run_across_open_ground_collapses_to_its_endpoints() {
        let grid = CostGrid::new(GridExtent::new(20, 4).unwrap());
        let path = path_over(&grid, Cell::new(0, 1), Cell::new(19, 1));
        assert_eq!(path.cells.len(), 20);
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert_eq!(pulled.cells, vec![Cell::new(0, 1), Cell::new(19, 1)]);
    }

    #[test]
    fn every_segment_of_a_pulled_path_is_still_walkable() {
        let mut grid = CostGrid::new(GridExtent::new(24, 24).unwrap());
        for y in 0..20 {
            grid.set_flags(Cell::new(8, y), WALL).unwrap();
        }
        for y in 4..24 {
            grid.set_flags(Cell::new(16, y), WALL).unwrap();
        }
        let path = path_over(&grid, Cell::new(1, 1), Cell::new(22, 22));
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert!(pulled.cells.len() < path.cells.len());
        for pair in pulled.cells.windows(2) {
            let (Some(a), Some(b)) = (pair.first(), pair.last()) else {
                continue;
            };
            assert!(
                line_is_passable(&grid, MOVER, *a, *b),
                "segment {a:?} -> {b:?} crosses a wall"
            );
        }
        assert_eq!(pulled.cells.first(), path.cells.first());
        assert_eq!(pulled.cells.last(), path.cells.last());
    }

    #[test]
    fn a_line_that_only_touches_a_wall_at_a_corner_is_refused() {
        let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        grid.set_flags(Cell::new(1, 0), WALL).unwrap();
        grid.set_flags(Cell::new(0, 1), WALL).unwrap();
        assert!(!line_is_passable(
            &grid,
            MOVER,
            Cell::new(0, 0),
            Cell::new(2, 2)
        ));
    }

    #[test]
    fn a_corner_cut_diagonal_gains_the_orthogonal_step_beside_it() {
        let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        grid.set_flags(Cell::new(1, 0), WALL).unwrap();
        let path = CellPath {
            cells: vec![Cell::new(0, 0), Cell::new(1, 1), Cell::new(2, 2)],
            cost: 28,
            hops: Vec::new(),
        };
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert_eq!(
            pulled.cells,
            vec![Cell::new(0, 0), Cell::new(0, 1), Cell::new(2, 2)]
        );
    }

    #[test]
    fn the_null_smoother_returns_the_search_path_untouched() {
        let grid = CostGrid::new(GridExtent::new(12, 12).unwrap());
        let path = path_over(&grid, Cell::new(0, 0), Cell::new(11, 7));
        assert_eq!(NoSmoothing.smooth(&grid, MOVER, &path), path);
    }

    struct OnePortal {
        grid: CostGrid,
        cell: Cell,
        tag: Portal,
    }

    impl Passability for OnePortal {
        fn extent(&self) -> GridExtent {
            self.grid.extent()
        }

        fn flags(&self, cell: Cell) -> CellFlags {
            self.grid.flags(cell)
        }

        fn extra_cost(&self, cell: Cell) -> u32 {
            self.grid.extra_cost(cell)
        }

        fn portal_at(&self, cell: Cell) -> Option<Portal> {
            (cell == self.cell).then_some(self.tag)
        }
    }

    #[test]
    fn a_straight_run_through_a_portal_keeps_the_tagged_cell() {
        let grid = OnePortal {
            grid: CostGrid::new(GridExtent::new(20, 1).unwrap()),
            cell: Cell::new(10, 0),
            tag: Portal::new(1, 2),
        };
        let path = CellPath {
            cells: (0..20).map(|x| Cell::new(x, 0)).collect(),
            cost: 19,
            hops: Vec::new(),
        };
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert!(
            pulled.cells.contains(&Cell::new(10, 0)),
            "the portal cell must survive smoothing: {:?}",
            pulled.cells
        );
    }

    const PINCH: CellFlags = CellFlags::from_bits(2);

    fn row(from: u16, to: u16) -> CellPath {
        CellPath {
            cells: (from..=to).map(|x| Cell::new(x, 1)).collect(),
            cost: 0,
            hops: Vec::new(),
        }
    }

    #[test]
    fn a_leg_never_straightens_across_a_barred_cell() {
        let mut grid = CostGrid::new(GridExtent::new(20, 3).unwrap());
        grid.set_flags(Cell::new(10, 1), PINCH).unwrap();
        let path = row(0, 19);
        let barred = StringPull.smooth(&grid, MOVER.line_barring(PINCH), &path);
        assert!(
            barred.cells.contains(&Cell::new(10, 1)),
            "{:?}",
            barred.cells
        );
        let free = StringPull.smooth(&grid, MOVER, &path);
        assert_eq!(free.cells, vec![Cell::new(0, 1), Cell::new(19, 1)]);
    }

    struct OpenTail(CostGrid);

    impl Passability for OpenTail {
        fn extent(&self) -> GridExtent {
            self.0.extent()
        }
        fn flags(&self, cell: Cell) -> CellFlags {
            self.0.flags(cell)
        }
        fn extra_cost(&self, cell: Cell) -> u32 {
            self.0.extra_cost(cell)
        }
        fn tail_clear(&self, _cell: Cell, _mover: MoverClass) -> bool {
            true
        }
    }

    #[test]
    fn a_tailed_mover_cuts_straight_into_its_goal_over_barred_cells() {
        let mut grid = CostGrid::new(GridExtent::new(14, 3).unwrap());
        for x in 10..=11 {
            grid.set_flags(Cell::new(x, 1), PINCH).unwrap();
        }
        let grid = OpenTail(grid);
        let path = row(0, 12);
        let plain = MOVER.line_barring(PINCH);
        let kept = StringPull.smooth(&grid, plain, &path);
        assert!(kept.cells.len() > 2, "{:?}", kept.cells);
        let machine = StringPull.smooth(&grid, plain.straight_tailed(4), &path);
        assert_eq!(machine.cells, vec![Cell::new(0, 1), Cell::new(12, 1)]);
        let mut far = CostGrid::new(GridExtent::new(14, 3).unwrap());
        far.set_flags(Cell::new(3, 1), PINCH).unwrap();
        let far = OpenTail(far);
        let away = StringPull.smooth(&far, plain.straight_tailed(4), &path);
        assert!(away.cells.contains(&Cell::new(3, 1)), "{:?}", away.cells);
    }

    #[test]
    fn the_tail_allowance_is_measured_to_the_paths_last_node() {
        let mut grid = CostGrid::new(GridExtent::new(40, 3).unwrap());
        for x in 10..=11 {
            grid.set_flags(Cell::new(x, 1), PINCH).unwrap();
        }
        let grid = OpenTail(grid);
        let stretch = row(0, 12);
        let tailed = MOVER.line_barring(PINCH).straight_tailed(4);
        let near = StringPull.smooth_toward(&grid, tailed, &stretch, Cell::new(12, 1));
        assert_eq!(near.cells, vec![Cell::new(0, 1), Cell::new(12, 1)]);
        let far = StringPull.smooth_toward(&grid, tailed, &stretch, Cell::new(39, 1));
        assert!(far.cells.len() > 2, "{:?}", far.cells);
    }

    fn bent_attack_path() -> (Vec<Cell>, CellPath) {
        let cells: Vec<Cell> = (0..=3)
            .map(|y| Cell::new(15, y))
            .chain((15..=20).map(|x| Cell::new(x, 6)))
            .collect();
        let path = CellPath {
            cells: cells.clone(),
            cost: 0,
            hops: Vec::new(),
        };
        (cells, path)
    }

    #[test]
    fn a_refused_candidate_is_kept_as_the_next_anchor_not_the_last_node_that_passed() {
        let grid = CostGrid::new(GridExtent::new(24, 12).unwrap());
        let (cells, path) = bent_attack_path();
        let heading = final_heading(&cells, (0.0, 1.0));
        assert!(heading.0 > 0.99, "{heading:?}");
        let backward = StringPull.smooth_backward(&grid, MOVER, &path, heading, true);
        assert_eq!(
            backward.cells,
            vec![Cell::new(15, 0), Cell::new(15, 3), Cell::new(20, 6)]
        );
    }

    #[test]
    fn a_backward_stretch_off_the_path_end_is_not_held_to_the_heading() {
        let grid = CostGrid::new(GridExtent::new(24, 12).unwrap());
        let (_, path) = bent_attack_path();
        let heading = (1.0, 0.0);
        let gated = StringPull.smooth_backward(&grid, MOVER, &path, heading, true);
        let ungated = StringPull.smooth_backward(&grid, MOVER, &path, heading, false);
        assert_eq!(gated.cells.len(), 3);
        assert_eq!(ungated.cells.len(), 2);
    }
}
