use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq)]
pub struct CastleBehavior {
    filter_valid_owned_entries: Vec<String>,
    max_castle_radius: Option<f32>,
    fade_time: Option<LogicFrames>,
    pre_built_plyr: Option<String>,
    keep_death_kills_everything: bool,
    eva_enemy_castle_sighted_event: Option<String>,
    instant_unpack: bool,
    scan_distance: Option<f32>,
    unpack_delay_time: Option<LogicFrames>,
}

impl CastleBehavior {
    #[must_use]
    pub fn filter_valid_owned_entries(&self) -> &[String] {
        &self.filter_valid_owned_entries
    }

    #[must_use]
    pub const fn max_castle_radius(&self) -> Option<f32> {
        self.max_castle_radius
    }

    #[must_use]
    pub const fn fade_time(&self) -> Option<LogicFrames> {
        self.fade_time
    }

    #[must_use]
    pub fn pre_built_plyr(&self) -> Option<&str> {
        self.pre_built_plyr.as_deref()
    }

    #[must_use]
    pub const fn keep_death_kills_everything(&self) -> bool {
        self.keep_death_kills_everything
    }

    #[must_use]
    pub fn eva_enemy_castle_sighted_event(&self) -> Option<&str> {
        self.eva_enemy_castle_sighted_event.as_deref()
    }

    #[must_use]
    pub const fn instant_unpack(&self) -> bool {
        self.instant_unpack
    }

    #[must_use]
    pub const fn scan_distance(&self) -> Option<f32> {
        self.scan_distance
    }

    #[must_use]
    pub const fn unpack_delay_time(&self) -> Option<LogicFrames> {
        self.unpack_delay_time
    }
}

impl Component for CastleBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.filter_valid_owned_entries.len());
        for token in &self.filter_valid_owned_entries {
            hasher.write_usize(token.len());
            hasher.write_bytes(token.as_bytes());
        }
        hash_optional_f32(hasher, self.max_castle_radius);
        hash_optional_frames(hasher, self.fade_time);
        hash_optional_string(hasher, self.pre_built_plyr.as_deref());
        hasher.write_bool(self.keep_death_kills_everything);
        hash_optional_string(hasher, self.eva_enemy_castle_sighted_event.as_deref());
        hasher.write_bool(self.instant_unpack);
        hash_optional_f32(hasher, self.scan_distance);
        hash_optional_frames(hasher, self.unpack_delay_time);
    }
}

fn hash_optional_f32(hasher: &mut StateHasher, value: Option<f32>) {
    hasher.write_bool(value.is_some());
    hasher.write_f32(value.unwrap_or(0.0));
}

fn hash_optional_frames(hasher: &mut StateHasher, value: Option<LogicFrames>) {
    match value {
        Some(frames) => {
            hasher.write_bool(true);
            frames.hash_into(hasher);
        }
        None => hasher.write_bool(false),
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

impl ModuleKind for CastleBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn castle_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CastleBehavior, FieldError> {
    Ok(CastleBehavior {
        filter_valid_owned_entries: fields
            .field_args("FilterValidOwnedEntries")
            .into_iter()
            .map(str::to_owned)
            .collect(),
        max_castle_radius: fields.field_numeric("MaxCastleRadius"),
        fade_time: fields
            .field_numeric("FadeTime")
            .map(LogicFrames::from_seconds),
        pre_built_plyr: fields.field("PreBuiltPlyr").map(str::to_owned),
        keep_death_kills_everything: optional_bool(fields, "KeepDeathKillsEverything")?
            .unwrap_or(false),
        eva_enemy_castle_sighted_event: fields
            .field("EvaEnemyCastleSightedEvent")
            .map(str::to_owned),
        instant_unpack: optional_bool(fields, "InstantUnpack")?.unwrap_or(false),
        scan_distance: fields.field_numeric("ScanDistance"),
        unpack_delay_time: fields
            .field_numeric("UnpackDelayTime")
            .map(LogicFrames::from_seconds),
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
    fn no_fields_at_all_succeeds() {
        let fields = TestFields(BTreeMap::new());
        let module = castle_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.filter_valid_owned_entries().is_empty());
        assert!(!module.instant_unpack());
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            (
                "FilterValidOwnedEntries",
                vec!["ANY", "+STRUCTURE", "+WALK_ON_TOP_OF_WALL"],
            ),
            ("MaxCastleRadius", vec!["280.0"]),
            ("FadeTime", vec!["2.0"]),
            ("InstantUnpack", vec!["Yes"]),
        ]));
        let module = castle_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.filter_valid_owned_entries(),
            ["ANY", "+STRUCTURE", "+WALK_ON_TOP_OF_WALL"]
        );
        assert_eq!(module.max_castle_radius(), Some(280.0));
        assert_eq!(module.fade_time(), Some(LogicFrames::from_seconds(2.0)));
        assert!(module.instant_unpack());
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        let fields = TestFields(BTreeMap::from([(
            "KeepDeathKillsEverything",
            vec!["Sideways"],
        )]));
        assert!(castle_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }
}
