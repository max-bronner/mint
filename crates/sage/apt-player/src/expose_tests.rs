#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use std::sync::Arc;

use crate::expose::{resolve_path, Exposure, ImageChange, ImageKey, MovieEvent, Readiness};
use crate::synth::{op, place, Asm, Frames, Item, Synth};
use crate::{Library, Limits, Player, Value};

fn player(s: &Synth, root: &Frames) -> Player {
    let (c, apt) = s.build(root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    let mut p = Player::new(Arc::new(lib), id, Limits::default()).expect("player starts");
    p.set_trace_log(true);
    p
}

fn define_show(a: &mut Asm) {
    a.function("Show", &["x"], |b| {
        b.get_var("x").op(op::TRACE);
    });
}

fn set_var(a: &mut Asm, name: &str, value: &str) {
    a.push_str(name).push_str(value).op(op::SET_VARIABLE);
}

fn show(arg: &str) -> Vec<Value> {
    vec![Value::str(arg)]
}

fn late_function_movie() -> (Synth, Frames) {
    let mut s = Synth::new();
    let mut a = Asm::new();
    define_show(&mut a);
    a.op(op::STOP).end();
    let a = s.code(&mut a);
    (s, vec![vec![], vec![Item::Action(a)]])
}

#[test]
fn landed_reports_whether_a_call_has_landed() {
    let (s, root) = late_function_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.call("k", "Show", show("a"));
    ex.sync(&mut p);
    assert!(!ex.landed("k"), "no function yet");
    for _ in 0..3 {
        ex.frame(&mut p, 1);
    }
    assert!(ex.landed("k"));
    ex.call("k", "Show", show("b"));
    assert!(!ex.landed("k"), "new arguments wait for the next sync");
    ex.sync(&mut p);
    assert!(ex.landed("k"));
    assert!(!ex.landed("never declared"));
}

#[test]
fn a_call_waits_for_its_function_and_runs_once() {
    let (s, root) = late_function_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.call("k", "Show", show("a"));
    ex.frame(&mut p, 1);
    assert_eq!(
        ex.stats().pending_calls,
        1,
        "Show is not defined after frame 1"
    );
    assert_eq!(ex.stats().calls_issued, 0);
    assert!(p.take_traces().is_empty());
    ex.frame(&mut p, 1);
    assert_eq!(p.take_traces(), vec!["a".to_owned()]);
    assert_eq!(ex.stats().pending_calls, 0);
    for _ in 0..3 {
        ex.call("k", "Show", show("a"));
        ex.frame(&mut p, 1);
    }
    assert!(
        p.take_traces().is_empty(),
        "a steady declaration calls nothing"
    );
    assert_eq!(ex.stats().calls_issued, 1);
}

#[test]
fn changed_arguments_re_issue_the_call() {
    let (s, root) = late_function_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.call("k", "Show", show("a"));
    ex.frame(&mut p, 2);
    ex.call("k", "Show", show("b"));
    ex.frame(&mut p, 1);
    ex.call("k", "Show", show("b"));
    ex.frame(&mut p, 1);
    assert_eq!(p.take_traces(), vec!["a".to_owned(), "b".to_owned()]);
}

fn handshake_movie(announce: &[usize], frames: usize) -> (Synth, Frames) {
    let mut s = Synth::new();
    let mut root: Frames = vec![vec![]; frames];
    let mut define = Asm::new();
    define_show(&mut define);
    define.end();
    root[0].push(Item::Action(s.code(&mut define)));
    for &f in announce {
        let mut a = Asm::new();
        a.get_url("FSCommand:AptTest::Ready", "").end();
        root[f].push(Item::Action(s.code(&mut a)));
    }
    let mut stop = Asm::new();
    stop.op(op::STOP).end();
    root[frames - 1].push(Item::Action(s.code(&mut stop)));
    (s, root)
}

#[test]
fn nothing_lands_before_the_handshake() {
    let (s, root) = handshake_movie(&[], 3);
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Event("Ready".to_owned()));
    ex.call("k", "Show", show("a"));
    for _ in 0..3 {
        ex.frame(&mut p, 1);
    }
    assert!(!ex.is_ready());
    assert!(p.take_traces().is_empty());
    assert_eq!(ex.stats().pending_calls, 1);

    let (s, root) = handshake_movie(&[0], 3);
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Event("Ready".to_owned()));
    ex.call("k", "Show", show("a"));
    ex.frame(&mut p, 1);
    assert!(ex.is_ready());
    assert_eq!(
        p.take_traces(),
        vec!["a".to_owned()],
        "lands the same frame"
    );
}

#[test]
fn a_re_announcement_re_issues_every_call() {
    let (s, root) = handshake_movie(&[0, 2], 3);
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Event("Ready".to_owned()));
    for _ in 0..3 {
        ex.call("k", "Show", show("a"));
        ex.frame(&mut p, 1);
    }
    assert_eq!(p.take_traces(), vec!["a".to_owned(), "a".to_owned()]);
}

fn late_clip_movie() -> (Synth, Frames) {
    let mut s = Synth::new();
    let mut first = Asm::new();
    set_var(&mut first, "label", "$A");
    first.end();
    let first = s.code(&mut first);
    let mut second = Asm::new();
    set_var(&mut second, "label", "$B");
    second.op(op::STOP).end();
    let second = s.code(&mut second);
    let clip = s.sprite(vec![vec![Item::Action(first)], vec![Item::Action(second)]]);
    let mut stop = Asm::new();
    stop.op(op::STOP).end();
    let stop = s.code(&mut stop);
    let root = vec![
        vec![],
        vec![Item::Place(place(1, clip).name("box")), Item::Action(stop)],
    ];
    (s, root)
}

#[test]
fn a_value_waits_for_its_clip_and_lands_the_frame_it_appears() {
    let (s, root) = late_clip_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.value("box", "label", Value::str("host"));
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().pending_values, 1);
    ex.frame(&mut p, 1);
    assert_eq!(ex.stats().pending_values, 0);
    let root_id = p.root();
    assert_eq!(p.variable(root_id, "box.label"), Value::str("host"));
}

#[test]
fn a_value_the_movie_overwrites_is_re_asserted() {
    let (s, root) = late_clip_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    for _ in 0..5 {
        ex.value("box", "label", Value::str("host"));
        ex.frame(&mut p, 1);
    }
    let root_id = p.root();
    assert_eq!(p.variable(root_id, "box.label"), Value::str("host"));
    assert!(ex.stats().reasserted >= 1, "{:?}", ex.stats());
}

fn recreated_clip_movie() -> (Synth, Frames) {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![]]);
    let mut stop = Asm::new();
    stop.op(op::STOP).end();
    let stop = s.code(&mut stop);
    let root = vec![
        vec![Item::Place(place(1, clip).name("box"))],
        vec![Item::Remove(1)],
        vec![Item::Place(place(1, clip).name("box")), Item::Action(stop)],
    ];
    (s, root)
}

#[test]
fn a_re_created_clip_gets_its_value_again() {
    let (s, root) = recreated_clip_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    for _ in 0..4 {
        ex.value("box", "label", Value::str("host"));
        ex.frame(&mut p, 1);
    }
    let root_id = p.root();
    assert_eq!(p.variable(root_id, "box.label"), Value::str("host"));
    assert_eq!(ex.stats().values_written, 2);
}

#[test]
fn a_path_image_follows_a_re_created_clip() {
    let (s, root) = recreated_clip_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<u8> = Exposure::new(Readiness::Immediate);
    ex.image(ImageKey::Path("box".to_owned()), Some(7));
    ex.frame(&mut p, 1);
    let first = ex.take_image_changes();
    let [ImageChange::Instance {
        instance: old,
        content: Some(7),
    }] = first[..]
    else {
        panic!("{first:?}")
    };
    ex.frame(&mut p, 1);
    ex.frame(&mut p, 1);
    let changes = ex.take_image_changes();
    let new = resolve_path(&p, "box").unwrap();
    assert_ne!(old, new);
    assert_eq!(
        changes,
        vec![
            ImageChange::Instance {
                instance: old,
                content: None
            },
            ImageChange::Instance {
                instance: new,
                content: Some(7)
            },
        ]
    );
}

#[test]
fn names_and_strings_report_only_changes() {
    let (s, root) = recreated_clip_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<u8> = Exposure::new(Readiness::Immediate);
    for value in [1, 1, 2] {
        ex.image(ImageKey::Name("RenderX".to_owned()), Some(value));
        ex.string("K", Some(value.to_string()));
        ex.sync(&mut p);
    }
    ex.string("K", None);
    ex.sync(&mut p);
    let images = ex.take_image_changes();
    assert_eq!(images.len(), 2, "{images:?}");
    assert_eq!(
        ex.take_string_changes(),
        vec![
            ("K".to_owned(), Some("1".to_owned())),
            ("K".to_owned(), Some("2".to_owned())),
            ("K".to_owned(), None),
        ]
    );
}

#[test]
fn an_extern_reaches_a_load_time_read() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.push_str("v")
        .push_str("Ns::Flag")
        .push_byte(1)
        .push_str("GetExtern")
        .op(op::CALL_FUNCTION)
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let mut p = player(&s, &vec![vec![Item::Action(a)]]);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.set_extern("Ns::Flag", Value::Bool(true));
    ex.frame(&mut p, 1);
    let root_id = p.root();
    assert_eq!(p.variable(root_id, "v"), Value::Bool(true));
}

#[test]
fn a_global_reaches_a_load_time_read_and_is_re_asserted() {
    let mut s = Synth::new();
    let mut load = Asm::new();
    load.push_str("seen").get_var("_global.InGame");
    load.op(op::SET_VARIABLE).end();
    let load = s.code(&mut load);
    let mut clobber = Asm::new();
    set_var(&mut clobber, "_global.InGame", "no");
    clobber.op(op::STOP).end();
    let clobber = s.code(&mut clobber);
    let root = vec![vec![Item::Action(load)], vec![Item::Action(clobber)]];
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Event("Never".to_owned()));
    ex.global("InGame", Value::Bool(true));
    ex.frame(&mut p, 1);
    let root_id = p.root();
    assert_eq!(p.variable(root_id, "seen"), Value::Bool(true));
    ex.frame(&mut p, 1);
    assert_eq!(p.variable(root_id, "_global.InGame"), Value::Bool(true));
    assert_eq!(ex.stats().values_written, 2, "{:?}", ex.stats());
}

fn recreated_subject_movie() -> (Synth, Frames) {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![]]);
    let mut define = Asm::new();
    define_show(&mut define);
    define.end();
    let define = s.code(&mut define);
    let mut stop = Asm::new();
    stop.op(op::STOP).end();
    let stop = s.code(&mut stop);
    let root = vec![
        vec![
            Item::Place(place(1, clip).name("box")),
            Item::Action(define),
        ],
        vec![Item::Remove(1)],
        vec![Item::Place(place(1, clip).name("box")), Item::Action(stop)],
    ];
    (s, root)
}

#[test]
fn a_subject_bound_call_follows_its_clip() {
    let (s, root) = recreated_subject_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    let mut traces = Vec::new();
    for _ in 0..3 {
        ex.call_on("k", "box", "Show", show("a"));
        ex.frame(&mut p, 1);
        traces.push(p.take_traces());
    }
    assert_eq!(
        traces,
        vec![vec!["a".to_owned()], vec![], vec!["a".to_owned()]],
        "once on the first box, nothing while gone, once on the second"
    );
    for _ in 0..2 {
        ex.call_on("k", "box", "Show", show("a"));
        ex.frame(&mut p, 1);
    }
    assert!(p.take_traces().is_empty(), "steady: nothing repeats");
    assert_eq!(ex.stats().reasserted, 1);
}

#[test]
fn a_subject_bound_call_waits_for_a_missing_clip() {
    let (s, root) = late_function_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    for _ in 0..3 {
        ex.call_on("k", "nowhere", "Show", show("a"));
        ex.frame(&mut p, 1);
    }
    assert!(p.take_traces().is_empty());
    assert_eq!(ex.stats().pending_calls, 1);
}

#[test]
fn events_are_decoded_once() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_url("FSCommand:AptTest::Go", "arg")
        .get_url("Other.swf", "_level0.slot");
    a.push_str("p")
        .push_str("Skirmish")
        .push_byte(2)
        .push_str("GameCode")
        .op(op::CALL_FUNCTION)
        .op(op::POP)
        .end();
    let a = s.code(&mut a);
    let mut p = player(&s, &vec![vec![Item::Action(a)]]);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.frame(&mut p, 1);
    assert_eq!(
        ex.take_events(),
        vec![
            MovieEvent::Command {
                name: "Go".to_owned(),
                qualified: "AptTest::Go".to_owned(),
                args: "arg".to_owned(),
            },
            MovieEvent::Load {
                url: "Other.swf".to_owned(),
                target: "_level0.slot".to_owned(),
            },
            MovieEvent::Command {
                name: "Skirmish".to_owned(),
                qualified: "Skirmish".to_owned(),
                args: "p".to_owned(),
            },
        ]
    );
    assert!(ex.take_events().is_empty());
}

#[test]
fn a_path_names_children_or_depths() {
    let mut s = Synth::new();
    let inner = s.sprite(vec![vec![]]);
    let outer = s.sprite(vec![vec![Item::Place(place(3, inner))]]);
    let mut p = player(&s, &vec![vec![Item::Place(place(1, outer).name("Outer"))]]);
    p.tick().unwrap();
    p.tick().unwrap();
    let outer_id = resolve_path(&p, "outer").unwrap();
    let inner_id = resolve_path(&p, "OUTER.#3").unwrap();
    assert_eq!(p.instance(inner_id).unwrap().parent(), Some(outer_id));
    assert_eq!(resolve_path(&p, "outer.#4"), None);
    assert_eq!(resolve_path(&p, ""), Some(p.root()));
}

fn late_grandchild_movie() -> (Synth, Frames) {
    let mut s = Synth::new();
    let mut define = Asm::new();
    define_show(&mut define);
    define.end();
    let define = s.code(&mut define);
    let inner = s.sprite(vec![vec![]]);
    let mut box_stop = Asm::new();
    box_stop.op(op::STOP).end();
    let box_stop = s.code(&mut box_stop);
    let box_clip = s.sprite(vec![
        vec![],
        vec![],
        vec![
            Item::Place(place(1, inner).name("inner")),
            Item::Action(box_stop),
        ],
    ]);
    let mut root_stop = Asm::new();
    root_stop.op(op::STOP).end();
    let root_stop = s.code(&mut root_stop);
    let root = vec![
        vec![
            Item::Place(place(1, box_clip).name("box")),
            Item::Action(define),
        ],
        vec![Item::Action(root_stop)],
    ];
    (s, root)
}

#[test]
fn a_subject_bound_call_waits_for_the_clip_named_not_an_ancestor() {
    let (s, root) = late_grandchild_movie();
    let mut p = player(&s, &root);
    let mut ex: Exposure<()> = Exposure::new(Readiness::Immediate);
    ex.call_on("child", "box.inner", "Show", show("a"));
    ex.call_on("parent", "box", "Show", show("a"));
    ex.frame(&mut p, 1);
    assert_eq!(
        p.take_traces(),
        vec!["a".to_owned()],
        "only the parent-bound call can land before inner exists"
    );
    assert_eq!(
        ex.stats().pending_calls,
        1,
        "the grandchild-bound call waits"
    );
    ex.frame(&mut p, 1);
    ex.frame(&mut p, 1);
    assert_eq!(
        p.take_traces(),
        vec!["a".to_owned()],
        "the grandchild-bound call lands once inner appears"
    );
    for _ in 0..2 {
        ex.call_on("child", "box.inner", "Show", show("a"));
        ex.call_on("parent", "box", "Show", show("a"));
        ex.frame(&mut p, 1);
    }
    assert!(p.take_traces().is_empty(), "steady: neither repeats");
}

#[test]
fn a_host_goto_rests_on_a_stopping_frame() {
    let mut s = Synth::new();
    let mut stop = Asm::new();
    stop.op(op::STOP).end();
    let stop = s.code(&mut stop);
    let clip = s.sprite(vec![
        vec![Item::Label("_hide"), Item::Action(stop)],
        vec![Item::Label("_show"), Item::Action(stop)],
    ]);
    let mut p = player(&s, &vec![vec![Item::Place(place(1, clip).name("box"))]]);
    p.tick().unwrap();
    let root = p.root();
    p.call_defined(root, "box.gotoAndPlay", show("_show"))
        .unwrap();
    for _ in 0..3 {
        p.tick().unwrap();
    }
    let id = resolve_path(&p, "box").unwrap();
    let Some(crate::InstanceKind::Clip(clip)) = p.instance(id).map(|i| i.kind().clone()) else {
        panic!("box is a clip")
    };
    assert_eq!(clip.current_frame, 2, "rests on _show");
    assert!(!clip.playing);
}
