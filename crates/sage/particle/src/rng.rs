#[derive(Debug, Clone)]
pub struct ClientRng {
    state: u64,
}

const MULTIPLIER: u64 = 0x2545_f491_4f6c_dd1d;

const TO_UNIT: f32 = 1.0 / 4_294_967_296.0;

impl ClientRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { MULTIPLIER } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut state = self.state;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.state = state;
        #[allow(clippy::cast_possible_truncation)]
        {
            (state.wrapping_mul(MULTIPLIER) >> 32) as u32
        }
    }

    #[must_use]
    pub fn unit(&mut self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        {
            self.next_u32() as f32 * TO_UNIT
        }
    }

    #[must_use]
    pub fn range(&mut self, range: mint_sage_ini::RandomRange) -> f32 {
        range.at(self.unit())
    }

    #[must_use]
    pub fn signed(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    #[must_use]
    pub fn direction(&mut self) -> [f32; 3] {
        for _ in 0..SPHERE_TRIES {
            let (x, y, z) = (self.signed(), self.signed(), self.signed());
            let length_squared = x * x + y * y + z * z;
            if !(MIN_LENGTH_SQUARED..=1.0).contains(&length_squared) {
                continue;
            }
            let length = length_squared.sqrt();
            return [x / length, y / length, z / length];
        }
        [0.0, 0.0, 1.0]
    }

    #[must_use]
    pub fn direction_xy(&mut self) -> [f32; 2] {
        for _ in 0..SPHERE_TRIES {
            let (x, y) = (self.signed(), self.signed());
            let length_squared = x * x + y * y;
            if !(MIN_LENGTH_SQUARED..=1.0).contains(&length_squared) {
                continue;
            }
            let length = length_squared.sqrt();
            return [x / length, y / length];
        }
        [1.0, 0.0]
    }
}

const SPHERE_TRIES: u32 = 8;

pub(crate) const MIN_LENGTH_SQUARED: f32 = 1e-12;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_seed_does_not_stick_at_zero() {
        let mut rng = ClientRng::new(0);
        let draws: Vec<u32> = (0..8).map(|_| rng.next_u32()).collect();
        assert!(
            draws.iter().any(|value| *value != 0),
            "a zero seed produced only zeroes: {draws:?}"
        );
    }

    #[test]
    fn every_draw_is_inside_the_unit_interval() {
        let mut rng = ClientRng::new(0x1234_5678_9abc_def0);
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for _ in 0..100_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value), "{value} outside [0, 1)");
            low = low.min(value);
            high = high.max(value);
        }
        assert!(low < 0.01 && high > 0.99, "range covered was {low}..{high}");
    }

    #[test]
    fn a_reversed_range_keeps_the_order_it_was_written_in() {
        let mut rng = ClientRng::new(7);
        let reversed = mint_sage_ini::RandomRange {
            low: -0.1,
            high: -1.0,
        };
        for _ in 0..1_000 {
            let value = rng.range(reversed);
            assert!((-1.0..=-0.1).contains(&value), "{value} outside the pair");
        }
    }
}
