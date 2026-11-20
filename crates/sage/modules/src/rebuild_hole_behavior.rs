use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

const DEFAULT_REGEN_PER_SECOND: f32 = 0.001;

#[derive(Debug, Clone, PartialEq)]
pub struct RebuildHoleBehavior {
    worker_object_name: Option<String>,
    worker_respawn_delay: Option<LogicFrames>,
    regen_per_second: f32,
}

impl RebuildHoleBehavior {
    #[must_use]
    pub fn worker_object_name(&self) -> Option<&str> {
        self.worker_object_name.as_deref()
    }

    #[must_use]
    pub const fn worker_respawn_delay(&self) -> Option<LogicFrames> {
        self.worker_respawn_delay
    }

    #[must_use]
    pub const fn regen_per_second(&self) -> f32 {
        self.regen_per_second
    }
}

impl Component for RebuildHoleBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.worker_object_name.as_deref() {
            Some(name) => {
                hasher.write_bool(true);
                hasher.write_usize(name.len());
                hasher.write_bytes(name.as_bytes());
            }
            None => hasher.write_bool(false),
        }
        match self.worker_respawn_delay {
            Some(frames) => {
                hasher.write_bool(true);
                frames.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_f32(self.regen_per_second);
    }
}

impl ModuleKind for RebuildHoleBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn rebuild_hole_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RebuildHoleBehavior, FieldError> {
    Ok(RebuildHoleBehavior {
        worker_object_name: fields.field("WorkerObjectName").map(str::to_owned),
        worker_respawn_delay: fields
            .field_numeric("WorkerRespawnDelay")
            .filter(|ms| (0.0..=86_400_000.0).contains(ms))
            .map(|ms| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let whole = ms as u32;
                crate::special_ability_update::retail_duration_ticks(whole)
            }),
        regen_per_second: fields
            .field_percent("HoleHealthRegen%PerSecond")
            .filter(|fraction| (0.0..=1.0).contains(fraction))
            .unwrap_or(DEFAULT_REGEN_PER_SECOND),
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
    }

    #[test]
    fn a_percent_suffixed_regen_reads_as_a_fraction() {
        let fields = TestFields(BTreeMap::from([
            ("WorkerObjectName", vec!["SomeWorker"]),
            ("WorkerRespawnDelay", vec!["120000"]),
            ("HoleHealthRegen%PerSecond", vec!["0.5%"]),
        ]));
        let module = rebuild_hole_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.worker_object_name(), Some("SomeWorker"));
        assert_eq!(
            module.worker_respawn_delay(),
            Some(crate::special_ability_update::retail_duration_ticks(
                120_000
            ))
        );
        assert!((module.regen_per_second() - 0.005).abs() < 1e-6);
    }

    #[test]
    fn a_bare_zero_regen_is_no_regen_rather_than_the_default() {
        let fields = TestFields(BTreeMap::from([("HoleHealthRegen%PerSecond", vec!["0"])]));
        let module = rebuild_hole_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.regen_per_second(), 0.0);
    }

    #[test]
    fn a_declaration_that_writes_no_regen_gets_the_data_s_own_default() {
        let fields = TestFields(BTreeMap::new());
        let module = rebuild_hole_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.regen_per_second(), DEFAULT_REGEN_PER_SECOND);
        assert_eq!(module.worker_respawn_delay(), None);
    }
}
