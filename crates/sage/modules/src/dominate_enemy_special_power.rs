use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{names, Effect, EffectLog, Name, Names, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

pub(crate) fn place_of(target: AbilityTarget) -> PowerPlace {
    match target {
        AbilityTarget::Object(entity) => PowerPlace::Object(entity),
        AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
        AbilityTarget::None => PowerPlace::Source,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DominateTrigger {
    radius: f32,
    affects: Names,
    permanent: bool,
    dominated_fx: Option<Name>,
    trigger_fx: Option<Name>,
}

impl Trigger for DominateTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        if let (Some(list), false) = (&self.trigger_fx, matches!(target, AbilityTarget::Object(_)))
        {
            log.push(
                entity,
                Effect::PlayFx {
                    list: Name::clone(list),
                    at: place_of(target),
                    on_object: false,
                    secondary: None,
                },
            );
        }
        log.push(
            entity,
            Effect::Dominate {
                at: place_of(target),
                radius: self.radius,
                affects: Names::clone(&self.affects),
                permanent: self.permanent,
                fx: self.dominated_fx.clone(),
            },
        );
    }
}

pub type DominateEnemySpecialPower = AbilityUpdate<DominateTrigger>;

pub fn dominate_enemy_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DominateEnemySpecialPower, FieldError> {
    let radius = fields
        .field_numeric("DominateRadius")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    let affects = names(
        fields
            .field_args("AttributeModifierAffects")
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let permanent = crate::fire_weapon_update::flag(fields, "PermanentlyConvert") == Some(true);
    let trigger = DominateTrigger {
        radius,
        affects,
        permanent,
        dominated_fx: fields.field("DominatedFX").map(Name::from),
        trigger_fx: fields.field("TriggerFX").map(Name::from),
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
    use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
    use mint_sim::{Activation, Effects, Pcg32, World};
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
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn missing_power_template_is_refused() {
        assert!(dominate_enemy_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn the_trigger_reports_one_dominate_where_the_cast_landed() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<DominateEnemySpecialPower>(&mut world, "DominateEnemySpecialPower");
        let holder = world.spawn().unwrap();
        let module = dominate_enemy_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorDominate"),
                ("StartAbilityRange", "200.0"),
                ("DominateRadius", "60"),
                ("AttributeModifierAffects", "ALL -HERO ENEMIES NEUTRAL"),
                ("PermanentlyConvert", "Yes"),
                ("DominatedFX", "FX_ModAuthorDominated"),
                ("UnpackTime", "1"),
                ("PreparationTime", "1"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorDominate",
            target: None,
            at: None,
        };
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut seen = None;
        for frame in 0..120u64 {
            let mut log = EffectLog::new();
            registry.update_with_reads(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                Reads::none(),
            );
            for (_, effect) in log.drain() {
                if let Effect::Dominate {
                    at,
                    radius,
                    permanent,
                    fx,
                    ..
                } = effect
                {
                    seen = Some((at, radius, permanent, fx));
                }
            }
        }
        let (at, radius, permanent, fx) = seen.expect("the trigger must report one dominate");
        assert_eq!(at, PowerPlace::Source);
        assert_eq!((radius, permanent), (60.0, true));
        assert_eq!(fx.as_deref(), Some("FX_ModAuthorDominated"));
    }

    #[test]
    fn an_unwritten_permanently_convert_is_a_temporary_dominate() {
        let module = dominate_enemy_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorDominate"),
                ("DominateRadius", "40"),
                ("AttributeModifierAffects", "NONE +ModAuthorUnit"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(!module.data.trigger.permanent);
        assert_eq!(
            &*module.data.trigger.affects,
            &[Name::from("NONE"), Name::from("+ModAuthorUnit")][..]
        );
    }
}
