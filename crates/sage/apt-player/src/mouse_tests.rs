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
use crate::{clip_event, ButtonVisual, HitMode, InstanceId, InstanceKind, Library, Limits, Player};

fn scripted(s: &Synth, root: &Frames) -> Player {
    let (c, apt) = s.build(root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).expect("synthetic movie parses");
    lib.link();
    let mut p = Player::new(Arc::new(lib), id, Limits::default()).expect("player starts");
    p.set_trace_log(true);
    p
}

fn child(p: &Player, parent: InstanceId, depth: i32) -> InstanceId {
    p.instance(parent).unwrap().child_at(depth).unwrap()
}

fn traced(s: &mut Synth, text: &str) -> u32 {
    let mut a = Asm::new();
    a.push_str(text).op(op::TRACE).end();
    s.code(&mut a)
}

fn state(p: &Player, button: InstanceId) -> ButtonVisual {
    match p.instance(button).unwrap().kind() {
        InstanceKind::Button(v) => *v,
        other => panic!("not a button: {other:?}"),
    }
}

fn button_movie(hit_at: (f32, f32)) -> (Synth, Frames) {
    let mut s = Synth::new();
    let art = s.shape_box([0.0, 0.0, 10.0, 10.0]);
    let actions = vec![
        (0x01, traced(&mut s, "roll over")),
        (0x02, traced(&mut s, "roll out")),
        (0x04, traced(&mut s, "press")),
        (0x08, traced(&mut s, "release")),
        (0x10, traced(&mut s, "drag out")),
        (0x40, traced(&mut s, "release outside")),
    ];
    let button = s.button_with_mesh(
        vec![(0x1, art, 1, 0.0, 0.0), (0x8, art, 2, hit_at.0, hit_at.1)],
        actions,
        vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
    );
    let root = vec![vec![Item::Place(place(1, button).at(100.0, 100.0))]];
    (s, root)
}

#[test]
fn over_press_release_change_state_and_run_their_records() {
    let (s, root) = button_movie((0.0, 0.0));
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 1);
    assert_eq!(state(&p, b), ButtonVisual::Up);

    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), Some(b));
    assert_eq!(state(&p, b), ButtonVisual::Over);
    assert_eq!(p.take_traces(), vec!["roll over".to_owned()]);

    p.pointer_button(true).unwrap();
    assert_eq!(state(&p, b), ButtonVisual::Down);
    assert_eq!(p.take_traces(), vec!["press".to_owned()]);

    p.pointer_button(false).unwrap();
    assert_eq!(state(&p, b), ButtonVisual::Over);
    assert_eq!(p.take_traces(), vec!["release".to_owned()]);

    p.pointer_move(300.0, 300.0).unwrap();
    assert_eq!(p.hovered(), None);
    assert_eq!(state(&p, b), ButtonVisual::Up);
    assert_eq!(p.take_traces(), vec!["roll out".to_owned()]);
}

#[test]
fn the_hit_area_is_the_hit_record_in_its_own_space() {
    let (s, root) = button_movie((50.0, 0.0));
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 1);

    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), None, "the Up art is not the click area");
    p.pointer_move(155.0, 105.0).unwrap();
    assert_eq!(p.hovered(), Some(b), "the hit record, offset by its matrix");

    p.set_hit_mode(HitMode::MeshBox);
    p.pointer_move(154.0, 104.0).unwrap();
    assert_eq!(p.hovered(), None);
    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), Some(b));
}

#[test]
fn releasing_elsewhere_is_release_outside() {
    let (s, root) = button_movie((0.0, 0.0));
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 1);
    p.pointer_move(105.0, 105.0).unwrap();
    p.pointer_button(true).unwrap();
    let _ = p.take_traces();

    p.pointer_move(300.0, 300.0).unwrap();
    assert_eq!(p.take_traces(), vec!["drag out".to_owned()]);
    assert_eq!(state(&p, b), ButtonVisual::Over);

    p.pointer_button(false).unwrap();
    assert_eq!(p.take_traces(), vec!["release outside".to_owned()]);
    assert_eq!(state(&p, b), ButtonVisual::Up);
}

#[test]
fn a_disabled_button_runs_nothing() {
    let (mut s, mut root) = button_movie((0.0, 0.0));
    let mut a = Asm::new();
    a.get_var("b")
        .push_str("enabled")
        .push_byte(0)
        .op(op::SET_MEMBER)
        .end();
    let disable = s.code(&mut a);
    if let Item::Place(pl) = &mut root[0][0] {
        pl.name = Some("b".to_owned());
    }
    root[0].push(Item::Action(disable));
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 1);
    p.pointer_move(105.0, 105.0).unwrap();
    p.pointer_button(true).unwrap();
    p.pointer_button(false).unwrap();
    assert!(p.take_traces().is_empty());
    assert_eq!(state(&p, b), ButtonVisual::Up);
}

#[test]
fn a_masked_button_is_picked_only_inside_its_mask() {
    let mut s = Synth::new();
    let art = s.shape_box([0.0, 0.0, 40.0, 10.0]);
    let mask = s.shape_box([0.0, 0.0, 10.0, 10.0]);
    let button = s.button(
        vec![(0x1, art, 1, 0.0, 0.0), (0x8, art, 2, 0.0, 0.0)],
        vec![],
    );
    let root = vec![vec![
        Item::Place(place(1, mask).at(100.0, 100.0).clip_depth(5)),
        Item::Place(place(2, button).at(100.0, 100.0)),
    ]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let b = child(&p, p.root(), 2);
    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), Some(b), "inside the mask");
    p.pointer_move(130.0, 105.0).unwrap();
    assert_eq!(p.hovered(), None, "on the button, outside the mask");
}

#[test]
fn a_clip_with_a_press_handler_takes_the_pointer() {
    let mut s = Synth::new();
    let art = s.shape_box([0.0, 0.0, 10.0, 10.0]);
    let clip = s.sprite(vec![vec![Item::Place(place(1, art))]]);
    let pressed = traced(&mut s, "clip press");
    let root = vec![vec![Item::Place(
        place(1, clip)
            .at(100.0, 100.0)
            .events(vec![(clip_event::PRESS, pressed)]),
    )]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let c = child(&p, p.root(), 1);
    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), Some(c));
    p.pointer_button(true).unwrap();
    assert_eq!(p.take_traces(), vec!["clip press".to_owned()]);
}

#[test]
fn a_button_appearing_under_a_still_pointer_is_rolled_over() {
    let (s, mut root) = button_movie((0.0, 0.0));
    let place_button = root[0].remove(0);
    root = vec![vec![], vec![place_button]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    p.pointer_move(105.0, 105.0).unwrap();
    assert_eq!(p.hovered(), None);
    assert!(p.instance(p.root()).unwrap().child_at(1).is_none());
    p.tick().unwrap();
    let b = child(&p, p.root(), 1);
    assert_eq!(p.hovered(), Some(b));
    assert_eq!(state(&p, b), ButtonVisual::Over);
    assert_eq!(p.take_traces(), vec!["roll over".to_owned()]);
}

#[test]
fn every_action_block_is_decoded_at_load() {
    let (mut s, mut root) = button_movie((0.0, 0.0));
    let frame = traced(&mut s, "frame");
    root[0].push(Item::Action(frame));
    let (c, apt) = s.build(&root);
    let mut lib = Library::new();
    let id = lib.add("test", &c, apt).unwrap();
    assert_eq!(lib.block_counts(), (7, 0));
    assert!(lib.block(id, frame).is_some_and(|b| b.is_some()));
}

#[test]
fn a_clip_with_a_type_renders_as_a_surface_when_asked() {
    use crate::{RenderCommand, RenderList, RenderOptions};
    let mut s = Synth::new();
    let art = s.shape_box([0.0, 0.0, 30.0, 20.0]);
    let clip = s.sprite(vec![vec![Item::Place(place(1, art))]]);
    let mut a = Asm::new();
    a.get_var("view")
        .push_str("_type")
        .push_str("View3D")
        .op(op::SET_MEMBER)
        .end();
    let set = s.code(&mut a);
    let root = vec![vec![
        Item::Place(place(1, clip).name("view")),
        Item::Action(set),
    ]];
    let mut p = scripted(&s, &root);
    p.tick().unwrap();
    let v = child(&p, p.root(), 1);
    assert_eq!(p.own_string(v, "_type").as_deref(), Some("View3D"));

    let mut list = RenderList::default();
    p.render(&mut list);
    assert!(matches!(
        list.commands.as_slice(),
        [RenderCommand::Shape { .. }]
    ));
    p.render_with(&mut list, RenderOptions { surfaces: true });
    match list.commands.as_slice() {
        [RenderCommand::Surface {
            instance, bounds, ..
        }] => {
            assert_eq!(*instance, v);
            assert_eq!(*bounds, [0.0, 0.0, 30.0, 20.0]);
        }
        other => panic!("{other:?}"),
    }
}
