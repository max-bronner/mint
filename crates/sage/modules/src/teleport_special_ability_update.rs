use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, busy_window_from_unpack, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub struct TeleportTrigger {
    source_weapon: Option<Name>,
    destination_weapon: Option<Name>,
}

impl Trigger for TeleportTrigger {
    fn begin(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        let (Some(weapon), Some(log)) = (&self.source_weapon, ctx.effects.get::<EffectLog>())
        else {
            return;
        };
        log.push(
            entity,
            Effect::FireWeapon {
                weapon: Name::clone(weapon),
                at: PowerPlace::Source,
                offset: (0.0, 0.0),
                repeat: None,
                alive_only: false,
                reload_slot: None,
            },
        );
    }

    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let at = match target {
            AbilityTarget::Object(target) => PowerPlace::Object(target),
            AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
            AbilityTarget::None => return,
        };
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(entity, Effect::Teleport { to: at, face: true });
        if let Some(weapon) = &self.destination_weapon {
            log.push(
                entity,
                Effect::FireWeapon {
                    weapon: Name::clone(weapon),
                    at,
                    offset: (0.0, 0.0),
                    repeat: None,
                    alive_only: false,
                    reload_slot: None,
                },
            );
        }
    }
}

pub type TeleportSpecialAbilityUpdate = AbilityUpdate<TeleportTrigger>;

pub fn teleport_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TeleportSpecialAbilityUpdate, FieldError> {
    let trigger = TeleportTrigger {
        source_weapon: fields.field("SourceWeaponName").map(Name::from),
        destination_weapon: fields.field("DestinationWeaponName").map(Name::from),
    };
    let data = busy_window_from_unpack(ability_data_from_fields(fields, trigger)?);
    Ok(AbilityUpdate {
        data: Arc::new(data),
        active: None,
        effect_until: None,
    })
}
