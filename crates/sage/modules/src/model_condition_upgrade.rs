use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, ModuleKind, StateHasher,
};

use crate::effects::{names, Effect, Name, Names};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

const TEMP_FLAG_ATTRIBUTE: &str = "ModelConditionState";

#[derive(Debug, PartialEq)]
struct Data {
    add_condition_flags: Names,
    remove_condition_flags: Names,
    remove_condition_flags_in_range: Vec<String>,
    add_temp_condition_flag: Option<Name>,
    temp_condition_time: LogicFrames,
    permanent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelConditionUpgrade {
    mux: UpgradeMuxData,
    data: Arc<Data>,
}

impl ModelConditionUpgrade {
    #[must_use]
    pub fn add_condition_flags(&self) -> Vec<&str> {
        self.data
            .add_condition_flags
            .iter()
            .map(AsRef::as_ref)
            .collect()
    }

    #[must_use]
    pub fn remove_condition_flags(&self) -> Vec<&str> {
        self.data
            .remove_condition_flags
            .iter()
            .map(AsRef::as_ref)
            .collect()
    }

    #[must_use]
    pub fn remove_condition_flags_in_range(&self) -> &[String] {
        &self.data.remove_condition_flags_in_range
    }

    #[must_use]
    pub fn add_temp_condition_flag(&self) -> Option<&str> {
        self.data.add_temp_condition_flag.as_deref()
    }

    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.data.permanent
    }
}

impl UpgradeEffect for ModelConditionUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        let add: Names = match &self.data.add_temp_condition_flag {
            Some(temp) => self
                .data
                .add_condition_flags
                .iter()
                .chain(std::iter::once(temp))
                .cloned()
                .collect::<Vec<_>>()
                .into(),
            None => Names::clone(&self.data.add_condition_flags),
        };
        if add.is_empty() && self.data.remove_condition_flags.is_empty() {
            return;
        }
        out.push(Effect::ModelConditions {
            add,
            remove: Names::clone(&self.data.remove_condition_flags),
        });
    }
}

impl Component for ModelConditionUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for ModelConditionUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut mint_sim::UpgradeContext<'_>) {
        let fires = self.mux.would_fire(active);
        attempt_upgrade_via_mux(self, active, ctx);
        if let (true, Some(flag)) = (fires, &self.data.add_temp_condition_flag) {
            let flag = Name::clone(flag);
            self.mux
                .clear_after(flag, ctx.now, self.data.temp_condition_time);
        }
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        self.mux.update(ctx)
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }
}

pub fn model_condition_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ModelConditionUpgrade, FieldError> {
    let permanent = match fields.field("Permanent") {
        None => false,
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("Permanent '{raw}' is not Yes/No")))?,
    };
    let list = |keyword: &str| -> Vec<String> {
        fields
            .field_args(keyword)
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let temp_condition_time = match fields.field("TempConditionTime") {
        None => LogicFrames::ZERO,
        Some(raw) => fields
            .field_numeric("TempConditionTime")
            .filter(|seconds| seconds.is_finite())
            .map(LogicFrames::from_seconds)
            .ok_or_else(|| FieldError::new(format!("TempConditionTime '{raw}' is not seconds")))?,
    };
    Ok(ModelConditionUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        data: Arc::new(Data {
            add_condition_flags: names(list("AddConditionFlags")),
            remove_condition_flags: names(remove_list(fields)?),
            remove_condition_flags_in_range: list("RemoveConditionFlagsInRange"),
            add_temp_condition_flag: temp_flag(fields)?,
            temp_condition_time,
            permanent,
        }),
    })
}

fn remove_list(fields: &dyn ModuleFieldSource) -> Result<Vec<String>, FieldError> {
    let mut remove: Vec<String> = fields
        .field_args("RemoveConditionFlags")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let range = fields.field_args("RemoveConditionFlagsInRange");
    if range.is_empty() {
        return Ok(remove);
    }
    let spanned = match range.as_slice() {
        [first, last] => mint_sage_ini::model_condition_flags::model_condition_range(first, last),
        _ => None,
    }
    .ok_or_else(|| {
        FieldError::new(format!(
            "RemoveConditionFlagsInRange '{}' is not two model condition flags",
            range.join(" ")
        ))
    })?;
    remove.extend(spanned.iter().map(|flag| (*flag).to_owned()));
    Ok(remove)
}

fn temp_flag(fields: &dyn ModuleFieldSource) -> Result<Option<Name>, FieldError> {
    let Some(raw) = fields.field("AddTempConditionFlag") else {
        return Ok(None);
    };
    match raw.split_once(':') {
        Some((attribute, flag))
            if attribute.eq_ignore_ascii_case(TEMP_FLAG_ATTRIBUTE) && !flag.is_empty() =>
        {
            Ok(Some(Name::from(flag)))
        }
        _ => Err(FieldError::new(format!(
            "AddTempConditionFlag '{raw}' is not {TEMP_FLAG_ATTRIBUTE}:<flag>"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectLog;
    use std::collections::BTreeMap;

    #[test]
    fn firing_reports_the_add_and_remove_lists() {
        struct Fields;
        impl ModuleFieldSource for Fields {
            fn field(&self, keyword: &str) -> Option<&str> {
                self.field_args(keyword).first().copied()
            }
            fn field_args(&self, keyword: &str) -> Vec<&str> {
                match keyword {
                    "AddConditionFlags" => vec!["USER_1", "USER_2"],
                    "RemoveConditionFlags" => vec!["USER_3"],
                    _ => Vec::new(),
                }
            }
            fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
                let mut mask = UpgradeMask::EMPTY;
                if keyword == "TriggeredBy" {
                    mask.set(mint_sim::UpgradeBit::new(0));
                }
                mask
            }
        }
        let mut module = model_condition_upgrade_from_fields(&Fields, Frame::ZERO).unwrap();
        let mut log = EffectLog::new();
        let mut active = UpgradeMask::EMPTY;
        active.set(mint_sim::UpgradeBit::new(0));
        let mut ctx = mint_sim::UpgradeContext {
            entity: mint_sim::World::new().spawn().unwrap(),
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        match &log.entries()[0].1 {
            Effect::ModelConditions { add, remove } => {
                assert_eq!(add.len(), 2);
                assert_eq!(&*remove[0], "USER_3");
            }
            other => panic!("expected model conditions, got {other:?}"),
        }
    }

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
    fn reads_add_condition_flags_and_permanent() {
        let fields = TestFields(BTreeMap::from([
            ("AddConditionFlags", vec!["BUILD_VARIATION_TWO"]),
            ("Permanent", vec!["Yes"]),
        ]));
        let module = model_condition_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.add_condition_flags(), ["BUILD_VARIATION_TWO"]);
        assert!(module.is_permanent());
    }

    #[test]
    fn absent_permanent_defaults_to_false() {
        let fields = TestFields(BTreeMap::new());
        let module = model_condition_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(!module.is_permanent());
    }

    #[test]
    fn a_flag_range_joins_the_remove_list_and_a_bad_one_is_refused() {
        let fields = TestFields(BTreeMap::from([
            ("RemoveConditionFlags", vec!["USER_9"]),
            (
                "RemoveConditionFlagsInRange",
                vec!["BACKCRUSHED", "TOPPLED"],
            ),
        ]));
        let module = model_condition_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.remove_condition_flags(),
            ["USER_9", "TOPPLED", "FRONTCRUSHED", "BACKCRUSHED"]
        );
        for bad in [vec!["TOPPLED"], vec!["TOPPLED", "NOT_A_FLAG"]] {
            let fields = TestFields(BTreeMap::from([("RemoveConditionFlagsInRange", bad)]));
            assert!(model_condition_upgrade_from_fields(&fields, Frame::ZERO).is_err());
        }
    }

    #[test]
    fn an_unparseable_permanent_is_refused() {
        let fields = TestFields(BTreeMap::from([("Permanent", vec!["Sideways"])]));
        assert!(model_condition_upgrade_from_fields(&fields, Frame::ZERO).is_err());
    }

    fn conditions_by_frame(
        pairs: &[(&'static str, &'static str)],
    ) -> Vec<(u64, Vec<String>, Vec<String>)> {
        use mint_sim::module::{DestroyRequests, ModuleRegistry};
        struct Masked(TestFields);
        impl ModuleFieldSource for Masked {
            fn field(&self, keyword: &str) -> Option<&str> {
                self.0.field(keyword)
            }
            fn field_args(&self, keyword: &str) -> Vec<&str> {
                self.0.field_args(keyword)
            }
            fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
                let mut mask = UpgradeMask::EMPTY;
                if keyword == "TriggeredBy" {
                    mask.set(mint_sim::UpgradeBit::new(0));
                }
                mask
            }
        }
        let fields = Masked(TestFields(
            pairs
                .iter()
                .map(|(key, value)| (*key, value.split(' ').collect()))
                .collect(),
        ));
        let module = model_condition_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut world = mint_sim::World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ModelConditionUpgrade>(&mut world, "ModelConditionUpgrade");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(mint_sim::UpgradeBit::new(0));
        let mut rng = mint_sim::Pcg32::new(7);
        let mut out = Vec::new();
        for frame in 0..400 {
            let mut log = EffectLog::new();
            if frame == 2 {
                registry.broadcast_upgrade_into(
                    &mut world,
                    entity,
                    &active,
                    Frame::new(frame),
                    mint_sim::Effects::into_sink(&mut log),
                );
            }
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut DestroyRequests::new(),
                mint_sim::Effects::into_sink(&mut log),
            );
            for (_, effect) in log.drain() {
                if let Effect::ModelConditions { add, remove } = effect {
                    let owned = |list: &Names| list.iter().map(|name| name.to_string()).collect();
                    out.push((frame, owned(&add), owned(&remove)));
                }
            }
        }
        out
    }

    #[test]
    fn the_temp_flag_is_raised_at_the_grant_and_cleared_after_its_time() {
        let one = |name: &str| vec![name.to_owned()];
        assert_eq!(
            conditions_by_frame(&[
                ("AddConditionFlags", "UPGRADE_OIL"),
                ("AddTempConditionFlag", "ModelConditionState:USER_1"),
                ("TempConditionTime", "5.0"),
            ]),
            [
                (
                    2,
                    vec!["UPGRADE_OIL".to_owned(), "USER_1".to_owned()],
                    Vec::new()
                ),
                (152, Vec::new(), one("USER_1")),
            ]
        );
        assert_eq!(
            conditions_by_frame(&[("AddConditionFlags", "UPGRADE_OIL")]),
            [(2, one("UPGRADE_OIL"), Vec::new())]
        );
    }

    #[test]
    fn a_temp_flag_without_its_attribute_is_refused() {
        let fields = TestFields(BTreeMap::from([("AddTempConditionFlag", vec!["USER_2"])]));
        assert!(model_condition_upgrade_from_fields(&fields, Frame::ZERO).is_err());
    }
}
