use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::Name;
use crate::object_filter::ObjectFilter;

const DEFAULT_UPDATE_PERIOD_MS: u32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvisibilityType {
    Camouflage,
    Stealth,
}

impl InvisibilityType {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Camouflage => "CAMOUFLAGE",
            Self::Stealth => "STEALTH",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        if raw.eq_ignore_ascii_case("CAMOUFLAGE") {
            Some(Self::Camouflage)
        } else if raw.eq_ignore_ascii_case("STEALTH") {
            Some(Self::Stealth)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealthCondition {
    Attacking,
    Moving,
    UsingAbility,
    FiringAny,
    TakingDamage,
    AwayFromTrees,
    HordebrainNotStealthed,
}

impl StealthCondition {
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Attacking => "ATTACKING",
            Self::Moving => "MOVING",
            Self::UsingAbility => "USING_ABILITY",
            Self::FiringAny => "FIRING_ANY",
            Self::TakingDamage => "TAKING_DAMAGE",
            Self::AwayFromTrees => "AWAY_FROM_TREES",
            Self::HordebrainNotStealthed => "HORDEBRAIN_NOT_STEALTHED",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        const ALL: [StealthCondition; 7] = [
            StealthCondition::Attacking,
            StealthCondition::Moving,
            StealthCondition::UsingAbility,
            StealthCondition::FiringAny,
            StealthCondition::TakingDamage,
            StealthCondition::AwayFromTrees,
            StealthCondition::HordebrainNotStealthed,
        ];
        ALL.into_iter()
            .find(|kind| raw.eq_ignore_ascii_case(kind.token()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvisibilityOption {
    UntoggleHiddenWhenLeavingStealth,
    AllowNearTrees,
    DetectedByFriendlies,
}

impl InvisibilityOption {
    fn parse(raw: &str) -> Option<Self> {
        const ALL: [(&str, InvisibilityOption); 3] = [
            (
                "UNTOGGLE_HIDDEN_WHEN_LEAVING_STEALTH",
                InvisibilityOption::UntoggleHiddenWhenLeavingStealth,
            ),
            ("ALLOW_NEAR_TREES", InvisibilityOption::AllowNearTrees),
            (
                "DETECTED_BY_FRIENDLIES",
                InvisibilityOption::DetectedByFriendlies,
            ),
        ];
        ALL.into_iter()
            .find(|(token, _)| raw.eq_ignore_ascii_case(token))
            .map(|(_, option)| option)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvisibilityNugget {
    pub kind: InvisibilityType,
    pub forbidden: Vec<StealthCondition>,
    pub forbidden_weapon_conditions: Vec<Name>,
    pub detection_range: Option<f32>,
    pub options: Vec<InvisibilityOption>,
    pub become_stealthed_fx: Option<Name>,
    pub exit_stealth_fx: Option<Name>,
    pub hint_detectable: Vec<Name>,
}

impl InvisibilityNugget {
    #[must_use]
    pub fn has(&self, option: InvisibilityOption) -> bool {
        self.options.contains(&option)
    }
}

#[derive(Debug, PartialEq)]
struct InvisibilityData {
    nugget: InvisibilityNugget,
    update_period: LogicFrames,
    starts_active: bool,
    required_upgrades: UpgradeMask,
    forbidden_upgrades: UpgradeMask,
    broadcast: bool,
    broadcast_range: Option<f32>,
    broadcast_filter: Arc<ObjectFilter>,
    detected_enemy: Option<String>,
    detected_ally: Option<String>,
    detected_owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvisibilityUpdate {
    data: Arc<InvisibilityData>,
}

impl InvisibilityUpdate {
    #[must_use]
    pub fn nugget(&self) -> &InvisibilityNugget {
        &self.data.nugget
    }

    #[must_use]
    pub fn update_period(&self) -> LogicFrames {
        self.data.update_period
    }

    #[must_use]
    pub fn starts_active(&self) -> bool {
        self.data.starts_active
    }

    #[must_use]
    pub fn required_upgrades(&self) -> &UpgradeMask {
        &self.data.required_upgrades
    }

    #[must_use]
    pub fn forbidden_upgrades(&self) -> &UpgradeMask {
        &self.data.forbidden_upgrades
    }

    #[must_use]
    pub fn broadcast(&self) -> bool {
        self.data.broadcast
    }

    #[must_use]
    pub fn broadcast_range(&self) -> Option<f32> {
        self.data.broadcast_range
    }

    #[must_use]
    pub fn broadcast_filter(&self) -> &ObjectFilter {
        &self.data.broadcast_filter
    }

    #[must_use]
    pub fn detected_enemy_event(&self) -> Option<&str> {
        self.data.detected_enemy.as_deref()
    }

    #[must_use]
    pub fn detected_ally_event(&self) -> Option<&str> {
        self.data.detected_ally.as_deref()
    }

    #[must_use]
    pub fn detected_owner_event(&self) -> Option<&str> {
        self.data.detected_owner.as_deref()
    }
}

impl Component for InvisibilityUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for InvisibilityUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn invisibility_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<InvisibilityUpdate, FieldError> {
    let mut nuggets: Vec<Result<InvisibilityNugget, FieldError>> = Vec::new();
    fields.nested("InvisibilityNugget", &mut |nested| {
        nuggets.push(nugget_from_fields(nested));
    });
    if nuggets.len() != 1 {
        return Err(FieldError::new(format!(
            "InvisibilityUpdate declares {} InvisibilityNugget blocks, not 1",
            nuggets.len()
        )));
    }
    let nugget = nuggets
        .pop()
        .unwrap_or_else(|| Err(FieldError::new("InvisibilityNugget missing")))?;
    let period_ms = number(fields, "UpdatePeriod")?.map_or(DEFAULT_UPDATE_PERIOD_MS, milliseconds);
    Ok(InvisibilityUpdate {
        data: Arc::new(InvisibilityData {
            nugget,
            update_period: LogicFrames::from_milliseconds(period_ms).max(LogicFrames::ONE),
            starts_active: flag(fields, "StartsActive")?.unwrap_or(false),
            required_upgrades: fields.field_upgrade_mask("RequiredUpgrades"),
            forbidden_upgrades: fields.field_upgrade_mask("ForbiddenUpgrades"),
            broadcast: flag(fields, "Broadcast")?.unwrap_or(false),
            broadcast_range: number(fields, "BroadcastRange")?,
            broadcast_filter: Arc::new(ObjectFilter::parse(
                &fields
                    .field_args("BroadcastObjectFilter")
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            )),
            detected_enemy: fields.field("EvaEventDetectedEnemy").map(str::to_owned),
            detected_ally: fields.field("EvaEventDetectedAlly").map(str::to_owned),
            detected_owner: fields.field("EvaEventDetectedOwner").map(str::to_owned),
        }),
    })
}

fn nugget_from_fields(fields: &dyn ModuleFieldSource) -> Result<InvisibilityNugget, FieldError> {
    let raw_type = fields
        .field("InvisibilityType")
        .ok_or_else(|| FieldError::new("InvisibilityNugget writes no InvisibilityType"))?;
    let kind = InvisibilityType::parse(raw_type).ok_or_else(|| {
        FieldError::new(format!(
            "InvisibilityType '{raw_type}' is not CAMOUFLAGE or STEALTH"
        ))
    })?;
    let forbidden = fields
        .field_args("ForbiddenConditions")
        .into_iter()
        .map(|token| {
            StealthCondition::parse(token).ok_or_else(|| {
                FieldError::new(format!("ForbiddenConditions '{token}' is not a condition"))
            })
        })
        .collect::<Result<Vec<_>, FieldError>>()?;
    let options = fields
        .field_args("Options")
        .into_iter()
        .map(|token| {
            InvisibilityOption::parse(token)
                .ok_or_else(|| FieldError::new(format!("Options '{token}' is not an option")))
        })
        .collect::<Result<Vec<_>, FieldError>>()?;
    Ok(InvisibilityNugget {
        kind,
        forbidden,
        forbidden_weapon_conditions: shared(fields.field_args("ForbiddenWeaponConditions")),
        detection_range: number(fields, "DetectionRange")?,
        options,
        become_stealthed_fx: fields.field("BecomeStealthedFX").map(Name::from),
        exit_stealth_fx: fields.field("ExitStealthFX").map(Name::from),
        hint_detectable: shared(fields.field_args("HintDetectableConditions")),
    })
}

fn shared(tokens: Vec<&str>) -> Vec<Name> {
    tokens.into_iter().map(Name::from).collect()
}

pub(crate) fn flag(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

pub(crate) fn number(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn milliseconds(value: f32) -> u32 {
    value.min(86_400_000.0) as u32
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct TestFields {
        own: BTreeMap<&'static str, Vec<&'static str>>,
        nested: Vec<(&'static str, BTreeMap<&'static str, Vec<&'static str>>)>,
    }

    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.own
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.own
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
            for (name, block) in &self.nested {
                if name.eq_ignore_ascii_case(keyword) {
                    let child = TestFields {
                        own: block.clone(),
                        nested: Vec::new(),
                    };
                    visit(&child);
                }
            }
        }
    }

    fn pairs(list: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, Vec<&'static str>> {
        list.iter()
            .map(|(k, v)| (*k, v.split_whitespace().collect()))
            .collect()
    }

    #[test]
    fn a_camouflage_declaration_reads_its_period_type_conditions_and_range() {
        let fields = TestFields {
            own: pairs(&[
                ("UpdatePeriod", "2000"),
                ("StartsActive", "Yes"),
                ("EvaEventDetectedEnemy", "StealthDiscovered"),
                ("EvaEventDetectedAlly", "StealthDiscoveredAlly"),
                ("EvaEventDetectedOwner", "StealthNeutralized"),
            ]),
            nested: vec![(
                "InvisibilityNugget",
                pairs(&[
                    ("InvisibilityType", "CAMOUFLAGE"),
                    ("ForbiddenConditions", "MOVING FIRING_ANY TAKING_DAMAGE"),
                    ("DetectionRange", "100"),
                    ("Options", "UNTOGGLE_HIDDEN_WHEN_LEAVING_STEALTH"),
                ]),
            )],
        };
        let read = invisibility_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(read.starts_active());
        assert_eq!(read.update_period(), LogicFrames::from_milliseconds(2000));
        assert_eq!(read.nugget().kind, InvisibilityType::Camouflage);
        assert_eq!(
            read.nugget().forbidden,
            vec![
                StealthCondition::Moving,
                StealthCondition::FiringAny,
                StealthCondition::TakingDamage
            ]
        );
        assert_eq!(read.nugget().detection_range, Some(100.0));
        assert!(read
            .nugget()
            .has(InvisibilityOption::UntoggleHiddenWhenLeavingStealth));
        assert_eq!(read.detected_enemy_event(), Some("StealthDiscovered"));
        assert_eq!(read.detected_ally_event(), Some("StealthDiscoveredAlly"));
        assert_eq!(read.detected_owner_event(), Some("StealthNeutralized"));
    }

    #[test]
    fn an_unknown_forbidden_condition_is_refused() {
        let fields = TestFields {
            own: pairs(&[]),
            nested: vec![(
                "InvisibilityNugget",
                pairs(&[
                    ("InvisibilityType", "STEALTH"),
                    ("ForbiddenConditions", "MOVING WHISTLING"),
                ]),
            )],
        };
        assert!(invisibility_update_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn a_declaration_without_exactly_one_nugget_is_refused() {
        let none = TestFields {
            own: pairs(&[("UpdatePeriod", "1000")]),
            nested: Vec::new(),
        };
        assert!(invisibility_update_from_fields(&none, Frame::ZERO).is_err());
    }
}
