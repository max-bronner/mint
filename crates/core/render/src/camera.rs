use crate::depth::DepthDirection;
use crate::math::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 5.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: DEFAULT_FOV_Y,
            near: 0.1,
            far: 1000.0,
        }
    }
}

pub const DEFAULT_FOV_Y: f32 = std::f32::consts::FRAC_PI_4;

impl Camera {
    #[must_use]
    pub fn view(&self) -> Mat4 {
        Mat4::look_at(self.eye, self.target, self.up)
    }

    #[must_use]
    pub fn projection(&self, aspect: f32) -> Mat4 {
        self.projection_with_depth(DepthDirection::default(), aspect)
    }

    #[must_use]
    pub fn projection_with_depth(&self, direction: DepthDirection, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        match direction {
            DepthDirection::Standard => Mat4::perspective(self.fov_y, aspect, self.near, self.far),
            DepthDirection::Reversed => {
                Mat4::perspective_reversed_z(self.fov_y, aspect, self.near, self.far)
            }
        }
    }

    #[must_use]
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection(aspect).mul_mat4(self.view())
    }

    #[must_use]
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.target.x - self.eye.x,
            self.target.y - self.eye.y,
            self.target.z - self.eye.z,
        )
        .normalize()
    }

    #[must_use]
    pub fn frustum_corners(&self, aspect: f32, near: f32, far: f32) -> [Vec3; 8] {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let near = if near.is_finite() { near.max(0.0) } else { 0.0 };
        let far = if far.is_finite() { far.max(near) } else { near };
        let forward = {
            let towards = self.forward();
            if towards.length_squared() > 0.0 {
                towards
            } else {
                Vec3::new(0.0, 0.0, -1.0)
            }
        };
        let right = {
            let side = forward.cross(self.up).normalize();
            if side.length_squared() > 0.0 {
                side
            } else {
                let axis = if forward.z.abs() > 0.9 {
                    Vec3::Y
                } else {
                    Vec3::Z
                };
                forward.cross(axis).normalize()
            }
        };
        let up = right.cross(forward);
        let fov_y = if self.fov_y.is_finite() {
            self.fov_y
        } else {
            DEFAULT_FOV_Y
        };
        let tan_half = mint_math::f32::tan((fov_y * 0.5).clamp(MIN_HALF_FOV, MAX_HALF_FOV));
        let half_height_near = near * tan_half;
        let half_height_far = far * tan_half;
        let quad = |distance: f32, half_height: f32| {
            let half_width = half_height * aspect;
            let at = |sx: f32, sy: f32| {
                Vec3::new(
                    self.eye.x + forward.x * distance + right.x * sx + up.x * sy,
                    self.eye.y + forward.y * distance + right.y * sx + up.y * sy,
                    self.eye.z + forward.z * distance + right.z * sx + up.z * sy,
                )
            };
            [
                at(-half_width, -half_height),
                at(half_width, -half_height),
                at(-half_width, half_height),
                at(half_width, half_height),
            ]
        };
        let [n0, n1, n2, n3] = quad(near, half_height_near);
        let [f0, f1, f2, f3] = quad(far, half_height_far);
        [n0, n1, n2, n3, f0, f1, f2, f3]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    #[must_use]
    pub fn at(self, t: f32) -> Vec3 {
        Vec3::new(
            self.origin.x + self.direction.x * t,
            self.origin.y + self.direction.y * t,
            self.origin.z + self.direction.z * t,
        )
    }
}

impl Camera {
    #[must_use]
    pub fn ray_through_pixel(&self, viewport: (u32, u32), px: f32, py: f32) -> Option<Ray> {
        let width = f32::from(u16::try_from(viewport.0).ok()?);
        let height = f32::from(u16::try_from(viewport.1).ok()?);
        if width <= 0.0 || height <= 0.0 || !px.is_finite() || !py.is_finite() {
            return None;
        }
        let projection = self.projection(width / height);
        let (fx, fy) = (projection.get(0, 0), projection.get(1, 1));
        if fx == 0.0 || fy == 0.0 || !fx.is_finite() || !fy.is_finite() {
            return None;
        }
        let ndc_x = 2.0 * px / width - 1.0;
        let ndc_y = 1.0 - 2.0 * py / height;

        let back = Vec3::new(
            self.eye.x - self.target.x,
            self.eye.y - self.target.y,
            self.eye.z - self.target.z,
        )
        .normalize();
        let right = self.up.cross(back).normalize();
        let up = back.cross(right);
        let direction = Vec3::new(
            -back.x + right.x * (ndc_x / fx) + up.x * (ndc_y / fy),
            -back.y + right.y * (ndc_x / fx) + up.y * (ndc_y / fy),
            -back.z + right.z * (ndc_x / fx) + up.z * (ndc_y / fy),
        )
        .normalize();
        if !direction.is_finite() || direction.length_squared() <= 0.0 {
            return None;
        }
        Some(Ray {
            origin: self.eye,
            direction,
        })
    }
}

const MIN_HALF_FOV: f32 = 1e-4;
const MAX_HALF_FOV: f32 = std::f32::consts::FRAC_PI_2 - 1e-4;

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
