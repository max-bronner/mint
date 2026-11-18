use super::*;

#[test]
fn pan_displacement_per_step_is_independent_of_pitch() {
    let mut baseline: Option<f32> = None;
    for pitch in [MIN_PITCH, 0.6, 0.9, 1.1, MAX_PITCH] {
        for distance in [20.0f32, 500.0, 3000.0] {
            let mut camera = RtsCamera::new(Vec3::Z);
            camera.orbit.pitch = pitch;
            camera.orbit.distance = distance;
            let before = camera.focus();
            camera.pan(1.0, 1.0, 0.0);
            let after = camera.focus();
            let moved = Vec3::new(after.x - before.x, after.y - before.y, 0.0).length();
            let expected = (distance * PAN_SPEED_PER_DISTANCE).max(MIN_PAN_PER_SECOND);
            assert!(
                (moved - expected).abs() < 1e-2,
                "pitch {pitch} distance {distance}: moved {moved}, expected {expected}"
            );
            if distance == 20.0 {
                if let Some(base) = baseline {
                    assert!(
                        (moved - base).abs() < 1e-2,
                        "pitch changed the pan step: {moved} vs {base}"
                    );
                } else {
                    baseline = Some(moved);
                }
            }
        }
    }
}

#[test]
fn a_zero_input_pans_nowhere() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let before = camera.focus();
    camera.pan(1.0 / 30.0, 0.0, 0.0);
    assert_eq!(camera.focus(), before);
}

#[test]
fn edge_scroll_in_the_margin_matches_the_held_key_case() {
    let viewport = (1280, 720);
    let margin = DEFAULT_EDGE_SCROLL_MARGIN;
    assert_eq!(
        RtsCamera::edge_pan_axes(Some((margin - 1.0, 360.0)), viewport, margin),
        (0.0, -1.0)
    );
    assert_eq!(
        RtsCamera::edge_pan_axes(Some((1279.0, 1.0)), viewport, margin),
        (1.0, 1.0)
    );
}

#[test]
fn panning_right_moves_along_the_renderers_right_not_against_it() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let before = camera.focus();
    let eye = camera.eye();
    let back = Vec3::new(eye.x - before.x, eye.y - before.y, eye.z - before.z).normalize();
    let renderer_right = Vec3::Z.cross(back).normalize();

    camera.pan(1.0 / 30.0, 0.0, 1.0);
    let after = camera.focus();
    let moved = Vec3::new(after.x - before.x, after.y - before.y, after.z - before.z);

    assert!(moved.length() > 0.0, "panning right moved nothing");
    assert!(
            moved.dot(renderer_right) > 0.0,
            "panning right moved along {moved:?}, which opposes the renderer's              right {renderer_right:?} - the ground basis is inverted"
        );
}

#[test]
fn a_pointer_away_from_every_edge_pans_nothing() {
    assert_eq!(
        RtsCamera::edge_pan_axes(
            Some((640.0, 360.0)),
            (1280, 720),
            DEFAULT_EDGE_SCROLL_MARGIN
        ),
        (0.0, 0.0)
    );
}

#[test]
fn no_pointer_pans_nothing() {
    assert_eq!(
        RtsCamera::edge_pan_axes(None, (1280, 720), DEFAULT_EDGE_SCROLL_MARGIN),
        (0.0, 0.0)
    );
    assert_eq!(
        RtsCamera::edge_pan_axes(Some((0.0, 0.0)), (0, 0), DEFAULT_EDGE_SCROLL_MARGIN),
        (0.0, 0.0)
    );
}

#[test]
fn zoom_is_multiplicative_and_bounded() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let before = camera.distance_target();
    camera.zoom(1.0);
    assert!(camera.distance_target() < before);
    for _ in 0..1000 {
        camera.zoom(10.0);
    }
    assert!(camera.distance_target() >= DEFAULT_MIN_DISTANCE - 1e-3);
    settle(&mut camera);
    assert!(camera.distance() >= DEFAULT_MIN_DISTANCE - 1e-3);
    for _ in 0..1000 {
        camera.zoom(-10.0);
    }
    assert!(camera.distance_target() <= DEFAULT_MAX_DISTANCE + 1e-3);
    settle(&mut camera);
    assert!(camera.distance() <= DEFAULT_MAX_DISTANCE + 1e-3);
}

#[test]
fn rotating_leaves_the_focus_point_fixed() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_focus(Vec3::new(120.0, -40.0, 7.0));
    let before = camera.focus();
    camera.rotate(300.0, -150.0);
    assert_eq!(camera.focus(), before);
}

#[test]
fn rotating_clamps_pitch_to_the_rts_range_not_the_orbit_cameras_wider_one() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.rotate(0.0, -1_000_000.0);
    assert!(
        camera.pitch() <= MAX_PITCH + 1e-4,
        "pitch {}",
        camera.pitch()
    );
    camera.rotate(0.0, 1_000_000.0);
    assert!(
        camera.pitch() >= MIN_PITCH - 1e-4,
        "pitch {}",
        camera.pitch()
    );
}

#[test]
fn reset_orientation_restores_yaw_pitch_and_zoom_but_not_the_focus() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_focus(Vec3::new(120.0, -40.0, 7.0));
    let focus = camera.focus();
    camera.rotate(500.0, -300.0);
    camera.zoom(5.0);
    camera.snap_zoom();
    assert_ne!(camera.yaw(), 0.0);
    camera.reset_orientation();
    assert_eq!(camera.yaw(), 0.0);
    assert!((camera.pitch() - 0.9).abs() < 1e-6);
    assert!((camera.distance() - 200.0).abs() < 1e-6);
    assert_eq!(camera.focus(), focus);
}

#[test]
fn zooming_in_keeps_the_whole_world_inside_the_far_plane() {
    let map = Aabb::from_corners(Vec3::new(0.0, 0.0, 0.0), Vec3::new(4_000.0, 3_500.0, 200.0));
    let world = Aabb::from_corners(
        Vec3::new(-1_900.0, -2_100.0, 0.0),
        Vec3::new(5_800.0, 5_700.0, 2_900.0),
    );
    for focus in [map.min(), map.max(), map.center()] {
        for yaw in [0.0f32, 1.0, 2.5, 4.0] {
            let mut camera = RtsCamera::new(Vec3::Z);
            camera.pan_bounds = Some(map);
            camera.keep_in_view(&world);
            camera.set_focus(focus);
            camera.orbit.yaw = yaw;
            camera.orbit.distance = camera.orbit.min_distance;
            let view = camera.camera();
            let forward = (view.target - view.eye).normalize();
            for index in 0..8u8 {
                let pick = |bit: u8, low: f32, high: f32| if index & bit == 0 { low } else { high };
                let corner = Vec3::new(
                    pick(1, world.min().x, world.max().x),
                    pick(2, world.min().y, world.max().y),
                    pick(4, world.min().z, world.max().z),
                );
                let depth = (corner - view.eye).dot(forward);
                assert!(
                    depth < view.far,
                    "focus {focus:?} yaw {yaw}: corner {corner:?} at depth {depth} beyond far {}",
                    view.far
                );
            }
        }
    }
}

#[test]
fn pan_is_clamped_to_the_map_bounds_in_every_direction() {
    let bounds = Aabb::from_corners(Vec3::new(-100.0, -50.0, 0.0), Vec3::new(100.0, 50.0, 0.0));
    for (forward, right) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.pan_bounds = Some(bounds);
        for _ in 0..10_000 {
            camera.pan(1.0, forward, right);
        }
        let focus = camera.focus();
        assert!(
            focus.x >= bounds.min().x - 1e-3 && focus.x <= bounds.max().x + 1e-3,
            "x {} escaped {:?}",
            focus.x,
            bounds
        );
        assert!(
            focus.y >= bounds.min().y - 1e-3 && focus.y <= bounds.max().y + 1e-3,
            "y {} escaped {:?}",
            focus.y,
            bounds
        );
    }
}

#[test]
fn panning_to_an_edge_keeps_the_near_screen_ray_inside_the_bounds() {
    let bounds = Aabb::from_corners(
        Vec3::new(-2_000.0, -2_000.0, 0.0),
        Vec3::new(2_000.0, 2_000.0, 0.0),
    );
    for pitch in [0.5_f32, 1.2] {
        for (forward, right) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let mut camera = RtsCamera::new(Vec3::Z);
            camera.orbit.pitch = pitch;
            camera.pan_bounds = Some(bounds);
            for _ in 0..10_000 {
                camera.pan(1.0, forward, right);
            }
            let probe = camera.orbit.camera();
            let ray = probe
                .ray_through_pixel(
                    GROUND_INSET_PROBE_VIEWPORT,
                    GROUND_INSET_PROBE_VIEWPORT.0 as f32 * 0.5,
                    GROUND_INSET_PROBE_VIEWPORT.1 as f32 * GROUND_INSET_SCREEN_FRACTION,
                )
                .expect("a camera this close to the ground always has a near ray");
            let hit = ground_plane_hit(ray, probe.target.z)
                .expect("the test pitches are well short of straight down");
            assert!(
                hit.x >= bounds.min().x - 1e-2 && hit.x <= bounds.max().x + 1e-2,
                "pitch {pitch} ({forward},{right}): near-ray x {} outside {:?}",
                hit.x,
                bounds
            );
            assert!(
                hit.y >= bounds.min().y - 1e-2 && hit.y <= bounds.max().y + 1e-2,
                "pitch {pitch} ({forward},{right}): near-ray y {} outside {:?}",
                hit.y,
                bounds
            );
        }
    }
}

#[test]
fn with_no_inset_policy_the_raw_inset_is_used_even_past_a_quarter_of_the_extent() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.orbit.pitch = 0.5;
    let bounds = Aabb::from_corners(Vec3::new(-50.0, -50.0, 0.0), Vec3::new(50.0, 50.0, 0.0));
    camera.pan_bounds = Some(bounds);
    let inset = camera.ground_inset();
    assert!(
        inset > (bounds.max().x - bounds.min().x) * 0.25,
        "this test needs an inset past a quarter of the extent to exercise the claim"
    );
    for _ in 0..10_000 {
        camera.pan(1.0, -1.0, -1.0);
    }
    let focus = camera.focus();
    let half_extent = (bounds.max().x - bounds.min().x) * 0.5;
    let expected = bounds.min().x + inset.min(half_extent);
    assert!(
        (focus.x - expected).abs() < 1e-3,
        "focus {focus:?} is not inset by the raw ground_inset {inset} (expected x {expected})"
    );
}

#[test]
fn no_bounds_means_no_clamp() {
    let mut camera = RtsCamera::new(Vec3::Z);
    for _ in 0..1000 {
        camera.pan(1.0, 1.0, 0.0);
    }
    assert!(camera.focus().length() > 10_000.0);
}

const FRAME_144: f32 = 1.0 / 144.0;

fn settle(camera: &mut RtsCamera) {
    for _ in 0..2_000 {
        camera.update(FRAME_144);
        if camera.distance() == camera.distance_target() {
            return;
        }
    }
}

#[test]
fn a_pan_ramps_up_instead_of_starting_at_full_speed() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pan_input(0.0, 1.0);

    let before = camera.focus();
    camera.update(FRAME_144);
    let first = (camera.focus() - before).length();

    for _ in 0..40 {
        camera.update(FRAME_144);
    }
    let at_speed = camera.focus();
    camera.update(FRAME_144);
    let steady = (camera.focus() - at_speed).length();

    assert!(
        first < steady * 0.2,
        "the first frame moved {first}, a steady frame moves {steady}: that is a step, not a ramp"
    );
    let expected = (camera.distance() * PAN_SPEED_PER_DISTANCE).max(MIN_PAN_PER_SECOND) * FRAME_144;
    assert!(
        (steady - expected).abs() < 1e-3,
        "steady frame moved {steady}, the speed rule says {expected}"
    );
    assert_eq!(camera.pan_velocity(), (0.0, 1.0));
}

#[test]
fn releasing_the_key_eases_to_a_stop_and_reaches_it() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pan_input(0.0, 1.0);
    for _ in 0..200 {
        camera.update(FRAME_144);
    }
    camera.set_pan_input(0.0, 0.0);

    camera.update(FRAME_144);
    assert!(
        camera.pan_velocity().1 > 0.0,
        "the camera stopped dead the frame the key came up"
    );
    for _ in 0..20 {
        camera.update(FRAME_144);
    }
    assert_eq!(camera.pan_velocity(), (0.0, 0.0));
    let stopped = camera.focus();
    camera.update(FRAME_144);
    assert_eq!(camera.focus(), stopped, "it never came to rest");
}

#[test]
fn a_second_of_panning_covers_the_same_ground_at_any_frame_rate() {
    let travel = |dt: f32, frames: usize| {
        let mut camera = RtsCamera::new(Vec3::Z);
        camera.set_pan_input(0.0, 1.0);
        let before = camera.focus();
        for _ in 0..frames {
            camera.update(dt);
        }
        (camera.focus() - before).length()
    };
    let fast = travel(FRAME_144, 144);
    let slow = travel(1.0 / 60.0, 60);
    assert!(
        (fast - slow).abs() < fast * 1e-3,
        "144 Hz travelled {fast}, 60 Hz travelled {slow}"
    );
}

#[test]
fn a_diagonal_is_no_faster_than_a_straight_line() {
    let mut straight = RtsCamera::new(Vec3::Z);
    let mut diagonal = RtsCamera::new(Vec3::Z);
    straight.set_pan_input(1.0, 0.0);
    diagonal.set_pan_input(1.0, 1.0);
    let (a, b) = (straight.focus(), diagonal.focus());
    for _ in 0..200 {
        straight.update(FRAME_144);
        diagonal.update(FRAME_144);
    }
    let straight_moved = (straight.focus() - a).length();
    let diagonal_moved = (diagonal.focus() - b).length();
    assert!(
        diagonal_moved <= straight_moved + 1e-3,
        "diagonal {diagonal_moved} beat straight {straight_moved}"
    );
}

#[test]
fn a_detent_selects_a_rung_and_the_eye_eases_to_it() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let start = camera.distance();

    camera.zoom(1.0);
    let rung = camera.distance_target();
    assert!(rung < start, "a detent must still pick the next rung");
    assert_eq!(
        camera.distance(),
        start,
        "the eye must not teleport to the rung"
    );

    camera.update(FRAME_144);
    let after_one_frame = camera.distance();
    assert!(
        after_one_frame < start && after_one_frame > rung,
        "one frame should be part of the way there: {after_one_frame} between {rung} and {start}"
    );
    settle(&mut camera);
    assert_eq!(camera.distance(), rung);

    let mut flicked = RtsCamera::new(Vec3::Z);
    let mut spread = RtsCamera::new(Vec3::Z);
    for _ in 0..3 {
        flicked.zoom(1.0);
    }
    for _ in 0..3 {
        spread.zoom(1.0);
        settle(&mut spread);
    }
    settle(&mut flicked);
    assert!(
        (flicked.distance() - spread.distance()).abs() < 1e-2,
        "flicked to {}, stepped to {}",
        flicked.distance(),
        spread.distance()
    );
}

#[test]
fn a_stalled_frame_is_capped_rather_than_integrated() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pan_input(0.0, 1.0);
    let before = camera.focus();
    camera.update(10.0);
    let moved = (camera.focus() - before).length();
    let ceiling =
        (camera.distance() * PAN_SPEED_PER_DISTANCE).max(MIN_PAN_PER_SECOND) * MAX_FRAME_SECONDS;
    assert!(moved <= ceiling + 1e-3, "a 10-second frame moved {moved}");
}

#[test]
fn non_finite_frame_time_and_input_are_ignored() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pan_input(0.0, 1.0);
    let before = camera.focus();
    camera.update(f32::NAN);
    camera.update(f32::INFINITY);
    camera.update(-1.0);
    assert_eq!(camera.focus(), before);
    camera.set_pan_input(f32::NAN, 1.0);
    assert_eq!(camera.pan_velocity(), (0.0, 0.0));
    camera.update(FRAME_144);
    assert_eq!(camera.focus(), before);
}

#[test]
fn non_finite_input_is_ignored() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let before = camera.focus();
    camera.pan(f32::NAN, 1.0, 0.0);
    camera.pan(f32::INFINITY, 1.0, 0.0);
    assert_eq!(camera.focus(), before);
}

#[test]
fn a_titles_pitch_and_lens_are_set_outright() {
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.set_pitch(0.65);
    assert_eq!(camera.pitch(), 0.65);
    camera.set_pitch(0.0);
    assert_eq!(camera.pitch(), MIN_PITCH);
    camera.set_pitch(f32::NAN);
    assert_eq!(camera.pitch(), MIN_PITCH);
    camera.set_fov_y(0.7);
    assert_eq!(camera.camera().fov_y, 0.7);
    camera.set_fov_y(-1.0);
    assert_eq!(camera.camera().fov_y, 0.7);
}

#[test]
fn a_shake_rotates_the_view_within_its_radius_and_expires() {
    let mut camera = RtsCamera::new(Vec3::Z);
    let eye = camera.eye();
    camera.shake(eye, 100.0, 5.0, 1.0);
    camera.update(1.0 / 30.0);
    let shaken = camera.camera();
    let base = camera.orbit.camera();
    assert_eq!(shaken.eye, base.eye, "a rotation, not a translation");
    assert_ne!(shaken.target, base.target, "within radius, shaken");
    for (angle, limit) in camera.shake.angles.iter().zip(SHAKE_AXIS_ROTATION) {
        assert!(angle.abs() <= limit);
    }

    let mut far = RtsCamera::new(Vec3::Z);
    let far_eye = far.eye();
    far.shake(
        Vec3::new(far_eye.x + 200.0, far_eye.y, far_eye.z),
        100.0,
        5.0,
        1.0,
    );
    far.update(1.0 / 30.0);
    assert_eq!(
        far.camera(),
        far.orbit.camera(),
        "past the radius, untouched"
    );

    for _ in 0..31 {
        camera.update(1.0 / 30.0);
    }
    assert_eq!(
        camera.camera(),
        camera.orbit.camera(),
        "expired after its duration"
    );
}

#[test]
fn a_jump_is_held_to_the_pan_bounds_but_never_touches_zoom_or_pitch() {
    let bounds = Aabb::from_corners(Vec3::new(0.0, 0.0, 0.0), Vec3::new(4_000.0, 3_000.0, 0.0));
    let mut camera = RtsCamera::new(Vec3::Z);
    camera.pan_bounds = Some(bounds);
    let (distance, pitch) = (camera.orbit.distance, camera.orbit.pitch);
    camera.jump_to(Vec3::new(-900.0, 8_000.0, 0.0));
    let focus = camera.focus();
    assert!(
        focus.x >= 0.0 && focus.x <= 4_000.0,
        "x {} escaped",
        focus.x
    );
    assert!(
        focus.y >= 0.0 && focus.y <= 3_000.0,
        "y {} escaped",
        focus.y
    );
    assert_eq!(camera.orbit.distance, distance);
    assert_eq!(camera.orbit.pitch, pitch);
    camera.set_focus(Vec3::new(-900.0, 8_000.0, 0.0));
    assert_eq!(camera.focus(), Vec3::new(-900.0, 8_000.0, 0.0));
}
