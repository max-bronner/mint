use super::*;

const DATA: &str = "\
Object Trinket\n  Side = Men\n  KindOf = SELECTABLE\n  \
  Behavior = AttachUpdate Tag_Attach\n    ObjectFilter = ANY ENEMIES\n    ScanRange = 50\n    \
    ParentStatus = HOLDING_THE_TRINKET\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = SELECTABLE INFANTRY HERO\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Wolf\n  Side = Men\n  KindOf = SELECTABLE CREEP\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Snare\n  Side = Men\n  KindOf = SELECTABLE\n  \
  Behavior = AttachUpdate Tag_Attach\n    ObjectFilter = ANY\n    ScanRange = 50\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Halo\n  Side = Men\n  KindOf = SELECTABLE IMMOBILE\n  \
  Behavior = AttachUpdate Tag_Attach\n    ObjectFilter = ANY +HERO\n    ScanRange = 50\n    \
    AnchorToTopOfGeometry = Yes\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n";

const CREEPS: PlayerId = PlayerId::new(2);

fn scene(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Trinket", "Hero", "Wolf", "Snare", "Halo"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<_> = placed
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
        .with_construction(Arc::clone(&kinds), production)
        .with_players(2);
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

#[test]
fn the_scan_filters_by_relation_to_its_own_player_not_just_kind() {
    let (mut simulation, ids, entities) = scene(&[
        ("Trinket", 300.0, 300.0, PLAYER),
        ("Hero", 310.0, 300.0, PLAYER),
        ("Hero", 320.0, 300.0, OTHER),
    ]);
    let (trinket, _own_hero, enemy_hero) = (entities[0], entities[1], entities[2]);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let link = simulation.state().get(ids.attach_link, trinket).copied();
    assert_eq!(
        link.map(|l| l.parent),
        Some(enemy_hero),
        "the trinket must skip its own player's closer hero for the enemy's farther one"
    );
}

#[test]
fn a_creep_of_a_non_playable_side_is_never_a_parent() {
    let (mut simulation, ids, entities) = scene(&[
        ("Snare", 300.0, 300.0, PLAYER),
        ("Wolf", 310.0, 300.0, CREEPS),
    ]);
    let (snare, _wolf) = (entities[0], entities[1]);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        simulation.state().get(ids.attach_link, snare).is_none(),
        "a creep of the non-playable creep side must never be picked"
    );

    let (mut simulation, ids, entities) = scene(&[
        ("Snare", 300.0, 300.0, PLAYER),
        ("Wolf", 310.0, 300.0, PLAYER),
    ]);
    let (snare, wolf) = (entities[0], entities[1]);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.attach_link, snare)
            .copied()
            .map(|l| l.parent),
        Some(wolf),
        "the same CREEP template owned by a playable side must be picked"
    );
}

#[test]
fn an_object_anchored_to_the_top_of_its_parent_ends_the_tick_above_it() {
    let (mut simulation, ids, entities) = scene(&[
        ("Halo", 300.0, 300.0, PLAYER),
        ("Hero", 305.0, 300.0, PLAYER),
    ]);
    let (halo, hero) = (entities[0], entities[1]);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let world = simulation.state();
    assert_eq!(
        world.get(ids.attach_link, halo).map(|l| l.parent),
        Some(hero)
    );
    let feet = world.get(ids.elevation, hero).map(|e| e.0).unwrap();
    let halo_z = world.get(ids.elevation, halo).map(|e| e.0).unwrap();
    assert!(
        (halo_z - (feet + 16.0)).abs() < 1e-3,
        "halo {halo_z}, hero feet {feet}"
    );
}
