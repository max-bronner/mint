use mint_sage_ini::game_data::RetreatTuning;
use mint_sim::StateHasher;

pub(crate) const FORFEIT_EXCLUDED_FACTORIES: [&str; 7] = [
    "MenFortressCitadel",
    "ElvenCitadel",
    "DwarvenFortressCitadel",
    "WildFortressCitadel",
    "MordorFortressCitadel",
    "AngmarFortressCitadel",
    "IsengardFortressCitadel",
];
const STRIKES_TO_FORFEIT: u32 = 3;
const WARN_BELOW: i32 = 70;
const HERO_LEAD: i32 = 3;
const QUALITY_FACTOR: f32 = 2.0;
const QUANTITY_FACTOR: f32 = 1.5;
const MAP_CONTROL_FACTOR: f32 = 2.0;
const MAP_CONTROL_FLOOR: i32 = 9;
const BASE_FACTOR: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForfeitStage {
    Dormant,
    Cloned,
    Seen(i64),
    Armed,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForfeitProgress {
    pub(crate) stage: ForfeitStage,
    next_check: i64,
    strikes: u32,
    forfeiting: bool,
    countdown: i32,
    pub(crate) warned_at: Option<u64>,
}

impl ForfeitProgress {
    pub(crate) const fn new(tuning: &RetreatTuning) -> Self {
        Self {
            stage: ForfeitStage::Dormant,
            next_check: 0,
            strikes: 0,
            forfeiting: false,
            countdown: tuning.frames_till_self_destruct,
            warned_at: None,
        }
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        match self.stage {
            ForfeitStage::Dormant => hasher.write_u8(0),
            ForfeitStage::Cloned => hasher.write_u8(1),
            ForfeitStage::Seen(frame) => {
                hasher.write_u8(2);
                hasher.write_i64(frame);
            }
            ForfeitStage::Armed => hasher.write_u8(3),
            ForfeitStage::Ended => hasher.write_u8(4),
        }
        hasher.write_i64(self.next_check);
        hasher.write_u32(self.strikes);
        hasher.write_bool(self.forfeiting);
        hasher.write_i32(self.countdown);
        match self.warned_at {
            Some(frame) => {
                hasher.write_u8(1);
                hasher.write_u64(frame);
            }
            None => hasher.write_u8(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ForfeitStep {
    pub(crate) warn: bool,
    pub(crate) self_destruct: bool,
}

pub(crate) fn advance_forfeit(
    progress: &mut ForfeitProgress,
    tuning: &RetreatTuning,
    now: i64,
    offered: impl FnOnce() -> bool,
    score: impl FnOnce() -> i32,
) -> ForfeitStep {
    let mut step = ForfeitStep::default();
    match progress.stage {
        ForfeitStage::Ended => return step,
        ForfeitStage::Dormant => {
            if now > i64::from(tuning.frames_till_checks_start) && offered() {
                progress.stage = ForfeitStage::Cloned;
            }
            return step;
        }
        ForfeitStage::Cloned => {
            progress.stage = ForfeitStage::Seen(now);
            return step;
        }
        ForfeitStage::Seen(first) => {
            if now.saturating_sub(first) >= 0 {
                progress.stage = ForfeitStage::Armed;
                progress.next_check = now;
            }
            return step;
        }
        ForfeitStage::Armed => {}
    }
    if progress.forfeiting {
        progress.countdown = progress.countdown.saturating_sub(1);
        if progress.countdown < WARN_BELOW && progress.warned_at.is_none() {
            step.warn = true;
        }
    } else if progress.next_check <= now {
        progress.next_check = now.saturating_add(i64::from(tuning.frames_between_checks));
        if score() < tuning.farming_threshold {
            progress.strikes = progress.strikes.saturating_add(1);
        } else {
            progress.strikes = 0;
        }
        if progress.strikes >= STRIKES_TO_FORFEIT {
            progress.forfeiting = true;
            progress.countdown = tuning.frames_till_self_destruct;
        }
    }
    if progress.countdown <= 0 {
        progress.stage = ForfeitStage::Ended;
        step.self_destruct = true;
    }
    step
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct SideSums {
    pub(crate) quality: f32,
    pub(crate) quantity: i32,
    pub(crate) hero_level: i32,
    pub(crate) map_control: i32,
    pub(crate) base: f32,
}

fn sign(own: f32, enemy: f32, factor: f32) -> i32 {
    if own > factor * enemy {
        1
    } else if enemy > factor * own {
        -1
    } else {
        0
    }
}

pub(crate) fn forfeit_score(
    own: &SideSums,
    enemy: &SideSums,
    tuning: &RetreatTuning,
    ring_owned: bool,
    constant: f32,
) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let as_f32 = |value: i32| value as f32;
    let hero = if own.hero_level > enemy.hero_level.saturating_add(HERO_LEAD) {
        1
    } else if enemy.hero_level > own.hero_level.saturating_add(HERO_LEAD) {
        -1
    } else {
        0
    };
    let control = match sign(
        as_f32(own.map_control),
        as_f32(enemy.map_control),
        MAP_CONTROL_FACTOR,
    ) {
        1 if own.map_control > MAP_CONTROL_FLOOR => 1,
        -1 if enemy.map_control > MAP_CONTROL_FLOOR => -1,
        _ => 0,
    };
    let weighted = [
        (
            sign(own.quality, enemy.quality, QUALITY_FACTOR),
            tuning.army_quality_bias,
        ),
        (
            sign(
                as_f32(own.quantity),
                as_f32(enemy.quantity),
                QUANTITY_FACTOR,
            ),
            tuning.army_quantity_bias,
        ),
        (hero, tuning.hero_quality_bias),
        (control, tuning.map_control_bias),
        (
            sign(own.base, enemy.base, BASE_FACTOR),
            tuning.base_strength_bias,
        ),
        (i32::from(ring_owned), tuning.ring_ownership_bias),
    ]
    .iter()
    .fold(0i32, |sum, (sign, bias)| {
        sum.saturating_add(sign.saturating_mul(*bias))
    });
    #[allow(clippy::cast_possible_truncation)]
    let score = (as_f32(weighted) + constant) as i32;
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUNING: RetreatTuning = RetreatTuning {
        farming_threshold: -2,
        army_quality_bias: 2,
        army_quantity_bias: 1,
        hero_quality_bias: 1,
        map_control_bias: 2,
        base_strength_bias: 1,
        ring_ownership_bias: 2,
        frames_till_checks_start: 10,
        frames_between_checks: 5,
        frames_till_self_destruct: 4,
        disable_tactical_ai: false,
    };

    fn run(scores: impl Fn(i64) -> i32) -> (Vec<i64>, Option<i64>, Option<i64>) {
        let mut progress = ForfeitProgress::new(&TUNING);
        let (mut checks, mut warned, mut destroyed) = (Vec::new(), None, None);
        for now in 0..40 {
            let mut checked = false;
            let step = advance_forfeit(
                &mut progress,
                &TUNING,
                now,
                || true,
                || {
                    checked = true;
                    scores(now)
                },
            );
            if checked {
                checks.push(now);
            }
            if step.warn {
                progress.warned_at = Some(u64::try_from(now).unwrap_or(0));
                warned = Some(now);
            }
            if step.self_destruct {
                destroyed = Some(now);
                break;
            }
        }
        (checks, warned, destroyed)
    }

    #[test]
    fn three_failing_checks_warn_then_self_destruct_on_the_documented_frames() {
        let (checks, warned, destroyed) = run(|_| -9);
        assert_eq!(checks, [14, 19, 24]);
        assert_eq!(warned, Some(25));
        assert_eq!(destroyed, Some(28));
    }

    #[test]
    fn a_passing_check_resets_the_strikes() {
        let (checks, warned, destroyed) = run(|now| if now == 19 { 0 } else { -9 });
        assert_eq!(&checks[..5], [14, 19, 24, 29, 34]);
        assert_eq!((warned, destroyed), (Some(35), Some(38)));
    }

    #[test]
    fn a_non_positive_countdown_destroys_on_the_first_update_after_arming() {
        let tuning = RetreatTuning {
            frames_till_self_destruct: 0,
            ..TUNING
        };
        let mut progress = ForfeitProgress::new(&tuning);
        let mut destroyed = None;
        for now in 0..30 {
            if advance_forfeit(&mut progress, &tuning, now, || true, || 0).self_destruct {
                destroyed = Some(now);
                break;
            }
        }
        assert_eq!(destroyed, Some(14));
    }

    #[test]
    fn an_offer_refused_is_asked_again_on_a_later_frame() {
        let mut progress = ForfeitProgress::new(&TUNING);
        let mut asked = 0;
        for now in 0..14 {
            advance_forfeit(
                &mut progress,
                &TUNING,
                now,
                || {
                    asked += 1;
                    false
                },
                || 0,
            );
        }
        assert_eq!((asked, progress.stage), (3, ForfeitStage::Dormant));
    }

    #[test]
    fn the_score_weighs_each_term_by_its_ratio_and_bias() {
        let strong = SideSums {
            quality: 1000.0,
            quantity: 40,
            hero_level: 8,
            map_control: 12,
            base: 900.0,
        };
        let weak = SideSums {
            quality: 400.0,
            quantity: 20,
            hero_level: 4,
            map_control: 5,
            base: 500.0,
        };
        assert_eq!(forfeit_score(&strong, &weak, &TUNING, false, 0.0), 7);
        assert_eq!(forfeit_score(&weak, &strong, &TUNING, false, 0.0), -7);
        assert_eq!(forfeit_score(&weak, &strong, &TUNING, true, 1.0), -4);
        let small = SideSums {
            map_control: 8,
            ..SideSums::default()
        };
        assert_eq!(
            forfeit_score(&small, &SideSums::default(), &TUNING, false, 0.0),
            0
        );
    }
}
