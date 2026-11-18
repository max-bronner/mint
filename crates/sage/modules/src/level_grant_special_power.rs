use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{names, Effect, EffectLog, Name, Names, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub struct LevelGrantTrigger {
    experience: u32,
    radius: f32,
    fx: Option<Name>,
    affects: Names,
}

impl Trigger for LevelGrantTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            entity,
            Effect::AreaLevelUp {
                at: match target {
                    AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
                    _ => PowerPlace::At(0.0, 0.0),
                },
                radius: self.radius,
                experience: self.experience,
                affects: Names::clone(&self.affects),
                fx: self.fx.clone(),
            },
        );
    }
}

pub type LevelGrantSpecialPower = AbilityUpdate<LevelGrantTrigger>;

pub fn level_grant_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<LevelGrantSpecialPower, FieldError> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let experience = fields
        .field_numeric("Experience")
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| value.min(1_000_000.0) as u32)
        .unwrap_or(0);
    let radius = fields
        .field_numeric("RadiusEffect")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    let fx = fields.field("LevelFX").map(Name::from);
    let affects = names(
        fields
            .field_args("AcceptanceFilter")
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let trigger = LevelGrantTrigger {
        experience,
        radius,
        fx,
        affects,
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
        assert!(level_grant_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn the_kings_favour_shape_reports_one_area_grant_at_the_trigger() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<LevelGrantSpecialPower>(&mut world, "LevelGrantSpecialPower");
        let holder = world.spawn().unwrap();
        let module = level_grant_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpecialAbilityKingsFavor"),
                ("StartAbilityRange", "200.0"),
                ("LevelFX", "FX_LevelUp"),
                ("Experience", "50"),
                ("RadiusEffect", "150"),
                ("AcceptanceFilter", "ANY +CAVALRY +INFANTRY ALLIES"),
                ("UnpackTime", "3000"),
                ("PreparationTime", "0"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "SpecialAbilityKingsFavor",
            target: None,
            at: Some((300.0, 400.0)),
        };
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));

        let positions =
            crate::world_reads::Positions::from_pairs([(holder, (250.0, 400.0, 0.0, 0.0))]);
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut granted = None;
        for frame in 0..120u64 {
            let mut log = EffectLog::new();
            registry.update_with_reads(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                Reads::from_snapshot(&positions),
            );
            for (_, effect) in log.drain() {
                if let Effect::AreaLevelUp {
                    at,
                    radius,
                    experience,
                    fx,
                    ..
                } = effect
                {
                    granted = Some((at, radius, experience, fx));
                }
            }
        }
        let (at, radius, experience, fx) = granted.expect("the trigger must report an area grant");
        assert_eq!(at, PowerPlace::At(300.0, 400.0));
        assert_eq!((radius, experience), (150.0, 50));
        assert_eq!(fx.as_deref(), Some("FX_LevelUp"));
    }

    #[test]
    fn a_mod_authored_shape_reads_its_own_names() {
        let module = level_grant_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorGrant"),
                ("Experience", "12"),
                ("RadiusEffect", "40"),
                ("LevelFX", "FX_ModAuthor"),
                ("AcceptanceFilter", "NONE +ModAuthorUnit"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.data.trigger.experience, 12);
        assert_eq!(module.data.trigger.radius, 40.0);
        assert_eq!(module.data.trigger.fx.as_deref(), Some("FX_ModAuthor"));
        assert_eq!(
            &*module.data.trigger.affects,
            &[Name::from("NONE"), Name::from("+ModAuthorUnit")][..]
        );
    }
}
