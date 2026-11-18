use super::*;

const DATA: &str = "\
Weapon DodgeBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  HitPercentage = 100\n  ScatterRadius = 16\n  CanBeDodged = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon PlainBow\n  AttackRange = 120\n  DelayBetweenShots = 500\n  \
  HitPercentage = 100\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY DodgeBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PlainBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dodger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n    DodgePercent = 100%\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Standee\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn simulation(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Archer", "PlainArcher", "Dodger", "Standee"].map(str::to_owned);
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

fn damage_dealt(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity, max: f32) -> f32 {
    max - simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

#[test]
fn a_full_dodge_percent_stops_a_dodgeable_weapon_and_not_a_plain_one() {
    let (mut dodgeable, ids, entities) = simulation(&[
        ("Archer", 300.0, 300.0, PLAYER),
        ("Dodger", 340.0, 300.0, OTHER),
    ]);
    for _ in 0..90 {
        dodgeable.tick().unwrap();
    }
    assert_eq!(
        damage_dealt(&dodgeable, ids, entities[1], 100000.0),
        0.0,
        "a 100% dodge let a CanBeDodged shot through"
    );

    let (mut plain, ids, entities) = simulation(&[
        ("PlainArcher", 300.0, 300.0, PLAYER),
        ("Dodger", 340.0, 300.0, OTHER),
    ]);
    for _ in 0..90 {
        plain.tick().unwrap();
    }
    assert!(
        damage_dealt(&plain, ids, entities[1], 100000.0) > 0.0,
        "a weapon without CanBeDodged must ignore the target's DodgePercent"
    );
}

#[test]
fn an_absent_dodge_percent_never_dodges() {
    let (mut simulation, ids, entities) = simulation(&[
        ("Archer", 300.0, 300.0, PLAYER),
        ("Standee", 340.0, 300.0, OTHER),
    ]);
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        damage_dealt(&simulation, ids, entities[1], 100000.0) > 0.0,
        "a victim with no DodgePercent must always take a struck, CanBeDodged shot"
    );
}
