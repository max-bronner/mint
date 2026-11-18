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

fn frames(fields: &dyn ModuleFieldSource, keyword: &str) -> LogicFrames {
    fields
        .field_numeric(keyword)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map_or(LogicFrames::ZERO, |value| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = value.min(f32::from(u16::MAX)) as u32;
            LogicFrames::from_frames(whole)
        })
}

#[derive(Debug, PartialEq)]
struct WeaponChangeData {
    power: Name,
    toggle_flags: Names,
    toggle_on_sleep: LogicFrames,
    toggle_off_sleep: LogicFrames,
    on_modifier: Option<Name>,
    off_modifier: Option<Name>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleState {
    Off,
    TurningOn(LogicFrames),
    On,
    TurningOff(LogicFrames),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponChangeSpecialPowerModule {
    data: Arc<WeaponChangeData>,
    state: ToggleState,
}

impl Component for WeaponChangeSpecialPowerModule {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.state {
            ToggleState::Off => hasher.write_u8(0),
            ToggleState::TurningOn(remaining) => {
                hasher.write_u8(1);
                hasher.write_u32(remaining.get());
            }
            ToggleState::On => hasher.write_u8(2),
            ToggleState::TurningOff(remaining) => {
                hasher.write_u8(3);
                hasher.write_u32(remaining.get());
            }
        }
    }
}

impl ModuleKind for WeaponChangeSpecialPowerModule {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower)
        .union(CapabilityMask::single(Capability::Update));

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.data.power, power)
    }

    fn activate(&mut self, _activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        self.state = match self.state {
            ToggleState::Off => {
                if self.data.toggle_on_sleep.is_zero() {
                    self.finish_toggle(ctx, true);
                    ToggleState::On
                } else {
                    ToggleState::TurningOn(self.data.toggle_on_sleep)
                }
            }
            ToggleState::On => {
                if self.data.toggle_off_sleep.is_zero() {
                    self.finish_toggle(ctx, false);
                    ToggleState::Off
                } else {
                    ToggleState::TurningOff(self.data.toggle_off_sleep)
                }
            }
            mid @ (ToggleState::TurningOn(_) | ToggleState::TurningOff(_)) => mid,
        };
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let (remaining, turning_on) = match self.state {
            ToggleState::TurningOn(remaining) => (remaining, true),
            ToggleState::TurningOff(remaining) => (remaining, false),
            ToggleState::Off | ToggleState::On => return Sleep::Forever,
        };
        let Some(next) = remaining.checked_sub(LogicFrames::ONE) else {
            self.finish_toggle_update(ctx, turning_on);
            self.state = if turning_on {
                ToggleState::On
            } else {
                ToggleState::Off
            };
            return Sleep::Forever;
        };
        if next.is_zero() {
            self.finish_toggle_update(ctx, turning_on);
            self.state = if turning_on {
                ToggleState::On
            } else {
                ToggleState::Off
            };
            return Sleep::Forever;
        }
        self.state = if turning_on {
            ToggleState::TurningOn(next)
        } else {
            ToggleState::TurningOff(next)
        };
        Sleep::NextFrame
    }
}

impl WeaponChangeSpecialPowerModule {
    fn finish_toggle(&self, ctx: &mut ActivateContext<'_>, turning_on: bool) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        Self::push_toggle_effects(log, ctx.entity, &self.data, turning_on);
    }

    fn finish_toggle_update(&self, ctx: &mut UpdateContext<'_>, turning_on: bool) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        Self::push_toggle_effects(log, ctx.entity, &self.data, turning_on);
    }

    fn push_toggle_effects(
        log: &mut EffectLog,
        entity: mint_sim::Entity,
        data: &WeaponChangeData,
        turning_on: bool,
    ) {
        for flag in data.toggle_flags.iter() {
            log.push(
                entity,
                if turning_on {
                    Effect::WeaponSetFlag(Name::clone(flag))
                } else {
                    Effect::ClearWeaponSetFlag(Name::clone(flag))
                },
            );
        }
        let (gains, drops) = if turning_on {
            (&data.on_modifier, &data.off_modifier)
        } else {
            (&data.off_modifier, &data.on_modifier)
        };
        if let Some(modifier) = drops {
            log.push(
                entity,
                Effect::ClearSelfTimedModifier(Name::clone(modifier)),
            );
        }
        if let Some(modifier) = gains {
            log.push(
                entity,
                Effect::SelfTimedModifier {
                    list: Name::clone(modifier),
                    lasts: None,
                },
            );
        }
    }
}

pub fn weapon_change_special_power_module_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<WeaponChangeSpecialPowerModule, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let toggle_flags: Names = fields
        .field_args("FlagsUsedForToggle")
        .into_iter()
        .map(Name::from)
        .collect();
    Ok(WeaponChangeSpecialPowerModule {
        data: Arc::new(WeaponChangeData {
            power,
            toggle_flags,
            toggle_on_sleep: frames(fields, "ToggleOnSleepFrames"),
            toggle_off_sleep: frames(fields, "ToggleOffSleepFrames"),
            on_modifier: fields.field("ToggleOnAttributeModifier").map(Name::from),
            off_modifier: fields.field("ToggleOffAttributeModifier").map(Name::from),
        }),
        state: ToggleState::Off,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::effects::UpgradeState;
    use mint_sim::module::ModuleRegistry;
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
        assert!(weapon_change_special_power_module_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn a_press_raises_the_flag_and_modifier_only_after_its_own_sleep() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<WeaponChangeSpecialPowerModule>(
            &mut world,
            "WeaponChangeSpecialPowerModule",
        );
        let holder = world.spawn().unwrap();
        let module = weapon_change_special_power_module_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpecialAbilityTestToggle"),
                ("FlagsUsedForToggle", "WEAPONSET_TOGGLE_1"),
                ("ToggleOnSleepFrames", "12"),
                ("ToggleOffSleepFrames", "0"),
                ("ToggleOnAttributeModifier", "TestTogglePenalty"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let activation = Activation {
            power: "SpecialAbilityTestToggle",
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
        assert!(
            log.drain().is_empty(),
            "the flag must not raise before the sleep elapses"
        );

        let mut rng = Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut state = UpgradeState::default();
        for frame in 0..30u64 {
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
        assert!(state.has_weapon_set_flag("WEAPONSET_TOGGLE_1"));
    }

    #[test]
    fn a_press_mid_sleep_is_ignored() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<WeaponChangeSpecialPowerModule>(
            &mut world,
            "WeaponChangeSpecialPowerModule",
        );
        let holder = world.spawn().unwrap();
        let module = weapon_change_special_power_module_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("FlagsUsedForToggle", "WEAPONSET_TOGGLE_1"),
                ("ToggleOnSleepFrames", "12"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "P",
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
            "a press mid-sleep must not restart or reverse the toggle"
        );
    }
}
