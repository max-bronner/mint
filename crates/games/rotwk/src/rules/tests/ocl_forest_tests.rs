use super::*;

fn source(clear_removables: &str) -> String {
    format!(
        "\
ObjectCreationList OCL_Clear\n  CreateObject\n    ObjectNames = Clearer\n    \
  Disposition = LIKE_EXISTING\n    {clear_removables}\n  End\nEnd\n\
Object Egg\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 0\n    OCL = INITIAL OCL_Clear\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Clearer\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object ZTree\n  Side = Wild\n  KindOf = TREE SHRUBBERY IMMOBILE SELECTABLE\n  IsHarvestable = Yes\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n"
    )
}

#[test]
fn an_ocl_creation_clears_the_tree_under_its_footprint() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &source("ClearRemovables = Yes"),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Egg".to_owned(), "ZTree".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let egg_pos = Vec2::new(300.0, 300.0);
    let spawns = [(("Egg"), egg_pos, PLAYER)]
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
        pos: Vec2::new(egg_pos.x + 10.0, egg_pos.y),
        kind: kinds.kind("ZTree").unwrap(),
        grabbable: false,
        harvestable: true,
        supply: 90,
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
        .with_creation_lists(Arc::clone(&data.ocls));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let forest_entity =
        crate::forest::singleton_of(simulation.state(), ids.forest).expect("forest singleton");
    assert_eq!(
        simulation
            .state()
            .get(ids.forest, forest_entity)
            .unwrap()
            .len(),
        1,
        "the tree should be seeded before the egg dies"
    );
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let forest = simulation.state().get(ids.forest, forest_entity).unwrap();
    assert_eq!(
        forest.len(),
        0,
        "the OCL creation should have cleared the tree"
    );
}

#[test]
fn an_ocl_creation_without_clear_removables_leaves_the_tree_standing() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &source(""),
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Egg".to_owned(), "ZTree".to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let egg_pos = Vec2::new(300.0, 300.0);
    let spawns = [(("Egg"), egg_pos, PLAYER)]
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
        pos: Vec2::new(egg_pos.x + 10.0, egg_pos.y),
        kind: kinds.kind("ZTree").unwrap(),
        grabbable: false,
        harvestable: true,
        supply: 90,
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
        .with_creation_lists(Arc::clone(&data.ocls));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let forest_entity =
        crate::forest::singleton_of(simulation.state(), ids.forest).expect("forest singleton");
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let forest = simulation.state().get(ids.forest, forest_entity).unwrap();
    assert_eq!(
        forest.len(),
        1,
        "without ClearRemovables the tree should still stand"
    );
}
