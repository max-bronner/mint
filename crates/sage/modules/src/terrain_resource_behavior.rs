use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

pub(crate) const VISIBLE_DEFAULT: bool = true;

pub(crate) const HIGH_PRIORITY_DEFAULT: bool = false;

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainResourceBehavior {
    radius: Option<f32>,
    max_income: Option<f32>,
    income_interval: Option<LogicFrames>,
    high_priority: bool,
    visible: bool,
    upgrade: Option<String>,
    upgrade_bonus_percent: Option<f32>,
    upgrade_must_be_present: UpgradeMask,
}

impl TerrainResourceBehavior {
    #[must_use]
    pub const fn radius(&self) -> Option<f32> {
        self.radius
    }

    #[must_use]
    pub const fn max_income(&self) -> Option<f32> {
        self.max_income
    }

    #[must_use]
    pub const fn income_interval(&self) -> Option<LogicFrames> {
        self.income_interval
    }

    #[must_use]
    pub const fn high_priority(&self) -> bool {
        self.high_priority
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    #[must_use]
    pub fn upgrade(&self) -> Option<&str> {
        self.upgrade.as_deref()
    }

    #[must_use]
    pub const fn upgrade_bonus_percent(&self) -> Option<f32> {
        self.upgrade_bonus_percent
    }

    #[must_use]
    pub const fn upgrade_must_be_present(&self) -> &UpgradeMask {
        &self.upgrade_must_be_present
    }
}

impl Component for TerrainResourceBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_optional_f32(hasher, self.radius);
        hash_optional_f32(hasher, self.max_income);
        match self.income_interval {
            Some(frames) => {
                hasher.write_bool(true);
                frames.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_bool(self.high_priority);
        hasher.write_bool(self.visible);
        match &self.upgrade {
            Some(s) => {
                hasher.write_bool(true);
                hasher.write_usize(s.len());
                hasher.write_bytes(s.as_bytes());
            }
            None => hasher.write_bool(false),
        }
        hash_optional_f32(hasher, self.upgrade_bonus_percent);
        self.upgrade_must_be_present.hash_into(hasher);
    }
}

fn hash_optional_f32(hasher: &mut StateHasher, value: Option<f32>) {
    hasher.write_bool(value.is_some());
    hasher.write_f32(value.unwrap_or(0.0));
}

impl ModuleKind for TerrainResourceBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn terrain_resource_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TerrainResourceBehavior, FieldError> {
    let income_interval = fields
        .field_numeric("IncomeInterval")
        .and_then(|ms| if ms >= 0.0 { Some(ms) } else { None })
        .map(|ms| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole_ms = ms as u32;
            LogicFrames::from_milliseconds(whole_ms)
        });
    Ok(TerrainResourceBehavior {
        radius: fields.field_numeric("Radius"),
        max_income: fields.field_numeric("MaxIncome"),
        income_interval,
        high_priority: optional_bool(fields, "HighPriority")?.unwrap_or(HIGH_PRIORITY_DEFAULT),
        visible: optional_bool(fields, "Visible")?.unwrap_or(VISIBLE_DEFAULT),
        upgrade: fields.field("Upgrade").map(str::to_owned),
        upgrade_bonus_percent: fields.field_numeric("UpgradeBonusPercent"),
        upgrade_must_be_present: fields.field_upgrade_mask("UpgradeMustBePresent"),
    })
}

fn optional_bool(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    #[test]
    fn no_fields_at_all_succeeds() {
        let fields = TestFields(BTreeMap::new());
        let module = terrain_resource_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.radius(), None);
        assert!(!module.high_priority());
        assert!(module.visible());
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            ("Radius", "160.0"),
            ("MaxIncome", "0"),
            ("IncomeInterval", "999999"),
            ("HighPriority", "Yes"),
        ]));
        let module = terrain_resource_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.radius(), Some(160.0));
        assert_eq!(
            module.income_interval(),
            Some(LogicFrames::from_milliseconds(999_999))
        );
        assert!(module.high_priority());
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        let fields = TestFields(BTreeMap::from([("Visible", "Sideways")]));
        assert!(terrain_resource_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }
}
