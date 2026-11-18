use super::*;

const DATA: &str = "\
Object ZMill\n  Side = Wild\n  KindOf = STRUCTURE SUPPLY_GATHERING_CENTER SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 40\nEnd\n\
Object ZWorker\n  Side = Wild\n  KindOf = SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Behavior = WorkerAIUpdate Tag_AI\n    MaxBoxes = 2\n    SupplyWarehouseScanDistance = 400\n    \
    HarvestTrees = Yes\n    HarvestActivationRange = 20\n    HarvestPreparationTime = 100\n    \
    HarvestActionTime = 100\n    SupplyCenterActionDelay = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZTree\n  Side = Wild\n  KindOf = TREE SHRUBBERY IMMOBILE SELECTABLE\n  IsHarvestable = Yes\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn scene(supply: u32) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    scene_with(DATA, Vec2::new(310.0, 300.0), supply)
}

fn scene_with(
    data: &str,
    tree: Vec2,
    supply: u32,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["ZMill".to_owned(), "ZWorker".to_owned(), "ZTree".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = [
        ("ZMill", Vec2::new(295.0, 300.0), PLAYER),
        ("ZWorker", Vec2::new(300.0, 300.0), PLAYER),
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
    let forest_seeds = vec![crate::forest::ForestTreeInit {
        pos: tree,
        kind: kinds.kind("ZTree").unwrap(),
        grabbable: false,
        harvestable: true,
        supply,
        tainted: false,
        source_index: 0,
    }];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_economy(Vec::new(), 0)
        .with_players(2)
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_forest_seeds(forest_seeds)
        .with_value_per_supply_box(10.0);
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

fn run(simulation: &mut Simulation<RotwkRules>, frames: u32) {
    for _ in 0..frames {
        simulation.tick().unwrap();
    }
}

fn purse_of(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    let economy = RotwkRules::economy_ids(ids);
    crate::economy::player_entity(simulation.state(), economy, player)
        .and_then(|entity| simulation.state().get(ids.purse, entity))
        .map_or(0, |purse| purse.0.amount())
}

#[test]
fn an_idle_harvester_chops_a_tree_and_banks_a_box_with_no_order() {
    let (mut simulation, ids, _mill, _worker) = scene(90);
    run(&mut simulation, 24);
    assert!(
        purse_of(&simulation, ids, PLAYER) > 0,
        "a load should have been banked within 24 frames"
    );
}

#[test]
fn harvesting_an_almost_empty_tree_sinks_it_and_still_banks_what_it_held() {
    let (mut simulation, ids, _mill, _worker) = scene(1);
    run(&mut simulation, 24);
    let forest_entity =
        crate::forest::singleton_of(simulation.state(), ids.forest).expect("forest singleton");
    let forest = simulation.state().get(ids.forest, forest_entity).unwrap();
    assert_eq!(forest.len(), 0, "the one-box tree should have sunk");
    assert!(purse_of(&simulation, ids, PLAYER) > 0);
}

#[test]
fn value_multiplier_scales_the_deposit() {
    let data = DATA.replace(
        "KindOf = STRUCTURE SUPPLY_GATHERING_CENTER SELECTABLE\n",
        "KindOf = STRUCTURE SUPPLY_GATHERING_CENTER SELECTABLE\n  \
         Behavior = SupplyCenterDockUpdate Tag_Dock\n    ValueMultiplier = 150%\n  End\n",
    );
    let (mut simulation, ids, _mill, _worker) = scene_with(&data, Vec2::new(310.0, 300.0), 90);
    run(&mut simulation, 24);
    assert_eq!(purse_of(&simulation, ids, PLAYER), 30);
}

#[test]
fn a_harvester_whose_walk_ends_out_of_activation_range_still_chops_and_banks() {
    let data = DATA.replace("HarvestActivationRange = 20", "HarvestActivationRange = 3");
    let (mut simulation, ids, _mill, _worker) = scene_with(&data, Vec2::new(350.0, 300.0), 90);
    run(&mut simulation, 900);
    assert!(
        purse_of(&simulation, ids, PLAYER) > 0,
        "a load should have been banked within 900 frames"
    );
}
