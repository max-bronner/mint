use super::*;
use mint_sage_map::LookKey;

const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn free(
    frame: u32,
    interpolation: Interpolation,
    position: [f32; 3],
    rotation: [f32; 4],
) -> Key<FreeKey> {
    Key {
        frame,
        interpolation,
        value: FreeKey {
            position,
            rotation,
            lens: 1.0,
        },
    }
}

fn animation(track: CameraTrack) -> CameraAnimation {
    CameraAnimation {
        name: "Synthetic".to_owned(),
        num_frames: 30,
        start_offset: 0,
        track,
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

fn look(position: [f32; 3], target: [f32; 3], roll: f32) -> CameraPose {
    let key = |frame| Key {
        frame,
        interpolation: Interpolation::Linear,
        value: LookKey {
            position,
            roll,
            lens: 1.0,
        },
    };
    let aim = |frame| Key {
        frame,
        interpolation: Interpolation::Linear,
        value: target,
    };
    evaluate(
        &animation(CameraTrack::Look {
            camera: vec![key(0), key(10)],
            target: vec![aim(0), aim(10)],
        }),
        0.0,
    )
    .expect("keys")
}

#[test]
fn an_identity_free_key_looks_down_minus_z_with_y_up() {
    let pose = evaluate(
        &animation(CameraTrack::Free(vec![free(
            0,
            Interpolation::Step,
            [1.0, 2.0, 3.0],
            IDENTITY,
        )])),
        0.0,
    )
    .expect("keys");
    let camera = pose.camera(1.0, 1.0, 100.0);
    assert!(close(camera.eye.x, 1.0) && close(camera.eye.y, 2.0) && close(camera.eye.z, 3.0));
    assert!(
        close(camera.target.x, 1.0) && close(camera.target.y, 2.0) && close(camera.target.z, 2.0)
    );
    assert!(close(camera.up.x, 0.0) && close(camera.up.y, 1.0) && close(camera.up.z, 0.0));
}

#[test]
fn a_look_pose_rolls_about_the_view_axis_and_survives_a_straight_down_look() {
    let level = look([0.0; 3], [1.0, 0.0, 0.0], 0.0).camera(1.0, 1.0, 100.0);
    assert!(close(level.up.z, 1.0) && close(level.up.y, 0.0));
    let rolled =
        look([0.0; 3], [1.0, 0.0, 0.0], core::f32::consts::FRAC_PI_2).camera(1.0, 1.0, 100.0);
    assert!(close(rolled.up.y, -1.0) && close(rolled.up.z, 0.0));
    let down = look([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], 0.0).camera(1.0, 1.0, 100.0);
    assert!(
        down.eye.length().is_finite() && down.up.x.abs() > 0.99,
        "{:?}",
        down.up
    );
    assert!(close(down.target.z, 9.0));
}

#[test]
fn the_lens_is_a_horizontal_field_of_view() {
    let mut pose = look([0.0; 3], [1.0, 0.0, 0.0], 0.0);
    pose.lens = core::f32::consts::FRAC_PI_2;
    let camera = pose.camera(2.0, 1.0, 100.0);
    assert!(close(camera.fov_y, 2.0 * atan(0.5)));
}

#[test]
fn a_linear_track_blends_by_the_fraction_between_keys() {
    let track = CameraTrack::Free(vec![
        free(0, Interpolation::Linear, [0.0; 3], IDENTITY),
        free(10, Interpolation::Linear, [10.0, 0.0, 0.0], IDENTITY),
    ]);
    let pose = evaluate(&animation(track), 2.5).expect("keys");
    assert!(close(pose.eye().x, 2.5));
}

#[test]
fn a_spline_rescales_its_outer_neighbours_to_the_segment_spacing() {
    let at = |frame, x| free(frame, Interpolation::CatmullRom, [x, 0.0, 0.0], IDENTITY);
    let track = CameraTrack::Free(vec![at(0, 0.0), at(10, 10.0), at(12, 12.0), at(30, 30.0)]);
    let x = evaluate(&animation(track), 11.0).expect("keys").eye().x;
    assert!(close(x, 11.0), "{x}");
}

#[test]
fn rotations_blend_along_the_shorter_arc() {
    let half = core::f32::consts::FRAC_1_SQRT_2;
    let negated = [0.0, 0.0, -half, -half];
    let track = CameraTrack::Free(vec![
        free(0, Interpolation::Linear, [0.0; 3], IDENTITY),
        free(10, Interpolation::Linear, [0.0; 3], negated),
    ]);
    let pose = evaluate(&animation(track), 5.0).expect("keys");
    assert!(close(pose.matrix[0][0], half), "{}", pose.matrix[0][0]);
}

#[test]
fn time_before_the_first_key_and_past_the_last_hold_the_end_keys() {
    let track = CameraTrack::Free(vec![
        free(5, Interpolation::Linear, [1.0, 0.0, 0.0], IDENTITY),
        free(10, Interpolation::Linear, [9.0, 0.0, 0.0], IDENTITY),
    ]);
    let animation = animation(track);
    assert!(close(
        evaluate(&animation, -1.0).expect("keys").eye().x,
        1.0
    ));
    assert!(close(
        evaluate(&animation, 99.0).expect("keys").eye().x,
        9.0
    ));
}
