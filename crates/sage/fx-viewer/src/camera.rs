use mint_math::f32 as math;
use mint_render::{Camera, Vec3};

const PITCH_LIMIT: f32 = 1.5533;

const ROTATE_PER_PIXEL: f32 = 0.005;

const PAN_PER_PIXEL: f32 = 0.0016;

const ZOOM_PER_DETENT: f32 = 1.18;

const MIN_DISTANCE: f32 = 2.0;
const MAX_DISTANCE: f32 = 4000.0;

const FRAME_DISTANCE: f32 = 110.0;
const FRAME_HEIGHT: f32 = 15.0;

const TRAVEL_PER_SECOND: f32 = 0.9;

const FAST: f32 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct ViewCamera {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for ViewCamera {
    fn default() -> Self {
        let mut camera = Self {
            focus: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            distance: FRAME_DISTANCE,
        };
        camera.reset();
        camera
    }
}

impl ViewCamera {
    pub fn reset(&mut self) {
        self.focus = Vec3::new(0.0, 0.0, FRAME_HEIGHT);
        self.yaw = std::f32::consts::FRAC_PI_2;
        self.pitch = -0.35;
        self.distance = FRAME_DISTANCE;
    }

    #[must_use]
    pub fn forward(&self) -> Vec3 {
        let (sin_pitch, cos_pitch) = math::sin_cos(self.pitch);
        let (sin_yaw, cos_yaw) = math::sin_cos(self.yaw);
        Vec3::new(cos_pitch * cos_yaw, cos_pitch * sin_yaw, sin_pitch)
    }

    #[must_use]
    pub fn right(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = math::sin_cos(self.yaw);
        Vec3::new(sin_yaw, -cos_yaw, 0.0)
    }

    #[must_use]
    pub fn screen_up(&self) -> Vec3 {
        let (right, forward) = (self.right(), self.forward());
        Vec3::new(
            right.y * forward.z - right.z * forward.y,
            right.z * forward.x - right.x * forward.z,
            right.x * forward.y - right.y * forward.x,
        )
    }

    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let forward = self.forward();
        Vec3::new(
            self.focus.x - forward.x * self.distance,
            self.focus.y - forward.y * self.distance,
            self.focus.z - forward.z * self.distance,
        )
    }

    fn shift(&mut self, along: Vec3, by: f32) {
        self.focus = Vec3::new(
            self.focus.x + along.x * by,
            self.focus.y + along.y * by,
            self.focus.z + along.z * by,
        );
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let scale = self.distance * PAN_PER_PIXEL;
        self.shift(self.right(), -dx * scale);
        self.shift(self.screen_up(), dy * scale);
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * ROTATE_PER_PIXEL;
        self.pitch = (self.pitch - dy * ROTATE_PER_PIXEL).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn zoom(&mut self, lines: f32) {
        let factor = math::powf(ZOOM_PER_DETENT, -lines);
        self.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    pub fn travel(&mut self, axes: (f32, f32, f32), fast: bool, dt: f32) {
        let step = self.distance * TRAVEL_PER_SECOND * dt * if fast { FAST } else { 1.0 };
        let (ahead, side, lift) = axes;
        let (sin_yaw, cos_yaw) = math::sin_cos(self.yaw);
        self.shift(Vec3::new(cos_yaw, sin_yaw, 0.0), ahead * step);
        self.shift(self.right(), side * step);
        self.shift(Vec3::Z, lift * step);
    }

    #[must_use]
    pub fn camera(&self, far: f32) -> Camera {
        Camera {
            eye: self.eye(),
            target: self.focus,
            up: Vec3::Z,
            fov_y: mint_render::DEFAULT_FOV_Y,
            near: 0.5,
            far,
        }
    }

    #[must_use]
    pub fn report(&self) -> String {
        format!(
            "camera: focus ({:.1}, {:.1}, {:.1}) yaw {:.2} pitch {:.2} distance {:.1}",
            self.focus.x, self.focus.y, self.focus.z, self.yaw, self.pitch, self.distance
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_past_vertical_clamps_rather_than_flipping() {
        let mut camera = ViewCamera::default();
        camera.rotate(0.0, -100_000.0);
        assert!(camera.pitch <= PITCH_LIMIT);
        camera.rotate(0.0, 200_000.0);
        assert!(camera.pitch >= -PITCH_LIMIT);
        assert!(camera.forward().is_finite());
    }

    #[test]
    fn a_pan_moves_the_focus_against_the_drag_and_keeps_the_distance() {
        let mut camera = ViewCamera::default();
        let right = camera.right();
        let before = camera.focus;
        camera.pan(100.0, 0.0);
        let moved = Vec3::new(
            camera.focus.x - before.x,
            camera.focus.y - before.y,
            camera.focus.z - before.z,
        );
        assert!(moved.x * right.x + moved.y * right.y < 0.0);
        let eye = camera.eye();
        let gap = Vec3::new(
            eye.x - camera.focus.x,
            eye.y - camera.focus.y,
            eye.z - camera.focus.z,
        );
        assert!((gap.length() - camera.distance).abs() < 1e-3);
    }
}
