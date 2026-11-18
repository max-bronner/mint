use super::super::audio::{AudioOp, ScriptAudio};
use super::*;

fn run_with_sounds(scripts: Vec<Script>, lengths: &[(&str, u32)], ticks: u32) -> MapScriptState {
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let lengths = lengths
        .iter()
        .map(|(name, ms)| ((*name).to_owned(), *ms))
        .collect();
    let engine = MapScriptEngine::build(&[tree], &[], &BTreeMap::new(), &BTreeMap::new(), false)
        .with_sound_lengths(lengths);
    let rules = rules_over(96)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, state)| state.clone()))
        .unwrap()
}

fn waits_on(sound: &str) -> Script {
    script_of(
        "Wait",
        true,
        false,
        Some(("HAS_FINISHED_AUDIO", vec![text(sound)])),
        vec![act("INCREMENT_COUNTER", vec![number(1), text("done")])],
    )
}

#[test]
fn the_wait_starts_at_the_first_evaluation_and_restarts_after_firing() {
    let mut audio = ScriptAudio::default();
    assert!(!audio.finished("tone", 2000, 100));
    assert!(!audio.finished("tone", 2000, 109));
    assert!(audio.finished("tone", 2000, 110));
    assert!(!audio.finished("tone", 2000, 111));
    assert!(audio.finished("tone", 2000, 121));
}

#[test]
fn a_sound_shorter_than_a_frame_is_finished_on_the_frame_it_is_asked() {
    let mut audio = ScriptAudio::default();
    assert!(audio.finished("click", 150, 7));
}

#[test]
fn an_unknown_event_is_finished_at_once_and_a_known_one_waits_its_length() {
    let ticks_per_frame = u32::try_from(TICKS_PER_SCRIPT_FRAME).unwrap();
    let state = run_with_sounds(vec![waits_on("NoSuchEvent")], &[], ticks_per_frame * 3);
    assert_eq!(state.counter_value("", "done"), 3);
    let state = run_with_sounds(
        vec![waits_on("TestTone")],
        &[("testtone", 1000)],
        ticks_per_frame * 4,
    );
    assert_eq!(state.counter_value("", "done"), 0);
    let state = run_with_sounds(
        vec![waits_on("TestTone")],
        &[("testtone", 1000)],
        ticks_per_frame * 6 + 1,
    );
    assert_eq!(state.counter_value("", "done"), 1);
}

#[test]
fn audio_actions_queue_numbered_commands_in_order() {
    let play = script_of(
        "Play",
        true,
        true,
        None,
        vec![
            act("PLAY_SOUND_EFFECT", vec![text("TestTone")]),
            act("EVA_SET_ENABLED_DISABLED", vec![number(0)]),
        ],
    );
    let state = run_with_sounds(vec![play], &[], 1);
    let ops: Vec<_> = state
        .audio
        .pending
        .iter()
        .map(|cmd| (cmd.serial, cmd.op.clone()))
        .collect();
    assert_eq!(ops.len(), 2);
    assert!(matches!(&ops[0], (1, AudioOp::Sound { name, at: None, .. }) if name == "TestTone"));
    assert_eq!(ops[1], (2, AudioOp::Eva(false)));
}
