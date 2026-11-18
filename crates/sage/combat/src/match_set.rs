use mint_sim::StateHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConditionMask(u64);

impl ConditionMask {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn bit(n: u32) -> Self {
        if n < 64 {
            Self(1u64 << n)
        } else {
            Self(0)
        }
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn intersection_count(self, other: Self) -> u32 {
        (self.0 & other.0).count_ones()
    }

    #[must_use]
    pub const fn inverse_intersection_count(self, other: Self) -> u32 {
        (!self.0 & other.0).count_ones()
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0);
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConditionTable {
    names: Vec<String>,
    sorted: Vec<(String, u32)>,
    refused: usize,
}

impl ConditionTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, name: &str) -> ConditionMask {
        let key = name.trim().to_ascii_uppercase();
        if key.is_empty() || key == "NONE" {
            return ConditionMask::NONE;
        }
        if let Ok(at) = self.sorted.binary_search_by(|(k, _)| k.as_str().cmp(&key)) {
            return self
                .sorted
                .get(at)
                .map_or(ConditionMask::NONE, |(_, bit)| ConditionMask::bit(*bit));
        }
        let Ok(bit) = u32::try_from(self.names.len()) else {
            self.refused = self.refused.saturating_add(1);
            return ConditionMask::NONE;
        };
        if bit >= 64 {
            self.refused = self.refused.saturating_add(1);
            return ConditionMask::NONE;
        }
        let at = self
            .sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .unwrap_or_else(|at| at);
        self.sorted.insert(at, (key.clone(), bit));
        self.names.push(key);
        ConditionMask::bit(bit)
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> ConditionMask {
        let key = name.trim().to_ascii_uppercase();
        self.sorted
            .binary_search_by(|(k, _)| k.as_str().cmp(&key))
            .ok()
            .and_then(|at| self.sorted.get(at))
            .map_or(ConditionMask::NONE, |(_, bit)| ConditionMask::bit(*bit))
    }

    pub fn intern_all(&mut self, names: &[&str]) -> ConditionMask {
        let mut mask = ConditionMask::NONE;
        for name in names {
            mask = mask.union(self.intern(name));
        }
        mask
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    #[must_use]
    pub const fn refused(&self) -> usize {
        self.refused
    }
}

#[must_use]
pub fn best_match(active: ConditionMask, sets: &[ConditionMask]) -> Option<usize> {
    let mut result = None;
    let mut best_yes = 0u32;
    let mut best_extraneous = 999u32;
    for (index, set) in sets.iter().enumerate() {
        let yes = active.intersection_count(*set);
        let extraneous = active.inverse_intersection_count(*set);
        if yes > best_yes || (yes >= best_yes && extraneous < best_extraneous) {
            result = Some(index);
            best_yes = yes;
            best_extraneous = extraneous;
        }
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_lone_conditions_none_set_is_selected() {
        let sets = [ConditionMask::NONE];
        assert_eq!(best_match(ConditionMask::NONE, &sets), Some(0));
        assert_eq!(best_match(ConditionMask::bit(3), &sets), Some(0));
    }

    #[test]
    fn the_set_matching_the_most_active_conditions_wins() {
        let a = ConditionMask::bit(0);
        let b = ConditionMask::bit(1);
        let sets = [ConditionMask::NONE, a, a.union(b)];
        assert_eq!(best_match(a.union(b), &sets), Some(2));
        assert_eq!(best_match(a, &sets), Some(1));
        assert_eq!(best_match(b, &sets), Some(2));
        assert_eq!(best_match(ConditionMask::bit(9), &sets), Some(0));
    }

    #[test]
    fn a_tie_on_matches_prefers_the_set_asking_for_fewer_absent_conditions() {
        let front = ConditionMask::bit(0);
        let back = ConditionMask::bit(1);
        let sets = [front.union(back), front];
        assert_eq!(best_match(front, &sets), Some(1));
    }

    #[test]
    fn a_full_tie_is_won_by_the_earliest_declared_set() {
        let a = ConditionMask::bit(0);
        let sets = [a, a, a];
        assert_eq!(best_match(a, &sets), Some(0));
        let none = [ConditionMask::NONE, ConditionMask::NONE];
        assert_eq!(best_match(ConditionMask::bit(9), &none), Some(0));
    }

    #[test]
    fn no_sets_means_no_match() {
        assert_eq!(best_match(ConditionMask::NONE, &[]), None);
    }

    #[test]
    fn condition_names_intern_once_and_none_interns_nothing() {
        let mut table = ConditionTable::new();
        let upgrade = table.intern("PLAYER_UPGRADE");
        assert_eq!(table.intern("player_upgrade"), upgrade);
        assert_eq!(table.len(), 1);
        assert_eq!(table.intern("None"), ConditionMask::NONE);
        assert_eq!(table.intern("NONE"), ConditionMask::NONE);
        assert_eq!(table.intern(""), ConditionMask::NONE);
        assert_eq!(table.len(), 1);
        let both = table.intern_all(&["PLAYER_UPGRADE", "MOUNTED"]);
        assert!(both.contains(upgrade));
        assert_eq!(table.len(), 2);
        assert_eq!(table.lookup("mounted"), table.intern("MOUNTED"));
        assert_eq!(table.lookup("NEVER_DECLARED"), ConditionMask::NONE);
        assert_eq!(table.len(), 2, "lookup interns nothing");
    }

    #[test]
    fn a_sixty_fifth_condition_is_refused_rather_than_aliased() {
        let mut table = ConditionTable::new();
        let mut seen = Vec::new();
        for i in 0..64 {
            let mask = table.intern(&format!("COND_{i}"));
            assert_ne!(mask, ConditionMask::NONE, "{i}");
            seen.push(mask);
        }
        assert_eq!(table.intern("ONE_TOO_MANY"), ConditionMask::NONE);
        assert_eq!(table.refused(), 1);
        let mut bits: Vec<u64> = seen.iter().map(|m| m.bits()).collect();
        bits.sort_unstable();
        bits.dedup();
        assert_eq!(bits.len(), 64);
        assert_eq!(ConditionMask::bit(64), ConditionMask::NONE);
        assert_eq!(ConditionMask::bit(u32::MAX), ConditionMask::NONE);
    }
}
