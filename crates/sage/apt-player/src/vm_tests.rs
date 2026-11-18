#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::float_cmp
)]

use std::sync::Arc;

use crate::synth::{op, place, Asm, Frames, Item, Synth};
use crate::{
    clip_event, ColorTransform, Halt, InstanceId, Library, Limits, Outward, Player, Value, VmLimits,
};

fn scripted(s: &Synth, root: &Frames) -> Player {
    scripted_with(s, root, Limits::default())
}

fn scripted_with(s: &Synth, root: &Frames, limits: Limits) -> Player {
    let (c, apt) = s.build(root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    let mut p = Player::new(Arc::new(lib), id, limits).expect("player starts");
    p.set_trace_log(true);
    p
}

fn child(p: &Player, parent: InstanceId, depth: i32) -> InstanceId {
    p.instance(parent).unwrap().child_at(depth).unwrap()
}

fn trace(a: &mut Asm, text: &str) {
    a.push_str(text).op(op::TRACE);
}

fn trace_var(a: &mut Asm, name: &str) {
    a.get_var(name).op(op::TRACE);
}

fn call0(a: &mut Asm, name: &str) {
    a.push_byte(0)
        .push_str(name)
        .op(op::CALL_FUNCTION)
        .op(op::POP);
}

#[test]
fn this_in_frame_code_is_the_clip() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.op(op::PUSH_THIS)
        .push_str("_x")
        .push_byte(5)
        .op(op::SET_MEMBER)
        .end();
    let block = s.code(&mut a);
    let clip = s.sprite(vec![vec![Item::Action(block)]]);
    let mut p = scripted(&s, &vec![vec![Item::Place(place(1, clip).name("c"))]]);
    p.tick().unwrap();
    let c = child(&p, p.root(), 1);
    assert_eq!(p.instance(c).unwrap().x(), 5.0);
    assert_eq!(p.script_stats().this_undefined, 0);
}

#[test]
fn a_function_defined_on_frame_one_runs_from_a_later_press() {
    let mut s = Synth::new();
    let shape = s.shape();
    let mut define = Asm::new();
    define.function("Greet", &[], |b| {
        trace(b, "hello");
    });
    define.end();
    let define = s.code(&mut define);
    let mut press = Asm::new();
    call0(&mut press, "Greet");
    press.end();
    let press = s.code(&mut press);
    let button = s.button(vec![(0x1, shape, 1, 0.0, 0.0)], vec![(0x8, press)]);
    let root = vec![vec![Item::Place(place(1, button)), Item::Action(define)]];
    let mut p = scripted(&s, &root);
    for _ in 0..3 {
        p.tick().unwrap();
    }
    assert!(p.take_traces().is_empty());
    let b = child(&p, p.root(), 1);
    assert_eq!(p.queue_button_actions(b, 0x8).unwrap(), 1);
    p.run_actions().unwrap();
    assert_eq!(p.take_traces(), vec!["hello".to_owned()]);
}

#[test]
fn names_are_case_insensitive() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![]]);
    let mut a = Asm::new();
    a.get_var("openbutton")
        .push_str("_x")
        .push_byte(9)
        .op(op::SET_MEMBER);
    a.push_str("Foo").push_byte(3).op(op::SET_VARIABLE);
    trace_var(&mut a, "FOO");
    a.end();
    let block = s.code(&mut a);
    let root = vec![vec![
        Item::Place(place(1, clip).name("OpenButton")),
        Item::Action(block),
    ]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    assert_eq!(p.instance(child(&p, p.root(), 1)).unwrap().x(), 9.0);
    assert_eq!(p.take_traces(), vec!["3".to_owned()]);
}

#[test]
fn a_function_closes_over_the_clip_that_defined_it() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.push_str("secret").push_str("a").op(op::SET_VARIABLE);
    a.function("Reveal", &[], |b| trace_var(b, "secret"));
    a.op(op::PUSH_GLOBAL)
        .push_str("reveal")
        .get_var("Reveal")
        .op(op::SET_MEMBER);
    a.end();
    let a = s.code(&mut a);
    let mut b = Asm::new();
    call0(&mut b, "reveal");
    b.end();
    let b = s.code(&mut b);
    let clip_a = s.sprite(vec![vec![Item::Action(a)]]);
    let clip_b = s.sprite(vec![vec![Item::Action(b)]]);
    let root = vec![vec![
        Item::Place(place(1, clip_a)),
        Item::Place(place(2, clip_b)),
    ]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["a".to_owned()]);
}

#[test]
fn a_function_reached_by_path_gets_that_object_as_this() {
    let mut s = Synth::new();
    let mut k = Asm::new();
    k.function("Who", &[], |b| {
        b.op(op::PUSH_THIS)
            .push_str("_name")
            .op(op::GET_MEMBER)
            .op(op::TRACE);
    });
    k.end();
    let k = s.code(&mut k);
    let kid = s.sprite(vec![vec![Item::Action(k)]]);
    let mut r = Asm::new();
    call0(&mut r, "Kid.Who");
    r.end();
    let r = s.code(&mut r);
    let root = vec![vec![
        Item::Place(place(1, kid).name("Kid")),
        Item::Action(r),
    ]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["Kid".to_owned()]);
}

#[test]
fn a_blank_name_call_method_calls_the_object() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.function("Hit", &[], |b| trace(b, "hit"));
    a.push_byte(0)
        .get_var("Hit")
        .push_str("")
        .op(op::CALL_METHOD)
        .op(op::POP);
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["hit".to_owned()]);
}

#[test]
fn a_goto_is_visible_on_the_next_line() {
    let mut s = Synth::new();
    let late = s.sprite(vec![vec![]]);
    let mut a = Asm::new();
    a.goto_frame(2);
    trace_var(&mut a, "late");
    a.end();
    let a = s.code(&mut a);
    let clip = s.sprite(vec![
        vec![Item::Action(a)],
        vec![],
        vec![Item::Place(place(1, late).name("late"))],
    ]);
    let mut p = scripted(&s, &vec![vec![Item::Place(place(1, clip).name("c"))]]);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["_level0.c.late".to_owned()]);
}

#[test]
fn get_and_set_property_use_the_swf4_index_table() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![]]);
    let mut a = Asm::new();
    a.push_str("c")
        .push_byte(0)
        .push_byte(7)
        .op(op::SET_PROPERTY);
    a.push_str("c")
        .push_byte(6)
        .push_byte(50)
        .op(op::SET_PROPERTY);
    a.push_str("c")
        .push_byte(0)
        .op(op::GET_PROPERTY)
        .op(op::TRACE);
    a.push_str("c")
        .push_byte(13)
        .op(op::GET_PROPERTY)
        .op(op::TRACE);
    a.end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(1, clip).name("c")), Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let c = p.instance(child(&p, p.root(), 1)).unwrap();
    assert_eq!(c.x(), 7.0);
    assert_eq!(c.alpha(), 50.0);
    assert_eq!(p.take_traces(), vec!["7".to_owned(), "c".to_owned()]);
}

#[test]
fn queued_scripts_run_initialize_construct_then_the_rest() {
    let mut s = Synth::new();
    let mut blocks = Vec::new();
    for text in ["init", "construct", "load", "frame"] {
        let mut a = Asm::new();
        trace(&mut a, text);
        a.end();
        blocks.push(s.code(&mut a));
    }
    let clip = s.sprite(vec![vec![Item::Action(blocks[3])]]);
    let events = vec![
        (clip_event::LOAD, blocks[2]),
        (clip_event::CONSTRUCT, blocks[1]),
        (clip_event::INITIALIZE, blocks[0]),
    ];
    let mut p = scripted(&s, &vec![vec![Item::Place(place(1, clip).events(events))]]);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["init", "construct", "load", "frame"]);
}

#[test]
fn garbage_is_collected_and_the_heap_stays_flat() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    let (top, out) = (a.label(), a.label());
    a.push_str("i").push_byte(0).op(op::SET_VARIABLE);
    a.bind(top);
    a.get_var("i")
        .push_float(1000.0)
        .op(op::NEW_LESS)
        .op(op::NOT)
        .branch(op::BRANCH_IF_TRUE, out);
    a.push_byte(0).push_str("Object").op(op::NEW).op(op::POP);
    a.push_str("i")
        .get_var("i")
        .op(op::INCREMENT)
        .op(op::SET_VARIABLE);
    a.branch(op::BRANCH, top);
    a.bind(out).end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)], vec![]]);
    let mut sizes = Vec::new();
    for tick in 1..=60 {
        p.tick().unwrap();
        if tick == 20 || tick == 60 {
            p.collect_garbage();
            sizes.push(p.heap_stats().objects);
        }
    }
    assert_eq!(p.script_stats().blocks_completed, 30);
    assert!(p.heap_stats().collections > 2, "the debt trigger fired");
    assert_eq!(sizes[0], sizes[1], "flat: {sizes:?}");
    assert!(p.heap_stats().object_slots < 3000, "slots are reused");
}

#[test]
fn a_reachable_object_survives_collection() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.op(op::PUSH_GLOBAL).push_str("keep");
    a.push_byte(0).push_str("Object").op(op::NEW);
    a.op(op::SET_MEMBER);
    a.op(op::PUSH_GLOBAL).push_str("keep").op(op::GET_MEMBER);
    a.push_str("v").push_byte(42).op(op::SET_MEMBER).end();
    let a = s.code(&mut a);
    let mut r = Asm::new();
    r.op(op::PUSH_GLOBAL)
        .push_str("keep")
        .op(op::GET_MEMBER)
        .push_str("v")
        .op(op::GET_MEMBER);
    r.op(op::TRACE).end();
    let r = s.code(&mut r);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)], vec![Item::Action(r)]]);
    p.tick().unwrap();
    for _ in 0..3 {
        p.collect_garbage();
    }
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["42".to_owned()]);
}

#[test]
fn the_object_cap_halts_the_action_and_nothing_else() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    let (top, out) = (a.label(), a.label());
    a.push_str("i").push_byte(0).op(op::SET_VARIABLE);
    a.bind(top);
    a.get_var("i")
        .push_float(1000.0)
        .op(op::NEW_LESS)
        .op(op::NOT)
        .branch(op::BRANCH_IF_TRUE, out);
    a.push_byte(0).push_str("Object").op(op::NEW).op(op::POP);
    a.push_str("i")
        .get_var("i")
        .op(op::INCREMENT)
        .op(op::SET_VARIABLE);
    a.branch(op::BRANCH, top);
    a.bind(out).end();
    let a = s.code(&mut a);
    let limits = Limits {
        vm: VmLimits {
            max_objects: 300,
            ..VmLimits::default()
        },
        ..Limits::default()
    };
    let mut p = scripted_with(&s, &vec![vec![Item::Action(a)]], limits);
    p.tick().unwrap();
    assert!(p.script_stats().halts.keys().any(|h| matches!(
        h,
        Halt::Limit {
            what: "script objects"
        }
    )));
    p.tick().unwrap();
}

#[test]
fn fscommand_and_engine_game_code_leave_as_values() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_url("FSCommand:AptTest::Go", "arg");
    a.push_str("p")
        .push_byte(1)
        .push_str("GameCode")
        .op(op::CALL_FUNCTION)
        .op(op::POP);
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    assert_eq!(
        p.take_outward(),
        vec![
            Outward::FsCommand {
                command: "AptTest::Go".to_owned(),
                args: "arg".to_owned()
            },
            Outward::GameCode {
                name: "GameCode".to_owned(),
                args: vec!["p".to_owned()]
            }
        ]
    );
}

#[test]
fn get_extern_and_toggle_extern_answer_from_the_hosts_own_table() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.push_str("answer1");
    a.push_str("Never::Set");
    a.push_byte(1);
    a.push_str("GetExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::SET_VARIABLE);
    a.push_str("answer2");
    a.push_str("AptMpGameRules::ShowChat");
    a.push_byte(1);
    a.push_str("GetExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::SET_VARIABLE);
    a.push_str("AptMpGameRules::ShowChat");
    a.push_byte(1);
    a.push_str("ToggleExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::POP);
    a.push_str("answer3");
    a.push_str("AptMpGameRules::ShowChat");
    a.push_byte(1);
    a.push_str("GetExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::SET_VARIABLE);
    trace_var(&mut a, "answer1");
    trace_var(&mut a, "answer2");
    trace_var(&mut a, "answer3");
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.set_extern("AptMpGameRules::ShowChat", false);
    p.tick().unwrap();
    assert_eq!(
        p.take_traces(),
        vec![
            "undefined".to_owned(),
            "false".to_owned(),
            "true".to_owned(),
        ]
    );
}

#[test]
fn the_trace_hook_fires_for_an_extern_call_with_its_argument_and_result() {
    struct ResetSink;
    impl Drop for ResetSink {
        fn drop(&mut self) {
            crate::trace::set_sink(None);
        }
    }
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink_calls = std::sync::Arc::clone(&calls);
    crate::trace::set_sink(Some(Box::new(move |name, args, result| {
        sink_calls
            .lock()
            .unwrap()
            .push((name.to_owned(), args.to_vec(), result.to_owned()));
    })));
    let _reset = ResetSink;

    const KEY: &str = "CaptureHookTest::TraceHookFiresUnique";
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.push_str(KEY);
    a.push_byte(1);
    a.push_str("GetExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::POP);
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.set_extern(KEY, true);
    p.tick().unwrap();

    let own: Vec<_> = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, args, _)| name == "GetExtern" && args == &[KEY.to_owned()])
        .cloned()
        .collect();
    assert_eq!(
        own,
        vec![(
            "GetExtern".to_owned(),
            vec![KEY.to_owned()],
            "Bool(true)".to_owned(),
        )]
    );
}

#[test]
fn color_set_rgb_recolours_the_clip() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![]]);
    let mut a = Asm::new();
    a.push_str("col");
    a.push_str("c").push_byte(1).push_str("Color").op(op::NEW);
    a.op(op::SET_VARIABLE);
    a.push_float(16_711_680.0)
        .push_byte(1)
        .get_var("col")
        .push_str("setRGB");
    a.op(op::CALL_METHOD).op(op::POP).end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(1, clip).name("c")), Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let c = p.instance(child(&p, p.root(), 1)).unwrap().color();
    assert_eq!(c.add[..3], [1.0, 0.0, 0.0]);
    assert_eq!(c.mult[..3], [0.0, 0.0, 0.0]);
}

#[test]
fn attach_movie_places_the_export_above_the_depth_bias() {
    let mut s = Synth::new();
    let item = s.sprite(vec![vec![]]);
    s.export("Item", item);
    let mut a = Asm::new();
    a.push_byte(3)
        .push_str("item1")
        .push_str("Item")
        .push_byte(3);
    a.op(op::PUSH_THIS)
        .push_str("attachMovie")
        .op(op::CALL_METHOD)
        .op(op::POP);
    trace_var(&mut a, "item1");
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    let id = child(&p, p.root(), 3 + crate::AVM_DEPTH_BIAS);
    let inst = p.instance(id).unwrap();
    assert!(inst.placed_by_script());
    assert_eq!(inst.name(), "item1");
    assert_eq!(p.take_traces(), vec!["_level0.item1".to_owned()]);
}

#[test]
fn an_unload_handler_sees_its_clip_before_removal() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.op(op::PUSH_THIS)
        .push_str("_name")
        .op(op::GET_MEMBER)
        .op(op::TRACE)
        .end();
    let a = s.code(&mut a);
    let clip = s.sprite(vec![vec![]]);
    let root = vec![
        vec![Item::Place(
            place(1, clip)
                .name("gone")
                .events(vec![(clip_event::UNLOAD, a)]),
        )],
        vec![Item::Remove(1)],
        vec![],
    ];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let id = child(&p, p.root(), 1);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["gone".to_owned()]);
    assert!(
        p.instance(id).unwrap().pending_removal(),
        "parked, not gone"
    );
    assert_eq!(p.instance(id).unwrap().depth(), -2);
    p.tick().unwrap();
    assert!(p.instance(id).is_none(), "reclaimed at the next tick");
}

#[test]
fn init_actions_run_before_the_first_frame() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.push_str("ready")
        .push_str("yes")
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Init(a)]]);
    let root = p.root();
    assert_eq!(p.variable(root, "ready"), Value::str("yes"));
}

#[test]
fn an_ea_non_pop_call_assigns_the_named_variable() {
    let mut s = Synth::new();
    let seven = s.text("Seven");
    let mut a = Asm::new();
    a.constant_pool(&[seven]);
    a.function("Seven", &[], |b| {
        b.push_byte(7).op(op::RETURN);
    });
    a.push_str("x").push_byte(0).call_named_fn(0);
    trace_var(&mut a, "x");
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["7".to_owned()]);
    assert_eq!(
        p.script_stats().residue_blocks,
        0,
        "nothing left on the stack"
    );
}

#[test]
fn define_function2_reads_its_flags_from_the_high_half() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.function2("F", 3 | (0x40 << 16), &[(2, "arg")], |b| {
        b.push_reg(1).op(op::TRACE).push_reg(2).op(op::TRACE);
    });
    a.push_str("value")
        .push_byte(1)
        .push_str("F")
        .op(op::CALL_FUNCTION)
        .op(op::POP);
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    assert_eq!(
        p.take_traces(),
        vec!["_level0".to_owned(), "value".to_owned()]
    );
}

#[test]
fn set_interval_runs_on_the_tick_clock() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.function("Bump", &[], |b| {
        b.push_str("n")
            .get_var("n")
            .op(op::INCREMENT)
            .op(op::SET_VARIABLE);
    });
    a.push_str("n").push_byte(0).op(op::SET_VARIABLE);
    a.push_str("id");
    a.push_float(100.0)
        .get_var("Bump")
        .push_byte(2)
        .push_str("setInterval")
        .op(op::CALL_FUNCTION);
    a.op(op::SET_VARIABLE).end();
    let a = s.code(&mut a);
    let mut stop = Asm::new();
    stop.get_var("id")
        .push_byte(1)
        .push_str("clearInterval")
        .op(op::CALL_FUNCTION)
        .op(op::POP)
        .end();
    let stop = s.code(&mut stop);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    for _ in 0..31 {
        p.tick().unwrap();
    }
    let root = p.root();
    assert_eq!(p.variable(root, "n"), Value::Number(10.0));
    let movie = p.library().movie_id("test").unwrap();
    p.run_block(root, movie, stop).unwrap();
    for _ in 0..30 {
        p.tick().unwrap();
    }
    assert_eq!(p.variable(root, "n"), Value::Number(10.0));
}

#[test]
fn a_bound_text_field_shows_its_variable() {
    let mut s = Synth::new();
    let field = s.bound_text("$Label", "caption");
    let mut a = Asm::new();
    a.push_str("caption")
        .push_str("Hello")
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(1, field))], vec![Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let f = child(&p, p.root(), 1);
    assert_eq!(p.text(f).as_deref(), Some("$Label"));
    p.tick().unwrap();
    assert_eq!(p.text(f).as_deref(), Some("Hello"));
}

#[test]
fn a_bound_localisation_key_is_passed_through_to_the_host() {
    let mut s = Synth::new();
    let field = s.bound_text("999999", "stringName");
    let mut a = Asm::new();
    a.push_str("stringName")
        .push_str("$PlayerRank&dropshadow")
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(1, field))], vec![Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let f = child(&p, p.root(), 1);
    p.tick().unwrap();
    assert_eq!(p.text(f).as_deref(), Some("$PlayerRank&dropshadow"));
}

#[test]
fn a_bound_value_not_starting_with_dollar_still_shows_normally() {
    let mut s = Synth::new();
    let field = s.bound_text("999999", "stringName");
    let mut a = Asm::new();
    a.push_str("stringName")
        .push_str("Gold: 1250")
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(1, field))], vec![Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let f = child(&p, p.root(), 1);
    p.tick().unwrap();
    assert_eq!(p.text(f).as_deref(), Some("Gold: 1250"));
}

#[test]
fn set_target_retargets_timeline_calls_and_variables() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![], vec![]]);
    let mut a = Asm::new();
    a.set_target("c").op(op::STOP);
    a.push_str("_x").push_byte(3).op(op::SET_VARIABLE);
    a.set_target("").end();
    let a = s.code(&mut a);
    let root = vec![
        vec![Item::Place(place(1, clip).name("c")), Item::Action(a)],
        vec![],
    ];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let c = child(&p, p.root(), 1);
    let inst = p.instance(c).unwrap();
    assert_eq!(inst.x(), 3.0);
    assert!(!inst.clip().unwrap().playing);
    assert!(p.instance(p.root()).unwrap().clip().unwrap().playing);
}

#[test]
fn push_data_of_a_number_item_reads_that_register() {
    let mut s = Synth::new();
    let one = s.number(1);
    let mut a = Asm::new();
    a.push_byte(9).set_register(1).op(op::POP);
    a.push_data(&[one]).op(op::TRACE).end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["9".to_owned()]);
}

#[test]
fn a_script_placed_clip_below_the_bias_does_not_survive_a_rewind() {
    let mut s = Synth::new();
    let item = s.sprite(vec![vec![]]);
    s.export("Item", item);
    let depth = 384;
    let mut a = Asm::new();
    a.push_float((depth - crate::AVM_DEPTH_BIAS) as f32)
        .push_str("item")
        .push_str("Item");
    a.push_byte(3)
        .op(op::PUSH_THIS)
        .push_str("attachMovie")
        .op(op::CALL_METHOD)
        .op(op::POP)
        .end();
    let a = s.code(&mut a);
    let root = vec![vec![Item::Place(place(depth, item))], vec![Item::Action(a)]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let attached = child(&p, p.root(), depth);
    assert!(p.instance(attached).unwrap().placed_by_script());
    p.tick().unwrap();
    let after = child(&p, p.root(), depth);
    assert_ne!(after, attached, "recreated by the timeline");
    assert!(!p.instance(after).unwrap().placed_by_script());
}

#[test]
fn a_host_call_runs_a_frame_one_function_and_returns_its_value() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.function("Add", &["a", "b"], |b| {
        b.get_var("a").get_var("b").op(0x0A).op(op::RETURN);
    });
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.tick().unwrap();
    let root = p.root();
    let v = p
        .call(root, "Add", vec![Value::Number(2.0), Value::Number(3.0)])
        .unwrap();
    assert_eq!(v, Value::Number(5.0));
    assert_eq!(p.queued_actions(), 0);
}

#[test]
fn a_host_call_reaches_a_named_child_by_path() {
    let mut s = Synth::new();
    let mut k = Asm::new();
    k.function("Greet", &[], |b| trace(b, "hit"));
    k.end();
    let k = s.code(&mut k);
    let kid = s.sprite(vec![vec![Item::Action(k)]]);
    let root = vec![vec![Item::Place(place(1, kid).name("Kid"))]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let at_root = p.root();
    let v = p.call(at_root, "Kid.Greet", Vec::new()).unwrap();
    assert_eq!(v, Value::Undefined);
    assert_eq!(p.take_traces(), vec!["hit".to_owned()]);
}

#[test]
fn a_host_call_to_nothing_is_undefined_not_an_error() {
    let s = Synth::new();
    let mut p = scripted(&s, &vec![vec![]]);
    p.tick().unwrap();
    let root = p.root();
    let v = p.call(root, "NoSuchFunction", Vec::new()).unwrap();
    assert_eq!(v, Value::Undefined);
}

#[test]
fn world_matrix_composes_nested_placements_root_first() {
    let mut s = Synth::new();
    let leaf_char = s.sprite(vec![vec![]]);
    let mid_char = s.sprite(vec![vec![Item::Place(place(1, leaf_char).at(5.0, 5.0))]]);
    let root_frames = vec![vec![Item::Place(
        place(1, mid_char).matrix([2.0, 0.0, 0.0, 2.0, 10.0, 20.0]),
    )]];
    let mut p = scripted(&s, &root_frames);
    p.tick().unwrap();
    let mid_id = child(&p, p.root(), 1);
    let leaf_id = child(&p, mid_id, 1);
    let m = p.world_matrix(leaf_id).expect("live instance");
    assert_eq!(m.apply(0.0, 0.0), (20.0, 30.0));
}

#[test]
fn world_color_composes_nested_fades_root_first() {
    let mut s = Synth::new();
    let leaf_char = s.sprite(vec![vec![]]);
    let parent_mult = 0x8080_8080u32;
    let parent_add = 0x0000_0040u32;
    let child_mult = 0x4040_4040u32;
    let child_add = 0x0040_0000u32;
    let mid_char = s.sprite(vec![vec![Item::Place(
        place(1, leaf_char).cx(child_mult, child_add),
    )]]);
    let root_frames = vec![vec![Item::Place(
        place(1, mid_char).cx(parent_mult, parent_add),
    )]];
    let mut p = scripted(&s, &root_frames);
    p.tick().unwrap();
    let mid_id = child(&p, p.root(), 1);
    let leaf_id = child(&p, mid_id, 1);

    let parent = ColorTransform::from_apt(parent_mult, parent_add);
    let child_cx = ColorTransform::from_apt(child_mult, child_add);
    let expected = parent.concat(&child_cx);
    let swapped = child_cx.concat(&parent);
    assert_ne!(
        expected.add, swapped.add,
        "the two orders must actually differ, or this test cannot tell them apart"
    );

    let got = p.world_color(leaf_id).expect("live instance");
    assert_eq!(got.mult, expected.mult);
    assert_eq!(got.add, expected.add);
}

fn host_and_child(host_frame: impl FnOnce(&mut Asm), child_frame: impl FnOnce(&mut Asm)) -> Player {
    let mut child = Synth::new();
    let shape = child.shape();
    let mut c = Asm::new();
    child_frame(&mut c);
    c.end();
    let child_code = child.code(&mut c);
    let (cc, capt) = child.build(&vec![vec![
        Item::Place(place(1, shape)),
        Item::Action(child_code),
    ]]);
    let mut host = Synth::new();
    let slot = host.sprite(vec![vec![]]);
    let mut h = Asm::new();
    h.function("Report", &["who"], |b| {
        trace(b, "report");
        trace_var(b, "who");
    });
    host_frame(&mut h);
    h.end();
    let host_code = host.code(&mut h);
    let (hc, hapt) = host.build(&vec![vec![
        Item::Place(place(1, slot).name("slot")),
        Item::Action(host_code),
    ]]);
    let mut lib = Library::new();
    let host_id = lib.add("host", &hc, hapt).unwrap();
    lib.add("child", &cc, capt).unwrap();
    lib.link();
    let mut p = Player::new(Arc::new(lib), host_id, Limits::default()).unwrap();
    p.set_trace_log(true);
    p
}

#[test]
fn get_url_into_a_clip_loads_the_movie_there_with_the_hosts_root() {
    let mut p = host_and_child(
        |h| {
            h.get_url("Child.swf", "_level0.slot");
        },
        |c| {
            c.op(op::PUSH_THIS)
                .push_byte(1)
                .get_var("_root")
                .push_str("Report")
                .op(op::CALL_METHOD)
                .op(op::POP);
            c.push_str("parent")
                .push_byte(1)
                .get_var("_parent")
                .push_str("Report")
                .op(op::CALL_METHOD)
                .op(op::POP);
        },
    );
    p.tick().unwrap();
    let slot = child(&p, p.root(), 1);
    assert!(
        p.instance(slot).unwrap().child_at(1).is_none(),
        "not before the next tick"
    );
    assert!(p.take_outward().contains(&Outward::GetUrl {
        url: "Child.swf".to_owned(),
        target: "_level0.slot".to_owned()
    }));
    p.tick().unwrap();
    assert!(
        p.instance(slot).unwrap().child_at(1).is_some(),
        "the child's shape"
    );
    assert_eq!(
        p.take_traces(),
        vec!["report", "_level0.slot", "report", "parent"]
    );
    assert_eq!(p.loaded_movies(), vec![(slot, "child".to_owned())]);
    assert_eq!(p.counters().movies_loaded, 1);
}

#[test]
fn load_movie_targets_its_clip_and_unload_empties_it() {
    let mut p = host_and_child(
        |h| {
            h.push_str("child.swf")
                .push_byte(1)
                .get_var("slot")
                .push_str("loadMovie")
                .op(op::CALL_METHOD)
                .op(op::POP);
        },
        |_| {},
    );
    p.tick().unwrap();
    assert!(p.take_outward().contains(&Outward::GetUrl {
        url: "child.swf".to_owned(),
        target: "_level0.slot".to_owned()
    }));
    p.tick().unwrap();
    let slot = child(&p, p.root(), 1);
    assert!(p.instance(slot).unwrap().child_at(1).is_some());
    p.call(p.root(), "slot.unloadMovie", Vec::new()).unwrap();
    p.tick().unwrap();
    assert!(p.instance(slot).unwrap().child_at(1).is_none());
    assert!(p.loaded_movies().is_empty());

    let mut q = host_and_child(
        |h| {
            h.get_url("absent.swf", "_level0.slot");
        },
        |_| {},
    );
    q.tick().unwrap();
    q.tick().unwrap();
    assert_eq!(q.counters().movies_loaded, 0);
    assert_eq!(q.counters().loads_refused, 1);
}

#[test]
fn extern_members_are_what_the_host_set() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_var("extern")
        .push_str("InGame")
        .op(op::GET_MEMBER)
        .op(op::TRACE)
        .end();
    let block = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(block)]]);
    p.set_extern_value("InGame", Value::Bool(true));
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["true".to_owned()]);
}

struct CountingHost(std::rc::Rc<std::cell::Cell<i32>>);

impl crate::SyncHost for CountingHost {
    fn command(&mut self, outward: &Outward) -> bool {
        match outward {
            Outward::FsCommand { command, .. } if command == "AptTest::Inc" => {
                self.0.set(self.0.get() + 1);
                true
            }
            _ => false,
        }
    }

    fn get_extern(&mut self, name: &str) -> Option<Value> {
        (name == "v").then(|| Value::Number(f64::from(self.0.get())))
    }
}

#[test]
fn a_sync_host_answers_commands_and_externs_inside_the_action() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_url("FSCommand:AptTest::Inc", "");
    a.get_url("FSCommand:AptTest::Other", "");
    a.push_str("t");
    a.push_str("v");
    a.push_byte(1);
    a.push_str("GetExtern");
    a.op(op::CALL_FUNCTION);
    a.op(op::SET_VARIABLE);
    trace_var(&mut a, "t");
    a.end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(a)]]);
    p.set_sync_host(Some(Box::new(CountingHost(Default::default()))));
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["1".to_owned()]);
    assert_eq!(
        p.take_outward(),
        vec![Outward::FsCommand {
            command: "AptTest::Other".to_owned(),
            args: String::new()
        }]
    );
}

struct RecordingHost(std::rc::Rc<std::cell::RefCell<Vec<(String, Value)>>>);

impl crate::SyncHost for RecordingHost {
    fn command(&mut self, _outward: &Outward) -> bool {
        false
    }

    fn get_extern(&mut self, _name: &str) -> Option<Value> {
        None
    }

    fn set_extern(&mut self, name: &str, value: &Value) -> bool {
        self.0.borrow_mut().push((name.to_owned(), value.clone()));
        true
    }
}

#[test]
fn a_member_write_on_extern_reaches_the_sync_host() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_var("extern")
        .push_str("SuppressX")
        .push_byte(1)
        .op(op::SET_MEMBER)
        .end();
    let block = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(block)]]);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    p.set_sync_host(Some(Box::new(RecordingHost(seen.clone()))));
    p.tick().unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![("SuppressX".to_owned(), Value::Number(1.0))]
    );
}

fn extern_member_trace(register: Option<&str>, name: &str) -> Vec<String> {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_var("extern")
        .push_str(name)
        .op(op::GET_MEMBER)
        .op(op::TRACE)
        .end();
    let block = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(block)]]);
    if let Some(register) = register {
        p.register_native_extern(register);
    }
    p.tick().unwrap();
    p.take_traces()
}

#[test]
fn an_extern_query_answers_extern_for_a_registered_name_and_zero_otherwise() {
    assert_eq!(extern_member_trace(None, "?PickerColor"), vec!["0"]);
    assert_eq!(
        extern_member_trace(Some("PickerColor"), "?PickerColor"),
        vec!["extern"]
    );
}

#[test]
fn writes_to_a_registered_native_extern_are_kept_for_the_component() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    for (name, value) in [("PickerCursor", "3 4"), ("Other", "9")] {
        a.get_var("extern")
            .push_str(name)
            .push_str(value)
            .op(op::SET_MEMBER);
    }
    a.end();
    let block = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![Item::Action(block)]]);
    p.register_native_extern("PickerCursor");
    p.tick().unwrap();
    assert_eq!(
        p.take_extern_writes(),
        vec![(Arc::from("PickerCursor"), Value::str("3 4"))]
    );
}

#[test]
fn an_assigned_on_enter_frame_runs_each_frame_until_cleared() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.function("Beat", &[], |b| {
        b.push_str("beats").op(op::TRACE);
    });
    a.push_str("onEnterFrame")
        .get_var("Beat")
        .op(op::SET_VARIABLE)
        .end();
    let a = s.code(&mut a);
    let mut clear = Asm::new();
    clear
        .push_str("onEnterFrame")
        .get_var("nothing")
        .op(op::SET_VARIABLE)
        .end();
    let clear = s.code(&mut clear);
    let mut p = scripted(
        &s,
        &vec![
            vec![Item::Action(a)],
            vec![],
            vec![Item::Action(clear)],
            vec![],
            vec![],
        ],
    );
    for _ in 0..5 {
        p.tick().unwrap();
    }
    assert_eq!(p.take_traces().len(), 2);
}

#[test]
fn xmouse_and_ymouse_read_the_pointer_the_host_moved() {
    let mut s = Synth::new();
    let mut a = Asm::new();
    a.get_var("_xmouse").op(op::TRACE);
    a.get_var("_ymouse").op(op::TRACE).end();
    let a = s.code(&mut a);
    let mut p = scripted(&s, &vec![vec![], vec![Item::Action(a)]]);
    p.tick().unwrap();
    p.pointer_move(30.0, 40.0).unwrap();
    p.tick().unwrap();
    assert_eq!(p.take_traces(), vec!["30".to_owned(), "40".to_owned()]);
}
