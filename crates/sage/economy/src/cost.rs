use mint_sim::LogicFrames;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostModifiers {
    pub production_cost_change_percent: f32,
    pub kind_of_modifier: f32,
    pub handicap: f32,
    pub brutal_reduction: f32,
}

impl Default for CostModifiers {
    fn default() -> Self {
        Self {
            production_cost_change_percent: 0.0,
            kind_of_modifier: 1.0,
            handicap: 1.0,
            brutal_reduction: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeModifiers {
    pub handicap: f32,
    pub production_time_change_percent: f32,
    pub builder_modifier: f32,
    pub speed_multiplier: f32,
    pub brutal_reduction: f32,
}

impl Default for TimeModifiers {
    fn default() -> Self {
        Self {
            handicap: 1.0,
            production_time_change_percent: 0.0,
            builder_modifier: 1.0,
            speed_multiplier: 1.0,
            brutal_reduction: 0.0,
        }
    }
}

#[must_use]
pub fn cost_to_build(build_cost: i64, modifiers: &CostModifiers) -> i64 {
    let percent = finite_or(modifiers.production_cost_change_percent, 0.0);
    let kind_of = finite_or(modifiers.kind_of_modifier, 1.0);
    let handicap = finite_or(modifiers.handicap, 1.0);

    let faction = (1.0 + percent) * kind_of;
    #[allow(clippy::cast_precision_loss)]
    let base = build_cost as f32;
    truncate(base * faction * handicap * brutal_factor(modifiers.brutal_reduction))
}

const RETAIL_FRAMES_PER_SECOND: u32 = 5;
const PER_RETAIL_FRAME: u32 = mint_sim::SIMULATION_HZ / RETAIL_FRAMES_PER_SECOND;

fn brutal_factor(reduction: f32) -> f32 {
    if reduction.is_nan() {
        0.0
    } else {
        1.0 - reduction.min(1.0)
    }
}

#[must_use]
pub fn reduce_frames(frames: LogicFrames, reduction: f32) -> LogicFrames {
    let retail = i64::from(frames.get() / PER_RETAIL_FRAME);
    let reduced = truncate(to_f32(retail) * brutal_factor(reduction)).max(0);
    let total = reduced.saturating_mul(i64::from(PER_RETAIL_FRAME)).max(1);
    LogicFrames::from_frames(u32::try_from(total).unwrap_or(u32::MAX))
}

#[must_use]
pub fn time_to_build(build_time_seconds: f32, modifiers: &TimeModifiers) -> LogicFrames {
    let mut seconds = truncate(finite_or(build_time_seconds, 0.0));
    for factor in [
        finite_or(modifiers.handicap, 1.0),
        1.0 + finite_or(modifiers.production_time_change_percent, 0.0),
        finite_or(modifiers.builder_modifier, 1.0),
        finite_or(modifiers.speed_multiplier, 1.0),
    ] {
        seconds = truncate(to_f32(seconds) * factor);
    }
    let retail = seconds
        .max(0)
        .saturating_mul(i64::from(RETAIL_FRAMES_PER_SECOND));
    let retail = if modifiers.brutal_reduction == 0.0 {
        retail
    } else {
        truncate(to_f32(retail) * brutal_factor(modifiers.brutal_reduction)).max(0)
    };
    let frames = retail.saturating_mul(i64::from(PER_RETAIL_FRAME)).max(1);
    LogicFrames::from_frames(u32::try_from(frames).unwrap_or(u32::MAX))
}

#[must_use]
pub fn upgrade_time_to_build(build_time_seconds: f32) -> LogicFrames {
    let retail = truncate(finite_or(build_time_seconds, 0.0) * 5.0).max(0);
    let per_retail = i64::from(mint_sim::SIMULATION_HZ / RETAIL_FRAMES_PER_SECOND);
    let frames = retail.saturating_mul(per_retail).max(1);
    LogicFrames::from_frames(u32::try_from(frames).unwrap_or(u32::MAX))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostModifierStacking {
    Replace,
    EachMultiplies,
}

#[must_use]
pub fn kind_of_modifier_for(
    percentages: &[f32],
    count: u32,
    stacking: CostModifierStacking,
) -> f32 {
    if count == 0 {
        return 1.0;
    }
    match stacking {
        CostModifierStacking::Replace => {
            let Some(last) = percentages.len().checked_sub(1) else {
                return 1.0;
            };
            let index = usize::try_from(count.saturating_sub(1))
                .unwrap_or(usize::MAX)
                .min(last);
            let percent = percentages.get(index).copied().unwrap_or(0.0);
            1.0 + finite_or(percent, 0.0)
        }
        CostModifierStacking::EachMultiplies => {
            let percent = finite_or(percentages.first().copied().unwrap_or(0.0), 0.0);
            let mut factor = 1.0f32;
            for _ in 0..count {
                factor *= 1.0 + percent;
            }
            finite_or(factor, 1.0)
        }
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn truncate(value: f32) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let whole = value as i64;
    whole
}

fn to_f32(frames: i64) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let value = frames as f32;
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmodified_cost_and_time_are_the_template_values() {
        assert_eq!(cost_to_build(1_200, &CostModifiers::default()), 1_200);
        assert_eq!(
            time_to_build(15.0, &TimeModifiers::default()),
            LogicFrames::from_frames(450)
        );
        assert_eq!(cost_to_build(0, &CostModifiers::default()), 0);
        assert_eq!(cost_to_build(20_000, &CostModifiers::default()), 20_000);
        assert_eq!(
            time_to_build(180.0, &TimeModifiers::default()),
            LogicFrames::from_frames(5_400)
        );
    }

    #[test]
    fn a_cost_discount_reads_in_eas_own_format() {
        let cheaper = CostModifiers {
            production_cost_change_percent: -0.2,
            ..CostModifiers::default()
        };
        assert_eq!(cost_to_build(1_000, &cheaper), 800);
        let dearer = CostModifiers {
            production_cost_change_percent: 0.25,
            ..CostModifiers::default()
        };
        assert_eq!(cost_to_build(1_000, &dearer), 1_250);
    }

    #[test]
    fn the_time_truncations_happen_in_eas_order_and_the_order_is_observable() {
        let modifiers = TimeModifiers {
            handicap: 0.5,
            production_time_change_percent: 0.5,
            ..TimeModifiers::default()
        };
        assert_eq!(
            time_to_build(7.0, &modifiers),
            LogicFrames::from_frames(4 * 30),
            "the intermediate truncation was skipped or reordered"
        );
    }

    #[test]
    fn a_producer_modifier_truncates_in_seconds_not_frames() {
        let modifiers = TimeModifiers {
            builder_modifier: 0.9,
            ..TimeModifiers::default()
        };
        assert_eq!(
            time_to_build(45.0, &modifiers),
            LogicFrames::from_frames(1_200)
        );
        assert_eq!(
            time_to_build(45.7, &TimeModifiers::default()),
            LogicFrames::from_frames(1_350),
            "BuildTime itself truncates to whole seconds"
        );
    }

    #[test]
    fn a_brutal_reduction_truncates_once_in_f32_on_retail_frames_and_on_cost() {
        let reduction = f32::from_bits(0x3E19_9999);
        let time = TimeModifiers {
            brutal_reduction: reduction,
            ..TimeModifiers::default()
        };
        assert_eq!(time_to_build(20.0, &time), LogicFrames::from_frames(85 * 6));
        let cost = CostModifiers {
            brutal_reduction: reduction,
            ..CostModifiers::default()
        };
        assert_eq!(cost_to_build(300, &cost), 255);
        assert_eq!(cost_to_build(1_000, &cost), 850);
        assert_eq!(
            reduce_frames(LogicFrames::from_frames(85 * 6), reduction),
            LogicFrames::from_frames(72 * 6)
        );
    }

    #[test]
    fn an_upgrade_is_five_retail_frames_per_second_and_takes_no_modifiers() {
        assert_eq!(upgrade_time_to_build(20.0), LogicFrames::from_frames(600));
        assert_eq!(upgrade_time_to_build(2.3), LogicFrames::from_frames(66));
    }

    #[test]
    fn the_cost_truncation_happens_once_and_not_per_multiplier() {
        let modifiers = CostModifiers {
            production_cost_change_percent: -0.5,
            kind_of_modifier: 0.3,
            handicap: 1.0,
            ..CostModifiers::default()
        };
        assert_eq!(cost_to_build(101, &modifiers), 15);
        let per_step = truncate(truncate(101.0 * 0.5) as f32 * 0.3);
        assert_eq!(per_step, 15);
        assert_eq!(cost_to_build(7, &modifiers), 1);
        assert_eq!(truncate(truncate(7.0 * 0.5) as f32 * 0.3), 0);
    }

    #[test]
    fn a_broken_modifier_is_ignored_rather_than_poisoning_the_purse() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let cost = CostModifiers {
                production_cost_change_percent: bad,
                kind_of_modifier: bad,
                handicap: bad,
                ..CostModifiers::default()
            };
            assert_eq!(cost_to_build(1_000, &cost), 1_000);
            let time = TimeModifiers {
                handicap: bad,
                production_time_change_percent: bad,
                builder_modifier: bad,
                speed_multiplier: bad,
                ..TimeModifiers::default()
            };
            assert_eq!(time_to_build(10.0, &time), LogicFrames::from_frames(300));
            assert_eq!(
                time_to_build(bad, &TimeModifiers::default()),
                LogicFrames::ONE
            );
        }
    }

    #[test]
    fn an_absurd_cost_or_time_saturates_rather_than_wrapping() {
        let huge = CostModifiers {
            production_cost_change_percent: 1.0e30,
            ..CostModifiers::default()
        };
        assert!(
            cost_to_build(20_000, &huge) > 0,
            "the cost must not wrap negative"
        );
        let long = TimeModifiers {
            handicap: 1.0e30,
            ..TimeModifiers::default()
        };
        assert_eq!(
            time_to_build(180.0, &long),
            LogicFrames::from_frames(u32::MAX)
        );
        let negative = TimeModifiers {
            handicap: -5.0,
            ..TimeModifiers::default()
        };
        assert_eq!(time_to_build(10.0, &negative), LogicFrames::ONE);
    }

    #[test]
    fn kind_of_modifier_for_selects_the_kth_entry_and_clamps_past_the_list() {
        let percentages = [-0.04, -0.08, -0.12];
        assert_eq!(
            kind_of_modifier_for(&percentages, 0, CostModifierStacking::Replace),
            1.0,
            "no standing instance changes nothing"
        );
        assert_eq!(
            kind_of_modifier_for(&percentages, 1, CostModifierStacking::Replace),
            0.96
        );
        assert_eq!(
            kind_of_modifier_for(&percentages, 2, CostModifierStacking::Replace),
            0.92
        );
        assert_eq!(
            kind_of_modifier_for(&percentages, 10, CostModifierStacking::Replace),
            0.88,
            "past the list's end clamps to the last entry rather than panicking"
        );
        assert_eq!(
            kind_of_modifier_for(&[], 3, CostModifierStacking::Replace),
            1.0,
            "an empty list changes nothing"
        );
    }

    #[test]
    fn each_multiplies_compounds_the_first_entry_per_instance() {
        let percentages = [-0.04];
        assert_eq!(
            kind_of_modifier_for(&percentages, 1, CostModifierStacking::EachMultiplies),
            0.96
        );
        let two = kind_of_modifier_for(&percentages, 2, CostModifierStacking::EachMultiplies);
        assert!((two - 0.9216).abs() < 1e-6, "0.96 * 0.96, got {two}");
    }

    #[test]
    fn a_zero_build_time_takes_one_frame_not_none() {
        assert_eq!(
            time_to_build(0.0, &TimeModifiers::default()),
            LogicFrames::ONE
        );
    }
}
