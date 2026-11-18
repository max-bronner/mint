use mint_movement::{Cell, CellFlags, GridExtent, MoverClass, Passability};
use mint_sim::{Entity, Tick};

use super::{Ids, RotwkRules};
use crate::goal_cells::Footprint;

const IMPASSABLE_FOOTPRINT_CELL: u32 = 10;

pub(super) struct FootprintView<'a> {
    inner: &'a dyn Passability,
    terrain: &'a dyn Passability,
    mover: MoverClass,
    footprint: Footprint,
}

impl RotwkRules {
    pub(super) fn footprint_view<'a>(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        mover: MoverClass,
        inner: &'a dyn Passability,
        terrain: &'a dyn Passability,
    ) -> FootprintView<'a> {
        let wide = self
            .record_of(tick, ids, entity)
            .is_some_and(|record| record.has_kind("HORDE") || record.has_kind("SHIP"));
        let footprint = if wide {
            Footprint::THREE_CENTRED
        } else {
            self.footprint_of(tick, ids, entity)
        };
        FootprintView {
            inner,
            terrain,
            mover,
            footprint,
        }
    }
}

impl FootprintView<'_> {
    fn refused_round(&self, cell: Cell, mover: MoverClass) -> u32 {
        let side = i32::from(self.footprint.side());
        let half = side / 2;
        let past = side.saturating_sub(half);
        let mut refused = 0u32;
        for dy in half.saturating_neg()..past {
            for dx in half.saturating_neg()..past {
                let (Ok(x), Ok(y)) = (
                    u16::try_from(i32::from(cell.x).saturating_add(dx)),
                    u16::try_from(i32::from(cell.y).saturating_add(dy)),
                ) else {
                    refused = refused.saturating_add(1);
                    continue;
                };
                if !self
                    .terrain
                    .is_passable(Cell::on_layer(x, y, cell.layer), mover)
                {
                    refused = refused.saturating_add(1);
                }
            }
        }
        refused
    }
}

impl Passability for FootprintView<'_> {
    fn extent(&self) -> GridExtent {
        self.inner.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        self.inner.flags(cell)
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.inner.extra_cost(cell).saturating_add(
            self.refused_round(cell, self.mover)
                .saturating_mul(IMPASSABLE_FOOTPRINT_CELL),
        )
    }

    fn is_passable(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.is_passable(cell, mover)
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }

    fn line_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.is_passable(cell, mover) && self.refused_round(cell, mover) == 0
    }

    fn portal_at(&self, cell: Cell) -> Option<mint_movement::Portal> {
        self.inner.portal_at(cell)
    }
}

#[cfg(test)]
mod tests {
    use mint_movement::smoothing::{Smoothing as _, StringPull};
    use mint_movement::{
        AStar, Cell, CostGrid, GoalId, GridExtent, NodeBudget, PathRequest, Search as _,
        SearchOutcome,
    };
    use mint_sage_passability::{GROUND, OBSTACLE};

    use super::{Footprint, FootprintView};

    fn blocked_grid() -> CostGrid {
        let mut grid = CostGrid::new(GridExtent::new(30, 24).unwrap());
        for x in 10..=14 {
            for y in 0..=9 {
                grid.set_flags(Cell::new(x, y), OBSTACLE).unwrap();
            }
        }
        grid
    }

    #[test]
    fn a_three_cell_footprint_routes_a_cell_clear_of_a_block() {
        let grid = blocked_grid();
        let view = FootprintView {
            inner: &grid,
            terrain: &grid,
            mover: GROUND,
            footprint: Footprint::THREE_CENTRED,
        };
        let request = PathRequest {
            start: Cell::new(2, 5),
            goal: Cell::new(27, 5),
            mover: GROUND,
            goal_id: GoalId(0),
            origin: None,
            corner_radius: 0.0,
            departure: None,
        };
        let found = AStar::octile()
            .find(&view, &request, &mut NodeBudget::unlimited())
            .unwrap();
        let SearchOutcome::Found(path) = found.outcome else {
            panic!("no path round the block");
        };
        let route = StringPull.smooth(&view, GROUND, &path);
        for leg in route.cells.windows(2) {
            let (a, b) = (leg[0], leg[1]);
            for step in 0..=100u16 {
                let t = f32::from(step) / 100.0;
                let x = (f32::from(a.x) + (f32::from(b.x) - f32::from(a.x)) * t).round();
                let y = (f32::from(a.y) + (f32::from(b.y) - f32::from(a.y)) * t).round();
                assert!(
                    !((9.0..=15.0).contains(&x) && y < 11.0),
                    "leg {a:?} -> {b:?} passes ({x}, {y}), its footprint on the block"
                );
            }
        }
    }
}
