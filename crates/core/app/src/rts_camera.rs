use mint_render::{Aabb, Camera, Ray, Vec3};

use crate::orbit::OrbitCamera;

const MIN_PITCH: f32 = 0.30;
const MAX_PITCH: f32 = 1.30;

const PAN_SPEED_PER_DISTANCE: f32 = 0.9;
const MIN_PAN_PER_SECOND: f32 = 8.0;

const DEFAULT_MIN_DISTANCE: f32 = 8.0;
const DEFAULT_MAX_DISTANCE: f32 = 4_000.0;

pub const DEFAULT_EDGE_SCROLL_MARGIN: f32 = 24.0;

const PAN_ACCELERATION: f32 = 9.0;
const PAN_DECELERATION: f32 = 14.0;

const ZOOM_RESPONSE: f32 = 12.0;

const GROUND_INSET_SCREEN_FRACTION: f32 = 0.95;
const GROUND_INSET_PROBE_VIEWPORT: (u32, u32) = (1_000, 1_000);

const MAX_FRAME_SECONDS: f32 = 0.1;

const ZOOM_SNAP_FRACTION: f32 = 1e-4;

fn approach(current: (f32, f32), target: (f32, f32), dt: f32) -> (f32, f32) {
    let delta = (target.0 - current.0, target.1 - current.1);
    let distance = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
    if !distance.is_finite() || distance == 0.0 {
        return target;
    }
    let speeding_up = (target.0 * target.0 + target.1 * target.1)
        > (current.0 * current.0 + current.1 * current.1);
    let rate = if speeding_up {
        PAN_ACCELERATION
    } else {
        PAN_DECELERATION
    };
    let step = rate * dt;
    if distance <= step {
        return target;
    }
    let scale = step / distance;
    (current.0 + delta.0 * scale, current.1 + delta.1 * scale)
}

#[derive(Debug, Clone, Copy)]
pub struct RtsCamera {
    orbit: OrbitCamera,
    pub pan_bounds: Option<Aabb>,
    pub pan_inset_policy: Option<fn(f32, f32) -> f32>,
    pan_input: (f32, f32),
    pan_velocity: (f32, f32),
    zoom_target: f32,
    scripted: Option<Camera>,
    posed: Option<Camera>,
    shake: Shake,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Shake {
    shakers: [Option<Shaker>; MAX_SHAKERS],
    angles: [f32; 3],
    rng: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Shaker {
    epicenter: Vec3,
    radius: f32,
    duration: f32,
    intensity: f32,
    elapsed: f32,
    omega: [f32; 3],
    phi: [f32; 3],
}

const MAX_SHAKERS: usize = 32;
const TAU: f32 = core::f32::consts::TAU;
const SHAKE_MIN_OMEGA: f32 = 12.5 * TAU;
const SHAKE_MAX_OMEGA: f32 = 15.0 * TAU;
const SHAKE_END_OMEGA: f32 = TAU;
const SHAKE_AXIS_ROTATION: [f32; 3] = [
    7.5 * core::f32::consts::PI / 180.0,
    15.0 * core::f32::consts::PI / 180.0,
    5.0 * core::f32::consts::PI / 180.0,
];

impl Default for Shake {
    fn default() -> Self {
        Self {
            shakers: [None; MAX_SHAKERS],
            angles: [0.0; 3],
            rng: 0x9e37_79b9_7f4a_7c15,
        }
    }
}

impl Shake {
    fn unit(&mut self) -> f32 {
        let mut state = self.rng;
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        self.rng = state;
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let unit = (state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as u32 as f32 / 16_777_216.0;
        unit
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    fn advance(&mut self, dt: f32, eye: Vec3) {
        for slot in &mut self.shakers {
            if let Some(shaker) = slot {
                shaker.elapsed += dt;
                if shaker.elapsed >= shaker.duration {
                    *slot = None;
                }
            }
        }
        let mut angles = [0.0f32; 3];
        for index in 0..MAX_SHAKERS {
            let Some(shaker) = self.shakers.get(index).copied().flatten() else {
                continue;
            };
            let distance = Vec3::new(
                eye.x - shaker.epicenter.x,
                eye.y - shaker.epicenter.y,
                eye.z - shaker.epicenter.z,
            )
            .length();
            if distance > shaker.radius {
                continue;
            }
            let intensity = shaker.intensity
                * (1.0 - distance / shaker.radius)
                * (1.0 - shaker.elapsed / shaker.duration);
            for ((angle_index, omega_start), (phi, amplitude)) in shaker
                .omega
                .into_iter()
                .enumerate()
                .zip(shaker.phi.into_iter().zip(SHAKE_AXIS_ROTATION))
            {
                let omega = omega_start + (SHAKE_END_OMEGA - omega_start) * shaker.elapsed;
                if let Some(angle) = angles.get_mut(angle_index) {
                    *angle +=
                        amplitude * intensity * mint_math::f32::sin(omega * shaker.elapsed + phi);
                }
                let minor = intensity * 0.5;
                for angle in &mut angles {
                    *angle += self.range(-minor, minor);
                }
            }
        }
        for (angle, limit) in angles.iter_mut().zip(SHAKE_AXIS_ROTATION) {
            *angle = angle.clamp(-limit, limit);
        }
        self.angles = angles;
    }
}

impl RtsCamera {
    #[must_use]
    pub fn new(up: Vec3) -> Self {
        let mut orbit = OrbitCamera::new(up);
        orbit.pitch = 0.9;
        orbit.distance = 200.0;
        orbit.min_distance = DEFAULT_MIN_DISTANCE;
        orbit.max_distance = DEFAULT_MAX_DISTANCE;
        let distance = orbit.distance;
        Self {
            orbit,
            pan_bounds: None,
            pan_inset_policy: None,
            pan_input: (0.0, 0.0),
            pan_velocity: (0.0, 0.0),
            zoom_target: distance,
            scripted: None,
            posed: None,
            shake: Shake::default(),
        }
    }

    #[must_use]
    pub const fn focus(&self) -> Vec3 {
        self.orbit.target
    }

    pub fn set_focus(&mut self, focus: Vec3) {
        self.scripted = None;
        self.orbit.target = focus;
    }

    pub fn jump_to(&mut self, focus: Vec3) {
        self.scripted = None;
        let clamped = self.clamp_to_bounds(focus);
        self.orbit.target = if clamped.is_finite() { clamped } else { focus };
    }

    pub fn keep_in_view(&mut self, world: &Aabb) {
        let mut reach = *world;
        if let Some(pan) = &self.pan_bounds {
            reach.union(pan);
        }
        if reach.is_empty() {
            return;
        }
        let diameter = reach.radius() * 2.0;
        self.orbit.far_reach = (diameter.is_finite() && diameter > 0.0).then_some(diameter);
    }

    #[must_use]
    pub const fn distance(&self) -> f32 {
        self.orbit.distance
    }

    #[must_use]
    pub const fn yaw(&self) -> f32 {
        self.orbit.yaw
    }

    #[must_use]
    pub const fn pitch(&self) -> f32 {
        self.orbit.pitch
    }

    #[must_use]
    pub fn eye(&self) -> Vec3 {
        self.posed
            .or(self.scripted)
            .map_or_else(|| self.orbit.eye(), |camera| camera.eye)
    }

    pub fn set_scripted(&mut self, camera: Option<Camera>) {
        self.scripted = camera;
    }

    pub fn set_posed(&mut self, camera: Option<Camera>) {
        self.posed = camera;
    }

    #[must_use]
    pub const fn is_posed(&self) -> bool {
        self.posed.is_some()
    }

    #[must_use]
    pub const fn is_scripted(&self) -> bool {
        self.scripted.is_some()
    }

    #[must_use]
    pub fn camera(&self) -> Camera {
        if let Some(posed) = self.posed {
            return posed;
        }
        let mut camera = self.scripted.unwrap_or_else(|| self.orbit.camera());
        let [pitch, yaw, roll] = self.shake.angles;
        if pitch == 0.0 && yaw == 0.0 && roll == 0.0 {
            return camera;
        }
        let view = Vec3::new(
            camera.target.x - camera.eye.x,
            camera.target.y - camera.eye.y,
            camera.target.z - camera.eye.z,
        );
        let distance = view.length();
        let side = view.cross(camera.up);
        if distance <= 0.0 || side.length() <= 0.0 {
            return camera;
        }
        let forward = view.normalize();
        let right = side.normalize();
        let up = right.cross(forward);
        let local_to_world = |v: [f32; 3]| {
            Vec3::new(
                right.x * v[0] + up.x * v[1] - forward.x * v[2],
                right.y * v[0] + up.y * v[1] - forward.y * v[2],
                right.z * v[0] + up.z * v[1] - forward.z * v[2],
            )
        };
        let rotate = |v: [f32; 3]| rotate_x(pitch, rotate_y(yaw, rotate_z(roll, v)));
        let shaken_forward = local_to_world(rotate([0.0, 0.0, -1.0]));
        camera.target = Vec3::new(
            camera.eye.x + shaken_forward.x * distance,
            camera.eye.y + shaken_forward.y * distance,
            camera.eye.z + shaken_forward.z * distance,
        );
        camera.up = local_to_world(rotate([0.0, 1.0, 0.0]));
        camera
    }

    pub fn shake(
        &mut self,
        epicenter: Vec3,
        radius: f32,
        amplitude_degrees: f32,
        duration_seconds: f32,
    ) {
        if !radius.is_finite()
            || radius <= 0.0
            || !amplitude_degrees.is_finite()
            || !duration_seconds.is_finite()
            || duration_seconds <= 0.0
        {
            return;
        }
        let mut shaker = Shaker {
            epicenter,
            radius,
            duration: duration_seconds,
            intensity: amplitude_degrees.to_radians(),
            elapsed: 0.0,
            omega: [0.0; 3],
            phi: [0.0; 3],
        };
        for (omega, phi) in shaker.omega.iter_mut().zip(shaker.phi.iter_mut()) {
            *omega = self.shake.range(SHAKE_MIN_OMEGA, SHAKE_MAX_OMEGA);
            *phi = self.shake.range(0.0, TAU);
        }
        let progress = |slot: &Option<Shaker>| slot.map_or(0.0, |s| s.elapsed / s.duration);
        let slot = self
            .shake
            .shakers
            .iter()
            .position(Option::is_none)
            .or_else(|| {
                self.shake
                    .shakers
                    .iter()
                    .enumerate()
                    .max_by(|a, b| progress(a.1).total_cmp(&progress(b.1)))
                    .map(|(index, _)| index)
            });
        if let Some(held) = slot.and_then(|index| self.shake.shakers.get_mut(index)) {
            *held = Some(shaker);
        }
    }

    pub fn pan(&mut self, dt: f32, forward: f32, right: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        if forward == 0.0 && right == 0.0 {
            return;
        }
        self.scripted = None;
        let Some((forward_dir, right_dir)) = self.ground_axes() else {
            return;
        };
        let speed = (self.orbit.distance * PAN_SPEED_PER_DISTANCE).max(MIN_PAN_PER_SECOND);
        let step = speed * dt;
        let target = self.orbit.target;
        let mut focus = Vec3::new(
            target.x + (forward_dir.x * forward + right_dir.x * right) * step,
            target.y + (forward_dir.y * forward + right_dir.y * right) * step,
            target.z + (forward_dir.z * forward + right_dir.z * right) * step,
        );
        focus = self.clamp_to_bounds(focus);
        if focus.is_finite() {
            self.orbit.target = focus;
        }
    }

    #[must_use]
    pub fn edge_pan_axes(
        pointer: Option<(f32, f32)>,
        viewport: (u32, u32),
        margin: f32,
    ) -> (f32, f32) {
        let Some((px, py)) = pointer else {
            return (0.0, 0.0);
        };
        let (width, height) = (viewport.0 as f32, viewport.1 as f32);
        if !(width > 0.0 && height > 0.0 && margin.is_finite() && margin > 0.0) {
            return (0.0, 0.0);
        }
        let mut forward = 0.0;
        let mut right = 0.0;
        if px <= margin {
            right -= 1.0;
        } else if px >= width - margin {
            right += 1.0;
        }
        if py <= margin {
            forward += 1.0;
        } else if py >= height - margin {
            forward -= 1.0;
        }
        (forward, right)
    }

    pub fn set_pan_input(&mut self, forward: f32, right: f32) {
        if !forward.is_finite() || !right.is_finite() {
            self.pan_input = (0.0, 0.0);
            return;
        }
        let length = (forward * forward + right * right).sqrt();
        self.pan_input = if length > 1.0 {
            (forward / length, right / length)
        } else {
            (forward, right)
        };
    }

    #[must_use]
    pub const fn pan_velocity(&self) -> (f32, f32) {
        self.pan_velocity
    }

    pub fn update(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(MAX_FRAME_SECONDS);

        let previous = self.pan_velocity;
        self.pan_velocity = approach(previous, self.pan_input, dt);
        let forward = (previous.0 + self.pan_velocity.0) * 0.5;
        let right = (previous.1 + self.pan_velocity.1) * 0.5;
        if forward != 0.0 || right != 0.0 {
            self.pan(dt, forward, right);
        }

        self.advance_zoom(dt);
        let eye = self.eye();
        self.shake.advance(dt, eye);
    }

    pub fn zoom(&mut self, lines: f32) {
        self.scripted = None;
        let mut ladder = self.orbit;
        ladder.distance = self.zoom_target;
        ladder.zoom(lines);
        self.zoom_target = ladder.distance;
    }

    #[must_use]
    pub const fn distance_range(&self) -> (f32, f32) {
        (self.orbit.min_distance, self.orbit.max_distance)
    }

    pub fn set_distance_range(&mut self, min: f32, max: f32) -> bool {
        if !(min.is_finite() && max.is_finite() && min > 0.0 && min <= max) {
            return false;
        }
        self.orbit.min_distance = min;
        self.orbit.max_distance = max;
        self.zoom_target = self.zoom_target.clamp(min, max);
        self.orbit.distance = self.orbit.distance.clamp(min, max);
        true
    }

    pub const DEFAULT_DISTANCE_RANGE: (f32, f32) = (DEFAULT_MIN_DISTANCE, DEFAULT_MAX_DISTANCE);

    #[must_use]
    pub const fn distance_target(&self) -> f32 {
        self.zoom_target
    }

    pub fn snap_zoom(&mut self) {
        if self.zoom_target.is_finite() {
            self.orbit.distance = self.zoom_target;
        }
    }

    fn advance_zoom(&mut self, dt: f32) {
        let target = self.zoom_target;
        let current = self.orbit.distance;
        if !target.is_finite() || !current.is_finite() {
            return;
        }
        let gap = target - current;
        if gap.abs() <= (target.abs() * ZOOM_SNAP_FRACTION).max(f32::EPSILON) {
            self.orbit.distance = target;
            return;
        }
        let closed = 1.0 - mint_math::f32::exp(-ZOOM_RESPONSE * dt);
        let next = current + gap * closed.clamp(0.0, 1.0);
        if next.is_finite() {
            self.orbit.distance = next;
        }
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.scripted = None;
        self.orbit.orbit(dx, dy);
        self.orbit.pitch = self.orbit.pitch.clamp(MIN_PITCH, MAX_PITCH);
    }

    const DEFAULT_YAW: f32 = 0.0;
    const DEFAULT_PITCH: f32 = 0.9;
    const DEFAULT_DISTANCE: f32 = 200.0;

    pub fn reset_orientation(&mut self) {
        self.scripted = None;
        self.orbit.yaw = Self::DEFAULT_YAW;
        self.orbit.pitch = Self::DEFAULT_PITCH;
        self.zoom_target = Self::DEFAULT_DISTANCE;
        self.snap_zoom();
    }

    pub fn place(&mut self, focus: Vec3, yaw: f32, pitch: f32, distance: f32) {
        if !(focus.is_finite() && yaw.is_finite() && pitch.is_finite() && distance.is_finite()) {
            return;
        }
        self.orbit.target = focus;
        self.orbit.yaw = yaw;
        self.orbit.pitch = pitch.clamp(MIN_PITCH, MAX_PITCH);
        let (min, max) = self.distance_range();
        self.orbit.distance = distance.clamp(min, max);
        self.zoom_target = self.orbit.distance;
    }

    pub fn set_pitch(&mut self, pitch: f32) {
        if pitch.is_finite() {
            self.orbit.pitch = pitch.clamp(MIN_PITCH, MAX_PITCH);
        }
    }

    pub fn set_fov_y(&mut self, fov_y: f32) {
        if fov_y.is_finite() && fov_y > 0.0 && fov_y < std::f32::consts::PI {
            self.orbit.fov_y = fov_y;
        }
    }

    fn ground_axes(&self) -> Option<(Vec3, Vec3)> {
        let up = self.orbit.up;
        let raw = {
            let eye = self.orbit.eye();
            let target = self.orbit.target;
            Vec3::new(target.x - eye.x, target.y - eye.y, target.z - eye.z)
        };
        let along_up = raw.dot(up);
        let flat = Vec3::new(
            raw.x - up.x * along_up,
            raw.y - up.y * along_up,
            raw.z - up.z * along_up,
        );
        let length = flat.length();
        if !length.is_finite() || length <= 1e-4 {
            return None;
        }
        let forward = Vec3::new(flat.x / length, flat.y / length, flat.z / length);
        let right = forward.cross(up);
        Some((forward, right))
    }

    fn clamp_to_bounds(&self, focus: Vec3) -> Vec3 {
        let Some(bounds) = &self.pan_bounds else {
            return focus;
        };
        if bounds.is_empty() {
            return focus;
        }
        let extent_x = bounds.max().x - bounds.min().x;
        let raw_inset = self.ground_inset();
        let inset = self
            .pan_inset_policy
            .map_or(raw_inset, |policy| policy(raw_inset, extent_x));
        let shrink = |lo: f32, hi: f32, value: f32| -> f32 {
            let axis_inset = inset.min((hi - lo) * 0.5).max(0.0);
            value.clamp(lo + axis_inset, hi - axis_inset)
        };
        Vec3::new(
            shrink(bounds.min().x, bounds.max().x, focus.x),
            shrink(bounds.min().y, bounds.max().y, focus.y),
            focus.z,
        )
    }

    #[must_use]
    pub fn ground_inset(&self) -> f32 {
        let camera = self.orbit.camera();
        let ground_z = camera.target.z;
        let hit = |py: f32| -> Option<Vec3> {
            let ray = camera.ray_through_pixel(
                GROUND_INSET_PROBE_VIEWPORT,
                GROUND_INSET_PROBE_VIEWPORT.0 as f32 * 0.5,
                py,
            )?;
            ground_plane_hit(ray, ground_z)
        };
        let Some(center) = hit(GROUND_INSET_PROBE_VIEWPORT.1 as f32 * 0.5) else {
            return 0.0;
        };
        let Some(near) = hit(GROUND_INSET_PROBE_VIEWPORT.1 as f32 * GROUND_INSET_SCREEN_FRACTION)
        else {
            return 0.0;
        };
        let dx = center.x - near.x;
        let dy = center.y - near.y;
        (dx * dx + dy * dy).sqrt()
    }
}

fn ground_plane_hit(ray: Ray, ground_z: f32) -> Option<Vec3> {
    if ray.direction.z.abs() <= 1e-6 {
        return None;
    }
    let t = (ground_z - ray.origin.z) / ray.direction.z;
    if !t.is_finite() || t <= 0.0 {
        return None;
    }
    Some(ray.at(t))
}

fn rotate_x(angle: f32, v: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    [v[0], v[1] * cos - v[2] * sin, v[1] * sin + v[2] * cos]
}

fn rotate_y(angle: f32, v: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    [v[0] * cos + v[2] * sin, v[1], -v[0] * sin + v[2] * cos]
}

fn rotate_z(angle: f32, v: [f32; 3]) -> [f32; 3] {
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos, v[2]]
}

#[cfg(test)]
mod range_tests {
    use super::*;

    #[test]
    fn a_distance_range_holds_the_zoom() {
        let mut camera = RtsCamera::new(Vec3::Z);
        assert_eq!(camera.distance_range(), RtsCamera::DEFAULT_DISTANCE_RANGE);
        assert!(camera.set_distance_range(300.0, 500.0));
        assert_eq!(camera.distance(), 300.0, "200 moved up into the range");
        assert_eq!(camera.distance_target(), 300.0);
        for _ in 0..50 {
            camera.zoom(10.0);
            camera.zoom(-10.0);
            camera.zoom(10.0);
        }
        camera.snap_zoom();
        assert!((300.0..=500.0).contains(&camera.distance()));
        assert!(!camera.set_distance_range(0.0, 10.0));
        assert!(!camera.set_distance_range(10.0, 5.0));
        assert!(!camera.set_distance_range(f32::NAN, 5.0));
        assert_eq!(camera.distance_range(), (300.0, 500.0));
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests;
