use std::sync::Arc;

use mint_sage_ini::{AnimationStateDef, AnimationStateKind};
use mint_sage_lua::Env;

use super::*;

fn state(script: &str) -> AnimationStateDef {
    AnimationStateDef {
        kind: AnimationStateKind::Conditioned,
        conditions: Vec::new(),
        state_name: None,
        entering_state_fx: None,
        flags: Default::default(),
        clips: Vec::new(),
        script: Some(mint_sage_ini::Script {
            body: script.to_owned(),
            file: Arc::from("t.ini"),
            line: 3,
        }),
        fx_events: Vec::new(),
        particles: Vec::new(),
        frame_for_pristine_bone_positions: None,
        lua_frame_events: Vec::new(),
        allow_repeat_in_random_pick: false,
        file: Arc::from("t.ini"),
        line: 1,
    }
}

#[test]
fn a_state_with_no_script_compiles_to_none() {
    let mut def = state("Prev = CurDrawablePrevAnimationState()");
    def.script = None;
    let (entries, report) = compile_states(std::slice::from_ref(&def));
    assert_eq!(entries, vec![None]);
    assert_eq!(report.parsed, 0);
    assert_eq!(report.failed, 0);
}

#[test]
fn a_valid_script_is_parsed_and_counted() {
    let states = vec![state(
        "Prev = CurDrawablePrevAnimationState()\nreturn \"A\"",
    )];
    let (entries, report) = compile_states(&states);
    assert!(entries[0].is_some());
    assert_eq!(report.parsed, 1);
    assert_eq!(report.failed, 0);
}

#[test]
fn an_unparseable_script_is_a_counted_load_error_naming_the_line() {
    let states = vec![state("while true do end")];
    let (entries, report) = compile_states(&states);
    assert_eq!(entries, vec![None]);
    assert_eq!(report.parsed, 0);
    assert_eq!(report.failed, 1);
    assert!(
        report.failures[0].contains("t.ini:3"),
        "{:?}",
        report.failures
    );
}

#[test]
fn a_transition_call_and_a_returned_label_are_both_collected() {
    let script = Script::parse(
        "Prev = CurDrawablePrevAnimationState()\n\
         if Prev == \"IdleUncaptured\" then return \"FlagUpNoLower\" end\n\
         CurDrawableSetTransitionAnimState(\"TRANS_FlagDown\")\n\
         return \"FlagUpAfterLower\"",
    )
    .unwrap();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let extras = ExtraTokens::default();
    let run = run(
        &script,
        "IdleCaptured",
        Conditions::NONE,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    assert_eq!(run.transitions, ["TRANS_FlagDown"]);
    assert_eq!(run.label.as_deref(), Some("FlagUpAfterLower"));
    assert!(refused.is_empty());
}

#[test]
fn the_prev_branch_taken_follows_the_state_left() {
    let script = Script::parse(
        "Prev = CurDrawablePrevAnimationState()\n\
         if Prev == \"IdleUncaptured\" then return \"FlagUpNoLower\" end\n\
         return \"Other\"",
    )
    .unwrap();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let extras = ExtraTokens::default();
    let run = run(
        &script,
        "IdleUncaptured",
        Conditions::NONE,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    assert_eq!(run.label.as_deref(), Some("FlagUpNoLower"));
}

#[test]
fn an_unbound_call_is_refused_and_counted_keeping_what_ran_before_it() {
    let script = Script::parse(
        "CurDrawableSetTransitionAnimState(\"T\")\nCurDrawablePlaySound(\"S\")\nreturn \"X\"",
    )
    .unwrap();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let extras = ExtraTokens::default();
    let run = run(
        &script,
        "Prev",
        Conditions::NONE,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    assert_eq!(run.transitions, ["T"]);
    assert_eq!(run.label, None, "the script stopped before its return");
    assert_eq!(
        refused.iter().collect::<Vec<_>>(),
        [("CurDrawablePlaySound", 1u64)]
    );
}

#[test]
fn sub_object_calls_including_permanently_are_collected_in_order() {
    let script = Script::parse(
        "CurDrawableHideSubObject(\"V1\")\n\
         CurDrawableShowSubObjectPermanently(\"N_Window\")",
    )
    .unwrap();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let extras = ExtraTokens::default();
    let run = run(
        &script,
        "Prev",
        Conditions::NONE,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    assert_eq!(
        run.sub_objects,
        vec![
            ("V1".to_owned(), false, false),
            ("N_Window".to_owned(), true, true),
        ]
    );
}

#[test]
fn model_condition_reads_the_live_bit_by_name() {
    let script = Script::parse(
        "if CurDrawableModelcondition(\"MOVING\") then return \"Yes\" end\nreturn \"No\"",
    )
    .unwrap();
    let extras = ExtraTokens::default();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let moving = run(
        &script,
        "Prev",
        Conditions::MOVING,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    let still = run(
        &script,
        "Prev",
        Conditions::NONE,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    assert_eq!(moving.label.as_deref(), Some("Yes"));
    assert_eq!(still.label.as_deref(), Some("No"));
}
