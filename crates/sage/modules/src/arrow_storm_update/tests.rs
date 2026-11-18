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
fn missing_weapon_template_is_refused() {
    assert!(
        arrow_storm_update_from_fields(&fields(&[("SpecialPowerTemplate", "P")]), Frame::ZERO,)
            .is_err()
    );
}

#[test]
fn a_mod_authored_shape_reads_its_own_fields() {
    let module = arrow_storm_update_from_fields(
        &fields(&[
            ("SpecialPowerTemplate", "ModAuthorStorm"),
            ("WeaponTemplate", "ModAuthorArrow"),
            ("TargetRadius", "120"),
            ("ShotsPerTarget", "1"),
            ("ShotsPerBurst", "3"),
            ("MaxShots", "50"),
            ("CanShootEmptyGround", "Yes"),
        ]),
        Frame::ZERO,
    )
    .unwrap();
    let trigger = &module.data.trigger;
    assert_eq!(&*trigger.weapon, "ModAuthorArrow");
    assert_eq!(trigger.radius, 120.0);
    assert_eq!(trigger.shots_per_burst, 3);
    assert_eq!(trigger.shots_per_target, 1);
    assert_eq!(trigger.max_shots, 50);
    assert!(trigger.can_shoot_empty_ground);
    assert_eq!(module.data.persistent_count(), Some(17));
}

fn cast(extra: &[(&'static str, &'static str)], frames: u64) -> Vec<(u64, Effect)> {
    let mut world = World::new();
    let mut registry = ModuleRegistry::new(&mut world);
    registry.register::<ArrowStormUpdate>(&mut world, "ArrowStormUpdate");
    let holder = world.spawn().unwrap();
    let mut pairs = vec![
        ("SpecialPowerTemplate", "SpecialAbilityArrowStorm"),
        ("WeaponTemplate", "LegolasBowArrowStorm"),
        ("TargetRadius", "120"),
        ("ShotsPerTarget", "1"),
        ("ShotsPerBurst", "3"),
        ("MaxShots", "10"),
        ("CanShootEmptyGround", "Yes"),
        ("PreparationTime", "200"),
        ("PersistentPrepTime", "100"),
        ("IgnoreFacingCheck", "Yes"),
        ("StartAbilityRange", "1000"),
    ];
    pairs.extend_from_slice(extra);
    let module = arrow_storm_update_from_fields(&fields(&pairs), Frame::ZERO).unwrap();
    registry
        .attach(&mut world, holder, Frame::ZERO, module)
        .unwrap();
    let activation = Activation {
        power: "SpecialAbilityArrowStorm",
        target: None,
        at: Some((0.0, 0.0)),
    };
    let mut log = EffectLog::new();
    assert!(registry.activate(
        &mut world,
        holder,
        &activation,
        Frame::ZERO,
        Effects::into_sink(&mut log)
    ));

    let positions = crate::world_reads::Positions::from_pairs([(holder, (0.0, 0.0, 1.0, 0.0))]);
    let mut rng = Pcg32::new(1);
    let mut destroy = DestroyRequests::new();
    let mut seen = Vec::new();
    for frame in 0..frames {
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

#[test]
fn a_burst_fires_every_persistent_prep_time_and_stops_after_max_shots_bursts() {
    let seen = cast(&[], 25);
    let bursts: Vec<u64> = seen
        .iter()
        .filter(|(_, effect)| matches!(effect, Effect::ArrowStorm { .. }))
        .map(|(frame, _)| *frame)
        .collect();
    assert_eq!(bursts, vec![6, 12, 18, 24], "{seen:?}");
    for (_, effect) in &seen {
        if let Effect::ArrowStorm {
            weapon,
            radius,
            shots_per_burst,
            shots_per_target,
            max_shots,
            can_shoot_empty_ground,
            ..
        } = effect
        {
            assert_eq!(&**weapon, "LegolasBowArrowStorm");
            assert_eq!(*radius, 120.0);
            assert_eq!(*shots_per_burst, 3);
            assert_eq!(*shots_per_target, 1);
            assert_eq!(*max_shots, 10);
            assert!(*can_shoot_empty_ground);
        }
    }
}

#[test]
fn the_cast_clears_its_shot_count_when_it_ends() {
    let seen = cast(&[], 25);
    let cleared: Vec<u64> = seen
        .iter()
        .filter(|(_, effect)| matches!(effect, Effect::ClearArrowStorm))
        .map(|(frame, _)| *frame)
        .collect();
    assert_eq!(cleared, vec![24], "{seen:?}");
}
