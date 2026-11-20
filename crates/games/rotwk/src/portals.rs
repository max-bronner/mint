use mint_movement::PortalLink;
use mint_sim::{Component, ComponentId, Entity, StateHasher, World};

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PortalLayer {
    registered: Vec<LinkSet>,
    links: Vec<PortalLink>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinkSet {
    pub(crate) entity: Entity,
    pub(crate) links: Vec<PortalLink>,
}

impl PortalLayer {
    pub(crate) fn links(&self) -> &[PortalLink] {
        &self.links
    }

    pub(crate) fn slots(&self) -> Vec<(u8, Entity)> {
        let mut slots: Vec<(u8, Entity)> = self
            .registered
            .iter()
            .flat_map(|set| set.links.iter().map(|link| (link.slot, set.entity)))
            .collect();
        slots.sort_unstable_by_key(|(slot, _)| *slot);
        slots
    }

    pub(crate) fn register(&mut self, entity: Entity, mut links: Vec<PortalLink>) {
        self.unregister(entity);
        let mut taken = [false; mint_movement::PortalPass::SLOTS as usize];
        for set in &self.registered {
            for link in &set.links {
                if let Some(held) = taken.get_mut(usize::from(link.slot)) {
                    *held = true;
                }
            }
        }
        let mut next = 0usize;
        links.retain_mut(|link| {
            while taken.get(next).copied().unwrap_or(false) {
                next = next.saturating_add(1);
            }
            let Ok(slot) = u8::try_from(next) else {
                return false;
            };
            if slot >= mint_movement::PortalPass::SLOTS {
                return false;
            }
            link.slot = slot;
            if let Some(held) = taken.get_mut(next) {
                *held = true;
            }
            true
        });
        let at = self
            .registered
            .binary_search_by_key(&entity.index(), |held| held.entity.index())
            .unwrap_or_else(|at| at);
        self.registered.insert(at, LinkSet { entity, links });
        self.relink();
    }

    pub(crate) fn unregister(&mut self, entity: Entity) -> bool {
        let before = self.registered.len();
        self.registered
            .retain(|set| set.entity.index() != entity.index());
        let changed = self.registered.len() != before;
        if changed {
            self.relink();
        }
        changed
    }

    pub(crate) fn holds(&self, entity: Entity) -> bool {
        self.registered
            .iter()
            .any(|set| set.entity.index() == entity.index())
    }

    fn relink(&mut self) {
        self.links = self
            .registered
            .iter()
            .flat_map(|set| set.links.iter().cloned())
            .collect();
        self.links.sort_by_key(|link| (link.entry, link.slot));
    }

    fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.registered.iter().map(|set| set.entity)
    }
}

impl Component for PortalLayer {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.registered.len());
        for set in &self.registered {
            set.entity.hash_into(hasher);
            hasher.write_usize(set.links.len());
            for link in &set.links {
                hasher.write_u8(link.slot);
                hasher.write_usize(link.nodes.len());
                for node in &link.nodes {
                    hasher.write_f32(node.position.x);
                    hasher.write_f32(node.position.y);
                }
            }
        }
    }
}

pub(crate) fn layer_in(world: &World, id: ComponentId<PortalLayer>) -> Option<&PortalLayer> {
    world
        .store(id)
        .and_then(|store| store.entities().first().copied())
        .and_then(|entity| world.get(id, entity))
}

pub(crate) fn links_of(world: &World, id: ComponentId<PortalLayer>) -> &[PortalLink] {
    layer_in(world, id).map_or(&[], PortalLayer::links)
}

pub(crate) fn reap_dead_portals(world: &mut World, id: ComponentId<PortalLayer>) {
    let Some(clock_entity) = world
        .store(id)
        .and_then(|store| store.entities().first().copied())
    else {
        return;
    };
    let dead: Vec<Entity> = world
        .get(id, clock_entity)
        .map(|layer| {
            layer
                .entities()
                .filter(|entity| !world.is_live(*entity))
                .collect()
        })
        .unwrap_or_default();
    if dead.is_empty() {
        return;
    }
    if let Some(layer) = world.get_mut(id, clock_entity) {
        for entity in dead {
            layer.unregister(entity);
        }
    }
}
