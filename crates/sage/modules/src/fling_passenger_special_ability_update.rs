use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

const MAX_SPEED: f32 = 100_000.0;

#[derive(Debug, Clone, PartialEq)]
struct Fling {
    velocity: [f32; 3],
    landing_warhead: Option<Name>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlingTrigger {
    power: Name,
    fling: Fling,
}

impl Trigger for FlingTrigger {
    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::Fling {
                    power: Name::clone(&self.power),
                    velocity: self.fling.velocity,
                    landing_warhead: self.fling.landing_warhead.clone(),
                },
            );
        }
    }
}

pub type FlingPassengerSpecialAbilityUpdate = AbilityUpdate<FlingTrigger>;

pub fn fling_passenger_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FlingPassengerSpecialAbilityUpdate, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let trigger = FlingTrigger {
        power,
        fling: fling(fields)?,
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
        active: None,
        effect_until: None,
    })
}

fn fling(fields: &dyn ModuleFieldSource) -> Result<Fling, FieldError> {
    let args = fields.field_args("FlingPassengerVelocity");
    let bad = || {
        FieldError::new(format!(
            "FlingPassengerVelocity '{}' is not X:.. Y:.. Z:..",
            args.join(" ")
        ))
    };
    let mut velocity = [None; 3];
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        let (tag, value) = token.split_once(':').ok_or_else(bad)?;
        let slot = match tag.to_ascii_uppercase().as_str() {
            "X" => 0,
            "Y" => 1,
            "Z" => 2,
            _ => return Err(bad()),
        };
        let raw = if value.is_empty() {
            tokens.next().copied().ok_or_else(bad)?
        } else {
            value
        };
        let number = raw
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= MAX_SPEED)
            .ok_or_else(bad)?;
        match velocity.get_mut(slot) {
            Some(cell @ None) => *cell = Some(number),
            _ => return Err(bad()),
        }
    }
    let landing_warhead = match fields.field("FlingPassengerLandingWarhead") {
        None => None,
        Some(name) if name.trim().is_empty() => {
            return Err(FieldError::new(
                "FlingPassengerLandingWarhead names no weapon",
            ))
        }
        Some(name) => Some(Name::from(name)),
    };
    Ok(Fling {
        velocity: velocity.map(|axis| axis.unwrap_or(0.0)),
        landing_warhead,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .get(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
    }

    #[test]
    fn the_velocity_is_read_strictly() {
        let read = |velocity: &'static str| {
            fling(&Fields(BTreeMap::from([(
                "FlingPassengerVelocity",
                velocity,
            )])))
        };
        assert_eq!(read("X:1 Y: -2 Z:3").unwrap().velocity, [1.0, -2.0, 3.0]);
        assert_eq!(read("Z:4").unwrap().velocity, [0.0, 0.0, 4.0]);
        assert!(read("X:1 X:2").is_err());
        assert!(read("W:1").is_err());
        assert!(read("X:fast").is_err());
    }
}
