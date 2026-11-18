#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pcg32 {
    state: u64,
    increment: u64,
}

const MULTIPLIER: u64 = 6_364_136_223_846_793_005;

const DEFAULT_STREAM: u64 = 721_347_520_444_481_703;

impl Pcg32 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::with_stream(seed, DEFAULT_STREAM)
    }

    #[must_use]
    pub fn with_stream(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: stream.wrapping_shl(1) | 1,
        };
        let _ = rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        let _ = rng.next_u32();
        rng
    }

    #[must_use]
    pub const fn from_parts(state: u64, increment: u64) -> Self {
        Self {
            state,
            increment: increment | 1,
        }
    }

    #[must_use]
    pub const fn to_parts(self) -> (u64, u64) {
        (self.state, self.increment)
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);

        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rotation = (old >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }

    pub fn next_u64(&mut self) -> u64 {
        let low = u64::from(self.next_u32());
        let high = u64::from(self.next_u32());
        low | (high << 32)
    }

    #[allow(clippy::cast_possible_truncation)]
    pub fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }

        let mut wide = u64::from(self.next_u32()).wrapping_mul(u64::from(bound));
        let mut low = wide as u32;

        if low < bound {
            let threshold = bound.wrapping_neg().checked_rem(bound).unwrap_or(0);
            while low < threshold {
                wide = u64::from(self.next_u32()).wrapping_mul(u64::from(bound));
                low = wide as u32;
            }
        }

        (wide >> 32) as u32
    }

    pub fn next_f32(&mut self) -> f32 {
        const SCALE: f32 = 1.0 / (1u32 << 24) as f32;
        let bits = self.next_u32() >> 8;
        bits as f32 * SCALE
    }

    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        let bits = self.next_u64() >> 11;
        bits as f64 * SCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED_1_FIRST_TEN: [u32; 10] = [
        0x5435_2d7f,
        0x6ac2_0236,
        0x0768_dd4c,
        0x7556_0a43,
        0x4065_d452,
        0x99b2_e260,
        0x7d63_c9e5,
        0x489b_1d36,
        0x2e4a_4bdb,
        0xf7b5_3450,
    ];

    #[test]
    fn a_given_seed_produces_a_fixed_recorded_sequence() {
        let mut rng = Pcg32::new(1);
        let got: [u32; 10] = core::array::from_fn(|_| rng.next_u32());
        assert_eq!(got, SEED_1_FIRST_TEN);
    }

    #[test]
    fn the_generator_matches_the_reference_pcg32_output() {
        let mut rng = Pcg32::with_stream(42, 54);
        let got: [u32; 6] = core::array::from_fn(|_| rng.next_u32());
        assert_eq!(
            got,
            [
                0xa15c_02b7,
                0x7b47_f409,
                0xba1d_3330,
                0x83d2_f293,
                0xbfa4_784b,
                0xcbed_606e,
            ]
        );
    }

    #[test]
    fn two_generators_with_the_same_seed_agree() {
        let mut a = Pcg32::new(0xdead_beef);
        let mut b = Pcg32::new(0xdead_beef);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Pcg32::new(1);
        let mut b = Pcg32::new(2);
        let diverged = (0..8).any(|_| a.next_u32() != b.next_u32());
        assert!(diverged);
    }

    #[test]
    fn different_streams_produce_different_sequences() {
        let mut a = Pcg32::with_stream(7, 1);
        let mut b = Pcg32::with_stream(7, 2);
        let diverged = (0..8).any(|_| a.next_u32() != b.next_u32());
        assert!(diverged);
    }

    #[test]
    fn the_whole_state_round_trips_through_its_parts() {
        let mut rng = Pcg32::new(12345);
        for _ in 0..37 {
            let _ = rng.next_u32();
        }
        let (state, increment) = rng.to_parts();
        let mut restored = Pcg32::from_parts(state, increment);
        assert_eq!(restored, rng);

        for _ in 0..100 {
            assert_eq!(restored.next_u32(), rng.next_u32());
        }
    }

    #[test]
    fn a_restored_generator_cannot_have_an_even_increment() {
        let rng = Pcg32::from_parts(0, 0);
        assert_eq!(rng.to_parts().1 & 1, 1);
    }

    #[test]
    fn below_stays_inside_the_bound() {
        let mut rng = Pcg32::new(99);
        for bound in [1u32, 2, 3, 6, 7, 100, 0xffff, u32::MAX] {
            for _ in 0..200 {
                assert!(rng.below(bound) < bound, "bound {bound}");
            }
        }
    }

    #[test]
    fn below_zero_is_zero_rather_than_a_division_by_zero() {
        let mut rng = Pcg32::new(3);
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn below_is_reproducible_from_a_seed() {
        let mut rng = Pcg32::new(2024);
        let rolls: [u32; 8] = core::array::from_fn(|_| rng.below(6));
        assert_eq!(rolls, [3, 1, 5, 5, 4, 0, 1, 5]);
    }

    #[test]
    fn below_covers_every_residue_it_should() {
        let mut rng = Pcg32::new(5);
        let mut seen = [false; 7];
        for _ in 0..500 {
            let v = rng.below(7);
            if let Some(slot) = seen.get_mut(v as usize) {
                *slot = true;
            }
        }
        assert!(seen.iter().all(|s| *s));
    }

    #[test]
    fn next_f32_stays_in_the_unit_interval() {
        let mut rng = Pcg32::new(11);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn next_f64_stays_in_the_unit_interval() {
        let mut rng = Pcg32::new(13);
        for _ in 0..10_000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn next_u64_is_the_two_halves_in_a_fixed_order() {
        let mut a = Pcg32::new(77);
        let mut b = Pcg32::new(77);
        let low = u64::from(b.next_u32());
        let high = u64::from(b.next_u32());
        assert_eq!(a.next_u64(), low | (high << 32));
    }
}
