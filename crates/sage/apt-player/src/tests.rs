#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::float_cmp
)]

use std::sync::Arc;

use crate::synth::{modify, place, replace, Frames, Item, Synth};
use crate::{
    clip_event, ActionSource, ButtonVisual, Error, InstanceId, Library, Limits, Player,
    RenderCommand, RenderList, RenderOptions, Value,
};

fn player(synth: &Synth, root: &Frames) -> Player {
    player_with(synth, root, Limits::default())
}

fn player_with(synth: &Synth, root: &Frames, limits: Limits) -> Player {
    let (c, apt) = synth.build(root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    Player::new_without_scripts(Arc::new(lib), id, limits).expect("player starts")
}

fn child(p: &Player, parent: InstanceId, depth: i32) -> Option<InstanceId> {
    p.instance(parent)?.child_at(depth)
}

fn frame_index(p: &Player, id: InstanceId) -> u16 {
    p.instance(id).unwrap().clip().unwrap().frame_index()
}

fn drain(p: &mut Player) -> Vec<ActionSource> {
    let mut out = Vec::new();
    while let Some(a) = p.pop_action() {
        out.push(a.source);
    }
    out
}

fn ticker(s: &mut Synth, frames: usize) -> u32 {
    s.sprite(vec![vec![]; frames])
}

#[test]
fn a_move_updates_the_same_instance_and_only_the_fields_it_sets() {
    let mut s = Synth::new();
    let shape = s.shape();
    let root = vec![
        vec![Item::Place(
            place(1, shape).at(1.0, 2.0).cx(0xFFFF_FFFF, 0x10),
        )],
        vec![Item::Place(modify(1).at(5.0, 6.0))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let before = child(&p, p.root(), 1).unwrap();
    p.tick().unwrap();
    let after = child(&p, p.root(), 1).unwrap();
    assert_eq!(before, after, "Move modifies, it does not recreate");
    let inst = p.instance(after).unwrap();
    assert_eq!((inst.matrix().tx, inst.matrix().ty), (5.0, 6.0));
    assert_ne!(
        inst.color().add[2],
        0.0,
        "the colour the Move left out stays"
    );
}

#[test]
fn remove_then_place_of_the_same_character_is_a_new_instance_with_a_fresh_playhead() {
    let mut s = Synth::new();
    let clip = ticker(&mut s, 10);
    let root = vec![
        vec![Item::Place(place(1, clip))],
        vec![],
        vec![Item::Remove(1), Item::Place(place(1, clip))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let old = child(&p, p.root(), 1).unwrap();
    assert_eq!(frame_index(&p, old), 1);
    p.tick().unwrap();
    let new = child(&p, p.root(), 1).unwrap();
    assert_ne!(old, new);
    assert!(
        p.instance(old).is_none(),
        "the removed instance is reclaimed"
    );
    assert_eq!(
        frame_index(&p, new),
        0,
        "a new clip starts at its first frame"
    );
}

#[test]
fn replace_swaps_a_shapes_art_and_keeps_its_matrix_and_colour() {
    let mut s = Synth::new();
    let a = s.shape();
    let b = s.shape();
    let root = vec![
        vec![Item::Place(place(1, a).at(7.0, 8.0).cx(0x80FF_FFFF, 0))],
        vec![Item::Place(replace(1, b))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let before = child(&p, p.root(), 1).unwrap();
    p.tick().unwrap();
    let inst = p.instance(before).expect("same instance");
    assert_eq!(inst.placed_id(), b);
    assert_eq!(inst.matrix().tx, 7.0);
    assert!((inst.color().mult[3] - 128.0 / 255.0).abs() < 1e-6);
}

#[test]
fn replace_on_a_clip_keeps_the_clip_and_its_character() {
    let mut s = Synth::new();
    let a = ticker(&mut s, 5);
    let b = ticker(&mut s, 5);
    let root = vec![
        vec![Item::Place(place(1, a))],
        vec![Item::Place(replace(1, b).at(3.0, 0.0))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let inst = p.instance(child(&p, p.root(), 1).unwrap()).unwrap();
    assert_eq!(inst.placed_id(), a, "only shapes and morphs swap art");
    assert_eq!(inst.matrix().tx, 3.0, "the record's properties still apply");
    assert_eq!(p.counters().replace_kept, 1);
}

#[test]
fn a_place_onto_an_occupied_depth_is_refused() {
    let mut s = Synth::new();
    let a = s.shape();
    let b = s.shape();
    let root = vec![vec![Item::Place(place(1, a)), Item::Place(place(1, b))]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let inst = p.instance(child(&p, p.root(), 1).unwrap()).unwrap();
    assert_eq!(inst.placed_id(), a);
    assert_eq!(p.counters().place_refused_occupied, 1);
}

#[test]
fn a_new_clip_shows_its_first_frame_on_the_tick_it_appears_then_advances() {
    let mut s = Synth::new();
    let inner = s.shape();
    let clip = s.sprite(vec![vec![Item::Place(place(1, inner))], vec![], vec![]]);
    let root = vec![vec![Item::Place(place(1, clip))], vec![]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let c = child(&p, p.root(), 1).unwrap();
    assert_eq!(frame_index(&p, c), 0);
    assert!(
        child(&p, c, 1).is_some(),
        "its frame 1 placed its own child"
    );
    p.tick().unwrap();
    assert_eq!(frame_index(&p, c), 1);
}

#[test]
fn looping_keeps_a_clip_whose_ratio_matches_and_recreates_one_that_does_not() {
    let mut s = Synth::new();
    let clip = ticker(&mut s, 50);
    let root = vec![
        vec![
            Item::Place(place(1, clip).ratio(1.0 / 65536.0)),
            Item::Place(place(2, clip).ratio(2.0 / 65536.0)),
        ],
        vec![Item::Place(modify(2).ratio(3.0 / 65536.0))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let kept = child(&p, p.root(), 1).unwrap();
    let changed = child(&p, p.root(), 2).unwrap();
    p.tick().unwrap();
    assert_eq!(frame_index(&p, p.root()), 0);
    assert_eq!(child(&p, p.root(), 1), Some(kept));
    assert_eq!(frame_index(&p, kept), 2, "a survivor keeps playing");
    let recreated = child(&p, p.root(), 2).unwrap();
    assert_ne!(recreated, changed);
    assert_eq!(frame_index(&p, recreated), 0);
}

#[test]
fn a_rewind_keeps_a_shape_button_and_edit_text_whose_record_matches() {
    let mut s = Synth::new();
    let shape = s.shape();
    let art = s.shape();
    let button = s.button(vec![(0x1, art, 1, 0.0, 0.0)], vec![]);
    let text = s.edit_text("t");
    let root = vec![
        vec![
            Item::Place(place(1, shape).at(5.0, 0.0).cx(0x80FF_FFFF, 0)),
            Item::Place(place(2, button)),
            Item::Place(place(3, text)),
        ],
        vec![],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let before: Vec<_> = (1..=3).map(|d| child(&p, p.root(), d).unwrap()).collect();
    p.tick().unwrap();
    p.tick().unwrap();
    assert_eq!(p.counters().loops, 1);
    let after: Vec<_> = (1..=3).map(|d| child(&p, p.root(), d).unwrap()).collect();
    assert_eq!(before, after, "all three survive the rewind");
}

#[test]
fn a_rewind_recreates_a_shape_button_and_edit_text_whose_id_differs() {
    let mut s = Synth::new();
    let (a_shape, b_shape) = (s.shape(), s.shape());
    let art = s.shape();
    let a_button = s.button(vec![(0x1, art, 1, 0.0, 0.0)], vec![]);
    let b_button = s.button(vec![(0x1, art, 1, 0.0, 0.0)], vec![]);
    let (a_text, b_text) = (s.edit_text("a"), s.edit_text("b"));
    let root = vec![
        vec![
            Item::Place(place(1, a_shape)),
            Item::Place(place(2, a_button)),
            Item::Place(place(3, a_text)),
        ],
        vec![
            Item::Remove(1),
            Item::Remove(2),
            Item::Remove(3),
            Item::Place(place(1, b_shape)),
            Item::Place(place(2, b_button)),
            Item::Place(place(3, b_text)),
        ],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let before: Vec<_> = (1..=3).map(|d| child(&p, p.root(), d).unwrap()).collect();
    p.tick().unwrap();
    for (depth, (old, id)) in (1..=3).zip(before.iter().zip([a_shape, a_button, a_text])) {
        let now = child(&p, p.root(), depth).unwrap();
        assert_ne!(now, *old, "depth {depth} must be a new instance");
        assert_eq!(p.instance(now).unwrap().placed_id(), id);
    }
}

#[test]
fn goto_backwards_keeps_surviving_children_and_resets_their_properties() {
    let mut s = Synth::new();
    let shape = s.shape();
    let clip = ticker(&mut s, 100);
    let root = vec![
        vec![Item::Place(place(1, clip)), Item::Place(place(2, shape))],
        vec![Item::Place(modify(1).at(9.0, 9.0))],
        vec![Item::Remove(2)],
    ];
    let mut p = player(&s, &root);
    for _ in 0..3 {
        p.tick().unwrap();
    }
    let c = child(&p, p.root(), 1).unwrap();
    assert_eq!(p.instance(c).unwrap().matrix().tx, 9.0);
    assert!(child(&p, p.root(), 2).is_none());
    let root_id = p.root();
    p.goto_frame(root_id, 1, true).unwrap();
    assert_eq!(
        child(&p, p.root(), 1),
        Some(c),
        "the clip survives the rewind"
    );
    assert_eq!(
        p.instance(c).unwrap().matrix().tx,
        0.0,
        "frame 1's matrix is back"
    );
    assert_eq!(frame_index(&p, c), 2, "its own playhead is untouched");
    assert!(
        child(&p, p.root(), 2).is_some(),
        "the removed shape is placed again"
    );
    p.tick().unwrap();
    assert_eq!(frame_index(&p, p.root()), 0, "gotoAndStop stops");
}

#[test]
fn goto_forward_applies_every_place_and_remove_in_between() {
    let mut s = Synth::new();
    let a = s.shape();
    let b = s.shape();
    let root = vec![
        vec![Item::Place(place(1, a))],
        vec![Item::Place(place(2, b)), Item::Remove(1)],
        vec![Item::Place(modify(2).at(4.0, 0.0))],
        vec![],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let root_id = p.root();
    p.goto_frame(root_id, 3, false).unwrap();
    assert!(child(&p, p.root(), 1).is_none());
    let b_inst = p.instance(child(&p, p.root(), 2).unwrap()).unwrap();
    assert_eq!(b_inst.matrix().tx, 4.0);
    p.tick().unwrap();
    assert_eq!(frame_index(&p, p.root()), 3, "gotoAndPlay plays on");
}

#[test]
fn a_goto_to_the_current_frame_does_nothing() {
    let mut s = Synth::new();
    let clip = ticker(&mut s, 5);
    let root = vec![vec![Item::Place(place(1, clip))], vec![Item::Action(0x40)]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    drain(&mut p);
    let c = child(&p, p.root(), 1).unwrap();
    let root_id = p.root();
    p.goto_frame(root_id, 2, true).unwrap();
    assert_eq!(child(&p, p.root(), 1), Some(c));
    assert!(drain(&mut p).is_empty(), "no frame script re-runs");
}

#[test]
fn goto_label_is_case_insensitive_and_an_unknown_label_moves_nothing() {
    let mut s = Synth::new();
    let clip = ticker(&mut s, 2);
    let root = vec![
        vec![Item::Place(place(1, clip))],
        vec![Item::Label("_Over")],
        vec![],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let root_id = p.root();
    assert!(p.goto_label(root_id, "_OVER", true).unwrap());
    assert_eq!(frame_index(&p, root_id), 1);
    assert!(!p.goto_label(root_id, "_nope", true).unwrap());
    assert_eq!(frame_index(&p, root_id), 1);
}

#[test]
fn clip_events_are_gated_by_their_flags() {
    let mut s = Synth::new();
    let clip = ticker(&mut s, 3);
    let handlers = vec![
        (clip_event::LOAD, 0x100),
        (clip_event::PRESS, 0x200),
        (clip_event::ENTER_FRAME, 0x300),
        (clip_event::INITIALIZE, 0x400),
        (clip_event::CONSTRUCT, 0x500),
    ];
    let root = vec![vec![Item::Place(place(1, clip).events(handlers))], vec![]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    assert_eq!(
        drain(&mut p),
        vec![
            ActionSource::Initialize { block: 0x400 },
            ActionSource::Construct {
                blocks: vec![0x500]
            },
            ActionSource::ClipEvent {
                event: clip_event::LOAD,
                block: 0x100
            },
        ],
        "priority order, and no Press or EnterFrame at placement"
    );
    p.tick().unwrap();
    assert_eq!(
        drain(&mut p),
        vec![ActionSource::ClipEvent {
            event: clip_event::ENTER_FRAME,
            block: 0x300
        }]
    );
    let c = child(&p, p.root(), 1).unwrap();
    p.dispatch_clip_event(c, clip_event::PRESS).unwrap();
    assert_eq!(
        drain(&mut p),
        vec![ActionSource::ClipEvent {
            event: clip_event::PRESS,
            block: 0x200
        }]
    );
}

#[test]
fn frame_scripts_queue_in_record_order_including_a_new_childs_first_frame() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![Item::Action(0xC1)]]);
    let root = vec![vec![
        Item::Action(0xA1),
        Item::Place(place(1, clip)),
        Item::Action(0xA2),
    ]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    assert_eq!(
        drain(&mut p),
        vec![
            ActionSource::Frame { block: 0xA1 },
            ActionSource::Frame { block: 0xC1 },
            ActionSource::Frame { block: 0xA2 },
        ]
    );
}

#[test]
fn an_action_for_a_removed_clip_is_dropped_but_its_unload_handler_is_not() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![Item::Action(0xC1)], vec![]]);
    let root = vec![
        vec![Item::Place(
            place(1, clip).events(vec![(clip_event::UNLOAD, 0xDD)]),
        )],
        vec![Item::Remove(1)],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let got = drain(&mut p);
    assert!(!got.contains(&ActionSource::Frame { block: 0xC1 }));
    assert!(got.contains(&ActionSource::ClipEvent {
        event: clip_event::UNLOAD,
        block: 0xDD
    }));
    assert_eq!(p.counters().actions_dropped_removed, 1);
}

#[test]
fn a_button_shows_only_its_state_records_and_keeps_shared_children() {
    let mut s = Synth::new();
    let up = s.shape();
    let over = s.shape();
    let hit = s.shape();
    let shared = s.shape();
    let button = s.button(
        vec![
            (0x1, up, 1, 0.0, 0.0),
            (0x2, over, 2, 0.0, 0.0),
            (0x8, hit, 3, 0.0, 0.0),
            (0x3, shared, 4, 0.0, 0.0),
        ],
        vec![(0x1, 0xB1), (0x4, 0xB4)],
    );
    let root = vec![vec![Item::Place(place(1, button))]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 1).unwrap();
    let depths = |p: &Player| {
        p.instance(b)
            .unwrap()
            .children()
            .map(|(d, _)| d)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        depths(&p),
        vec![1, 4],
        "Up records, and never the hit record"
    );
    let shared_before = child(&p, b, 4).unwrap();
    p.set_button_state(b, ButtonVisual::Over).unwrap();
    assert_eq!(depths(&p), vec![2, 4]);
    assert_eq!(child(&p, b, 4), Some(shared_before));
    assert_eq!(p.queue_button_actions(b, 0x1).unwrap(), 1);
    let action = p.pop_action().unwrap();
    assert_eq!(
        action.clip,
        p.root(),
        "button actions run on the parent timeline"
    );
}

#[test]
fn masks_bracket_exactly_the_depths_up_to_their_clip_depth() {
    let mut s = Synth::new();
    let mask = s.shape();
    let a = s.shape();
    let b = s.shape();
    let c = s.shape();
    let root = vec![vec![
        Item::Place(place(1, mask).clip_depth(3)),
        Item::Place(place(2, a)),
        Item::Place(place(3, b)),
        Item::Place(place(4, c)),
    ]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let mut list = RenderList::default();
    p.render(&mut list);
    let shape_of = |cmd: &RenderCommand| match cmd {
        RenderCommand::Shape { character, .. } => format!("s{}", character.id),
        other => format!("{other:?}"),
    };
    let got: Vec<String> = list.commands.iter().map(shape_of).collect();
    let want = [
        "PushMask".to_owned(),
        format!("s{mask}"),
        "ActivateMask".to_owned(),
        format!("s{a}"),
        format!("s{b}"),
        "DeactivateMask".to_owned(),
        format!("s{mask}"),
        "PopMask".to_owned(),
        format!("s{c}"),
    ];
    assert_eq!(got, want);
}

#[test]
fn an_invisible_child_is_not_drawn() {
    let mut s = Synth::new();
    let a = s.shape();
    let root = vec![vec![Item::Place(place(1, a))]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let id = child(&p, p.root(), 1).unwrap();
    p.instance_mut(id).unwrap().set_visible(false);
    let mut list = RenderList::default();
    p.render(&mut list);
    assert!(list.commands.is_empty());
}

#[test]
fn transforms_and_colour_concatenate_down_the_tree() {
    let mut s = Synth::new();
    let shape = s.shape();
    let clip = s.sprite(vec![vec![Item::Place(
        place(1, shape).at(1.0, 0.0).cx(0xFFFF_FFFF, 0x0000_0080),
    )]]);
    let root = vec![vec![Item::Place(
        place(1, clip)
            .matrix([2.0, 0.0, 0.0, 2.0, 10.0, 0.0])
            .cx(0x80FF_FFFF, 0),
    )]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let mut list = RenderList::default();
    p.render(&mut list);
    let [RenderCommand::Shape {
        transform, color, ..
    }] = list.commands.as_slice()
    else {
        panic!("one shape: {:?}", list.commands);
    };
    assert_eq!(
        transform.apply(0.0, 0.0),
        (12.0, 0.0),
        "parent scale applies to child tx"
    );
    assert!((color.mult[3] - 128.0 / 255.0).abs() < 1e-6);
    assert!(
        (color.add[2] - 128.0 / 255.0).abs() < 1e-6,
        "child add scaled by parent mult 1.0"
    );
}

#[test]
fn removed_subtrees_are_reclaimed_and_their_handles_go_stale() {
    let mut s = Synth::new();
    let leaf = s.shape();
    let inner = s.sprite(vec![vec![Item::Place(place(1, leaf))]]);
    let outer = s.sprite(vec![vec![Item::Place(place(1, inner))]]);
    let root = vec![vec![Item::Place(place(1, outer))], vec![Item::Remove(1)]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let o = child(&p, p.root(), 1).unwrap();
    let i = child(&p, o, 1).unwrap();
    assert_eq!(p.live_instances(), 4);
    p.tick().unwrap();
    assert_eq!(p.live_instances(), 1, "only the root is left");
    assert!(p.instance(o).is_none() && p.instance(i).is_none());
    assert_eq!(p.counters().reclaimed, 3);
}

#[test]
fn a_self_placing_sprite_stops_at_the_depth_cap_with_an_error() {
    let mut s = Synth::new();
    let me = 1;
    let sprite = s.sprite(vec![vec![Item::Place(place(1, me))]]);
    assert_eq!(sprite, me);
    let root = vec![vec![Item::Place(place(1, sprite))]];
    let limits = Limits {
        max_tree_depth: 10,
        ..Limits::default()
    };
    let mut p = player_with(&s, &root, limits);
    let err = p.tick().unwrap_err();
    assert!(matches!(
        err,
        Error::Limit {
            what: "display tree depth",
            ..
        }
    ));
    assert_eq!(p.live_instances(), 11, "root plus ten levels");
}

#[test]
fn the_instance_cap_is_an_error_and_the_tree_stays_consistent() {
    let mut s = Synth::new();
    let shape = s.shape();
    let root = vec![(1..=8).map(|d| Item::Place(place(d, shape))).collect()];
    let limits = Limits {
        max_instances: 5,
        ..Limits::default()
    };
    let mut p = player_with(&s, &root, limits);
    assert!(matches!(
        p.tick(),
        Err(Error::Limit {
            what: "live instances",
            ..
        })
    ));
    assert_eq!(p.live_instances(), 5);
    assert_eq!(p.instance(p.root()).unwrap().children().count(), 4);
}

#[test]
fn an_imported_character_resolves_to_the_exporting_movie() {
    let mut lib_side = Synth::new();
    let art = lib_side.shape();
    lib_side.export("Art", art);
    let (lc, la) = lib_side.build(&vec![vec![]]);
    let mut s = Synth::new();
    let imported = s.import("library.apt", "art");
    let root = vec![vec![Item::Place(place(1, imported))]];
    let (c, apt) = s.build(&root);
    let mut lib = Library::new();
    let main = lib.add("main", &c, apt).unwrap();
    let other = lib.add("library", &lc, la).unwrap();
    assert!(matches!(
        Player::new(Arc::new(Library::new()), main, Limits::default()),
        Err(Error::Unlinked)
    ));
    let mut empty = Library::new();
    empty.link();
    assert!(matches!(
        Player::new(Arc::new(empty), main, Limits::default()),
        Err(Error::UnknownMovie(_))
    ));
    lib.link();
    let mut p = Player::new(Arc::new(lib), main, Limits::default()).unwrap();
    p.tick().unwrap();
    let inst = p.instance(child(&p, p.root(), 1).unwrap()).unwrap();
    assert_eq!(inst.character().movie, other);
    assert_eq!(inst.character().id, art);
    assert_eq!(
        inst.placed_id(),
        imported,
        "rewind compares the id as placed"
    );
}

#[test]
fn the_first_background_colour_wins() {
    let s = Synth::new();
    let root = vec![vec![Item::Background(0x11)], vec![Item::Background(0x22)]];
    let mut p = player(&s, &root);
    assert_eq!(p.background(), None);
    p.tick().unwrap();
    p.tick().unwrap();
    assert_eq!(p.background(), Some(0x11));
}

#[test]
fn init_actions_queue_once_per_timeline_at_initialize_priority() {
    let mut s = Synth::new();
    let clip = s.sprite(vec![vec![Item::Init(0x1A), Item::Action(0x2A)]]);
    let root = vec![vec![
        Item::Place(place(1, clip)),
        Item::Place(place(2, clip)),
    ]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    assert_eq!(
        drain(&mut p),
        vec![
            ActionSource::InitAction { block: 0x1A },
            ActionSource::Frame { block: 0x2A },
            ActionSource::Frame { block: 0x2A },
        ]
    );
}

#[test]
fn an_edit_text_starts_with_its_literal_text_and_draws_as_text() {
    let mut s = Synth::new();
    let field = s.edit_text("$Label");
    let root = vec![vec![Item::Place(place(1, field))]];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    let id = child(&p, p.root(), 1).unwrap();
    assert_eq!(
        p.instance(id).unwrap().kind(),
        &crate::InstanceKind::EditText {
            text: "$Label".to_owned()
        }
    );
    let mut list = RenderList::default();
    p.render(&mut list);
    assert!(
        matches!(list.commands.as_slice(), [RenderCommand::EditText { instance, .. }] if *instance == id)
    );
}

#[test]
fn a_morph_replaced_by_a_morph_swaps_and_draws_at_its_ratio() {
    let mut s = Synth::new();
    let a = s.morph();
    let b = s.morph();
    let root = vec![
        vec![Item::Place(place(1, a).ratio(0.25))],
        vec![Item::Place(replace(1, b).ratio(0.5))],
    ];
    let mut p = player(&s, &root);
    p.tick().unwrap();
    p.tick().unwrap();
    let mut list = RenderList::default();
    p.render(&mut list);
    assert!(matches!(
        list.commands.as_slice(),
        [RenderCommand::Morph { character, ratio, .. }] if character.id == b && *ratio == 0.5
    ));
}

#[test]
fn truncated_or_corrupted_movies_error_or_play_but_never_panic() {
    let mut s = Synth::new();
    let shape = s.shape();
    let clip = s.sprite(vec![
        vec![Item::Place(place(1, shape).events(vec![(1, 4)]))],
        vec![Item::Remove(1)],
    ]);
    let button = s.button(vec![(0x1, shape, 1, 0.0, 0.0)], vec![(1, 8)]);
    let root = vec![
        vec![
            Item::Place(place(1, clip).name("c")),
            Item::Place(place(2, button)),
            Item::Label("x"),
        ],
        vec![Item::Place(replace(1, shape)), Item::Action(4)],
    ];
    let (c, apt) = s.build(&root);
    let run = |apt: Vec<u8>| {
        let mut lib = Library::new();
        if let Ok(id) = lib.add("m", &c, apt) {
            lib.link();
            if let Ok(mut p) = Player::new(Arc::new(lib), id, Limits::default()) {
                for _ in 0..4 {
                    let _ = p.tick();
                }
                let mut list = RenderList::default();
                p.render(&mut list);
                while p.pop_action().is_some() {}
            }
        }
    };
    for len in 0..apt.len() {
        run(apt[..len].to_vec());
    }
    for i in 0..apt.len() {
        for bit in [0x01u8, 0x80] {
            let mut bad = apt.clone();
            bad[i] ^= bit;
            run(bad);
        }
    }
}

#[test]
fn a_visible_clip_with_a_type_becomes_a_surface_and_hiding_it_removes_it_again() {
    let mut s = Synth::new();
    let shape = s.shape();
    let clip = s.sprite(vec![vec![Item::Place(place(1, shape))]]);
    let root = vec![vec![Item::Place(place(1, clip).name("slot"))]];
    let (c, apt) = s.build(&root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    let mut p =
        Player::new(Arc::new(lib), id, Limits::default()).expect("player starts with scripts");
    p.tick().unwrap();
    let slot = child(&p, p.root(), 1).unwrap();

    p.set_variable(slot, "_visible", Value::Bool(false))
        .unwrap();
    let mut list = RenderList::default();
    p.render_with(&mut list, RenderOptions { surfaces: true });
    assert!(
        list.commands.is_empty(),
        "an invisible clip must emit nothing: {:?}",
        list.commands
    );

    p.set_variable(slot, "_visible", Value::Bool(true)).unwrap();
    p.set_variable(slot, "_type", Value::str("CommandIcon"))
        .unwrap();
    p.render_with(&mut list, RenderOptions { surfaces: true });
    assert_eq!(list.commands.len(), 1, "{:?}", list.commands);
    assert!(
        matches!(list.commands[0], RenderCommand::Surface { instance, .. } if instance == slot),
        "{:?}",
        list.commands
    );

    p.set_variable(slot, "_visible", Value::Bool(false))
        .unwrap();
    p.render_with(&mut list, RenderOptions { surfaces: true });
    assert!(list.commands.is_empty(), "{:?}", list.commands);
}
