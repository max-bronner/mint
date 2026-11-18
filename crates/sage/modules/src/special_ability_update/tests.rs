use super::*;
use crate::world_reads::{Hordes, Immobile, Structures};
use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
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
fn unpack_time_of_one_millisecond_still_unpacks_for_a_whole_retail_frame() {
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("UnpackTime", "1"),
            ("PreparationTime", "1"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    assert!(module.data.unpack_needed);
    assert_eq!(module.data.unpack_frames, LogicFrames::from_frames(6));
}

#[test]
fn missing_power_template_is_refused() {
    assert!(special_ability_update_from_fields(&fields(&[]), Frame::ZERO).is_err());
}

#[test]
fn a_condition_outside_the_three_name_enum_is_refused() {
    assert!(special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("RequiredConditions", "TAINT"),
        ]),
        Frame::ZERO,
    )
    .is_err());
}

#[test]
fn trigger_attribute_modifier_is_given_at_the_trigger_for_its_declared_duration() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("Instant", "Yes"),
            ("TriggerAttributeModifier", "Boost"),
            ("AttributeModifierDuration", "2000"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    let given = log
        .drain()
        .into_iter()
        .find_map(|(_, effect)| match effect {
            Effect::SelfTimedModifier { list, lasts } if list.as_ref() == "Boost" => Some(lasts),
            _ => None,
        });
    assert_eq!(
        given,
        Some(Some(retail_duration_ticks(2000))),
        "TriggerAttributeModifier must be given for AttributeModifierDuration"
    );
}

#[test]
fn rejected_conditions_abort_a_preparing_cast() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "1000"),
            ("RejectedConditions", "WEAPON_TOGGLE"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    let conditions =
        ModelConditions::from_pairs([(holder, Names::from(vec![name("WEAPONSET_TOGGLE_1")]))]);
    let mut aborted = false;
    for frame in 1..10u64 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshot(&conditions),
        );
        if log.drain().into_iter().any(|(_, effect)| {
            matches!(
                effect,
                Effect::CastEdge {
                    edge: CastEdge::Abort,
                    ..
                }
            )
        }) {
            aborted = true;
        }
    }
    assert!(
        aborted,
        "a cast must abort while the caster holds a RejectedConditions flag"
    );
}

fn name(value: &str) -> Name {
    Name::from(value)
}

#[test]
fn the_state_machine_walks_unpack_prepare_and_pack_then_stops() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "Athelas"),
            ("StartAbilityRange", "5"),
            ("UnpackTime", "100"),
            ("PreparationTime", "100"),
            ("PackTime", "100"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();

    let mut log = EffectLog::new();
    let activation = Activation {
        power: "Athelas",
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

    let positions = Positions::from_pairs([(holder, (0.0, 0.0, 0.0, 0.0))]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut started_recharge = false;
    for frame in 0..40u64 {
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
            if matches!(effect, Effect::StartRecharge(_)) {
                started_recharge = true;
            }
        }
    }
    assert!(
        started_recharge,
        "the cooldown must start at the trigger (MAP-roundE-abilities.md §9)"
    );
}

fn one_tick_effects(distance: f32, self_radius: f32, target_radius: f32) -> Vec<Effect> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "P"), ("StartAbilityRange", "20")]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
        target: Some(target),
        at: None,
    };
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, self_radius, 0.0)),
        (target, (distance, 0.0, target_radius, 0.0)),
    ]);
    let mut log = EffectLog::new();
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::from_snapshot(&positions),
    );
    log.drain().into_iter().map(|(_, effect)| effect).collect()
}

#[test]
fn a_target_wider_than_start_ability_range_is_reached_at_its_surface() {
    let effects = one_tick_effects(83.0, 5.0, 60.0);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "already within range at the surface: {effects:?}"
    );
}

#[derive(Debug, Clone, PartialEq)]
struct Counting;
impl Trigger for Counting {
    fn fire(&self, entity: Entity, _target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(entity, Effect::StartRecharge(Name::from("fired")));
        }
    }
}

fn triggers_with_order_from(ordered_from: u64, frames: u64) -> usize {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<AbilityUpdate<Counting>>(&mut world, "Repeating");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let data = ability_data_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("StartAbilityRange", "50"),
            ("PreparationTime", "100"),
            ("PersistentPrepTime", "100"),
            ("MustFinishAbility", "Yes"),
            ("IgnoreFacingCheck", "Yes"),
        ]),
        Counting,
    )
    .unwrap();
    let module = AbilityUpdate {
        data: Arc::new(data),
        active: None,
        effect_until: None,
    };
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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
    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, 0.0)),
        (target, (10.0, 0.0, 1.0, 0.0)),
    ]);
    let unordered = PlayerOrdered::default();
    let ordered = PlayerOrdered::from_entities([holder]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut fired = 0usize;
    for frame in 0..frames {
        let orders = if frame >= ordered_from {
            &ordered
        } else {
            &unordered
        };
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&[&positions, orders]),
        );
        fired = fired.saturating_add(
                log.drain()
                    .into_iter()
                    .filter(|(_, effect)| {
                        matches!(effect, Effect::StartRecharge(name) if &**name == "fired")
                    })
                    .count(),
            );
    }
    fired
}

#[test]
fn a_player_order_cancels_a_repeating_cast() {
    let unordered = triggers_with_order_from(u64::MAX, 40);
    assert!(unordered >= 5, "the cast repeats: {unordered}");
    let ordered = triggers_with_order_from(10, 40);
    assert!(
        (1..unordered).contains(&ordered),
        "the order stops the repeat: {ordered} of {unordered}"
    );
}

#[test]
fn a_target_still_outside_its_surface_range_keeps_approaching() {
    let effects = one_tick_effects(87.0, 5.0, 60.0);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "still outside range at the surface: {effects:?}"
    );
}

fn one_tick_horde_effects(
    distance: f32,
    self_radius: f32,
    approach_until_members: bool,
) -> Vec<Effect> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("StartAbilityRange", "20"),
            (
                "ApproachUntilMembersInRange",
                if approach_until_members { "Yes" } else { "No" },
            ),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
        target: Some(target),
        at: None,
    };
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, self_radius, 0.0)),
        (target, (distance, 0.0, 0.0, 0.0)),
    ]);
    let hordes = Hordes::from_entities([holder]);
    let mut log = EffectLog::new();
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::from_snapshots(&[&positions, &hordes]),
    );
    log.drain().into_iter().map(|(_, effect)| effect).collect()
}

#[test]
fn approach_until_members_in_range_counts_the_hordes_own_centre_and_radius() {
    let (range, radius) = (20.0_f32, 10.0_f32);
    let boundary = (range * range - radius).sqrt();
    let in_range = one_tick_horde_effects(boundary - 1.0, radius, true);
    assert!(
        !in_range
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "must already be in range: {in_range:?}"
    );
    let out_of_range = one_tick_horde_effects(boundary + 1.0, radius, true);
    assert!(
        out_of_range
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "must still be approaching: {out_of_range:?}"
    );
}

#[test]
fn approach_until_members_in_range_off_uses_edge_to_edge() {
    let (range, radius) = (20.0_f32, 10.0_f32);
    let boundary = (range * range - radius).sqrt();
    let effects = one_tick_horde_effects(boundary + 1.0, radius, false);
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "edge-to-edge must already be in range at this spot: {effects:?}"
    );
}

fn cast_facing_a_target_behind(
    extra: &[(&'static str, &'static str)],
    realign_after: Option<u64>,
    frames: u64,
) -> Vec<(u64, Effect)> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "ModAuthorAbility");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let mut pairs = vec![
        ("SpecialPowerTemplate", "ModAuthorPower"),
        ("StartAbilityRange", "50"),
        ("PreparationTime", "1"),
    ];
    pairs.extend_from_slice(extra);
    let module = special_ability_update_from_fields(&fields(&pairs), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "ModAuthorPower",
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

    let behind = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, 0.0)),
        (target, (-10.0, 0.0, 1.0, 0.0)),
    ]);
    let aligned = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, core::f32::consts::PI)),
        (target, (-10.0, 0.0, 1.0, 0.0)),
    ]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut seen = Vec::new();
    for frame in 0..frames {
        let positions = match realign_after {
            Some(after) if frame >= after => &aligned,
            _ => &behind,
        };
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshot(positions),
        );
        seen.extend(log.drain().into_iter().map(|(_, effect)| (frame, effect)));
    }
    seen
}

#[test]
fn a_target_placed_behind_the_caster_is_faced_before_the_ability_fires() {
    let seen = cast_facing_a_target_behind(&[], None, 20);
    assert!(
        seen.iter()
            .any(|(_, effect)| matches!(effect, Effect::FaceObject { .. })),
        "a target behind the caster must be turned to: {seen:?}"
    );
    assert!(
        !seen
            .iter()
            .any(|(_, effect)| matches!(effect, Effect::StartRecharge(_))),
        "the ability must not fire before facing completes: {seen:?}"
    );
}

#[test]
fn facing_completing_lets_the_ability_proceed() {
    let seen = cast_facing_a_target_behind(&[], Some(3), 20);
    assert!(
        seen.iter()
            .any(|(_, effect)| matches!(effect, Effect::StartRecharge(_))),
        "the ability must fire once facing is aligned: {seen:?}"
    );
}

#[test]
fn an_immobile_caster_completes_facing_without_turning() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "ModAuthorAbility");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "ModAuthorPower"),
            ("StartAbilityRange", "50"),
            ("PreparationTime", "1"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "ModAuthorPower",
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
    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, 0.0)),
        (target, (-10.0, 0.0, 1.0, 0.0)),
    ]);
    let immobile = Immobile::from_entities([holder]);
    let snapshots: [&dyn core::any::Any; 2] = [&positions, &immobile];
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut seen = Vec::new();
    for frame in 0..10 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&snapshots),
        );
        seen.extend(log.drain().into_iter().map(|(_, effect)| effect));
    }
    assert!(
        !seen
            .iter()
            .any(|effect| matches!(effect, Effect::FaceObject { .. })),
        "an IMMOBILE caster must never be asked to turn: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|effect| matches!(effect, Effect::StartRecharge(_))),
        "an IMMOBILE caster must not wait on a facing it cannot reach: {seen:?}"
    );
}

#[test]
fn a_position_on_the_casters_own_spot_counts_as_faced() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "ModAuthorAbility");
    let holder = world.spawn().unwrap();
    let pairs = [
        ("SpecialPowerTemplate", "ModAuthorPower"),
        ("StartAbilityRange", "50"),
        ("PreparationTime", "1"),
    ];
    let module = special_ability_update_from_fields(&fields(&pairs), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "ModAuthorPower",
        target: None,
        at: Some((7.0, 3.0)),
    };
    let mut log = EffectLog::new();
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let positions = Positions::from_pairs([(holder, (7.0, 3.0, 1.0, 1.0))]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut seen = Vec::new();
    for frame in 0..10 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshot(&positions),
        );
        seen.extend(log.drain().into_iter().map(|(_, effect)| effect));
    }
    assert!(
        seen.iter()
            .any(|effect| matches!(effect, Effect::StartRecharge(_))),
        "a cast at the caster's own spot must not wait on a facing it cannot reach: {seen:?}"
    );
}

#[test]
fn ignore_facing_check_skips_the_facing_phase() {
    let seen = cast_facing_a_target_behind(&[("IgnoreFacingCheck", "Yes")], None, 10);
    assert!(
        !seen
            .iter()
            .any(|(_, effect)| matches!(effect, Effect::FaceObject { .. })),
        "IgnoreFacingCheck = Yes must skip the facing phase: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(_, effect)| matches!(effect, Effect::StartRecharge(_))),
        "the ability must still fire: {seen:?}"
    );
}

fn last_using_ability_cleared(seen: &[(u64, Effect)]) -> Option<u64> {
    seen.iter().rev().find_map(|(frame, effect)| match effect {
        Effect::StatusBits { clear, .. } if clear.iter().any(|n| &**n == "IS_USING_ABILITY") => {
            Some(*frame)
        }
        _ => None,
    })
}

fn ai_busy_issued(seen: &[(u64, Effect)]) -> Option<(u64, LogicFrames)> {
    seen.iter().find_map(|(frame, effect)| match effect {
        Effect::AiBusy(frames) => Some((*frame, *frames)),
        _ => None,
    })
}

#[test]
fn paralyze_duration_when_completed_makes_the_ai_busy_after_the_end() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "100"),
            ("ParalyzeDurationWhenCompleted", "200"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    let mut seen = Vec::new();
    for frame in 0..15u64 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::none(),
        );
        seen.extend(log.drain().into_iter().map(|(_, effect)| (frame, effect)));
    }
    assert_eq!(last_using_ability_cleared(&seen), Some(6), "{seen:?}");
    assert_eq!(
        ai_busy_issued(&seen),
        Some((6, LogicFrames::from_frames(6))),
        "{seen:?}"
    );
}

#[test]
fn paralyze_duration_when_aborted_applies_on_a_cancelled_cast() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "300"),
            ("ParalyzeDurationWhenCompleted", "3000"),
            ("ParalyzeDurationWhenAborted", "100"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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

    let ordered = PlayerOrdered::from_entities([holder]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut seen = Vec::new();
    for frame in 0..15u64 {
        let reads = if frame >= 3 {
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
        seen.extend(log.drain().into_iter().map(|(_, effect)| (frame, effect)));
    }
    assert_eq!(last_using_ability_cleared(&seen), Some(3), "{seen:?}");
    assert_eq!(
        ai_busy_issued(&seen),
        Some((3, LogicFrames::from_frames(6))),
        "{seen:?}"
    );
}

#[test]
fn skip_packing_with_no_target_spares_a_targetless_cast_its_unpack_and_pack() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("UnpackTime", "300"),
            ("PreparationTime", "100"),
            ("PackTime", "300"),
            ("SkipPackingWithNoTarget", "Yes"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    let mut seen = Vec::new();
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
        seen.extend(log.drain().into_iter().map(|(_, effect)| (frame, effect)));
    }
    let raised = |name: &str| {
        seen.iter().any(|(_, effect)| {
            matches!(effect, Effect::ModelConditions { add, .. } if add.iter().any(|n| &**n == name))
        })
    };
    assert!(!raised("UNPACKING"), "{seen:?}");
    assert!(!raised("PACKING"), "{seen:?}");
    assert_eq!(last_using_ability_cleared(&seen), Some(6), "{seen:?}");
}

#[test]
fn freeze_after_trigger_holds_the_caster_for_the_fields_length() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "100"),
            ("FreezeAfterTriggerDuration", "700"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    let mut edges = Vec::new();
    for frame in 0..60u64 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::none(),
        );
        edges.extend(
            log.drain()
                .into_iter()
                .filter_map(|(_, effect)| match effect {
                    Effect::HoldOrders(held) => Some((frame, held)),
                    _ => None,
                }),
        );
    }
    let trigger = u64::from(super::retail_duration_ticks(100).get());
    let freeze = u64::from(super::retail_duration_ticks(700).get());
    assert_eq!(
        edges,
        vec![(trigger, true), (trigger + freeze, false)],
        "hold edges"
    );
}

fn triggers_despite_dead_target(must_finish: bool) -> bool {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let mut pairs = vec![
        ("SpecialPowerTemplate", "P"),
        ("StartAbilityRange", "50"),
        ("PreparationTime", "1000"),
        ("IgnoreFacingCheck", "Yes"),
    ];
    if must_finish {
        pairs.push(("MustFinishAbility", "Yes"));
    }
    let module = special_ability_update_from_fields(&fields(&pairs), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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

    let alive = Positions::from_pairs([
        (holder, (0.0, 0.0, 0.0, 0.0)),
        (target, (10.0, 0.0, 0.0, 0.0)),
    ]);
    let dead = Positions::from_pairs([(holder, (0.0, 0.0, 0.0, 0.0))]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut triggered = false;
    for frame in 0..40u64 {
        let positions = if frame < 10 { &alive } else { &dead };
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshot(positions),
        );
        for (_, effect) in log.drain() {
            if matches!(
                effect,
                Effect::CastEdge {
                    edge: CastEdge::Trigger,
                    ..
                }
            ) {
                triggered = true;
            }
        }
    }
    triggered
}

#[test]
fn must_finish_ability_survives_its_target_dying_mid_cast() {
    assert!(
        triggers_despite_dead_target(true),
        "MustFinishAbility must trigger despite the target's death"
    );
    assert!(
        !triggers_despite_dead_target(false),
        "without it, a dead target aborts the cast before it triggers"
    );
}

#[test]
fn busy_for_duration_ignores_an_order_in_its_window() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "100"),
            ("PersistentPrepTime", "1400"),
            ("PersistentCount", "2"),
            ("BusyForDuration", "700"),
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
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let trigger = u64::from(super::retail_duration_ticks(100).get());
    let busy = u64::from(super::retail_duration_ticks(700).get());
    let persistent = u64::from(super::retail_duration_ticks(1400).get());
    assert!(busy < persistent, "the order must land inside busy only");
    let ordered = PlayerOrdered::from_entities([holder]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut triggered = 0u32;
    for frame in 0..(trigger + persistent + 5) {
        let reads = if (trigger + 1..trigger + busy).contains(&frame) {
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
            if matches!(
                effect,
                Effect::CastEdge {
                    edge: CastEdge::Trigger,
                    ..
                }
            ) {
                triggered += 1;
            }
        }
    }
    assert_eq!(
        triggered, 2,
        "the order inside BusyForDuration's window must be ignored, not cancel the cast \
         before its second trigger"
    );
}

#[test]
fn freeze_after_trigger_ends_the_cast_at_once_but_holds_movement_until_it_runs_out() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "100"),
            ("PackTime", "2000"),
            ("FreezeAfterTriggerDuration", "700"),
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
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let trigger = u64::from(super::retail_duration_ticks(100).get());
    let pack = u64::from(super::retail_duration_ticks(2000).get());
    let freeze = u64::from(super::retail_duration_ticks(700).get());
    assert!(
        freeze < pack,
        "the freeze must run out well before an untouched pack would"
    );
    let order_at = trigger + 1;
    let ordered = PlayerOrdered::from_entities([holder]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut aborted_at = None;
    let mut released_at = None;
    for frame in 0..(trigger + pack + 5) {
        let reads = if frame == order_at {
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
                Effect::StatusBits { clear, .. } if !clear.is_empty() && aborted_at.is_none() => {
                    aborted_at = Some(frame);
                }
                Effect::HoldOrders(false) => released_at = released_at.or(Some(frame)),
                _ => {}
            }
        }
    }
    assert_eq!(
        aborted_at,
        Some(trigger + freeze),
        "the order must cut the cast short at the freeze's length, not run the full pack"
    );
    assert_eq!(
        released_at,
        Some(trigger + freeze),
        "the movement hold must run exactly the freeze's length"
    );
}

#[test]
fn chained_button_presses_only_on_a_completed_cast() {
    fn pressed_names(pairs: &[(&'static str, &'static str)], order_at: Option<u64>) -> Vec<Name> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
        let holder = world.spawn().unwrap();
        let module = special_ability_update_from_fields(&fields(pairs), Frame::ZERO).unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "P",
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
        let ordered = PlayerOrdered::from_entities([holder]);
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut pressed = Vec::new();
        for frame in 0..20u64 {
            let reads = if order_at == Some(frame) {
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
                if let Effect::PressCommand(name) = effect {
                    pressed.push(name);
                }
            }
        }
        pressed
    }

    let completed = pressed_names(
        &[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "100"),
            ("ChainedButton", "NextAbility"),
        ],
        None,
    );
    assert_eq!(
        completed.iter().map(|name| &**name).collect::<Vec<_>>(),
        vec!["NextAbility"],
        "a completed cast must press its chained button exactly once"
    );

    let aborted = pressed_names(
        &[
            ("SpecialPowerTemplate", "P"),
            ("PreparationTime", "1000"),
            ("ChainedButton", "NextAbility"),
        ],
        Some(1),
    );
    assert!(
        aborted.is_empty(),
        "an aborted cast must never press its chained button"
    );
}

#[test]
fn ability_abort_range_ends_preparation_with_no_trigger_once_the_target_leaves_it() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("StartAbilityRange", "50"),
            ("PreparationTime", "1000"),
            ("AbilityAbortRange", "20"),
            ("IgnoreFacingCheck", "Yes"),
            ("ChainedButton", "Marker"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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

    let near = Positions::from_pairs([
        (holder, (0.0, 0.0, 0.0, 0.0)),
        (target, (10.0, 0.0, 0.0, 0.0)),
    ]);
    let far = Positions::from_pairs([
        (holder, (0.0, 0.0, 0.0, 0.0)),
        (target, (1000.0, 0.0, 0.0, 0.0)),
    ]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut triggered = false;
    let mut pressed = false;
    for frame in 0..40u64 {
        let positions = if frame < 5 { &near } else { &far };
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshot(positions),
        );
        for (_, effect) in log.drain() {
            match effect {
                Effect::CastEdge {
                    edge: CastEdge::Trigger,
                    ..
                } => triggered = true,
                Effect::PressCommand(name) if &*name == "Marker" => pressed = true,
                _ => {}
            }
        }
    }
    assert!(
        !triggered,
        "a target that left AbilityAbortRange must end the cast without ever triggering"
    );
    assert!(
        pressed,
        "the cast must still finish (completed, not aborted) once the target leaves range"
    );
}

#[test]
fn instant_fires_at_once_with_no_phase_recharge_or_chained_button() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("Instant", "Yes"),
            ("UnpackTime", "1000"),
            ("PreparationTime", "1000"),
            ("PackTime", "1000"),
            ("ParalyzeDurationWhenCompleted", "5000"),
            ("ChainedButton", "Marker"),
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
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut triggered_at = None;
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
                Effect::CastEdge {
                    edge: CastEdge::Trigger,
                    ..
                } => {
                    assert!(
                        triggered_at.is_none(),
                        "Instant must trigger exactly once, not again once paralyze would have elapsed"
                    );
                    triggered_at = Some(frame);
                }
                Effect::StartRecharge(_) => {
                    panic!(
                        "Instant never reaches startPreparation, so it never starts the cooldown"
                    )
                }
                Effect::PressCommand(_) => {
                    panic!("Instant never reaches finishAbility, so ChainedButton never fires")
                }
                Effect::StatusBits { set, .. } if !set.is_empty() => {
                    panic!("Instant must never set IS_USING_ABILITY - there is no phase to report")
                }
                Effect::ModelConditions { add, .. } if !add.is_empty() => {
                    panic!("Instant must never enter unpack, prepare or pack")
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        triggered_at,
        Some(0),
        "Instant must fire on the first tick after the press, not after a declared unpack/prepare"
    );
}

#[test]
fn trigger_sound_plays_even_when_chained_button_suppresses_recharge() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("Instant", "Yes"),
            ("ChainedButton", "NextAbility"),
            ("TriggerSound", "PowerFireSound"),
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
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut played = Vec::new();
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    for (_, effect) in log.drain() {
        if let Effect::PlayObjectSound(name) = effect {
            played.push(name);
        }
    }
    assert_eq!(played.len(), 1, "exactly one TriggerSound, at the trigger");
    assert_eq!(played[0].as_ref(), "PowerFireSound");
}

fn trigger_reward_of(pairs: &[(&'static str, &'static str)]) -> Vec<Effect> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let mut all = vec![("SpecialPowerTemplate", "P"), ("Instant", "Yes")];
    all.extend_from_slice(pairs);
    let module = special_ability_update_from_fields(&fields(&all), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut Pcg32::new(1),
        &mut DestroyRequests::new(),
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    log.drain()
        .into_iter()
        .map(|(_, effect)| effect)
        .filter(|effect| matches!(effect, Effect::TriggerReward { .. }))
        .collect()
}

#[test]
fn skill_points_for_triggering_falls_back_to_the_experience_award_only_when_unwritten() {
    let rows: [(&[(&'static str, &'static str)], u32, u32); 4] = [
        (&[("AwardXPForTriggering", "7")], 7, 7),
        (
            &[
                ("AwardXPForTriggering", "7"),
                ("SkillPointsForTriggering", "0"),
            ],
            7,
            0,
        ),
        (&[("SkillPointsForTriggering", "3")], 0, 3),
        (
            &[
                ("AwardXPForTriggering", "7"),
                ("SkillPointsForTriggering", "-1"),
            ],
            7,
            7,
        ),
    ];
    for (pairs, experience, skill_points) in rows {
        assert_eq!(
            trigger_reward_of(pairs),
            vec![Effect::TriggerReward {
                experience,
                skill_points
            }],
            "{pairs:?}"
        );
    }
}

#[test]
fn an_approach_gives_up_after_ten_tries_against_a_target_that_has_not_moved() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "P"), ("StartAbilityRange", "5")]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
        target: Some(target),
        at: None,
    };
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, 0.0, 0.0)),
        (target, (1000.0, 0.0, 0.0, 0.0)),
    ]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut approaches = 0usize;
    for frame in 0..20u64 {
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
            if matches!(effect, Effect::MoveToObject { .. }) {
                approaches += 1;
            }
        }
    }
    assert_eq!(
        approaches, 11,
        "retail gives up after 11 idle approaches to a target that has not moved"
    );
}

#[test]
fn a_far_start_ability_range_walks_a_non_structure_target_to_a_fixed_point() {
    fn one_tick_approach(target_is_structure: bool) -> Vec<Effect> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
        let holder = world.spawn().unwrap();
        let target = world.spawn().unwrap();
        let module = special_ability_update_from_fields(
            &fields(&[("SpecialPowerTemplate", "P"), ("StartAbilityRange", "80")]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let mut log = EffectLog::new();
        let activation = Activation {
            power: "P",
            target: Some(target),
            at: None,
        };
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));
        let positions = Positions::from_pairs([
            (holder, (0.0, 0.0, 0.0, 0.0)),
            (target, (1000.0, 0.0, 0.0, 0.0)),
        ]);
        let structures = Structures::from_entities(target_is_structure.then_some(target));
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(0),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&[&positions, &structures]),
        );
        log.drain().into_iter().map(|(_, effect)| effect).collect()
    }
    assert!(
        one_tick_approach(false)
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToApproachPoint { .. })),
        "a non-STRUCTURE target past StartAbilityRange = 50 walks to the approach point"
    );
    assert!(
        one_tick_approach(true)
            .iter()
            .any(|effect| matches!(effect, Effect::MoveToObject { .. })),
        "a STRUCTURE target keeps the ordinary chase regardless of StartAbilityRange"
    );
}

#[test]
fn no_auto_acquire_is_set_at_the_press_not_only_the_unpack() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "P"), ("StartAbilityRange", "5")]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
        target: Some(target),
        at: None,
    };
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let set_no_auto_acquire = log.drain().into_iter().any(|(_, effect)| {
        matches!(effect, Effect::StatusBits { set, .. } if set.iter().any(|n| &**n == "NO_AUTO_ACQUIRE"))
    });
    assert!(
        set_no_auto_acquire,
        "NO_AUTO_ACQUIRE must be set at the press, before the caster has even started its approach"
    );
}

#[test]
fn a_fresh_activation_ends_every_other_active_cast_on_the_object_but_not_its_own() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let a = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "A"), ("PreparationTime", "1000")]),
        Frame::ZERO,
    )
    .unwrap();
    let b = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "B"), ("PreparationTime", "1000")]),
        Frame::ZERO,
    )
    .unwrap();
    registry.attach(&mut world, holder, Frame::ZERO, a).unwrap();
    registry.attach(&mut world, holder, Frame::ZERO, b).unwrap();

    let mut log = EffectLog::new();
    assert!(registry.activate(
        &mut world,
        holder,
        &Activation {
            power: "A",
            target: None,
            at: None,
        },
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    log.drain();

    assert!(registry.activate(
        &mut world,
        holder,
        &Activation {
            power: "B",
            target: None,
            at: None,
        },
        Frame::new(1),
        Effects::into_sink(&mut log)
    ));
    registry.end_other_activations(
        &mut world,
        holder,
        "B",
        Frame::new(1),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
    );
    let seen: Vec<_> = log.drain();
    assert!(
        seen.iter().any(|(_, effect)| matches!(
            effect,
            Effect::CastEdge {
                power,
                edge: CastEdge::Abort,
                ..
            } if &**power == "A"
        )),
        "a fresh activation of B must abort A's own active cast"
    );
    assert!(
        !seen.iter().any(|(_, effect)| matches!(
            effect,
            Effect::CastEdge {
                power,
                edge: CastEdge::Abort,
                ..
            } if &**power == "B"
        )),
        "a module must never end its own fresh activation"
    );
}

#[test]
fn preparation_time_zero_never_persists() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[("SpecialPowerTemplate", "P"), ("PersistentPrepTime", "250")]),
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
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut triggers = 0u32;
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
            if matches!(
                effect,
                Effect::CastEdge {
                    edge: CastEdge::Trigger,
                    ..
                }
            ) {
                triggers += 1;
            }
        }
    }
    assert_eq!(triggers, 1, "PreparationTime 0 must trigger exactly once");
}

fn model_condition_edits(
    pairs: &[(&'static str, &'static str)],
    frames: u64,
) -> Vec<(u64, String, String)> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(&fields(pairs), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    let mut out = Vec::new();
    for frame in 0..frames {
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
                out.push((frame, add.join(" "), remove.join(" ")));
            }
        }
    }
    out
}

#[test]
fn custom_anim_and_duration_rises_on_unpack_and_ends_with_the_ability() {
    let edits = model_condition_edits(
        &[
            ("SpecialPowerTemplate", "P"),
            ("UnpackTime", "1000"),
            ("CustomAnimAndDuration", "AnimState:USER_1 AnimTime:0"),
        ],
        40,
    );
    assert!(
        edits
            .iter()
            .any(|(frame, add, _)| *frame == 0 && add.contains("USER_1")),
        "USER_1 must rise on the unpack frame: {edits:?}"
    );
    assert!(
        edits.iter().any(|(_, _, remove)| remove.contains("USER_1")),
        "USER_1 must be cleared once the ability ends: {edits:?}"
    );
}

#[test]
fn custom_anim_and_duration_never_rises_without_an_unpack() {
    let edits = model_condition_edits(
        &[
            ("SpecialPowerTemplate", "P"),
            ("CustomAnimAndDuration", "AnimState:USER_1 AnimTime:0"),
        ],
        10,
    );
    assert!(
        edits.iter().all(|(_, add, _)| !add.contains("USER_1")),
        "USER_1 must never be raised with no UnpackTime: {edits:?}"
    );
}

#[test]
fn trigger_model_condition_duration_is_retail_frames_not_milliseconds() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("UnpackTime", "1000"),
            (
                "TriggerModelCondition",
                "ModelConditionState:SPECIAL_POWER_1",
            ),
            ("TriggerModelConditionDuration", "20"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let mut log = EffectLog::new();
    let activation = Activation {
        power: "P",
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
    registry.update_with_reads(
        &mut world,
        Frame::new(0),
        &mut rng,
        &mut destroy,
        Effects::into_sink(&mut log),
        Reads::none(),
    );
    let timed = log
        .drain()
        .into_iter()
        .find_map(|(_, effect)| match effect {
            Effect::TimedModelCondition { flag, lasts, .. }
                if flag.as_ref() == "SPECIAL_POWER_1" =>
            {
                Some(lasts)
            }
            _ => None,
        });
    assert_eq!(
        timed,
        Some(LogicFrames::from_frames(120)),
        "20 retail frames must be 120 sim ticks, not 20 ms"
    );
}

#[test]
fn trigger_model_condition_without_a_label_is_refused() {
    let err = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("TriggerModelCondition", "SPECIAL_POWER_1"),
        ]),
        Frame::ZERO,
    );
    assert!(err.is_err());
}

#[test]
fn active_loop_sound_starts_once_and_stops_at_packing() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("StartAbilityRange", "50"),
            ("PreparationTime", "100"),
            ("PersistentPrepTime", "100"),
            ("PersistentCount", "3"),
            ("PackTime", "100"),
            ("IgnoreFacingCheck", "Yes"),
            ("ActiveLoopSound", "LoopFxTest"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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
    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, 0.0)),
        (target, (0.0, 0.0, 1.0, 0.0)),
    ]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut starts = 0usize;
    let mut stops = 0usize;
    for frame in 0..40u64 {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&[&positions]),
        );
        for (_, effect) in log.drain() {
            match effect {
                Effect::StartLoopSound(name) => {
                    assert_eq!(&*name, "LoopFxTest");
                    starts += 1;
                }
                Effect::StopLoopSound => stops += 1,
                _ => {}
            }
        }
    }
    assert_eq!(
        starts, 1,
        "the loop starts exactly once across every persistent trigger"
    );
    assert!(stops >= 1, "packing stops the loop");
}

#[test]
fn do_capture_fx_flashes_only_on_even_phase_crossings() {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<SpecialAbilityUpdate>(&mut world, "SpecialAbilityUpdate");
    let holder = world.spawn().unwrap();
    let target = world.spawn().unwrap();
    let module = special_ability_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "P"),
            ("StartAbilityRange", "50"),
            ("PreparationTime", "10000"),
            ("IgnoreFacingCheck", "Yes"),
            ("DoCaptureFX", "Yes"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "P",
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
    let positions = Positions::from_pairs([
        (holder, (0.0, 0.0, 1.0, 0.0)),
        (target, (0.0, 0.0, 1.0, 0.0)),
    ]);
    let total_frames = u64::from(retail_duration_ticks(10_000).get());
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut flashes = 0usize;
    for frame in 0..=total_frames {
        let mut log = EffectLog::new();
        registry.update_with_reads(
            &mut world,
            Frame::new(frame),
            &mut rng,
            &mut destroy,
            Effects::into_sink(&mut log),
            Reads::from_snapshots(&[&positions]),
        );
        flashes += log
            .drain()
            .into_iter()
            .filter(|(_, effect)| matches!(effect, Effect::CaptureFlash { .. }))
            .count();
    }
    #[allow(clippy::cast_precision_loss)]
    let total = total_frames.max(1) as f32;
    let mut phase = 0.0f32;
    let mut remaining = total_frames;
    let mut expected = 0usize;
    for _ in 0..total_frames {
        remaining -= 1;
        #[allow(clippy::cast_precision_loss)]
        let fraction_left = remaining as f32 / total;
        let before = phase;
        phase += (1.0 - fraction_left) / 3.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (before_floor, after_floor) = (before.floor() as u32, phase.floor() as u32);
        if after_floor > before_floor && after_floor % 2 == 0 {
            expected += 1;
        }
    }
    assert!(expected > 0, "the test window must see at least one edge");
    assert_eq!(flashes, expected);
}
