#[derive(Debug, Clone)]
pub struct ClientRng {
    state: u64,
    increment: u64,
}

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

impl ClientRng {
    #[must_use]
    pub fn seeded(seed: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: seed.wrapping_shl(1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    #[must_use]
    pub fn from_clock() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x2545_F491_4F6C_DD1D, |d| {
                d.as_secs()
                    .wrapping_mul(1_000_000_000)
                    .wrapping_add(u64::from(d.subsec_nanos()))
            });
        Self::seeded(nanos)
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);
        let xorshifted = u32::try_from((((old >> 18) ^ old) >> 27) & 0xFFFF_FFFF).unwrap_or(0);
        let rotation = u32::try_from((old >> 59) & 0x1F).unwrap_or(0);
        xorshifted.rotate_right(rotation)
    }

    pub fn next_unit(&mut self) -> f32 {
        let bits = self.next_u32() >> 8;
        bits as f32 / 16_777_216.0
    }

    pub fn next_range(&mut self, low: f32, high: f32) -> f32 {
        if !(low.is_finite() && high.is_finite()) || high <= low {
            return low;
        }
        low + (high - low) * self.next_unit()
    }

    pub fn next_index(&mut self, bound: usize) -> Option<usize> {
        if bound == 0 {
            return None;
        }
        Some((self.next_u32() as usize).checked_rem(bound).unwrap_or(0))
    }

    pub fn next_weighted(&mut self, weights: &[u32]) -> Option<usize> {
        let total: u64 = weights.iter().map(|w| u64::from(*w)).sum();
        if total == 0 {
            return self.next_index(weights.len());
        }
        let mut roll = u64::from(self.next_u32()).checked_rem(total).unwrap_or(0);
        for (index, weight) in weights.iter().enumerate() {
            let weight = u64::from(*weight);
            if roll < weight {
                return Some(index);
            }
            roll = roll.saturating_sub(weight);
        }
        weights.len().checked_sub(1)
    }
}

impl Default for ClientRng {
    fn default() -> Self {
        Self::from_clock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        let mut a = ClientRng::seeded(12345);
        let mut b = ClientRng::seeded(12345);
        for _ in 0..64 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = ClientRng::seeded(1);
        let mut b = ClientRng::seeded(2);
        let differs = (0..64).any(|_| a.next_u32() != b.next_u32());
        assert!(differs);
    }

    #[test]
    fn unit_values_stay_in_range() {
        let mut rng = ClientRng::seeded(7);
        for _ in 0..10_000 {
            let value = rng.next_unit();
            assert!((0.0..1.0).contains(&value), "{value}");
        }
    }

    #[test]
    fn a_weighted_pick_follows_its_weights() {
        let mut rng = ClientRng::seeded(99);
        let mut hits = [0usize; 2];
        for _ in 0..10_000 {
            if let Some(index) = rng.next_weighted(&[9000, 1000]) {
                if let Some(slot) = hits.get_mut(index) {
                    *slot += 1;
                }
            }
        }
        assert!(hits[0] > 8_000, "{hits:?}");
        assert!(hits[1] > 500, "{hits:?}");
    }

    #[test]
    fn a_zero_weight_variant_is_never_chosen() {
        let mut rng = ClientRng::seeded(5);
        for _ in 0..1_000 {
            assert_eq!(rng.next_weighted(&[0, 1000]), Some(1));
        }
    }

    #[test]
    fn an_empty_list_yields_nothing_rather_than_panicking() {
        let mut rng = ClientRng::seeded(5);
        assert_eq!(rng.next_index(0), None);
        assert_eq!(rng.next_weighted(&[]), None);
    }

    #[test]
    fn an_inverted_range_returns_its_low_end() {
        let mut rng = ClientRng::seeded(5);
        assert_eq!(rng.next_range(5.0, 1.0), 5.0);
        assert!(rng.next_range(f32::NAN, 1.0).is_nan());
    }
}
