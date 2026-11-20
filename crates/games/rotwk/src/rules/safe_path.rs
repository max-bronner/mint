use mint_movement::{Cell, NodeBudget, SearchOutcome, Vec2};
use mint_sim::{Entity, Tick};

use super::{Ids, RotwkRules};
use crate::goal_cells;

const MAX_CELLS: u32 = 2000;

#[derive(Debug, Clone, Copy)]
pub(super) struct SafeRequest {
    pub(super) repulsor: Vec2,
    pub(super) prior: Vec2,
    pub(super) radius: f32,
}

pub(super) const ABSENT_REPULSOR: Vec2 = Vec2 {
    x: -1000.0,
    y: -1000.0,
};

impl RotwkRules {
    pub(super) fn find_safe_goal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        request: SafeRequest,
    ) -> Option<Vec2> {
        let world = tick.state();
        let from = world.get(ids.body, entity)?.0.position;
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let mover = self.mover_class(tick, ids, entity);
        let start = self.passable_cell_for(tick, ids, from, mover)?;
        let footprint = self.path_footprint(tick, ids, entity);
        let view = self.dynamic_view(tick, ids);
        let reach =
            request.radius * 1.5 + geometry.cell_size() * (f32::from(footprint.side()) + 2.0);
        let units = self.unit_cells(tick, ids, entity, None, from, reach);
        let cell_of = |(x, y): (i32, i32)| {
            let cell = Cell::new(u16::try_from(x).ok()?, u16::try_from(y).ok()?);
            extent.contains(cell).then_some(cell)
        };
        let admits = |cell: Cell| {
            let at = (i32::from(cell.x), i32::from(cell.y));
            let Some(rect) = goal_cells::rect(extent, at, footprint) else {
                return false;
            };
            let (x0, y0, x1, y1) = rect.span();
            let covered = || (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)));
            covered().all(|at| {
                cell_of(at)
                    .is_some_and(|cell| mint_movement::Passability::is_passable(&view, cell, mover))
            }) && units.destination_clear(covered())
        };
        let distance_squared = |cell: Cell| {
            let centre = geometry.centre_of(cell);
            let near = |to: Vec2| {
                let (dx, dy) = (centre.x - to.x, centre.y - to.y);
                dx * dx + dy * dy
            };
            near(request.repulsor).min(near(request.prior))
        };
        let radius_squared = request.radius * request.radius;
        let mut farthest = distance_squared(start);
        let mut kept = self.attack_spot_search.try_borrow_mut();
        let mut fresh;
        let search = match kept.as_deref_mut() {
            Ok(kept) => kept,
            Err(_) => {
                fresh = mint_movement::AStar::octile();
                &mut fresh
            }
        };
        let mut budget = NodeBudget::unlimited();
        let result = search
            .find_where_with(
                &view,
                start,
                mover,
                start,
                u32::MAX,
                &mut budget,
                |cell, progress| {
                    let distance = distance_squared(cell);
                    let mut candidate =
                        distance > radius_squared || (progress.open_empty && progress.added > 0);
                    if distance > farthest {
                        farthest = distance;
                        candidate |= progress.added > MAX_CELLS;
                    }
                    candidate && admits(cell)
                },
            )
            .ok()?;
        match result.outcome {
            SearchOutcome::Found(path) => path.cells.last().map(|&cell| geometry.centre_of(cell)),
            _ => None,
        }
    }
}
