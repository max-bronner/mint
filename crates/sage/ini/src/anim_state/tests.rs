use super::*;
use crate::{BlockTable, Document};

fn draw_of(text: &str) -> Block {
    let table = BlockTable::object_model_path();
    let document = Document::parse("t.ini", text, &table).unwrap();
    let draw = document.roots()[0].blocks().next().unwrap().clone();
    draw
}

const ARCHER: &str = "\
Object A
  Draw = W3DScriptedModelDraw Tag
    DefaultModelConditionState
      Model = GUArcher_SKN
    End
    IdleAnimationState
      StateName = STATE_Idle
      Animation = IdleA
        AnimationName = GUArcher_IDLA
        AnimationPriority = 6
        AnimationSpeedFactorRange = 0.7 1.3
      End
      Animation = IdleH
        AnimationName = GUArcher_IDLH
        AnimationBlendTime = 8
      End
    End
    AnimationState = DYING SPLATTED
      Animation
        AnimationName = GUArcher_DIEB
        AnimationMode = ONCE
      End
    End
    AnimationState = DYING
      Animation
        AnimationName = GUArcher_DIEA
        AnimationMode = ONCE
      End
    End
    AnimationState = MOVING
      Animation = Move1
        AnimationName = GUArcher_RUNA
        Distance = 28
      End
      Flags = RANDOMSTART
    End
    TransitionState = TRANS_Idle_to_Selected
      Animation
        AnimationName = GUArcher_ATNA
        AnimationMode = ONCE
      End
    End
  End
End
";

#[test]
fn every_state_family_is_read_in_declaration_order() {
    let states = draw_animation_states(&draw_of(ARCHER));
    let names: Vec<(&AnimationStateKind, &[String])> = states
        .iter()
        .map(|state| (&state.kind, state.conditions.as_slice()))
        .collect();
    assert_eq!(states.len(), 5, "{names:?}");
    assert_eq!(states[0].kind, AnimationStateKind::Idle);
    assert!(states[0].conditions.is_empty());
    assert_eq!(states[1].conditions, ["DYING", "SPLATTED"]);
    assert_eq!(states[2].conditions, ["DYING"]);
    assert_eq!(states[3].conditions, ["MOVING"]);
    assert_eq!(states[4].kind, AnimationStateKind::Transition);
    assert_eq!(
        states[4].transition_name(),
        Some("TRANS_Idle_to_Selected"),
        "a transition's argument is its name, not a condition"
    );
}

#[test]
fn a_states_clips_carry_their_label_mode_rate_and_distance() {
    let states = draw_animation_states(&draw_of(ARCHER));
    let idle = &states[0];
    assert_eq!(idle.state_name.as_deref(), Some("STATE_Idle"));
    assert_eq!(idle.clips.len(), 2);
    assert_eq!(idle.clips[0].label.as_deref(), Some("IdleA"));
    assert_eq!(idle.clips[0].speed_range, Some((0.7, 1.3)));
    assert_eq!(idle.clips[0].priority, Some(6));
    assert_eq!(idle.clips[1].blend_time, Some(8.0));

    let moving = &states[3];
    assert_eq!(moving.clips[0].distance, Some(28.0));
    assert!(moving.flags.random_start);
    assert_eq!(states[2].clips[0].label, None);
    assert_eq!(states[2].clips[0].mode, Some(AnimationMode::Once));
}

#[test]
fn an_animation_name_whose_first_token_is_latin1_whitespace_yields_no_clip() {
    let text = format!(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             Animation\n        AnimationName = {} GUArcher_RUNA\n      End\n    End\n  \
             End\nEnd\n",
        '\u{00a0}'
    );
    let states = draw_animation_states(&draw_of(&text));
    assert_eq!(states.len(), 1);
    assert!(
        states[0].clips.is_empty(),
        "a name whose first token trimmed to nothing still became a clip: {:?}",
        states[0].clips
    );
}

#[test]
fn an_animation_name_that_is_empty_yields_no_clip_rather_than_an_empty_one() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             Animation\n        AnimationName =\n      End\n    End\n  End\nEnd\n",
    ));
    assert_eq!(states.len(), 1, "the state itself is still read");
    assert!(
        states[0].clips.is_empty(),
        "an empty name would resolve to no file at all"
    );
}

#[test]
fn an_explicit_none_condition_reads_as_no_condition() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = NONE\n      \
             Animation\n        AnimationName = X\n      End\n    End\n  End\nEnd\n",
    ));
    assert!(states[0].conditions.is_empty());
    assert!(states[0].is_unconditional());
}

#[test]
fn condition_tokens_are_upper_cased_so_a_matcher_never_has_to_care() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = Moving attacking\n  \
             End\nEnd\n",
    ));
    assert_eq!(states[0].conditions, ["MOVING", "ATTACKING"]);
}

#[test]
fn a_document_cannot_declare_more_states_or_clips_than_the_caps_allow() {
    let mut text = String::from("Object A\n  Draw = W3DScriptedModelDraw T\n");
    for _ in 0..(MAX_STATES_PER_DRAW + 10) {
        text.push_str("    AnimationState = MOVING\n      Animation\n        AnimationName = X\n      End\n    End\n");
    }
    text.push_str("  End\nEnd\n");
    let states = draw_animation_states(&draw_of(&text));
    assert_eq!(states.len(), MAX_STATES_PER_DRAW);

    let mut text =
        String::from("Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n");
    for _ in 0..(MAX_CLIPS_PER_STATE + 10) {
        text.push_str("      Animation\n        AnimationName = X\n      End\n");
    }
    text.push_str("    End\n  End\nEnd\n");
    let states = draw_animation_states(&draw_of(&text));
    assert_eq!(states[0].clips.len(), MAX_CLIPS_PER_STATE);
}

#[test]
fn a_script_region_is_kept_verbatim_with_its_line() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             BeginScript\n        Prev = CurDrawablePrevAnimationState()\n      EndScript\n    \
             End\n  End\nEnd\n",
    ));
    let script = states[0].script.as_ref().expect("script region");
    assert!(script.body.contains("CurDrawablePrevAnimationState"));
    assert_eq!(script.line, 4);
}

#[test]
fn a_state_with_no_script_reads_none() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             Animation\n        AnimationName = X\n      End\n    End\n  End\nEnd\n",
    ));
    assert!(states[0].script.is_none());
}

#[test]
fn every_shipped_fx_event_spelling_reads_the_same_fields() {
    let cases: &[(&str, u32, bool, &str, Option<u32>, Option<&str>)] = &[
        (
            "Frame:14 Name:FX_EntSprout",
            14,
            false,
            "FX_EntSprout",
            None,
            None,
        ),
        (
            "Frame: 300 Name: FX_BaradDurWaveDust",
            300,
            false,
            "FX_BaradDurWaveDust",
            None,
            None,
        ),
        (
            "Frame:20 FireWhenSkipped Name:FX_BalrogScreamEfx",
            20,
            true,
            "FX_BalrogScreamEfx",
            None,
            None,
        ),
        (
            "Frame:14 Name:FX_GandalfLightningSword FrameStop:70",
            14,
            false,
            "FX_GandalfLightningSword",
            Some(70),
            None,
        ),
        (
            "Name: FX_ElendilGlowEvent",
            0,
            false,
            "FX_ElendilGlowEvent",
            None,
            None,
        ),
        (
            "Frame:14 Name:FX_X FrameStop:20 Bone:SWORD",
            14,
            false,
            "FX_X",
            Some(20),
            Some("SWORD"),
        ),
    ];
    for (text, frame, fire_when_skipped, name, frame_stop, bone) in cases {
        let states = draw_animation_states(&draw_of(&format!(
                "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
                 Animation\n        AnimationName = X\n      End\n      FXEvent = {text}\n    End\n  \
                 End\nEnd\n"
            )));
        let event = states[0]
            .fx_events
            .first()
            .unwrap_or_else(|| panic!("{text}"));
        assert_eq!(event.frame, *frame, "{text}");
        assert_eq!(event.fire_when_skipped, *fire_when_skipped, "{text}");
        assert_eq!(event.fx_list, *name, "{text}");
        assert_eq!(event.frame_stop, *frame_stop, "{text}");
        assert_eq!(event.bone.as_deref(), *bone, "{text}");
    }
}

#[test]
fn an_fx_event_with_no_name_is_not_read_as_one() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             FXEvent = Frame:14\n    End\n  End\nEnd\n",
    ));
    assert!(states[0].fx_events.is_empty());
}

#[test]
fn fx_events_are_capped_per_state() {
    let mut text =
        String::from("Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n");
    for i in 0..(MAX_FX_EVENTS_PER_STATE + 10) {
        text.push_str(&format!("      FXEvent = Frame:{i} Name:FX_X\n"));
    }
    text.push_str("    End\n  End\nEnd\n");
    let states = draw_animation_states(&draw_of(&text));
    assert_eq!(states[0].fx_events.len(), MAX_FX_EVENTS_PER_STATE);
}

#[test]
fn particle_sys_bone_is_read_from_an_animation_state() {
    let states = draw_animation_states(&draw_of(
        "Object A\n  Draw = W3DScriptedModelDraw T\n    AnimationState = MOVING\n      \
             Animation\n        AnimationName = X\n      End\n    End\n    AnimationState = \
             MOVING ACCELERATE\n      ParticleSysBone NONE StunLarge\n    End\n  End\nEnd\n",
    ));
    assert!(states[0].particles.is_empty());
    assert_eq!(states[1].particles.len(), 1);
    let attachment = &states[1].particles[0];
    assert_eq!(attachment.bone, None);
    assert_eq!(attachment.system, "StunLarge");
}

#[test]
fn a_states_pristine_bone_frame_is_read() {
    let states = draw_animation_states(&draw_of(
        "\
Object A
  Draw = W3DScriptedModelDraw Tag
    DefaultModelConditionState
      Model = GUArcher_SKN
    End
    AnimationState = FIRING_OR_PREATTACK_A
      Animation
        AnimationName = GUArcher_THRA
      End
      FrameForPristineBonePositions = 39
    End
    AnimationState = FIRING_OR_PREATTACK_B
      Animation
        AnimationName = GUArcher_THRB
      End
      FrameForPristineBonePositions = 42;44
    End
    AnimationState = MOVING
      Animation
        AnimationName = GUArcher_RUNA
      End
    End
  End
End
",
    ));
    let frames: Vec<Option<u32>> = states
        .iter()
        .map(|state| state.frame_for_pristine_bone_positions)
        .collect();
    assert_eq!(frames, [Some(39), Some(42), None]);
}

#[test]
fn the_first_model_token_becomes_the_prefix() {
    let clip = |name: &str| AnimationClip {
        label: None,
        name: name.to_owned(),
        mode: None,
        blend_time: None,
        must_complete_blend: false,
        speed_range: None,
        distance: None,
        use_weapon_timing: false,
        priority: None,
    };
    for (name, prefix, resolved, asset) in [
        ("#(MODEL)_X", "P", "P_X", "P_X"),
        ("#(MODEL)_X#(MODEL)", "P", "P_X#(MODEL)", "P_X#(MODEL)"),
        ("H.#(MODEL)_X", "P", "H.P_X", "P_X"),
        ("PLAIN_X", "P", "PLAIN_X", "PLAIN_X"),
        ("#(MODEL)_X", "", "_X", "_X"),
    ] {
        let clip = clip(name);
        assert_eq!(clip.name_for(prefix), resolved, "{name}");
        assert_eq!(clip.asset_for(prefix), asset, "{name}");
    }
}
