use crate::grid::{Cell, GridExtent, MoverClass, Passability, PortalLink, PortalPass};
use crate::search::offset;

const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

const IMPASSABLE: u32 = u32::MAX;

#[derive(Debug, Clone)]
pub struct Reachability {
    extent: GridExtent,
    component: Vec<u32>,
    mover: MoverClass,
}

impl Reachability {
    #[must_use]
    pub fn build(passability: &dyn Passability, mover: MoverClass) -> Self {
        let extent = passability.extent();
        let count = usize::try_from(extent.cell_count()).unwrap_or(0);
        let mut component: Vec<u32> = vec![IMPASSABLE; count];
        let mut next_id: u32 = 0;
        let mut stack: Vec<u32> = Vec::new();

        for seed in 0..count {
            if component.get(seed).copied() != Some(IMPASSABLE) {
                continue;
            }
            let Ok(seed_index) = u32::try_from(seed) else {
                continue;
            };
            let Some(seed_cell) = extent.cell_at_index(seed_index) else {
                continue;
            };
            if !passability.is_passable(seed_cell, mover) {
                continue;
            }

            let id = next_id;
            next_id = next_id.saturating_add(1);
            if let Some(slot) = component.get_mut(seed) {
                *slot = id;
            }
            stack.push(seed_index);

            while let Some(node) = stack.pop() {
                let Some(cell) = extent.cell_at_index(node) else {
                    continue;
                };
                for delta in NEIGHBOURS {
                    let Some(neighbour) = offset(cell, delta, extent) else {
                        continue;
                    };
                    if !passability.is_passable(neighbour, mover) {
                        continue;
                    }
                    let Some(index) = extent.index(neighbour) else {
                        continue;
                    };
                    let Ok(at) = usize::try_from(index) else {
                        continue;
                    };
                    if component.get(at).copied() != Some(IMPASSABLE) {
                        continue;
                    }
                    if let Some(slot) = component.get_mut(at) {
                        *slot = id;
                    }
                    stack.push(index);
                }
            }
        }

        Self {
            extent,
            component,
            mover,
        }
    }

    #[must_use]
    pub const fn extent(&self) -> GridExtent {
        self.extent
    }

    #[must_use]
    pub const fn mover(&self) -> MoverClass {
        self.mover
    }

    #[must_use]
    pub fn same_component(&self, a: Cell, b: Cell) -> bool {
        let one = self.component_of(a);
        let other = self.component_of(b);
        match (one, other) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    #[must_use]
    pub fn joined_by(&self, a: Cell, b: Cell, links: &[PortalLink], pass: PortalPass) -> bool {
        let (Some(from), Some(to)) = (self.component_of(a), self.component_of(b)) else {
            return false;
        };
        let mut reached = vec![from];
        loop {
            let before = reached.len();
            for link in links.iter().filter(|link| pass.has(link.slot)) {
                let (Some(entry), Some(exit)) =
                    (self.component_of(link.entry), self.component_of(link.exit))
                else {
                    continue;
                };
                if reached.contains(&entry) && !reached.contains(&exit) {
                    reached.push(exit);
                }
            }
            if reached.contains(&to) {
                return true;
            }
            if reached.len() == before {
                return false;
            }
        }
    }

    fn component_of(&self, cell: Cell) -> Option<u32> {
        let index = self.extent.index(cell)?;
        let at = usize::try_from(index).ok()?;
        match self.component.get(at).copied() {
            Some(IMPASSABLE) | None => None,
            Some(id) => Some(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{CellFlags, CostGrid};

    const WALL: CellFlags = CellFlags::bit(0);

    fn mover() -> MoverClass {
        MoverClass::new(WALL)
    }

    #[test]
    fn two_cells_on_open_ground_are_the_same_component() {
        let grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
        let reach = Reachability::build(&grid, mover());
        assert!(reach.same_component(Cell::new(0, 0), Cell::new(7, 7)));
    }

    #[test]
    fn a_full_height_wall_with_no_gap_splits_the_grid_in_two() {
        let mut grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let reach = Reachability::build(&grid, mover());
        assert!(!reach.same_component(Cell::new(0, 0), Cell::new(7, 7)));
        assert!(reach.same_component(Cell::new(0, 0), Cell::new(3, 7)));
        assert!(reach.same_component(Cell::new(5, 0), Cell::new(7, 7)));
    }

    #[test]
    fn a_single_gap_in_the_wall_reconnects_both_sides() {
        let mut grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
        for y in 0..8 {
            if y == 4 {
                continue;
            }
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let reach = Reachability::build(&grid, mover());
        assert!(reach.same_component(Cell::new(0, 0), Cell::new(7, 7)));
    }

    #[test]
    fn an_impassable_cell_shares_no_component_with_anything() {
        let mut grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        grid.set_flags(Cell::new(2, 2), WALL).unwrap();
        let reach = Reachability::build(&grid, mover());
        assert!(!reach.same_component(Cell::new(2, 2), Cell::new(2, 2)));
        assert!(!reach.same_component(Cell::new(2, 2), Cell::new(0, 0)));
    }

    #[test]
    fn diagonal_adjacency_alone_is_enough_to_connect_two_cells() {
        let mut grid = CostGrid::new(GridExtent::new(3, 3).unwrap());
        grid.set_flags(Cell::new(1, 0), WALL).unwrap();
        grid.set_flags(Cell::new(0, 1), WALL).unwrap();
        let reach = Reachability::build(&grid, mover());
        assert!(reach.same_component(Cell::new(0, 0), Cell::new(1, 1)));
    }

    #[test]
    fn a_cell_outside_the_grid_is_never_reachable() {
        let grid = CostGrid::new(GridExtent::new(4, 4).unwrap());
        let reach = Reachability::build(&grid, mover());
        assert!(!reach.same_component(Cell::new(0, 0), Cell::new(40, 40)));
    }

    #[test]
    fn a_different_mover_class_sees_a_different_partition() {
        let mut grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let free = Reachability::build(&grid, MoverClass::new(CellFlags::NONE));
        assert!(free.same_component(Cell::new(0, 0), Cell::new(7, 7)));
    }

    #[test]
    fn a_portal_link_joins_two_regions_only_for_a_mover_admitted_to_it() {
        let mut grid = CostGrid::new(GridExtent::new(8, 8).unwrap());
        for y in 0..8 {
            grid.set_flags(Cell::new(4, y), WALL).unwrap();
        }
        let reach = Reachability::build(&grid, mover());
        let link = PortalLink {
            entry: Cell::new(3, 3),
            exit: Cell::new(5, 3),
            slot: 2,
            nodes: Vec::new(),
            after: None,
        };
        let (a, b) = (Cell::new(0, 0), Cell::new(7, 7));
        let links = [link];
        assert!(reach.joined_by(a, b, &links, PortalPass::NONE.with(2)));
        assert!(!reach.joined_by(a, b, &links, PortalPass::NONE));
        assert!(!reach.joined_by(a, b, &links, PortalPass::NONE.with(3)));
    }

    #[test]
    fn build_is_deterministic_across_repeated_runs() {
        let mut grid = CostGrid::new(GridExtent::new(16, 12).unwrap());
        for y in 3..9 {
            grid.set_flags(Cell::new(8, y), WALL).unwrap();
        }
        let a = Reachability::build(&grid, mover());
        let b = Reachability::build(&grid, mover());
        assert_eq!(a.component, b.component);
    }
}
