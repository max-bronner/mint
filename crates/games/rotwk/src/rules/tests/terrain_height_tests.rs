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
