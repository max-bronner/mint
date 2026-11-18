use mint_sage_ini::value::parse_bool;
use mint_sim::module::{
    Capability, CapabilityMask, DieContext, ModuleKind, ModuleRegistry, ModuleSlot, UpgradeMask,
};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher, World};

use crate::die_mux::DieMuxData;

const UNDER_CONSTRUCTION: &str = "UNDER_CONSTRUCTION";
const BUILD_BEING_CANCELED: &str = "BUILD_BEING_CANCELED";
use crate::effects::{CreateLocation, Effect, EffectLog, Name, OclFrom, PowerPlace};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreateObjectDie {
    creation_list: Option<String>,
    debris_portion_of_self: Option<String>,
    upgrade_required: UpgradeMask,
    mux: DieMuxData,
}

impl Component for CreateObjectDie {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for field in [&self.creation_list, &self.debris_portion_of_self] {
            match field {
                Some(name) => {
                    hasher.write_bool(true);
                    hasher.write_usize(name.len());
                    hasher.write_bytes(name.as_bytes());
                }
                None => hasher.write_bool(false),
            }
        }
        self.upgrade_required.hash_into(hasher);
    }
}

impl ModuleKind for CreateObjectDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for slot in instances.iter() {
            if !slot
                .state
                .mux
                .gate(ctx.entity, "CreateObjectDie", &ctx.death)
            {
                continue;
            }
            if !slot.state.upgrade_required.is_empty()
                && !ctx
                    .death
                    .upgrades
                    .is_some_and(|held| held.contains_all(&slot.state.upgrade_required))
            {
                continue;
            }
            if let Some(list) = &slot.state.creation_list {
                log.push(
                    ctx.entity,
                    Effect::CreateObjects {
                        list: list.as_str().into(),
                        location: CreateLocation::AtLocation,
                        at: PowerPlace::Source,
                        from: OclFrom::Object {
                            secondary: ctx.death.killer,
                        },
                    },
                );
            }
            if let Some(template) = &slot.state.debris_portion_of_self {
                log.push(
                    ctx.entity,
                    Effect::CreateObject {
                        name: Name::from(template.as_str()),
                        at: PowerPlace::Source,
                    },
                );
            }
        }
    }
}

pub fn create_object_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CreateObjectDie, FieldError> {
    Ok(CreateObjectDie {
        creation_list: fields.field("CreationList").map(str::to_owned),
        debris_portion_of_self: fields.field("DebrisPortionOfSelf").map(str::to_owned),
        upgrade_required: fields.field_upgrade_mask("UpgradeRequired"),
        mux: DieMuxData::from_fields(fields)?,
    })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FireWeaponWhenDeadBehavior {
    starts_active: bool,
    death_weapon: Option<String>,
    delay: LogicFrames,
    offset: (f32, f32, f32),
    active_during_construction: bool,
    mux: DieMuxData,
}

impl Component for FireWeaponWhenDeadBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.starts_active);
        self.delay.hash_into(hasher);
        hasher.write_f32(self.offset.0);
        hasher.write_f32(self.offset.1);
        hasher.write_f32(self.offset.2);
        hasher.write_bool(self.active_during_construction);
        match &self.death_weapon {
            Some(name) => {
                hasher.write_bool(true);
                hasher.write_usize(name.len());
                hasher.write_bytes(name.as_bytes());
            }
            None => hasher.write_bool(false),
        }
    }
}

impl ModuleKind for FireWeaponWhenDeadBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for slot in instances.iter() {
            if !slot.state.starts_active {
                continue;
            }
            if !slot
                .state
                .mux
                .gate(ctx.entity, "FireWeaponWhenDeadBehavior", &ctx.death)
            {
                continue;
            }
            let building = ctx.death.status.contains(&UNDER_CONSTRUCTION);
            if ctx.death.status.contains(&BUILD_BEING_CANCELED)
                || (building && !slot.state.active_during_construction)
            {
                continue;
            }
            if let Some(weapon) = &slot.state.death_weapon {
                log.push(
                    ctx.entity,
                    Effect::FireWeaponAtDeath {
                        weapon: weapon.as_str().into(),
                        offset: slot.state.offset,
                        delay: slot.state.delay,
                    },
                );
            }
        }
    }
}

pub fn fire_weapon_when_dead_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FireWeaponWhenDeadBehavior, FieldError> {
    let starts_active = match fields.field("StartsActive") {
        None => false,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("StartsActive '{raw}' is not Yes/No")))?,
    };
    let offset = fields.field_args("WeaponOffset");
    Ok(FireWeaponWhenDeadBehavior {
        starts_active,
        death_weapon: fields.field("DeathWeapon").map(str::to_owned),
        delay: crate::fire_weapon_update::milliseconds(fields, "DelayTime"),
        offset: (
            crate::fire_weapon_update::tagged(&offset, "X"),
            crate::fire_weapon_update::tagged(&offset, "Y"),
            crate::fire_weapon_update::tagged(&offset, "Z"),
        ),
        active_during_construction: crate::fire_weapon_update::flag(
            fields,
            "ActiveDuringConstruction",
        )
        .unwrap_or(false),
        mux: DieMuxData::from_fields(fields)?,
    })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DamageFilteredCreateObjectDie {
    trigger_instantly: Option<String>,
    trigger_for_duration: Option<String>,
    post_filter_triggered_duration_ms: Option<u32>,
    creation_list: Option<String>,
    mux: DieMuxData,
}

impl Component for DamageFilteredCreateObjectDie {
    fn hash_into(&self, hasher: &mut StateHasher) {
        for field in [
            &self.trigger_instantly,
            &self.trigger_for_duration,
            &self.creation_list,
        ] {
            match field {
                Some(name) => {
                    hasher.write_bool(true);
                    hasher.write_usize(name.len());
                    hasher.write_bytes(name.as_bytes());
                }
                None => hasher.write_bool(false),
            }
        }
        match self.post_filter_triggered_duration_ms {
            Some(ms) => {
                hasher.write_bool(true);
                hasher.write_u32(ms);
            }
            None => hasher.write_bool(false),
        }
    }
}

const ONCE_PER_KILLER: &str = "BECOME_UNDEAD_ONCE";
const ONCE_PER_KILLER_FRAMES: u32 = 5;

impl ModuleKind for DamageFilteredCreateObjectDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for slot in instances.iter() {
            let triggered = slot
                .state
                .trigger_instantly
                .as_deref()
                .is_some_and(|sub| sub.eq_ignore_ascii_case(ctx.death.sub_cause));
            if !triggered {
                continue;
            }
            if !slot
                .state
                .mux
                .gate(ctx.entity, "DamageFilteredCreateObjectDie", &ctx.death)
            {
                continue;
            }
            let Some(primary) = ctx.death.killer else {
                continue;
            };
            let throttled = slot
                .state
                .trigger_instantly
                .as_deref()
                .is_some_and(|sub| sub.eq_ignore_ascii_case(ONCE_PER_KILLER));
            if let Some(list) = &slot.state.creation_list {
                log.push(
                    ctx.entity,
                    Effect::CreateObjects {
                        list: list.as_str().into(),
                        location: CreateLocation::AtLocation,
                        at: PowerPlace::Source,
                        from: OclFrom::For {
                            primary,
                            min_gap: throttled.then_some(ONCE_PER_KILLER_FRAMES),
                        },
                    },
                );
            }
        }
    }
}

pub fn damage_filtered_create_object_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DamageFilteredCreateObjectDie, FieldError> {
    let post_filter_triggered_duration_ms =
        match fields.field_numeric("PostFilterTriggeredDuration") {
            None => None,
            Some(ms) if ms.is_finite() && ms >= 0.0 => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let whole = ms as u32;
                Some(whole)
            }
            Some(ms) => {
                return Err(FieldError::new(format!(
                    "PostFilterTriggeredDuration '{ms}' is not a non-negative number"
                )))
            }
        };
    Ok(DamageFilteredCreateObjectDie {
        trigger_instantly: fields
            .field("DamageTypeTriggersInstantly")
            .map(str::to_owned),
        trigger_for_duration: fields
            .field("DamageTypeTriggersForDuration")
            .map(str::to_owned),
        post_filter_triggered_duration_ms,
        creation_list: fields.field("CreationList").map(str::to_owned),
        mux: DieMuxData::from_fields(fields)?,
    })
}

pub fn register_effects(registry: &mut ModuleRegistry, world: &mut World) {
    registry.register_loadable(world, "CreateObjectDie", create_object_die_from_fields);
    registry.register_loadable(
        world,
        "FireWeaponWhenDeadBehavior",
        fire_weapon_when_dead_behavior_from_fields,
    );
    registry.register_loadable(
        world,
        "DamageFilteredCreateObjectDie",
        damage_filtered_create_object_die_from_fields,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    #[test]
    fn register_effects_registers_all_three_names() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        register_effects(&mut registry, &mut world);
        for name in [
            "CreateObjectDie",
            "FireWeaponWhenDeadBehavior",
            "DamageFilteredCreateObjectDie",
        ] {
            assert!(registry.is_registered(name), "{name} was not registered");
        }
    }

    #[test]
    fn a_declared_creation_list_is_reported_on_death() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<CreateObjectDie>(&mut world, "CreateObjectDie");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                CreateObjectDie {
                    creation_list: Some(String::from("OCL_MordorCatapultRockProjectileBits")),
                    ..CreateObjectDie::default()
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.broadcast_die_into(
            &mut world,
            entity,
            Frame::ZERO,
            &mut rng,
            &mut destroy,
            mint_sim::Effects::into_sink(&mut log),
            mint_sim::module::Death::NONE,
        );
        let entries = log.drain();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            (
                entity,
                Effect::CreateObjects {
                    list: "OCL_MordorCatapultRockProjectileBits".into(),
                    location: CreateLocation::AtLocation,
                    at: PowerPlace::Source,
                    from: OclFrom::Object { secondary: None },
                }
            )
        );
    }

    #[test]
    fn the_undead_filter_reads_the_blows_sub_type_and_not_its_damage_type() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry
            .register::<DamageFilteredCreateObjectDie>(&mut world, "DamageFilteredCreateObjectDie");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                DamageFilteredCreateObjectDie {
                    trigger_instantly: Some(String::from("BECOME_UNDEAD")),
                    creation_list: Some(String::from("OCL_BecomeUndead")),
                    ..DamageFilteredCreateObjectDie::default()
                },
            )
            .unwrap();
        let ran = |registry: &mut ModuleRegistry, world: &mut World, death| {
            let mut rng = mint_sim::Pcg32::new(1);
            let mut destroy = mint_sim::module::DestroyRequests::new();
            let mut log = EffectLog::new();
            registry.broadcast_die_into(
                world,
                entity,
                Frame::ZERO,
                &mut rng,
                &mut destroy,
                mint_sim::Effects::into_sink(&mut log),
                death,
            );
            !log.drain().is_empty()
        };
        let as_damage_type = mint_sim::module::Death {
            cause: "BECOME_UNDEAD",
            ..mint_sim::module::Death::NONE
        };
        let killer = world.spawn().unwrap();
        let as_sub_type = mint_sim::module::Death {
            cause: "POISON",
            sub_cause: "BECOME_UNDEAD",
            killer: Some(killer),
            ..mint_sim::module::Death::NONE
        };
        let no_killer = mint_sim::module::Death {
            killer: None,
            ..as_sub_type
        };
        assert!(!ran(&mut registry, &mut world, as_damage_type));
        assert!(!ran(&mut registry, &mut world, no_killer));
        assert!(ran(&mut registry, &mut world, as_sub_type));
    }

    #[test]
    fn an_inactive_fire_weapon_when_dead_reports_nothing() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<FireWeaponWhenDeadBehavior>(&mut world, "FireWeaponWhenDeadBehavior");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                FireWeaponWhenDeadBehavior {
                    starts_active: false,
                    death_weapon: Some(String::from("StandardWallDeath")),
                    ..FireWeaponWhenDeadBehavior::default()
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.broadcast_die_into(
            &mut world,
            entity,
            Frame::ZERO,
            &mut rng,
            &mut destroy,
            mint_sim::Effects::into_sink(&mut log),
            mint_sim::module::Death::NONE,
        );
        assert!(log.drain().is_empty());
    }

    #[test]
    fn an_active_fire_weapon_when_dead_fires_at_its_own_position() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<FireWeaponWhenDeadBehavior>(&mut world, "FireWeaponWhenDeadBehavior");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                FireWeaponWhenDeadBehavior {
                    starts_active: true,
                    death_weapon: Some(String::from("StandardWallDeath")),
                    ..FireWeaponWhenDeadBehavior::default()
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.broadcast_die_into(
            &mut world,
            entity,
            Frame::ZERO,
            &mut rng,
            &mut destroy,
            mint_sim::Effects::into_sink(&mut log),
            mint_sim::module::Death::NONE,
        );
        let entries = log.drain();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            (
                entity,
                Effect::FireWeaponAtDeath {
                    weapon: "StandardWallDeath".into(),
                    offset: (0.0, 0.0, 0.0),
                    delay: LogicFrames::ZERO,
                }
            )
        );
    }

    #[test]
    fn a_death_weapon_waits_for_the_building_unless_active_during_construction() {
        let fired = |active_during_construction: bool| {
            let mut world = World::new();
            let mut registry = ModuleRegistry::new(&mut world);
            registry
                .register::<FireWeaponWhenDeadBehavior>(&mut world, "FireWeaponWhenDeadBehavior");
            let entity = world.spawn().unwrap();
            registry
                .attach(
                    &mut world,
                    entity,
                    Frame::ZERO,
                    FireWeaponWhenDeadBehavior {
                        starts_active: true,
                        death_weapon: Some(String::from("StandardWallDeath")),
                        active_during_construction,
                        ..FireWeaponWhenDeadBehavior::default()
                    },
                )
                .unwrap();
            let mut rng = mint_sim::Pcg32::new(1);
            let mut destroy = mint_sim::module::DestroyRequests::new();
            let mut log = EffectLog::new();
            registry.broadcast_die_into(
                &mut world,
                entity,
                Frame::ZERO,
                &mut rng,
                &mut destroy,
                mint_sim::Effects::into_sink(&mut log),
                mint_sim::module::Death {
                    status: &["UNDER_CONSTRUCTION"],
                    ..mint_sim::module::Death::NONE
                },
            );
            log.drain().len()
        };
        assert_eq!(fired(false), 0);
        assert_eq!(fired(true), 1);
    }

    #[test]
    fn damage_filtered_create_object_die_reads_its_four_fields() {
        let fields = TestFields(BTreeMap::from([
            ("DamageTypeTriggersInstantly", "BECOME_UNDEAD"),
            ("DamageTypeTriggersForDuration", "BECOME_UNDEAD"),
            ("PostFilterTriggeredDuration", "10000"),
            ("CreationList", "OCL_BecomeUndead"),
        ]));
        let module = damage_filtered_create_object_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.trigger_instantly.as_deref(), Some("BECOME_UNDEAD"));
        assert_eq!(module.post_filter_triggered_duration_ms, Some(10_000));
        assert_eq!(module.creation_list.as_deref(), Some("OCL_BecomeUndead"));
    }

    #[test]
    fn a_negative_post_filter_duration_is_refused() {
        let fields = TestFields(BTreeMap::from([("PostFilterTriggeredDuration", "-1")]));
        assert!(damage_filtered_create_object_die_from_fields(&fields, Frame::ZERO).is_err());
    }
}
