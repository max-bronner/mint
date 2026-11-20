use super::fighting_b4_tests::{attack, scene};
use super::*;

const DATA: &str = "\
Armor NoArmor\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Pea\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Gunner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Pea\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = NoArmor\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn arena() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let (simulation, ids, e) = scene(
        DATA,
        &[
            ("Gunner", Vec2::new(100.0, 100.0), PLAYER),
            ("Dummy", Vec2::new(160.0, 100.0), OTHER),
        ],
    );
    (simulation, ids, e[0], e[1])
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |pool| pool.0.current())
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

#[test]
fn hold_fire_stops_automatic_acquisition_and_an_ordered_attack_still_fires() {
    let (mut free, ids, _, foe) = arena();
    run(&mut free, 120);
    assert!(
        health(&free, ids, foe) < 100_000.0,
        "without hold fire the gunner acquires the dummy"
    );

    let (mut held, ids, gunner, foe) = arena();
    held.submit(
        PLAYER,
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![gunner],
        },
    )
    .unwrap();
    held.submit(PLAYER, RotwkCommand::ToggleHoldFire { lead: Some(gunner) })
        .unwrap();
    run(&mut held, 120);
    assert_eq!(health(&held, ids, foe), 100_000.0, "idle, it picks nothing");
    attack(&mut held, gunner, foe);
    run(&mut held, 60);
    assert!(health(&held, ids, foe) < 100_000.0, "ordered, it fires");
}
