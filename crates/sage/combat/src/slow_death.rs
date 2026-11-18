use mint_sim::{LogicFrames, Pcg32, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlowDeathOption {
    pub probability_modifier: i32,
    pub modifier_bonus_per_overkill_percent: i32,
    pub destruction_delay: LogicFrames,
    pub sink_rate: f32,
    pub sink_delay: LogicFrames,
    pub sink_delay_variance: LogicFrames,
    pub fade_delay: Option<LogicFrames>,
}

impl Default for SlowDeathOption {
    fn default() -> Self {
        Self {
            probability_modifier: 1,
            modifier_bonus_per_overkill_percent: 0,
            destruction_delay: LogicFrames::ZERO,
            sink_rate: 0.0,
            sink_delay: LogicFrames::ZERO,
            sink_delay_variance: LogicFrames::ZERO,
            fade_delay: None,
        }
    }
}

impl SlowDeathOption {
    #[must_use]
    pub fn weight(&self, overkill_fraction: f32) -> u32 {
        let bonus = if overkill_fraction.is_finite() && overkill_fraction > 0.0 {
            let scaled = overkill_fraction * self.modifier_bonus_per_overkill_percent as f32;
            #[allow(clippy::cast_possible_truncation)]
            let truncated = scaled.trunc() as i32;
            truncated
        } else {
            0
        };
        let total = self.probability_modifier.saturating_add(bonus);
        if total < 1 {
            1
        } else {
            u32::try_from(total).unwrap_or(u32::MAX)
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.probability_modifier);
        hasher.write_i32(self.modifier_bonus_per_overkill_percent);
        self.destruction_delay.hash_into(hasher);
        hasher.write_f32(self.sink_rate);
        self.sink_delay.hash_into(hasher);
        self.sink_delay_variance.hash_into(hasher);
        match self.fade_delay {
            Some(fade_delay) => {
                hasher.write_u8(1);
                fade_delay.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
    }
}

pub fn choose_slow_death(
    options: &[SlowDeathOption],
    overkill_fraction: f32,
    rng: &mut Pcg32,
) -> Option<usize> {
    if options.is_empty() {
        return None;
    }
    let mut total: u32 = 0;
    for option in options {
        total = total.saturating_add(option.weight(overkill_fraction));
    }
    if total == 0 {
        return Some(0);
    }
    let mut roll = i64::from(rng.below(total)).saturating_add(1);
    for (index, option) in options.iter().enumerate() {
        roll = roll.saturating_sub(i64::from(option.weight(overkill_fraction)));
        if roll <= 0 {
            return Some(index);
        }
    }
    Some(options.len().saturating_sub(1))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn option(modifier: i32, bonus: i32) -> SlowDeathOption {
        SlowDeathOption {
            probability_modifier: modifier,
            modifier_bonus_per_overkill_percent: bonus,
            ..Default::default()
        }
    }

    #[test]
    fn a_weight_never_falls_below_one() {
        assert_eq!(option(0, 0).weight(0.0), 1);
        assert_eq!(option(-50, 0).weight(0.0), 1);
        assert_eq!(option(5, -1000).weight(1.0), 1);
    }

    #[test]
    fn the_overkill_bonus_multiplies_a_fraction_and_truncates() {
        assert_eq!(option(10, 50).weight(0.2), 20);
        assert_eq!(option(10, 10).weight(0.25), 12);
        assert_eq!(option(10, 10).weight(0.09), 10);
        assert_eq!(option(10, 50).weight(0.0), 10);
    }

    #[test]
    fn a_hostile_overkill_fraction_gives_a_sane_weight() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 1e30] {
            let weight = option(10, 1000).weight(bad);
            assert!(weight >= 1, "{bad}");
        }
        assert_eq!(option(10, 1000).weight(f32::NAN), 10);
        assert_eq!(option(10, 1000).weight(-5.0), 10);
    }

    #[test]
    fn the_roll_follows_the_weights_and_repeats_for_the_same_seed() {
        let options = [option(1, 0), option(9, 0)];
        let mut counts = [0usize; 2];
        let mut rng = Pcg32::new(0xC0FFEE);
        for _ in 0..10_000 {
            let picked = choose_slow_death(&options, 0.0, &mut rng).unwrap();
            counts[picked] += 1;
        }
        assert!(counts[0] > 700 && counts[0] < 1_300, "{counts:?}");
        assert!(counts[1] > 8_700 && counts[1] < 9_300, "{counts:?}");

        let mut a = Pcg32::new(7);
        let mut b = Pcg32::new(7);
        for _ in 0..100 {
            assert_eq!(
                choose_slow_death(&options, 0.5, &mut a),
                choose_slow_death(&options, 0.5, &mut b)
            );
        }
    }

    #[test]
    fn one_option_is_always_chosen_and_costs_exactly_one_random_value() {
        let options = [option(1, 0)];
        let mut rng = Pcg32::new(3);
        let mut reference = Pcg32::new(3);
        for _ in 0..50 {
            assert_eq!(choose_slow_death(&options, 0.0, &mut rng), Some(0));
            let _ = reference.below(1);
        }
        assert_eq!(rng.next_u32(), reference.next_u32());
    }

    #[test]
    fn an_empty_list_picks_nothing_and_costs_no_random_value() {
        let mut rng = Pcg32::new(11);
        let mut reference = Pcg32::new(11);
        assert_eq!(choose_slow_death(&[], 1.0, &mut rng), None);
        assert_eq!(rng.next_u32(), reference.next_u32());
    }

    #[test]
    fn the_walk_always_lands_on_an_option_even_with_hostile_weights() {
        let options = [option(i32::MAX, i32::MAX), option(i32::MAX, i32::MAX)];
        let mut rng = Pcg32::new(5);
        for overkill in [0.0f32, 1.0, f32::NAN, 1e30] {
            let picked = choose_slow_death(&options, overkill, &mut rng);
            assert!(picked.is_some_and(|i| i < options.len()), "{overkill}");
        }
    }
}
