use core::f32::consts::TAU;

pub const MAX_SWAY_TYPES: usize = 10;

pub const SWAY_OFFSET_COUNT: usize = MAX_SWAY_TYPES + 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Breeze {
    pub direction: f32,
    pub intensity: f32,
    pub lean: f32,
    pub period: f32,
    pub randomness: f32,
}

impl Breeze {
    pub const GENERALS: Self = Self {
        direction: core::f32::consts::PI / 3.0,
        intensity: 0.07 * core::f32::consts::PI / 4.0,
        lean: 0.07 * core::f32::consts::PI / 4.0,
        period: 5.0,
        randomness: 0.2,
    };

    pub const LATER_TITLE: Self = Self {
        direction: core::f32::consts::FRAC_PI_3,
        intensity: 0.027_488_125,
        lean: 0.015_707_5,
        period: 10.0,
        randomness: 0.2,
    };

    pub const STILL: Self = Self {
        direction: 0.0,
        intensity: 0.0,
        lean: 0.0,
        period: 5.0,
        randomness: 0.0,
    };

    pub const GENTLE: Self = Self {
        intensity: Self::GENERALS.intensity * 0.5,
        lean: Self::GENERALS.lean * 0.5,
        period: 7.0,
        ..Self::GENERALS
    };

    pub const STRONG: Self = Self {
        intensity: Self::GENERALS.intensity * 3.0,
        lean: Self::GENERALS.lean * 3.0,
        period: 3.0,
        ..Self::GENERALS
    };

    #[must_use]
    pub fn offsets(&self, seconds: f32) -> [[f32; 3]; SWAY_OFFSET_COUNT] {
        let mut out = [[0.0; 3]; SWAY_OFFSET_COUNT];
        if !seconds.is_finite() || !self.period.is_finite() || self.period <= 0.0 {
            return out;
        }
        let direction = mint_math::f32::wrap_tau(self.direction);
        let (dir_x, dir_y) = mint_math::f32::sin_cos(direction);
        let delta = self.randomness.clamp(0.0, 1.0) * 0.5;
        for (index, entry) in out.iter_mut().enumerate().skip(1) {
            let Some(kind) = index.checked_sub(1) else {
                continue;
            };
            let rate = 1.0 + delta * jitter(kind, 0);
            let amplitude = 1.0 + delta * jitter(kind, 1);
            let rate = rate.max(0.0);
            let phase = mint_math::f32::wrap_tau(TAU * seconds * rate / self.period);
            let angle = self.lean + self.intensity * mint_math::f32::cos(phase);
            let (sin, cos) = mint_math::f32::sin_cos(mint_math::f32::wrap_tau(angle));
            *entry = [
                dir_x * sin * amplitude,
                dir_y * sin * amplitude,
                (cos - 1.0) * amplitude,
            ];
        }
        out
    }
}

impl Default for Breeze {
    fn default() -> Self {
        Self::GENERALS
    }
}

#[must_use]
pub fn sway_index(x: f32, y: f32) -> usize {
    let quantise = |value: f32| -> u64 {
        if !value.is_finite() {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rounded = value.round() as i64 as u64;
        rounded
    };
    let hash = mix(mix(quantise(x)) ^ quantise(y));
    let kind = usize::try_from(hash % 10).unwrap_or(0);
    kind.saturating_add(1)
}

fn jitter(kind: usize, stream: u64) -> f32 {
    let hash = mix((kind as u64)
        .wrapping_mul(2)
        .wrapping_add(stream)
        .wrapping_add(1));
    #[allow(clippy::cast_precision_loss)]
    let unit = (hash >> 40) as f32 / 16_777_216.0;
    unit * 2.0 - 1.0
}

fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5
    }

    #[test]
    fn a_still_breeze_moves_nothing_at_any_time() {
        for t in [0.0, 0.7, 137.25] {
            for entry in Breeze::STILL.offsets(t) {
                assert_eq!(entry, [0.0, 0.0, 0.0], "t = {t}");
            }
        }
    }

    #[test]
    fn slot_zero_is_the_zero_vector_whatever_the_wind_is_doing() {
        for t in [0.0, 1.3, 2.6, 99.0] {
            let offsets = Breeze::STRONG.offsets(t);
            assert_eq!(offsets[0], [0.0, 0.0, 0.0], "t = {t}");
        }
    }

    #[test]
    fn an_entry_is_the_displacement_of_a_point_one_unit_up() {
        let breeze = Breeze {
            direction: 0.0,
            intensity: 0.2,
            lean: 0.1,
            period: 4.0,
            randomness: 0.0,
        };
        let offsets = breeze.offsets(0.0);
        let angle = 0.1 + 0.2;
        let (sin, cos) = mint_math::f32::sin_cos(angle);
        for entry in offsets.iter().skip(1) {
            assert!(close(entry[0], 0.0), "{entry:?}");
            assert!(close(entry[1], sin), "{entry:?}");
            assert!(close(entry[2], cos - 1.0), "{entry:?}");
        }
        let half = breeze.offsets(2.0);
        let (sin_back, _) = mint_math::f32::sin_cos(0.1 - 0.2);
        assert!(close(half[1][1], sin_back), "{:?}", half[1]);
        assert!(sin_back < 0.0 && sin > 0.0, "the swing crosses upright");
    }

    #[test]
    fn the_wind_direction_takes_the_sine_on_x_and_the_cosine_on_y() {
        let breeze = Breeze {
            direction: core::f32::consts::FRAC_PI_2,
            randomness: 0.0,
            ..Breeze::GENERALS
        };
        let entry = breeze.offsets(0.0)[1];
        assert!(entry[0].abs() > 0.01, "{entry:?}");
        assert!(close(entry[1], 0.0), "{entry:?}");
    }

    #[test]
    fn the_table_repeats_after_one_period_of_every_type() {
        let breeze = Breeze {
            randomness: 0.0,
            period: 2.5,
            ..Breeze::GENERALS
        };
        let start = breeze.offsets(0.0);
        let cycle = breeze.offsets(2.5);
        for (a, b) in start.iter().zip(cycle.iter()) {
            for (x, y) in a.iter().zip(b.iter()) {
                assert!(close(*x, *y), "{a:?} against {b:?}");
            }
        }
    }

    #[test]
    fn randomness_spreads_the_ten_types_and_zero_puts_them_in_lock_step() {
        let locked = Breeze {
            randomness: 0.0,
            ..Breeze::GENERALS
        };
        let table = locked.offsets(1.7);
        for entry in table.iter().skip(2) {
            assert_eq!(*entry, table[1], "every type is identical at randomness 0");
        }

        let spread = Breeze::GENERALS;
        let table = spread.offsets(1.7);
        let distinct = table
            .iter()
            .skip(1)
            .filter(|entry| (entry[1] - table[1][1]).abs() > 1e-6)
            .count();
        assert!(
            distinct >= 8,
            "at randomness 0.2, at least eight of the nine other types differ: {table:?}"
        );
    }

    #[test]
    fn each_sway_type_draws_a_separate_rate_and_amplitude() {
        for kind in 0..MAX_SWAY_TYPES {
            let (rate, amplitude) = (jitter(kind, 0), jitter(kind, 1));
            assert!(
                (rate - amplitude).abs() > 1e-6,
                "type {kind} drew {rate} for both"
            );
            assert!((-1.0..1.0).contains(&rate), "type {kind}: {rate}");
            assert!((-1.0..1.0).contains(&amplitude), "type {kind}: {amplitude}");
        }
    }

    #[test]
    fn the_table_is_a_pure_function_of_the_breeze_and_the_clock() {
        assert_eq!(
            Breeze::GENERALS.offsets(12.5),
            Breeze::GENERALS.offsets(12.5)
        );
        assert_ne!(
            Breeze::GENERALS.offsets(12.5),
            Breeze::GENERALS.offsets(12.6)
        );
    }

    #[test]
    fn a_non_finite_clock_or_a_zero_period_gives_the_still_table() {
        for t in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(Breeze::GENERALS.offsets(t), [[0.0; 3]; SWAY_OFFSET_COUNT]);
        }
        for period in [0.0, -1.0, f32::NAN] {
            let breeze = Breeze {
                period,
                ..Breeze::GENERALS
            };
            assert_eq!(breeze.offsets(1.0), [[0.0; 3]; SWAY_OFFSET_COUNT]);
        }
    }

    #[test]
    fn every_entry_stays_finite_and_small_over_a_long_run() {
        for breeze in [Breeze::GENERALS, Breeze::GENTLE, Breeze::STRONG] {
            for tick in 0..2000i16 {
                let seconds = f32::from(tick) * 0.37;
                for entry in breeze.offsets(seconds) {
                    for value in entry {
                        assert!(value.is_finite(), "{breeze:?} at {seconds}s: {entry:?}");
                        assert!(value.abs() < 0.5, "{breeze:?} at {seconds}s: {entry:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_sway_index_is_always_a_real_slot_and_never_the_zero_one() {
        for x in -50..50i16 {
            for y in -50..50i16 {
                let index = sway_index(f32::from(x) * 13.7, f32::from(y) * 9.1);
                assert!(
                    (1..=MAX_SWAY_TYPES).contains(&index),
                    "({x}, {y}) gave {index}"
                );
            }
        }
        assert!((1..=MAX_SWAY_TYPES).contains(&sway_index(f32::NAN, f32::INFINITY)));
        assert!((1..=MAX_SWAY_TYPES).contains(&sway_index(1e30, -1e30)));
    }

    #[test]
    fn the_index_is_stable_under_a_hair_and_spread_over_a_map() {
        let centre = sway_index(100.0, 200.0);
        for dx in [-0.3, 0.0, 0.3] {
            for dy in [-0.3, 0.0, 0.3] {
                assert_eq!(
                    sway_index(100.0 + dx, 200.0 + dy),
                    centre,
                    "({dx}, {dy}) from the centre"
                );
            }
        }
        let mut seen = [0usize; MAX_SWAY_TYPES + 1];
        for step in 0..400i16 {
            let index = sway_index(f32::from(step) * 7.0, f32::from(step) * 3.0);
            if let Some(slot) = seen.get_mut(index) {
                *slot += 1;
            }
        }
        let used = seen.iter().skip(1).filter(|count| **count > 0).count();
        assert_eq!(used, MAX_SWAY_TYPES, "all ten phases are reached: {seen:?}");
        for count in seen.iter().skip(1) {
            assert!(*count < 133, "{seen:?}");
        }
    }
}
