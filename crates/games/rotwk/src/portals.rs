use mint_movement::{CellGeometry, GridExtent, Vec2};
use mint_sage_passability::{covered_cells, FootprintRecord, Obstacle};
use mint_sim::{Component, ComponentId, Entity, StateHasher, World};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PortalStamp {
    pub(crate) entity: Entity,
    pub(crate) centre: Vec2,
    pub(crate) angle: f32,
    pub(crate) records: Vec<FootprintRecord>,
    pub(crate) admitted: u32,
    pub(crate) kind: u8,
}

impl PortalStamp {
    fn as_obstacle(&self) -> Obstacle {
        Obstacle {
            id: 0,
            centre: self.centre,
            angle: self.angle,
            records: self.records.clone(),
        }
    }
}

fn pack_tag(entity: Entity, kind: u8) -> u32 {
    let id = entity.index().wrapping_add(1) & 0x00FF_FFFF;
    (u32::from(kind) << 24) | id
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PortalLayer {
    masks: Vec<u32>,
    tags: Vec<u32>,
    stamps: Vec<PortalStamp>,
}

impl PortalLayer {
    pub(crate) fn empty(cell_count: usize) -> Self {
        Self {
            masks: vec![0; cell_count],
            tags: vec![0; cell_count],
            stamps: Vec::new(),
        }
    }

    pub(crate) fn masks(&self) -> &[u32] {
        &self.masks
    }

    pub(crate) fn tags(&self) -> &[u32] {
        &self.tags
    }

    pub(crate) fn entities(&self) -> Vec<Entity> {
        self.stamps.iter().map(|stamp| stamp.entity).collect()
    }

    pub(crate) fn stamp(&mut self, geometry: CellGeometry, extent: GridExtent, stamp: PortalStamp) {
        self.remove_stamp(stamp.entity);
        let at = self
            .stamps
            .binary_search_by_key(&stamp.entity.index(), |held| held.entity.index())
            .unwrap_or_else(|at| at);
        self.stamps.insert(at, stamp);
        self.rebuild(geometry, extent);
    }

    pub(crate) fn unstamp(&mut self, geometry: CellGeometry, extent: GridExtent, entity: Entity) {
        if self.remove_stamp(entity) {
            self.rebuild(geometry, extent);
        }
    }

    fn remove_stamp(&mut self, entity: Entity) -> bool {
        let before = self.stamps.len();
        self.stamps
            .retain(|stamp| stamp.entity.index() != entity.index());
        self.stamps.len() != before
    }

    fn rebuild(&mut self, geometry: CellGeometry, extent: GridExtent) {
        self.masks.iter_mut().for_each(|mask| *mask = 0);
        self.tags.iter_mut().for_each(|tag| *tag = 0);
        for stamp in &self.stamps {
            let tag = pack_tag(stamp.entity, stamp.kind);
            for cell in covered_cells(geometry, extent, &stamp.as_obstacle()) {
                let Some(index) = extent.index(cell).and_then(|i| usize::try_from(i).ok()) else {
                    continue;
                };
                if let Some(mask) = self.masks.get_mut(index) {
                    *mask |= stamp.admitted;
                }
                if let Some(slot) = self.tags.get_mut(index) {
                    *slot = tag;
                }
            }
        }
    }
}

impl Component for PortalLayer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.stamps.len());
        for stamp in &self.stamps {
            stamp.entity.hash_into(hasher);
            hasher.write_f32(stamp.centre.x);
            hasher.write_f32(stamp.centre.y);
            hasher.write_f32(stamp.angle);
            hasher.write_u32(stamp.admitted);
            hasher.write_u8(stamp.kind);
            hasher.write_usize(stamp.records.len());
        }
    }
}

pub(crate) fn masks_of(world: &World, id: ComponentId<PortalLayer>) -> &[u32] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[], PortalLayer::masks)
}

pub(crate) fn tags_of(world: &World, id: ComponentId<PortalLayer>) -> &[u32] {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
        .map_or(&[], PortalLayer::tags)
}

pub(crate) fn reap_dead_portals(
    world: &mut World,
    id: ComponentId<PortalLayer>,
    geometry: CellGeometry,
    extent: GridExtent,
) {
    let Some(clock_entity) = world
        .store(id)
        .and_then(|store| store.entities().first().copied())
    else {
        return;
    };
    let Some(mut layer) = world.get(id, clock_entity).cloned() else {
        return;
    };
    let dead: Vec<Entity> = layer
        .entities()
        .into_iter()
        .filter(|entity| !world.is_live(*entity))
        .collect();
    if dead.is_empty() {
        return;
    }
    for entity in dead {
        layer.unstamp(geometry, extent, entity);
    }
    let _ = world.insert(id, clock_entity, layer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_index_zero_with_kind_zero_still_packs_to_a_nonzero_tag() {
        let entity = Entity::from_parts(0, 0);
        assert_ne!(pack_tag(entity, 0), 0);
    }
}
