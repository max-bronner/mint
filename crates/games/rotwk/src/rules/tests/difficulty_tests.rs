use super::*;
use crate::lobby::Difficulty;
use crate::rules::difficulty::{DifficultyPlayer, DifficultySetup};

const NAMES: [&str; 8] = [
    "Upgrade_EasyAIMultiPlayer",
    "Upgrade_MediumAIMultiPlayer",
    "Upgrade_HardAIMultiPlayer",
    "Upgrade_BrutalAIMultiPlayer",
    "Upgrade_EasyAISinglePlayer",
    "Upgrade_MediumAISinglePlayer",
    "Upgrade_HardAISinglePlayer",
    "Upgrade_BrutalAISinglePlayer",
];

fn player(id: u8, human: bool, playable: bool, ai: Option<Difficulty>) -> DifficultyPlayer {
    DifficultyPlayer {
        player: PlayerId::new(id),
        human,
        playable,
        ai,
    }
}

fn held(setup: DifficultySetup) -> Vec<Vec<&'static str>> {
    let mut text = String::new();
    for name in NAMES {
        text.push_str(&format!("Upgrade {name}\n  Type = OBJECT\nEnd\n"));
    }
    text.push_str(
        "Object DifficultyTestUnit\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
         Body = ActiveBody Tag\n    MaxHealth = 10\n  End\n  \
         Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n",
    );
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("DifficultyTestUnit")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let kind = kinds.kind("DifficultyTestUnit").unwrap();
    let owners: Vec<PlayerId> = setup.players.iter().map(|p| p.player).collect();
    let spawns: Vec<_> = owners
        .iter()
        .map(|owner| {
            kinds
                .spawn(
                    &data.combat,
                    kind,
                    Vec2::new(0.0, 0.0),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let map = flat_map(64, 64);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_difficulty(setup);
    let simulation = Simulation::new(1, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    owners
        .iter()
        .map(|owner| {
            let entity = world
                .store(ids.owner)
                .unwrap()
                .iter()
                .find(|(_, o): &(Entity, &Owner)| o.0 == *owner)
                .map(|(entity, _)| entity)
                .unwrap();
            let mask = world.get(ids.object_upgrades, entity);
            NAMES
                .into_iter()
                .filter(|name| {
                    let bit = data.upgrade_bits.bit(name).unwrap();
                    mask.is_some_and(|m| m.0.contains(bit))
                })
                .collect()
        })
        .collect()
}

#[test]
fn multiplayer_ai_units_hold_their_own_difficulty_upgrade_and_nobody_else_does() {
    let got = held(DifficultySetup {
        multiplayer: true,
        game: Difficulty::Hard,
        players: vec![
            player(0, true, true, None),
            player(1, false, true, Some(Difficulty::Easy)),
            player(2, false, false, Some(Difficulty::Hard)),
            player(3, false, true, Some(Difficulty::Brutal)),
        ],
    });
    assert_eq!(
        got,
        vec![
            vec![],
            vec!["Upgrade_EasyAIMultiPlayer"],
            vec![],
            vec!["Upgrade_BrutalAIMultiPlayer"],
        ]
    );
}

#[test]
fn single_player_ai_units_hold_the_game_difficulty_upgrade() {
    let got = held(DifficultySetup {
        multiplayer: false,
        game: Difficulty::Hard,
        players: vec![
            player(0, true, true, None),
            player(1, false, true, Some(Difficulty::Easy)),
            player(2, false, true, None),
        ],
    });
    assert_eq!(
        got,
        vec![
            vec![],
            vec!["Upgrade_HardAISinglePlayer"],
            vec!["Upgrade_HardAISinglePlayer"],
        ]
    );
}
