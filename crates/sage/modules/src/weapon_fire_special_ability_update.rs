use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, Names, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

const ORDINALS: [&str; 5] = ["ONE", "TWO", "THREE", "FOUR", "FIVE"];

#[derive(Debug, Clone, PartialEq)]
pub struct WeaponTrigger {
    weapon: Name,
    condition: Option<Name>,
    need_living_targets: bool,
    play_pre_fire_fx: bool,
}

impl Trigger for WeaponTrigger {
    fn begin(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        if self.play_pre_fire_fx {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    entity,
                    Effect::PlayWeaponPreFireFX {
                        weapon: Name::clone(&self.weapon),
                    },
                );
            }
        }
        let (Some(condition), Some(log)) = (&self.condition, ctx.effects.get::<EffectLog>()) else {
            return;
        };
        log.push(
            entity,
            Effect::ModelConditions {
                add: Names::from(vec![Name::clone(condition)]),
                remove: Names::from(Vec::new()),
            },
        );
    }

    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let slot = ctx.requester.slot;
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        let at = match target {
            AbilityTarget::Object(target) => PowerPlace::Object(target),
            AbilityTarget::Position(x, y) => {
                if self.need_living_targets || (x == 0.0 && y == 0.0) {
                    PowerPlace::Source
                } else {
                    PowerPlace::At(x, y)
                }
            }
            AbilityTarget::None => PowerPlace::Source,
        };
        log.push(
            entity,
            Effect::FireWeapon {
                weapon: Name::clone(&self.weapon),
                at,
                offset: (0.0, 0.0),
                repeat: None,
                alive_only: false,
                reload_slot: Some(slot),
            },
        );
    }

    fn finish(&self, entity: Entity, ctx: &mut UpdateContext<'_>) {
        let Some(condition) = &self.condition else {
            return;
        };
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            entity,
            Effect::ModelConditions {
                add: Names::from(Vec::new()),
                remove: Names::from(vec![Name::clone(condition)]),
            },
        );
    }
}

pub type WeaponFireSpecialAbilityUpdate = AbilityUpdate<WeaponTrigger>;

#[must_use]
pub fn special_weapon_condition(which: Option<f32>) -> Option<Name> {
    let which = which?;
    if !which.is_finite() {
        return None;
    }
    let ordinal = which.round();
    if ordinal < 1.0 || ordinal > ORDINALS.len() as f32 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = (ordinal - 1.0) as usize;
    ORDINALS
        .get(index)
        .map(|ordinal| Name::from(format!("SPECIAL_WEAPON_{ordinal}")))
}

pub fn weapon_fire_special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<WeaponFireSpecialAbilityUpdate, FieldError> {
    let weapon = fields
        .field("SpecialWeapon")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialWeapon is missing"))?;
    let condition = special_weapon_condition(fields.field_numeric("WhichSpecialWeapon"));
    let need_living_targets =
        crate::fire_weapon_update::flag(fields, "NeedLivingTargets") == Some(true);
    let play_pre_fire_fx =
        crate::fire_weapon_update::flag(fields, "PlayWeaponPreFireFX") == Some(true);
    let trigger = WeaponTrigger {
        weapon,
        condition,
        need_living_targets,
        play_pre_fire_fx,
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
    use crate::world_reads::Positions;
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
    fn missing_special_weapon_is_refused() {
        assert!(weapon_fire_special_ability_update_from_fields(
            &fields(&[("SpecialPowerTemplate", "P")]),
            Frame::ZERO,
        )
        .is_err());
    }

    #[test]
    fn which_special_weapon_selects_the_ordinal_condition_name() {
        assert_eq!(
            special_weapon_condition(Some(1.0)).as_deref(),
            Some("SPECIAL_WEAPON_ONE")
        );
        assert_eq!(
            special_weapon_condition(Some(5.0)).as_deref(),
            Some("SPECIAL_WEAPON_FIVE")
        );
        assert_eq!(special_weapon_condition(Some(0.0)), None);
        assert_eq!(special_weapon_condition(Some(6.0)), None);
        assert_eq!(special_weapon_condition(None), None);
    }

    fn cast(
        extra: &[(&'static str, &'static str)],
        target_x: impl Fn(u64) -> f32,
        frames: u64,
    ) -> Vec<(u64, Effect)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<WeaponFireSpecialAbilityUpdate>(
            &mut world,
            "WeaponFireSpecialAbilityUpdate",
        );
        let holder = world.spawn().unwrap();
        let target = world.spawn().unwrap();
        let mut pairs = vec![
            ("SpecialPowerTemplate", "ModAuthorBlast"),
            ("SpecialWeapon", "ModAuthorBlastWeapon"),
            ("WhichSpecialWeapon", "2"),
            ("StartAbilityRange", "80"),
            ("IgnoreFacingCheck", "Yes"),
        ];
        pairs.extend_from_slice(extra);
        let module =
            weapon_fire_special_ability_update_from_fields(&fields(&pairs), Frame::ZERO).unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "ModAuthorBlast",
            target: Some(target),
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
        let mut seen = Vec::new();
        for frame in 0..frames {
            let x = target_x(frame);
            let positions = Positions::from_pairs(
                [(holder, (0.0, 0.0, 0.0, 0.0)), (target, (x, 0.0, 0.0, 0.0))]
                    .into_iter()
                    .filter(|(_, at)| at.0.is_finite()),
            );
            let mut log = EffectLog::new();
            registry.update_with_reads(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                Reads::from_snapshot(&positions),
            );
            seen.extend(log.drain().into_iter().map(|(_, effect)| (frame, effect)));
        }
        seen
    }

    fn frames_of(seen: &[(u64, Effect)], wanted: impl Fn(&Effect) -> bool) -> Vec<u64> {
        seen.iter()
            .filter(|(_, effect)| wanted(effect))
            .map(|(frame, _)| *frame)
            .collect()
    }

    fn is_fire(effect: &Effect) -> bool {
        matches!(effect, Effect::FireWeapon { .. })
    }

    fn first_add_frame(seen: &[(u64, Effect)], name: &str) -> Option<u64> {
        seen.iter().find_map(|(frame, effect)| match effect {
            Effect::ModelConditions { add, .. } if add.iter().any(|n| n.as_ref() == name) => {
                Some(*frame)
            }
            _ => None,
        })
    }

    #[test]
    fn need_living_targets_picks_where_a_position_target_fires() {
        let fire_at = |need_living_targets: bool| -> Vec<PowerPlace> {
            let seen = cast(
                &[
                    ("PreparationTime", "1000"),
                    ("MustFinishAbility", "Yes"),
                    (
                        "NeedLivingTargets",
                        if need_living_targets { "Yes" } else { "No" },
                    ),
                ],
                |frame| if frame < 3 { 10.0 } else { f32::NAN },
                40,
            );
            seen.into_iter()
                .filter_map(|(_, effect)| match effect {
                    Effect::FireWeapon { at, .. } => Some(at),
                    _ => None,
                })
                .collect()
        };
        let yes = fire_at(true);
        assert!(!yes.is_empty(), "must still fire despite the dead target");
        assert!(
            yes.iter().all(|at| matches!(at, PowerPlace::Source)),
            "{yes:?}"
        );
        let no = fire_at(false);
        assert!(!no.is_empty());
        assert!(
            no.iter().all(
                |at| matches!(at, PowerPlace::At(x, y) if (*x - 10.0).abs() < 1e-3 && *y == 0.0)
            ),
            "{no:?}"
        );
    }

    #[test]
    fn the_ability_machine_walks_unpacking_preparing_packing() {
        let seen = cast(
            &[
                ("UnpackTime", "900"),
                ("PreparationTime", "900"),
                ("PackTime", "900"),
            ],
            |_| 10.0,
            150,
        );
        assert_eq!(first_add_frame(&seen, "UNPACKING"), Some(0), "{seen:?}");
        assert_eq!(first_add_frame(&seen, "PREPARING"), Some(30), "{seen:?}");
        assert_eq!(first_add_frame(&seen, "PACKING"), Some(60), "{seen:?}");
        let cleared_at_end = seen.iter().any(|(frame, effect)| {
            *frame == 90
                && matches!(effect, Effect::ModelConditions { remove, .. }
                    if ["UNPACKING", "PREPARING", "PACKING"]
                        .iter()
                        .all(|name| remove.iter().any(|n| n.as_ref() == *name)))
        });
        assert!(cleared_at_end, "{seen:?}");
    }

    #[test]
    fn unpacking_variation_adds_its_packing_type() {
        let seen = cast(&[("UnpackingVariation", "3")], |_| 10.0, 10);
        assert_eq!(
            first_add_frame(&seen, "PACKING_TYPE_3"),
            Some(0),
            "{seen:?}"
        );
    }

    #[test]
    fn skip_continue_fires_a_persistent_ability_once_then_holds_before_packing() {
        let seen = cast(
            &[
                ("UnpackTime", "900"),
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "1500"),
                ("PackTime", "1100"),
                ("SkipContinue", "Yes"),
            ],
            |_| 10.0,
            600,
        );
        let fired = frames_of(&seen, is_fire);
        assert_eq!(fired.len(), 1, "{seen:?}");
        let packing_at = first_add_frame(&seen, "PACKING");
        assert_eq!(packing_at, Some(fired[0] + 48), "{seen:?}");
        let cleared = frames_of(&seen, |effect| {
            matches!(effect, Effect::ModelConditions { remove, .. }
                if remove.iter().any(|name| name.as_ref() == "SPECIAL_WEAPON_TWO"))
        });
        assert_eq!(cleared.len(), 1, "the ability must end: {seen:?}");
        assert_eq!(cleared[0], packing_at.unwrap() + 36, "{seen:?}");
    }

    #[test]
    fn without_skip_continue_a_persistent_ability_repeats_until_its_target_dies() {
        let seen = cast(
            &[("PreparationTime", "1"), ("PersistentPrepTime", "1000")],
            |frame| if frame < 200 { 10.0 } else { f32::NAN },
            400,
        );
        let fired = frames_of(&seen, is_fire);
        assert_eq!(fired.len(), 7, "every 30 frames to frame 200: {seen:?}");
        assert_eq!(fired[1] - fired[0], 30);
    }

    #[test]
    fn a_persistent_count_repeats_the_trigger_that_many_times() {
        let seen = cast(
            &[
                ("PreparationTime", "1"),
                ("PersistentPrepTime", "900"),
                ("PersistentCount", "3"),
            ],
            |_| 10.0,
            600,
        );
        let fired = frames_of(&seen, is_fire);
        assert_eq!(fired.len(), 3, "{seen:?}");
        assert_eq!(
            fired[1] - fired[0],
            30,
            "900 ms rounds up to 1000 ms at retail's 200 ms frame"
        );
    }

    #[test]
    fn the_pose_starts_on_arrival_and_the_unit_stays_put() {
        let seen = cast(
            &[("UnpackTime", "900"), ("PreparationTime", "1")],
            |frame| match frame {
                0..10 => 200.0,
                10..20 => 50.0,
                _ => 300.0,
            },
            120,
        );
        let posed = frames_of(&seen, |effect| {
            matches!(effect, Effect::ModelConditions { add, .. }
                if add.iter().any(|name| name.as_ref() == "SPECIAL_WEAPON_TWO"))
        });
        let halted = frames_of(&seen, |effect| matches!(effect, Effect::Halt));
        let moved = frames_of(&seen, |effect| {
            matches!(effect, Effect::MoveToApproachPoint { .. })
        });
        let fired = frames_of(&seen, is_fire);
        assert_eq!(posed, vec![10], "{seen:?}");
        assert_eq!(halted, vec![10], "{seen:?}");
        assert_eq!(moved, (0..10).collect::<Vec<_>>(), "{seen:?}");
        assert_eq!(fired.len(), 1, "{seen:?}");
        assert!(fired[0] >= 10 + 27, "the unpack runs before the trigger");
    }

    #[test]
    fn a_charge_does_not_halt_on_arrival() {
        let seen = cast(&[("ChargeAttackSpeedBoost", "Yes")], |_| 10.0, 5);
        assert!(frames_of(&seen, |effect| matches!(effect, Effect::Halt)).is_empty());
    }

    fn is_lose_stealth(effect: &Effect) -> bool {
        matches!(effect, Effect::LoseStealth)
    }

    #[test]
    fn lose_stealth_on_trigger_reveals_through_unpack_and_pack() {
        let seen = cast(
            &[
                ("UnpackTime", "900"),
                ("PreparationTime", "900"),
                ("PackTime", "900"),
                ("LoseStealthOnTrigger", "Yes"),
                ("PreTriggerUnstealthTime", "2000"),
            ],
            |_| 10.0,
            150,
        );
        let revealed = frames_of(&seen, is_lose_stealth);
        assert!(revealed.contains(&1), "{seen:?}");
        assert!(revealed.iter().any(|frame| *frame >= 60), "{seen:?}");
    }

    #[test]
    fn without_lose_stealth_on_trigger_nothing_reveals() {
        let seen = cast(
            &[
                ("UnpackTime", "900"),
                ("PreparationTime", "900"),
                ("PackTime", "900"),
                ("PreTriggerUnstealthTime", "2000"),
            ],
            |_| 10.0,
            150,
        );
        assert!(frames_of(&seen, is_lose_stealth).is_empty(), "{seen:?}");
    }

    fn is_pre_fire_fx(effect: &Effect) -> bool {
        matches!(effect, Effect::PlayWeaponPreFireFX { .. })
    }

    #[test]
    fn play_weapon_pre_fire_fx_plays_once_at_unpack_start() {
        let seen = cast(
            &[("UnpackTime", "900"), ("PlayWeaponPreFireFX", "Yes")],
            |_| 10.0,
            40,
        );
        let played = frames_of(&seen, is_pre_fire_fx);
        assert_eq!(played, vec![0], "{seen:?}");
    }

    #[test]
    fn without_play_weapon_pre_fire_fx_nothing_plays() {
        let seen = cast(&[("UnpackTime", "900")], |_| 10.0, 40);
        assert!(frames_of(&seen, is_pre_fire_fx).is_empty(), "{seen:?}");
    }
}
