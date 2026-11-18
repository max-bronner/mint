use super::*;

const DATA: &str = "\
ObjectCreationList OCL_Plain\n  CreateObject\n    ObjectNames = Piece\n    \
  Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList OCL_Oriented\n  CreateObject\n    ObjectNames = Piece\n    \
  Disposition = LIKE_EXISTING\n    OrientInPrimaryDirection = Yes\n    OrientationOffset = 45\n  End\nEnd\n\
Object PlainEgg\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 0\n    OCL = INITIAL OCL_Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object OrientedEgg\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 0\n    OCL = INITIAL OCL_Oriented\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object Piece\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n";

fn piece_facing(egg: &str) -> f32 {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = [egg.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let north = Facing::from_radians(core::f32::consts::FRAC_PI_2);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind(egg).unwrap(),
            Vec2::new(300.0, 300.0),
            north,
            PLAYER,
        )
        .unwrap()];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_creation_lists(Arc::clone(&data.ocls))
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let piece = kinds.kind("Piece").unwrap();
    let world = simulation.state();
    let faces: Vec<f32> = world
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, held)| **held == piece)
        .filter_map(|(entity, _)| world.get(ids.body, entity))
        .map(|body| body.0.facing.radians())
        .collect();
    assert_eq!(faces.len(), 1, "{faces:?}");
    faces[0]
}

#[test]
fn a_slow_death_phase_list_creates_at_angle_zero_or_the_oriented_offset() {
    assert!(piece_facing("PlainEgg").abs() < 1e-3);
    let oriented = piece_facing("OrientedEgg").to_degrees();
    assert!((oriented - 135.0).abs() < 0.1, "{oriented}");
}
