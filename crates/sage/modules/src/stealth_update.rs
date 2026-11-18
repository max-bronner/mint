use std::sync::Arc;

use mint_sim::module::{CapabilityMask, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::Name;
use crate::invisibility_update::{flag, number, StealthCondition};

const DEFAULT_PULSE_FRAMES: u32 = 30;

#[derive(Debug, PartialEq)]
struct StealthData {
    stealth_delay: LogicFrames,
    forbidden: Vec<StealthCondition>,
    hint_detectable: Vec<Name>,
    pulse_frequency: LogicFrames,
    detected_by_anyone_range: Option<f32>,
    reveal_weapon_sets: Vec<Name>,
    reveal_distance_from_target: Option<f32>,
    order_idle_enemies_to_attack: bool,
    innate_stealth: bool,
    starts_active: bool,
    disguises_as_team: bool,
    disguise_transition: (LogicFrames, LogicFrames),
    required_upgrades: UpgradeMask,
    remove_terrain_restriction_on_upgrade: UpgradeMask,
    friendly_opacity: (Option<f32>, Option<f32>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StealthUpdate {
    data: Arc<StealthData>,
}

impl StealthUpdate {
    #[must_use]
    pub fn stealth_delay(&self) -> LogicFrames {
        self.data.stealth_delay
    }

    #[must_use]
    pub fn forbidden(&self) -> &[StealthCondition] {
        &self.data.forbidden
    }

    #[must_use]
    pub fn hint_detectable(&self) -> &[Name] {
        &self.data.hint_detectable
    }

    #[must_use]
    pub fn pulse_frequency(&self) -> LogicFrames {
        self.data.pulse_frequency
    }

    #[must_use]
    pub fn detected_by_anyone_range(&self) -> Option<f32> {
        self.data.detected_by_anyone_range
    }

    #[must_use]
    pub fn reveal_weapon_sets(&self) -> &[Name] {
        &self.data.reveal_weapon_sets
    }

    #[must_use]
    pub fn reveal_distance_from_target(&self) -> Option<f32> {
        self.data.reveal_distance_from_target
    }

    #[must_use]
    pub fn order_idle_enemies_to_attack(&self) -> bool {
        self.data.order_idle_enemies_to_attack
    }

    #[must_use]
    pub fn innate_stealth(&self) -> bool {
        self.data.innate_stealth
    }

    #[must_use]
    pub fn starts_active(&self) -> bool {
        self.data.starts_active
    }

    #[must_use]
    pub fn disguises_as_team(&self) -> bool {
        self.data.disguises_as_team
    }

    #[must_use]
    pub fn disguise_transition(&self) -> (LogicFrames, LogicFrames) {
        self.data.disguise_transition
    }

    #[must_use]
    pub fn required_upgrades(&self) -> &UpgradeMask {
        &self.data.required_upgrades
    }

    #[must_use]
    pub fn remove_terrain_restriction_on_upgrade(&self) -> &UpgradeMask {
        &self.data.remove_terrain_restriction_on_upgrade
    }

    #[must_use]
    pub fn friendly_opacity(&self) -> (Option<f32>, Option<f32>) {
        self.data.friendly_opacity
    }
}

impl Component for StealthUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for StealthUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn stealth_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StealthUpdate, FieldError> {
    let forbidden = fields
        .field_args("StealthForbiddenConditions")
        .into_iter()
        .map(|token| {
            StealthCondition::parse(token).ok_or_else(|| {
                FieldError::new(format!(
                    "StealthForbiddenConditions '{token}' is not a condition"
                ))
            })
        })
        .collect::<Result<Vec<_>, FieldError>>()?;
    Ok(StealthUpdate {
        data: Arc::new(StealthData {
            stealth_delay: duration(fields, "StealthDelay")?.unwrap_or(LogicFrames::ZERO),
            forbidden,
            hint_detectable: names(fields, "HintDetectableConditions"),
            pulse_frequency: duration(fields, "PulseFrequency")?
                .unwrap_or(LogicFrames::from_frames(DEFAULT_PULSE_FRAMES)),
            detected_by_anyone_range: number(fields, "DetectedByAnyoneRange")?
                .filter(|range| *range > 0.0),
            reveal_weapon_sets: names(fields, "RevealWeaponSets"),
            reveal_distance_from_target: number(fields, "RevealDistanceFromTarget")?
                .filter(|range| *range > 0.0),
            order_idle_enemies_to_attack: flag(fields, "OrderIdleEnemiesToAttackMeUponReveal")?
                .unwrap_or(false),
            innate_stealth: flag(fields, "InnateStealth")?.unwrap_or(true),
            starts_active: flag(fields, "StartsActive")?.unwrap_or(true),
            disguises_as_team: flag(fields, "DisguisesAsTeam")?.unwrap_or(false),
            disguise_transition: (
                duration(fields, "DisguiseTransitionTime")?.unwrap_or(LogicFrames::ZERO),
                duration(fields, "DisguiseRevealTransitionTime")?.unwrap_or(LogicFrames::ZERO),
            ),
            required_upgrades: fields.field_upgrade_mask("RequiredUpgradeNames"),
            remove_terrain_restriction_on_upgrade: fields
                .field_upgrade_mask("RemoveTerrainRestrictionOnUpgrade"),
            friendly_opacity: (
                percent(fields, "FriendlyOpacityMin")?,
                percent(fields, "FriendlyOpacityMax")?,
            ),
        }),
    })
}

fn names(fields: &dyn ModuleFieldSource, keyword: &str) -> Vec<Name> {
    fields
        .field_args(keyword)
        .into_iter()
        .map(Name::from)
        .collect()
}

fn percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => fields
            .field_percent(keyword)
            .filter(|value| value.is_finite())
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage"))),
    }
}

fn duration(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<LogicFrames>, FieldError> {
    Ok(number(fields, keyword)?.map(|ms| LogicFrames::from_milliseconds(milliseconds(ms))))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn milliseconds(value: f32) -> u32 {
    value.min(86_400_000.0) as u32
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn field_numeric(&self, keyword: &str) -> Option<f32> {
            self.field(keyword).and_then(|value| value.parse().ok())
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn an_elven_tree_stealth_declaration_reads_its_delay_conditions_and_ranges() {
        let read = stealth_update_from_fields(
            &fields(&[
                ("StealthDelay", "500"),
                ("PulseFrequency", "750"),
                ("StealthForbiddenConditions", "AWAY_FROM_TREES ATTACKING"),
                ("OrderIdleEnemiesToAttackMeUponReveal", "Yes"),
                ("DetectedByAnyoneRange", "120"),
                ("RevealWeaponSets", "CLOSE_RANGE CONTESTING_BUILDING"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(read.stealth_delay(), LogicFrames::from_milliseconds(500));
        assert_eq!(
            read.forbidden(),
            [StealthCondition::AwayFromTrees, StealthCondition::Attacking]
        );
        assert_eq!(read.detected_by_anyone_range(), Some(120.0));
        assert_eq!(read.reveal_weapon_sets().len(), 2);
        assert!(read.order_idle_enemies_to_attack());
        assert!(read.innate_stealth());
    }

    #[test]
    fn an_unknown_forbidden_condition_is_refused() {
        assert!(stealth_update_from_fields(
            &fields(&[("StealthForbiddenConditions", "ATTACKING SINGING")]),
            Frame::ZERO
        )
        .is_err());
    }
}
