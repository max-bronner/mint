#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]

use super::*;
use crate::ai::tests::{catalog, data, session, Verdict};
use crate::engine::apply_with;
use crate::harness::{play_with_ai, AiRun, Boundary};
use crate::tests::AttackerLoses;

const DATE: [u16; 8] = [2026, 10, 28, 3, 12, 0, 0, 0];

fn six_ai_session() -> (LivingWorldState, StrategicAi) {
    let seats: Vec<_> = ["A", "B", "C", "D", "E", "F"]
        .into_iter()
        .map(|start| (PlayerKind::Computer, start))
        .collect();
    let state = session(&seats, 8, "", 7);
    let ai = StrategicAi::for_computers(&state, 5, false);
    (state, ai)
}

fn rules() -> Rules {
    Rules::build(&data(6, 8, ""), &catalog(), "Camp", 0, 0).unwrap()
}

fn drive(state: &mut LivingWorldState, ai: &mut StrategicAi, turns: u32) -> Vec<Boundary> {
    let mut predictor = Verdict::new(true, 3);
    let run = AiRun {
        ai,
        predictor: &mut predictor,
    };
    play_with_ai(state, &mut AttackerLoses, run, turns, &mut Vec::new()).boundaries
}

fn at_turn_ten_mid_planning() -> (LivingWorldState, StrategicAi) {
    let (mut state, mut ai) = six_ai_session();
    drive(&mut state, &mut ai, 9);
    assert_eq!(state.turn, 10);
    let mut predictor = Verdict::new(true, 3);
    for _ in 0..8 {
        let commands = ai.step(&mut state, &mut predictor).unwrap();
        for (player, command) in commands {
            apply_with(&mut state, Some(&mut AttackerLoses), player, command).ok();
        }
    }
    (state, ai)
}

#[test]
fn a_session_saved_at_turn_ten_and_loaded_matches_an_uninterrupted_run_to_turn_twenty() {
    let (mut state, mut ai) = at_turn_ten_mid_planning();
    let bytes = save_file(&state, Some(&ai), "ten", DATE).unwrap();
    let key = file_key(&bytes).unwrap();
    assert_eq!(
        (key.campaign.as_str(), key.scenario, key.victory_type),
        ("Camp", 0, 0)
    );

    let straight = drive(&mut state, &mut ai, 10);

    let (mut loaded, loaded_ai) = load_file(rules(), &bytes).unwrap();
    let mut loaded_ai = loaded_ai.unwrap();
    let resumed = drive(&mut loaded, &mut loaded_ai, 10);

    assert_eq!(loaded.turn, 20);
    assert!(straight.len() >= 10 * 6, "{} boundaries", straight.len());
    assert_eq!(straight, resumed);
    assert_eq!(state.state_hash(), loaded.state_hash());
}

#[test]
fn the_payload_round_trips_byte_for_byte() {
    let (state, ai) = at_turn_ten_mid_planning();
    let payload = state.save_payload(Some(&ai)).unwrap();
    let (loaded, loaded_ai) = LivingWorldState::load_payload(rules(), &payload, 0).unwrap();
    assert_eq!(loaded.state_hash(), state.state_hash());
    assert_eq!(
        loaded.save_payload(loaded_ai.as_ref()).unwrap(),
        payload,
        "a field was dropped or reordered"
    );
}

#[test]
fn a_battle_in_flight_or_a_phase_between_planning_stops_a_save() {
    let (mut state, _) = six_ai_session();
    state.awaiting = Some(1);
    assert_eq!(state.save_payload(None), Err(SaveRefused::BattleAwaited));
    state.awaiting = None;
    state.phase = Phase::ResolveBattles;
    assert_eq!(
        state.save_payload(None),
        Err(SaveRefused::WrongPhase(Phase::ResolveBattles))
    );
}

#[test]
fn an_unknown_version_or_other_rules_load_as_errors() {
    let (state, _) = six_ai_session();
    let mut payload = state.save_payload(None).unwrap();
    assert_eq!(
        LivingWorldState::load_payload(
            Rules::build(&data(6, 7, ""), &catalog(), "Camp", 0, 0).unwrap(),
            &payload,
            0
        )
        .err(),
        Some(LoadError::RulesMismatch)
    );
    let version_at = 1 + TAG.len();
    payload[version_at] = SESSION_VERSION + 1;
    let error = LivingWorldState::load_payload(rules(), &payload, 0).err();
    assert!(matches!(
        error,
        Some(LoadError::Format(SaveError::UnsupportedVersion { .. }))
    ));
}

#[test]
fn every_truncation_of_a_save_errors() {
    let (state, ai) = at_turn_ten_mid_planning();
    let payload = state.save_payload(Some(&ai)).unwrap();
    for len in 0..payload.len() {
        assert!(
            LivingWorldState::load_payload(rules(), &payload[..len], 0).is_err(),
            "prefix of {len} bytes loaded"
        );
    }
}

#[test]
fn the_file_lists_under_the_single_player_tab() {
    let (state, _) = six_ai_session();
    let bytes = save_file(&state, None, "d", DATE).unwrap();
    let info = mint_sage_save::read_info(&bytes).unwrap();
    assert!(mint_sage_save::tab_accepts(
        mint_sage_save::SaveTab::WotrSingle,
        info.save_file_type
    ));
}

fn with_rank_state_and_a_tutorial_hook() -> LivingWorldState {
    let (mut state, _) = six_ai_session();
    state.players[0].skill = Skill::Brutal;
    state.players[0].skill_points = 12.5;
    state.players[0].sciences = vec!["SCIENCE_MEN".to_owned()];
    state.players[0].upgrades = vec!["Upgrade_Persists".to_owned()];
    state.scripted = Some(Scripted {
        armies: vec![("Scout".to_owned(), ArmyId(1))],
    });
    state
}

#[test]
fn the_rank_state_and_the_tutorial_hook_survive_a_save() {
    let state = with_rank_state_and_a_tutorial_hook();
    let payload = state.save_payload(None).unwrap();
    let (loaded, _) = LivingWorldState::load_payload(rules(), &payload, 0).unwrap();
    let held = &loaded.players[0];
    assert_eq!(
        (
            held.skill,
            held.skill_points,
            &held.sciences,
            &held.upgrades
        ),
        (
            Skill::Brutal,
            12.5,
            &state.players[0].sciences,
            &state.players[0].upgrades
        )
    );
    assert_eq!(
        loaded.scripted.as_ref().map(|s| s.armies.clone()),
        state.scripted.as_ref().map(|s| s.armies.clone())
    );
}

#[test]
fn the_palantir_markers_survive_a_save_and_are_in_the_state_hash() {
    let (mut state, _) = six_ai_session();
    let before = state.state_hash();
    state.palantir_markers = vec![crate::PalantirMarker {
        event: "Event".to_owned(),
        movie: "Movie".to_owned(),
        position: [1.5, -2.0],
        play_next_act_after_movie: true,
    }];
    assert_ne!(state.state_hash(), before);
    let payload = state.save_payload(None).unwrap();
    let (loaded, _) = LivingWorldState::load_payload(rules(), &payload, 0).unwrap();
    assert_eq!(loaded.palantir_markers, state.palantir_markers);
    assert_eq!(loaded.state_hash(), state.state_hash());
}

#[test]
fn the_rank_state_and_the_tutorial_hook_are_in_the_state_hash() {
    let base = with_rank_state_and_a_tutorial_hook();
    let changes: [fn(&mut LivingWorldState); 6] = [
        |s| s.players[0].skill = Skill::Easy,
        |s| s.players[0].skill_points = 12.75,
        |s| s.players[0].sciences.clear(),
        |s| s.players[0].upgrades.clear(),
        |s| s.scripted = None,
        |s| s.scripted = Some(Scripted::default()),
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut other = base.clone();
        change(&mut other);
        assert_ne!(other.state_hash(), base.state_hash(), "change {i}");
    }
}

#[test]
fn a_battles_withdrawn_list_survives_a_save_and_is_in_the_state_hash() {
    let (mut state, _) = six_ai_session();
    state.battles.push(crate::Battle {
        id: 1,
        region: crate::RegionId(0),
        players: vec![PlayerId(1)],
        choices: Vec::new(),
        withdrawn: Vec::new(),
        snapshot: Vec::new(),
    });
    let before = state.state_hash();
    state.battles[0].withdrawn = vec![PlayerId(2)];
    assert_ne!(state.state_hash(), before);
    let payload = state.save_payload(None).unwrap();
    let (loaded, _) = LivingWorldState::load_payload(rules(), &payload, 0).unwrap();
    assert_eq!(loaded.battles[0].withdrawn, vec![PlayerId(2)]);
}
