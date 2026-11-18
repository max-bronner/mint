use super::*;

const DATA: &str = "\
Object ZCritter\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Behavior = AnimalAIUpdate Tag_AI\n    WanderPercentage = 100\n    \
    MaxWanderDistance = 20\n    MaxWanderRadius = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(data: &str) -> (Simulation<RotwkRules>, Ids, Entity) {
    scene_on(data, &flat_map(96, 96))
}

fn scene_on(data: &str, map: &HeightMapData) -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZCritter".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [("ZCritter", Vec2::new(300.0, 300.0), PLAYER)]
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
    let passability = Passability::build(map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(1)
        .with_map_players(
            1,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        );
    let simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec()[0];
    (simulation, ids, entity)
}

fn run(simulation: &mut Simulation<RotwkRules>, frames: u32) {
    for _ in 0..frames {
        simulation.tick().unwrap();
    }
}

#[test]
fn an_idle_animal_with_full_wander_percentage_moves_from_its_spawn_point() {
    let (mut simulation, ids, critter) = scene(DATA);
    let origin = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    run(&mut simulation, 90);
    let moved = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    assert!(
        moved.minus(origin).length() > 0.5,
        "expected the animal to have wandered from {origin:?}, still at {moved:?}"
    );
}

#[test]
fn an_animal_past_its_wander_radius_walks_back_toward_its_origin() {
    let (mut simulation, ids, critter) = scene(DATA);
    let origin = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    run(&mut simulation, 400);
    let last = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    assert!(
        last.minus(origin).length() <= 30.0 + 20.0 + 1.0,
        "expected the animal within MaxWanderRadius + MaxWanderDistance of \
         {origin:?}, found it at {last:?} ({} away)",
        last.minus(origin).length()
    );
}

#[test]
fn an_animal_wanders_on_after_finishing_a_route_round_a_cliff() {
    const ROAMER: &str = "\
Object ZCritter\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Behavior = AnimalAIUpdate Tag_AI\n    WanderPercentage = 100\n    \
    MaxWanderDistance = 60\n    MaxWanderRadius = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";
    let (mut simulation, ids, critter) = scene_on(ROAMER, &wall_map(96, 96, 33, 50..53));
    let mut finished = None;
    let mut cursor = 0;
    for _ in 0..120 * SIMULATION_HZ {
        simulation.tick().unwrap();
        let world = simulation.state();
        let body = world.get(ids.body, critter).unwrap().0;
        if world.contains(ids.path, critter) {
            cursor = cursor.max(body.waypoint);
        } else if cursor >= 2 {
            finished = Some(body.position);
            break;
        } else {
            cursor = 0;
        }
    }
    let finished = finished.expect("no wander walk was ever re-planned round the cliff");
    run(&mut simulation, 90);
    let later = simulation
        .state()
        .get(ids.body, critter)
        .unwrap()
        .0
        .position;
    assert!(
        later.minus(finished).length() > 0.5,
        "the critter stayed where its re-planned route ended, {finished:?}"
    );
}
