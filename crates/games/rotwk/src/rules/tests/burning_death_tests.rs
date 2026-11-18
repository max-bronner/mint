use super::*;

const DATA: &str = "\
Weapon FireBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = FLAME\n    DeathType = BURNED\n  End\nEnd\n\
Weapon PlainBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon LogicalFireBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = LOGICAL_FIRE\n  End\nEnd\n\
Weapon PierceButNamedBurnedBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = PIERCE\n    DeathType = BURNED\n  End\nEnd\n\
Object FireArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY FireBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PlainBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object LogicalFireArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY LogicalFireBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PierceButNamedBurnedArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PierceButNamedBurnedBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor BurningRunLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\n  BurningDeathRadius = 60\nEnd\n\
Object Burner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n    BurningDeathBehavior = Yes\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    BurningDeathTime = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = BurningRunLocomotor\n    Condition = SET_BURNINGDEATH\n    \
    Speed = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StillBurner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n    BurningDeathBehavior = Yes\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    BurningDeathTime = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Faller\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    BurningDeathTime = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const BURNING_RADIUS: f32 = 60.0;

fn data() -> String {
    DATA.to_owned()
}

fn simulation(shooter: &str, victim: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data(), &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names: Vec<String> = [
        "FireArcher",
        "PlainArcher",
        "LogicalFireArcher",
        "PierceButNamedBurnedArcher",
        "Burner",
        "StillBurner",
        "Faller",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [(shooter, 300.0, PLAYER), (victim, 340.0, OTHER)]
        .iter()
        .map(|(name, x, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, 300.0),
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
    (simulation, ids, entities[1])
}

fn tick_until_killed(simulation: &mut Simulation<RotwkRules>, ids: Ids, victim: Entity) -> u32 {
    for ticks in 1..=300 {
        simulation.tick().unwrap();
        let dead = simulation
            .state()
            .get(ids.health, victim)
            .is_none_or(|health| health.0.is_dead());
        if dead {
            return ticks;
        }
    }
    panic!("the victim was never killed");
}

fn has_died(simulation: &Simulation<RotwkRules>, ids: Ids, victim: Entity) -> bool {
    !simulation.state().is_live(victim) || simulation.state().contains(ids.doomed, victim)
}

#[test]
fn a_burned_kill_of_a_burning_body_burns_for_its_time_then_dies() {
    let (mut simulation, ids, victim) = simulation("FireArcher", "Burner");
    tick_until_killed(&mut simulation, ids, victim);
    for _ in 0..28 {
        simulation.tick().unwrap();
    }
    assert!(
        !has_died(&simulation, ids, victim),
        "died before its BurningDeathTime was out"
    );
    for _ in 0..4 {
        simulation.tick().unwrap();
    }
    assert!(
        has_died(&simulation, ids, victim),
        "still burning after its BurningDeathTime"
    );
}

#[test]
fn only_a_flame_or_logical_fire_blow_on_a_flagged_body_delays_the_death() {
    for (shooter, victim) in [
        ("PlainArcher", "Burner"),
        ("FireArcher", "Faller"),
        ("PierceButNamedBurnedArcher", "Burner"),
    ] {
        let (mut simulation, ids, entity) = simulation(shooter, victim);
        tick_until_killed(&mut simulation, ids, entity);
        assert!(
            has_died(&simulation, ids, entity),
            "{shooter} killing {victim} did not die at once"
        );
    }
}

#[test]
fn a_lethal_logical_fire_blow_burns_a_flagged_body() {
    let (mut simulation, ids, victim) = simulation("LogicalFireArcher", "Burner");
    tick_until_killed(&mut simulation, ids, victim);
    assert!(
        !has_died(&simulation, ids, victim),
        "died at once on a lethal LOGICAL_FIRE blow"
    );
}

#[test]
fn a_burning_unit_with_a_burning_locomotor_moves_within_the_radius_and_never_beyond_it() {
    let (mut simulation, ids, victim) = simulation("FireArcher", "Burner");
    tick_until_killed(&mut simulation, ids, victim);
    let origin = simulation.state().get(ids.body, victim).unwrap().0.position;
    let mut moved = false;
    for _ in 0..30 {
        simulation.tick().unwrap();
        let Some(body) = simulation.state().get(ids.body, victim) else {
            break;
        };
        let distance = body.0.position.minus(origin).length();
        assert!(
            distance <= BURNING_RADIUS + 20.0,
            "ran {distance} beyond its BurningDeathRadius of {BURNING_RADIUS}"
        );
        moved = moved || distance > 1.0;
    }
    assert!(moved, "never left where it caught fire");
}

#[test]
fn a_burning_unit_with_no_burning_locomotor_stays_put() {
    let (mut simulation, ids, victim) = simulation("FireArcher", "StillBurner");
    tick_until_killed(&mut simulation, ids, victim);
    let origin = simulation.state().get(ids.body, victim).unwrap().0.position;
    for _ in 0..30 {
        simulation.tick().unwrap();
        let Some(body) = simulation.state().get(ids.body, victim) else {
            break;
        };
        assert!(
            body.0.position.minus(origin).length() < 0.5,
            "moved without a SET_BURNINGDEATH locomotor"
        );
    }
}
