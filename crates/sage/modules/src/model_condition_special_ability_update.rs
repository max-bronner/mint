use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sage_ini::EmotionType;
use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::object_filter::ObjectFilter;
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

fn which_special_power_condition(which: u32) -> Option<Name> {
    match which {
        0 => Some(Name::from("INVULNERABLE")),
        1..=3 => Some(Name::from(format!("SPECIAL_POWER_{which}"))),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmotionPulseTrigger {
    radius: f32,
    kinds: Vec<EmotionType>,
    filter: Arc<ObjectFilter>,
    pub which_special_power: u32,
}

impl Trigger for EmotionPulseTrigger {
    fn begin(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        let Some(condition) = which_special_power_condition(self.which_special_power) else {
            return;
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ModelConditions {
                    add: vec![condition].into(),
                    remove: Vec::new().into(),
                },
            );
        }
    }

    fn finish(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        let Some(condition) = which_special_power_condition(self.which_special_power) else {
            return;
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ModelConditions {
                    add: Vec::new().into(),
                    remove: vec![condition].into(),
                },
            );
        }
    }

    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if self.radius <= 0.0 || self.kinds.is_empty() {
            return;
        }
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for kind in &self.kinds {
            log.push(
                entity,
                Effect::EmotionPulse {
                    at: PowerPlace::Source,
                    radius: self.radius,
                    kind: *kind,
                    filter: Arc::clone(&self.filter),
                    lasts: mint_sim::LogicFrames::ZERO,
                },
            );
        }
    }

    fn continue_preparation(&self, triggers: u32) -> bool {
        triggers != 2
    }
}

pub type ModelConditionSpecialAbilityUpdate = AbilityUpdate<EmotionPulseTrigger>;

pub fn model_condition_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ModelConditionSpecialAbilityUpdate, FieldError> {
    let flag = |keyword: &str| -> Result<bool, FieldError> {
        match fields.field(keyword) {
            None => Ok(false),
            Some(raw) => parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
        }
    };
    let radius = match fields.field("EmotionPulseRadius") {
        None => 50.0,
        Some(raw) => fields
            .field_numeric("EmotionPulseRadius")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| {
                FieldError::new(format!("EmotionPulseRadius '{raw}' is not a distance"))
            })?,
    };
    let kinds = [
        ("GenerateTerror", EmotionType::Terror),
        (
            "GenerateUncontrollableFear",
            EmotionType::UncontrollableFear,
        ),
    ]
    .into_iter()
    .map(|(keyword, kind)| Ok(flag(keyword)?.then_some(kind)))
    .collect::<Result<Vec<_>, FieldError>>()?
    .into_iter()
    .flatten()
    .collect();
    let tokens: Vec<String> = fields
        .field_args("ObjectFilter")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let trigger = EmotionPulseTrigger {
        radius,
        kinds,
        filter: Arc::new(ObjectFilter::parse(&tokens)),
        which_special_power: match fields.field("WhichSpecialPower") {
            None => 1,
            Some(raw) => fields
                .field_numeric("WhichSpecialPower")
                .filter(|value| (0.0..=f32::from(u16::MAX)).contains(value))
                .ok_or_else(|| {
                    FieldError::new(format!("WhichSpecialPower '{raw}' is not a number"))
                })
                .map(|value| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let index = value as u32;
                    index
                })?,
        },
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
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
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn a_declaration_with_a_pulse_names_its_type_radius_and_filter() {
        let module = model_condition_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorHornPower"),
                ("UnpackTime", "1700"),
                ("PackTime", "1666"),
                ("GenerateTerror", "Yes"),
                ("EmotionPulseRadius", "180"),
                ("ObjectFilter", "ANY +INFANTRY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let trigger = &module.data.trigger;
        assert_eq!(trigger.radius, 180.0);
        assert_eq!(trigger.kinds, vec![EmotionType::Terror]);
        assert!(trigger.filter.passes(&["INFANTRY".to_owned()], "ZFighter"));
        assert!(!trigger.filter.passes(&["CAVALRY".to_owned()], "ZRider"));
    }

    #[test]
    fn a_pulse_radius_that_is_not_a_number_is_refused_rather_than_defaulted() {
        assert!(model_condition_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorHornPower"),
                ("EmotionPulseRadius", "loud"),
            ]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn which_special_power_defaults_to_one_and_names_its_condition() {
        let module = model_condition_special_ability_update_from_fields(
            &fields(&[("SpecialPowerTemplate", "ModAuthorHornPower")]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.data.trigger.which_special_power, 1);
        assert_eq!(
            which_special_power_condition(1),
            Some(Name::from("SPECIAL_POWER_1"))
        );
        assert_eq!(
            which_special_power_condition(0),
            Some(Name::from("INVULNERABLE"))
        );
        assert_eq!(which_special_power_condition(4), None);
    }

    #[test]
    fn the_condition_rises_at_unpack_start_and_clears_on_exit() {
        use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
        use mint_sim::{Activation, Effects, Pcg32, World};

        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ModelConditionSpecialAbilityUpdate>(
            &mut world,
            "ModelConditionSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = model_condition_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorElendil"),
                ("UnpackTime", "200"),
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
            power: "ModAuthorElendil",
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
        let (mut added, mut removed) = (false, false);
        for frame in 0..40u64 {
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
                if let Effect::ModelConditions { add, remove } = effect {
                    added |= add.iter().any(|flag| &**flag == "SPECIAL_POWER_1");
                    removed |= remove.iter().any(|flag| &**flag == "SPECIAL_POWER_1");
                }
            }
        }
        assert!(added, "SPECIAL_POWER_1 must rise at unpack start");
        assert!(removed, "SPECIAL_POWER_1 must clear on exit");
    }

    #[test]
    fn a_persistent_cast_triggers_twice_then_packs_and_exits() {
        use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
        use mint_sim::{Activation, Effects, Pcg32, World};

        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ModelConditionSpecialAbilityUpdate>(
            &mut world,
            "ModelConditionSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = model_condition_special_ability_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorHornPower"),
                ("UnpackTime", "200"),
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "400"),
                ("PackTime", "200"),
                ("GenerateTerror", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorHornPower",
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
        let (mut pulses, mut recharges, mut cleared_on) = (Vec::new(), 0u32, None);
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
                    Effect::EmotionPulse { .. } => pulses.push(frame),
                    Effect::StartRecharge(_) => recharges += 1,
                    Effect::ModelConditions { remove, .. }
                        if remove.iter().any(|flag| &**flag == "SPECIAL_POWER_1") =>
                    {
                        cleared_on.get_or_insert(frame);
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            pulses.len(),
            2,
            "two triggers, not one per period: {pulses:?}"
        );
        assert_eq!(pulses[1] - pulses[0], 12, "one 400 ms period apart");
        assert_eq!(recharges, 2, "each trigger restarts the recharge");
        assert!(
            cleared_on.is_some_and(|frame| frame > pulses[1]),
            "the cast must pack and exit after its second trigger"
        );
    }
}
