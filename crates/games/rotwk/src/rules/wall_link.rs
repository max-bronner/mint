use mint_sim::{Component, Entity, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct WallLinks {
    pub(crate) producer: Option<Entity>,
    pub(crate) linked: Vec<Entity>,
}

impl Component for WallLinks {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.producer.is_some());
        if let Some(producer) = self.producer {
            producer.hash_into(hasher);
        }
        hasher.write_u32(u32::try_from(self.linked.len()).unwrap_or(u32::MAX));
        for entity in &self.linked {
            entity.hash_into(hasher);
        }
    }
}

impl RotwkRules {
    fn wall_kind(&self, world: &World, ids: Ids, entity: Entity, bits: &[&str]) -> bool {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| bits.iter().any(|bit| entry.record.has_kind(bit)))
    }

    pub(crate) fn hubs_linked(&self, world: &World, ids: Ids, a: Entity, b: Entity) -> bool {
        if !self.wall_kind(world, ids, a, &["WALL_HUB"])
            || !self.wall_kind(world, ids, b, &["WALL_HUB"])
        {
            return false;
        }
        let points_at = |from: Entity, to: Entity| {
            world
                .get(ids.wall_links, from)
                .is_some_and(|links| links.producer == Some(to) || links.linked.contains(&to))
        };
        points_at(a, b) || points_at(b, a)
    }

    pub(crate) fn link_add(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        to: Entity,
        other: Entity,
    ) -> Result<()> {
        if to == other {
            return Ok(());
        }
        let has_list = tick
            .state()
            .get(ids.kind, to)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.spawn_timer.is_some());
        if !has_list {
            return Ok(());
        }
        let mut links = tick
            .state()
            .get(ids.wall_links, to)
            .cloned()
            .unwrap_or_default();
        if !links.linked.contains(&other) {
            links.linked.push(other);
            tick.state_mut().insert(ids.wall_links, to, links)?;
        }
        Ok(())
    }

    pub(super) fn write_span_links(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        start: Entity,
        target: Option<Entity>,
        created: &[Entity],
    ) -> Result<()> {
        if let Some(target) = target {
            self.link_add(tick, ids, target, start)?;
            self.link_add(tick, ids, start, target)?;
            for &piece in created {
                self.link_add(tick, ids, start, piece)?;
            }
        }
        for &piece in created {
            if let Some(target) = target {
                self.link_add(tick, ids, piece, target)?;
            }
            self.link_add(tick, ids, piece, start)?;
            for &other in created {
                self.link_add(tick, ids, piece, other)?;
            }
        }
        for &piece in created {
            self.link_add(tick, ids, start, piece)?;
        }
        Ok(())
    }

    pub(super) fn links_to_carry(
        &self,
        world: &World,
        ids: Ids,
        old: Entity,
    ) -> Option<(Vec<Entity>, Vec<Entity>)> {
        const WALL: [&str; 3] = ["WALL_HUB", "WALL_UPGRADE", "WALL_SEGMENT"];
        if !self.wall_kind(world, ids, old, &WALL) {
            return None;
        }
        let store = world.store(ids.wall_links)?;
        let holders = store
            .iter()
            .filter(|(entity, links)| {
                *entity != old
                    && links.linked.contains(&old)
                    && self.wall_kind(world, ids, *entity, &WALL)
            })
            .map(|(entity, _)| entity)
            .collect();
        let listed = world
            .get(ids.wall_links, old)
            .map(|links| {
                links
                    .linked
                    .iter()
                    .copied()
                    .filter(|entity| {
                        world.is_live(*entity) && self.wall_kind(world, ids, *entity, &WALL)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some((holders, listed))
    }

    pub(super) fn carry_links(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        new: Entity,
        carried: (Vec<Entity>, Vec<Entity>),
    ) -> Result<()> {
        let (holders, listed) = carried;
        for holder in holders {
            self.link_add(tick, ids, holder, new)?;
        }
        for other in listed {
            self.link_add(tick, ids, new, other)?;
        }
        Ok(())
    }
}
