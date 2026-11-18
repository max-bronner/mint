use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeGroupAudioUpdate {
    key: Vec<String>,
    unit_weight: u32,
}

impl LargeGroupAudioUpdate {
    #[must_use]
    pub fn key(&self) -> &[String] {
        &self.key
    }

    #[must_use]
    pub const fn unit_weight(&self) -> u32 {
        self.unit_weight
    }
}

impl Component for LargeGroupAudioUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.key.len());
        for tag in &self.key {
            hasher.write_usize(tag.len());
            hasher.write_bytes(tag.as_bytes());
        }
        hasher.write_u32(self.unit_weight);
    }
}

impl ModuleKind for LargeGroupAudioUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn large_group_audio_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<LargeGroupAudioUpdate, FieldError> {
    let key = fields
        .field_args("Key")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let unit_weight = match fields.field_numeric("UnitWeight") {
        None => 1,
        Some(value) if value >= 0.0 && value.fract() == 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let weight = value as u32;
            weight
        }
        Some(value) => {
            return Err(FieldError::new(format!(
                "UnitWeight '{value}' is not a non-negative whole number"
            )))
        }
    };
    Ok(LargeGroupAudioUpdate { key, unit_weight })
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
    fn reads_a_multi_token_key_and_defaults_unit_weight_to_one() {
        let fields = TestFields(BTreeMap::from([(
            "Key",
            vec!["Human", "Unit", "Infantry", "Rohan_Archer"],
        )]));
        let module = large_group_audio_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.key(), ["Human", "Unit", "Infantry", "Rohan_Archer"]);
        assert_eq!(module.unit_weight(), 1);
    }

    #[test]
    fn reads_an_explicit_unit_weight() {
        let fields = TestFields(BTreeMap::from([
            ("Key", vec!["Human", "Unit", "Infantry"]),
            ("UnitWeight", vec!["2"]),
        ]));
        let module = large_group_audio_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.unit_weight(), 2);
    }

    #[test]
    fn a_negative_or_fractional_unit_weight_is_refused() {
        let fields = TestFields(BTreeMap::from([("UnitWeight", vec!["-1"])]));
        assert!(large_group_audio_update_from_fields(&fields, Frame::ZERO).is_err());

        let fields = TestFields(BTreeMap::from([("UnitWeight", vec!["1.5"])]));
        assert!(large_group_audio_update_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn no_fields_at_all_succeeds() {
        let fields = TestFields(BTreeMap::new());
        let module = large_group_audio_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.key().is_empty());
        assert_eq!(module.unit_weight(), 1);
    }
}
