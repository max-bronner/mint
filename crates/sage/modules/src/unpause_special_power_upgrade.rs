use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct UnpauseSpecialPowerUpgrade {
    mux: UpgradeMuxData,
    power: Name,
}

impl UnpauseSpecialPowerUpgrade {
    #[must_use]
    pub fn power(&self) -> &str {
        &self.power
    }

    #[must_use]
    pub const fn has_fired(&self) -> bool {
        self.mux.has_fired()
    }
}

impl UpgradeEffect for UnpauseSpecialPowerUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        out.push(Effect::UnpausePower(Name::clone(&self.power)));
    }
}

impl Component for UnpauseSpecialPowerUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for UnpauseSpecialPowerUpgrade {
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

pub fn unpause_special_power_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<UnpauseSpecialPowerUpgrade, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    Ok(UnpauseSpecialPowerUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        power: Name::from(power),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectLog;
    use mint_sim::module::UpgradeBit;

    struct Fields(Option<&'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            (keyword == "SpecialPowerTemplate")
                .then_some(self.0)
                .flatten()
        }
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            if keyword == "TriggeredBy" {
                mask.set(UpgradeBit::new(3));
            }
            mask
        }
    }

    #[test]
    fn firing_releases_the_named_power_once() {
        let mut module =
            unpause_special_power_upgrade_from_fields(&Fields(Some("Oil")), Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(3));
        let mut log = EffectLog::new();
        for _ in 0..2 {
            let mut ctx = mint_sim::UpgradeContext {
                entity: mint_sim::World::new().spawn().unwrap(),
                now: Frame::ZERO,
                effects: mint_sim::Effects::into_sink(&mut log),
            };
            module.attempt_upgrade(&active, &mut ctx);
        }
        assert_eq!(log.entries().len(), 1);
        assert_eq!(log.entries()[0].1, Effect::UnpausePower(Name::from("Oil")));
    }

    #[test]
    fn a_declaration_naming_no_power_is_refused() {
        assert!(unpause_special_power_upgrade_from_fields(&Fields(None), Frame::ZERO).is_err());
    }
}
