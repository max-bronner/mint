use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingBehavior {
    night_window_name: Option<String>,
    glow_window_name: Option<String>,
    fire_window_name: Option<String>,
    fire_name: Option<String>,
}

impl BuildingBehavior {
    #[must_use]
    pub fn night_window_name(&self) -> Option<&str> {
        self.night_window_name.as_deref()
    }

    #[must_use]
    pub fn glow_window_name(&self) -> Option<&str> {
        self.glow_window_name.as_deref()
    }

    #[must_use]
    pub fn fire_window_name(&self) -> Option<&str> {
        self.fire_window_name.as_deref()
    }

    #[must_use]
    pub fn fire_name(&self) -> Option<&str> {
        self.fire_name.as_deref()
    }
}

impl Component for BuildingBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_optional_string(hasher, self.night_window_name.as_deref());
        hash_optional_string(hasher, self.glow_window_name.as_deref());
        hash_optional_string(hasher, self.fire_window_name.as_deref());
        hash_optional_string(hasher, self.fire_name.as_deref());
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

impl ModuleKind for BuildingBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn building_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<BuildingBehavior, FieldError> {
    Ok(BuildingBehavior {
        night_window_name: fields.field("NightWindowName").map(str::to_owned),
        glow_window_name: fields.field("GlowWindowName").map(str::to_owned),
        fire_window_name: fields.field("FireWindowName").map(str::to_owned),
        fire_name: fields.field("FireName").map(str::to_owned),
    })
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
        let module = building_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.night_window_name(), None);
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            ("NightWindowName", "WINDOW_N01"),
            ("FireName", "FIRE01"),
        ]));
        let module = building_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.night_window_name(), Some("WINDOW_N01"));
        assert_eq!(module.fire_name(), Some("FIRE01"));
    }
}
