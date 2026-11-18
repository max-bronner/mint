use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

pub const DISGUISED: &str = "DISGUISED";

#[derive(Debug, Clone, PartialEq)]
pub struct DisguiseTrigger {
    force_mounted_when_disguising: bool,
    pub trigger_instantly_on_create: bool,
    pub disguise_as_template: Option<Name>,
    pub enemy_perspective_template: Option<Name>,
    disguise_fx: Option<Name>,
    pub opacity_target: f32,
}

impl Trigger for DisguiseTrigger {
    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ToggleDisguise {
                    force_mounted: self.force_mounted_when_disguising,
                },
            );
        }
    }

    fn fires_after_first_trigger(&self) -> bool {
        false
    }

    fn continue_preparation(&self, triggers: u32) -> bool {
        triggers != 2
    }
}

pub type SpecialDisguiseUpdate = AbilityUpdate<DisguiseTrigger>;

impl SpecialDisguiseUpdate {
    #[must_use]
    pub fn disguise_fx(&self) -> Option<&str> {
        self.data.trigger.disguise_fx.as_deref()
    }

    #[must_use]
    pub fn disguise_as_template(&self) -> Option<&str> {
        self.data.trigger.disguise_as_template.as_deref()
    }

    #[must_use]
    pub fn enemy_perspective_template(&self) -> Option<&str> {
        self.data.trigger.enemy_perspective_template.as_deref()
    }

    #[must_use]
    pub fn opacity(&self) -> f32 {
        let target = self.data.trigger.opacity_target.clamp(0.0, 1.0);
        1.0 - (1.0 - target) * self.trigger_nearness()
    }
}

pub fn special_disguise_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpecialDisguiseUpdate, FieldError> {
    let opacity_target = fields
        .field_numeric("OpacityTarget")
        .filter(|value| value.is_finite())
        .unwrap_or(1.0);
    let trigger_instantly_on_create =
        crate::fire_weapon_update::flag(fields, "TriggerInstantlyOnCreate") == Some(true);
    let force_mounted_when_disguising =
        crate::fire_weapon_update::flag(fields, "ForceMountedWhenDisguising") == Some(true);
    let trigger = DisguiseTrigger {
        force_mounted_when_disguising,
        trigger_instantly_on_create,
        disguise_as_template: fields.field("DisguiseAsTemplate").map(Name::from),
        enemy_perspective_template: fields
            .field("DisguisedAsTemplate_EnemyPerspective")
            .map(Name::from),
        disguise_fx: fields
            .field("DisguiseFX")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
        opacity_target,
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
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
            self.0.get(keyword).copied()
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
    fn a_declaration_s_six_fields_all_round_trip() {
        let module = special_disguise_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorDisguisePower"),
                ("OpacityTarget", "0.5"),
                ("TriggerInstantlyOnCreate", "Yes"),
                ("DisguiseAsTemplate", "ModAuthorCavalryChild"),
                (
                    "DisguisedAsTemplate_EnemyPerspective",
                    "ModAuthorCavalryHorde",
                ),
                ("DisguiseFX", "FX_ModAuthorDisguise"),
                ("ForceMountedWhenDisguising", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let trigger = &module.data.trigger;
        assert!((trigger.opacity_target - 0.5).abs() < 1e-6);
        assert!(trigger.trigger_instantly_on_create);
        assert_eq!(
            trigger.disguise_as_template.as_deref(),
            Some("ModAuthorCavalryChild")
        );
        assert_eq!(
            trigger.enemy_perspective_template.as_deref(),
            Some("ModAuthorCavalryHorde")
        );
        assert_eq!(module.disguise_fx(), Some("FX_ModAuthorDisguise"));
        assert!(trigger.force_mounted_when_disguising);
    }

    #[test]
    fn a_persistent_cast_toggles_only_on_its_first_trigger_then_packs_itself() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SpecialDisguiseUpdate>(&mut world, "SpecialDisguiseUpdate");
        let holder = world.spawn().unwrap();
        let module = special_disguise_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorDisguisePower"),
                ("UnpackTime", "1000"),
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "250"),
                ("PackTime", "1000"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "ModAuthorDisguisePower",
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
        let (mut toggles, mut recharges, mut packed) = (0, 0, false);
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
                    Effect::ToggleDisguise { .. } => toggles += 1,
                    Effect::StartRecharge(_) => recharges += 1,
                    Effect::ModelConditions { add, .. }
                        if add.iter().any(|name| &**name == "PACKING") =>
                    {
                        packed = true;
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(
            recharges, 2,
            "the cast must trigger exactly twice, then end itself: {recharges} triggers"
        );
        assert_eq!(toggles, 1);
        assert!(
            packed,
            "the cast must reach PACKING on its own, with no order"
        );
    }

    #[test]
    fn an_order_mid_cast_restores_opacity_at_once() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SpecialDisguiseUpdate>(&mut world, "SpecialDisguiseUpdate");
        let column = world.register::<mint_sim::module::ModuleColumn<SpecialDisguiseUpdate>>();
        let holder = world.spawn().unwrap();
        let module = special_disguise_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorDisguisePower"),
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "2000"),
                ("OpacityTarget", "0.5"),
                ("ParalyzeDurationWhenAborted", "1000"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "ModAuthorDisguisePower",
            target: None,
            at: None,
        };
        let mut log = EffectLog::new();
        registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log),
        );
        let ordered = mint_sim::module::Reads::none();
        let player_ordered = crate::world_reads::PlayerOrdered::from_entities([holder]);
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut opacity_at_order = None;
        for frame in 0..20u64 {
            let reads = if frame == 5 {
                mint_sim::module::Reads::from_snapshot(&player_ordered)
            } else {
                ordered
            };
            let mut log = EffectLog::new();
            registry.update_with_reads(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                reads,
            );
            log.drain();
            if frame == 6 {
                opacity_at_order = Some(world.get(column, holder).unwrap().0[0].state.opacity());
            }
        }
        assert_eq!(
            opacity_at_order,
            Some(1.0),
            "opacity must snap back the frame the order lands, not fade"
        );
    }

    #[test]
    fn missing_power_template_is_refused() {
        assert!(special_disguise_update_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }
}
