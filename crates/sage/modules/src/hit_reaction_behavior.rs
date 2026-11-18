use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

const HIT_REACTION_LEVELS: usize = 3;

pub const HIT_REACTION_CONDITION: &str = "HIT_REACTION";

#[must_use]
pub fn hit_level_condition(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("HIT_LEVEL_1"),
        2 => Some("HIT_LEVEL_2"),
        3 => Some("HIT_LEVEL_3"),
        _ => None,
    }
}

#[derive(Debug, PartialEq)]
struct HitReactionData {
    thresholds: [Option<f32>; HIT_REACTION_LEVELS],
    timers: [LogicFrames; HIT_REACTION_LEVELS],
    fast_hits_reset_reaction: bool,
    hits_paralyze: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HitReactionBehavior {
    data: Arc<HitReactionData>,
}

impl HitReactionBehavior {
    #[must_use]
    pub fn level_for(&self, amount: f32) -> Option<u8> {
        (1..=HIT_REACTION_LEVELS)
            .rev()
            .find(|level| {
                self.data
                    .thresholds
                    .get(level.saturating_sub(1))
                    .copied()
                    .flatten()
                    .is_some_and(|threshold| amount >= threshold)
            })
            .and_then(|level| u8::try_from(level).ok())
    }

    #[must_use]
    pub fn timer(&self, level: u8) -> LogicFrames {
        usize::from(level)
            .checked_sub(1)
            .and_then(|at| self.data.timers.get(at).copied())
            .unwrap_or(LogicFrames::ZERO)
    }

    #[must_use]
    pub fn fast_hits_reset_reaction(&self) -> bool {
        self.data.fast_hits_reset_reaction
    }

    #[must_use]
    pub fn hits_paralyze(&self) -> bool {
        self.data.hits_paralyze
    }
}

impl Component for HitReactionBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for HitReactionBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn hit_reaction_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<HitReactionBehavior, FieldError> {
    let mut thresholds = [None; HIT_REACTION_LEVELS];
    let mut timers = [LogicFrames::ZERO; HIT_REACTION_LEVELS];
    for level in 1..=HIT_REACTION_LEVELS {
        let at = level.saturating_sub(1);
        let threshold_key = format!("HitReactionThreshold{level}");
        let timer_key = format!("HitReactionLifeTimer{level}");
        let threshold = match fields.field(&threshold_key) {
            None => None,
            Some(raw) => Some(
                fields
                    .field_numeric(&threshold_key)
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| {
                        FieldError::new(format!("{threshold_key} '{raw}' is not a damage amount"))
                    })?,
            ),
        };
        let timer_ms = match fields.field(&timer_key) {
            None => 0,
            Some(raw) => {
                let ms = fields
                    .field_numeric(&timer_key)
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| {
                        FieldError::new(format!("{timer_key} '{raw}' is not a duration"))
                    })?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let whole = ms.min(86_400_000.0) as u32;
                whole
            }
        };
        if let Some(slot) = thresholds.get_mut(at) {
            *slot = threshold;
        }
        if let Some(slot) = timers.get_mut(at) {
            *slot = crate::special_ability_update::retail_duration_ticks(timer_ms);
        }
    }
    Ok(HitReactionBehavior {
        data: Arc::new(HitReactionData {
            thresholds,
            timers,
            fast_hits_reset_reaction: optional_bool(fields, "FastHitsResetReaction")?,
            hits_paralyze: optional_bool(fields, "HitsParalyze")?,
        }),
    })
}

fn optional_bool(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        TestFields(
            pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
        )
    }

    #[test]
    fn the_highest_threshold_a_hit_reaches_picks_the_level() {
        let module = hit_reaction_behavior_from_fields(
            &fields(&[
                ("HitReactionThreshold1", "5.0"),
                ("HitReactionThreshold2", "25.0"),
                ("HitReactionThreshold3", "50.0"),
                ("HitReactionLifeTimer1", "100"),
                ("HitReactionLifeTimer2", "1000"),
                ("HitReactionLifeTimer3", "1993"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let levels: Vec<Option<u8>> = [4.9, 5.0, 24.9, 25.0, 49.9, 50.0, 1000.0]
            .into_iter()
            .map(|amount| module.level_for(amount))
            .collect();
        assert_eq!(
            levels,
            vec![None, Some(1), Some(1), Some(2), Some(2), Some(3), Some(3)]
        );
    }

    #[test]
    fn a_declaration_with_one_threshold_has_one_level() {
        let module = hit_reaction_behavior_from_fields(
            &fields(&[
                ("HitReactionThreshold1", "0.0"),
                ("HitReactionLifeTimer1", "2000"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.level_for(10_000.0), Some(1));
        assert_eq!(module.timer(1).get(), 60, "2000 ms is 60 frames");
        assert_eq!(module.timer(3), LogicFrames::ZERO);
    }

    #[test]
    fn an_unreadable_field_is_refused() {
        for pair in [
            ("HitReactionThreshold2", "hard"),
            ("HitReactionLifeTimer3", "a while"),
            ("FastHitsResetReaction", "Sometimes"),
        ] {
            assert!(
                hit_reaction_behavior_from_fields(&fields(&[pair]), Frame::ZERO).is_err(),
                "{pair:?}"
            );
        }
    }
}
