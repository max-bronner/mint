use super::*;

const DATA: &str = "\
Locomotor WalkerLocomotor\n  Surfaces = GROUND\n  Acceleration = 100\n  Braking = 100\n  \
  TurnTime = 500\n  ZAxisBehavior = NO_Z_MOTIVE_FORCE\nEnd\n\
Locomotor FlierLocomotor\n  Surfaces = AIR\n  Acceleration = 100\n  Braking = 100\n  \
  TurnTime = 500\n  PreferredHeight = 50\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Locomotor FloaterLocomotor\n  Surfaces = GROUND\n  Acceleration = 100\n  Braking = 100\n  \
  TurnTime = 500\n  ZAxisBehavior = FLOATING_Z\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = WalkerLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Flier\n  Side = Men\n  KindOf = AIRCRAFT SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = FlierLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Floater\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = FloaterLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn ramp(width: u32, height: u32) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: (0..width * height)
            .map(|i| u16::try_from((i % width) * 48).unwrap())
            .collect(),
    }
}

fn after_a_walk_up_the_ramp(name: &str) -> (f32, f32) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[name.to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind(name).unwrap(),
            Vec2::new(100.0, 480.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&ramp(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![unit])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 850.0,
                y: 480.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, entity);
    assert!(
        at.x > 130.0,
        "the unit must have climbed the ramp, at {at:?}"
    );
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, entity);
    let z = simulation.state().get(ids.elevation, entity).unwrap().0;
    (z, simulation.rules().ground_height(at.x, at.y))
}

#[test]
fn a_ground_mover_stands_on_the_terrain_it_has_walked_onto() {
    let (z, ground) = after_a_walk_up_the_ramp("Walker");
    assert_eq!(z, ground);
}

#[test]
fn a_floating_body_starts_on_the_terrain_and_never_sinks_into_it() {
    let (z, ground) = after_a_walk_up_the_ramp("Floater");
    assert_eq!(z, ground);
}

#[test]
fn a_flier_keeps_its_preferred_height_above_the_terrain_it_crosses() {
    let (z, ground) = after_a_walk_up_the_ramp("Flier");
    assert!(
        (z - (ground + 50.0)).abs() < 0.5,
        "z={z} ground={ground}, expected within 0.5 of {}",
        ground + 50.0
    );
}

#[test]
fn a_flier_climbing_to_its_cruise_height_rises_every_tick() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, _) = KindTable::closure(&data, &["Flier".to_owned()]);
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind("Flier").unwrap(),
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let flat = HeightMapData {
        width: 32,
        height: 32,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![10; 32 * 32],
    };
    let passability = Passability::build(&flat, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![unit])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation.tick().unwrap();
    let mut z = simulation.state().get(ids.elevation, entity).unwrap().0;
    for tick in 0..12 {
        simulation.tick().unwrap();
        let next = simulation.state().get(ids.elevation, entity).unwrap().0;
        assert!(next > z, "tick {tick}: {z} -> {next}");
        z = next;
    }
}

const DIVE_DATA: &str = "\
Locomotor DiverLocomotor\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  PreferredHeight = 60\n  PreferredAttackHeight = 10\n  \
  SwoopStandoffRadius = 200\n  SwoopStandoffHeight = 60\n  AirborneTargetingHeight = 25\n  \
  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Weapon Talons\n  AttackRange = 20\n  DelayBetweenShots = 500\n  CanSwoop = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Diver\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Talons\n  End\n  \
  LocomotorSet\n    Locomotor = DiverLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Prey\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_flier_dives_to_its_attack_height_on_its_target_and_climbs_back_after() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DIVE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Diver", "Prey"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 480.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![spawn("Diver", 300.0, PLAYER), spawn("Prey", 450.0, OTHER)];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let above = |simulation: &Simulation<RotwkRules>| {
        let at = position(simulation, ids, units[0]);
        simulation.state().get(ids.elevation, units[0]).unwrap().0
            - simulation.rules().ground_height(at.x, at.y)
    };
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    let airborne = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .airborne_category(simulation.state(), ids, units[0])
    };
    for _ in 0..(2 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert!(airborne(&simulation).is_some());
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let first = above(&simulation);
    assert!(
        first < 60.0 && first > 10.0,
        "two ticks into the dive the diver is between its heights, at {first}"
    );
    for _ in 0..(5 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert_eq!(above(&simulation), 10.0);
    assert!(airborne(&simulation).is_none());
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 20.0,
                y: 480.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..(12 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let above = above(&simulation);
    assert!(
        (above - 60.0).abs() < 0.01,
        "expected to have climbed back to 60.0, at {above}"
    );
    assert!(!simulation.state().contains(ids.flight_level, units[0]));
}

#[test]
fn a_diver_reaches_its_attack_height_as_it_comes_into_range() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DIVE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Diver", "Prey"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 480.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![spawn("Diver", 300.0, PLAYER), spawn("Prey", 450.0, OTHER)];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
        .unwrap();
    for _ in 0..(5 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let at = position(&simulation, ids, units[0]);
        if at.minus(position(&simulation, ids, units[1])).length() <= 30.5 {
            let above = simulation.state().get(ids.elevation, units[0]).unwrap().0
                - simulation.rules().ground_height(at.x, at.y);
            assert!(above <= 12.0, "came into range {above} above the ground");
            return;
        }
    }
    panic!("the diver never came into range");
}

#[test]
fn a_swooping_diver_strikes_only_once_down_at_its_attack_height() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        DIVE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Diver", "Prey"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 480.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![spawn("Diver", 300.0, PLAYER), spawn("Prey", 315.0, OTHER)];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[1] })
        .unwrap();
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let health = simulation.state().get(ids.health, units[1]).unwrap().0;
        if health.current() < health.max() {
            let at = position(&simulation, ids, units[0]);
            let above = simulation.state().get(ids.elevation, units[0]).unwrap().0
                - simulation.rules().ground_height(at.x, at.y);
            assert!(above <= 10.5, "struck from {above} above the ground");
            return;
        }
    }
    panic!("the diver never struck");
}

#[test]
fn a_body_standing_still_is_passed_over_and_one_that_walks_is_stood_again() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("Walker")]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind("Walker").unwrap(),
            Vec2::new(100.0, 480.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&ramp(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![unit])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    let still = simulation
        .state()
        .get(ids.elevation, entity)
        .copied()
        .unwrap();
    assert!(still.1.is_some(), "a body at rest is recorded");
    simulation.tick().unwrap();
    assert_eq!(simulation.state().get(ids.elevation, entity), Some(&still));
    let walls = crate::wall_layer::layer_of(simulation.state(), ids.wall_layer)
        .map(crate::wall_layer::WallLayer::generation);
    let at = position(&simulation, ids, entity);
    assert!(RotwkRules::stood_still(
        simulation.state(),
        ids,
        entity,
        at,
        walls
    ));
    assert!(
        !RotwkRules::stood_still(
            simulation.state(),
            ids,
            entity,
            Vec2::new(at.x + 10.0, at.y),
            walls
        ),
        "a body moved by a pass that leaves its height alone is stood again"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 400.0,
                y: 480.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    let at = position(&simulation, ids, entity);
    assert!(at.x > 300.0, "the walker climbed the ramp, at {at:?}");
    let z = simulation.state().get(ids.elevation, entity).unwrap().0;
    assert_eq!(z, simulation.rules().ground_height(at.x, at.y));
    assert!(z > still.0);
}
