use core::any::{Any, TypeId};
use core::fmt;
use core::marker::PhantomData;

use crate::entity::{Entity, EntityAllocator};
use crate::hash::StateHasher;
use crate::store::ComponentStore;
use crate::{Error, Result};

pub trait Component: 'static + Clone {
    fn hash_into(&self, hasher: &mut StateHasher);

    fn hash_into_cached(&mut self, hasher: &mut StateHasher) {
        self.hash_into(hasher);
    }
}

pub struct ComponentId<T> {
    index: usize,
    marker: PhantomData<fn() -> T>,
}

impl<T> ComponentId<T> {
    const fn new(index: usize) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }
}

impl<T> Clone for ComponentId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ComponentId<T> {}

impl<T> PartialEq for ComponentId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for ComponentId<T> {}

impl<T> fmt::Debug for ComponentId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ComponentId({})", self.index)
    }
}

trait ErasedStore: Any {
    fn remove_entity(&mut self, entity: Entity);
    fn remove_entities(&mut self, entities: &[Entity]);
    fn hash_full(&self) -> u64;
    fn hash_cached(&mut self) -> u64;
    fn hash_legacy(&self, hasher: &mut StateHasher);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn boxed_clone(&self) -> Box<dyn ErasedStore>;
}

impl<T: Component> ErasedStore for ComponentStore<T> {
    fn remove_entity(&mut self, entity: Entity) {
        self.remove(entity);
    }

    fn remove_entities(&mut self, entities: &[Entity]) {
        self.remove_many(entities);
    }

    fn hash_full(&self) -> u64 {
        ComponentStore::hash_full(self, Component::hash_into)
    }

    fn hash_cached(&mut self) -> u64 {
        ComponentStore::hash_cached(self, Component::hash_into_cached)
    }

    fn hash_legacy(&self, hasher: &mut StateHasher) {
        ComponentStore::hash_legacy(self, hasher, Component::hash_into);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn boxed_clone(&self) -> Box<dyn ErasedStore> {
        Box::new(self.clone())
    }
}

struct Registration {
    type_id: TypeId,
    store: Box<dyn ErasedStore>,
}

impl Clone for Registration {
    fn clone(&self) -> Self {
        Self {
            type_id: self.type_id,
            store: self.store.boxed_clone(),
        }
    }
}

pub struct World {
    entities: EntityAllocator,
    registrations: Vec<Registration>,
}

impl World {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entities: EntityAllocator::new(),
            registrations: Vec::new(),
        }
    }

    pub fn register<T: Component>(&mut self) -> ComponentId<T> {
        let type_id = TypeId::of::<T>();
        if let Some(index) = self
            .registrations
            .iter()
            .position(|registration| registration.type_id == type_id)
        {
            return ComponentId::new(index);
        }
        let index = self.registrations.len();
        self.registrations.push(Registration {
            type_id,
            store: Box::new(ComponentStore::<T>::new()),
        });
        ComponentId::new(index)
    }

    #[must_use]
    pub fn component_count(&self) -> usize {
        self.registrations.len()
    }

    pub fn spawn(&mut self) -> Result<Entity> {
        self.entities.spawn()
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.entities.despawn(entity) {
            return false;
        }
        for registration in &mut self.registrations {
            registration.store.remove_entity(entity);
        }
        true
    }

    pub fn despawn_many(&mut self, entities: &[Entity]) -> usize {
        let live: Vec<Entity> = entities
            .iter()
            .copied()
            .filter(|entity| self.entities.despawn(*entity))
            .collect();
        if live.is_empty() {
            return 0;
        }
        for registration in &mut self.registrations {
            registration.store.remove_entities(&live);
        }
        live.len()
    }

    #[must_use]
    pub fn is_live(&self, entity: Entity) -> bool {
        self.entities.is_live(entity)
    }

    pub fn iter_entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.entities.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    #[must_use]
    pub fn store<T: Component>(&self, id: ComponentId<T>) -> Option<&ComponentStore<T>> {
        self.registrations
            .get(id.index)?
            .store
            .as_any()
            .downcast_ref()
    }

    #[must_use]
    pub fn store_mut<T: Component>(
        &mut self,
        id: ComponentId<T>,
    ) -> Option<&mut ComponentStore<T>> {
        self.registrations
            .get_mut(id.index)?
            .store
            .as_any_mut()
            .downcast_mut()
    }

    #[must_use]
    pub fn stores_mut<A: Component, B: Component>(
        &mut self,
        mut_id: ComponentId<A>,
        ref_id: ComponentId<B>,
    ) -> Option<(&mut ComponentStore<A>, &ComponentStore<B>)> {
        let (m, r) = (mut_id.index, ref_id.index);
        if m == r {
            return None;
        }
        let (lo, hi) = if m < r { (m, r) } else { (r, m) };
        if hi >= self.registrations.len() {
            return None;
        }
        let (left, right) = self.registrations.split_at_mut(hi);
        let lo_store = &mut left.get_mut(lo)?.store;
        let hi_store = &mut right.first_mut()?.store;
        if m < r {
            let a = lo_store.as_any_mut().downcast_mut::<ComponentStore<A>>()?;
            let b = hi_store.as_any().downcast_ref::<ComponentStore<B>>()?;
            Some((a, b))
        } else {
            let b = lo_store.as_any().downcast_ref::<ComponentStore<B>>()?;
            let a = hi_store.as_any_mut().downcast_mut::<ComponentStore<A>>()?;
            Some((a, b))
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            entities: self.entities.clone(),
            registrations: self.registrations.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: &WorldSnapshot) {
        self.entities = snapshot.entities.clone();
        self.registrations = snapshot.registrations.clone();
    }

    pub fn insert<T: Component>(
        &mut self,
        id: ComponentId<T>,
        entity: Entity,
        value: T,
    ) -> Result<Option<T>> {
        if !self.entities.is_live(entity) {
            return Err(Error::StaleEntity {
                index: entity.index(),
                generation: entity.generation(),
            });
        }
        let store = self
            .store_mut(id)
            .ok_or(Error::UnknownComponent { index: id.index })?;
        Ok(store.insert(entity, value))
    }

    pub fn remove<T: Component>(&mut self, id: ComponentId<T>, entity: Entity) -> Option<T> {
        self.store_mut(id)?.remove(entity)
    }

    #[must_use]
    pub fn get<T: Component>(&self, id: ComponentId<T>, entity: Entity) -> Option<&T> {
        self.store(id)?.get(entity)
    }

    #[must_use]
    pub fn get_mut<T: Component>(&mut self, id: ComponentId<T>, entity: Entity) -> Option<&mut T> {
        self.store_mut(id)?.get_mut(entity)
    }

    #[must_use]
    pub fn contains<T: Component>(&self, id: ComponentId<T>, entity: Entity) -> bool {
        self.store(id).is_some_and(|store| store.contains(entity))
    }

    #[must_use]
    pub fn state_hash(&self) -> u64 {
        self.hash_with(|store| store.hash_full())
    }

    pub fn state_hash_incremental(&mut self) -> u64 {
        let mut hasher = StateHasher::new();
        self.hash_entities(&mut hasher);
        for registration in &mut self.registrations {
            hasher.write_u64(registration.store.hash_cached());
        }
        hasher.finish()
    }

    #[must_use]
    pub fn legacy_state_hash(&self) -> u64 {
        let mut hasher = StateHasher::legacy();
        self.hash_entities(&mut hasher);
        for registration in &self.registrations {
            registration.store.hash_legacy(&mut hasher);
        }
        hasher.finish()
    }

    fn hash_with(&self, mut store_hash: impl FnMut(&dyn ErasedStore) -> u64) -> u64 {
        let mut hasher = StateHasher::new();
        self.hash_entities(&mut hasher);
        for registration in &self.registrations {
            hasher.write_u64(store_hash(registration.store.as_ref()));
        }
        hasher.finish()
    }

    fn hash_entities(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.entities.len());
        for entity in self.entities.iter() {
            entity.hash_into(hasher);
        }
        hasher.write_usize(self.registrations.len());
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WorldSnapshot {
    entities: EntityAllocator,
    registrations: Vec<Registration>,
}

impl fmt::Debug for World {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("World")
            .field("entities", &self.entities.len())
            .field("components", &self.registrations.len())
            .finish()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod hash_tests;
