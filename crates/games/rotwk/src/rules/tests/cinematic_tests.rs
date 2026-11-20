use super::*;
use crate::rules::map_scripts::{MapScriptEngine, ScriptView};
use mint_sage_map::{
    Condition, OrCondition, Parameter, Script, ScriptAction, ScriptNode, ScriptTree,
};

fn param(string: &str, int: i32, real: f32) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value: int,
        real_value: real,
        string_value: string.to_owned(),
    }
}

fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_owned(),
        params,
    }
}

fn run(actions: Vec<ScriptAction>, objectives: &[(&str, bool)], ticks: u32) -> ScriptView {
    let script = Script {
        name: "Cine".to_owned(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot: true,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions: vec![OrCondition {
            conditions: vec![Condition {
                condition_type: 0,
                inverted: false,
                enabled: true,
                name: "CONDITION_TRUE".to_owned(),
                params: vec![],
            }],
        }],
        actions,
        actions_false: vec![],
        sequence: Default::default(),
    };
    let tree = ScriptTree {
        player_slots: vec![vec![ScriptNode::Script(script)]],
    };
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        ("WpA".to_owned(), (Vec2::new(10.0, 20.0), 0.0)),
        ("WpB".to_owned(), (Vec2::new(30.0, 40.0), 0.0)),
    ]
    .into_iter()
    .collect();
    let engine = MapScriptEngine::build_with_cameras(
        &[tree],
        &[],
        &waypoints,
        &BTreeMap::new(),
        true,
        &BTreeMap::new(),
    )
    .with_objectives(
        objectives
            .iter()
            .map(|(tag, bonus)| ((*tag).to_owned(), *bonus))
            .collect(),
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        );
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation.rules().script_view(simulation.state())
}

#[test]
fn letterbox_hides_the_control_bar_and_end_restores_it() {
    let begun = run(vec![act("CAMERA_LETTERBOX_BEGIN", vec![])], &[], 2);
    assert!(begun.cine.letterbox && begun.ui_hidden);
    let ended = run(
        vec![
            act("CAMERA_LETTERBOX_BEGIN", vec![]),
            act("CAMERA_LETTERBOX_END", vec![]),
        ],
        &[],
        2,
    );
    assert!(!ended.cine.letterbox && !ended.ui_hidden);
}

#[test]
fn disabling_input_counts_once_per_action_and_enabling_clears_the_flag() {
    let disabled = run(vec![act("DISABLE_INPUT", vec![])], &[], 2);
    assert!(disabled.cine.input_disabled);
    assert_eq!(disabled.cine.input_seq, 1);
    let enabled = run(
        vec![act("DISABLE_INPUT", vec![]), act("ENABLE_INPUT", vec![])],
        &[],
        2,
    );
    assert!(!enabled.cine.input_disabled);
    assert_eq!(enabled.cine.input_seq, 1);
}

#[test]
fn the_victory_screen_disables_input_and_asks_for_the_overlay_without_ending_the_mission() {
    let view = run(vec![act("VICTORY_SCREEN", vec![])], &[], 2);
    assert!(view.cine.input_disabled);
    assert_eq!(view.victory_screen_seq, 1);
    assert_eq!(view.mission_end, None);
}

#[test]
fn freeze_time_is_a_flag_and_the_camera_freeze_counts_its_firings() {
    let view = run(
        vec![
            act("FREEZE_TIME", vec![]),
            act("CAMERA_MOD_FREEZE_TIME", vec![]),
        ],
        &[],
        2,
    );
    assert!(view.cine.time_frozen);
    assert_eq!(view.camera_ops.freeze_seq, 1);
    let thawed = run(
        vec![act("FREEZE_TIME", vec![]), act("UNFREEZE_TIME", vec![])],
        &[],
        2,
    );
    assert!(!thawed.cine.time_frozen);
}

#[test]
fn the_newest_caption_and_notification_replace_the_older() {
    let view = run(
        vec![
            act(
                "SHOW_MILITARY_CAPTION",
                vec![param("SCRIPT:First", 0, 0.0), param("", 0, 5.0)],
            ),
            act(
                "SHOW_MILITARY_CAPTION",
                vec![param("SCRIPT:Second", 0, 0.0), param("", 0, 6.0)],
            ),
            act(
                "DISPLAY_NOTIFICATION_BOX",
                vec![
                    param("Hint", 0, 0.0),
                    param("SCRIPT:One", 0, 0.0),
                    param("", 3, 0.0),
                ],
            ),
            act(
                "DISPLAY_NOTIFICATION_BOX",
                vec![
                    param("Hint", 0, 0.0),
                    param("SCRIPT:Two", 0, 0.0),
                    param("", 5, 0.0),
                ],
            ),
        ],
        &[],
        2,
    );
    let caption = view.cine.caption.expect("a caption");
    assert_eq!(
        (caption.label.as_str(), caption.ms, caption.seq),
        ("SCRIPT:Second", 6000, 2)
    );
    let note = view.cine.notification.expect("a notification");
    assert_eq!(
        (note.label.as_str(), note.ms, note.seq),
        ("SCRIPT:Two", 5000, 2)
    );
}

#[test]
fn objectives_are_numbered_over_one_list_and_announced_once() {
    let objectives = [("PrimaryA", false), ("PrimaryB", false), ("BonusC", true)];
    let n = |value| vec![param("", value, 0.0)];
    let view = run(
        vec![
            act("SHOW_MISSION_OBJECTIVE", n(2)),
            act("SHOW_MISSION_OBJECTIVE", n(2)),
            act("SHOW_MISSION_OBJECTIVE", n(3)),
            act("MARK_MISSION_OBJECTIVE_COMPLETED", n(3)),
            act("SHOW_MISSION_OBJECTIVE", n(9)),
        ],
        &objectives,
        2,
    );
    let shown: Vec<_> = view
        .cine
        .objectives
        .iter()
        .map(|o| (o.tag.as_str(), o.shown, o.completed, o.bonus))
        .collect();
    assert_eq!(
        shown,
        [
            ("PrimaryA", false, false, false),
            ("PrimaryB", true, false, false),
            ("BonusC", true, true, true)
        ]
    );
    let added = view
        .cine
        .objective_sounds
        .iter()
        .filter(|s| s.kind == crate::rules::map_scripts::ObjectiveSoundKind::Added)
        .count();
    assert_eq!(added, 2, "a second SHOW of a shown entry is silent");
}

#[test]
fn a_completed_objective_is_silent_in_the_first_logic_frames() {
    use crate::rules::map_scripts::{CineState, ObjectiveSoundKind};
    let sounds = |frame| {
        let mut cine = CineState::with_objectives(&[("T".to_owned(), false)]);
        cine.complete_objective(1, true, frame);
        cine.complete_objective(1, true, frame);
        cine.objective_sounds
            .iter()
            .filter(|s| s.kind == ObjectiveSoundKind::Completed)
            .count()
    };
    assert_eq!(sounds(2), 0);
    assert_eq!(sounds(10), 1);
}

#[test]
fn radar_events_carry_their_position_and_the_team_position_is_the_member_mean() {
    let at = Parameter::Coord3D {
        x: 5.0,
        y: 6.0,
        z: 0.0,
    };
    let view = run(
        vec![
            act("RADAR_CREATE_EVENT", vec![at, param("", 3, 0.0)]),
            act("REFRESH_RADAR", vec![]),
        ],
        &[],
        2,
    );
    assert_eq!(view.cine.radar.len(), 1);
    assert_eq!(
        (
            view.cine.radar[0].at.x,
            view.cine.radar[0].at.y,
            view.cine.radar[0].kind
        ),
        (5.0, 6.0, 3)
    );
    assert_eq!(view.cine.radar_refresh_seq, 1);
    let mean =
        crate::rules::map_scripts::mean_position(&[Vec2::new(0.0, 0.0), Vec2::new(10.0, 4.0)]);
    assert_eq!(mean.map(|m| (m.x, m.y)), Some((5.0, 2.0)));
}

#[test]
fn camera_verbs_record_their_commands_and_move_home_is_accepted_as_a_no_op() {
    let view = run(
        vec![
            act(
                "ZOOM_CAMERA",
                vec![
                    param("", 0, 1.5),
                    param("", 0, 2.0),
                    param("", 0, 0.5),
                    param("", 0, 0.25),
                ],
            ),
            act(
                "ROTATE_CAMERA",
                vec![
                    param("", 0, 1.0),
                    param("", 0, 3.0),
                    param("", 0, 0.0),
                    param("", 0, 0.0),
                ],
            ),
            act(
                "RESET_CAMERA",
                vec![
                    param("WpA", 0, 0.0),
                    param("", 0, 1.0),
                    param("", 0, 0.0),
                    param("", 0, 0.0),
                ],
            ),
            act("LOCK_CAMERA", vec![param("", 1, 0.0)]),
            act("MOVE_CAMERA_BY_ANIMATION", vec![param("Flyby", 0, 0.0)]),
            act("CAMERA_MOVE_HOME", vec![]),
        ],
        &[],
        2,
    );
    let zoom = view.camera_ops.zoom.expect("zoom");
    assert_eq!(
        (zoom.target, zoom.ms, zoom.ease_in_ms, zoom.ease_out_ms),
        (1.5, 2000, 500, 250)
    );
    assert_eq!(
        view.camera_ops.rotate.map(|r| (r.target, r.ms)),
        Some((1.0, 3000))
    );
    assert_eq!(
        view.camera_ops.reset.map(|r| (r.target.x, r.target.y)),
        Some((10.0, 20.0))
    );
    assert!(view.camera_ops.lock_all);
    assert_eq!(
        view.camera_ops.animation.map(|a| a.name),
        Some("Flyby".to_owned())
    );
}

#[test]
fn setup_camera_is_an_instant_move_that_looks_toward_its_waypoint() {
    let view = run(
        vec![act(
            "SETUP_CAMERA",
            vec![
                param("WpA", 0, 0.0),
                param("", 0, 1.2),
                param("", 0, 0.8),
                param("WpB", 0, 0.0),
            ],
        )],
        &[],
        2,
    );
    let command = view.camera_move.expect("a move");
    assert_eq!(
        (command.ms, command.zoom, command.pitch),
        (1, Some(1.2), Some(0.8))
    );
    assert_eq!(command.look_toward.map(|p| (p.x, p.y)), Some((30.0, 40.0)));
}
