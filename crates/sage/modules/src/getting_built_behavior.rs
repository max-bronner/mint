use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq)]
pub struct GettingBuiltBehavior {
    worker_name: Option<String>,
    evil_worker_name: Option<String>,
    test_faction: bool,
    spawn_timer_raw: Option<f32>,
    rebuild_when_dead: bool,
    self_building_loop: Option<String>,
    self_repair_from_damage_loop: Option<String>,
    self_repair_from_rubble_loop: Option<String>,
    rebuild_time: Option<LogicFrames>,
    use_spawn_timer_without_worker: bool,
    disallow_rebuild_range: Option<f32>,
    disallow_rebuild_filter: Vec<String>,
}

impl GettingBuiltBehavior {
    #[must_use]
    pub fn worker_name(&self) -> Option<&str> {
        self.worker_name.as_deref()
    }

    #[must_use]
    pub fn evil_worker_name(&self) -> Option<&str> {
        self.evil_worker_name.as_deref()
    }

    #[must_use]
    pub const fn test_faction(&self) -> bool {
        self.test_faction
    }

    #[must_use]
    pub const fn spawn_timer_raw(&self) -> Option<f32> {
        self.spawn_timer_raw
    }

    #[must_use]
    pub const fn rebuild_when_dead(&self) -> bool {
        self.rebuild_when_dead
    }

    #[must_use]
    pub fn self_building_loop(&self) -> Option<&str> {
        self.self_building_loop.as_deref()
    }

    #[must_use]
    pub fn self_repair_from_damage_loop(&self) -> Option<&str> {
        self.self_repair_from_damage_loop.as_deref()
    }

    #[must_use]
    pub fn self_repair_from_rubble_loop(&self) -> Option<&str> {
        self.self_repair_from_rubble_loop.as_deref()
    }

    #[must_use]
    pub const fn rebuild_time(&self) -> Option<LogicFrames> {
        self.rebuild_time
    }

    #[must_use]
    pub const fn use_spawn_timer_without_worker(&self) -> bool {
        self.use_spawn_timer_without_worker
    }

    #[must_use]
    pub const fn disallow_rebuild_range(&self) -> Option<f32> {
        self.disallow_rebuild_range
    }

    #[must_use]
    pub fn disallow_rebuild_filter(&self) -> &[String] {
        &self.disallow_rebuild_filter
    }
}

impl Component for GettingBuiltBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_optional_string(hasher, self.worker_name.as_deref());
        hash_optional_string(hasher, self.evil_worker_name.as_deref());
        hasher.write_bool(self.test_faction);
        hash_optional_f32(hasher, self.spawn_timer_raw);
        hasher.write_bool(self.rebuild_when_dead);
        hash_optional_string(hasher, self.self_building_loop.as_deref());
        hash_optional_string(hasher, self.self_repair_from_damage_loop.as_deref());
        hash_optional_string(hasher, self.self_repair_from_rubble_loop.as_deref());
        match self.rebuild_time {
            Some(frames) => {
                hasher.write_bool(true);
                frames.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_bool(self.use_spawn_timer_without_worker);
        hash_optional_f32(hasher, self.disallow_rebuild_range);
        hasher.write_usize(self.disallow_rebuild_filter.len());
        for token in &self.disallow_rebuild_filter {
            hasher.write_usize(token.len());
            hasher.write_bytes(token.as_bytes());
        }
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

fn hash_optional_f32(hasher: &mut StateHasher, value: Option<f32>) {
    hasher.write_bool(value.is_some());
    hasher.write_f32(value.unwrap_or(0.0));
}

impl ModuleKind for GettingBuiltBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn getting_built_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<GettingBuiltBehavior, FieldError> {
    let rebuild_time = fields
        .field_numeric("RebuildTimeSeconds")
        .map(LogicFrames::from_seconds);
    Ok(GettingBuiltBehavior {
        worker_name: fields.field("WorkerName").map(str::to_owned),
        evil_worker_name: fields.field("EvilWorkerName").map(str::to_owned),
        test_faction: optional_bool(fields, "TestFaction")?.unwrap_or(false),
        spawn_timer_raw: fields.field_numeric("SpawnTimer"),
        rebuild_when_dead: optional_bool(fields, "RebuildWhenDead")?.unwrap_or(false),
        self_building_loop: fields.field("SelfBuildingLoop").map(str::to_owned),
        self_repair_from_damage_loop: fields.field("SelfRepairFromDamageLoop").map(str::to_owned),
        self_repair_from_rubble_loop: fields.field("SelfRepairFromRubbleLoop").map(str::to_owned),
        rebuild_time,
        use_spawn_timer_without_worker: optional_bool(fields, "UseSpawnTimerWithoutWorker")?
            .unwrap_or(false),
        disallow_rebuild_range: fields.field_numeric("DisallowRebuildRange"),
        disallow_rebuild_filter: fields
            .field_args("DisallowRebuildFilter")
            .into_iter()
            .map(str::to_owned)
            .collect(),
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
        let module = getting_built_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.worker_name(), None);
        assert!(!module.test_faction());
        assert_eq!(module.rebuild_time(), None);
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let fields = TestFields(BTreeMap::from([
            ("WorkerName", vec!["GondorWorkerNoSelect"]),
            ("SpawnTimer", vec!["-1.0"]),
            ("RebuildTimeSeconds", vec!["180"]),
            ("UseSpawnTimerWithoutWorker", vec!["Yes"]),
            ("DisallowRebuildFilter", vec!["ANY", "+STRUCTURE"]),
        ]));
        let module = getting_built_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.worker_name(), Some("GondorWorkerNoSelect"));
        assert_eq!(module.spawn_timer_raw(), Some(-1.0));
        assert_eq!(
            module.rebuild_time(),
            Some(LogicFrames::from_seconds(180.0))
        );
        assert!(module.use_spawn_timer_without_worker());
        assert_eq!(module.disallow_rebuild_filter(), ["ANY", "+STRUCTURE"]);
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        let fields = TestFields(BTreeMap::from([("RebuildWhenDead", vec!["Sideways"])]));
        assert!(getting_built_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn absence_and_an_explicit_zero_hash_differently() {
        let mut absent = StateHasher::new();
        hash_optional_f32(&mut absent, None);
        let mut zero = StateHasher::new();
        hash_optional_f32(&mut zero, Some(0.0));
        assert_ne!(absent, zero);
    }
}
