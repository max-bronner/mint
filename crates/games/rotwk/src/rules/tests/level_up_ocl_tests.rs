use super::*;

const DATA: &str = "\
ObjectCreationList OCL_GradA\n  CreateObject\n    ObjectNames = MarkA\n    Disposition = LIKE_EXISTING\n  End\nEnd\n\
ObjectCreationList OCL_GradB\n  CreateObject\n    ObjectNames = MarkB\n    Disposition = LIKE_EXISTING\n  End\nEnd\n\
ExperienceLevel GradLevel1\n  TargetNames = Graduate\n  RequiredExperience = 1\n  Rank = 1\nEnd\n\
ExperienceLevel GradLevel2\n  TargetNames = Graduate\n  RequiredExperience = 5\n  Rank = 2\n  \
  LevelUpOCL = OCL_GradA\nEnd\n\
ExperienceLevel GradLevel3\n  TargetNames = Graduate\n  RequiredExperience = 10\n  Rank = 3\n  \
  LevelUpOCL = OCL_GradB\nEnd\n\
Object Graduate\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object MarkA\n  Side = Men\n  KindOf = INERT\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object MarkB\n  Side = Men\n  KindOf = INERT\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n";

fn marks_after_gain(warm_up: u32) -> (usize, usize) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = ["Graduate", "MarkA", "MarkB"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let (kinds, missing) = KindTable::closure(&game, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let spawns = vec![kinds
        .spawn(
            &game.combat,
            kinds.kind("Graduate").unwrap(),
            Vec2::new(200.0, 200.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap()];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits))
        .with_experience(Arc::clone(&game.experience), game.armor_bonus_cap)
        .with_experience_scalars(Arc::clone(&game.experience_scalars))
        .with_creation_lists(Arc::clone(&game.ocls));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let graduate = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .first()
        .copied()
        .unwrap();
    for _ in 0..warm_up {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![graduate],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 10 })
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    let count = |name: &str| {
        let kind = kinds.kind(name).unwrap();
        simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(_, held)| **held == kind)
            .count()
    };
    (count("MarkA"), count("MarkB"))
}

#[test]
fn every_level_reached_creates_its_own_level_up_list() {
    assert_eq!(marks_after_gain(70), (1, 1));
    assert_eq!(marks_after_gain(5), (0, 0));
}
