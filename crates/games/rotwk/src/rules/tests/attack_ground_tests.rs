use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Barrage\n  AttackRange = 150\n  DelayBetweenShots = 500\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Mortar\n  AttackRange = 150\n  MinimumAttackRange = 100\n  DelayBetweenShots = 500\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Volley\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
Object Bombardier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n    Weapon = TERTIARY Barrage\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Volleyer Bombardier\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n    Weapon = TERTIARY Volley\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\nEnd\n\
ChildObject Mortarman Bombardier\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Mortar\n    Weapon = TERTIARY Mortar\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\nEnd\n\
Weapon OneShotBombard\n  AttackRange = 150\n  DelayBetweenShots = 100\n  NoVictimNeeded = Yes\n  \
  IdleAfterFiringDelay = 100\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
ChildObject OneShotBombardier Bombardier\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n    Weapon = TERTIARY OneShotBombard\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\nEnd\n\
Object Standee\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[derive(Clone, Copy, Debug)]
enum Order {
    Ground,
    Object,
}

fn ordered(
    shooter: &str,
    place: Vec2,
    order: Order,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Bombardier",
        "Volleyer",
        "Mortarman",
        "OneShotBombardier",
        "Standee",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: Vec2, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn(shooter, Vec2::new(300.0, 300.0), PLAYER),
        spawn("Standee", place, OTHER),
    ];
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
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (shooter, standee) = (entities[0], entities[1]);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let command = match order {
        Order::Ground => RotwkCommand::AttackGround {
            slot: 2,
            x: place.x,
            y: place.y,
        },
        Order::Object => RotwkCommand::AttackObject { target: standee },
    };
    simulation.submit(PLAYER, command).unwrap();
    (simulation, ids, shooter, standee)
}

fn damage_taken(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    100_000.0
        - simulation
            .state()
            .get(ids.health, entity)
            .unwrap()
            .0
            .current()
}

#[test]
fn attack_ground_fires_a_no_victim_needed_slot_at_the_place() {
    let (mut simulation, ids, _, standee) =
        ordered("Bombardier", Vec2::new(400.0, 300.0), Order::Ground);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(damage_taken(&simulation, ids, standee) > 0.0);
}

#[test]
fn a_ground_attack_with_idle_after_firing_delay_fires_once_then_stops() {
    let (mut simulation, ids, _, standee) =
        ordered("OneShotBombardier", Vec2::new(400.0, 300.0), Order::Ground);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let after_one_shot = damage_taken(&simulation, ids, standee);
    assert!(after_one_shot > 0.0, "the first shot never landed");
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        damage_taken(&simulation, ids, standee),
        after_one_shot,
        "the slot fired again after IdleAfterFiringDelay"
    );
}

#[test]
fn attack_ground_with_a_weapon_that_needs_a_victim_is_refused() {
    let (mut simulation, ids, _, standee) =
        ordered("Volleyer", Vec2::new(400.0, 300.0), Order::Ground);
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert_eq!(damage_taken(&simulation, ids, standee), 0.0);
}

#[test]
fn attack_ground_out_of_range_walks_in_before_it_fires() {
    let (mut simulation, ids, shooter, standee) =
        ordered("Bombardier", Vec2::new(700.0, 300.0), Order::Ground);
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    let walked = simulation
        .state()
        .get(ids.body, shooter)
        .unwrap()
        .0
        .position
        .x;
    assert!(
        walked > 300.0 && damage_taken(&simulation, ids, standee) == 0.0,
        "after one second: at x {walked}, standee took {}",
        damage_taken(&simulation, ids, standee)
    );
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(damage_taken(&simulation, ids, standee) > 0.0);
}

fn shooter_at(simulation: &Simulation<RotwkRules>, ids: Ids, shooter: Entity) -> Vec2 {
    simulation
        .state()
        .get(ids.body, shooter)
        .unwrap()
        .0
        .position
}

fn shooter_x(simulation: &Simulation<RotwkRules>, ids: Ids, shooter: Entity) -> f32 {
    simulation
        .state()
        .get(ids.body, shooter)
        .unwrap()
        .0
        .position
        .x
}

#[test]
fn an_attacker_inside_its_minimum_range_backs_off_and_fires_only_from_there() {
    for order in [Order::Ground, Order::Object] {
        let (mut simulation, ids, shooter, standee) =
            ordered("Mortarman", Vec2::new(350.0, 300.0), order);
        let mut first_blow_from = None;
        for _ in 0..300 {
            simulation.tick().unwrap();
            if damage_taken(&simulation, ids, standee) > 0.0 {
                first_blow_from = Some(shooter_at(&simulation, ids, shooter));
                break;
            }
        }
        let from = first_blow_from.unwrap_or_else(|| {
            panic!(
                "{order:?}: never fired, at x {}",
                shooter_x(&simulation, ids, shooter)
            )
        });
        let distance = from.minus(Vec2::new(350.0, 300.0)).length();
        assert!(
            distance >= 102.5,
            "{order:?}: fired from {from:?}, {distance} away"
        );
    }
}

#[test]
fn an_attacker_outside_its_minimum_range_fires_without_moving() {
    for (shooter, place) in [("Mortarman", 420.0), ("Bombardier", 350.0)] {
        let (mut simulation, ids, entity, standee) =
            ordered(shooter, Vec2::new(place, 300.0), Order::Ground);
        for _ in 0..60 {
            simulation.tick().unwrap();
        }
        assert!(
            damage_taken(&simulation, ids, standee) > 0.0
                && shooter_x(&simulation, ids, entity) == 300.0,
            "{shooter}: at x {}, standee took {}",
            shooter_x(&simulation, ids, entity),
            damage_taken(&simulation, ids, standee)
        );
    }
}
