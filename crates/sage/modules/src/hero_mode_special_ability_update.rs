use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, LogicFrames, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name};
use crate::special_ability_update::{
    ability_data_from_fields, duration, AbilityTarget, AbilityUpdate, Trigger,
};

const WEAPONSET_HERO_MODE: &str = "WEAPONSET_HERO_MODE";

const USER_3: &str = "USER_3";
const HERO: &str = "HERO";

#[derive(Debug, Clone, PartialEq)]
pub struct HeroTrigger {
    modifier: Option<Name>,
    hero_effect_duration: LogicFrames,
    condition: &'static str,
}

impl Trigger for HeroTrigger {
    fn begin(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::WeaponSetFlag(Name::from(WEAPONSET_HERO_MODE)),
            );
        }
    }

    fn effect_timer(&self) -> Option<LogicFrames> {
        Some(self.hero_effect_duration)
    }

    fn effect_expired(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ClearWeaponSetFlag(Name::from(WEAPONSET_HERO_MODE)),
            );
        }
    }

    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        if let Some(modifier) = &self.modifier {
            log.push(
                entity,
                Effect::SelfTimedModifier {
                    list: Name::clone(modifier),
                    lasts: None,
                },
            );
        }
        log.push(
            entity,
            Effect::TimedModelCondition {
                flag: Name::from(self.condition),
                lasts: self.hero_effect_duration,
                only_if_absent: true,
            },
        );
    }
}

pub type HeroModeSpecialAbilityUpdate = AbilityUpdate<HeroTrigger>;

pub fn hero_mode_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<HeroModeSpecialAbilityUpdate, FieldError> {
    let modifier = fields.field("HeroAttributeModifier").map(Name::from);
    let (_, hero_effect_duration) = duration(fields, "HeroEffectDuration");
    let condition =
        if crate::fire_weapon_update::flag(fields, "UseUSERModelcondition") == Some(true) {
            USER_3
        } else {
            HERO
        };
    let trigger = HeroTrigger {
        modifier,
        hero_effect_duration,
        condition,
    };
    let mut data = ability_data_from_fields(fields, trigger)?;
    data.stops_before_activating =
        crate::fire_weapon_update::flag(fields, "StopUnitBeforeActivating") == Some(true);
    Ok(AbilityUpdate {
        data: Arc::new(data),
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
        assert!(hero_mode_special_ability_update_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn a_permanent_duration_converts_without_overflow() {
        let module = hero_mode_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("HeroAttributeModifier", "M"),
                ("HeroEffectDuration", "999999999"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(module.data.trigger.hero_effect_duration.get() > 0);
    }

    #[test]
    fn use_user_modelcondition_picks_user_3_over_hero() {
        let module = hero_mode_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("UseUSERModelcondition", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.data.trigger.condition, "USER_3");
        let module = hero_mode_special_ability_update_from_fields(
            &fields(&[("SpecialPowerTemplate", "P")]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.data.trigger.condition, "HERO");
    }

    #[test]
    fn no_start_ability_range_means_the_machine_starts_already_in_range_and_triggers() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry
            .register::<HeroModeSpecialAbilityUpdate>(&mut world, "HeroModeSpecialAbilityUpdate");
        let holder = world.spawn().unwrap();
        let module = hero_mode_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorHeroPower"),
                ("HeroAttributeModifier", "ModAuthorHeroModifier"),
                ("HeroEffectDuration", "30000"),
                ("PreparationTime", "100"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorHeroPower",
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
        let mut triggered = false;
        let mut weapon_set_flag_at_unpack = false;
        for frame in 0..20u64 {
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
                match effect {
                    Effect::SelfTimedModifier { .. } => triggered = true,
                    Effect::WeaponSetFlag(flag) if &*flag == WEAPONSET_HERO_MODE => {
                        weapon_set_flag_at_unpack = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(
            triggered,
            "the hero modifier must be given once preparation completes"
        );
        assert!(
            weapon_set_flag_at_unpack,
            "WEAPONSET_HERO_MODE must be raised at unpack start"
        );
    }
}
