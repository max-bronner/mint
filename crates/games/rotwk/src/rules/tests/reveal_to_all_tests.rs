use super::*;

const DATA: &str = "\
Object Band\n  Side = Men\n  KindOf = SELECTABLE REVEAL_TO_ALL\n  ShroudClearingRange = 150\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Plain\n  Side = Men\n  KindOf = SELECTABLE\n  ShroudClearingRange = 150\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n";

fn scene(kind: &str) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Band", "Plain"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind(kind).unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_players(2)
        .with_shroud(true);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

#[test]
fn reveal_to_all_uncovers_its_spot_for_every_player() {
    let (mut simulation, ids) = scene("Plain");
    simulation.tick().unwrap();
    let shroud = RotwkRules::shroud_ids(ids);
    let (_, _, visible) = crate::shroud::cell_counts(simulation.state(), shroud, OTHER);
    assert_eq!(
        visible, 0,
        "a plain object must not reveal for the other player"
    );

    let (mut simulation, ids) = scene("Band");
    simulation.tick().unwrap();
    let shroud = RotwkRules::shroud_ids(ids);
    let (_, _, visible) = crate::shroud::cell_counts(simulation.state(), shroud, OTHER);
    assert!(
        visible > 0,
        "REVEAL_TO_ALL must uncover its spot for the other player too"
    );
}
