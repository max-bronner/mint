use super::*;

const FALLER_DATA: &str = "\
Locomotor FallerLocomotor\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  PreferredHeight = 121\n  PreferredAttackHeight = 5\n  \
  LocomotorWorksWhenDead = Yes\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Object Faller\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 60000\n  End\n  \
  LocomotorSet\n    Locomotor = FallerLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_dead_flier_that_works_when_dead_falls_by_gravity_and_splats() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        FALLER_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, _) = KindTable::closure(&data, &["Faller".to_owned()]);
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind("Faller").unwrap(),
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(32, 32), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![unit])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_gravity(Some(-64.0));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::DebugCommand::Kill {
                target: crate::rules::DebugTarget::All,
            }),
        )
        .unwrap();
    simulation.tick().unwrap();
    let z = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.elevation, entity).unwrap().0
    };
    let before = z(&simulation);
    for _ in 0..6 {
        simulation.tick().unwrap();
    }
    let drop = before - z(&simulation);
    assert!(
        (drop - 2.56).abs() < 0.01,
        "dropped {drop} in one retail frame"
    );
    for _ in 0..(12 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert!(z(&simulation) < 0.5, "still at {}", z(&simulation));
    let state = simulation.rules().upgrade_state_id().unwrap();
    assert!(simulation
        .state()
        .get(state, entity)
        .is_some_and(|s| s.model_conditions.iter().any(|n| n.as_ref() == "SPLATTED")));
}

const BLENDER_DATA: &str = "\
Locomotor BlenderLocomotor\n  Surfaces = AIR\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\n  PreferredHeight = 100\n  PreferredAttackHeight = 10\n  CirclingRadius = 200\n  \
  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Weapon Beak\n  AttackRange = 20\n  DelayBetweenShots = 60000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Blender\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Beak\n  End\n  \
  LocomotorSet\n    Locomotor = BlenderLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Prey\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn an_attacking_flier_with_a_circling_radius_sinks_toward_its_attack_height() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        BLENDER_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Blender", "Prey"].map(str::to_owned);
    let (kinds, _) = KindTable::closure(&data, &names);
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
    let spawns = vec![spawn("Blender", 300.0, PLAYER), spawn("Prey", 450.0, OTHER)];
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
        let at = simulation
            .state()
            .get(ids.body, units[0])
            .unwrap()
            .0
            .position;
        simulation.state().get(ids.elevation, units[0]).unwrap().0
            - simulation.rules().ground_height(at.x, at.y)
    };
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert!(
        above(&simulation) > 95.0,
        "cruising at {}",
        above(&simulation)
    );
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
    let mut lowest = f32::MAX;
    for _ in 0..(15 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        lowest = lowest.min(above(&simulation));
    }
    assert!(lowest < 60.0, "never sank below {lowest}");
}
