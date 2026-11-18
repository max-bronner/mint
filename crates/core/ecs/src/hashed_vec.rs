use crate::hash::StateHasher;
use crate::pages::{page_of, DirtyPages, PAGE_SHIFT};

#[derive(Debug, Clone, Default)]
pub struct HashedVec<T> {
    items: Vec<T>,
    pages: Vec<u64>,
    dirty: DirtyPages,
}

impl<T> HashedVec<T> {
    fn page_index(index: usize) -> u32 {
        page_of(u32::try_from(index).unwrap_or(u32::MAX))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    #[must_use]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        let item = self.items.get_mut(index)?;
        self.dirty.mark(Self::page_index(index));
        Some(item)
    }

    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        let dirty = &mut self.dirty;
        self.items.iter_mut().enumerate().map(move |(index, item)| {
            dirty.mark(Self::page_index(index));
            item
        })
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    pub fn hash_into<F>(&self, hasher: &mut StateHasher, mut hash_item: F)
    where
        F: FnMut(&T, &mut StateHasher),
    {
        hasher.write_usize(self.items.len());
        if hasher.is_legacy() {
            for item in &self.items {
                hash_item(item, hasher);
            }
            return;
        }
        for page in self.items.chunks(1 << PAGE_SHIFT) {
            hasher.write_u64(Self::hash_page(page, &mut hash_item));
        }
    }

    pub fn hash_into_cached<F>(&mut self, hasher: &mut StateHasher, mut hash_item: F)
    where
        F: FnMut(&T, &mut StateHasher),
    {
        if hasher.is_legacy() {
            self.hash_into(hasher, hash_item);
            return;
        }
        let pages = &mut self.pages;
        let items = &self.items;
        self.dirty.drain(|page| {
            let start = page.saturating_mul(1 << PAGE_SHIFT);
            let end = start.saturating_add(1 << PAGE_SHIFT).min(items.len());
            if let (Some(elements), Some(cached)) = (items.get(start..end), pages.get_mut(page)) {
                *cached = Self::hash_page(elements, &mut hash_item);
            }
        });
        hasher.write_usize(self.items.len());
        for page in &self.pages {
            hasher.write_u64(*page);
        }
    }

    fn hash_page<F>(elements: &[T], hash_item: &mut F) -> u64
    where
        F: FnMut(&T, &mut StateHasher),
    {
        let mut page = StateHasher::new();
        for item in elements {
            hash_item(item, &mut page);
        }
        page.finish()
    }
}

impl<T: PartialEq> PartialEq for HashedVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl<T> From<Vec<T>> for HashedVec<T> {
    fn from(items: Vec<T>) -> Self {
        let page_count = items.len().div_ceil(1 << PAGE_SHIFT);
        let mut dirty = DirtyPages::new();
        for page in 0..page_count {
            dirty.mark(u32::try_from(page).unwrap_or(u32::MAX));
        }
        Self {
            items,
            pages: vec![0; page_count],
            dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_u32(item: &u32, hasher: &mut StateHasher) {
        hasher.write_u32(*item);
    }

    fn full(vec: &HashedVec<u32>, hasher: StateHasher) -> u64 {
        let mut hasher = hasher;
        vec.hash_into(&mut hasher, hash_u32);
        hasher.finish()
    }

    fn cached(vec: &mut HashedVec<u32>) -> u64 {
        let mut hasher = StateHasher::new();
        vec.hash_into_cached(&mut hasher, hash_u32);
        hasher.finish()
    }

    #[test]
    fn a_write_to_any_element_moves_the_cached_hash() {
        let mut vec = HashedVec::from((0..1000u32).collect::<Vec<_>>());
        let mut before = cached(&mut vec);
        assert_eq!(before, full(&vec, StateHasher::new()));
        for index in (0..1000).step_by(7) {
            if index % 2 == 0 {
                *vec.get_mut(index).unwrap() ^= 1 << (index % 32);
            } else {
                *vec.iter_mut().nth(index).unwrap() ^= 1 << (index % 32);
            }
            let after = cached(&mut vec);
            assert_ne!(after, before, "element {index}");
            assert_eq!(after, full(&vec, StateHasher::new()), "element {index}");
            before = after;
        }
    }

    #[test]
    fn a_clone_carries_a_true_cache() {
        let mut vec = HashedVec::from((0..300u32).collect::<Vec<_>>());
        let at_clone = cached(&mut vec);
        let mut copy = vec.clone();
        *vec.get_mut(5).unwrap() = 77;
        assert_ne!(cached(&mut vec), at_clone);
        assert_eq!(cached(&mut copy), at_clone);
    }

    #[test]
    fn a_legacy_hasher_sees_the_elements_themselves() {
        let items: Vec<u32> = (0..200).collect();
        let mut plain = StateHasher::legacy();
        plain.write_usize(items.len());
        for item in &items {
            plain.write_u32(*item);
        }
        let mut vec = HashedVec::from(items);
        assert_eq!(full(&vec, StateHasher::legacy()), plain.finish());
        let mut hasher = StateHasher::legacy();
        vec.hash_into_cached(&mut hasher, hash_u32);
        assert_eq!(hasher.finish(), plain.finish());
    }
}
