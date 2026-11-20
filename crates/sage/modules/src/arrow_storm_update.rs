use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, cap_persistent_count, AbilityTarget, AbilityUpdate, Trigger,
};
use crate::world_reads::ArrowStormShots;

#[derive(Debug, Clone, PartialEq)]
pub struct ArrowStormTrigger {
    weapon: Name,
    radius: f32,
    shots_per_burst: u32,
    shots_per_target: u32,
    max_shots: u32,
    can_shoot_empty_ground: bool,
}

fn place_of(target: AbilityTarget) -> PowerPlace {
    match target {
        AbilityTarget::Object(target) => PowerPlace::Object(target),
        AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
        AbilityTarget::None => PowerPlace::Source,
    }
}

impl Trigger for ArrowStormTrigger {
    fn prepare(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            entity,
            Effect::ArrowStormPrepare {
                at: place_of(target),
                radius: self.radius,
                shots_per_target: self.shots_per_target,
                max_shots: self.max_shots,
            },
        );
    }

    fn ends_cast_after_fire(&self, entity: Entity, ctx: &UpdateContext<'_>) -> bool {
        !self.can_shoot_empty_ground
            && ctx
                .reads
                .get::<ArrowStormShots>()
                .is_some_and(|shots| shots.left(entity) < self.shots_per_burst)
    }

    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        let at = place_of(target);
        log.push(
            entity,
            Effect::ArrowStorm {
                weapon: Name::clone(&self.weapon),
                at,
                radius: self.radius,
                shots_per_burst: self.shots_per_burst,
                shots_per_target: self.shots_per_target,
                max_shots: self.max_shots,
                can_shoot_empty_ground: self.can_shoot_empty_ground,
            },
        );
    }

    fn finish(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(entity, Effect::ClearArrowStorm);
    }
}

pub type ArrowStormUpdate = AbilityUpdate<ArrowStormTrigger>;

pub fn arrow_storm_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ArrowStormUpdate, FieldError> {
    let weapon = fields
        .field("WeaponTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("WeaponTemplate is missing"))?;
    let radius = fields
        .field_numeric("TargetRadius")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = |keyword: &str| -> u32 {
        fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value.min(1_000_000.0) as u32)
            .unwrap_or(0)
    };
    let shots_per_target = count("ShotsPerTarget").max(1);
    let shots_per_burst = count("ShotsPerBurst").max(1);
    let max_shots = count("MaxShots");
    let can_shoot_empty_ground =
        crate::fire_weapon_update::flag(fields, "CanShootEmptyGround") == Some(true);
    let trigger = ArrowStormTrigger {
        weapon,
        radius,
        shots_per_burst,
        shots_per_target,
        max_shots,
        can_shoot_empty_ground,
    };
    let mut data = ability_data_from_fields(fields, trigger)?;
    cap_persistent_count(&mut data, max_shots.div_ceil(shots_per_burst).max(1));
    Ok(AbilityUpdate {
        data: Arc::new(data),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
