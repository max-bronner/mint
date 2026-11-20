use std::sync::Arc;

use mint_sim::module::{CreateContext, UpdateContext};
use mint_sim::{Entity, FieldError, Frame, LogicFrames, ModuleFieldSource};

use crate::effects::{Effect, EffectLog};
use crate::invisibility_update::{flag, number};
use crate::special_ability_update::{
    ability_data_from_fields_with_persistence, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ToggleHiddenTrigger {
    pub power: crate::effects::Name,
    pub effect_duration: Option<LogicFrames>,
    pub show_palantir_timer: bool,
}

impl Trigger for ToggleHiddenTrigger {
    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ToggleStealth {
                    power: self.power.clone(),
                    lasts: self.effect_duration,
                    palantir_timer: self.show_palantir_timer,
                },
            );
        }
    }

    fn on_create(&self, _entity: Entity, _ctx: &mut CreateContext<'_>) {}

    fn fires_after_first_trigger(&self) -> bool {
        false
    }

    fn continue_preparation(&self, triggers: u32) -> bool {
        triggers != 2
    }
}

pub type ToggleHiddenSpecialAbilityUpdate = AbilityUpdate<ToggleHiddenTrigger>;

pub fn toggle_hidden_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ToggleHiddenSpecialAbilityUpdate, FieldError> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let effect_duration = number(fields, "EffectDuration")?
        .filter(|ms| *ms > 0.0)
        .map(
            |ms| crate::special_ability_update::retail_duration_ticks(ms.min(86_400_000.0) as u32),
        );
    let trigger = ToggleHiddenTrigger {
        power: crate::effects::Name::from(
            fields
                .field("SpecialPowerTemplate")
                .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?,
        ),
        effect_duration,
        show_palantir_timer: flag(fields, "ShowPalantirTimer")?.unwrap_or(false),
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields_with_persistence(
            fields, trigger, true,
        )?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
    use mint_sim::{Activation, Effects, Pcg32, World};
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn field_numeric(&self, keyword: &str) -> Option<f32> {
            self.field(keyword).and_then(|value| value.parse().ok())
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn one_press_reports_one_stealth_toggle_with_its_duration() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleHiddenSpecialAbilityUpdate>(
            &mut world,
            "ToggleHiddenSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_hidden_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorCloakPower"),
                ("EffectDuration", "15000"),
                ("ShowPalantirTimer", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "ModAuthorCloakPower",
            target: None,
            at: None,
        };
        let mut log = EffectLog::new();
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut toggles: Vec<(Option<LogicFrames>, bool)> = Vec::new();
        for frame in 0..200u64 {
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
                if let Effect::ToggleStealth {
                    lasts,
                    palantir_timer,
                    ..
                } = effect
                {
                    toggles.push((lasts, palantir_timer));
                }
            }
        }
        assert_eq!(
            toggles,
            [(Some(LogicFrames::from_engine_milliseconds(15000)), true)]
        );
    }

    #[test]
    fn a_persistent_hide_toggles_once_and_packs_after_its_second_trigger() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleHiddenSpecialAbilityUpdate>(
            &mut world,
            "ToggleHiddenSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_hidden_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorCloakPower"),
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "1600"),
                ("PackTime", "1666"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "ModAuthorCloakPower",
            target: None,
            at: None,
        };
        let mut log = EffectLog::new();
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let (mut toggles, mut recharges, mut packed_at) = (0u32, 0u32, None);
        for frame in 0..200u64 {
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
                    Effect::ToggleStealth { .. } => toggles += 1,
                    Effect::StartRecharge(_) => recharges += 1,
                    Effect::ModelConditions { add, .. }
                        if add.iter().any(|name| &**name == "PACKING") =>
                    {
                        packed_at = packed_at.or(Some(frame));
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(toggles, 1, "no re-toggle on the second trigger");
        assert_eq!(recharges, 2, "the base machine recharges on both triggers");
        let one_period = LogicFrames::from_engine_milliseconds(1).get() + 1;
        let two_periods = one_period + LogicFrames::from_engine_milliseconds(1600).get();
        assert!(
            packed_at.is_some_and(|frame| frame >= u64::from(two_periods)),
            "must not pack after only the first period ({one_period}): {packed_at:?}"
        );
    }
}
