use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::armor_upgrade::DEFAULT_UPGRADE_FLAG;
use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponSetUpgrade {
    mux: UpgradeMuxData,
    declared: Option<Arc<str>>,
    flag: Name,
}

impl WeaponSetUpgrade {
    #[must_use]
    pub fn weapon_condition(&self) -> Option<&str> {
        self.declared.as_deref()
    }

    #[must_use]
    pub fn flag(&self) -> &str {
        &self.flag
    }

    #[must_use]
    pub const fn is_raised(&self) -> bool {
        self.mux.has_fired()
    }
}

impl UpgradeEffect for WeaponSetUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        out.push(Effect::WeaponSetFlag(Name::clone(&self.flag)));
    }
}

impl Component for WeaponSetUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for WeaponSetUpgrade {
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

pub fn weapon_set_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<WeaponSetUpgrade, FieldError> {
    let declared: Option<Arc<str>> = fields.field("WeaponCondition").map(Arc::from);
    let flag = declared
        .clone()
        .unwrap_or_else(|| Name::from(DEFAULT_UPGRADE_FLAG));
    Ok(WeaponSetUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        declared,
        flag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectLog;
    use mint_sim::module::UpgradeBit;
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
            self.field(keyword).map(|v| vec![v]).unwrap_or_default()
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

    #[test]
    fn absent_weapon_condition_raises_player_upgrade() {
        let fields = TestFields(BTreeMap::new());
        let module = weapon_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.weapon_condition(), None);
        assert_eq!(module.flag(), DEFAULT_UPGRADE_FLAG);
    }

    #[test]
    fn a_declared_condition_is_read() {
        let fields = TestFields(BTreeMap::from([(
            "WeaponCondition",
            "WEAPONSET_CREATE_A_HERO_WS_01",
        )]));
        let module = weapon_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.weapon_condition(),
            Some("WEAPONSET_CREATE_A_HERO_WS_01")
        );
        assert_eq!(module.flag(), "WEAPONSET_CREATE_A_HERO_WS_01");
    }

    #[test]
    fn firing_reports_the_condition_exactly_once() {
        let fields = TestFields(BTreeMap::from([("TriggeredBy", "1")]));
        let mut module = weapon_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut log = EffectLog::new();
        for _ in 0..2 {
            let mut ctx = mint_sim::UpgradeContext {
                entity: mint_sim::World::new().spawn().unwrap(),
                now: Frame::ZERO,
                effects: mint_sim::Effects::into_sink(&mut log),
            };
            module.attempt_upgrade(&active, &mut ctx);
        }
        assert!(module.is_raised());
        assert_eq!(log.entries().len(), 1);
        assert_eq!(
            log.entries()[0].1,
            Effect::WeaponSetFlag(Name::from(DEFAULT_UPGRADE_FLAG))
        );
    }
}
