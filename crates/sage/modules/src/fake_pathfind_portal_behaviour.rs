use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::upgrade_mux::UpgradeMuxData;

#[derive(Debug, Clone, PartialEq)]
pub struct FakePathfindPortalBehaviour {
    allow_enemies: bool,
    allow_non_skirmish_ai_units: bool,
    mux: UpgradeMuxData,
}

impl FakePathfindPortalBehaviour {
    #[must_use]
    pub const fn allow_enemies(&self) -> bool {
        self.allow_enemies
    }

    #[must_use]
    pub const fn allow_non_skirmish_ai_units(&self) -> bool {
        self.allow_non_skirmish_ai_units
    }
}

impl Component for FakePathfindPortalBehaviour {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for FakePathfindPortalBehaviour {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }
}

pub fn fake_pathfind_portal_behaviour_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FakePathfindPortalBehaviour, FieldError> {
    Ok(FakePathfindPortalBehaviour {
        allow_enemies: flag(fields, "AllowEnemies")?,
        allow_non_skirmish_ai_units: flag(fields, "AllowNonSkirmishAIUnits")?,
        mux: UpgradeMuxData::from_fields(fields)?,
    })
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<bool, FieldError> {
    match fields.field(keyword) {
        None => Ok(false),
        Some(raw) => parse_bool(raw)
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
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword).into_iter().collect()
        }
    }

    #[test]
    fn the_shipped_gate_admits_neither_enemies_nor_non_skirmish_ai_units() {
        let fields = TestFields(BTreeMap::from([
            ("AllowEnemies", "No"),
            ("AllowNonSkirmishAIUnits", "No"),
        ]));
        let module = fake_pathfind_portal_behaviour_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(!module.allow_enemies());
        assert!(!module.allow_non_skirmish_ai_units());
    }

    #[test]
    fn a_flag_that_is_not_yes_or_no_is_refused() {
        let fields = TestFields(BTreeMap::from([("AllowEnemies", "Sometimes")]));
        assert!(fake_pathfind_portal_behaviour_from_fields(&fields, Frame::ZERO).is_err());
    }
}
