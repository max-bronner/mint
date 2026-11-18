use mint_movement::{
    Cell, CellFlags, CellGeometry, GridExtent, HeightField, MoverClass, Passability,
};
use mint_sage_passability::{halved_when_extra_passable, CLIFF, IMPASSABLE_TO_PLAYERS, PINCHED};
use mint_sim::{Entity, Tick};

use super::{Ids, RotwkRules};
use crate::goal_cells::Footprint;
use crate::wall_layer::{Layer, FIRST_WALL_TAG};

const IMPASSABLE_FOOTPRINT_CELL: u32 = 10;

const CLIFF_STEP_COST: u32 = 98;

const CLIFF_STEP_HEIGHT: f32 = 10.0;

pub(super) struct FootprintView<'a> {
    inner: &'a dyn Passability,
    terrain: &'a dyn Passability,
    mover: MoverClass,
    footprint: Footprint,
    tags: &'a [u8],
    ground: Option<(&'a HeightField, CellGeometry)>,
}

impl RotwkRules {
    pub(super) fn footprint_view<'a>(
        &'a self,
        tick: &'a Tick<'_>,
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
            tags: crate::wall_layer::layer_of(tick.state(), ids.wall_layer)
                .map_or(&[][..], crate::wall_layer::WallLayer::tags),
            ground: Some((&self.passability.heights, self.passability.geometry)),
        }
    }
}

impl FootprintView<'_> {
    fn footprint_cells(&self, cell: Cell) -> impl Iterator<Item = Option<Cell>> + '_ {
        let side = i32::from(self.footprint.side());
        let half = side / 2;
        let past = side.saturating_sub(half);
        let extent = self.terrain.extent();
        (half.saturating_neg()..past).flat_map(move |dy| {
            (half.saturating_neg()..past).map(move |dx| {
                let x = u16::try_from(i32::from(cell.x).saturating_add(dx)).ok()?;
                let y = u16::try_from(i32::from(cell.y).saturating_add(dy)).ok()?;
                (x < extent.width && y < extent.height).then_some(Cell::on_layer(x, y, cell.layer))
            })
        })
    }

    fn off_grid(&self, cell: Cell) -> bool {
        self.footprint_cells(cell).any(|on| on.is_none())
    }

    fn cliff_step_cost(&self, from: Cell, to: Cell) -> u32 {
        let flags = self.inner.flags(to);
        if !flags.intersects(CLIFF) || flags.intersects(PINCHED) {
            return 0;
        }
        let Some((heights, geometry)) = self.ground else {
            return 0;
        };
        let size = geometry.cell_size();
        let corner = |cell: Cell| {
            heights.height_at(
                geometry.origin().x + f32::from(cell.x) * size,
                geometry.origin().y + f32::from(cell.y) * size,
            )
        };
        match corner(from).zip(corner(to)) {
            Some((parent, entered)) if (parent - entered).abs() > CLIFF_STEP_HEIGHT => {
                CLIFF_STEP_COST
            }
            _ => 0,
        }
    }

    fn tag(&self, cell: Cell) -> u8 {
        self.terrain
            .extent()
            .index(cell)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.tags.get(index).copied())
            .unwrap_or(Layer::GROUND.0)
    }

    fn layer_mismatches(&self, from: Cell, to: Cell) -> u32 {
        let parent = self.tag(from);
        if parent != Layer::GROUND.0 && parent < FIRST_WALL_TAG {
            return 0;
        }
        let mut counted = 0u32;
        for on in self.footprint_cells(to).flatten() {
            let tag = self.tag(on);
            if tag != parent && tag != Layer::RAMPS.0 {
                counted = counted.saturating_add(1);
            }
        }
        counted
    }

    fn refused_round(&self, cell: Cell, mover: MoverClass) -> u32 {
        let barred_to_players = mover.blocked_by.intersects(IMPASSABLE_TO_PLAYERS);
        let mut counted = 0u32;
        for on in self.footprint_cells(cell).flatten() {
            if !self.terrain.is_passable(on, mover) {
                counted = counted.saturating_add(1);
            }
            if barred_to_players && self.terrain.flags(on).intersects(IMPASSABLE_TO_PLAYERS) {
                counted = counted.saturating_add(1);
            }
        }
        counted
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
        self.inner
            .extra_cost(cell)
            .saturating_add(halved_when_extra_passable(
                self.inner.flags(cell),
                self.refused_round(cell, self.mover)
                    .saturating_mul(IMPASSABLE_FOOTPRINT_CELL),
            ))
    }

    fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
        self.inner.goal_extra_cost(cell, goal)
    }

    fn step_extra_cost(&self, from: Cell, to: Cell) -> u32 {
        self.inner
            .step_extra_cost(from, to)
            .saturating_add(halved_when_extra_passable(
                self.inner.flags(to),
                self.layer_mismatches(from, to)
                    .saturating_mul(IMPASSABLE_FOOTPRINT_CELL),
            ))
            .saturating_add(self.cliff_step_cost(from, to))
    }

    fn tail_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.tail_clear(cell, mover)
    }

    fn is_passable(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.is_passable(cell, mover)
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }

    fn straight_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.straight_clear(cell, mover)
            && !self.off_grid(cell)
            && self.refused_round(cell, mover) == 0
    }

    fn line_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.line_clear(cell, mover)
            && !self.off_grid(cell)
            && self.refused_round(cell, mover) == 0
    }

    fn admits_step(&self, from: Cell, to: Cell, mover: MoverClass) -> bool {
        self.inner.admits_step(from, to, mover) && !self.off_grid(to)
    }

    fn portal_at(&self, cell: Cell) -> Option<mint_movement::Portal> {
        self.inner.portal_at(cell)
    }

    fn portal_links(&self, cell: Cell) -> &[mint_movement::PortalLink] {
        self.inner.portal_links(cell)
    }
}

#[cfg(test)]
mod tests {
    use mint_movement::smoothing::{Smoothing as _, StringPull};
    use mint_movement::Passability as _;
    use mint_movement::{
        AStar, Cell, CellFlags, CellGeometry, CostGrid, GoalId, GridExtent, HeightField,
        NodeBudget, PathRequest, Search as _, SearchOutcome,
    };
    use mint_sage_passability::{
        CLIFF, GROUND, GROUND_PLAYER, IMPASSABLE_TO_PLAYERS, OBSTACLE, PINCHED,
    };

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
            tags: &[],
            ground: None,
        };
        let request = PathRequest {
            start: Cell::new(2, 5),
            goal: Cell::new(27, 5),
            mover: GROUND,
            goal_id: GoalId(0),
            origin: None,
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
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

    #[test]
    fn a_footprint_off_the_grid_refuses_the_step() {
        let grid = CostGrid::new(GridExtent::new(30, 24).unwrap());
        let view = FootprintView {
            inner: &grid,
            terrain: &grid,
            mover: GROUND,
            footprint: Footprint::THREE_CENTRED,
            tags: &[],
            ground: None,
        };
        assert!(!view.admits_step(Cell::new(5, 2), Cell::new(5, 0), GROUND));
        assert!(!view.admits_step(Cell::new(28, 5), Cell::new(29, 5), GROUND));
        assert!(view.admits_step(Cell::new(5, 2), Cell::new(5, 1), GROUND));
    }

    #[test]
    fn a_cell_painted_impassable_to_players_counts_twice_for_a_player_mover() {
        let mut grid = CostGrid::new(GridExtent::new(30, 24).unwrap());
        grid.set_flags(Cell::new(10, 10), IMPASSABLE_TO_PLAYERS)
            .unwrap();
        let view = |mover| FootprintView {
            inner: &grid,
            terrain: &grid,
            mover,
            footprint: Footprint::THREE_CENTRED,
            tags: &[],
            ground: None,
        };
        assert_eq!(view(GROUND_PLAYER).extra_cost(Cell::new(10, 11)), 20);
        assert_eq!(view(GROUND).extra_cost(Cell::new(10, 11)), 0);
    }

    #[test]
    fn a_footprint_cell_on_another_layer_costs_ten_unless_it_is_a_ramp() {
        let grid = CostGrid::new(GridExtent::new(30, 24).unwrap());
        let extent = grid.extent();
        let tags_with = |tag: u8| {
            let mut tags = vec![1u8; usize::try_from(extent.cell_count()).unwrap()];
            let at = usize::try_from(extent.index(Cell::new(11, 10)).unwrap()).unwrap();
            tags[at] = tag;
            tags
        };
        let cost = |tags: &[u8]| {
            FootprintView {
                inner: &grid,
                terrain: &grid,
                mover: GROUND,
                footprint: Footprint::THREE_CENTRED,
                tags,
                ground: None,
            }
            .step_extra_cost(Cell::new(9, 10), Cell::new(10, 10))
        };
        assert_eq!(cost(&tags_with(17)), 10);
        assert_eq!(cost(&tags_with(16)), 0);
        assert_eq!(cost(&[]), 0);
    }

    #[test]
    fn entering_a_cliff_cell_past_a_height_step_costs_98() {
        let extent = GridExtent::new(30, 24).unwrap();
        let geometry = CellGeometry::new(10.0, mint_movement::Vec2::ZERO).unwrap();
        let nodes = GridExtent::new(31, 25).unwrap();
        let samples = (0..25)
            .flat_map(|_| (0..31).map(|x| if x >= 10 { 30.0 } else { 0.0 }))
            .collect();
        let heights = HeightField::new(nodes, geometry, samples).unwrap();
        let cost = |flags: CellFlags| {
            let mut grid = CostGrid::new(extent);
            grid.set_flags(Cell::new(10, 10), flags).unwrap();
            FootprintView {
                inner: &grid,
                terrain: &grid,
                mover: GROUND,
                footprint: Footprint::ONE_CENTRED,
                tags: &[],
                ground: Some((&heights, geometry)),
            }
            .step_extra_cost(Cell::new(9, 10), Cell::new(10, 10))
        };
        assert_eq!(cost(CLIFF), 98);
        assert_eq!(cost(CLIFF.union(PINCHED)), 0);
        assert_eq!(cost(CellFlags::NONE), 0);
        assert_eq!(
            FootprintView {
                inner: &CostGrid::new(extent),
                terrain: &CostGrid::new(extent),
                mover: GROUND,
                footprint: Footprint::ONE_CENTRED,
                tags: &[],
                ground: Some((&heights, geometry)),
            }
            .step_extra_cost(Cell::new(11, 10), Cell::new(12, 10)),
            0
        );
    }

    #[test]
    fn an_extra_passable_cell_pays_half_the_footprint_charge() {
        let mut grid = CostGrid::new(GridExtent::new(30, 24).unwrap());
        for y in 10..=12 {
            grid.set_flags(Cell::new(10, y), OBSTACLE).unwrap();
        }
        let view = |grid: &CostGrid| {
            FootprintView {
                inner: grid,
                terrain: grid,
                mover: GROUND,
                footprint: Footprint::THREE_CENTRED,
                tags: &[],
                ground: None,
            }
            .extra_cost(Cell::new(10, 11))
        };
        let whole = view(&grid);
        grid.add_flags(Cell::new(10, 11), mint_sage_passability::EXTRA_PASSABLE)
            .unwrap();
        assert_eq!((whole, view(&grid)), (30, 15));
    }
}
