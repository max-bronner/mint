use super::*;

const SOURCE: &str = "\
Upgrade Upgrade_GateTest\n  Type = OBJECT\nEnd\n\
ObjectCreationList OCL_GateTest\n  \
  CreateObject\n    ObjectNames = PlainHatchling\n    ForbiddenUpgrades = Upgrade_GateTest\n  End\n  \
  CreateObject\n    ObjectNames = MarkedHatchling\n    RequiredUpgrades = Upgrade_GateTest\n  End\n\
End\n\
Object PlainPod\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 0\n    OCL = INITIAL OCL_GateTest\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object MarkedPod\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_GateTest\n  End\n  \
  Behavior = LifetimeUpdate Tag_Life\n    MinLifetime = 0.0\n    MaxLifetime = 0.0\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 0\n    OCL = INITIAL OCL_GateTest\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object PlainHatchling\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n\
Object MarkedHatchling\n  Side = Wild\n  KindOf = INERT IMMOBILE UNATTACKABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 10\nEnd\n";

fn hatched_by(pod: &str) -> (usize, usize) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", SOURCE, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[pod.to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind(pod).unwrap(),
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
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let world = simulation.state();
    let count = |name: &str| {
        let kind = kinds.kind(name).unwrap();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(_, held)| **held == kind)
            .count()
    };
    (count("PlainHatchling"), count("MarkedHatchling"))
}

#[test]
fn a_creation_nugget_runs_only_when_its_source_passes_the_upgrade_gate() {
    assert_eq!(hatched_by("PlainPod"), (1, 0));
    assert_eq!(hatched_by("MarkedPod"), (0, 1));
}
