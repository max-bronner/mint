use super::*;
use mint_sage_map::{OrCondition, ScriptGroup};

fn value(int_value: i32, real_value: f32, string_value: &str) -> Parameter {
    Parameter::Value {
        param_type: 0,
        int_value,
        real_value,
        string_value: string_value.to_string(),
    }
}

fn s(text: &str) -> Parameter {
    value(0, 0.0, text)
}

fn i(n: i32) -> Parameter {
    value(n, 0.0, "")
}

fn r(x: f32) -> Parameter {
    value(0, x, "")
}

fn cond(name: &str, params: Vec<Parameter>) -> Condition {
    Condition {
        condition_type: 0,
        inverted: false,
        enabled: true,
        name: name.to_string(),
        params,
    }
}

fn act(name: &str, params: Vec<Parameter>) -> ScriptAction {
    ScriptAction {
        action_type: 0,
        enabled: true,
        name: name.to_string(),
        params,
    }
}

fn script(name: &str, ands: Vec<Condition>, actions: Vec<ScriptAction>) -> Script {
    Script {
        name: name.to_string(),
        comment: String::new(),
        condition_comment: String::new(),
        action_comment: String::new(),
        active: true,
        one_shot: false,
        easy: true,
        normal: true,
        hard: true,
        subroutine: false,
        delay_evaluation_seconds: 0,
        or_conditions: vec![OrCondition { conditions: ands }],
        actions,
        actions_false: Vec::new(),
        sequence: Default::default(),
    }
}

fn library(nodes: Vec<ScriptNode>) -> MusicScripts {
    MusicScripts::new(
        ScriptTree {
            player_slots: vec![nodes],
        },
        ClientRng::seeded(7),
    )
}

struct World {
    side: &'static str,
    units: Vec<[f32; 2]>,
    eva: Option<(u32, [f32; 2])>,
}

impl MusicWorld for World {
    fn local_side(&self) -> Option<&str> {
        Some(self.side)
    }
    fn home_base(&self, _who: Who) -> Option<[f32; 2]> {
        Some([0.0, 0.0])
    }
    fn units_beyond(&self, _who: Who, at: [f32; 2], distance: f32) -> u32 {
        let far = self
            .units
            .iter()
            .filter(|u| {
                let (dx, dy) = (u[0] - at[0], u[1] - at[1]);
                (dx * dx + dy * dy).sqrt() >= distance
            })
            .count();
        u32::try_from(far).unwrap_or(u32::MAX)
    }
    fn has_science(&self, _who: Who, _science: &str) -> bool {
        false
    }
    fn objects_with_condition(&self, _who: Who, _condition: &str) -> Option<u32> {
        None
    }
    fn units_near(&self, _who: Who, at: [f32; 2], distance: f32) -> u32 {
        let near = self
            .units
            .iter()
            .filter(|u| {
                let (dx, dy) = (u[0] - at[0], u[1] - at[1]);
                (dx * dx + dy * dy).sqrt() <= distance
            })
            .count();
        u32::try_from(near).unwrap_or(u32::MAX)
    }
    fn eva_last_played(&self, event: &str) -> Option<Option<(u32, [f32; 2])>> {
        (event == "Ev").then_some(self.eva)
    }
    fn reached_level_cap(&self, _who: Who) -> bool {
        false
    }
}

const QUIET: World = World {
    side: "SideA",
    units: Vec::new(),
    eva: None,
};

#[test]
fn an_announcer_event_played_recently_near_enough_units_pushes_the_action_track() {
    let watch = script(
        "Watch",
        vec![
            cond("HAS_EVA_EVENT_PLAYED_IN_LAST_N_SECONDS", vec![s("Ev"), r(1.5)]),
            cond(
                "IS_NUM_OF_UNITS_BELONGING_TO_PLAYER_NEAR_EVA_EVENT_LAST_PLAYED_LOCATION_COMPARISON_INT",
                vec![s("<Local Player>"), r(50.0), s("Ev"), i(3), i(2)],
            ),
        ],
        vec![act(
            "MUSIC_SCRIPT_PUSH_TRACK_FINITE_TIMES_AND_NOTIFY",
            vec![s("Action"), i(1), i(1), i(0), s("Done")],
        )],
    );
    let world = |ago: u32| World {
        side: "SideA",
        units: vec![[100.0, 100.0], [120.0, 90.0], [400.0, 400.0]],
        eva: Some((ago, [110.0, 100.0])),
    };
    let mut scripts = library(vec![ScriptNode::Script(watch)]);
    let pushed = scripts.update(&world(8));
    assert!(
        matches!(pushed.as_slice(), [MusicCommand::Push(_)]),
        "{pushed:?}"
    );
    let mut scripts = library(vec![ScriptNode::Script(script(
        "Late",
        vec![cond(
            "HAS_EVA_EVENT_PLAYED_IN_LAST_N_SECONDS",
            vec![s("Ev"), r(1.5)],
        )],
        vec![act("MUSIC_SCRIPT_POP_MUSIC", vec![i(0), i(0)])],
    ))]);
    assert!(scripts.update(&world(9)).is_empty());
}

#[test]
fn a_zero_timer_set_earlier_in_the_frame_has_expired_and_a_finished_track_raises_its_flag() {
    let mut start = script(
        "Start",
        vec![cond("CONDITION_TRUE", vec![])],
        vec![act("SET_MILLISECOND_TIMER", vec![s("Wait"), r(0.0)])],
    );
    start.one_shot = true;
    let play = script(
        "Play",
        vec![
            cond("TIMER_EXPIRED", vec![s("Wait")]),
            cond("FLAG", vec![s("Done"), i(0)]),
            cond("COUNTER", vec![s("Plays"), i(0), i(1)]),
        ],
        vec![
            act("INCREMENT_COUNTER", vec![i(1), s("Plays")]),
            act(
                "MUSIC_SCRIPT_PLAY_TRACK_FINITE_TIMES_AND_NOTIFY",
                vec![s("TuneA"), i(1), i(0), i(1), s("Done")],
            ),
        ],
    );
    let after = script(
        "After",
        vec![cond("FLAG", vec![s("Done"), i(1)])],
        vec![act("MUSIC_SCRIPT_POP_MUSIC", vec![i(1), i(0)])],
    );
    let mut music = library(vec![
        ScriptNode::Script(start),
        ScriptNode::Script(play),
        ScriptNode::Script(after),
    ]);
    let commands = music.update(&QUIET);
    let [MusicCommand::Replace(request)] = commands.as_slice() else {
        panic!("{commands:?}");
    };
    assert_eq!(
        (
            request.name.as_str(),
            request.times,
            request.fade_in,
            request.notify.as_deref()
        ),
        ("TuneA", Some(1), true, Some("Done"))
    );
    assert!(
        music.update(&QUIET).is_empty(),
        "the counter allows one play"
    );
    music.set_flag("Done", true);
    assert_eq!(
        music.update(&QUIET),
        [MusicCommand::Pop {
            fade_out: true,
            fade_in: false
        }]
    );
}

#[test]
fn a_subroutine_runs_only_when_called_and_an_evaluation_delay_spaces_its_script() {
    let mut sub = script(
        "Sub",
        vec![cond("CONDITION_TRUE", vec![])],
        vec![act("MUSIC_SCRIPT_SET_TRACK", vec![s("TuneB"), i(0), i(0)])],
    );
    sub.subroutine = true;
    let mut caller = script(
        "Caller",
        vec![cond("CONDITION_TRUE", vec![])],
        vec![act("CALL_SUBROUTINE", vec![s("Sub")])],
    );
    caller.delay_evaluation_seconds = 2;
    let mut music = library(vec![ScriptNode::Group(ScriptGroup {
        name: "Folder".to_string(),
        active: true,
        is_subroutine: false,
        children: vec![ScriptNode::Script(sub), ScriptNode::Script(caller)],
    })]);
    let mut calls = 0;
    for _ in 0..(2 * FRAMES_PER_SECOND + 1) {
        calls += music.update(&QUIET).len();
    }
    assert_eq!(
        calls, 2,
        "frame 0 and frame 60, never the subroutine by itself"
    );
}

#[test]
fn faction_and_distance_from_a_found_home_base_read_the_world() {
    let mut find = script(
        "Find",
        vec![cond(
            "SKIRMISH_PLAYER_FACTION",
            vec![s("<Local Player>"), s("SideA")],
        )],
        vec![act(
            "FIND_HOME_BASE_OF_PLAYER",
            vec![s("<Local Player>"), s("Base"), i(0)],
        )],
    );
    find.one_shot = true;
    let far = script(
        "Far",
        vec![cond(
            "PLAYER_HAS_NUMBER_UNITS_DISTANCE_FROM_OBJECT",
            vec![s("<Local Player>"), i(3), i(2), r(100.0), s("Base")],
        )],
        vec![act("MUSIC_SCRIPT_SET_TRACK", vec![s("TuneA"), i(0), i(0)])],
    );
    let mut music = library(vec![ScriptNode::Script(find), ScriptNode::Script(far)]);
    let near = World {
        side: "SideA",
        units: vec![[10.0, 0.0], [150.0, 0.0]],
        eva: None,
    };
    assert!(
        music.update(&near).is_empty(),
        "one unit beyond 100, two wanted"
    );
    let spread = World {
        side: "SideA",
        units: vec![[200.0, 0.0], [150.0, 0.0]],
        eva: None,
    };
    assert_eq!(music.update(&spread).len(), 1);
    let mut other = library(vec![]);
    assert!(other.update(&spread).is_empty());
}
