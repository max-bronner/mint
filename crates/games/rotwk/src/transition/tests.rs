use super::*;

fn sound_window(
    style: Style,
    delay: i32,
    start: i32,
    end: i32,
    views: u8,
    leave_silent: bool,
) -> WindowDef {
    WindowDef {
        style,
        delay,
        start,
        end,
        views,
        leave_silent,
    }
}

fn with_group(name: &str, fire_once: bool, windows: Vec<WindowDef>) -> TransitionHandler {
    let mut groups = Groups::default();
    groups
        .by_name
        .insert(name.into(), GroupDef { fire_once, windows });
    TransitionHandler::new(groups)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn alpha8(h: &TransitionHandler) -> u32 {
    (h.screen_alpha() * 255.0 + 1e-3) as u32
}

fn handler() -> TransitionHandler {
    let window = |style, end| sound_window(style, 0, 0, end, 7, false);
    let mut groups = Groups::default();
    groups.by_name.insert(
        "fadetest".into(),
        GroupDef {
            fire_once: true,
            windows: vec![window(Style::ScreenFade, 20), window(Style::SoundFade, 20)],
        },
    );
    groups.by_name.insert(
        "othertest".into(),
        GroupDef {
            fire_once: true,
            windows: vec![window(Style::ScreenFade, 5)],
        },
    );
    TransitionHandler::new(groups)
}

fn run(h: &mut TransitionHandler, frames: u32) {
    for _ in 0..frames {
        h.update();
    }
}

#[test]
fn a_held_fire_once_group_keeps_its_end_frame() {
    let mut h = handler();
    h.set_hold(true);
    h.set_group("FadeTest");
    run(&mut h, 40);
    assert!(h.is_finished() && h.has_current());
    assert!((h.screen_alpha() - 1.0).abs() < 1e-6);
    h.set_hold(false);
    run(&mut h, 1);
    assert!(!h.has_current());
}

#[test]
fn reversing_a_group_that_is_not_current_starts_from_its_end_frame() {
    let mut h = handler();
    h.set_hold(true);
    h.set_group("OtherTest");
    h.reverse("FadeTest");
    assert!((h.screen_alpha() - 1.0).abs() < 1e-6);
    run(&mut h, 10);
    let mid = h.screen_alpha();
    assert!(mid > 0.0 && mid < 1.0);
    run(&mut h, 30);
    assert!(!h.has_current());
    assert!((h.pool_gain(0) - 1.0).abs() < 1e-6);
}

#[test]
fn reset_releases_the_group_and_the_sound() {
    let mut h = handler();
    h.set_hold(true);
    h.set_group("FadeTest");
    run(&mut h, 40);
    assert!(h.pool_gain(0) < 0.01);
    h.reset();
    assert!((h.pool_gain(0) - 1.0).abs() < 1e-6 && h.screen_alpha() == 0.0);
}

#[test]
fn a_screen_fade_divides_by_the_end_frame_less_one() {
    let mut h = handler();
    h.set_group("FadeTest");
    run(&mut h, 10);
    assert_eq!(alpha8(&h), 134);
}

#[test]
fn a_screen_fade_with_a_start_frame_is_not_reduced_by_it() {
    let mut h = with_group(
        "late",
        true,
        vec![sound_window(Style::ScreenFade, 0, 5, 10, 7, false)],
    );
    h.set_group("late");
    run(&mut h, 4);
    assert_eq!(alpha8(&h), 0);
    run(&mut h, 2);
    assert_eq!(alpha8(&h), 170);
}

#[test]
fn a_running_sound_fade_is_heard_squared() {
    let mut h = handler();
    h.set_hold(true);
    h.set_group("FadeTest");
    run(&mut h, 10);
    assert!((h.pool_gain(2) - 0.25).abs() < 1e-6);
}

#[test]
fn a_sound_fade_restores_its_pools_on_release_unless_it_leaves_them_silent() {
    let windows = |leave| vec![sound_window(Style::SoundFade, 0, 0, 20, 7, leave)];
    for (leave, restored) in [(false, true), (true, false)] {
        let mut h = with_group("quiet", true, windows(leave));
        h.set_hold(true);
        h.set_group("quiet");
        run(&mut h, 20);
        assert!(h.pool_gain(0) < 1e-6);
        h.set_hold(false);
        run(&mut h, 1);
        assert!(!h.has_current());
        assert_eq!(h.pool_gain(0) > 0.99, restored);
    }
}

#[test]
fn a_sound_fade_drives_only_the_pools_it_names() {
    let mut h = with_group(
        "tac",
        true,
        vec![sound_window(Style::SoundFade, 0, 0, 20, 1, false)],
    );
    h.set_group("tac");
    run(&mut h, 10);
    assert!(h.pool_gain(0) < 0.3);
    assert!((h.pool_gain(2) - 1.0).abs() < 1e-6);
}

#[test]
fn sound_fades_wait_while_suspended_and_apply_on_resume() {
    let mut h = handler();
    h.set_hold(true);
    h.suspend();
    h.set_group("FadeTest");
    run(&mut h, 10);
    assert!((h.pool_gain(0) - 1.0).abs() < 1e-6);
    h.resume();
    assert!((h.pool_gain(0) - 0.25).abs() < 1e-6);
}
