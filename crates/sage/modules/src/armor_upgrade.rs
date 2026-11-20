use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{
    Component, Effects, Entity, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher,
};

use crate::effects::{Effect, EffectLog, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

pub const DEFAULT_UPGRADE_FLAG: &str = "PLAYER_UPGRADE";

#[derive(Debug, Clone, PartialEq)]
pub struct ArmorUpgrade {
    mux: UpgradeMuxData,
    declared: Option<Arc<str>>,
    flag: Name,
    ignore: bool,
    kill: bool,
}

#[must_use]
pub(crate) fn armor_set_condition(flag: &str) -> Name {
    if flag.eq_ignore_ascii_case("AS_TOWER") {
        Name::from("ARMORSET_PLAYER_UPGRADE")
    } else {
        Name::from(format!("ARMORSET_{flag}"))
    }
}

fn set_effects(flag: &Name, raise: bool) -> [Effect; 2] {
    let condition = vec![armor_set_condition(flag)];
    if raise {
        [
            Effect::ArmorSetFlag(Name::clone(flag)),
            Effect::ModelConditions {
                add: condition.into(),
                remove: Vec::new().into(),
            },
        ]
    } else {
        [
            Effect::ClearArmorSetFlag(Name::clone(flag)),
            Effect::ModelConditions {
                add: Vec::new().into(),
                remove: condition.into(),
            },
        ]
    }
}

impl ArmorUpgrade {
    #[must_use]
    pub fn armor_set_flag(&self) -> Option<&str> {
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

impl UpgradeEffect for ArmorUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if !self.ignore {
            out.extend(set_effects(&self.flag, !self.kill));
        }
    }
}

impl Component for ArmorUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for ArmorUpgrade {
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
        if self.ignore || !self.mux.undone_by(removed) {
            return;
        }
        if let Some(log) = effects.get::<EffectLog>() {
            for effect in set_effects(&self.flag, false) {
                log.push(entity, effect);
            }
        }
    }
}

pub fn armor_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ArmorUpgrade, FieldError> {
    let declared: Option<Arc<str>> = fields.field("ArmorSetFlag").map(Arc::from);
    let flag = declared
        .clone()
        .unwrap_or_else(|| Name::from(DEFAULT_UPGRADE_FLAG));
    Ok(ArmorUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        declared,
        flag,
        ignore: crate::fire_weapon_update::flag(fields, "IgnoreArmorUpgrade") == Some(true),
        kill: crate::fire_weapon_update::flag(fields, "KillArmorUpgrade") == Some(true),
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

    fn offer(module: &mut ArmorUpgrade, active: &UpgradeMask) -> EffectLog {
        let mut log = EffectLog::new();
        let mut ctx = mint_sim::UpgradeContext {
            entity: mint_sim::World::new().spawn().unwrap(),
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(active, &mut ctx);
        log
    }

    #[test]
    fn absent_armor_set_flag_raises_player_upgrade() {
        let fields = TestFields(BTreeMap::new());
        let module = armor_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.armor_set_flag(), None);
        assert_eq!(module.flag(), DEFAULT_UPGRADE_FLAG);
        assert!(!module.is_raised());
    }

    #[test]
    fn a_declared_flag_is_read() {
        let fields = TestFields(BTreeMap::from([("ArmorSetFlag", "AS_TOWER")]));
        let module = armor_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.armor_set_flag(), Some("AS_TOWER"));
        assert_eq!(module.flag(), "AS_TOWER");
    }

    #[test]
    fn firing_reports_the_flag_exactly_once() {
        let fields = TestFields(BTreeMap::from([("TriggeredBy", "1")]));
        let mut module = armor_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let first = offer(&mut module, &active);
        assert!(module.is_raised());
        assert_eq!(
            first
                .entries()
                .iter()
                .map(|(_, e)| e.clone())
                .collect::<Vec<_>>(),
            set_effects(&Name::from(DEFAULT_UPGRADE_FLAG), true).to_vec()
        );
        assert!(offer(&mut module, &active).is_empty(), "the latch holds");
    }

    fn fired(pairs: &[(&'static str, &'static str)]) -> (ArmorUpgrade, UpgradeMask) {
        let mut all = vec![("TriggeredBy", "1")];
        all.extend_from_slice(pairs);
        let mut module =
            armor_upgrade_from_fields(&TestFields(all.into_iter().collect()), Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        offer(&mut module, &active);
        (module, active)
    }

    fn taken_back(module: &ArmorUpgrade, removed: &UpgradeMask) -> Vec<Effect> {
        let mut log = EffectLog::new();
        module.upgrade_removed_effects(
            removed,
            mint_sim::World::new().spawn().unwrap(),
            mint_sim::Effects::into_sink(&mut log),
        );
        log.entries()
            .iter()
            .map(|(_, effect)| effect.clone())
            .collect()
    }

    #[test]
    fn an_armour_flag_raises_its_model_condition_unless_ignored() {
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut ignored = armor_upgrade_from_fields(
            &TestFields(BTreeMap::from([
                ("TriggeredBy", "1"),
                ("IgnoreArmorUpgrade", "Yes"),
            ])),
            Frame::ZERO,
        )
        .unwrap();
        assert!(offer(&mut ignored, &active).is_empty());
        for (flag, condition) in [
            ("PLAYER_UPGRADE_2", "ARMORSET_PLAYER_UPGRADE_2"),
            ("AS_TOWER", "ARMORSET_PLAYER_UPGRADE"),
        ] {
            let mut module = armor_upgrade_from_fields(
                &TestFields(BTreeMap::from([
                    ("TriggeredBy", "1"),
                    ("ArmorSetFlag", flag),
                ])),
                Frame::ZERO,
            )
            .unwrap();
            let effects: Vec<Effect> = offer(&mut module, &active)
                .entries()
                .iter()
                .map(|(_, effect)| effect.clone())
                .collect();
            assert!(
                effects.iter().any(|effect| matches!(
                    effect,
                    Effect::ModelConditions { add, .. } if add.iter().any(|name| &**name == condition)
                )),
                "{flag}"
            );
        }
    }

    #[test]
    fn losing_the_trigger_undoes_a_non_permanent_armour_upgrade() {
        let (module, removed) = fired(&[]);
        let effects = taken_back(&module, &removed);
        assert!(effects.contains(&Effect::ClearArmorSetFlag(Name::from(DEFAULT_UPGRADE_FLAG))));
        let (permanent, removed) = fired(&[("Permanent", "Yes")]);
        assert!(taken_back(&permanent, &removed).is_empty());
    }
}
