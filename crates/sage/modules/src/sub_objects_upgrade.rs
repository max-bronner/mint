use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{
    Component, Effects, Entity, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher,
};

use crate::effects::{names, Effect, EffectLog, Name, Names};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    show_sub_objects: Names,
    hide_sub_objects: Names,
    hide_sub_objects_on_remove: bool,
    unhide_sub_objects_on_remove: bool,
    upgrade_textures: Arc<[(Name, Name)]>,
    exclude_sub_objects: Names,
    recolor_house: Option<String>,
    fade_time_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubObjectsUpgrade {
    mux: UpgradeMuxData,
    data: Arc<Data>,
}

impl SubObjectsUpgrade {
    #[must_use]
    pub fn show_sub_objects(&self) -> Vec<&str> {
        self.data
            .show_sub_objects
            .iter()
            .map(AsRef::as_ref)
            .collect()
    }

    #[must_use]
    pub fn hide_sub_objects(&self) -> Vec<&str> {
        self.data
            .hide_sub_objects
            .iter()
            .map(AsRef::as_ref)
            .collect()
    }

    #[must_use]
    pub fn upgrade_textures(&self) -> Vec<(&str, &str)> {
        self.data
            .upgrade_textures
            .iter()
            .map(|(from, to)| (from.as_ref(), to.as_ref()))
            .collect()
    }

    fn texture_effect(&self) -> Option<Effect> {
        let data = &self.data;
        (!data.upgrade_textures.is_empty()).then(|| Effect::Texture {
            swaps: Arc::clone(&data.upgrade_textures),
            exclude: Names::clone(&data.exclude_sub_objects),
        })
    }

    #[must_use]
    pub fn has_fired(&self) -> bool {
        self.mux.has_fired()
    }
}

impl UpgradeEffect for SubObjectsUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        let data = &self.data;
        if !data.hide_sub_objects.is_empty() {
            out.push(Effect::SubObjects {
                show: Names::from(Vec::new()),
                hide: Names::clone(&data.hide_sub_objects),
                permanent: true,
            });
        }
        if !data.show_sub_objects.is_empty() {
            out.push(Effect::SubObjects {
                show: Names::clone(&data.show_sub_objects),
                hide: Names::from(Vec::new()),
                permanent: true,
            });
        }
        out.extend(self.texture_effect());
    }
}

impl Component for SubObjectsUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for SubObjectsUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn update(&mut self, ctx: &mut mint_sim::module::UpdateContext<'_>) -> mint_sim::module::Sleep {
        self.mux.update(ctx)
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut mint_sim::UpgradeContext<'_>) {
        attempt_upgrade_via_mux(self, active, ctx);
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }

    fn upgrade_removed_effects(
        &self,
        removed: &UpgradeMask,
        entity: Entity,
        mut effects: Effects<'_>,
    ) {
        let data = &self.data;
        if !self.mux.has_fired() || !self.mux.triggered_by().intersects(removed) {
            return;
        }
        let Some(log) = effects.get::<EffectLog>() else {
            return;
        };
        if data.hide_sub_objects_on_remove && !data.show_sub_objects.is_empty() {
            log.push(
                entity,
                Effect::SubObjects {
                    show: Names::from(Vec::new()),
                    hide: Names::clone(&data.show_sub_objects),
                    permanent: true,
                },
            );
        }
        if data.hide_sub_objects_on_remove {
            if let Some(texture) = self.texture_effect() {
                log.push(entity, texture);
            }
        }
        if data.unhide_sub_objects_on_remove && !data.hide_sub_objects.is_empty() {
            log.push(
                entity,
                Effect::SubObjects {
                    show: Names::clone(&data.hide_sub_objects),
                    hide: Names::from(Vec::new()),
                    permanent: true,
                },
            );
        }
    }
}

pub fn sub_objects_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SubObjectsUpgrade, FieldError> {
    let list = |keyword: &str| -> Vec<String> {
        fields
            .field_args(keyword)
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let flag = |keyword: &str| -> Result<bool, FieldError> {
        fields.field(keyword).map_or(Ok(false), |raw| {
            parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No")))
        })
    };
    let upgrade_textures = fields
        .field_args_each("UpgradeTexture")
        .into_iter()
        .filter_map(|line| match line.as_slice() {
            [from, _, to] => Some((Name::from(*from), Name::from(*to))),
            _ => None,
        })
        .collect();
    Ok(SubObjectsUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        data: Arc::new(Data {
            show_sub_objects: names(list("ShowSubObjects")),
            hide_sub_objects: names(list("HideSubObjects")),
            hide_sub_objects_on_remove: flag("HideSubObjectsOnRemove")?,
            unhide_sub_objects_on_remove: flag("UnHideSubObjectsOnRemove")?,
            upgrade_textures,
            exclude_sub_objects: names(list("ExcludeSubobjects")),
            recolor_house: fields.field("RecolorHouse").map(str::to_owned),
            fade_time_seconds: fields.field_numeric("FadeTimeInSeconds"),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::UpgradeBit;
    use mint_sim::Effects;
    use std::collections::BTreeMap;

    struct TestFields(
        BTreeMap<&'static str, Vec<&'static str>>,
        Vec<Vec<&'static str>>,
    );
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
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            if keyword.eq_ignore_ascii_case("UpgradeTexture") && !self.1.is_empty() {
                return self.1.clone();
            }
            let first = self.field_args(keyword);
            if first.is_empty() {
                Vec::new()
            } else {
                vec![first]
            }
        }
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(n) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(n));
                }
            }
            mask
        }
    }

    fn test_fields(pairs: &[(&'static str, Vec<&'static str>)]) -> TestFields {
        TestFields(pairs.iter().cloned().collect(), Vec::new())
    }

    fn entity() -> mint_sim::Entity {
        mint_sim::World::new().spawn().unwrap()
    }

    #[test]
    fn losing_the_trigger_hides_what_a_fired_instance_showed_and_shows_what_it_hid() {
        let fields = test_fields(&[
            ("TriggeredBy", vec!["1"]),
            ("ShowSubObjects", vec!["Sub_Shown"]),
            ("HideSubObjects", vec!["Sub_Hidden"]),
            ("HideSubObjectsOnRemove", vec!["Yes"]),
            ("UnHideSubObjectsOnRemove", vec!["Yes"]),
        ]);
        let mut module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let unfired = module.clone();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut log = EffectLog::new();
        let mut ctx = mint_sim::UpgradeContext {
            entity: entity(),
            now: Frame::ZERO,
            effects: Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        let _ = log.drain();

        unfired.upgrade_removed_effects(&active, entity(), Effects::into_sink(&mut log));
        assert!(
            log.drain().is_empty(),
            "an unfired instance reports nothing"
        );

        module.upgrade_removed_effects(&active, entity(), Effects::into_sink(&mut log));
        let mut state = crate::effects::UpgradeState::default();
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
        assert_eq!(
            [
                state.sub_object("Sub_Shown"),
                state.sub_object("Sub_Hidden")
            ],
            [Some(false), Some(true)]
        );
    }

    #[test]
    fn reads_show_and_hide_lists() {
        let fields = test_fields(&[
            ("ShowSubObjects", vec!["Sub_Orig"]),
            ("HideSubObjects", vec!["Sub_V2", "Sub_V3"]),
        ]);
        let module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.show_sub_objects(), ["Sub_Orig"]);
        assert_eq!(module.hide_sub_objects(), ["Sub_V2", "Sub_V3"]);
    }

    #[test]
    fn upgrade_texture_reads_the_first_and_third_token() {
        let fields = test_fields(&[(
            "UpgradeTexture",
            vec!["GUArcher.tga", "0", "GUArcher_HA.tga"],
        )]);
        let module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.upgrade_textures(),
            [("GUArcher.tga", "GUArcher_HA.tga")]
        );
        let short = test_fields(&[("UpgradeTexture", vec!["GUArcher.tga"])]);
        let module = sub_objects_upgrade_from_fields(&short, Frame::ZERO).unwrap();
        assert!(module.upgrade_textures().is_empty());
    }

    fn chest(trigger: &'static str, lines: Vec<Vec<&'static str>>) -> SubObjectsUpgrade {
        let mut fields = test_fields(&[
            ("TriggeredBy", vec![trigger]),
            ("HideSubObjectsOnRemove", vec!["Yes"]),
        ]);
        fields.1 = lines;
        sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap()
    }

    fn fire(module: &mut SubObjectsUpgrade, bit: u32, state: &mut crate::effects::UpgradeState) {
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(bit));
        let mut log = EffectLog::new();
        let mut ctx = mint_sim::UpgradeContext {
            entity: entity(),
            now: Frame::ZERO,
            effects: Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
    }

    fn in_force(state: &crate::effects::UpgradeState) -> Vec<(String, String)> {
        state
            .textures
            .iter()
            .map(|swap| (swap.from.to_string(), swap.to.to_string()))
            .collect()
    }

    #[test]
    fn every_upgrade_texture_line_is_in_force() {
        let mut one = chest(
            "1",
            vec![
                vec!["Body_B.tga", "0", "Body_A.tga"],
                vec!["Body_C.tga", "0", "Body_A.tga"],
                vec!["Body_Own.tga", "0", "Body_A.tga"],
            ],
        );
        assert_eq!(one.upgrade_textures().len(), 3);
        let mut state = crate::effects::UpgradeState::default();
        fire(&mut one, 1, &mut state);
        assert!(in_force(&state).contains(&("Body_Own.tga".into(), "Body_A.tga".into())));
    }

    #[test]
    fn a_firing_module_replaces_the_texture_list_in_force() {
        let a = vec![vec!["Body_Own.tga", "0", "Body_A.tga"]];
        let b = vec![vec!["Body_Own.tga", "0", "Body_B.tga"]];
        let (mut one, mut two) = (chest("1", a), chest("2", b));
        let mut state = crate::effects::UpgradeState::default();
        fire(&mut one, 1, &mut state);
        fire(&mut two, 2, &mut state);
        assert_eq!(
            in_force(&state),
            [("Body_Own.tga".into(), "Body_B.tga".into())]
        );
        let mut again = one.clone();
        let mut gone = UpgradeMask::EMPTY;
        gone.set(UpgradeBit::new(1));
        again.on_upgrade_removed(&gone);
        fire(&mut again, 1, &mut state);
        assert_eq!(
            in_force(&state),
            [("Body_Own.tga".into(), "Body_A.tga".into())]
        );
    }

    #[test]
    fn losing_the_trigger_with_hide_on_remove_puts_the_list_back_in_force() {
        let mut one = chest("1", vec![vec!["Body_Own.tga", "0", "Body_A.tga"]]);
        let mut state = crate::effects::UpgradeState::default();
        fire(&mut one, 1, &mut state);
        state.textures.clear();
        let mut removed = UpgradeMask::EMPTY;
        removed.set(UpgradeBit::new(1));
        let mut log = EffectLog::new();
        one.upgrade_removed_effects(&removed, entity(), Effects::into_sink(&mut log));
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
        assert_eq!(
            in_force(&state),
            [("Body_Own.tga".into(), "Body_A.tga".into())]
        );
    }

    #[test]
    fn firing_hides_before_it_shows_so_a_name_in_both_lists_ends_shown() {
        let fields = test_fields(&[
            ("TriggeredBy", vec!["1"]),
            ("ShowSubObjects", vec!["PartTestShared"]),
            ("HideSubObjects", vec!["PartTestShared", "PartTestOther"]),
        ]);
        let mut module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut log = EffectLog::new();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut ctx = mint_sim::UpgradeContext {
            entity: entity(),
            now: Frame::ZERO,
            effects: Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        let mut state = crate::effects::UpgradeState::default();
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
        assert_eq!(state.sub_object("PartTestShared"), Some(true));
        assert_eq!(state.sub_object("PartTestOther"), Some(false));
    }

    #[test]
    fn a_module_with_no_triggered_by_never_fires() {
        let fields = test_fields(&[]);
        let mut module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(0));
        let mut ctx = mint_sim::UpgradeContext {
            entity: entity(),
            now: Frame::ZERO,
            effects: Effects::none(),
        };
        module.attempt_upgrade(&active, &mut ctx);
        assert!(!module.has_fired());
    }

    #[test]
    fn a_texture_swap_reports_its_exclusions() {
        let fields = test_fields(&[
            ("TriggeredBy", vec!["1"]),
            (
                "UpgradeTexture",
                vec!["GUArcher.tga", "0", "GUArcher_HA.tga"],
            ),
            ("ExcludeSubobjects", vec!["FireArowTip"]),
        ]);
        let mut module = sub_objects_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut log = EffectLog::new();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut ctx = mint_sim::UpgradeContext {
            entity: entity(),
            now: Frame::ZERO,
            effects: Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        match &log.entries()[0].1 {
            Effect::Texture { swaps, exclude } => {
                assert_eq!(&*swaps[0].0, "GUArcher.tga");
                assert_eq!(&*swaps[0].1, "GUArcher_HA.tga");
                assert_eq!(&*exclude[0], "FireArowTip");
            }
            other => panic!("expected a texture swap, got {other:?}"),
        }
    }
}
