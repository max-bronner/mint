use crate::hash::StateHasher;
use crate::to_usize;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub const NONE: Self = Self {
        index: u32::MAX,
        generation: 0,
    };

    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u32(self.index);
        hasher.write_u32(self.generation);
    }

    #[cfg(test)]
    pub(crate) const fn raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[must_use]
    pub const fn from_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    generation: u32,
    live: bool,
}

#[derive(Debug, Default, Clone)]
pub struct EntityAllocator {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
}

impl EntityAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    pub fn spawn(&mut self) -> Result<Entity> {
        while let Some(index) = self.free.pop() {
            let Some(slot) = self.slots.get_mut(to_usize(index)) else {
                continue;
            };
            let Some(generation) = slot.generation.checked_add(1) else {
                continue;
            };
            slot.generation = generation;
            slot.live = true;
            self.live = self.live.saturating_add(1);
            return Ok(Entity { index, generation });
        }

        let index = u32::try_from(self.slots.len()).map_err(|_| Error::EntitySpaceExhausted)?;
        self.slots.push(Slot {
            generation: 1,
            live: true,
        });
        self.live = self.live.saturating_add(1);
        Ok(Entity {
            index,
            generation: 1,
        })
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(slot) = self.slots.get_mut(to_usize(entity.index)) else {
            return false;
        };
        if !slot.live || slot.generation != entity.generation {
            return false;
        }
        slot.live = false;
        self.live = self.live.saturating_sub(1);
        if slot.generation != u32::MAX {
            self.free.push(entity.index);
        }
        true
    }

    #[must_use]
    pub fn is_live(&self, entity: Entity) -> bool {
        self.slots
            .get(to_usize(entity.index))
            .is_some_and(|slot| slot.live && slot.generation == entity.generation)
    }

    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            if !slot.live {
                return None;
            }
            Some(Entity {
                index: u32::try_from(index).ok()?,
                generation: slot.generation,
            })
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.live
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.live == 0
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    #[cfg(test)]
    fn force_generation(&mut self, index: u32, generation: u32) -> bool {
        match self.slots.get_mut(to_usize(index)) {
            Some(slot) => {
                slot.generation = generation;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn_n(allocator: &mut EntityAllocator, count: usize) -> Vec<Entity> {
        (0..count).map(|_| allocator.spawn().unwrap()).collect()
    }

    #[test]
    fn fresh_slots_are_numbered_from_zero_at_generation_one() {
        let mut allocator = EntityAllocator::new();
        let entities = spawn_n(&mut allocator, 3);
        assert_eq!(
            entities,
            vec![Entity::raw(0, 1), Entity::raw(1, 1), Entity::raw(2, 1)]
        );
        assert_eq!(allocator.len(), 3);
    }

    #[test]
    fn ordering_is_lexicographic_on_index_then_generation() {
        assert!(Entity::raw(0, 9) < Entity::raw(1, 1));
        assert!(Entity::raw(1, 1) < Entity::raw(1, 2));
        let mut sorted = vec![Entity::raw(2, 1), Entity::raw(1, 7), Entity::raw(1, 2)];
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            vec![Entity::raw(1, 2), Entity::raw(1, 7), Entity::raw(2, 1)]
        );
    }

    #[test]
    fn a_despawned_handle_stays_stale_after_its_slot_is_reused() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.spawn().unwrap();
        assert!(allocator.is_live(first));
        assert!(allocator.despawn(first));
        assert!(!allocator.is_live(first));

        let second = allocator.spawn().unwrap();
        assert_eq!(second.index(), first.index());
        assert_eq!(second.generation(), first.generation().wrapping_add(1));
        assert!(allocator.is_live(second));
        assert!(!allocator.is_live(first));
        assert!(!allocator.despawn(first));
        assert!(allocator.is_live(second));
    }

    #[test]
    fn free_slots_are_reused_last_in_first_out() {
        let mut allocator = EntityAllocator::new();
        let entities = spawn_n(&mut allocator, 4);

        assert!(allocator.despawn(entities[0]));
        assert!(allocator.despawn(entities[1]));
        assert!(allocator.despawn(entities[2]));
        assert_eq!(allocator.spawn().unwrap(), Entity::raw(2, 2));
        assert_eq!(allocator.spawn().unwrap(), Entity::raw(1, 2));
        assert_eq!(allocator.spawn().unwrap(), Entity::raw(0, 2));
        assert_eq!(allocator.spawn().unwrap(), Entity::raw(4, 1));
    }

    #[test]
    fn the_same_operation_sequence_yields_the_same_handles() {
        fn run() -> Vec<Entity> {
            let mut allocator = EntityAllocator::new();
            let mut issued = Vec::new();
            let mut live: Vec<Entity> = Vec::new();
            for _ in 0..8 {
                for _ in 0..2 {
                    let entity = allocator.spawn().unwrap();
                    issued.push(entity);
                    live.push(entity);
                }
                let victim = live.remove(0);
                assert!(allocator.despawn(victim));
                let entity = allocator.spawn().unwrap();
                issued.push(entity);
                live.push(entity);
            }
            issued
        }
        assert_eq!(run(), run());
        assert!(run().iter().any(|e| e.generation() > 1));
    }

    #[test]
    fn live_iteration_is_ascending_and_skips_the_dead() {
        let mut allocator = EntityAllocator::new();
        let entities = spawn_n(&mut allocator, 5);
        assert!(allocator.despawn(entities[1]));
        assert!(allocator.despawn(entities[3]));
        let live: Vec<Entity> = allocator.iter().collect();
        assert_eq!(live, vec![entities[0], entities[2], entities[4]]);
        assert_eq!(allocator.len(), 3);
    }

    #[test]
    fn an_exhausted_slot_is_retired_rather_than_wrapped() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.spawn().unwrap();
        assert!(allocator.force_generation(first.index(), u32::MAX));
        let last = Entity::raw(first.index(), u32::MAX);
        assert!(allocator.is_live(last));

        assert!(allocator.despawn(last));
        let next = allocator.spawn().unwrap();
        assert_eq!(next, Entity::raw(1, 1));
        assert!(!allocator.is_live(last));
        assert!(!allocator.is_live(Entity::raw(0, 1)));
        assert_eq!(allocator.capacity(), 2);
        assert_eq!(allocator.len(), 1);
    }

    #[test]
    fn despawn_rejects_a_handle_from_a_previous_generation() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.spawn().unwrap();
        assert!(allocator.despawn(first));
        let second = allocator.spawn().unwrap();
        assert!(!allocator.despawn(first));
        assert!(allocator.is_live(second));
        assert_eq!(allocator.spawn().unwrap(), Entity::raw(1, 1));
    }

    #[test]
    fn an_out_of_range_handle_is_simply_not_live() {
        let allocator = EntityAllocator::new();
        assert!(!allocator.is_live(Entity::raw(9_999, 1)));
        assert!(allocator.is_empty());
    }
}
