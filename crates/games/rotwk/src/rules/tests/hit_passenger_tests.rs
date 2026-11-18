use super::*;

const DATA: &str = "\
Weapon PassBow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  HitPercentage = 100\n  HitPassengerPercentage = 100%\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PASSENGER\n  End\nEnd\n\
Weapon SlashBow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  HitPercentage = 100\n  HitPassengerPercentage = 100%\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object SlashArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY SlashBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon PlainBow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  HitPercentage = 100\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PASSENGER\n  End\nEnd\n\
Weapon LaunchBow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  HitPassengerPercentage = 100%\n  \
  ProjectileNugget\n    WarheadTemplateName = PlainHead\n  End\nEnd\n\
Weapon PlainLaunchBow\n  AttackRange = 300\n  DelayBetweenShots = 500\n  WeaponSpeed = 600\n  \
  HitPercentage = 100\n  \
  ProjectileNugget\n    WarheadTemplateName = PlainHead\n  End\nEnd\n\
Weapon PlainHead\n  DamageNugget\n    Damage = 10\n    DamageType = PASSENGER\n  End\nEnd\n\
Object LaunchArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY LaunchBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainLaunchArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PlainLaunchBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PassArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PassBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PlainArcher\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY PlainBow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    MoodAttackCheckRate = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PassRider\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PassBoat\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Boat\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    Slots = 1\n    \
    PassengerFilter = NONE +PassRider\n    InitialPayload = PassRider 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 60\nEnd\n\
Object EmptyBoat\n  Side = Wild\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeTransportContain Tag_Boat\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    Slots = 1\n    \
    PassengerFilter = NONE +PassRider\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 60\nEnd\n";

const ARCHER_AT: Vec2 = Vec2::new(300.0, 300.0);
const BOAT_AT: Vec2 = Vec2::new(500.0, 300.0);

fn scene(archer: &str, container: &str) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [archer.to_owned(), container.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [(archer, ARCHER_AT, PLAYER), (container, BOAT_AT, OTHER)]
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
        .with_players(2);
    let simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let boat = entities
        .iter()
        .copied()
        .find(|e| *e != entities[0])
        .unwrap();
    (simulation, ids, entities[0], boat)
}

fn first_passenger(simulation: &Simulation<RotwkRules>, ids: Ids, container: Entity) -> Entity {
    simulation
        .state()
        .get(ids.passengers, container)
        .and_then(|list| list.0.first().copied())
        .unwrap()
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn attack(simulation: &mut Simulation<RotwkRules>, archer: Entity, target: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![archer],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
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
fn a_full_hit_passenger_chance_strikes_the_passenger_instead_of_the_container() {
    let (mut simulation, ids, archer, boat) = scene("PassArcher", "PassBoat");
    run(&mut simulation, 1);
    let rider = first_passenger(&simulation, ids, boat);
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert_eq!(
        damage_dealt(&simulation, ids, boat, 1000.0),
        0.0,
        "the container is not fired at"
    );
    assert!(
        damage_dealt(&simulation, ids, rider, 500.0) > 0.0,
        "a 100% HitPassengerPercentage weapon must strike the passenger"
    );
}

#[test]
fn a_contained_victim_ignores_blows_that_are_not_passenger_typed() {
    let (mut simulation, ids, archer, boat) = scene("SlashArcher", "PassBoat");
    run(&mut simulation, 1);
    let rider = first_passenger(&simulation, ids, boat);
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert_eq!(damage_dealt(&simulation, ids, rider, 500.0), 0.0);
    assert_eq!(damage_dealt(&simulation, ids, boat, 1000.0), 0.0);
}

#[test]
fn no_hit_passenger_chance_never_hurts_the_passenger() {
    let (mut simulation, ids, archer, boat) = scene("PlainArcher", "PassBoat");
    run(&mut simulation, 1);
    let rider = first_passenger(&simulation, ids, boat);
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert!(
        damage_dealt(&simulation, ids, boat, 1000.0) > 0.0,
        "the plain weapon must still hit the container"
    );
    assert_eq!(
        damage_dealt(&simulation, ids, rider, 500.0),
        0.0,
        "a weapon without HitPassengerPercentage must never hurt a passenger"
    );
}

#[test]
fn a_container_with_no_passengers_is_unaffected() {
    let (mut simulation, ids, archer, boat) = scene("PassArcher", "EmptyBoat");
    assert!(simulation.state().get(ids.passengers, boat).is_none());
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert!(
        damage_dealt(&simulation, ids, boat, 1000.0) > 0.0,
        "the container must still take damage with nobody aboard"
    );
}

#[test]
fn a_launchers_hit_passenger_chance_carries_to_its_warheads_landing() {
    let (mut simulation, ids, archer, boat) = scene("LaunchArcher", "PassBoat");
    run(&mut simulation, 1);
    let rider = first_passenger(&simulation, ids, boat);
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert_eq!(damage_dealt(&simulation, ids, boat, 1000.0), 0.0);
    assert!(damage_dealt(&simulation, ids, rider, 500.0) > 0.0);
}

#[test]
fn a_launcher_without_hit_passenger_chance_never_reaches_the_passenger() {
    let (mut simulation, ids, archer, boat) = scene("PlainLaunchArcher", "PassBoat");
    run(&mut simulation, 1);
    let rider = first_passenger(&simulation, ids, boat);
    attack(&mut simulation, archer, boat);
    run(&mut simulation, 90);
    assert!(damage_dealt(&simulation, ids, boat, 1000.0) > 0.0);
    assert_eq!(damage_dealt(&simulation, ids, rider, 500.0), 0.0);
}
