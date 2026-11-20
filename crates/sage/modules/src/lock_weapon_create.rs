use mint_sim::module::{Capability, CapabilityMask, CreateContext, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog};

#[derive(Debug, Clone, PartialEq)]
pub struct LockWeaponCreate {
    slot: u8,
}

impl LockWeaponCreate {
    #[must_use]
    pub fn slot(&self) -> u8 {
        self.slot
    }
}

impl Component for LockWeaponCreate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for LockWeaponCreate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Create);

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(ctx.entity, Effect::LockWeaponSlot(self.slot));
        }
    }
}

pub fn lock_weapon_create_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<LockWeaponCreate, FieldError> {
    let token = fields
        .field("SlotToLock")
        .ok_or_else(|| FieldError::new("SlotToLock is missing"))?;
    let slot = mint_sage_combat::WeaponSlot::parse(token)
        .and_then(|slot| u8::try_from(slot.index()).ok())
        .ok_or_else(|| FieldError::new(format!("SlotToLock {token} is no weapon slot")))?;
    Ok(LockWeaponCreate { slot })
}
