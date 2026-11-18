use super::*;

const DATA: &str = "\
Locomotor FootLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 100\nEnd\n\
Weapon Blade\n  AttackRange = 10\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Footman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = FootLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pillar\n  Side = Men\n  KindOf = INFANTRY SELECTABLE LARGE_RECTANGLE_PATHFIND\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = FootLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Trampler\n  Side = Men\n  KindOf = INFANTRY SELECTABLE PATH_THROUGH_INFANTRY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = FootLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Crusher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrusherLevel = 5\n  \
  MinCrushVelocityPercent = 0%\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = FootLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Crushable\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = FootLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Keepsake\n  Side = Men\n  KindOf = SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 10\nEnd\n";

fn world(units: &[(&str, Vec2, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Footman",
        "Pillar",
        "Crusher",
        "Crushable",
        "Trampler",
        "Keepsake",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = units
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec!["Creeps".to_owned()],
            vec![crate::alliance::MapRelations {
                universal_hostile: true,
                ..crate::alliance::MapRelations::default()
            }],
        );
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

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation.state().get(ids.body, entity).unwrap().0.position
}

#[test]
fn an_idle_unit_an_enemy_stands_on_moves_off_it() {
    let at = Vec2::new(300.0, 300.0);
    let (mut simulation, ids, entities) = world(&[
        ("Footman", at, PLAYER),
        ("Pillar", Vec2::new(302.0, 300.0), OTHER),
    ]);
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let footman = position(&simulation, ids, entities[0]);
    let pillar = position(&simulation, ids, entities[1]);
    assert!(
        footman.minus(pillar).length() >= 10.0,
        "the footman stayed on the pillar: {footman:?} against {pillar:?}"
    );
    assert!(
        pillar.minus(Vec2::new(302.0, 300.0)).length() < 1.0,
        "the pillar moved to {pillar:?}"
    );
}

#[test]
fn a_path_through_infantry_unit_keeps_its_ground_under_enemy_infantry() {
    let at = Vec2::new(300.0, 300.0);
    let (mut simulation, ids, entities) = world(&[
        ("Trampler", at, PLAYER),
        ("Pillar", Vec2::new(302.0, 300.0), OTHER),
    ]);
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let trampler = position(&simulation, ids, entities[0]);
    assert!(
        trampler.minus(at).length() < 1.0,
        "the trampler moved off to {trampler:?}"
    );
}

#[test]
fn a_walker_passing_an_idle_enemy_draws_its_attack() {
    let (mut simulation, ids, entities) = world(&[
        ("Footman", Vec2::new(300.0, 300.0), PLAYER),
        ("Footman", Vec2::new(250.0, 312.0), OTHER),
    ]);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[1]],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 360.0,
                y: 312.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let targets = |entity: Entity, other: Entity| {
        simulation
            .state()
            .get(ids.attack_order, entity)
            .is_some_and(|order| order.target == other)
    };
    assert!(
        targets(entities[0], entities[1]),
        "the idle footman never attacked the walker"
    );
    assert!(
        !targets(entities[1], entities[0]),
        "the walker left its move order to attack"
    );
}

#[test]
fn an_idle_crusher_runs_down_a_passing_enemy_instead_of_swinging() {
    let (mut simulation, ids, entities) = world(&[
        ("Crusher", Vec2::new(300.0, 300.0), PLAYER),
        ("Crushable", Vec2::new(250.0, 312.0), OTHER),
    ]);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[1]],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 360.0,
                y: 312.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..3 * SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    assert!(
        simulation
            .state()
            .get(ids.attack_order, entities[0])
            .is_none(),
        "the idle crusher swung instead of running its passing enemy down"
    );
}

#[test]
fn a_walker_never_yields_to_an_enemy_body_without_a_locomotor() {
    let (mut simulation, ids, entities) = world(&[
        ("Footman", Vec2::new(300.0, 300.0), PLAYER),
        ("Keepsake", Vec2::new(360.0, 300.0), OTHER),
    ]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entities[0]],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 420.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..8 * SIMULATION_HZ {
        simulation.tick().unwrap();
        assert!(
            !simulation.state().contains(ids.contact, entities[0]),
            "the footman yielded to the keepsake at {:?}",
            position(&simulation, ids, entities[0])
        );
    }
    assert!(
        position(&simulation, ids, entities[0])
            .minus(Vec2::new(420.0, 300.0))
            .length()
            < 8.0,
        "the footman never reached its goal"
    );
}

#[test]
fn a_blocked_mover_asks_for_its_detour_on_the_frame_its_id_selects() {
    let (mut simulation, ids, entities) = world(&[
        ("Keepsake", Vec2::new(50.0, 50.0), PLAYER),
        ("Keepsake", Vec2::new(80.0, 50.0), PLAYER),
        ("Footman", Vec2::new(300.0, 300.0), PLAYER),
        ("Pillar", Vec2::new(326.0, 300.0), OTHER),
    ]);
    let walker = entities[2];
    assert_eq!(walker.index() % 5, 2, "the id this test relies on");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![walker],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 420.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut blocked_ticks = 0;
    let mut seen = false;
    for _ in 0..8 * SIMULATION_HZ {
        simulation.tick().unwrap();
        let blocked = simulation
            .state()
            .get(ids.contact, walker)
            .is_some_and(|c| c.blocked());
        if blocked {
            seen = true;
            blocked_ticks += 1;
        } else if seen {
            break;
        }
    }
    assert!(seen, "the walker was never blocked");
    assert!(
        blocked_ticks <= 2 * SIMULATION_HZ / 5 + 1,
        "blocked for {blocked_ticks} ticks"
    );
}
