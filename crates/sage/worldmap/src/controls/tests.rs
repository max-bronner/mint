use super::*;
use mint_sage_ini::living_world::presentation::MapInfoDef;
use mint_sage_ini::living_world::Coord2;

fn camera() -> WorldCamera {
    let info = MapInfoDef {
        center: Some(Coord2 { x: 0.0, y: 0.0 }),
        extent: Some(Coord2 {
            x: 9000.0,
            y: 9000.0,
        }),
        ..MapInfoDef::default()
    };
    WorldCamera::new(&info)
}

fn controls() -> ScrollControls {
    let mut c = ScrollControls::new(ScrollFactors::default());
    c.set_viewport(800.0, 600.0);
    c.pointer_moved(400.0, 300.0);
    c
}

#[test]
fn a_held_arrow_key_scrolls_350_screen_units_a_frame_scaled_by_the_zoom() {
    let mut c = controls();
    let mut cam = camera();
    c.key(ScrollKey::Right, true);
    let before = cam.pan_point();
    c.frame(&mut cam);
    let moved = cam.pan_point()[0] - before[0];
    assert!(
        (moved - 350.0 * 0.5 * (1.0 + 4.0) * 0.25).abs() < 1.0e-3,
        "{moved}"
    );
    c.key(ScrollKey::Right, false);
    c.key(ScrollKey::Up, true);
    let before = cam.pan_point();
    c.frame(&mut cam);
    assert!(cam.pan_point()[1] > before[1], "up scrolls north");
}

#[test]
fn the_right_button_scrolls_by_the_distance_from_its_anchor_plus_a_floor() {
    let mut c = controls();
    c.right_button(true);
    c.pointer_moved(400.0, 400.0);
    let (offset, turn) = c.offset();
    assert_eq!(turn, 0.0);
    assert!((offset[1] - 101.0).abs() < 1.0e-4, "{offset:?}");
    assert_eq!(offset[0], 0.0);
    c.right_button(false);
    assert_eq!(c.offset().0, [0.0, 0.0]);
}

#[test]
fn the_anchor_stays_within_half_a_window_of_the_pointer() {
    let mut c = controls();
    c.pointer_moved(0.0, 300.0);
    c.right_button(true);
    c.pointer_moved(790.0, 300.0);
    let (offset, _) = c.offset();
    assert!((offset[0] - 401.0).abs() < 1.0e-4, "{offset:?}");
}

#[test]
fn the_screen_edge_scrolls_within_three_pixels_unless_turned_off() {
    let mut c = controls();
    c.pointer_moved(798.0, 300.0);
    assert_eq!(c.offset().0, [350.0, 0.0]);
    c.pointer_moved(796.0, 300.0);
    assert_eq!(c.offset().0, [0.0, 0.0]);
    c.pointer_moved(798.0, 300.0);
    c.pointer_left();
    assert_eq!(
        c.offset().0,
        [0.0, 0.0],
        "a pointer outside the window is at no edge"
    );
    c.pointer_moved(798.0, 300.0);
    c.factors.edge_scroll = false;
    assert_eq!(c.offset().0, [0.0, 0.0]);
}

#[test]
fn the_keypad_turns_and_zooms_only_while_no_scroll_mode_is_on() {
    let mut c = controls();
    c.factors.rotate = 0.1;
    let mut cam = camera();
    c.pad_key(PadKey::TurnLeft, true);
    c.frame(&mut cam);
    assert!(
        (cam.yaw() + 0.03 * 15.0 * 0.1).abs() < 1.0e-6,
        "{}",
        cam.yaw()
    );
    c.key(ScrollKey::Up, true);
    let yaw = cam.yaw();
    c.frame(&mut cam);
    assert_eq!(cam.yaw(), yaw, "an arrow key held takes the frame");
    c.key(ScrollKey::Up, false);
    c.pad_key(PadKey::TurnLeft, false);
    c.pad_key(PadKey::ZoomIn, true);
    let zoom = cam.zoom();
    c.frame(&mut cam);
    cam.tick();
    assert!(cam.zoom() < zoom, "keypad 8 zooms in");
}

#[test]
fn keypad_five_flies_the_yaw_back_to_zero_in_seven_frames() {
    let mut c = controls();
    let mut cam = camera();
    cam.scroll_by([0.0, 0.0], 10.0);
    c.pad_key(PadKey::Reset, true);
    c.frame(&mut cam);
    assert_eq!(cam.fly_frames(), 7);
    for _ in 0..7 {
        cam.tick();
    }
    assert_eq!(cam.yaw(), 0.0);
    assert!(!cam.flying());
}

#[test]
fn a_press_and_release_within_the_tolerance_on_both_axes_is_a_click_at_the_low_corner() {
    assert_eq!(
        click_point([100.0, 50.0], [110.0, 40.0], 15.0),
        Some([100.0, 40.0])
    );
    assert_eq!(click_point([100.0, 50.0], [115.0, 50.0], 15.0), None);
}

#[test]
fn the_middle_button_turns_the_view_and_the_yaw_stays_within_one() {
    let mut c = controls();
    let mut cam = camera();
    c.middle_button(true);
    c.pointer_moved(600.0, 300.0);
    c.frame(&mut cam);
    assert!((cam.yaw() - 0.03 * 200.0 * 0.005).abs() < 1.0e-6);
    for _ in 0..200 {
        c.frame(&mut cam);
    }
    assert_eq!(cam.yaw(), 1.0);
}

#[test]
fn a_middle_double_click_resets_the_view_and_turns_nothing() {
    let mut c = controls();
    let mut cam = camera();
    cam.scroll_by([0.0, 0.0], 10.0);
    c.middle_button(true);
    c.middle_double_click();
    c.pointer_moved(600.0, 300.0);
    c.frame(&mut cam);
    assert_eq!(cam.fly_frames(), 7);
    for _ in 0..7 {
        cam.tick();
    }
    assert_eq!(cam.yaw(), 0.0);
}
