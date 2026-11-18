use std::sync::Arc;

use mint_sim::module::{CreateContext, UpdateContext};
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, Names, ReplaceCarry};
use crate::special_ability_update::{
    ability_data_from_fields_with_persistence, set_ignore_ai_command_always, AbilityTarget,
    AbilityUpdate, Trigger,
};

const MOUNTED: &str = "MOUNTED";

#[derive(Debug, Clone, PartialEq)]
pub struct ToggleMountedTrigger {
    pub opacity_target: f32,
    trigger_instantly_on_create: bool,
    synchronize_timer: Names,
    pub ignore_facing_check: bool,
    pub cancel_disguise_when_dismounting: bool,
    pub mounted_template: Option<Name>,
}

impl Trigger for ToggleMountedTrigger {
    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for power in self.synchronize_timer.iter() {
            log.push(entity, Effect::StartRecharge(Name::clone(power)));
        }
        match &self.mounted_template {
            Some(template) => log.push(
                entity,
                Effect::Replace {
                    template: Name::clone(template),
                    carry: ReplaceCarry::MOUNT,
                },
            ),
            None => log.push(entity, Effect::ToggleModelCondition(Name::from(MOUNTED))),
        }
    }

    fn on_create(&self, entity: Entity, ctx: &mut CreateContext<'_>) {
        if !self.trigger_instantly_on_create {
            return;
        }
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ModelConditions {
                    add: Names::from(vec![Name::from(MOUNTED)]),
                    remove: Names::from(Vec::new()),
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

pub type ToggleMountedSpecialAbilityUpdate = AbilityUpdate<ToggleMountedTrigger>;

impl ToggleMountedSpecialAbilityUpdate {
    #[must_use]
    pub fn opacity(&self) -> f32 {
        let target = self.data.trigger.opacity_target.clamp(0.0, 1.0);
        1.0 - (1.0 - target) * self.trigger_nearness()
    }

    #[must_use]
    pub fn cancel_disguise_when_dismounting(&self) -> bool {
        self.data.trigger.cancel_disguise_when_dismounting
    }
}

pub fn toggle_mounted_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ToggleMountedSpecialAbilityUpdate, FieldError> {
    let opacity_target = fields
        .field_numeric("OpacityTarget")
        .filter(|value| value.is_finite())
        .unwrap_or(1.0);
    let trigger_instantly_on_create =
        crate::fire_weapon_update::flag(fields, "TriggerInstantlyOnCreate") == Some(true);
    let synchronize_timer: Names = fields
        .field_args("SynchronizeTimerOnSpecialPower")
        .into_iter()
        .map(Name::from)
        .collect();
    let ignore_facing_check =
        crate::fire_weapon_update::flag(fields, "IgnoreFacingCheck") == Some(true);
    let cancel_disguise_when_dismounting =
        crate::fire_weapon_update::flag(fields, "CancelDisguiseWhenDismounting") == Some(true);
    let mounted_template = fields.field("MountedTemplate").map(Name::from);
    let trigger = ToggleMountedTrigger {
        opacity_target,
        trigger_instantly_on_create,
        synchronize_timer,
        ignore_facing_check,
        cancel_disguise_when_dismounting,
        mounted_template,
    };
    let mut data = ability_data_from_fields_with_persistence(fields, trigger, true)?;
    set_ignore_ai_command_always(&mut data);
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
        assert!(
            toggle_mounted_special_ability_update_from_fields(&fields(&[]), Frame::ZERO).is_err()
        );
    }

    fn theoden_like(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
        let mut pairs = vec![
            ("SpecialPowerTemplate", "ModAuthorMountPower"),
            ("UnpackTime", "60"),
            ("PreparationTime", "1"),
            ("PersistentPrepTime", "250"),
            ("PackTime", "60"),
            ("OpacityTarget", ".3"),
        ];
        pairs.extend_from_slice(extra);
        pairs
    }

    #[test]
    fn one_press_toggles_once_and_never_re_arms() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "ModAuthorMountPower",
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
        let (mut toggles, mut recharges, mut packed) = (0u32, 0u32, false);
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
                    Effect::ToggleModelCondition(_) => toggles += 1,
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
        assert_eq!(toggles, 1, "no re-toggle without a second press");
        assert_eq!(recharges, 2, "triggers exactly twice, then ends itself");
        assert!(packed, "the cast must reach PACKING on its own");
    }

    #[test]
    fn the_fade_reaches_its_target_at_the_swap_and_returns_to_opaque() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let column =
            world.register::<mint_sim::module::ModuleColumn<ToggleMountedSpecialAbilityUpdate>>();
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[
                ("UnpackTime", "2000"),
                ("PackTime", "2000"),
            ])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "ModAuthorMountPower",
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
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut at_swap = None;
        let mut seen = Vec::new();
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
            let opacity = world.get(column, holder).unwrap().0[0].state.opacity();
            if log
                .drain()
                .iter()
                .any(|(_, effect)| matches!(effect, Effect::ToggleModelCondition(_)))
            {
                at_swap = Some(opacity);
            }
            seen.push(opacity);
        }
        assert!((at_swap.unwrap() - 0.3).abs() < 1e-6, "{at_swap:?}");
        let halfway = seen.iter().filter(|o| (0.6..0.7).contains(*o)).count();
        assert!(
            halfway >= 2,
            "passes through the middle both ways: {seen:?}"
        );
        assert_eq!(seen.last(), Some(&1.0));
    }

    #[test]
    fn an_order_during_either_fade_neither_aborts_nor_moves_the_rider() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[
                ("UnpackTime", "2000"),
                ("PackTime", "2000"),
            ])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "ModAuthorMountPower",
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
        let ordered = crate::world_reads::PlayerOrdered::from_entities([holder]);
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let (mut toggles, mut released) = (0u32, None);
        for frame in 0..200u64 {
            let reads = if frame >= 1 {
                Reads::from_snapshot(&ordered)
            } else {
                Reads::none()
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
            for (_, effect) in log.drain() {
                match effect {
                    Effect::ToggleModelCondition(_) => toggles += 1,
                    Effect::HoldOrders(false) => released = released.or(Some(frame)),
                    _ => {}
                }
            }
        }
        assert_eq!(toggles, 1, "the swap still happens");
        assert!(
            released.is_some_and(|frame| frame >= 120),
            "held through both 2 s fades: released at {released:?}"
        );
    }

    #[test]
    fn trigger_instantly_on_create_mounts_at_spawn() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[("TriggerInstantlyOnCreate", "Yes")])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        registry.broadcast_create(
            &mut world,
            holder,
            Frame::ZERO,
            Effects::into_sink(&mut log),
        );
        let mounted = log.drain().into_iter().any(
            |(_, effect)| matches!(effect, Effect::ModelConditions { add, .. } if !add.is_empty()),
        );
        assert!(
            mounted,
            "TriggerInstantlyOnCreate must set MOUNTED at spawn"
        );
    }

    #[test]
    fn synchronize_timer_starts_recharge_on_the_named_powers() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[(
                "SynchronizeTimerOnSpecialPower",
                "ModAuthorChargePower",
            )])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "ModAuthorMountPower",
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
        log.drain();

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut synced = false;
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
                if matches!(&effect, Effect::StartRecharge(power) if &**power == "ModAuthorChargePower")
                {
                    synced = true;
                }
            }
        }
        assert!(synced);
    }

    #[test]
    fn a_mounted_template_fires_replace_with_its_carry_set_instead_of_a_toggle() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ToggleMountedSpecialAbilityUpdate>(
            &mut world,
            "ToggleMountedSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let module = toggle_mounted_special_ability_update_from_fields(
            &fields(&theoden_like(&[
                ("MountedTemplate", "MordorWitchKingOnFellBeast"),
                ("SynchronizeTimerOnSpecialPower", "ModAuthorChargePower"),
            ])),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "ModAuthorMountPower",
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
        let mut entries: Vec<(Entity, Effect)> = log.drain();
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
            entries.extend(log.drain());
        }
        assert!(
            entries
                .iter()
                .all(|(_, effect)| !matches!(effect, Effect::ToggleModelCondition(_))),
            "a real object swap does not also toggle MOUNTED on the source"
        );
        let replace_at = entries
            .iter()
            .position(|(_, effect)| matches!(effect, Effect::Replace { .. }));
        let recharge_at = entries
            .iter()
            .position(|(_, effect)| matches!(effect, Effect::StartRecharge(_)));
        match (recharge_at, replace_at) {
            (Some(recharge), Some(replace)) => assert!(
                recharge < replace,
                "the recharge must still find the source alive when it fires"
            ),
            _ => panic!("expected both a recharge and a replace effect, got {entries:?}"),
        }
        match &entries[replace_at.unwrap()].1 {
            Effect::Replace { template, carry } => {
                assert_eq!(&**template, "MordorWitchKingOnFellBeast");
                assert_eq!(*carry, ReplaceCarry::MOUNT);
            }
            other => panic!("expected Effect::Replace, got {other:?}"),
        }
    }
}
