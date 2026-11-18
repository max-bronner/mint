use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Knife\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 100\n    DamageType = SLASH\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  WeaponSet\n    Conditions = CLOSE_RANGE\n    Weapon = PRIMARY Knife\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = DualWeaponBehavior Tag_Dual\n    \
    SwitchWeaponOnCloseRangeDistance = 40\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Blade\n  AttackRange = 20\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Chaser\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n    StopChaseDistance = 40\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Roamer Chaser\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\nEnd\n\
Object Target\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Tall Target\n  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  \
  GeometryHeight = 160\nEnd\n\
Weapon AimedBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  IsAimingWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Aimer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY AimedBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = AimWeaponBehavior Tag_Aim\n    AimHighThreshold = 0.15\n    \
    AimLowThreshold = -0.15\n    AimNearDistance = 40\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject CloseAimer Aimer\n  \
  Behavior = AimWeaponBehavior Tag_Aim\n    AimHighThreshold = 0.15\n    \
    AimLowThreshold = -0.15\n    AimNearDistance = 9\n  End\nEnd\n\
Weapon LeechSword\n  AttackRange = 20\n  LeechRangeWeapon = Yes\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 100\n    DamageType = SLASH\n  End\nEnd\n\
Object Leecher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY LeechSword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Rangefinder\n  AttackRange = 100\n  DelayBetweenShots = 30000\n  CanFireWhileCharging = Yes\n  \
  AcceptableAimDelta = 10\n  \
  RadiusDamageAffects = ENEMIES\n  \
  DamageNugget\n    Damage = 10\n    Radius = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
ChildObject Charger Chaser\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Rangefinder\n  End\nEnd\n\
Object Weakling\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const CREEPS: PlayerId = PlayerId::new(2);

fn simulation(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Archer",
        "Chaser",
        "Roamer",
        "Target",
        "Tall",
        "Aimer",
        "CloseAimer",
        "Leecher",
        "Weakling",
        "Charger",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = placed
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
    assert_eq!(entities.len(), placed.len());
    (simulation, ids, entities)
}

fn damage(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    2000.0
        - simulation
            .state()
            .get(ids.health, entity)
            .unwrap()
            .0
            .current()
}

#[test]
fn the_close_range_set_is_raised_inside_the_switch_distance_and_cleared_outside() {
    for (gap, close) in [(30.0, true), (80.0, false)] {
        let (mut simulation, ids, entities) = simulation(&[
            ("Archer", 300.0, 300.0, PLAYER),
            ("Target", 300.0 + gap, 300.0, OTHER),
        ]);
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        let dealt = damage(&simulation, ids, entities[1]);
        assert!(dealt > 0.0, "nothing was fired at a gap of {gap}");
        assert_eq!(
            dealt >= 100.0,
            close,
            "at a gap of {gap} the archer dealt {dealt}"
        );
    }
}

#[test]
fn close_range_follows_any_enemy_within_distance_not_only_the_order_target() {
    let (mut simulation, ids, entities) = simulation(&[
        ("Archer", 300.0, 300.0, PLAYER),
        ("Target", 300.0, 400.0, OTHER),
        ("Weakling", 300.0, 330.0, OTHER),
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
            RotwkCommand::AttackObject {
                target: entities[1],
            },
        )
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let state = simulation.rules().modules.as_ref().unwrap().state;
    let held: Vec<String> = simulation
        .state()
        .get(state, entities[0])
        .unwrap()
        .model_conditions
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert!(
        held.iter().any(|name| name == "WEAPONSTATE_CLOSE_RANGE"),
        "ordered onto a distant victim with a near bystander, the archer held {held:?}"
    );
    let dealt = damage(&simulation, ids, entities[1]);
    assert!(
        dealt >= 100.0,
        "the ordered victim took only {dealt}, not the knife's damage"
    );
}

#[test]
fn a_chase_fails_past_the_units_own_stop_chase_distance_where_it_stands() {
    for (chaser, owner, flees, reaches) in [
        ("Chaser", PLAYER, true, false),
        ("Chaser", PLAYER, false, true),
        ("Roamer", PLAYER, true, true),
        ("Chaser", CREEPS, true, true),
    ] {
        let (mut simulation, ids, entities) = simulation(&[
            (chaser, 300.0, 300.0, owner),
            ("Target", 600.0, 300.0, OTHER),
        ]);
        if flees {
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
                        x: 900.0,
                        y: 300.0,
                        queue: false,
                    },
                )
                .unwrap();
        }
        let start = position(&simulation, ids, entities[0]);
        let mut furthest: f32 = 0.0;
        for _ in 0..600 {
            simulation.tick().unwrap();
            let from_start = position(&simulation, ids, entities[0])
                .minus(start)
                .length();
            furthest = furthest.max(from_start);
        }
        let dealt = damage(&simulation, ids, entities[1]);
        assert_eq!(
            dealt > 0.0,
            reaches,
            "{chaser} of {owner:?}, its target fleeing {flees}, dealt {dealt}"
        );
        if !reaches {
            assert!(
                furthest < 5.0,
                "{chaser} walked {furthest} from where it started",
            );
        }
    }
}

#[test]
fn a_leech_range_weapon_walks_up_to_its_next_target_after_a_kill() {
    let (mut simulation, ids, entities) = simulation(&[
        ("Leecher", 300.0, 300.0, CREEPS),
        ("Weakling", 315.0, 300.0, OTHER),
        ("Target", 400.0, 300.0, OTHER),
    ]);
    let (leecher, next) = (entities[0], entities[2]);
    for _ in 0..600 {
        simulation.tick().unwrap();
        if damage(&simulation, ids, next) > 0.0 {
            let gap = position(&simulation, ids, leecher)
                .minus(position(&simulation, ids, next))
                .length();
            assert!(gap < 40.0, "struck the next target from {gap} away");
            return;
        }
    }
    panic!("the next target was never struck");
}

#[test]
fn an_aiming_weapon_puts_the_slopes_aim_condition_on_the_shooter() {
    for (target, expected) in [("Target", "AIM_STRAIGHT"), ("Tall", "AIM_HIGH")] {
        let (mut simulation, _ids, entities) = simulation(&[
            ("Aimer", 300.0, 300.0, PLAYER),
            (target, 400.0, 300.0, OTHER),
        ]);
        for _ in 0..40 {
            simulation.tick().unwrap();
        }
        let state = simulation.rules().modules.as_ref().unwrap().state;
        let held: Vec<String> = simulation
            .state()
            .get(state, entities[0])
            .unwrap()
            .model_conditions
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert!(
            held.iter().any(|name| name == expected),
            "firing at {target} the shooter held {held:?}"
        );
    }
}

#[test]
fn a_near_high_shot_sets_both_the_near_and_high_conditions() {
    let (mut simulation, _ids, entities) = simulation(&[
        ("Aimer", 300.0, 300.0, PLAYER),
        ("Tall", 300.0, 330.0, OTHER),
    ]);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let state = simulation.rules().modules.as_ref().unwrap().state;
    let held: Vec<String> = simulation
        .state()
        .get(state, entities[0])
        .unwrap()
        .model_conditions
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert!(
        held.iter().any(|name| name == "AIM_NEAR"),
        "a close, steep shot held {held:?}, missing AIM_NEAR"
    );
    assert!(
        held.iter().any(|name| name == "AIM_HIGH"),
        "a close, steep shot held {held:?}, missing AIM_HIGH"
    );
}

#[test]
fn the_near_far_axis_uses_the_same_floored_distance_as_the_slope() {
    let (mut simulation, ids, entities) = simulation(&[
        ("CloseAimer", 300.0, 300.0, PLAYER),
        ("Target", 303.0, 300.0, OTHER),
    ]);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let gap = position(&simulation, ids, entities[0])
        .minus(position(&simulation, ids, entities[1]))
        .length();
    assert!(
        (0.0..9.0).contains(&gap),
        "the settled gap {gap} is not under AimNearDistance (9), so this test cannot tell the \
         raw distance from the 10-unit floor"
    );
    let state = simulation.rules().modules.as_ref().unwrap().state;
    let held: Vec<String> = simulation
        .state()
        .get(state, entities[0])
        .unwrap()
        .model_conditions
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert!(
        !held.iter().any(|name| name == "AIM_NEAR"),
        "a {gap}-unit gap with AimNearDistance 9 held {held:?}, which should read as 10 floored, not near"
    );
}

#[test]
fn a_charging_weapon_fires_once_on_the_run_and_carries_through() {
    let (mut simulation, ids, entities) = simulation(&[
        ("Charger", 300.0, 300.0, PLAYER),
        ("Target", 500.0, 300.0, OTHER),
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
            RotwkCommand::AttackObject {
                target: entities[1],
            },
        )
        .unwrap();
    let mut furthest: f32 = 0.0;
    for _ in 0..300 {
        simulation.tick().unwrap();
        furthest = furthest.max(position(&simulation, ids, entities[0]).x);
    }
    assert_eq!(damage(&simulation, ids, entities[1]), 10.0, "not one shot");
    assert!(
        furthest > 520.0,
        "stopped at {furthest}, short of running past"
    );
}
