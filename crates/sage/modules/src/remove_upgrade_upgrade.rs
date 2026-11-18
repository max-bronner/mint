use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::Effect;
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    upgrade_to_remove: UpgradeMask,
    upgrade_groups_to_remove: Vec<String>,
    group_bits: UpgradeMask,
    suppress_eva_event_for_removal: bool,
    remove_from_all_player_objects: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoveUpgradeUpgrade {
    mux: UpgradeMuxData,
    data: Arc<Data>,
}

impl RemoveUpgradeUpgrade {
    #[must_use]
    pub fn bits_to_remove(&self) -> &UpgradeMask {
        &self.data.upgrade_to_remove
    }

    #[must_use]
    pub fn upgrade_groups_to_remove(&self) -> &[String] {
        &self.data.upgrade_groups_to_remove
    }

    #[must_use]
    pub fn suppresses_eva_event(&self) -> bool {
        self.data.suppress_eva_event_for_removal
    }

    #[must_use]
    pub fn removes_from_all_player_objects(&self) -> bool {
        self.data.remove_from_all_player_objects
    }
}

impl UpgradeEffect for RemoveUpgradeUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if !self.data.upgrade_to_remove.is_empty() {
            out.push(Effect::RemoveUpgrades(
                self.data.upgrade_to_remove.clone(),
                self.data.remove_from_all_player_objects,
            ));
        }
        if !self.data.group_bits.is_empty() {
            out.push(Effect::RemoveUpgrades(self.data.group_bits.clone(), false));
        }
    }
}

impl Component for RemoveUpgradeUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for RemoveUpgradeUpgrade {
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
}

pub fn remove_upgrade_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RemoveUpgradeUpgrade, FieldError> {
    let suppress_eva_event_for_removal = match fields.field("SuppressEvaEventForRemoval") {
        None => false,
        Some(raw) => parse_bool(raw).ok_or_else(|| {
            FieldError::new(format!("SuppressEvaEventForRemoval '{raw}' is not Yes/No"))
        })?,
    };
    let remove_from_all_player_objects = match fields.field("RemoveFromAllPlayerObjects") {
        None => false,
        Some(raw) => parse_bool(raw).ok_or_else(|| {
            FieldError::new(format!("RemoveFromAllPlayerObjects '{raw}' is not Yes/No"))
        })?,
    };
    let mux = UpgradeMuxData::from_fields(fields)?;
    let mut group_bits = fields.field_upgrade_group_mask("UpgradeGroupsToRemove");
    group_bits.clear_all(mux.triggered_by());
    Ok(RemoveUpgradeUpgrade {
        mux,
        data: Arc::new(Data {
            upgrade_to_remove: fields.field_upgrade_mask("UpgradeToRemove"),
            upgrade_groups_to_remove: fields
                .field_args("UpgradeGroupsToRemove")
                .into_iter()
                .map(str::to_owned)
                .collect(),
            group_bits,
            suppress_eva_event_for_removal,
            remove_from_all_player_objects,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::UpgradeBit;
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
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(n) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(n));
                }
            }
            mask
        }
        fn field_upgrade_group_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            if self.field_args(keyword).contains(&"GroupTest") {
                mask.set(UpgradeBit::new(3));
                mask.set(UpgradeBit::new(4));
            }
            mask
        }
    }

    #[test]
    fn a_group_removal_spares_the_modules_own_trigger() {
        let fields = TestFields(BTreeMap::from([
            ("TriggeredBy", vec!["4"]),
            ("UpgradeGroupsToRemove", vec!["GroupTest"]),
        ]));
        let mut module = remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut out = Vec::new();
        module.on_fire(&UpgradeMask::EMPTY, &mut out);
        let [Effect::RemoveUpgrades(removed, false)] = out.as_slice() else {
            panic!("one object-only removal, got {out:?}");
        };
        assert!(removed.contains(UpgradeBit::new(3)));
        assert!(!removed.contains(UpgradeBit::new(4)));
    }

    #[test]
    fn reads_upgrade_to_remove_and_suppress_flag() {
        let fields = TestFields(BTreeMap::from([
            ("UpgradeToRemove", vec!["7"]),
            ("SuppressEvaEventForRemoval", vec!["Yes"]),
        ]));
        let module = remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.bits_to_remove().contains(UpgradeBit::new(7)));
        assert!(module.suppresses_eva_event());
    }

    #[test]
    fn reads_upgrade_groups_to_remove_as_raw_tokens() {
        let fields = TestFields(BTreeMap::from([(
            "UpgradeGroupsToRemove",
            vec!["CreateAHero_Weapon"],
        )]));
        let module = remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.upgrade_groups_to_remove(), ["CreateAHero_Weapon"]);
        assert!(module.bits_to_remove().is_empty());
    }

    #[test]
    fn reads_remove_from_all_player_objects() {
        let fields = TestFields(BTreeMap::from([(
            "RemoveFromAllPlayerObjects",
            vec!["Yes"],
        )]));
        let module = remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(module.removes_from_all_player_objects());
    }

    #[test]
    fn absent_suppress_flag_defaults_to_false() {
        let fields = TestFields(BTreeMap::new());
        let module = remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert!(!module.suppresses_eva_event());
        assert!(!module.removes_from_all_player_objects());
    }

    #[test]
    fn an_unparseable_suppress_flag_is_refused() {
        let fields = TestFields(BTreeMap::from([(
            "SuppressEvaEventForRemoval",
            vec!["Sideways"],
        )]));
        assert!(remove_upgrade_upgrade_from_fields(&fields, Frame::ZERO).is_err());
    }
}
