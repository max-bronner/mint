use mint_math::f32::{atan, cos, sin, tan};
use mint_render::{Camera, Vec3};

const OFFSET_PER_MAX_HEIGHT: f32 = 1.642_665;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BfmeView {
    pub look_at: [f32; 2],
    pub ground: f32,
    pub zoom: f32,
    pub yaw: f32,
    pub fx_pitch: f32,
    pub roll: f32,
    pub hfov: f32,
    pub offset: Vec3,
    pub height_adjust: f32,
}

impl BfmeView {
    #[must_use]
    pub fn default_offset(max_height: f32, pitch: f32) -> Vec3 {
        let length = max_height * OFFSET_PER_MAX_HEIGHT;
        Vec3::new(0.0, -length * cos(pitch), length * sin(pitch))
    }

    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        let turn = (other.yaw - self.yaw + core::f32::consts::PI)
            .rem_euclid(core::f32::consts::TAU)
            - core::f32::consts::PI;
        Self {
            look_at: [
                mix(self.look_at[0], other.look_at[0]),
                mix(self.look_at[1], other.look_at[1]),
            ],
            ground: mix(self.ground, other.ground),
            zoom: mix(self.zoom, other.zoom),
            yaw: self.yaw + turn * t,
            fx_pitch: mix(self.fx_pitch, other.fx_pitch),
            roll: mix(self.roll, other.roll),
            hfov: mix(self.hfov, other.hfov),
            offset: self.offset,
            height_adjust: mix(self.height_adjust, other.height_adjust),
        }
    }
}

#[must_use]
pub fn bfme_camera(view: &BfmeView, aspect: f32, near: f32, far: f32) -> Camera {
    let [x, y] = view.look_at;
    let offset = Vec3::new(
        view.offset.x * view.zoom,
        view.offset.y * view.zoom,
        view.offset.z * view.zoom,
    );
    let scale = if offset.z.abs() > f32::EPSILON {
        1.0 - view.ground / offset.z
    } else {
        1.0
    };
    let (sin_t, cos_t) = (sin(view.yaw), cos(view.yaw));
    let mut eye = Vec3::new(
        x - offset.y * scale * sin_t,
        y + offset.y * scale * cos_t,
        offset.z * scale + view.ground,
    );
    let mut target = Vec3::new(x, y, view.ground + view.height_adjust);
    let fx = view.fx_pitch;
    if fx <= 1.0 {
        target.z = eye.z - (eye.z - target.z) * fx;
    } else {
        eye.x = target.x + (eye.x - target.x) / fx;
        eye.y = target.y + (eye.y - target.y) / fx;
    }
    let up = rolled_up(
        Vec3::new(target.x - eye.x, target.y - eye.y, target.z - eye.z),
        view.roll,
    );
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    };
    Camera {
        eye,
        target,
        up,
        fov_y: 2.0 * atan(tan(view.hfov * 0.5) / aspect),
        near,
        far,
    }
}

fn rolled_up(view: Vec3, roll: f32) -> Vec3 {
    if roll == 0.0 || view.length() <= 0.0 {
        return Vec3::Z;
    }
    let axis = view.normalize();
    let (s, c) = (sin(roll), cos(roll));
    let cross = axis.cross(Vec3::Z);
    let along = axis.dot(Vec3::Z) * (1.0 - c);
    Vec3::new(
        Vec3::Z.x * c + cross.x * s + axis.x * along,
        Vec3::Z.y * c + cross.y * s + axis.y * along,
        Vec3::Z.z * c + cross.z * s + axis.z * along,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> BfmeView {
        BfmeView {
            look_at: [100.0, 40.0],
            ground: 0.0,
            zoom: 1.0,
            yaw: 0.0,
            fx_pitch: 1.0,
            roll: 0.0,
            hfov: 0.83,
            offset: Vec3::new(0.0, -80.0, 60.0),
            height_adjust: 0.0,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_eye_sits_behind_the_target_and_the_fx_pitch_moves_one_end() {
        let flat = bfme_camera(&view(), 1.0, 1.0, 100.0);
        assert!(close(flat.eye.x, 100.0) && close(flat.eye.y, -40.0) && close(flat.eye.z, 60.0));
        assert!(close(flat.target.x, 100.0) && close(flat.target.z, 0.0));

        let level = bfme_camera(
            &BfmeView {
                fx_pitch: 0.5,
                ..view()
            },
            1.0,
            1.0,
            100.0,
        );
        assert!(close(level.target.z, 30.0) && close(level.eye.z, 60.0));

        let tucked = bfme_camera(
            &BfmeView {
                fx_pitch: 2.0,
                ..view()
            },
            1.0,
            1.0,
            100.0,
        );
        assert!(close(tucked.eye.y, 0.0) && close(tucked.eye.z, 60.0));
        assert!(close(tucked.target.z, 0.0));
    }

    #[test]
    fn ground_raises_the_target_but_not_the_eye_height() {
        let raised = bfme_camera(
            &BfmeView {
                ground: 10.0,
                ..view()
            },
            1.0,
            1.0,
            100.0,
        );
        assert!(close(raised.eye.z, 60.0) && close(raised.target.z, 10.0));
    }

    #[test]
    fn the_horizontal_fov_narrows_the_vertical_one_on_a_wide_viewport() {
        let wide = bfme_camera(&view(), 2.0, 1.0, 100.0);
        let square = bfme_camera(&view(), 1.0, 1.0, 100.0);
        assert!(close(square.fov_y, 0.83) && wide.fov_y < square.fov_y);
    }

    #[test]
    fn a_roll_turns_the_up_vector_and_zero_roll_keeps_it_vertical() {
        assert_eq!(bfme_camera(&view(), 1.0, 1.0, 100.0).up, Vec3::Z);
        let rolled = bfme_camera(
            &BfmeView {
                roll: 0.5,
                ..view()
            },
            1.0,
            1.0,
            100.0,
        );
        assert!(rolled.up.dot(Vec3::Z) < 0.99);
    }
}
