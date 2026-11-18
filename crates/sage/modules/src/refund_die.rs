use mint_sim::module::{CapabilityMask, Death, ModuleKind, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;

#[derive(Debug, Clone, PartialEq)]
pub struct RefundDie {
    refund_percent: f32,
    upgrade_required: UpgradeMask,
    building_required: Vec<String>,
    mux: DieMuxData,
}

impl RefundDie {
    #[must_use]
    pub const fn refund_percent(&self) -> f32 {
        self.refund_percent
    }

    #[must_use]
    pub const fn upgrade_required(&self) -> &UpgradeMask {
        &self.upgrade_required
    }

    #[must_use]
    pub fn building_required(&self) -> &[String] {
        &self.building_required
    }

    #[must_use]
    pub fn admits(&self, death: &Death<'_>) -> bool {
        self.mux.admits(death)
    }
}

impl Component for RefundDie {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.refund_percent);
        self.upgrade_required.hash_into(hasher);
        hasher.write_usize(self.building_required.len());
        for token in &self.building_required {
            hasher.write_usize(token.len());
            hasher.write_bytes(token.as_bytes());
        }
    }
}

impl ModuleKind for RefundDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn refund_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RefundDie, FieldError> {
    Ok(RefundDie {
        refund_percent: fields
            .field_percent("RefundPercent")
            .filter(|fraction| (0.0..=1.0).contains(fraction))
            .unwrap_or(0.0),
        upgrade_required: fields.field_upgrade_mask("UpgradeRequired"),
        mux: DieMuxData::from_fields(fields)?,
        building_required: fields
            .field_args("BuildingRequired")
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}

#[cfg(test)]
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

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            ("RefundPercent", vec!["50%"]),
            (
                "UpgradeRequired",
                vec!["Upgrade_MarketplaceUpgradeDefiance"],
            ),
            ("BuildingRequired", vec!["ANY", "+GondorMarketPlace"]),
        ]));
        let module = refund_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert!((module.refund_percent() - 0.5).abs() < 1e-6);
        assert_eq!(module.building_required(), ["ANY", "+GondorMarketPlace"]);
    }

    #[test]
    fn a_declaration_with_no_refund_percent_refunds_nothing() {
        let fields = TestFields(BTreeMap::new());
        let module = refund_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.refund_percent(), 0.0);
        assert!(module.upgrade_required().is_empty());
        assert!(module.building_required().is_empty());
    }
}
