use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Weapon Bow\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Sling\n  AttackRange = 50\n  DelayBetweenShots = 500\n  AcceptableAimDelta = 180\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Slinger Archer\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sling\n  End\nEnd\n\
Weapon Arc\n  AttackRange = 50\n  DelayBetweenShots = 500\n  AcceptableAimDelta = 20\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Tower\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Arc\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor Wings\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  \
  MinSpeed = 30\nEnd\n\
Weapon Runbow\n  AttackRange = 50\n  DelayBetweenShots = 500\n  CanFireWhileMoving = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Flyer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Runbow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Wings\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Drawbow\n  AttackRange = 150\n  DelayBetweenShots = 500\n  AcceptableAimDelta = 30\n  \
  PreAttackDelay = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
ChildObject Drawer Archer\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Drawbow\n  End\nEnd\n\
Object Target\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn duel(
    shooter: &str,
    target_x: f32,
    target_y: f32,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Archer", "Slinger", "Tower", "Flyer", "Drawer", "Target"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [
        (shooter, 300.0, 300.0, PLAYER),
        ("Target", target_x, target_y, OTHER),
    ]
    .iter()
    .map(|(name, x, y, owner)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(*x, *y),
                Facing::EAST,
                *owner,
            )
            .unwrap()
    })
    .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities[0], entities[1])
}

fn struck(simulation: &Simulation<RotwkRules>, ids: Ids, target: Entity) -> bool {
    simulation
        .state()
        .get(ids.health, target)
        .unwrap()
        .0
        .current()
        < 1000.0
}

fn facing(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.body, entity)
        .unwrap()
        .0
        .facing
        .radians()
}

#[test]
fn an_attacker_turns_to_its_victim_before_it_shoots() {
    let off = Facing::from_radians(pose_at_first_strike("Archer", 300.0).0)
        .delta_to(Facing::from_radians(core::f32::consts::PI))
        .abs();
    assert!(off < 0.035, "shot {off} radians off its victim");
    assert_eq!(
        pose_at_first_strike("Slinger", 320.0).0,
        0.0,
        "an all-round weapon turned"
    );
}

fn pose_at_first_strike(shooter: &str, target_y: f32) -> (f32, Vec2) {
    let (mut simulation, ids, shooter, target) = duel(shooter, 260.0, target_y);
    for _ in 0..60 {
        simulation.tick().unwrap();
        if struck(&simulation, ids, target) {
            let body = simulation.state().get(ids.body, shooter).unwrap().0;
            return (body.facing.radians(), body.position);
        }
    }
    panic!("never shot");
}

#[test]
fn an_immobile_attacker_fires_only_within_its_aim_delta() {
    let (mut behind, ids, tower, target) = duel("Tower", 260.0, 300.0);
    for _ in 0..60 {
        behind.tick().unwrap();
    }
    assert!(!struck(&behind, ids, target), "fired outside its arc");
    assert_eq!(
        facing(&behind, ids, tower),
        0.0,
        "an immobile attacker turned"
    );

    let (mut ahead, ids, _, target) = duel("Tower", 340.0, 310.0);
    for _ in 0..60 {
        ahead.tick().unwrap();
    }
    assert!(struck(&ahead, ids, target), "held its shot inside its arc");
}

#[test]
fn an_attacker_that_cannot_stop_steers_at_its_victim_rather_than_pivoting() {
    let start = Vec2::new(300.0, 300.0);
    let (_, pivoted) = pose_at_first_strike("Archer", 300.0);
    assert!(pivoted.minus(start).length() < 0.1, "the archer walked");
    let (_, steered) = pose_at_first_strike("Flyer", 300.0);
    let moved = steered.minus(start).length();
    assert!(moved > 0.5, "moved only {moved} while aiming");
}

#[test]
fn the_body_keeps_turning_onto_the_victim_through_the_wind_up() {
    let bearing = 20.0f32.to_radians();
    let (sin, cos) = mint_math::f32::sin_cos(bearing);
    let (mut simulation, ids, shooter, target) =
        duel("Drawer", 300.0 + 100.0 * cos, 300.0 + 100.0 * sin);
    for _ in 0..90 {
        simulation.tick().unwrap();
        if struck(&simulation, ids, target) {
            let off = Facing::from_radians(facing(&simulation, ids, shooter))
                .delta_to(Facing::from_radians(bearing))
                .abs();
            assert!(off < 0.035, "shot {off} radians off its victim");
            return;
        }
    }
    panic!("never shot");
}
