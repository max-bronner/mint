use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{
    ActivateContext, Activation, Component, FieldError, Frame, LogicFrames, ModuleFieldSource,
    StateHasher,
};

use crate::effects::{Effect, EffectLog, Name, Names};

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[derive(Debug, PartialEq)]
struct WeaponModeData {
    power: Name,
    duration: LogicFrames,
    modifier: Option<Name>,
    weapon_set_flags: Names,
    lock: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponModeSpecialPowerUpdate {
    data: Arc<WeaponModeData>,
    remaining: Option<LogicFrames>,
}

impl Component for WeaponModeSpecialPowerUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.remaining {
            None => hasher.write_u8(0),
            Some(remaining) => {
                hasher.write_u8(1);
                hasher.write_u32(remaining.get());
            }
        }
    }
}

impl ModuleKind for WeaponModeSpecialPowerUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower)
        .union(CapabilityMask::single(Capability::Update));

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.data.power, power)
    }

    fn activate(&mut self, _activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        if self.remaining.is_some() {
            return;
        }
        self.remaining = Some(self.data.duration);
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        if let Some(slot) = self.data.lock {
            log.push(ctx.entity, Effect::LockWeaponSlot(slot));
        } else {
            for flag in self.data.weapon_set_flags.iter() {
                log.push(ctx.entity, Effect::WeaponSetFlag(Name::clone(flag)));
            }
        }
        if let Some(modifier) = &self.data.modifier {
            log.push(
                ctx.entity,
                Effect::SelfTimedModifier {
                    list: Name::clone(modifier),
                    lasts: Some(self.data.duration),
                },
            );
        }
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let Some(remaining) = self.remaining else {
            return Sleep::Forever;
        };
        let next = remaining.checked_sub(LogicFrames::ONE);
        if let Some(next) = next {
            if !next.is_zero() {
                self.remaining = Some(next);
                return Sleep::NextFrame;
            }
        }
        self.remaining = None;
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return Sleep::Forever;
        };
        log.push(ctx.entity, Effect::ReleaseWeaponLock);
        for flag in self.data.weapon_set_flags.iter() {
            log.push(ctx.entity, Effect::ClearWeaponSetFlag(Name::clone(flag)));
        }
        Sleep::Forever
    }
}

pub fn weapon_mode_special_power_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<WeaponModeSpecialPowerUpdate, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let duration = fields
        .field_numeric("Duration")
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .map_or(LogicFrames::ZERO, |ms| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            LogicFrames::from_milliseconds(whole)
        });
    let modifier = fields.field("AttributeModifier").map(Name::from);
    let lock = match fields.field("LockWeaponSlot") {
        None => None,
        Some(token) => {
            let slot = mint_sage_combat::WeaponSlot::parse(token).ok_or_else(|| {
                FieldError::new(format!("LockWeaponSlot {token} is no weapon slot"))
            })?;
            u8::try_from(slot.index()).ok()
        }
    };
    let weapon_set_flags: Names = fields
        .field_args("WeaponSetFlags")
        .into_iter()
        .map(Name::from)
        .collect();
    Ok(WeaponModeSpecialPowerUpdate {
        data: Arc::new(WeaponModeData {
            power,
            duration,
            modifier,
            weapon_set_flags,
            lock,
        }),
        remaining: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::effects::UpgradeState;
    use mint_sim::module::{DestroyRequests, ModuleRegistry};
    use mint_sim::{Effects, Pcg32, World};
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
        assert!(weapon_mode_special_power_update_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn activation_raises_the_flag_and_a_second_press_while_active_is_ignored() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry
            .register::<WeaponModeSpecialPowerUpdate>(&mut world, "WeaponModeSpecialPowerUpdate");
        let holder = world.spawn().unwrap();
        let module = weapon_mode_special_power_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorPower"),
                ("Duration", "30000"),
                ("AttributeModifier", "ModAuthorModifier"),
                ("WeaponSetFlags", "WEAPONSET_MOD_TEST"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorPower",
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
        let mut state = UpgradeState::default();
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
        assert!(state.has_weapon_set_flag("WEAPONSET_MOD_TEST"));

        let mut log = EffectLog::new();
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::new(1),
            Effects::into_sink(&mut log)
        ));
        assert!(
            log.drain().is_empty(),
            "re-activation while active is a no-op"
        );
    }

    #[test]
    fn lock_weapon_slot_replaces_the_flags_and_is_released_when_duration_elapses() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry
            .register::<WeaponModeSpecialPowerUpdate>(&mut world, "WeaponModeSpecialPowerUpdate");
        let holder = world.spawn().unwrap();
        let module = weapon_mode_special_power_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorPower"),
                ("Duration", "100"),
                ("LockWeaponSlot", "SECONDARY"),
                ("WeaponSetFlags", "WEAPONSET_MOD_TEST"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorPower",
            target: None,
            at: None,
        };
        registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log),
        );
        let mut state = UpgradeState::default();
        for (_, effect) in log.drain() {
            state.apply(&effect);
        }
        assert_eq!(state.weapon_lock, Some(1));
        assert!(!state.has_weapon_set_flag("WEAPONSET_MOD_TEST"));

        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        for frame in 0..10u64 {
            let mut log = EffectLog::new();
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
            );
            for (_, effect) in log.drain() {
                state.apply(&effect);
            }
        }
        assert_eq!(state.weapon_lock, None);
    }

    #[test]
    fn duration_elapsing_clears_the_flag() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry
            .register::<WeaponModeSpecialPowerUpdate>(&mut world, "WeaponModeSpecialPowerUpdate");
        let holder = world.spawn().unwrap();
        let module = weapon_mode_special_power_update_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorPower"),
                ("Duration", "100"),
                ("WeaponSetFlags", "WEAPONSET_MOD_TEST"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorPower",
            target: None,
            at: None,
        };
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
        let mut cleared = false;
        for frame in 0..10u64 {
            let mut log = EffectLog::new();
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
            );
            for (_, effect) in log.drain() {
                if matches!(effect, Effect::ClearWeaponSetFlag(_)) {
                    cleared = true;
                }
            }
        }
        assert!(cleared, "the flag must clear once Duration elapses");
    }
}
