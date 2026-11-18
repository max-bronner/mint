use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct BuildableHeroListUpgrade {
    mux: UpgradeMuxData,
}

impl UpgradeEffect for BuildableHeroListUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }
}

impl Component for BuildableHeroListUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for BuildableHeroListUpgrade {
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

pub fn buildable_hero_list_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<BuildableHeroListUpgrade, FieldError> {
    Ok(BuildableHeroListUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
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
            self.0.get(keyword).and_then(|v| v.first().copied())
        }

        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0.get(keyword).cloned().unwrap_or_default()
        }

        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(bit) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(bit));
                }
            }
            mask
        }
    }

    #[test]
    fn triggered_by_is_read_through_the_mixin() {
        let fields = TestFields(BTreeMap::from([("TriggeredBy", vec!["7"])]));
        let module = buildable_hero_list_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(7));
        assert!(module.would_upgrade(&active));
        assert!(!module.would_upgrade(&UpgradeMask::EMPTY));
    }
}
