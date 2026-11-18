use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TeleportGroupTrigger {
    power: Name,
    radius: f32,
    min_ring: f32,
    max_ring: f32,
    trigger_fx: Option<Name>,
    target_fx: Option<Name>,
}

impl Trigger for TeleportGroupTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let at = match target {
            AbilityTarget::Object(target) => PowerPlace::Object(target),
            AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
            AbilityTarget::None => return,
        };
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            entity,
            Effect::TeleportGroup {
                power: Name::clone(&self.power),
                radius: self.radius,
                min_ring: self.min_ring,
                max_ring: self.max_ring,
                trigger_fx: self.trigger_fx.clone(),
                target_fx: self.target_fx.clone(),
                at,
            },
        );
    }
}

pub(crate) type TeleportToCasterSpecialPower = AbilityUpdate<TeleportGroupTrigger>;

pub(crate) fn teleport_to_caster_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TeleportToCasterSpecialPower, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let trigger = TeleportGroupTrigger {
        power,
        radius: fields.field_numeric("Radius").unwrap_or(0.0),
        min_ring: fields.field_numeric("MinDestinationRadius").unwrap_or(0.0),
        max_ring: fields.field_numeric("MaxDestinationRadius").unwrap_or(0.0),
        trigger_fx: fields.field("TriggerFX").map(Name::from),
        target_fx: fields.field("TargetFX").map(Name::from),
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn missing_power_template_is_refused() {
        assert!(teleport_to_caster_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }
}
