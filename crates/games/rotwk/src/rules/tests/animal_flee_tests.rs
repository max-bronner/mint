use super::*;

fn data(flee_distance: u32) -> String {
    format!(
        "Object ZCritter\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
           Behavior = AnimalAIUpdate Tag_AI\n    WanderPercentage = 0\n    \
             FleeRange = 100\n    FleeDistance = {flee_distance}\n    UpdateTimer = 1\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
         Object Hunter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
           Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
           Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

fn scene_with(flee_distance: u32) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let data = data(flee_distance);
    let doc =
        mint_sage_ini::Document::parse("t.ini", &data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZCritter".to_owned(), "Hunter".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZCritter", Vec2::new(300.0, 300.0), PLAYER),
        ("Hunter", Vec2::new(330.0, 300.0), OTHER),
    ]
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
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(2);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let critter = kinds.kind("ZCritter").unwrap();
    let hunter = kinds.kind("Hunter").unwrap();
    let find = |kind: crate::unit::Kind| {
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| **k == kind)
            .unwrap()
            .0
    };
    let (found_critter, found_hunter) = (find(critter), find(hunter));
    (simulation, ids, found_critter, found_hunter)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

const LICK_DATA: &str = "\
Object ZCritter\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Behavior = AnimalAIUpdate Tag_AI\n    WanderPercentage = 100\n    \
    MaxWanderDistance = 5\n    MaxWanderRadius = 10\n    FleeRange = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SaltLick\n  Side = Neutral\n  KindOf = SALT_LICK IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn lick_scene() -> (Simulation<RotwkRules>, Ids, Entity, Vec2) {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        LICK_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZCritter".to_owned(), "SaltLick".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let lick_at = Vec2::new(350.0, 300.0);
    let spawns = [
        ("ZCritter", Vec2::new(300.0, 300.0), PLAYER),
        ("SaltLick", lick_at, PLAYER),
    ]
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
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(1);
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let critter = kinds.kind("ZCritter").unwrap();
    let found = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(_, k)| **k == critter)
        .unwrap()
        .0;
    (simulation, ids, found, lick_at)
}

#[test]
fn a_salt_lick_past_the_wander_radius_pulls_the_animal_to_it() {
    let (mut simulation, ids, critter, lick_at) = lick_scene();
    run(&mut simulation, 200);
    let now = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    assert!(
        now.minus(lick_at).length() < 15.0,
        "expected the animal to have closed on the salt lick at {lick_at:?}, found it at {now:?}"
    );
}

#[test]
fn a_nearby_non_allied_infantry_sends_the_animal_fleeing() {
    let (mut simulation, ids, critter, hunter) = scene_with(50);
    let hunter_at = simulation.state().get(ids.body, hunter).unwrap().0.position;
    let start = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    run(&mut simulation, 60);
    let now = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    assert!(
        now.minus(hunter_at).length() > start.minus(hunter_at).length() + 5.0,
        "expected the animal to have moved away from the hunter at {hunter_at:?}; started at \
         {start:?}, now at {now:?}"
    );
}

#[test]
fn a_higher_flee_distance_sends_the_animal_farther_from_its_scarer() {
    let far = {
        let (mut simulation, ids, critter, hunter) = scene_with(400);
        let hunter_at = simulation.state().get(ids.body, hunter).unwrap().0.position;
        run(&mut simulation, 90);
        simulation
            .state()
            .get(ids.body, critter)
            .unwrap()
            .0
            .position
            .minus(hunter_at)
            .length()
    };
    let near = {
        let (mut simulation, ids, critter, hunter) = scene_with(0);
        let hunter_at = simulation.state().get(ids.body, hunter).unwrap().0.position;
        run(&mut simulation, 90);
        simulation
            .state()
            .get(ids.body, critter)
            .unwrap()
            .0
            .position
            .minus(hunter_at)
            .length()
    };
    assert!(
        far > near + 10.0,
        "FleeDistance 400 ({far}) must send the animal farther than FleeDistance 0 ({near})"
    );
}
