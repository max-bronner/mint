use super::*;

const DATA: &str = "\
Locomotor FastLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor SlowLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Weapon Fists\n  AttackRange = 20\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Fast\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fists\n  End\n  \
  LocomotorSet\n    Locomotor = FastLocomotor\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Slow\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = SlowLocomotor\n    Condition = SET_NORMAL\n    Speed = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Prey\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn build(placed: &[(&str, Vec2, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Fast", "Slow", "Prey"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = placed
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
        .with_construction(Arc::clone(&kinds), production);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let found = placed
        .iter()
        .map(|(_, at, _)| {
            simulation
                .state()
                .store(ids.body)
                .unwrap()
                .iter()
                .find(|(_, body)| body.0.position == *at)
                .unwrap()
                .0
        })
        .collect();
    (simulation, ids, found)
}

fn scene(prey_at: Vec2) -> (Simulation<RotwkRules>, Ids, Entity, Entity, Entity) {
    let (simulation, ids, found) = build(&[
        ("Fast", Vec2::new(0.0, 0.0), PLAYER),
        ("Slow", Vec2::new(0.0, 80.0), PLAYER),
        ("Prey", prey_at, OTHER),
    ]);
    (simulation, ids, found[0], found[1], found[2])
}

fn capped_after_cohere(prey_distance: f32) -> (bool, bool) {
    let (mut simulation, ids, fast, slow, prey) = scene(Vec2::new(prey_distance, 0.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![fast],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: prey })
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, fast),
        "the attack order should be recorded before the group move"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![fast, slow],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 50.0,
                y: 40.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (
        simulation.state().contains(ids.formation_speed, fast),
        simulation.state().contains(ids.formation_speed, slow),
    )
}

#[test]
fn a_victim_within_600_vetoes_the_whole_groups_cap() {
    assert_eq!(capped_after_cohere(500.0), (false, false));
}

#[test]
fn a_victim_past_600_leaves_the_ordinary_cap_in_place() {
    assert_eq!(capped_after_cohere(700.0), (true, true));
}

#[test]
fn the_group_cap_ends_when_the_capped_mover_arrives() {
    let (mut simulation, ids, fast, slow, _prey) = scene(Vec2::new(5000.0, 5000.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![fast, slow],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 90.0,
                y: 40.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.formation_speed, fast),
        "the faster mover should start out capped to the slower one"
    );
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.path, fast),
        "the fast mover should have arrived by now"
    );
    assert!(
        !simulation.state().contains(ids.formation_speed, fast),
        "arrival should have dropped its group speed cap"
    );
}

fn order(simulation: &mut Simulation<RotwkRules>, units: Vec<Entity>, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
    simulation.tick().unwrap();
}

#[test]
fn an_acquired_target_uncoheres_the_attackers_own_group_only() {
    let (mut simulation, ids, found) = build(&[
        ("Fast", Vec2::new(100.0, 100.0), PLAYER),
        ("Slow", Vec2::new(100.0, 180.0), PLAYER),
        ("Fast", Vec2::new(100.0, 800.0), PLAYER),
        ("Slow", Vec2::new(100.0, 880.0), PLAYER),
        ("Prey", Vec2::new(500.0, 100.0), OTHER),
    ]);
    let (fast, slow, other_fast, other_slow, prey) =
        (found[0], found[1], found[2], found[3], found[4]);
    order(
        &mut simulation,
        vec![fast, slow],
        RotwkCommand::AttackMove {
            x: 500.0,
            y: 100.0,
            queue: false,
        },
    );
    order(
        &mut simulation,
        vec![other_fast, other_slow],
        RotwkCommand::MoveTo {
            x: 800.0,
            y: 800.0,
            queue: false,
        },
    );
    assert!(
        simulation.state().contains(ids.formation_speed, slow)
            && simulation.state().contains(ids.formation_speed, other_slow),
        "the fixture's premise: both groups start out capped"
    );
    let acquired = (0..60 * SIMULATION_HZ).any(|_| {
        simulation.tick().unwrap();
        simulation
            .state()
            .get(ids.armed, fast)
            .is_some_and(|armed| armed.target == Some(prey))
    });
    assert!(acquired, "the attack-move should acquire the prey");
    assert_eq!(
        (
            simulation.state().contains(ids.formation_speed, slow),
            simulation.state().contains(ids.formation_speed, other_slow),
        ),
        (false, true)
    );
}
