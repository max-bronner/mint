use super::*;

const DATA: &str = "\
Weapon ZToss\n  AttackRange = 300\n  DelayBetweenShots = 500\n  WeaponSpeed = 300\n  \
  ProjectileFilterInContainer = NONE +INFANTRY\n  \
  ProjectileNugget\n    WarheadTemplateName = ZTossHead\n  End\nEnd\n\
Weapon ZTossHead\n  DamageNugget\n    Damage = 10\n    DamageType = MAGIC\n  End\nEnd\n\
Object ZTosser\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZToss\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +INFANTRY +ROCK\n    Slots = 1\n    InitialPayload = ZImp 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object ZRockTosser\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZToss\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +INFANTRY +ROCK\n    Slots = 1\n    InitialPayload = ZStone 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object ZEmptyTosser\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY ZToss\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = TransportContain Tag_Hands\n    ObjectStatusOfContained = UNSELECTABLE\n    \
    PassengerFilter = ANY +INFANTRY +ROCK\n    Slots = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object ZImp\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 12\nEnd\n\
Object ZStone\n  Side = Wild\n  KindOf = ROCK\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 12\nEnd\n\
Object ZTarget\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const TOSSER_AT: Vec2 = Vec2::new(300.0, 300.0);
const TARGET_AT: Vec2 = Vec2::new(450.0, 300.0);

fn scene(tosser: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [tosser.to_owned(), "ZTarget".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [(tosser, TOSSER_AT, OTHER), ("ZTarget", TARGET_AT, PLAYER)]
        .into_iter()
        .map(|(name, at, player)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    player,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities[0], entities[1])
}

fn passengers(simulation: &Simulation<RotwkRules>, ids: Ids, tosser: Entity) -> Vec<Entity> {
    simulation
        .state()
        .get(ids.passengers, tosser)
        .map(|list| list.0.clone())
        .unwrap_or_default()
}

fn carried(simulation: &Simulation<RotwkRules>, ids: Ids) -> Vec<Option<Entity>> {
    simulation
        .state()
        .store(ids.flight)
        .map(|store| store.iter().map(|(_, flight)| flight.carried).collect())
        .unwrap_or_default()
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
fn a_filtered_weapon_throws_its_admitted_passenger_as_the_projectile() {
    let (mut simulation, ids, tosser, _) = scene("ZTosser");
    let [imp] = passengers(&simulation, ids, tosser)[..] else {
        panic!("the payload never entered");
    };
    let mut rode = false;
    for _ in 0..90 {
        simulation.tick().unwrap();
        rode |= carried(&simulation, ids).contains(&Some(imp));
    }
    assert!(rode, "the passenger never rode a flight");
    assert!(passengers(&simulation, ids, tosser).is_empty());
    assert!(!simulation.state().contains(ids.contained, imp));
}

#[test]
fn a_thrown_passenger_lands_as_its_warheads_victim() {
    let (mut simulation, ids, tosser, target) = scene("ZTosser");
    let [imp] = passengers(&simulation, ids, tosser)[..] else {
        panic!("the payload never entered");
    };
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(carried(&simulation, ids).is_empty(), "still in the air");
    assert_eq!(health(&simulation, ids, imp), 490.0);
    assert_eq!(health(&simulation, ids, target), 4000.0);
    assert!(simulation.state().get(ids.ground_offset, imp).is_none());
}

#[test]
fn nothing_is_thrown_without_a_passenger_the_filter_admits() {
    for tosser in ["ZRockTosser", "ZEmptyTosser"] {
        let (mut simulation, ids, tosser_entity, target) = scene(tosser);
        let aboard = passengers(&simulation, ids, tosser_entity);
        for _ in 0..90 {
            simulation.tick().unwrap();
            assert!(carried(&simulation, ids).is_empty(), "{tosser} fired");
        }
        assert_eq!(
            passengers(&simulation, ids, tosser_entity),
            aboard,
            "{tosser}"
        );
        assert_eq!(health(&simulation, ids, target), 4000.0, "{tosser}");
    }
}
