use super::*;

const DATA: &str = "\
GameData\n  Gravity = -64.0\nEnd\n\
Weapon ZClaw\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  GrabNugget\n    ContainTargetOnEffect = Yes\n    ImpactTargetOnEffect = Yes\n    \
    ShockWaveAmount = 60\n    ShockWaveRadius = 16\n    ShockWaveTaperOff = 1.0\n    \
    ShockWaveZMult = 1.0\n  End\nEnd\n\
Weapon ZSwipe\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = MAGIC\n  End\nEnd\n\
Weapon ZMaul\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  GrabNugget\n    ContainTargetOnEffect = Yes\n  End\n  \
  DamageNugget\n    Damage = 100\n    DamageType = MAGIC\n    Radius = 30\n  End\nEnd\n\
Weapon ZSplash\n  DamageNugget\n    Damage = 1\n    DamageType = MAGIC\n    Radius = 150\n  End\nEnd\n\
Object ZGrabber\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZClaw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = TransportContain Tag_Claws\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    \
    PassengerFilter = ANY +INFANTRY -ZBrute\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n    GrabWeapon = ZSplash\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
ChildObject ZSwiper ZGrabber\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZSwipe\n  End\nEnd\n\
ChildObject ZMauler ZGrabber\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZMaul\n  End\nEnd\n\
Object ZPeon\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Behavior = PhysicsBehavior Tag_Physics\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject ZBrute ZPeon\nEnd\n\
Object ZBird\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZClaw\n  End\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n    GrabTossTimeTrigger = 1.0\n    \
    GrabTossHeightTrigger = 50.0\n  End\n  \
  Behavior = TransportContain Tag_Claws\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    \
    PassengerFilter = ANY +INFANTRY\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Weapon ZTalon\n  AttackRange = 300\n  DelayBetweenShots = 10000\n  \
  GrabNugget\n    ContainTargetOnEffect = Yes\n  End\nEnd\n\
ChildObject ZSlowBird ZBird\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZTalon\n  End\nEnd\n";

const GRABBER_AT: Vec2 = Vec2::new(300.0, 300.0);
const VICTIM_AT: Vec2 = Vec2::new(400.0, 300.0);

fn scene(shooter: &str, victim: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    scene_raised(shooter, victim, None)
}

fn scene_raised(
    shooter: &str,
    victim: &str,
    up: Option<f32>,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let (simulation, ids, entities) = scene_with(shooter, &[(victim, VICTIM_AT)], up);
    (simulation, ids, entities[0], entities[1])
}

fn scene_with(
    shooter: &str,
    victims: &[(&str, Vec2)],
    up: Option<f32>,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = std::iter::once(shooter)
        .chain(victims.iter().map(|(name, _)| *name))
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = std::iter::once((shooter, GRABBER_AT, OTHER, up))
        .chain(victims.iter().map(|(name, at)| (*name, *at, PLAYER, None)))
        .map(|(name, at, player, up)| Spawn {
            ground_offset: up,
            ..kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    player,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn run(simulation: &mut Simulation<RotwkRules>, frames: u32) {
    for _ in 0..frames {
        simulation.tick().unwrap();
    }
}

fn held_by(simulation: &Simulation<RotwkRules>, ids: Ids, victim: Entity) -> Option<Entity> {
    simulation
        .state()
        .get(ids.contained, victim)
        .map(|held| held.container)
}

fn wears_passenger(simulation: &Simulation<RotwkRules>, victim: Entity) -> bool {
    let state = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state, victim)
        .is_some_and(|s| s.model_conditions.iter().any(|n| n.as_ref() == "PASSENGER"))
}

#[test]
fn a_grab_nugget_hit_holds_the_victim_in_the_grabbers_contain() {
    let (mut simulation, ids, grabber, victim) = scene("ZGrabber", "ZPeon");
    run(&mut simulation, 90);
    assert_eq!(held_by(&simulation, ids, victim), Some(grabber));
    assert!(simulation
        .state()
        .get(ids.passengers, grabber)
        .is_some_and(|list| list.0 == vec![victim]));
    assert!(wears_passenger(&simulation, victim));
    assert!(wears(&simulation, victim, "FREEFALL"));
}

#[test]
fn a_grab_spares_its_catch_the_damage_it_does_around_it() {
    let beside = Vec2::new(VICTIM_AT.x, VICTIM_AT.y + 12.0);
    let (mut simulation, ids, entities) =
        scene_with("ZMauler", &[("ZPeon", VICTIM_AT), ("ZPeon", beside)], None);
    let (grabber, victim, neighbour) = (entities[0], entities[1], entities[2]);
    run_to_grab(&mut simulation, ids, victim);
    assert_eq!(held_by(&simulation, ids, victim), Some(grabber));
    let health = |entity| simulation.state().get(ids.health, entity).unwrap().0;
    assert_eq!(
        health(victim).current(),
        health(victim).max(),
        "the catch was hurt"
    );
    assert!(
        health(neighbour).current() < health(neighbour).max(),
        "the neighbour was spared"
    );
}

#[test]
fn a_plain_hit_by_a_grabber_takes_nothing_in() {
    let (mut simulation, ids, swiper, victim) = scene("ZSwiper", "ZPeon");
    run(&mut simulation, 90);
    let health = simulation.state().get(ids.health, victim).unwrap();
    assert!(health.0.current() < health.0.max(), "the swiper never hit");
    assert_eq!(held_by(&simulation, ids, victim), None);
    assert!(simulation
        .state()
        .get(ids.passengers, swiper)
        .is_none_or(|list| list.0.is_empty()));
}

#[test]
fn a_victim_the_contain_refuses_is_shoved_instead() {
    let (mut simulation, ids, _, victim) = scene("ZGrabber", "ZBrute");
    let mut thrown = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        thrown |= simulation.state().contains(ids.knocked, victim);
    }
    assert!(thrown, "the refused victim was never shoved");
    assert_eq!(held_by(&simulation, ids, victim), None);
}

#[test]
fn the_grabbers_death_lets_the_victim_go_alive_and_ungrabbed() {
    let (mut simulation, ids, grabber, victim) = scene("ZGrabber", "ZPeon");
    run(&mut simulation, 90);
    assert_eq!(held_by(&simulation, ids, victim), Some(grabber));
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![grabber],
            },
        )
        .unwrap();
    simulation.submit(OTHER, RotwkCommand::DebugKill).unwrap();
    run(&mut simulation, 3);
    assert_eq!(held_by(&simulation, ids, victim), None);
    assert!(!simulation
        .state()
        .get(ids.health, victim)
        .unwrap()
        .0
        .is_dead());
    assert!(!wears_passenger(&simulation, victim));
    assert!(!wears(&simulation, victim, "FREEFALL"));
}

#[test]
fn a_passenger_held_unattackable_is_no_longer_a_target() {
    let (mut simulation, ids, grabber, victim) = scene("ZGrabber", "ZPeon");
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![grabber],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    run(&mut simulation, 90);
    assert_eq!(held_by(&simulation, ids, victim), Some(grabber));
    run(&mut simulation, 2);
    assert!(!simulation.state().contains(ids.attack_order, grabber));
    assert_ne!(
        simulation
            .state()
            .get(ids.armed, grabber)
            .and_then(|armed| armed.target),
        Some(victim)
    );
}

fn run_to_grab(simulation: &mut Simulation<RotwkRules>, ids: Ids, victim: Entity) -> u64 {
    for _ in 0..300 {
        simulation.tick().unwrap();
        if held_by(simulation, ids, victim).is_some() {
            return simulation.frame().get();
        }
    }
    panic!("no grab");
}

#[test]
fn a_giant_bird_tosses_its_catch_after_the_time_trigger() {
    let (mut simulation, ids, _, victim) = scene("ZBird", "ZPeon");
    let grabbed = run_to_grab(&mut simulation, ids, victim);
    while simulation.frame().get() < grabbed + 28 {
        simulation.tick().unwrap();
    }
    let hold = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.contained, victim)
            .and_then(|held| held.grabbed_at)
    };
    let first = hold(&simulation);
    assert!(first.is_some(), "let go early");
    run(&mut simulation, 4);
    assert_ne!(hold(&simulation), first, "never let go");
}

#[test]
fn a_giant_bird_high_enough_tosses_its_catch_at_once() {
    let (mut simulation, ids, _, victim) = scene_raised("ZBird", "ZPeon", Some(60.0));
    run_to_grab(&mut simulation, ids, victim);
    run(&mut simulation, 2);
    assert_eq!(held_by(&simulation, ids, victim), None);
}

fn wears(simulation: &Simulation<RotwkRules>, entity: Entity, condition: &str) -> bool {
    let state = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state, entity)
        .is_some_and(|s| s.model_conditions.iter().any(|n| n.as_ref() == condition))
}

fn dead(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.health, entity)
        .is_none_or(|health| health.0.is_dead())
}

#[test]
fn a_tossed_catch_dies_of_its_fall_splatted() {
    let (mut simulation, ids, _, victim) = scene_raised("ZSlowBird", "ZPeon", Some(40.0));
    run_to_grab(&mut simulation, ids, victim);
    let mut fell = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        fell |= wears(&simulation, victim, "FREEFALL");
        if dead(&simulation, ids, victim) {
            break;
        }
    }
    assert!(fell, "never fell free");
    assert!(dead(&simulation, ids, victim), "survived its fall");
    assert!(wears(&simulation, victim, "SPLATTED"), "not splatted");
    assert!(!wears(&simulation, victim, "FREEFALL"), "still falling");
}

#[test]
fn a_body_caught_in_its_fall_is_held_not_felled() {
    let (mut simulation, ids, bird, victim) = scene_raised("ZBird", "ZPeon", Some(40.0));
    run_to_grab(&mut simulation, ids, victim);
    let grabbed_at = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.contained, victim)
            .filter(|held| held.container == bird)
            .and_then(|held| held.grabbed_at)
    };
    let first = grabbed_at(&simulation);
    let mut caught_again = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        caught_again |= grabbed_at(&simulation).is_some_and(|at| Some(at) != first);
        if held_by(&simulation, ids, victim).is_some() {
            let carried = simulation.state().store(ids.flight).is_some_and(|store| {
                store
                    .iter()
                    .any(|(_, flight)| flight.carried == Some(victim))
            });
            assert!(!carried, "still riding its fall in the claws");
        }
    }
    assert!(caught_again, "the bird never caught its falling catch");
    assert!(!dead(&simulation, ids, victim), "killed in the claws");
}
