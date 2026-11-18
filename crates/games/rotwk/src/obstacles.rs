use mint_movement::{Cell, CellGeometry, CostGrid, GridExtent, Vec2};
use mint_sage_passability::{
    covered_cells, grow_bounds, obstacle_border_passes, CellRect, Footprint, FootprintRecord,
    Obstacle, GROUND, OBSTACLE,
};
use mint_sim::{Component, ComponentId, Entity, StateHasher, Tick, World};

#[derive(Debug, Clone, PartialEq)]
struct Stamp {
    entity: Entity,
    centre: Vec2,
    angle: f32,
    records: Vec<FootprintRecord>,
    rubble: bool,
    scaleable: bool,
}

impl Stamp {
    fn as_obstacle(&self) -> Obstacle {
        Obstacle {
            id: 0,
            centre: self.centre,
            angle: self.angle,
            records: self.records.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObstacleLayer {
    counts: Vec<u8>,
    rubble: Vec<u8>,
    closed: Vec<u8>,
    pinched: Vec<u8>,
    scaleable: Vec<u8>,
    stamps: Vec<Stamp>,
}

impl ObstacleLayer {
    pub(crate) fn empty(cell_count: usize) -> Self {
        Self {
            counts: vec![0; cell_count],
            rubble: vec![0; cell_count],
            closed: vec![0; cell_count],
            pinched: vec![0; cell_count],
            scaleable: vec![0; cell_count],
            stamps: Vec::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn blocked(&self, index: usize) -> bool {
        self.counts.get(index).is_some_and(|count| *count > 0)
    }

    pub(crate) fn counts(&self) -> &[u8] {
        &self.counts
    }

    pub(crate) fn rubble(&self) -> &[u8] {
        &self.rubble
    }

    pub(crate) fn closed(&self) -> &[u8] {
        &self.closed
    }

    pub(crate) fn scaleable(&self) -> &[u8] {
        &self.scaleable
    }

    pub(crate) fn pinched(&self) -> &[u8] {
        &self.pinched
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.stamps.len()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stamp(
        &mut self,
        grid: &CostGrid,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
        centre: Vec2,
        angle: f32,
        records: Vec<FootprintRecord>,
        scaleable: bool,
    ) -> Result<(), String> {
        let stamp = Stamp {
            entity,
            centre,
            angle,
            records,
            rubble: false,
            scaleable,
        };
        let cells = covered_cells(geometry, extent, &stamp.as_obstacle());
        let mut indices = Vec::with_capacity(cells.len());
        for cell in &cells {
            let Some(index) = extent
                .index(*cell)
                .and_then(|raw| usize::try_from(raw).ok())
            else {
                continue;
            };
            let Some(count) = self.counts.get(index).copied() else {
                continue;
            };
            if count.checked_add(1).is_none() {
                return Err(format!(
                    "entity {}'s footprint refused: cell {cell:?} already carries {count} \
                     overlapping structures",
                    entity.index()
                ));
            }
            indices.push(index);
        }
        let bounds = cells_bounds(&cells);
        self.unstamp(grid, geometry, extent, entity);
        for index in &indices {
            if let Some(slot) = self.counts.get_mut(*index) {
                *slot = slot.saturating_add(1);
            }
            if scaleable {
                if let Some(slot) = self.scaleable.get_mut(*index) {
                    *slot = slot.saturating_add(1);
                }
            }
        }
        let at = self
            .stamps
            .partition_point(|held| held.entity.index() < entity.index());
        self.stamps.insert(at, stamp);
        if let Some(bounds) = bounds {
            self.recompute_borders(grid, extent, grow_bounds(bounds, extent, 2));
        }
        Ok(())
    }

    pub(crate) fn holds(&self, entity: Entity) -> bool {
        self.stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .is_ok()
    }

    pub(crate) fn records_of(&self, entity: Entity) -> Option<Vec<FootprintRecord>> {
        let at = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .ok()?;
        self.stamps.get(at).map(|stamp| stamp.records.clone())
    }

    pub(crate) fn cells_of(
        &self,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
    ) -> Option<Vec<mint_movement::Cell>> {
        let at = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .ok()?;
        let stamp = self.stamps.get(at).filter(|stamp| !stamp.rubble)?;
        Some(covered_cells(geometry, extent, &stamp.as_obstacle()))
    }

    pub(crate) fn unstamp(
        &mut self,
        grid: &CostGrid,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
    ) {
        let Ok(at) = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
        else {
            return;
        };
        let stamp = self.stamps.remove(at);
        let cells = covered_cells(geometry, extent, &stamp.as_obstacle());
        let bounds = cells_bounds(&cells);
        let was_scaleable = stamp.scaleable;
        let counts = if stamp.rubble {
            &mut self.rubble
        } else {
            &mut self.counts
        };
        for cell in &cells {
            let Some(index) = extent
                .index(*cell)
                .and_then(|raw| usize::try_from(raw).ok())
            else {
                continue;
            };
            if let Some(slot) = counts.get_mut(index) {
                *slot = slot.saturating_sub(1);
            }
        }
        if was_scaleable {
            for cell in &cells {
                let Some(index) = extent
                    .index(*cell)
                    .and_then(|raw| usize::try_from(raw).ok())
                else {
                    continue;
                };
                if let Some(slot) = self.scaleable.get_mut(index) {
                    *slot = slot.saturating_sub(1);
                }
            }
        }
        if let Some(bounds) = bounds {
            let grown = grow_bounds(bounds, extent, 2);
            self.clear_borders(extent, grown);
            self.recompute_borders(grid, extent, grown);
        }
    }

    pub(crate) fn make_rubble(
        &mut self,
        grid: &CostGrid,
        geometry: CellGeometry,
        extent: GridExtent,
        entity: Entity,
    ) {
        let Ok(at) = self
            .stamps
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
        else {
            return;
        };
        let Some(stamp) = self.stamps.get(at).filter(|stamp| !stamp.rubble) else {
            return;
        };
        let cells = covered_cells(geometry, extent, &stamp.as_obstacle());
        let bounds = cells_bounds(&cells);
        let indices: Vec<usize> = cells
            .into_iter()
            .filter_map(|cell| extent.index(cell).and_then(|raw| usize::try_from(raw).ok()))
            .filter(|index| index < &self.rubble.len())
            .collect();
        if indices.iter().any(|index| {
            self.rubble
                .get(*index)
                .is_some_and(|count| *count == u8::MAX)
        }) {
            return;
        }
        for index in indices {
            if let Some(slot) = self.counts.get_mut(index) {
                *slot = slot.saturating_sub(1);
            }
            if let Some(slot) = self.rubble.get_mut(index) {
                *slot = slot.saturating_add(1);
            }
        }
        if let Some(stamp) = self.stamps.get_mut(at) {
            stamp.rubble = true;
        }
        if let Some(bounds) = bounds {
            let grown = grow_bounds(bounds, extent, 2);
            self.clear_borders(extent, grown);
            self.recompute_borders(grid, extent, grown);
        }
    }

    fn dead_stamps(&self, world: &World) -> Vec<Entity> {
        self.stamps
            .iter()
            .map(|stamp| stamp.entity)
            .filter(|entity| !world.is_live(*entity))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn rebuild(&self, geometry: CellGeometry, extent: GridExtent) -> (Vec<u8>, Vec<u8>) {
        let mut counts = vec![0u8; self.counts.len()];
        let mut rubble = vec![0u8; self.rubble.len()];
        for stamp in &self.stamps {
            let into = if stamp.rubble {
                &mut rubble
            } else {
                &mut counts
            };
            for cell in covered_cells(geometry, extent, &stamp.as_obstacle()) {
                let Some(index) = extent.index(cell).and_then(|raw| usize::try_from(raw).ok())
                else {
                    continue;
                };
                if let Some(slot) = into.get_mut(index) {
                    *slot = slot.saturating_add(1);
                }
            }
        }
        (counts, rubble)
    }

    fn clear_borders(&mut self, extent: GridExtent, bounds: CellRect) {
        for y in bounds.y0..=bounds.y1 {
            for x in bounds.x0..=bounds.x1 {
                let cell = Cell::new(x, y);
                let Some(index) = extent.index(cell).and_then(|raw| usize::try_from(raw).ok())
                else {
                    continue;
                };
                if let Some(slot) = self.closed.get_mut(index) {
                    *slot = 0;
                }
                if let Some(slot) = self.pinched.get_mut(index) {
                    *slot = 0;
                }
            }
        }
    }

    fn recompute_borders(&mut self, grid: &CostGrid, extent: GridExtent, bounds: CellRect) {
        let counts = &self.counts;
        let rubble = &self.rubble;
        let closed = &self.closed;
        let dyn_index = |cell: Cell| extent.index(cell).and_then(|i| usize::try_from(i).ok());
        let is_obstacle = |cell: Cell| {
            dyn_index(cell).is_some_and(|i| {
                counts.get(i).copied().unwrap_or(0) > 0 || closed.get(i).copied().unwrap_or(0) != 0
            }) || mint_movement::Passability::flags(grid, cell).intersects(OBSTACLE)
        };
        let is_clear = |cell: Cell| {
            let dynamically_blocked = dyn_index(cell).is_some_and(|i| {
                counts.get(i).copied().unwrap_or(0) > 0
                    || rubble.get(i).copied().unwrap_or(0) > 0
                    || closed.get(i).copied().unwrap_or(0) != 0
            });
            !dynamically_blocked
                && !mint_movement::Passability::flags(grid, cell).intersects(GROUND.blocked_by)
        };
        let (new_closed, new_pinched) =
            obstacle_border_passes(bounds, extent, is_clear, is_obstacle, |_| true);
        for cell in new_closed {
            if let Some(index) = dyn_index(cell) {
                if let Some(slot) = self.closed.get_mut(index) {
                    *slot = 1;
                }
            }
        }
        for cell in new_pinched {
            if let Some(index) = dyn_index(cell) {
                if let Some(slot) = self.pinched.get_mut(index) {
                    *slot = 1;
                }
            }
        }
    }
}

fn cells_bounds(cells: &[Cell]) -> Option<CellRect> {
    cells.iter().fold(None, |bounds, cell| {
        Some(match bounds {
            None => CellRect {
                x0: cell.x,
                x1: cell.x,
                y0: cell.y,
                y1: cell.y,
            },
            Some(r) => CellRect {
                x0: r.x0.min(cell.x),
                x1: r.x1.max(cell.x),
                y0: r.y0.min(cell.y),
                y1: r.y1.max(cell.y),
            },
        })
    })
}

pub(crate) fn counts_of(world: &World, id: ComponentId<ObstacleLayer>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[][..], ObstacleLayer::counts)
}

pub(crate) fn layer_of(world: &World, id: ComponentId<ObstacleLayer>) -> Option<&ObstacleLayer> {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
}

pub(crate) fn scaleable_walls_of(
    world: &World,
    id: ComponentId<ScaleableWalls>,
) -> Option<&ScaleableWalls> {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
}

pub(crate) fn rubble_of(world: &World, id: ComponentId<ObstacleLayer>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[][..], ObstacleLayer::rubble)
}

pub(crate) fn scaleable_of(world: &World, id: ComponentId<ObstacleLayer>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[][..], ObstacleLayer::scaleable)
}

pub(crate) fn closed_of(world: &World, id: ComponentId<ObstacleLayer>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[][..], ObstacleLayer::closed)
}

pub(crate) fn pinched_of(world: &World, id: ComponentId<ObstacleLayer>) -> &[u8] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[][..], ObstacleLayer::pinched)
}

pub(crate) fn reap_dead_stamps(
    tick: &mut Tick<'_>,
    id: ComponentId<ObstacleLayer>,
    grid: &CostGrid,
    geometry: CellGeometry,
    extent: GridExtent,
) -> mint_sim::Result<()> {
    let Some(clock_entity) = tick
        .state()
        .store(id)
        .and_then(|store| store.entities().first().copied())
    else {
        return Ok(());
    };
    let dead = tick
        .state()
        .get(id, clock_entity)
        .map(|layer| layer.dead_stamps(tick.state()))
        .unwrap_or_default();
    if dead.is_empty() {
        return Ok(());
    }
    let Some(mut layer) = tick.state().get(id, clock_entity).cloned() else {
        return Ok(());
    };
    for entity in dead {
        layer.unstamp(grid, geometry, extent, entity);
    }
    tick.state_mut().insert(id, clock_entity, layer)?;
    Ok(())
}

impl Component for ObstacleLayer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.stamps.len());
        for stamp in &self.stamps {
            stamp.entity.hash_into(hasher);
            hasher.write_f32(stamp.centre.x);
            hasher.write_f32(stamp.centre.y);
            hasher.write_f32(stamp.angle);
            hasher.write_bool(stamp.rubble);
            hasher.write_usize(stamp.records.len());
            for record in &stamp.records {
                hasher.write_f32(record.offset.x);
                hasher.write_f32(record.offset.y);
                match record.footprint {
                    Footprint::Box { major, minor } => {
                        hasher.write_u8(0);
                        hasher.write_f32(major);
                        hasher.write_f32(minor);
                    }
                    Footprint::Disc { radius } => {
                        hasher.write_u8(1);
                        hasher.write_f32(radius);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ScaleableWalls {
    entries: Vec<(Entity, mint_sage_passability::WallGeometry)>,
}

impl ScaleableWalls {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn insert(&mut self, entity: Entity, geometry: mint_sage_passability::WallGeometry) {
        let at = self
            .entries
            .partition_point(|(held, _)| held.index() < entity.index());
        if let Some(existing) = self.entries.get_mut(at).filter(|(held, _)| *held == entity) {
            existing.1 = geometry;
        } else {
            self.entries.insert(at, (entity, geometry));
        }
    }

    pub(crate) fn remove(&mut self, entity: Entity) {
        if let Ok(at) = self
            .entries
            .binary_search_by_key(&entity.index(), |(held, _)| held.index())
        {
            self.entries.remove(at);
        }
    }

    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = &(Entity, mint_sage_passability::WallGeometry)> {
        self.entries.iter()
    }
}

impl Component for ScaleableWalls {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.entries.len());
        for (entity, wall) in &self.entries {
            entity.hash_into(hasher);
            hasher.write_f32(wall.centre.x);
            hasher.write_f32(wall.centre.y);
            hasher.write_f32(wall.angle);
            hasher.write_f32(wall.major_radius);
            hasher.write_f32(wall.height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_movement::GridExtent as Extent;
    use mint_sim::World;

    fn geometry_and_extent() -> (CellGeometry, GridExtent, CostGrid) {
        let extent = Extent::new(40, 40).unwrap();
        (
            CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap(),
            extent,
            CostGrid::new(extent),
        )
    }

    fn box_records(major: f32, minor: f32) -> Vec<FootprintRecord> {
        vec![FootprintRecord {
            offset: Vec2::new(0.0, 0.0),
            footprint: Footprint::Box { major, minor },
        }]
    }

    #[test]
    fn the_counts_rebuild_to_the_same_thing_after_any_stamp_sequence() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut ids = World::new();
        let entities: Vec<Entity> = (0..5).map(|_| ids.spawn().unwrap()).collect();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entities[0],
                Vec2::new(105.0, 105.0),
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entities[1],
                Vec2::new(115.0, 105.0),
                0.0,
                box_records(15.0, 15.0),
                false,
            )
            .unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entities[2],
                Vec2::new(300.0, 300.0),
                0.3,
                box_records(10.0, 25.0),
                false,
            )
            .unwrap();
        assert_eq!(
            layer.rebuild(geometry, extent),
            (layer.counts.clone(), layer.rubble.clone())
        );
        layer.unstamp(&grid, geometry, extent, entities[0]);
        assert_eq!(
            layer.rebuild(geometry, extent),
            (layer.counts.clone(), layer.rubble.clone())
        );
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entities[3],
                Vec2::new(115.0, 105.0),
                0.0,
                box_records(15.0, 15.0),
                false,
            )
            .unwrap();
        layer.unstamp(&grid, geometry, extent, entities[1]);
        layer.unstamp(&grid, geometry, extent, entities[2]);
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entities[4],
                Vec2::new(115.0, 105.0),
                0.0,
                box_records(30.0, 30.0),
                false,
            )
            .unwrap();
        assert_eq!(
            layer.rebuild(geometry, extent),
            (layer.counts.clone(), layer.rubble.clone())
        );
        for entity in entities {
            layer.unstamp(&grid, geometry, extent, entity);
        }
        assert!(layer.counts.iter().all(|count| *count == 0));
        assert_eq!(layer.len(), 0);
    }

    #[test]
    fn unstamping_clears_exactly_the_cells_the_stamp_covered() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut ids = World::new();
        let entity = ids.spawn().unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                entity,
                Vec2::new(105.0, 105.0),
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        let index =
            usize::try_from(extent.index(mint_movement::Cell::new(10, 10)).unwrap()).unwrap();
        assert!(layer.blocked(index));
        layer.unstamp(&grid, geometry, extent, entity);
        assert!(!layer.blocked(index));
    }

    #[test]
    fn a_stamp_for_a_reused_entity_index_replaces_the_stale_one() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut world = World::new();
        let centre = Vec2::new(105.0, 105.0);
        let gone = world.spawn().unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                gone,
                centre,
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        world.despawn(gone);
        let reused = world.spawn().unwrap();
        assert_eq!(reused.index(), gone.index(), "the index is reused");
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                reused,
                centre,
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        assert_eq!(layer.len(), 1);
        layer.unstamp(&grid, geometry, extent, reused);
        assert!(layer.counts.iter().all(|count| *count == 0));
    }

    #[test]
    fn overlap_survives_one_of_the_two_structures_dying() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut ids = World::new();
        let a = ids.spawn().unwrap();
        let b = ids.spawn().unwrap();
        let centre = Vec2::new(105.0, 105.0);
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                a,
                centre,
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                b,
                centre,
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        let index =
            usize::try_from(extent.index(mint_movement::Cell::new(10, 10)).unwrap()).unwrap();
        assert!(layer.blocked(index));
        layer.unstamp(&grid, geometry, extent, a);
        assert!(layer.blocked(index), "b still stands here");
        layer.unstamp(&grid, geometry, extent, b);
        assert!(!layer.blocked(index));
    }

    #[test]
    fn a_stamp_that_would_overflow_a_cell_is_refused_and_nothing_it_touched_changes() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut ids = World::new();
        let centre = Vec2::new(105.0, 105.0);
        for _ in 0..255 {
            let entity = ids.spawn().unwrap();
            layer
                .stamp(
                    &grid,
                    geometry,
                    extent,
                    entity,
                    centre,
                    0.0,
                    box_records(5.0, 5.0),
                    false,
                )
                .unwrap();
        }
        let before = layer.counts.clone();
        let one_too_many = ids.spawn().unwrap();
        let error = layer
            .stamp(
                &grid,
                geometry,
                extent,
                one_too_many,
                centre,
                0.0,
                box_records(5.0, 5.0),
                false,
            )
            .unwrap_err();
        assert!(error.contains(&one_too_many.index().to_string()));
        assert_eq!(layer.counts, before, "a refused stamp must change nothing");
        assert_eq!(layer.len(), 255, "the refused stamp must not be recorded");
    }

    #[test]
    fn a_scaleable_wall_stamp_carries_the_scaleable_bit_and_unstamping_clears_it() {
        let (geometry, extent, grid) = geometry_and_extent();
        let mut layer = ObstacleLayer::empty(usize::try_from(extent.cell_count()).unwrap());
        let mut ids = World::new();
        let wall = ids.spawn().unwrap();
        let plain = ids.spawn().unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                wall,
                Vec2::new(105.0, 105.0),
                0.0,
                box_records(20.0, 20.0),
                true,
            )
            .unwrap();
        layer
            .stamp(
                &grid,
                geometry,
                extent,
                plain,
                Vec2::new(305.0, 305.0),
                0.0,
                box_records(20.0, 20.0),
                false,
            )
            .unwrap();
        let wall_index =
            usize::try_from(extent.index(mint_movement::Cell::new(10, 10)).unwrap()).unwrap();
        let plain_index =
            usize::try_from(extent.index(mint_movement::Cell::new(30, 30)).unwrap()).unwrap();
        assert!(
            layer.scaleable()[wall_index] > 0,
            "the SCALEABLE_WALL stamp's own cell must carry the scaleable bit"
        );
        assert_eq!(
            layer.scaleable()[plain_index],
            0,
            "an ordinary stamp's cell must not carry it"
        );
        layer.unstamp(&grid, geometry, extent, wall);
        assert_eq!(
            layer.scaleable()[wall_index],
            0,
            "unstamping must clear the scaleable bit along with the obstacle"
        );
    }
}
