use mint_sim::module::{CapabilityMask, CaptureContext, ModuleKind, ModuleRegistry, OwnerChange};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, PlayerId, StateHasher, World,
};

use crate::effects::{Effect, EffectLog, Name, Names};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttributeModifierPoolUpdate;

impl Component for AttributeModifierPoolUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AttributeModifierPoolUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn attribute_modifier_pool_update_from_fields(
    _fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AttributeModifierPoolUpdate, FieldError> {
    Ok(AttributeModifierPoolUpdate)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShareExperienceBehavior {
    radius: f32,
    object_filter: Names,
    drop_off: f32,
}

impl ShareExperienceBehavior {
    #[must_use]
    pub const fn radius(&self) -> f32 {
        self.radius
    }

    #[must_use]
    pub fn object_filter(&self) -> &[Name] {
        &self.object_filter
    }

    #[must_use]
    pub const fn drop_off(&self) -> f32 {
        self.drop_off
    }
}

impl Component for ShareExperienceBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.radius);
        hasher.write_usize(self.object_filter.len());
        for name in self.object_filter.iter() {
            hasher.write_bytes(name.as_bytes());
        }
        hasher.write_f32(self.drop_off);
    }
}

impl ModuleKind for ShareExperienceBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn share_experience_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ShareExperienceBehavior, FieldError> {
    let radius = fields.field_numeric("Radius").unwrap_or(0.0);
    let drop_off = fields.field_numeric("DropOff").unwrap_or(0.0);
    let object_filter = fields
        .field_args("ObjectFilter")
        .into_iter()
        .map(Name::from)
        .collect();
    Ok(ShareExperienceBehavior {
        radius,
        object_filter,
        drop_off,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporarilyDefectUpdate {
    defect_duration: Option<LogicFrames>,
}

impl TemporarilyDefectUpdate {
    #[must_use]
    pub const fn defect_duration(self) -> Option<LogicFrames> {
        self.defect_duration
    }
}

impl Component for TemporarilyDefectUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.defect_duration.is_some());
        if let Some(duration) = self.defect_duration {
            duration.hash_into(hasher);
        }
    }
}

impl ModuleKind for TemporarilyDefectUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;

    fn on_capture(&mut self, old: PlayerId, _new: PlayerId, ctx: &mut CaptureContext<'_>) {
        let (OwnerChange::Defect, Some(lasts)) = (ctx.reason, self.defect_duration) else {
            return;
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::DefectFor {
                    back_to: old,
                    lasts,
                },
            );
        }
    }
}

pub fn temporarily_defect_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TemporarilyDefectUpdate, FieldError> {
    let defect_duration = match fields.field("DefectDuration") {
        None => None,
        Some(_) => {
            let ms = fields.field_numeric("DefectDuration").ok_or_else(|| {
                FieldError::new("DefectDuration is present but not a number/known macro")
            })?;
            if ms < 0.0 {
                return Err(FieldError::new(format!(
                    "DefectDuration '{ms}' is negative"
                )));
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some(LogicFrames::from_milliseconds(ms as u32))
        }
    };
    Ok(TemporarilyDefectUpdate { defect_duration })
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NotifyTargetsOfImminentProbableCrushingUpdate {
    time_between_updates: LogicFrames,
    scan_ahead_time: LogicFrames,
    scan_height: Option<f32>,
    scan_width: Option<f32>,
}

impl NotifyTargetsOfImminentProbableCrushingUpdate {
    #[must_use]
    pub const fn time_between_updates(&self) -> LogicFrames {
        self.time_between_updates
    }

    #[must_use]
    pub const fn scan_ahead_time(&self) -> LogicFrames {
        self.scan_ahead_time
    }

    #[must_use]
    pub const fn scan_height(&self) -> Option<f32> {
        self.scan_height
    }

    #[must_use]
    pub const fn scan_width(&self) -> Option<f32> {
        self.scan_width
    }
}

impl Component for NotifyTargetsOfImminentProbableCrushingUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.time_between_updates.get());
        hasher.write_u32(self.scan_ahead_time.get());
        hasher.write_f32(self.scan_height.unwrap_or(0.0));
        hasher.write_f32(self.scan_width.unwrap_or(0.0));
    }
}

impl ModuleKind for NotifyTargetsOfImminentProbableCrushingUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

const NOTIFY_DEFAULT_MS: u32 = 2000;

pub fn notify_targets_of_imminent_probable_crushing_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<NotifyTargetsOfImminentProbableCrushingUpdate, FieldError> {
    let number = |keyword: &str| -> Result<Option<f32>, FieldError> {
        match fields.field(keyword) {
            None => Ok(None),
            Some(raw) => fields
                .field_numeric(keyword)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(Some)
                .ok_or_else(|| {
                    FieldError::new(format!("{keyword} '{raw}' is not a non-negative number"))
                }),
        }
    };
    let duration = |keyword: &str| -> Result<LogicFrames, FieldError> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = number(keyword)?.map_or(NOTIFY_DEFAULT_MS, |ms| ms.min(86_400_000.0) as u32);
        Ok(LogicFrames::from_milliseconds(ms))
    };
    Ok(NotifyTargetsOfImminentProbableCrushingUpdate {
        time_between_updates: duration("TimeBetweenUpdatesMS")?,
        scan_ahead_time: duration("ScanAheadTimeMS")?,
        scan_height: number("ScanHeight")?.filter(|value| *value > 0.0),
        scan_width: number("ScanWidth")?.filter(|value| *value > 0.0),
    })
}

pub fn register_modifiers(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(
        world,
        "AttributeModifierPoolUpdate",
        attribute_modifier_pool_update_from_fields,
    );
    registry.register_loadable(
        world,
        "NotifyTargetsOfImminentProbableCrushingUpdate",
        notify_targets_of_imminent_probable_crushing_update_from_fields,
    );
    registry.register_loadable(
        world,
        "HordeNotifyTargetsOfImminentProbableCrushingUpdate",
        notify_targets_of_imminent_probable_crushing_update_from_fields,
    );
    registry.register_loadable(
        world,
        "ShareExperienceBehavior",
        share_experience_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "TemporarilyDefectUpdate",
        temporarily_defect_update_from_fields,
    );
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
            self.field(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn register_modifiers_registers_all_five_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_modifiers(&mut registry, &mut world);
        for name in [
            "AttributeModifierPoolUpdate",
            "NotifyTargetsOfImminentProbableCrushingUpdate",
            "HordeNotifyTargetsOfImminentProbableCrushingUpdate",
            "ShareExperienceBehavior",
            "TemporarilyDefectUpdate",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn attribute_modifier_pool_update_always_succeeds_and_reads_nothing() {
        let fields = TestFields(BTreeMap::new());
        assert!(attribute_modifier_pool_update_from_fields(&fields, Frame::ZERO).is_ok());
    }

    #[test]
    fn share_experience_behavior_reads_radius_filter_and_drop_off() {
        let fields = TestFields(BTreeMap::from([
            ("Radius", "100.0"),
            ("ObjectFilter", "ANY +HERO"),
            ("DropOff", "1.0"),
        ]));
        let module = share_experience_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.radius(), 100.0);
        assert_eq!(module.drop_off(), 1.0);
        assert_eq!(
            module.object_filter(),
            &[Name::from("ANY"), Name::from("+HERO")]
        );
    }

    #[test]
    fn temporarily_defect_update_reads_defect_duration_in_milliseconds() {
        let fields = TestFields(BTreeMap::from([("DefectDuration", "30000")]));
        let module = temporarily_defect_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.defect_duration(),
            Some(LogicFrames::from_milliseconds(30_000))
        );
    }

    #[test]
    fn temporarily_defect_update_with_no_duration_reads_as_none() {
        let fields = TestFields(BTreeMap::new());
        let module = temporarily_defect_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.defect_duration(), None);
    }

    #[test]
    fn a_negative_defect_duration_is_refused() {
        let fields = TestFields(BTreeMap::from([("DefectDuration", "-1")]));
        assert!(temporarily_defect_update_from_fields(&fields, Frame::ZERO).is_err());
    }
}
