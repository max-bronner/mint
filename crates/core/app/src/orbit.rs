use mint_render::{Aabb, Camera, Vec3, DEFAULT_FOV_Y};

const PITCH_LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;

const RADIANS_PER_PIXEL: f32 = 0.008;

const INVERT_VERTICAL_DRAG: bool = true;

const VERTICAL_DRAG_SIGN: f32 = if INVERT_VERTICAL_DRAG { 1.0 } else { -1.0 };

const ZOOM_PER_LINE: f32 = 1.15;

const MIN_ZOOM_STEP_OF_MIN_DISTANCE: f32 = 1.0;

const FRAMING_MARGIN: f32 = 1.15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub up: Vec3,
    pub fov_y: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub far_reach: Option<f32>,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self::new(Vec3::Y)
    }
}

impl OrbitCamera {
    #[must_use]
    pub fn new(up: Vec3) -> Self {
        let up = if up.length_squared() > 0.0 {
            up.normalize()
        } else {
            Vec3::Y
        };
        Self {
            target: Vec3::ZERO,
            distance: 10.0,
            yaw: 0.0,
            pitch: 0.35,
            up,
            fov_y: DEFAULT_FOV_Y,
            min_distance: 0.01,
            max_distance: 100_000.0,
            far_reach: None,
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.yaw = mint_math::f32::wrap_pi(self.yaw - dx * RADIANS_PER_PIXEL);
        self.pitch = (self.pitch + VERTICAL_DRAG_SIGN * dy * RADIANS_PER_PIXEL)
            .clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn zoom(&mut self, lines: f32) {
        if !lines.is_finite() || lines == 0.0 {
            return;
        }
        let factor = mint_math::f32::powf(ZOOM_PER_LINE, -lines);
        let proportional = self.distance * factor;
        let floor = self.min_distance * MIN_ZOOM_STEP_OF_MIN_DISTANCE * lines.abs();
        let distance = if lines > 0.0 {
            proportional.min(self.distance - floor)
        } else {
            proportional.max(self.distance + floor)
        };
        if distance.is_finite() {
            self.distance = distance.clamp(self.min_distance, self.max_distance);
        }
    }

    pub fn frame(&mut self, bounds: &Aabb, aspect: f32) {
        if bounds.is_empty() {
            return;
        }
        let radius = bounds.radius();
        self.target = bounds.center();
        let radius = if radius > 0.0 { radius } else { 1.0 };
        self.min_distance = radius * 0.05;
        self.max_distance = radius * 100.0;
        self.far_reach = Some(radius * 2.0);

        let (pitch, yaw) = initial_direction(bounds, self.up);
        self.pitch = pitch;
        self.yaw = yaw;

        let half_fov = self.fov_y * 0.5;
        let tangent = mint_math::f32::tan(half_fov);
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let distance = if tangent > 0.0 {
            let (right, true_up) = self.view_axes();
            let half = half_extents(bounds);
            let extent_right = project_extent(half, right);
            let extent_up = project_extent(half, true_up);
            let vertical = extent_up * FRAMING_MARGIN / tangent;
            let horizontal = extent_right * FRAMING_MARGIN / (tangent * aspect);
            let fitted = vertical.max(horizontal);
            if fitted > 0.0 {
                fitted
            } else {
                radius * FRAMING_MARGIN / mint_math::f32::sin(half_fov).max(1e-6)
            }
        } else {
            radius * 4.0
        };

        self.distance = distance.clamp(self.min_distance, self.max_distance);
    }

    fn view_axes(&self) -> (Vec3, Vec3) {
        let back = self.direction();
        let right = self.up.cross(back).normalize();
        let true_up = back.cross(right);
        (right, true_up)
    }

    fn direction(&self) -> Vec3 {
        let (east, north) = self.basis();
        let (sin_pitch, cos_pitch) = mint_math::f32::sin_cos(self.pitch);
        let (sin_yaw, cos_yaw) = mint_math::f32::sin_cos(self.yaw);
        Vec3::new(
            (east.x * cos_yaw + north.x * sin_yaw) * cos_pitch + self.up.x * sin_pitch,
            (east.y * cos_yaw + north.y * sin_yaw) * cos_pitch + self.up.y * sin_pitch,
            (east.z * cos_yaw + north.z * sin_yaw) * cos_pitch + self.up.z * sin_pitch,
        )
    }

    #[must_use]
    pub fn eye(&self) -> Vec3 {
        let direction = self.direction();
        Vec3::new(
            self.target.x + direction.x * self.distance,
            self.target.y + direction.y * self.distance,
            self.target.z + direction.z * self.distance,
        )
    }

    #[must_use]
    pub fn camera(&self) -> Camera {
        let near = (self.distance * 0.005).max(1e-3);
        let reach = self
            .far_reach
            .filter(|reach| reach.is_finite() && *reach > 0.0);
        let far = (self.distance * 10.0)
            .max(near * 1000.0)
            .max(reach.map_or(0.0, |reach| self.distance + reach));
        Camera {
            eye: self.eye(),
            target: self.target,
            up: self.up,
            fov_y: self.fov_y,
            near,
            far,
        }
    }

    fn basis(&self) -> (Vec3, Vec3) {
        let reference = if self.up.z.abs() < 0.9 {
            Vec3::Z
        } else {
            Vec3::X
        };
        let east = reference.cross(self.up).normalize();
        let north = self.up.cross(east);
        (east, north)
    }
}

fn half_extents(bounds: &Aabb) -> Vec3 {
    let min = bounds.min();
    let max = bounds.max();
    Vec3::new(
        (max.x - min.x) * 0.5,
        (max.y - min.y) * 0.5,
        (max.z - min.z) * 0.5,
    )
}

fn project_extent(half: Vec3, axis: Vec3) -> f32 {
    half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs()
}

fn initial_direction(bounds: &Aabb, up: Vec3) -> (f32, f32) {
    const FRONT: (f32, f32) = (0.0, 0.0);
    if bounds.is_empty() {
        return FRONT;
    }
    let extent = half_extents(bounds);
    let axes = [
        (extent.x, Vec3::X),
        (extent.y, Vec3::Y),
        (extent.z, Vec3::Z),
    ];
    let Some(&(thin, normal)) = axes
        .iter()
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
    else {
        return FRONT;
    };
    let widest_other = axes
        .iter()
        .filter(|(extent, axis)| *axis != normal && *extent > 0.0)
        .fold(0.0f32, |acc, (extent, _)| acc.max(*extent));
    if widest_other <= 0.0 || thin > widest_other * FLAT_RATIO {
        return FRONT;
    }

    let vertical = normal.dot(up).clamp(-1.0, 1.0);
    if vertical.abs() > 0.99 {
        return FRONT;
    }
    let pitch = mint_math::f32::asin(vertical).clamp(-PITCH_LIMIT, PITCH_LIMIT);

    let reference = if up.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
    let east = reference.cross(up).normalize();
    let north = up.cross(east);
    let horizontal = Vec3::new(
        normal.x - up.x * vertical,
        normal.y - up.y * vertical,
        normal.z - up.z * vertical,
    );
    let horizontal_length = horizontal.length();
    if horizontal_length <= 0.0 {
        return FRONT;
    }
    let cos_yaw = horizontal.dot(east) / horizontal_length;
    let sin_yaw = horizontal.dot(north) / horizontal_length;
    let yaw = mint_math::f32::atan2(sin_yaw, cos_yaw);
    (pitch, yaw)
}

const FLAT_RATIO: f32 = 0.2;

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    #[test]
    fn the_eye_is_always_exactly_the_stated_distance_from_the_target() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.target = Vec3::new(3.0, -2.0, 7.0);
        camera.distance = 12.5;
        for (dx, dy) in [(0.0, 0.0), (100.0, 40.0), (-350.0, -900.0), (17.0, -3.0)] {
            camera.orbit(dx, dy);
            let eye = camera.eye();
            let offset = Vec3::new(
                eye.x - camera.target.x,
                eye.y - camera.target.y,
                eye.z - camera.target.z,
            );
            assert!(
                close(offset.length(), 12.5),
                "distance drifted to {}",
                offset.length()
            );
        }
    }

    #[test]
    fn pitch_is_clamped_short_of_the_pole_where_look_at_degenerates() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.orbit(0.0, -100_000.0);
        assert!(camera.pitch <= PITCH_LIMIT);
        camera.orbit(0.0, 100_000.0);
        assert!(camera.pitch >= -PITCH_LIMIT);

        assert!(camera
            .camera()
            .view_projection(1.0)
            .to_cols_array()
            .iter()
            .all(|v| v.is_finite()));
    }

    #[test]
    fn dragging_up_tilts_the_camera_the_way_the_owner_asked_for() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.pitch = 0.0;
        camera.orbit(0.0, -50.0);
        let after_up = camera.pitch;

        camera.pitch = 0.0;
        camera.orbit(0.0, 50.0);
        let after_down = camera.pitch;

        assert!(
            after_up < 0.0 && after_down > 0.0,
            "inverted: up should lower the camera. up {after_up}, down {after_down}"
        );
        assert_eq!(
            VERTICAL_DRAG_SIGN, 1.0,
            "this test asserts the inverted convention; flipping              INVERT_VERTICAL_DRAG has to flip the expectations above with it"
        );
    }

    #[test]
    fn a_drag_and_its_opposite_cancel_whichever_convention_is_in_force() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.pitch = 0.1;
        camera.orbit(30.0, 40.0);
        camera.orbit(-30.0, -40.0);
        assert!(
            (camera.pitch - 0.1).abs() < 1e-5 && camera.yaw.abs() < 1e-5,
            "pitch {} yaw {}",
            camera.pitch,
            camera.yaw
        );
    }

    #[test]
    fn yaw_wraps_rather_than_growing_without_bound() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        for _ in 0..200 {
            camera.orbit(100.0, 0.0);
        }
        assert!(
            camera.yaw.abs() <= std::f32::consts::PI + 1e-4,
            "yaw {} left the wrapped range",
            camera.yaw
        );
    }

    #[test]
    fn at_zero_pitch_the_eye_lies_in_the_targets_horizontal_plane() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.pitch = 0.0;
        camera.target = Vec3::new(1.0, 2.0, 3.0);
        camera.distance = 5.0;
        assert!(close(camera.eye().z, 3.0), "eye {:?}", camera.eye());
    }

    #[test]
    fn at_full_pitch_the_eye_is_almost_directly_above_the_target() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.pitch = PITCH_LIMIT;
        camera.distance = 10.0;
        let eye = camera.eye();
        assert!(eye.z > 9.99, "eye {eye:?} should be nearly overhead");
    }

    #[test]
    fn the_up_axis_decides_which_way_overhead_is() {
        let mut camera = OrbitCamera::new(Vec3::Y);
        camera.pitch = PITCH_LIMIT;
        camera.distance = 10.0;
        assert!(camera.eye().y > 9.99, "Y-up content orbits about Y");
    }

    #[test]
    fn scrolling_forwards_moves_closer_and_backwards_moves_away() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.distance = 10.0;
        camera.zoom(1.0);
        assert!(camera.distance < 10.0, "distance {}", camera.distance);
        camera.zoom(-1.0);
        assert!(
            close(camera.distance, 10.0),
            "zooming out should undo zooming in, got {}",
            camera.distance
        );
    }

    #[test]
    fn zoom_is_multiplicative_so_the_step_scales_with_the_model() {
        let mut near = OrbitCamera::new(Vec3::Z);
        near.distance = 1.0;
        near.min_distance = 0.001;
        let mut far = OrbitCamera::new(Vec3::Z);
        far.distance = 1000.0;

        near.zoom(1.0);
        far.zoom(1.0);
        assert!(
            close(far.distance / 1000.0, near.distance / 1.0),
            "the same detent should be the same proportion: {} vs {}",
            far.distance / 1000.0,
            near.distance
        );
    }

    #[test]
    fn zoom_cannot_be_driven_through_the_target_or_out_to_infinity() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.frame(&Aabb::from_corners(Vec3::ZERO, Vec3::ONE), 1.0);
        for _ in 0..1000 {
            camera.zoom(10.0);
        }
        assert!(camera.distance >= camera.min_distance);
        assert!(camera.distance > 0.0);
        for _ in 0..1000 {
            camera.zoom(-10.0);
        }
        assert!(camera.distance <= camera.max_distance);
        assert!(camera.distance.is_finite());
    }

    #[test]
    fn framing_centres_on_the_model_and_backs_off_enough_to_contain_it() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        let bounds = Aabb::from_corners(Vec3::new(-2.0, -2.0, 0.0), Vec3::new(6.0, 2.0, 4.0));
        camera.frame(&bounds, 1.0);

        assert_eq!(camera.target, bounds.center());

        let view_projection = camera.camera().view_projection(1.0);
        for x in [bounds.min().x, bounds.max().x] {
            for y in [bounds.min().y, bounds.max().y] {
                for z in [bounds.min().z, bounds.max().z] {
                    let ndc = view_projection.transform_point(Vec3::new(x, y, z));
                    assert!(
                        ndc.y.abs() <= 1.0 && ndc.z > 0.0 && ndc.z < 1.0,
                        "corner ({x}, {y}, {z}) fell outside the frame at {ndc:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn framing_a_huge_model_and_a_tiny_one_both_produce_usable_depth_ranges() {
        for scale in [0.001f32, 1.0, 10_000.0] {
            let mut camera = OrbitCamera::new(Vec3::Z);
            camera.frame(
                &Aabb::from_corners(Vec3::splat(-scale), Vec3::splat(scale)),
                1.0,
            );
            let view = camera.camera();
            assert!(view.near > 0.0 && view.near < view.far, "scale {scale}");
            assert!(
                view.near < camera.distance && view.far > camera.distance,
                "scale {scale}: the model must sit between the clip planes"
            );
        }
    }

    #[test]
    fn zooming_all_the_way_in_keeps_the_far_corner_of_the_model_in_front_of_the_far_plane() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        let bounds = Aabb::from_corners(Vec3::splat(0.0), Vec3::splat(5_000.0));
        camera.frame(&bounds, 1.0);
        camera.distance = camera.min_distance;
        let view = camera.camera();
        let away = |corner: Vec3| (corner - bounds.center()).length();
        let furthest = away(Vec3::splat(0.0)).max(away(Vec3::splat(5_000.0)));
        assert!(
            view.far > camera.distance + furthest,
            "far {} does not reach {} past the eye",
            view.far,
            camera.distance + furthest
        );
    }

    #[test]
    fn framing_an_empty_box_leaves_the_camera_where_it_was() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.target = Vec3::new(1.0, 2.0, 3.0);
        let before = camera;
        camera.frame(&Aabb::EMPTY, 1.0);
        assert_eq!(camera, before);
    }

    #[test]
    fn framing_a_single_point_still_gives_a_finite_camera() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        camera.frame(&Aabb::from_corners(Vec3::ONE, Vec3::ONE), 1.0);
        assert!(camera.distance.is_finite() && camera.distance > 0.0);
        assert!(camera
            .camera()
            .view_projection(1.0)
            .to_cols_array()
            .iter()
            .all(|v| v.is_finite()));
    }

    #[test]
    fn a_non_finite_drag_is_ignored_rather_than_poisoning_the_camera() {
        let mut camera = OrbitCamera::new(Vec3::Z);
        let before = camera;
        camera.orbit(f32::NAN, 1.0);
        camera.orbit(1.0, f32::INFINITY);
        camera.zoom(f32::NAN);
        assert_eq!(camera, before);
    }

    #[test]
    fn a_degenerate_up_vector_falls_back_rather_than_producing_nan() {
        let camera = OrbitCamera::new(Vec3::ZERO);
        assert_eq!(camera.up, Vec3::Y);
        assert!(camera.eye().is_finite());
    }
}
