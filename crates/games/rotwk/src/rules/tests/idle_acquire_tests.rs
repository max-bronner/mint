use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Spear\n  AttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Sentry\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Idler Sentry\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\nEnd\n\
ChildObject Planted Sentry\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 0\n  End\nEnd\n\
Weapon Sword\n  AttackRange = 20\n  DelayBetweenShots = 500\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
ChildObject Swordsman Sentry\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\nEnd\n\
Object Target\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const NEUTRAL: PlayerId = PlayerId::new(2);
const ALLY: PlayerId = PlayerId::new(3);
const CREEPS: PlayerId = PlayerId::new(4);

fn simulation(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Sentry", "Idler", "Planted", "Swordsman", "Target"].map(str::to_owned);
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
    let relations = vec![
        crate::alliance::MapRelations::default(),
        crate::alliance::MapRelations {
            allies: vec![PLAYER],
            ..crate::alliance::MapRelations::default()
        },
        crate::alliance::MapRelations {
            universal_hostile: true,
            ..crate::alliance::MapRelations::default()
        },
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            ["Neutral", "Ally", "Creeps"].map(str::to_owned).to_vec(),
            relations,
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

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

#[test]
fn an_idle_lone_unit_attacks_only_a_hostile_unit_in_reach_when_its_data_says_yes() {
    for (shooter, target_owner, expect_hit) in [
        ("Sentry", OTHER, true),
        ("Sentry", NEUTRAL, false),
        ("Sentry", ALLY, false),
        ("Idler", OTHER, false),
    ] {
        let (mut simulation, ids, entities) = simulation(&[
            (shooter, 300.0, 300.0, PLAYER),
            ("Target", 340.0, 300.0, target_owner),
        ]);
        for _ in 0..60 {
            simulation.tick().unwrap();
        }
        let hit = health(&simulation, ids, entities[1]) < 1000.0;
        assert_eq!(
            hit, expect_hit,
            "{shooter} against a target owned by {target_owner:?}"
        );
    }
}

#[test]
fn an_ordered_unit_does_not_acquire_until_it_is_idle_again() {
    let (mut simulation, ids, entities) = simulation(&[
        ("Sentry", 300.0, 300.0, PLAYER),
        ("Target", 340.0, 300.0, OTHER),
    ]);
    let (sentry, target) = (entities[0], entities[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![sentry],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 345.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let mut moving = 0;
    while simulation.state().contains(ids.path, sentry) {
        assert_eq!(
            health(&simulation, ids, target),
            1000.0,
            "struck while ordered"
        );
        simulation.tick().unwrap();
        moving += 1;
        assert!(moving < 300, "never arrived");
    }
    let stopped = position(&simulation, ids, sentry);
    assert!(
        stopped.minus(Vec2::new(300.0, 345.0)).length() < 5.0,
        "the route ended at {stopped:?}, short of where it was ordered"
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        health(&simulation, ids, target) < 1000.0,
        "idle again, yet never struck"
    );
}

fn pursuit(kind: &str, owner: PlayerId) -> (f32, Vec<u64>) {
    let (mut simulation, ids, entities) =
        simulation(&[(kind, 300.0, 300.0, owner), ("Target", 450.0, 300.0, OTHER)]);
    let mut hashes = Vec::new();
    for _ in 0..150 {
        simulation.tick().unwrap();
        hashes.push(simulation.state_hash());
    }
    (health(&simulation, ids, entities[1]), hashes)
}

#[test]
fn an_idle_unit_pursues_what_it_sees_unless_a_human_players_ranged_unit() {
    assert!(
        pursuit("Sentry", CREEPS).0 < 1000.0,
        "the creep never reached its target"
    );
    assert!(
        pursuit("Swordsman", PLAYER).0 < 1000.0,
        "the human player's melee unit never reached its target"
    );
    assert_eq!(
        pursuit("Sentry", PLAYER).0,
        1000.0,
        "the human player's ranged unit acquired beyond its weapon's range"
    );
}

#[test]
fn an_idle_pursuit_replays_to_the_same_hash_sequence() {
    assert_eq!(pursuit("Sentry", CREEPS).1, pursuit("Sentry", CREEPS).1);
}

#[test]
fn a_unit_whose_locomotor_speed_is_zero_never_chases_but_fires_in_reach() {
    let (mut far, ids, entities) = simulation(&[
        ("Planted", 300.0, 300.0, PLAYER),
        ("Target", 450.0, 300.0, OTHER),
    ]);
    let planted = entities[0];
    let start = position(&far, ids, planted);
    far.submit(
        PLAYER,
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![planted],
        },
    )
    .unwrap();
    far.submit(
        PLAYER,
        RotwkCommand::AttackObject {
            target: entities[1],
        },
    )
    .unwrap();
    for tick in 0..150 {
        far.tick().unwrap();
        assert!(
            !far.state().contains(ids.path, planted),
            "tick {tick}: a Speed = 0 unit holds a route"
        );
    }
    assert_eq!(position(&far, ids, planted), start);

    let (mut simulation, ids, entities) = simulation(&[
        ("Planted", 300.0, 300.0, PLAYER),
        ("Target", 340.0, 300.0, OTHER),
    ]);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        health(&simulation, ids, entities[1]) < 1000.0,
        "a foe in reach was never struck"
    );
}
