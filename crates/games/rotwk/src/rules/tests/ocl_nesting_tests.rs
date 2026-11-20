use super::*;

const SOURCE: &str = "Upgrade Upgrade_Loop
  Type = OBJECT
End
Object LoopSeed
  Side = Wild
  KindOf = INERT IMMOBILE UNATTACKABLE
    Body = ActiveBody Tag_Body
    MaxHealth = 1
  End
    Behavior = GrantUpgradeCreate Tag_Grant
    UpgradeToGrant = Upgrade_Loop
  End
    Behavior = ReplaceSelfUpgrade Tag_Again
    TriggeredBy = Upgrade_Loop
        ReplaceWith = LoopSeed
  End
    Geometry = CYLINDER
  GeometryMajorRadius = 5
  GeometryHeight = 10
End
";

#[test]
fn an_object_that_creates_itself_on_creation_is_cut_off_at_the_nesting_cap() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", SOURCE, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["LoopSeed".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind("LoopSeed").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap()];
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
        .with_creation_lists(Arc::clone(&data.ocls));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    let seed = kinds.kind("LoopSeed").unwrap();
    let seeds = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, held)| **held == seed)
        .count();
    assert!(seeds == 1, "{seeds} seeds");
}
