use crate::grid::{Cell, MoverClass, Passability};
use crate::search::CellPath;

pub trait Smoothing {
    fn smooth(&self, grid: &dyn Passability, mover: MoverClass, path: &CellPath) -> CellPath;
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
        let widened = widen_corner_cuts(grid, mover, &path.cells);
        let count = widened.len();
        if count <= 2 {
            return CellPath {
                cells: widened,
                cost: path.cost,
            };
        }
        let mut cells = Vec::with_capacity(count);
        let Some(first) = widened.first().copied() else {
            return path.clone();
        };
        cells.push(first);
        let mut anchor = first;
        for index in 1..count.saturating_sub(1) {
            let (Some(current), Some(next)) = (
                widened.get(index).copied(),
                widened.get(index.saturating_add(1)).copied(),
            ) else {
                break;
            };
            if grid.portal_at(current).is_some() || !line_is_passable(grid, mover, anchor, next) {
                cells.push(current);
                anchor = current;
            }
        }
        if let Some(last) = widened.last().copied() {
            cells.push(last);
        }
        CellPath {
            cells,
            cost: path.cost,
        }
    }
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
        grid.line_clear(Cell::on_layer(cx, cy, from.layer), mover)
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
            tag: Portal { id: 1, kind: 2 },
        };
        let path = CellPath {
            cells: (0..20).map(|x| Cell::new(x, 0)).collect(),
            cost: 19,
        };
        let pulled = StringPull.smooth(&grid, MOVER, &path);
        assert!(
            pulled.cells.contains(&Cell::new(10, 0)),
            "the portal cell must survive smoothing: {:?}",
            pulled.cells
        );
    }
}
