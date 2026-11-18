use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoreObjectsTrigger {
    power: Name,
    radius: f32,
}

impl Trigger for StoreObjectsTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let slot = ctx.requester.slot;
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
            Effect::StoreObjects {
                power: Name::clone(&self.power),
                radius: self.radius,
                at,
                slot,
            },
        );
    }
}

pub(crate) type StoreObjectsSpecialPower = AbilityUpdate<StoreObjectsTrigger>;

pub(crate) fn store_objects_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StoreObjectsSpecialPower, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let radius = fields.field_numeric("Radius").unwrap_or(0.0);
    let trigger = StoreObjectsTrigger { power, radius };
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
        assert!(store_objects_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }
}
