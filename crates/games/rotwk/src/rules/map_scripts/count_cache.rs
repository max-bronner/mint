use std::collections::BTreeMap;

use mint_sim::StateHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CacheKey(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CountCache {
    result: i32,
    stamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CountStamp {
    stamp: u64,
    caches: BTreeMap<CacheKey, CountCache>,
}

impl CountStamp {
    pub(crate) fn adopt_later_stamp(&mut self, other: &Self) {
        self.stamp = self.stamp.max(other.stamp);
    }

    pub(crate) fn note_object_count_changed(&mut self, script_frame: u64) {
        self.stamp = script_frame;
    }

    pub(crate) fn fresh(&self, key: CacheKey) -> Option<i32> {
        self.caches
            .get(&key)
            .filter(|cache| cache.stamp == self.stamp)
            .map(|cache| cache.result)
    }

    pub(crate) fn action_is_current(&self, key: CacheKey) -> bool {
        self.caches.get(&key).map_or(0, |cache| cache.stamp) == self.stamp
    }

    pub(crate) fn mark_action_ran(&mut self, key: CacheKey) {
        let result = self.caches.get(&key).map_or(0, |cache| cache.result);
        self.store(key, result);
    }

    pub(crate) fn store(&mut self, key: CacheKey, result: i32) {
        self.caches.insert(
            key,
            CountCache {
                result,
                stamp: self.stamp,
            },
        );
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.stamp);
        hasher.write_usize(self.caches.len());
        for (key, cache) in &self.caches {
            hasher.write_u32(key.0);
            hasher.write_i32(cache.result);
            hasher.write_u64(cache.stamp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_is_reused_until_an_object_event_moves_the_stamp() {
        let mut stamp = CountStamp::default();
        let key = CacheKey(7);
        stamp.note_object_count_changed(3);
        assert_eq!(stamp.fresh(key), None);
        stamp.store(key, 5);
        assert_eq!(stamp.fresh(key), Some(5));
        stamp.note_object_count_changed(4);
        assert_eq!(stamp.fresh(key), None);
        stamp.store(key, 6);
        assert_eq!(stamp.fresh(key), Some(6));
    }

    #[test]
    fn an_action_record_does_nothing_until_the_first_stamp_event() {
        let mut stamp = CountStamp::default();
        let key = CacheKey(1);
        assert!(stamp.action_is_current(key));
        stamp.note_object_count_changed(2);
        assert!(!stamp.action_is_current(key));
        stamp.mark_action_ran(key);
        assert!(stamp.action_is_current(key));
    }
}
