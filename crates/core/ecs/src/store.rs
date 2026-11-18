use crate::entity::Entity;
use crate::hash::StateHasher;
use crate::pages::{page_of, DirtyPages, WriteStamps, PAGE_SHIFT};
use crate::to_usize;

pub const PAGE_SLOTS: u32 = 1 << PAGE_SHIFT;

const VACANT: usize = usize::MAX;

#[derive(Debug, Clone)]
struct PageHashes {
    pages: Vec<Option<u64>>,
    dirty: DirtyPages,
    store: u64,
    stale: bool,
    stamps: WriteStamps,
}

impl PageHashes {
    const fn new() -> Self {
        Self {
            pages: Vec::new(),
            dirty: DirtyPages::new(),
            store: 0,
            stale: true,
            stamps: WriteStamps::new(),
        }
    }

    fn mark(&mut self, index: u32) {
        self.dirty.mark(page_of(index));
        self.stamps.mark(page_of(index));
        self.stale = true;
    }
}

#[derive(Debug, Clone)]
pub struct ComponentStore<T> {
    entities: Vec<Entity>,
    values: Vec<T>,
    sparse: Vec<usize>,
    hashes: PageHashes,
}

impl<T> ComponentStore<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entities: Vec::new(),
            values: Vec::new(),
            sparse: Vec::new(),
            hashes: PageHashes::new(),
        }
    }

    fn dense_of(&self, entity: Entity) -> Option<usize> {
        let dense = *self.sparse.get(to_usize(entity.index()))?;
        if dense == VACANT {
            return None;
        }
        if *self.entities.get(dense)? != entity {
            return None;
        }
        Some(dense)
    }

    fn reindex_from(&mut self, from: usize) {
        for (dense, entity) in self.entities.iter().enumerate().skip(from) {
            if let Some(slot) = self.sparse.get_mut(to_usize(entity.index())) {
                *slot = dense;
            }
        }
    }

    pub fn insert(&mut self, entity: Entity, value: T) -> Option<T> {
        self.hashes.mark(entity.index());
        let slot = to_usize(entity.index());
        if slot >= self.sparse.len() {
            self.sparse.resize(slot.saturating_add(1), VACANT);
        }

        let occupied = self.sparse.get(slot).copied().unwrap_or(VACANT);
        if occupied != VACANT {
            if let (Some(existing), Some(stored)) = (
                self.entities.get_mut(occupied),
                self.values.get_mut(occupied),
            ) {
                let replaced = core::mem::replace(stored, value);
                let was_same_entity = *existing == entity;
                *existing = entity;
                return was_same_entity.then_some(replaced);
            }
            return None;
        }

        let position = self
            .entities
            .partition_point(|existing| existing.index() < entity.index());
        self.entities.insert(position, entity);
        self.values.insert(position, value);
        self.reindex_from(position);
        None
    }

    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let dense = self.dense_of(entity)?;
        self.hashes.mark(entity.index());
        self.entities.remove(dense);
        let value = self.values.remove(dense);
        if let Some(slot) = self.sparse.get_mut(to_usize(entity.index())) {
            *slot = VACANT;
        }
        self.reindex_from(dense);
        Some(value)
    }

    pub fn remove_many(&mut self, entities: &[Entity]) -> usize {
        let mut doomed: Vec<usize> = entities
            .iter()
            .filter_map(|entity| self.dense_of(*entity))
            .collect();
        if doomed.is_empty() {
            return 0;
        }
        doomed.sort_unstable();
        doomed.dedup();
        for &dense in &doomed {
            if let Some(entity) = self.entities.get(dense) {
                self.hashes.mark(entity.index());
                if let Some(slot) = self.sparse.get_mut(to_usize(entity.index())) {
                    *slot = VACANT;
                }
            }
        }
        let first = doomed.first().copied().unwrap_or(0);
        let doomed_at = |position: usize| doomed.binary_search(&position).is_ok();
        let mut position = 0usize;
        self.entities.retain(|_| {
            let keep = !doomed_at(position);
            position = position.saturating_add(1);
            keep
        });
        let mut position = 0usize;
        self.values.retain(|_| {
            let keep = !doomed_at(position);
            position = position.saturating_add(1);
            keep
        });
        self.reindex_from(first);
        doomed.len()
    }

    #[must_use]
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.values.get(self.dense_of(entity)?)
    }

    #[must_use]
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let dense = self.dense_of(entity)?;
        self.hashes.mark(entity.index());
        self.values.get_mut(dense)
    }

    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.dense_of(entity).is_some()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        self.entities.iter().copied().zip(self.values.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> + '_ {
        let hashes = &mut self.hashes;
        self.entities
            .iter()
            .copied()
            .zip(self.values.iter_mut())
            .map(move |(entity, value)| {
                hashes.mark(entity.index());
                (entity, value)
            })
    }

    pub fn iter_mut_where<'a, P>(
        &'a mut self,
        mut wanted: P,
    ) -> impl Iterator<Item = (Entity, &'a mut T)> + 'a
    where
        P: FnMut(&T) -> bool + 'a,
    {
        let hashes = &mut self.hashes;
        self.entities
            .iter()
            .copied()
            .zip(self.values.iter_mut())
            .filter(move |(_, value)| wanted(value))
            .map(move |(entity, value)| {
                hashes.mark(entity.index());
                (entity, value)
            })
    }

    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    #[must_use]
    pub fn writes(&self) -> u64 {
        self.hashes.stamps.count()
    }

    pub fn pages_written_since(&self, since: u64) -> impl Iterator<Item = u32> + '_ {
        self.hashes
            .stamps
            .since(since)
            .map(|page| page.wrapping_shl(PAGE_SHIFT))
    }

    #[must_use]
    pub fn from_sorted(pairs: Vec<(Entity, T)>) -> Option<Self> {
        for pair in pairs.windows(2) {
            let (Some((prev, _)), Some((next, _))) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if prev.index() >= next.index() {
                return None;
            }
        }

        let mut sparse = Vec::new();
        let mut entities = Vec::with_capacity(pairs.len());
        let mut values = Vec::with_capacity(pairs.len());
        let mut hashes = PageHashes::new();
        for (dense, (entity, value)) in pairs.into_iter().enumerate() {
            hashes.mark(entity.index());
            let slot = to_usize(entity.index());
            if slot >= sparse.len() {
                sparse.resize(slot.saturating_add(1), VACANT);
            }
            if let Some(cell) = sparse.get_mut(slot) {
                *cell = dense;
            }
            entities.push(entity);
            values.push(value);
        }
        Some(Self {
            entities,
            values,
            sparse,
            hashes,
        })
    }

    pub fn clear(&mut self) {
        self.entities.clear();
        self.values.clear();
        for slot in &mut self.sparse {
            *slot = VACANT;
        }
        let mut stamps = core::mem::take(&mut self.hashes.stamps);
        stamps.clear_all();
        self.hashes = PageHashes::new();
        self.hashes.stamps = stamps;
    }
}

impl<T> Default for ComponentStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ComponentStore<T> {
    pub(crate) fn hash_full<F>(&self, mut hash_value: F) -> u64
    where
        F: FnMut(&T, &mut StateHasher),
    {
        let mut hasher = StateHasher::new();
        hasher.write_usize(self.len());
        let mut start = 0usize;
        while let Some(first) = self.entities.get(start) {
            let page = page_of(first.index());
            let rest = self.entities.get(start..).unwrap_or(&[]);
            let end =
                start.saturating_add(rest.partition_point(|e| page_of(e.index()) == page).max(1));
            let mut page_hasher = StateHasher::new();
            let entities = self.entities.get(start..end).unwrap_or(&[]);
            let values = self.values.get(start..end).unwrap_or(&[]);
            for (entity, value) in entities.iter().zip(values) {
                entity.hash_into(&mut page_hasher);
                hash_value(value, &mut page_hasher);
            }
            hasher.write_u64(page_hasher.finish());
            start = end;
        }
        hasher.finish()
    }

    pub(crate) fn hash_cached<F>(&mut self, mut hash_value: F) -> u64
    where
        F: FnMut(&mut T, &mut StateHasher),
    {
        if !self.hashes.stale {
            return self.hashes.store;
        }
        let entities = &self.entities;
        let values = &mut self.values;
        let pages = &mut self.hashes.pages;
        let mut from = 0usize;
        self.hashes.dirty.drain(|page| {
            let first = u64::try_from(page)
                .unwrap_or(u64::MAX)
                .checked_shl(PAGE_SHIFT)
                .unwrap_or(u64::MAX);
            let next = first.saturating_add(1u64 << PAGE_SHIFT);
            let rest = entities.get(from..).unwrap_or(&[]);
            let start = from.saturating_add(rest.partition_point(|e| u64::from(e.index()) < first));
            let rest = entities.get(start..).unwrap_or(&[]);
            let end = start.saturating_add(rest.partition_point(|e| u64::from(e.index()) < next));
            from = end;
            let hash = (start < end).then(|| {
                let mut page_hasher = StateHasher::new();
                let page_entities = entities.get(start..end).unwrap_or(&[]);
                let page_values = values.get_mut(start..end).unwrap_or(&mut []);
                for (entity, value) in page_entities.iter().zip(page_values) {
                    entity.hash_into(&mut page_hasher);
                    hash_value(value, &mut page_hasher);
                }
                page_hasher.finish()
            });
            if page >= pages.len() {
                pages.resize(page.saturating_add(1), None);
            }
            if let Some(cached) = pages.get_mut(page) {
                *cached = hash;
            }
        });

        let mut hasher = StateHasher::new();
        hasher.write_usize(self.len());
        for page in self.hashes.pages.iter().flatten() {
            hasher.write_u64(*page);
        }
        self.hashes.store = hasher.finish();
        self.hashes.stale = false;
        self.hashes.store
    }

    pub(crate) fn hash_legacy<F>(&self, hasher: &mut StateHasher, mut hash_value: F)
    where
        F: FnMut(&T, &mut StateHasher),
    {
        hasher.write_usize(self.len());
        for (entity, value) in self.iter() {
            entity.hash_into(hasher);
            hash_value(value, hasher);
        }
    }
}

pub fn join<'a, A, B>(
    a: &'a ComponentStore<A>,
    b: &'a ComponentStore<B>,
) -> impl Iterator<Item = (Entity, &'a A, &'a B)> + 'a {
    a.iter()
        .filter_map(move |(entity, left)| b.get(entity).map(|right| (entity, left, right)))
}

pub fn join_mut<'a, A, B>(
    a: &'a mut ComponentStore<A>,
    b: &'a ComponentStore<B>,
) -> impl Iterator<Item = (Entity, &'a mut A, &'a B)> + 'a {
    a.iter_mut()
        .filter_map(move |(entity, left)| b.get(entity).map(|right| (entity, left, right)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(index: u32) -> Entity {
        Entity::raw(index, 1)
    }

    fn val(index: u32) -> i32 {
        i32::try_from(index).unwrap_or(-1)
    }

    fn collected(store: &ComponentStore<i32>) -> Vec<(u32, i32)> {
        store
            .iter()
            .map(|(entity, value)| (entity.index(), *value))
            .collect()
    }

    #[test]
    fn writes_stamp_the_pages_they_touch() {
        let mut store = ComponentStore::new();
        store.insert(entity(3), 30);
        store.insert(entity(200), 2);
        let seen = store.writes();
        let pages = |store: &ComponentStore<i32>, since| {
            store.pages_written_since(since).collect::<Vec<u32>>()
        };
        assert_eq!(pages(&store, 0), vec![0, 192]);
        assert!(store.get(entity(3)).is_some() && pages(&store, seen).is_empty());
        if let Some(value) = store.get_mut(entity(200)) {
            *value = 3;
        }
        assert_eq!(pages(&store, seen), vec![192]);
        let seen = store.writes();
        store.iter_mut().take(1).for_each(|(_, value)| *value = 4);
        assert_eq!(pages(&store, seen), vec![0]);
        let seen = store.writes();
        store.remove_many(&[entity(3)]);
        assert_eq!(pages(&store, seen), vec![0]);
        let seen = store.writes();
        store.clear();
        assert!(store.writes() > seen);
        assert_eq!(pages(&store, seen), vec![0, 64, 128, 192]);
    }

    #[test]
    fn insert_get_remove_round_trips() {
        let mut store = ComponentStore::new();
        assert!(store.is_empty());
        assert_eq!(store.insert(entity(3), 30), None);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(entity(3)), Some(&30));
        assert!(store.contains(entity(3)));
        assert_eq!(store.insert(entity(3), 31), Some(30));
        if let Some(value) = store.get_mut(entity(3)) {
            *value = 32;
        }
        assert_eq!(store.remove(entity(3)), Some(32));
        assert_eq!(store.remove(entity(3)), None);
        assert!(store.is_empty());
    }

    #[test]
    fn iteration_is_ascending_by_entity_index_not_insertion_order() {
        let mut store = ComponentStore::new();
        for index in [7_u32, 2, 9, 0, 4] {
            store.insert(entity(index), val(index));
        }
        assert_eq!(
            collected(&store),
            vec![(0, 0), (2, 2), (4, 4), (7, 7), (9, 9)]
        );
        assert_eq!(
            store.entities(),
            &[entity(0), entity(2), entity(4), entity(7), entity(9)]
        );
    }

    #[test]
    fn removal_leaves_the_remaining_order_and_lookups_intact() {
        let mut store = ComponentStore::new();
        for index in 0..6_u32 {
            store.insert(entity(index), val(index));
        }
        assert_eq!(store.remove(entity(2)), Some(2));
        assert_eq!(store.remove(entity(0)), Some(0));
        assert_eq!(collected(&store), vec![(1, 1), (3, 3), (4, 4), (5, 5)]);
        for index in [1_u32, 3, 4, 5] {
            assert_eq!(store.get(entity(index)), Some(&val(index)));
        }
        assert_eq!(store.get(entity(0)), None);
        assert_eq!(store.get(entity(2)), None);
    }

    #[test]
    fn remove_many_matches_removing_one_at_a_time() {
        let mut one_by_one = ComponentStore::new();
        let mut batched = ComponentStore::new();
        for index in 0..10_u32 {
            one_by_one.insert(entity(index), val(index));
            batched.insert(entity(index), val(index));
        }
        let doomed = [entity(7), entity(2), entity(9), entity(2), entity(42)];
        for e in doomed {
            one_by_one.remove(e);
        }
        assert_eq!(batched.remove_many(&doomed), 3);
        assert_eq!(collected(&batched), collected(&one_by_one));
        for index in [0_u32, 1, 3, 4, 5, 6, 8] {
            assert_eq!(batched.get(entity(index)), Some(&val(index)));
        }
        assert_eq!(batched.get(entity(7)), None);
        assert_eq!(batched.remove_many(&[]), 0);
        assert_eq!(batched.remove_many(&[Entity::raw(3, 9)]), 0);
        assert_eq!(batched.get(entity(3)), Some(&3));
    }

    #[test]
    fn reinsertion_after_removal_lands_back_in_order() {
        let mut store = ComponentStore::new();
        for index in [0_u32, 1, 2, 3] {
            store.insert(entity(index), val(index));
        }
        assert_eq!(store.remove(entity(1)), Some(1));
        assert_eq!(store.insert(entity(1), 11), None);
        assert_eq!(collected(&store), vec![(0, 0), (1, 11), (2, 2), (3, 3)]);
        assert_eq!(store.get(entity(3)), Some(&3));
    }

    #[test]
    fn a_stale_handle_does_not_read_a_recycled_slot() {
        let mut store = ComponentStore::new();
        let old = Entity::raw(4, 1);
        let new = Entity::raw(4, 2);
        store.insert(new, 99);
        assert_eq!(store.get(new), Some(&99));
        assert_eq!(store.get(old), None);
        assert!(!store.contains(old));
        assert_eq!(store.remove(old), None);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn inserting_for_a_recycled_slot_replaces_the_stale_entry() {
        let mut store = ComponentStore::new();
        store.insert(Entity::raw(1, 1), 10);
        store.insert(Entity::raw(2, 1), 20);
        assert_eq!(store.insert(Entity::raw(1, 5), 15), None);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(Entity::raw(1, 5)), Some(&15));
        assert_eq!(store.get(Entity::raw(1, 1)), None);
        assert_eq!(collected(&store), vec![(1, 15), (2, 20)]);
    }

    #[test]
    fn clear_empties_the_store_and_leaves_it_usable() {
        let mut store = ComponentStore::new();
        for index in 0..4_u32 {
            store.insert(entity(index), val(index));
        }
        store.clear();
        assert!(store.is_empty());
        assert_eq!(store.get(entity(2)), None);
        store.insert(entity(2), 22);
        assert_eq!(collected(&store), vec![(2, 22)]);
    }

    #[test]
    fn iter_mut_visits_in_the_same_order_and_writes_through() {
        let mut store = ComponentStore::new();
        for index in [5_u32, 1, 3] {
            store.insert(entity(index), 0);
        }
        let mut seen = Vec::new();
        for (entity, value) in store.iter_mut() {
            seen.push(entity.index());
            *value = val(entity.index());
        }
        assert_eq!(seen, vec![1, 3, 5]);
        assert_eq!(collected(&store), vec![(1, 1), (3, 3), (5, 5)]);
    }

    #[test]
    fn join_yields_the_intersection_ascending() {
        let mut left: ComponentStore<i32> = ComponentStore::new();
        let mut right: ComponentStore<char> = ComponentStore::new();
        for index in [4_u32, 1, 2, 8] {
            left.insert(entity(index), val(index));
        }
        for index in [8_u32, 2, 5] {
            right.insert(entity(index), 'x');
        }
        let joined: Vec<(u32, i32)> = join(&left, &right)
            .map(|(entity, value, _)| (entity.index(), *value))
            .collect();
        assert_eq!(joined, vec![(2, 2), (8, 8)]);
    }

    #[test]
    fn join_skips_a_stale_handle_on_the_other_side() {
        let mut left: ComponentStore<i32> = ComponentStore::new();
        let mut right: ComponentStore<char> = ComponentStore::new();
        left.insert(Entity::raw(1, 1), 10);
        right.insert(Entity::raw(1, 2), 'x');
        assert_eq!(join(&left, &right).count(), 0);
    }

    #[test]
    fn join_mut_writes_through_the_left_store_while_reading_the_right() {
        let mut left: ComponentStore<i32> = ComponentStore::new();
        let mut right: ComponentStore<char> = ComponentStore::new();
        for index in [1_u32, 2, 3] {
            left.insert(entity(index), 0);
        }
        for index in [2_u32, 3] {
            right.insert(entity(index), 'x');
        }
        for (_entity, value, _) in join_mut(&mut left, &right) {
            *value = 100;
        }
        assert_eq!(collected(&left), vec![(1, 0), (2, 100), (3, 100)]);
    }

    #[test]
    fn from_sorted_matches_what_repeated_insert_would_have_built() {
        let pairs: Vec<(Entity, i32)> = [7_u32, 2, 9, 0, 4]
            .into_iter()
            .map(|index| (entity(index), val(index)))
            .collect();
        let mut expected = ComponentStore::new();
        for &(e, v) in &pairs {
            expected.insert(e, v);
        }

        let mut sorted = pairs;
        sorted.sort_unstable_by_key(|(e, _)| e.index());
        let built = ComponentStore::from_sorted(sorted).expect("input is sorted and unique");

        assert_eq!(collected(&built), collected(&expected));
        assert_eq!(built.get(entity(5)), None);
        assert_eq!(built.get(entity(9)), Some(&9));
    }

    #[test]
    fn from_sorted_rejects_input_that_is_not_strictly_ascending() {
        let unsorted = vec![(entity(2), 2), (entity(1), 1)];
        assert!(ComponentStore::<i32>::from_sorted(unsorted).is_none());

        let duplicate = vec![(entity(1), 1), (entity(1), 2)];
        assert!(ComponentStore::<i32>::from_sorted(duplicate).is_none());
    }

    #[test]
    fn from_sorted_of_an_empty_vec_is_an_empty_store() {
        let built =
            ComponentStore::<i32>::from_sorted(Vec::new()).expect("empty is trivially sorted");
        assert!(built.is_empty());
    }
}
