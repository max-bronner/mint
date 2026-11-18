use std::sync::Arc;

use mint_sage_ini::{AnimationStateDef, AnimationStateKind};
use mint_sage_lua::Env;

use super::*;

fn left(state: &str) -> Prev<'_> {
    Prev {
        state,
        animation: "",
        fraction: 1.0,
    }
}

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
        left("IdleCaptured"),
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
        left("IdleUncaptured"),
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
        "CurDrawableSetTransitionAnimState(\"T\")\nCurDrawableIsCurrentTargetKindof(\"S\")\nreturn \"X\"",
    )
    .unwrap();
    let mut env = Env::new();
    let mut refused = Refusals::default();
    let extras = ExtraTokens::default();
    let run = run(
        &script,
        left("Prev"),
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
        [("CurDrawableIsCurrentTargetKindof", 1u64)]
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
        left("Prev"),
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
        left("Prev"),
        Conditions::MOVING,
        &extras,
        1,
        0,
        &mut env,
        &mut refused,
    );
    let still = run(
        &script,
        left("Prev"),
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

#[test]
fn a_script_decides_from_the_animation_it_left_and_how_far_through_it_was() {
    let script = Script::parse(
        "if CurDrawablePrevAnimation() == \"Bow\" and CurDrawablePrevAnimFraction() > .5 then\n\
         CurDrawableHideSubObject(\"Part\")\nend\n",
    )
    .unwrap();
    let extras = ExtraTokens::default();
    let writes = |animation: &str, fraction: f32| {
        let mut refused = Refusals::default();
        run(
            &script,
            Prev {
                state: "Any",
                animation,
                fraction,
            },
            Conditions::NONE,
            &extras,
            1,
            0,
            &mut Env::new(),
            &mut refused,
        )
        .sub_objects
        .len()
    };
    assert_eq!(writes("Bow", 0.9), 1);
    assert_eq!(writes("Bow", 0.1), 0);
    assert_eq!(writes("Sword", 0.9), 0);
}

#[test]
fn a_sound_call_does_not_stop_the_writes_behind_it() {
    let script =
        Script::parse("CurDrawablePlaySound(\"Sound\")\nCurDrawableHideSubObject(\"Part\")\n")
            .unwrap();
    let mut refused = Refusals::default();
    let run = run(
        &script,
        left(""),
        Conditions::NONE,
        &ExtraTokens::default(),
        1,
        0,
        &mut Env::new(),
        &mut refused,
    );
    assert_eq!(run.sub_objects.len(), 1);
    assert!(refused.is_empty());
}
