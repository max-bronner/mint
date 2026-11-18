use mint_sage_ini::value::parse_bool;
use mint_sim::module::Death;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;

#[derive(Debug, Clone, PartialEq)]
pub struct RebuildHoleExposeDie {
    hole_name: Option<String>,
    hole_max_health: Option<f32>,
    fade_in: Option<LogicFrames>,
    transfer_attackers: bool,
    mux: DieMuxData,
}

impl RebuildHoleExposeDie {
    #[must_use]
    pub fn hole_name(&self) -> Option<&str> {
        self.hole_name.as_deref()
    }

    #[must_use]
    pub const fn hole_max_health(&self) -> Option<f32> {
        self.hole_max_health
    }

    #[must_use]
    pub const fn fade_in(&self) -> Option<LogicFrames> {
        self.fade_in
    }

    #[must_use]
    pub const fn transfer_attackers(&self) -> bool {
        self.transfer_attackers
    }

    #[must_use]
    pub fn exempts(&self, status: &str) -> bool {
        !self.mux.admits(&Death {
            status: &[status],
            ..Death::NONE
        })
    }

    #[must_use]
    pub fn admits(&self, death: &Death<'_>) -> bool {
        self.mux.admits(death)
    }
}

impl Component for RebuildHoleExposeDie {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_optional_string(hasher, self.hole_name.as_deref());
        hasher.write_bool(self.hole_max_health.is_some());
        hasher.write_f32(self.hole_max_health.unwrap_or(0.0));
        match self.fade_in {
            Some(frames) => {
                hasher.write_bool(true);
                frames.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_bool(self.transfer_attackers);
    }
}

fn hash_optional_string(hasher: &mut StateHasher, value: Option<&str>) {
    match value {
        Some(s) => {
            hasher.write_bool(true);
            hasher.write_usize(s.len());
            hasher.write_bytes(s.as_bytes());
        }
        None => hasher.write_bool(false),
    }
}

impl ModuleKind for RebuildHoleExposeDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn rebuild_hole_expose_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RebuildHoleExposeDie, FieldError> {
    let transfer_attackers = match fields.field("TransferAttackers") {
        None => true,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("TransferAttackers '{raw}' is not Yes/No")))?,
    };
    Ok(RebuildHoleExposeDie {
        hole_name: fields.field("HoleName").map(str::to_owned),
        hole_max_health: fields
            .field_numeric("HoleMaxHealth")
            .filter(|health| *health > 0.0),
        fade_in: fields
            .field_numeric("FadeInTimeSeconds")
            .filter(|seconds| (0.0..=3_600.0).contains(seconds))
            .map(LogicFrames::from_seconds),
        transfer_attackers,
        mux: DieMuxData::from_fields(fields)?,
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
    fn a_shipped_shape_declaration_reads_its_five_fields() {
        let fields = TestFields(BTreeMap::from([
            ("ExemptStatus", vec!["SOLD"]),
            ("HoleName", vec!["SomeStructureHole"]),
            ("HoleMaxHealth", vec!["500.0"]),
            ("FadeInTimeSeconds", vec!["2.0"]),
            ("TransferAttackers", vec!["No"]),
        ]));
        let module = rebuild_hole_expose_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.hole_name(), Some("SomeStructureHole"));
        assert_eq!(module.hole_max_health(), Some(500.0));
        assert_eq!(module.fade_in(), Some(LogicFrames::from_seconds(2.0)));
        assert!(!module.transfer_attackers());
        assert!(module.exempts("sold"));
        assert!(!module.exempts("UNDER_CONSTRUCTION"));
    }

    #[test]
    fn a_declaration_that_omits_transfer_attackers_transfers_them() {
        let fields = TestFields(BTreeMap::from([("HoleName", vec!["Hole"])]));
        let module = rebuild_hole_expose_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.transfer_attackers());
    }

    #[test]
    fn an_unparseable_transfer_attackers_is_refused() {
        let fields = TestFields(BTreeMap::from([("TransferAttackers", vec!["Sideways"])]));
        assert!(rebuild_hole_expose_die_from_fields(&fields, Frame::ZERO).is_err());
    }
}
