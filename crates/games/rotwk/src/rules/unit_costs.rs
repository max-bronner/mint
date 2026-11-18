use std::collections::BTreeMap;

use mint_movement::{Cell, CellFlags, GridExtent, Passability, Vec2};
use mint_sim::{Entity, Tick, World};

use super::{Ids, RotwkRules};
use crate::goal_cells::{self, Footprint};

const BUCKET: i32 = 8;

const AT_GOAL: u32 = 8;
const ARRIVES_FIRST: u32 = 4;

const FIXED_ALLIES: u32 = 14;

const OTHER_GOAL: u32 = 30;

#[derive(Debug, Clone, Copy)]
pub(super) struct BodySight {
    each_other: bool,
    through_infantry: bool,
    submarine: bool,
    large: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Bodies {
    refused: bool,
    allied: bool,
}

#[derive(Debug, Default)]
struct BodyCells {
    width: usize,
    height: usize,
    cells: Vec<Bodies>,
}

impl BodyCells {
    fn get(&self, x: i32, y: i32) -> Option<&Bodies> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells.get(y.checked_mul(self.width)?.checked_add(x)?)
    }

    fn entry(&mut self, extent: GridExtent, x: i32, y: i32) -> Option<&mut Bodies> {
        if self.cells.is_empty() {
            self.width = usize::from(extent.width);
            self.height = usize::from(extent.height);
            self.cells = vec![Bodies::default(); self.width.checked_mul(self.height)?];
        }
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells
            .get_mut(y.checked_mul(self.width)?.checked_add(x)?)
    }
}

struct Claim {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    flat: bool,
    position: Vec2,
    speed: f32,
}

pub(super) struct UnitCostView<'a> {
    inner: &'a dyn Passability,
    claims: Vec<Claim>,
    buckets: BTreeMap<(i32, i32), Vec<usize>>,
    half: i32,
    side: i32,
    shift: u32,
    geometry: mint_movement::CellGeometry,
    position: Vec2,
    speed: f32,
    bodies: BodyCells,
    exempt: Vec<Cell>,
    attack_path: bool,
}

impl Passability for UnitCostView<'_> {
    fn extent(&self) -> GridExtent {
        self.inner.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        self.inner.flags(cell)
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        let allied = self.bodies_on(cell).is_some_and(|bodies| bodies.allied);
        self.inner
            .extra_cost(cell)
            .saturating_add(self.unit_cost(cell))
            .saturating_add(if allied { FIXED_ALLIES } else { 0 })
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }

    fn is_passable(&self, cell: Cell, mover: mint_movement::MoverClass) -> bool {
        self.inner.is_passable(cell, mover)
            && (self.exempt.contains(&cell)
                || !self.bodies_on(cell).is_some_and(|bodies| bodies.refused))
    }

    fn portal_at(&self, cell: Cell) -> Option<mint_movement::Portal> {
        self.inner.portal_at(cell)
    }
}

impl UnitCostView<'_> {
    pub(super) fn exempting(mut self, cells: &[Cell]) -> Self {
        self.exempt.extend_from_slice(cells);
        self
    }

    pub(super) fn for_attack_path(mut self) -> Self {
        self.attack_path = true;
        self
    }

    fn bodies_on(&self, cell: Cell) -> Option<&Bodies> {
        self.bodies.get(i32::from(cell.x), i32::from(cell.y))
    }

    fn unit_cost(&self, cell: Cell) -> u32 {
        if self.claims.is_empty() {
            return 0;
        }
        let x0 = i32::from(cell.x) - self.half;
        let y0 = i32::from(cell.y) - self.half;
        let (x1, y1) = (x0 + self.side, y0 + self.side);
        let centre = self.geometry.centre_of(cell);
        let own = centre.minus(self.position).length() / self.speed.max(f32::EPSILON);
        let mut seen: Vec<usize> = Vec::new();
        let mut total = 0u32;
        let mut claimed = false;
        for bx in x0.div_euclid(BUCKET)..=(x1 - 1).div_euclid(BUCKET) {
            for by in y0.div_euclid(BUCKET)..=(y1 - 1).div_euclid(BUCKET) {
                let Some(list) = self.buckets.get(&(bx, by)) else {
                    continue;
                };
                for &index in list {
                    if seen.contains(&index) {
                        continue;
                    }
                    seen.push(index);
                    let Some(claim) = self.claims.get(index) else {
                        continue;
                    };
                    let w = x1.min(claim.x1) - x0.max(claim.x0);
                    let h = y1.min(claim.y1) - y0.max(claim.y0);
                    if w <= 0 || h <= 0 {
                        continue;
                    }
                    claimed = true;
                    let charge = if claim.flat {
                        AT_GOAL
                    } else if centre.minus(claim.position).length() / claim.speed.max(f32::EPSILON)
                        < own
                    {
                        ARRIVES_FIRST
                    } else {
                        continue;
                    };
                    let cells = u32::try_from(w.saturating_mul(h)).unwrap_or(1);
                    total = total.saturating_add((charge >> self.shift).saturating_mul(cells));
                }
            }
        }
        if self.attack_path && claimed {
            total = total.saturating_add(OTHER_GOAL);
        }
        total
    }
}

impl RotwkRules {
    pub(super) fn path_priority(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> u32 {
        let Some(record) = self.record_of(tick, ids, entity) else {
            return 0;
        };
        let crush = &record.crush;
        let level = crush
            .mounted_crusher_level
            .filter(|_| self.is_mounted(tick, entity))
            .unwrap_or(crush.crusher_level);
        let facts = self.kind_facts(tick.state(), ids, entity);
        let mut priority = 10 * u32::from(level);
        if facts.dozer {
            priority += 100;
        }
        if facts.cavalry {
            priority += 5;
        }
        priority
    }

    pub(super) fn path_footprint(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Footprint {
        let world = tick.state();
        if world.contains(ids.horde, entity) {
            let radius = crate::horde::horde_block(world, Self::horde_ids(ids), entity)
                .map_or(0.0, |block| {
                    Vec2::new(block.half_along, block.half_across).length()
                });
            let pathfind_diameter = self
                .record_of(tick, ids, entity)
                .and_then(|record| record.pathfind_diameter);
            Footprint::of(
                radius,
                true,
                self.passability.geometry.cell_size(),
                pathfind_diameter,
            )
        } else {
            self.footprint_of(tick, ids, entity)
        }
    }

    pub(super) fn unit_cost_view<'a>(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        inner: &'a dyn Passability,
    ) -> UnitCostView<'a> {
        let footprint = self.path_footprint(tick, ids, entity);
        self.unit_cost_view_for(tick, ids, entity, inner, footprint)
    }

    pub(super) fn unit_cost_view_for<'a>(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        inner: &'a dyn Passability,
        footprint: Footprint,
    ) -> UnitCostView<'a> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("unit_cost_view"));
        let world = tick.state();
        let geometry = self.passability.geometry;
        let side = i32::from(footprint.side());
        let half = side / 2;
        let shift = match half {
            h if h >= 4 => 2,
            h if h > 1 => 1,
            _ => 0,
        };
        let position = world
            .get(ids.body, entity)
            .map_or(Vec2::ZERO, |body| body.0.position);
        let speed = self.mover_limits(tick, ids, entity).max_speed;
        let own_locomotor = speed > 0.0;
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let own_priority = self.path_priority(tick, ids, entity);
        let mut claims = Vec::new();
        let mut buckets: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
        let relation = self.relation_from(world, entity);
        if let Some(store) = world.store(ids.shape) {
            for (other, shape) in store.iter() {
                if other == entity
                    || world.contains(ids.member, other)
                    || shape.kind_of.contains(super::KindOf::IMMOBILE)
                    || world.contains(ids.flight, other)
                    || world.contains(ids.doomed, other)
                    || world.contains(ids.concealed, other)
                {
                    continue;
                }
                if owner.is_some() && relation.to(other) == super::map_scripts::Relation::Enemies {
                    continue;
                }
                let limits = self.mover_limits(tick, ids, other);
                if limits.max_speed <= 0.0 || self.path_priority(tick, ids, other) < own_priority {
                    continue;
                }
                let Some(body) = world.get(ids.body, other) else {
                    continue;
                };
                let route = world.get(ids.path, other);
                let goal = route.map_or(body.0.position, |path| path.goal);
                let theirs = self.footprint_of(tick, ids, other);
                let cell = goal_cells::anchor(geometry, goal, theirs);
                let their_half = i32::from(theirs.side()) / 2;
                let x0 = cell.0 - their_half;
                let y0 = cell.1 - their_half;
                let their_side = i32::from(theirs.side());
                let other_at_goal = goal.minus(body.0.position).length() <= geometry.cell_size();
                let claim = Claim {
                    x0,
                    y0,
                    x1: x0 + their_side,
                    y1: y0 + their_side,
                    flat: other_at_goal || !own_locomotor || limits.max_speed <= 0.0,
                    position: body.0.position,
                    speed: limits.max_speed,
                };
                let index = claims.len();
                for bx in claim.x0.div_euclid(BUCKET)..=(claim.x1 - 1).div_euclid(BUCKET) {
                    for by in claim.y0.div_euclid(BUCKET)..=(claim.y1 - 1).div_euclid(BUCKET) {
                        buckets.entry((bx, by)).or_default().push(index);
                    }
                }
                claims.push(claim);
            }
        }
        let bodies = self.body_cells(tick, ids, entity);
        let exempt = self.passability.cell_at(position).into_iter().collect();
        UnitCostView {
            inner,
            claims,
            buckets,
            half,
            side,
            shift,
            geometry,
            position,
            speed,
            bodies,
            exempt,
            attack_path: false,
        }
    }

    pub(super) fn body_sight(&self, world: &World, ids: Ids, mover: Entity) -> BodySight {
        let facts = self.kind_facts(world, ids, mover);
        BodySight {
            each_other: facts.path_through_each_other,
            through_infantry: facts.path_through_infantry,
            submarine: facts.submarine,
            large: facts.large_rectangle_pathfind,
        }
    }

    pub(super) fn sees_past(
        &self,
        world: &World,
        ids: Ids,
        sight: BodySight,
        other: Entity,
        ally: bool,
    ) -> bool {
        let facts = self.kind_facts(world, ids, other);
        sight.submarine
            || (ally && sight.each_other && facts.path_through_each_other)
            || (sight.through_infantry && facts.infantry)
            || (ally && sight.large && !facts.large_rectangle_pathfind)
    }

    fn body_cells(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> BodyCells {
        let _perf = crate::perf::sub(crate::perf::sub_row!("body_cells"));
        let world = tick.state();
        let mut cells = BodyCells::default();
        let Some(_) = world.get(ids.owner, entity).map(|owner| owner.0) else {
            return cells;
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let record = self.record_of(tick, ids, entity);
        let hero = self.kind_facts(world, ids, entity).hero;
        let crusher = record.map_or(0, |record| record.crush.crusher_level);
        let sight = self.body_sight(world, ids, entity);
        let target = world
            .get(ids.attack_order, entity)
            .map(|order| order.target)
            .or_else(|| {
                world
                    .get(ids.horde_target, entity)
                    .and_then(|held| held.target)
            })
            .or_else(|| world.get(ids.horde_melee, entity).map(|melee| melee.goal))
            .or_else(|| world.get(ids.armed, entity).and_then(|armed| armed.target));
        let horde_of = |of: Entity| world.get(ids.member, of).map(|member| member.horde);
        let target_horde = target.map(|target| horde_of(target).unwrap_or(target));
        let Some(store) = world.store(ids.shape) else {
            return cells;
        };
        let relation = self.relation_from(world, entity);
        for (other, shape) in store.iter() {
            if other == entity
                || world.contains(ids.horde, other)
                || shape.kind_of.contains(super::KindOf::IMMOBILE)
                || world.contains(ids.flight, other)
                || world.contains(ids.doomed, other)
                || world.contains(ids.concealed, other)
                || world
                    .get(ids.health, other)
                    .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            let Some(body) = world.get(ids.body, other) else {
                continue;
            };
            if self.mover_limits(tick, ids, other).max_speed <= 0.0 {
                continue;
            }
            let enemy = relation.to(other) == super::map_scripts::Relation::Enemies;
            if self.sees_past(world, ids, sight, other, !enemy) {
                continue;
            }
            if enemy {
                let walked_into = hero
                    || Some(other) == target
                    || Some(horde_of(other).unwrap_or(other)) == target_horde
                    || self.record_of(tick, ids, other).is_some_and(|theirs| {
                        i16::from(crusher) > i16::from(theirs.crush.crushable_level)
                    });
                if walked_into {
                    continue;
                }
            }
            let footprint = self.footprint_of(tick, ids, other);
            let Some(rect) = goal_cells::rect(
                extent,
                goal_cells::anchor(geometry, body.0.position, footprint),
                footprint,
            ) else {
                continue;
            };
            let (x0, y0, x1, y1) = rect.span();
            for y in y0..y1 {
                for x in x0..x1 {
                    let Some(held) = cells.entry(extent, x, y) else {
                        continue;
                    };
                    if enemy {
                        held.refused = true;
                    } else {
                        held.allied = true;
                    }
                }
            }
        }
        cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_movement::{CellGeometry, CostGrid};

    fn view(grid: &CostGrid, attack_path: bool) -> UnitCostView<'_> {
        let mut buckets: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
        buckets.insert((0, 0), vec![0]);
        UnitCostView {
            inner: grid,
            claims: vec![Claim {
                x0: 5,
                y0: 5,
                x1: 6,
                y1: 6,
                flat: true,
                position: Vec2::new(0.0, 0.0),
                speed: 1.0,
            }],
            buckets,
            half: 0,
            side: 1,
            shift: 0,
            geometry: CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap(),
            position: Vec2::new(0.0, 0.0),
            speed: 1.0,
            bodies: BodyCells::default(),
            exempt: Vec::new(),
            attack_path,
        }
    }

    #[test]
    fn the_overlap_charge_is_the_areas_product_not_the_mean_of_its_sides() {
        let grid = CostGrid::new(GridExtent::new(10, 10).unwrap());
        let mut buckets: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
        buckets.insert((0, 0), vec![0]);
        let view = UnitCostView {
            inner: &grid,
            claims: vec![Claim {
                x0: 5,
                y0: 5,
                x1: 8,
                y1: 6,
                flat: true,
                position: Vec2::new(0.0, 0.0),
                speed: 1.0,
            }],
            buckets,
            half: 2,
            side: 4,
            shift: 0,
            geometry: CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap(),
            position: Vec2::new(0.0, 0.0),
            speed: 1.0,
            bodies: BodyCells::default(),
            exempt: Vec::new(),
            attack_path: false,
        };
        assert_eq!(view.extra_cost(Cell::new(6, 5)), AT_GOAL * 3);
    }

    #[test]
    fn a_claimed_cell_costs_thirty_more_only_in_an_attack_path_search() {
        let grid = CostGrid::new(GridExtent::new(10, 10).unwrap());
        let claimed = Cell::new(5, 5);
        let unclaimed = Cell::new(1, 1);
        assert_eq!(view(&grid, false).extra_cost(claimed), AT_GOAL);
        assert_eq!(view(&grid, true).extra_cost(claimed), AT_GOAL + OTHER_GOAL);
        assert_eq!(view(&grid, true).extra_cost(unclaimed), 0);
    }
}
