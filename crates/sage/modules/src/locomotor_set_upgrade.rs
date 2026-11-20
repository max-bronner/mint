use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{
    Component, Effects, Entity, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher,
};

use crate::effects::{Effect, EffectLog};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct LocomotorSetUpgrade {
    mux: UpgradeMuxData,
    kill: bool,
}

impl UpgradeEffect for LocomotorSetUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        out.push(Effect::LocomotorUpgraded(!self.kill));
    }
}

impl Component for LocomotorSetUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_bool(self.kill);
    }
}

impl ModuleKind for LocomotorSetUpgrade {
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

    fn upgrade_removed_effects(
        &self,
        removed: &UpgradeMask,
        entity: Entity,
        mut effects: Effects<'_>,
    ) {
        if !self.mux.has_fired() || !self.mux.triggered_by().intersects(removed) {
            return;
        }
        if let Some(log) = effects.get::<EffectLog>() {
            log.push(entity, Effect::LocomotorUpgraded(false));
        }
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }
}

pub fn locomotor_set_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<LocomotorSetUpgrade, FieldError> {
    let kill = match fields.field("KillLocomotorUpgrade") {
        None => false,
        Some(raw) => mint_sage_ini::value::parse_bool(raw).ok_or_else(|| {
            FieldError::new(format!("KillLocomotorUpgrade '{raw}' is not Yes/No"))
        })?,
    };
    Ok(LocomotorSetUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        kill,
    })
}
