use mint_ecs::StateHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UpgradeBit(u32);

impl UpgradeBit {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

const WORD_BITS: u32 = u64::BITS;

fn word_and_shift(bit: UpgradeBit) -> (usize, u32) {
    let word = bit.0.checked_div(WORD_BITS).unwrap_or(0);
    let shift = bit.0.checked_rem(WORD_BITS).unwrap_or(0);
    (word as usize, shift)
}

fn bit_mask(shift: u32) -> u64 {
    1u64.checked_shl(shift).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpgradeMask(Vec<u64>);

impl UpgradeMask {
    pub const EMPTY: Self = Self(Vec::new());

    pub fn set(&mut self, bit: UpgradeBit) {
        let (word, shift) = word_and_shift(bit);
        if word >= self.0.len() {
            self.0.resize(word.saturating_add(1), 0);
        }
        if let Some(slot) = self.0.get_mut(word) {
            *slot |= bit_mask(shift);
        }
    }

    #[must_use]
    pub fn contains(&self, bit: UpgradeBit) -> bool {
        let (word, shift) = word_and_shift(bit);
        self.0.get(word).is_some_and(|w| w & bit_mask(shift) != 0)
    }

    pub fn clear_all(&mut self, other: &Self) {
        for (word, &clear) in self.0.iter_mut().zip(other.0.iter()) {
            *word &= !clear;
        }
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (i, &word) in other.0.iter().enumerate() {
            if i >= out.0.len() {
                out.0.resize(i.saturating_add(1), 0);
            }
            if let Some(slot) = out.0.get_mut(i) {
                *slot |= word;
            }
        }
        out
    }

    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.0.iter().zip(other.0.iter()).any(|(a, b)| a & b != 0)
    }

    #[must_use]
    pub fn contains_all(&self, other: &Self) -> bool {
        other.0.iter().enumerate().all(|(i, &word)| {
            let mine = self.0.get(i).copied().unwrap_or(0);
            mine & word == word
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        let significant = self
            .0
            .iter()
            .rposition(|&w| w != 0)
            .map_or(0, |i| i.saturating_add(1));
        let words = self.0.get(..significant).unwrap_or(&[]);
        hasher.write_usize(words.len());
        for &word in words {
            hasher.write_u64(word);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_mask_is_empty_and_contains_nothing() {
        let mask = UpgradeMask::EMPTY;
        assert!(mask.is_empty());
        assert!(!mask.contains(UpgradeBit::new(0)));
        assert!(!mask.contains(UpgradeBit::new(500)));
    }

    #[test]
    fn set_then_contains_round_trips_across_word_boundaries() {
        let mut mask = UpgradeMask::EMPTY;
        for i in [0u32, 1, 63, 64, 65, 127, 128, 1_050] {
            mask.set(UpgradeBit::new(i));
        }
        for i in [0u32, 1, 63, 64, 65, 127, 128, 1_050] {
            assert!(
                mask.contains(UpgradeBit::new(i)),
                "bit {i} did not round-trip"
            );
        }
        assert!(!mask.is_empty());
        assert!(!mask.contains(UpgradeBit::new(2)));
        assert!(!mask.contains(UpgradeBit::new(1_051)));
    }

    #[test]
    fn union_combines_bits_regardless_of_which_side_is_wider() {
        let mut a = UpgradeMask::EMPTY;
        a.set(UpgradeBit::new(3));
        let mut b = UpgradeMask::EMPTY;
        b.set(UpgradeBit::new(200));

        let ab = a.union(&b);
        assert!(ab.contains(UpgradeBit::new(3)));
        assert!(ab.contains(UpgradeBit::new(200)));

        let ba = b.union(&a);
        assert!(ba.contains(UpgradeBit::new(3)));
        assert!(ba.contains(UpgradeBit::new(200)));
    }

    #[test]
    fn intersects_is_any_shared_bit_contains_all_is_every_bit() {
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        active.set(UpgradeBit::new(2));

        let mut any_of = UpgradeMask::EMPTY;
        any_of.set(UpgradeBit::new(2));
        any_of.set(UpgradeBit::new(99));
        assert!(active.intersects(&any_of));
        assert!(!active.contains_all(&any_of), "99 is not in active");

        let mut all_of = UpgradeMask::EMPTY;
        all_of.set(UpgradeBit::new(1));
        all_of.set(UpgradeBit::new(2));
        assert!(active.contains_all(&all_of));
    }

    #[test]
    fn an_empty_mask_intersects_and_contains_all_of_nothing_trivially() {
        let empty = UpgradeMask::EMPTY;
        let mut other = UpgradeMask::EMPTY;
        other.set(UpgradeBit::new(5));
        assert!(!empty.intersects(&other));
        assert!(empty.contains_all(&UpgradeMask::EMPTY));
        assert!(
            other.contains_all(&UpgradeMask::EMPTY),
            "everyone contains the empty set"
        );
    }

    #[test]
    fn hash_ignores_a_grown_but_all_zero_tail() {
        let mut grown = UpgradeMask::EMPTY;
        grown.set(UpgradeBit::new(5));
        grown.set(UpgradeBit::new(200));
        let shrunk = UpgradeMask(grown.0.iter().take(1).copied().collect());
        grown_minus_high_bit(&mut grown);

        let mut hasher_a = StateHasher::new();
        let mut hasher_b = StateHasher::new();
        grown.hash_into(&mut hasher_a);
        shrunk.hash_into(&mut hasher_b);
        assert_eq!(hasher_a, hasher_b);
    }

    #[test]
    fn clear_all_removes_exactly_the_given_bits_and_hashes_like_a_fresh_mask() {
        let mut mask = UpgradeMask::EMPTY;
        for i in [1u32, 70, 200] {
            mask.set(UpgradeBit::new(i));
        }
        let mut removed = UpgradeMask::EMPTY;
        removed.set(UpgradeBit::new(200));
        removed.set(UpgradeBit::new(5));
        mask.clear_all(&removed);
        assert!(mask.contains(UpgradeBit::new(1)));
        assert!(mask.contains(UpgradeBit::new(70)));
        assert!(!mask.contains(UpgradeBit::new(200)));

        let mut fresh = UpgradeMask::EMPTY;
        fresh.set(UpgradeBit::new(1));
        fresh.set(UpgradeBit::new(70));
        let (mut a, mut b) = (StateHasher::new(), StateHasher::new());
        mask.hash_into(&mut a);
        fresh.hash_into(&mut b);
        assert_eq!(a, b);
    }

    fn grown_minus_high_bit(mask: &mut UpgradeMask) {
        let (word, shift) = word_and_shift(UpgradeBit::new(200));
        if let Some(slot) = mask.0.get_mut(word) {
            *slot &= !bit_mask(shift);
        }
    }
}
