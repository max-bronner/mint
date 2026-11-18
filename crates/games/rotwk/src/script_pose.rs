use mint_app::RtsCamera;
use mint_sage_map::CameraAnimation;
use mint_sage_scene::camera_pose::evaluate;

use crate::diag::diag;
use crate::rules::map_scripts::AnimationPlay;

const FRAMES_PER_SECOND: f32 = 30.0;

#[derive(Debug, Clone, Copy)]
struct Playing {
    index: usize,
    time: f32,
}

#[derive(Debug, Default)]
pub(crate) struct ScriptPose {
    seen: Option<u32>,
    playing: Option<Playing>,
}

impl ScriptPose {
    pub(crate) fn playing(&self) -> bool {
        self.playing.is_some()
    }

    pub(crate) fn update(
        &mut self,
        play: Option<&AnimationPlay>,
        animations: &[CameraAnimation],
        interrupted: bool,
        camera: &mut RtsCamera,
        aspect: f32,
        dt: f32,
    ) {
        if let Some(command) = play.filter(|p| self.seen != Some(p.seq)) {
            self.seen = Some(command.seq);
            self.playing = animations
                .iter()
                .position(|a| a.name.eq_ignore_ascii_case(&command.name))
                .map(|index| Playing { index, time: 0.0 });
            if self.playing.is_none() {
                diag!(
                    "script: MOVE_CAMERA_BY_ANIMATION names no animation {}",
                    command.name
                );
            }
        } else if interrupted {
            self.playing = None;
        } else if let Some(playing) = self.playing.as_mut() {
            playing.time += dt.max(0.0) * FRAMES_PER_SECOND;
        }
        camera.set_posed(None);
        let Some(playing) = self.playing else {
            return;
        };
        let Some(animation) = animations.get(playing.index) else {
            self.playing = None;
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        if playing.time > animation.num_frames as f32 {
            self.playing = None;
            return;
        }
        let Some(pose) = evaluate(animation, playing.time) else {
            self.playing = None;
            return;
        };
        let base = camera.camera();
        let mut posed = pose.camera(aspect, base.near, base.far);
        if !(pose.lens > 0.0 && pose.lens < core::f32::consts::PI) {
            posed.fov_y = base.fov_y;
        }
        camera.set_posed(Some(posed));
    }
}

#[cfg(test)]
mod tests {
    use mint_render::Vec3;
    use mint_sage_map::{CameraTrack, FreeKey, Interpolation, Key};

    use super::*;

    fn animation(frames: u32) -> CameraAnimation {
        let key = |frame, x| Key {
            frame,
            interpolation: Interpolation::Linear,
            value: FreeKey {
                position: [x, 0.0, 5.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                lens: 1.0,
            },
        };
        CameraAnimation {
            name: "Fly".to_owned(),
            num_frames: frames,
            start_offset: 0,
            track: CameraTrack::Free(vec![key(0, 0.0), key(frames, 100.0)]),
        }
    }

    fn play(seq: u32, name: &str) -> AnimationPlay {
        AnimationPlay {
            seq,
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_pose_holds_through_the_last_frame_then_cuts_back_to_an_untouched_orbit() {
        let mut camera = RtsCamera::new(Vec3::Z);
        let orbit = camera.camera();
        let animations = [animation(30)];
        let mut pose = ScriptPose::default();
        let command = play(1, "fly");
        pose.update(Some(&command), &animations, false, &mut camera, 1.5, 0.0);
        assert!(camera.is_posed());
        pose.update(Some(&command), &animations, false, &mut camera, 1.5, 1.0);
        assert!(camera.is_posed(), "time == numFrames is still drawn");
        assert!((camera.camera().eye.x - 100.0).abs() < 1e-3);
        pose.update(Some(&command), &animations, false, &mut camera, 1.5, 0.033);
        assert!(!camera.is_posed() && !pose.playing());
        assert_eq!(camera.camera(), orbit);
    }

    #[test]
    fn another_camera_move_ends_the_playback_for_good() {
        let mut camera = RtsCamera::new(Vec3::Z);
        let animations = [animation(30)];
        let mut pose = ScriptPose::default();
        let command = play(1, "Fly");
        pose.update(Some(&command), &animations, false, &mut camera, 1.5, 0.0);
        pose.update(Some(&command), &animations, true, &mut camera, 1.5, 0.1);
        assert!(!camera.is_posed());
        pose.update(Some(&command), &animations, false, &mut camera, 1.5, 0.1);
        assert!(!camera.is_posed() && !pose.playing());
    }

    #[test]
    fn an_unknown_name_plays_nothing() {
        let mut camera = RtsCamera::new(Vec3::Z);
        let mut pose = ScriptPose::default();
        let command = play(1, "Nope");
        pose.update(
            Some(&command),
            &[animation(30)],
            false,
            &mut camera,
            1.5,
            0.0,
        );
        assert!(!camera.is_posed());
    }
}
