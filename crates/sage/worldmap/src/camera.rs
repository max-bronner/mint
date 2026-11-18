use mint_render::{Camera, Vec3};
use mint_sage_ini::living_world::presentation::MapInfoDef;

const DISTANCE_MIN: f32 = 1600.83;
const PITCH_MAX: f32 = core::f32::consts::FRAC_PI_3;
const HALF_FOV_X: f32 = 25.0 * core::f32::consts::PI / 180.0;
const MARGIN: f32 = 10.0;
const DEFAULT_ASPECT: f32 = 4.0 / 3.0;

#[must_use]
pub fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x + x * (x.sqrt() - x * x)
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldCamera {
    center: [f32; 2],
    extent: [f32; 2],
    bound: [f32; 2],
    pan: [f32; 2],
    zoom: f32,
    aspect: f32,
    zoom_per_tick: f32,
    dampen: f32,
    auto_scroll_speed: f32,
    max_auto_scroll_time: f32,
    zoom_velocity: f32,
    yaw: f32,
    fly: Option<Fly>,
    shell_pitch: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
struct Fly {
    from: ([f32; 2], f32, f32),
    to: ([f32; 2], f32, f32),
    frame: u32,
    frames: u32,
}

const FLY_FRAMES_PER_SECOND: f32 = 30.0;
const RESET_FRAMES: u32 = 7;

impl WorldCamera {
    #[must_use]
    pub fn new(info: &MapInfoDef) -> Self {
        let center = info.center.map_or([0.0, 0.0], |c| [c.x, c.y]);
        let extent = info
            .extent
            .map_or([1000.0, 1000.0], |e| [e.x.abs(), e.y.abs()]);
        let mut camera = Self {
            center,
            extent,
            bound: [
                info.camera_bound_x.unwrap_or(1.0).clamp(0.0, 1.0),
                info.camera_bound_y.unwrap_or(1.0).clamp(0.0, 1.0),
            ],
            pan: center,
            zoom: 1.0,
            aspect: DEFAULT_ASPECT,
            zoom_per_tick: info.mouse_wheel_zoom_per_tick.unwrap_or(0.01),
            dampen: info
                .mouse_wheel_zoom_dampen_factor
                .unwrap_or(0.82)
                .clamp(0.0, 0.999),
            auto_scroll_speed: info.auto_scroll_speed.unwrap_or(350.0),
            max_auto_scroll_time: info.max_auto_scroll_time.unwrap_or(1.5),
            zoom_velocity: 0.0,
            yaw: 0.0,
            fly: None,
            shell_pitch: None,
        };
        camera.clamp();
        camera
    }

    #[must_use]
    pub fn shell(info: &MapInfoDef) -> Self {
        let mut camera = Self::new(info);
        if let Some(zoom) = info.apt_zoom.filter(|z| z.is_finite()) {
            camera.zoom = zoom.clamp(0.0, 1.0);
        }
        if let Some(center) = info.apt_center.as_ref() {
            camera.pan = [center.x, center.y];
        }
        camera.shell_pitch = Some(info.apt_pitch.filter(|p| p.is_finite()).unwrap_or(0.0));
        camera
    }

    #[must_use]
    pub fn pan_point(&self) -> [f32; 2] {
        self.pan
    }

    #[must_use]
    pub fn focus(&self) -> [f32; 2] {
        let pull = ease(self.zoom);
        [
            self.pan[0] + (self.center[0] - self.pan[0]) * pull,
            self.pan[1] + (self.center[1] - self.pan[1]) * pull,
        ]
    }

    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    #[must_use]
    pub fn distance(&self) -> f32 {
        let tan = mint_math::f32::tan(HALF_FOV_X);
        let fit_x = (self.extent[0] - MARGIN) / tan;
        let fit_y = (self.extent[1] - MARGIN) * (4.0 / 3.0) / tan;
        let far = fit_x.max(fit_y);
        DISTANCE_MIN + (far - DISTANCE_MIN) * ease((self.zoom * self.zoom + self.zoom) * 0.5)
    }

    #[must_use]
    pub fn pitch(&self) -> f32 {
        self.shell_pitch
            .unwrap_or_else(|| PITCH_MAX * (1.0 - ease(self.zoom)))
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        if aspect.is_finite() && aspect > 0.0 {
            self.aspect = aspect;
            self.clamp();
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.fly = None;
        self.pan[0] += dx;
        self.pan[1] += dy;
        self.clamp();
    }

    pub fn scroll_by(&mut self, offset: [f32; 2], turn: f32) {
        let (ox, oy) = (offset[0] * 0.5, offset[1] * 0.5);
        if ox != 0.0 || oy != 0.0 {
            let (sin, cos) = mint_math::f32::sin_cos(-self.yaw);
            let scale = (self.zoom * 4.0 + 1.0) * 0.25;
            self.pan(
                (cos * ox - sin * oy) * scale,
                -(cos * oy + sin * ox) * scale,
            );
        }
        if turn != 0.0 && turn.is_finite() {
            self.fly = None;
            self.yaw = (self.yaw + 0.03 * turn).clamp(-1.0, 1.0);
        }
    }

    pub fn centre_on(&mut self, point: [f32; 2]) {
        self.pan = point;
        self.clamp();
    }

    pub fn fly_to(&mut self, point: [f32; 2], zoom: f32, heading: f32) {
        let zoom = if zoom.is_finite() {
            zoom.clamp(0.0, 1.0)
        } else {
            self.zoom
        };
        let yaw = if heading.is_finite() {
            heading
        } else {
            self.yaw
        };
        let (dx, dy) = (point[0] - self.pan[0], point[1] - self.pan[1]);
        let travel = (dx * dx + dy * dy).sqrt();
        let by_distance = travel / self.auto_scroll_speed;
        let by_zoom = (zoom - self.zoom).abs() / self.zoom_per_tick / FLY_FRAMES_PER_SECOND;
        let seconds = by_distance
            .max(by_zoom)
            .clamp(0.0, self.max_auto_scroll_time.max(0.0));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (FLY_FRAMES_PER_SECOND * seconds) as u32;
        self.fly_over(point, zoom, yaw, frames);
    }

    pub fn reset_view(&mut self) {
        if self.fly.is_none() {
            self.fly_over(self.pan, self.zoom, 0.0, RESET_FRAMES);
        }
    }

    fn fly_over(&mut self, point: [f32; 2], zoom: f32, yaw: f32, frames: u32) {
        self.zoom_velocity = 0.0;
        self.fly = Some(Fly {
            from: (self.pan, self.zoom, self.yaw),
            to: (point, zoom, yaw),
            frame: 0,
            frames,
        });
    }

    #[must_use]
    pub fn fly_frames(&self) -> u32 {
        self.fly.as_ref().map_or(0, |fly| fly.frames)
    }

    #[must_use]
    pub fn flying(&self) -> bool {
        self.fly.is_some()
    }

    #[must_use]
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    pub fn wheel(&mut self, lines: f32) {
        let can_move = (lines > 0.0 && self.zoom > 0.0) || (lines < 0.0 && self.zoom < 1.0);
        if lines.is_finite() && can_move {
            self.fly = None;
            self.zoom_velocity -= lines * self.zoom_per_tick;
        }
    }

    pub fn tick(&mut self) {
        if let Some(fly) = self.fly.as_mut() {
            fly.frame = fly.frame.saturating_add(1);
            let t = if fly.frame >= fly.frames {
                1.0
            } else {
                f32::from(u16::try_from(fly.frame).unwrap_or(u16::MAX))
                    / f32::from(u16::try_from(fly.frames).unwrap_or(u16::MAX))
            };
            let blend = t * t * (3.0 - 2.0 * t);
            let lerp = |a: f32, b: f32| a + (b - a) * blend;
            let (from, to) = (fly.from, fly.to);
            if t >= 1.0 {
                self.fly = None;
            }
            self.pan = [lerp(from.0[0], to.0[0]), lerp(from.0[1], to.0[1])];
            self.zoom = lerp(from.1, to.1);
            self.yaw = lerp(from.2, to.2);
            self.clamp();
            return;
        }
        self.zoom = (self.zoom + self.zoom_velocity).clamp(0.0, 1.0);
        self.zoom_velocity *= self.dampen;
        if self.zoom_velocity.abs() < 0.001 {
            self.zoom_velocity = 0.0;
        }
    }

    #[must_use]
    pub fn camera(&self) -> Camera {
        self.camera_for(self.aspect)
    }

    #[must_use]
    pub fn camera_for(&self, aspect: f32) -> Camera {
        let d = self.distance();
        let (sin, cos) = mint_math::f32::sin_cos(self.pitch());
        let back = if sin * d == 0.0 { 0.5 } else { d * sin };
        let focus = self.focus();
        let target = Vec3::new(focus[0], focus[1], 0.0);
        let psi = self.yaw * (1.0 - ease(self.zoom));
        let (sin_psi, cos_psi) = mint_math::f32::sin_cos(psi);
        let turn = |x: f32, y: f32| (x * cos_psi - y * sin_psi, x * sin_psi + y * cos_psi);
        let (eye_x, eye_y) = turn(0.0, -back);
        let (up_x, up_y) = turn(0.0, 1.0);
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            self.aspect
        };
        let tan_y = mint_math::f32::tan(HALF_FOV_X) / aspect;
        Camera {
            eye: Vec3::new(target.x + eye_x, target.y + eye_y, d * cos),
            target,
            up: Vec3::new(up_x, up_y, 0.0),
            fov_y: 2.0 * mint_math::f32::atan(tan_y),
            near: 10.0,
            far: d * 1.5 + 2000.0,
        }
    }

    #[must_use]
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let (half_width, near, far) = self.footprint();
        let half = [
            self.extent[0] * self.bound[0] - MARGIN,
            self.extent[1] * self.bound[1] - MARGIN,
        ];
        (
            [
                self.center[0] - half[0] + half_width,
                self.center[1] - half[1] + near,
            ],
            [
                self.center[0] + half[0] - half_width,
                self.center[1] + half[1] - far,
            ],
        )
    }

    fn footprint(&self) -> (f32, f32, f32) {
        let (sin, cos) = mint_math::f32::sin_cos(PITCH_MAX);
        let tan_x = mint_math::f32::tan(HALF_FOV_X);
        let tan_y = tan_x / self.aspect;
        let eye = [-DISTANCE_MIN * sin, DISTANCE_MIN * cos];
        let reach = |sy: f32| -> Option<(f32, f32)> {
            let dir = [sin + sy * tan_y * cos, -cos + sy * tan_y * sin];
            (dir[1] < 0.0).then(|| {
                let t = eye[1] / -dir[1];
                (t * tan_x, eye[0] + t * dir[0])
            })
        };
        let near = reach(-1.0).map_or(0.0, |(_, y)| -y);
        let (width, far) = reach(1.0).unwrap_or((0.0, 0.0));
        (width, near, far)
    }

    fn clamp(&mut self) {
        let (min, max) = self.bounds();
        let axes = self
            .pan
            .iter_mut()
            .zip(self.center)
            .zip(min.iter().zip(max));
        for ((value, center), (lo, hi)) in axes {
            *value = if !value.is_finite() || *lo > hi {
                center
            } else {
                value.clamp(*lo, hi)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_ini::living_world::Coord2;

    fn info() -> MapInfoDef {
        MapInfoDef {
            center: Some(Coord2 { x: 200.0, y: 800.0 }),
            extent: Some(Coord2 {
                x: 9000.0,
                y: 6000.0,
            }),
            camera_bound_x: Some(0.5),
            camera_bound_y: Some(0.6),
            apt_zoom: Some(0.2),
            apt_center: Some(Coord2 { x: 900.0, y: 900.0 }),
            apt_pitch: Some(0.9),
            ..MapInfoDef::default()
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1.0e-3 * b.abs().max(1.0)
    }

    #[test]
    fn the_strategic_view_starts_fully_out_over_center_ignoring_the_shell_fields() {
        let camera = WorldCamera::new(&info());
        assert_eq!(camera.zoom(), 1.0);
        assert_eq!(camera.focus(), [200.0, 800.0]);
        assert_eq!(camera.pitch(), 0.0);
    }

    #[test]
    fn distance_and_pitch_follow_the_zoom_formulas() {
        let mut camera = WorldCamera::new(&info());
        let tan = mint_math::f32::tan(HALF_FOV_X);
        let fit = ((9000.0f32 - 10.0) / tan).max((6000.0f32 - 10.0) * (4.0 / 3.0) / tan);
        assert!(close(camera.distance(), fit));
        camera.zoom = 0.0;
        assert!(close(camera.distance(), 1600.83));
        assert!(close(camera.pitch(), core::f32::consts::FRAC_PI_3));
        camera.zoom = 0.5;
        let e = ease(0.375);
        assert!(close(camera.distance(), 1600.83 + (fit - 1600.83) * e));
        assert!(close(
            camera.pitch(),
            core::f32::consts::FRAC_PI_3 * (1.0 - ease(0.5))
        ));
    }

    #[test]
    fn zooming_out_pulls_the_focus_toward_center() {
        let mut camera = WorldCamera::new(&info());
        camera.zoom = 0.0;
        camera.pan(-400.0, 300.0);
        let pan = camera.pan;
        assert_eq!(camera.focus(), pan);
        camera.zoom = 0.5;
        let pull = ease(0.5);
        let focus = camera.focus();
        assert!(close(focus[0], pan[0] + (200.0 - pan[0]) * pull));
        assert!(close(focus[1], pan[1] + (800.0 - pan[1]) * pull));
    }

    #[test]
    fn the_zoomed_in_footprint_stays_inside_the_bound_box() {
        let mut camera = WorldCamera::new(&info());
        camera.zoom = 0.0;
        let viewport = (800, 600);
        camera.set_aspect(4.0 / 3.0);
        for (dx, dy) in [(1.0e6, 1.0e6), (-1.0e6, -1.0e6)] {
            camera.pan(dx, dy);
            let view = camera.camera();
            let mut edges = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
            for (px, py) in [(0.0, 0.0), (800.0, 0.0), (0.0, 600.0), (800.0, 600.0)] {
                let ray = view.ray_through_pixel(viewport, px, py).expect("ray");
                let t = ray.origin.z / -ray.direction.z;
                let (x, y) = (
                    ray.origin.x + ray.direction.x * t,
                    ray.origin.y + ray.direction.y * t,
                );
                edges = [
                    edges[0].min(x),
                    edges[1].max(x),
                    edges[2].min(y),
                    edges[3].max(y),
                ];
            }
            assert!(edges[0] >= 200.0 - 4500.0 + 9.0 && edges[1] <= 200.0 + 4500.0 - 9.0);
            assert!(edges[2] >= 800.0 - 3600.0 + 9.0 && edges[3] <= 800.0 + 3600.0 - 9.0);
            let touching = (edges[1] - (200.0 + 4500.0 - 10.0)).abs() < 1.0
                || (edges[0] - (200.0 - 4500.0 + 10.0)).abs() < 1.0;
            assert!(touching, "footprint {edges:?}");
        }
    }

    #[test]
    fn a_wheel_tick_zooms_by_a_damped_series_and_stays_in_range() {
        let mut camera = WorldCamera::new(&info());
        camera.zoom = 0.5;
        camera.wheel(1.0);
        camera.tick();
        assert!((camera.zoom() - 0.49).abs() < 1.0e-6);
        camera.tick();
        assert!((camera.zoom() - (0.49 - 0.01 * 0.82)).abs() < 1.0e-6);
        camera.wheel(-1000.0);
        camera.tick();
        assert_eq!(camera.zoom(), 1.0);
    }

    fn frames_of_fly(camera: &mut WorldCamera) -> u32 {
        let mut frames = 0;
        while camera.flying() {
            camera.tick();
            frames += 1;
            assert!(frames < 1000);
        }
        frames
    }

    #[test]
    fn a_fly_lasts_the_longer_of_the_distance_and_the_zoom_time_up_to_the_cap() {
        let at = |dx: f32, zoom: f32| {
            let mut camera = WorldCamera::new(&info());
            camera.fly_to([200.0 + dx, 800.0], zoom, 0.0);
            frames_of_fly(&mut camera)
        };
        assert_eq!(at(180.0, 1.0), 15, "180 / 350 s, truncated");
        assert_eq!(at(0.0, 0.745), 25, "0.255 / 0.01 / 30 s, truncated");
        assert_eq!(at(180.0, 0.745), 25, "the longer of the two");
        assert_eq!(at(1000.0, 1.0), 45, "capped at MaxAutoScrollTime 1.5 s");
    }

    #[test]
    fn a_fly_reads_its_speed_and_cap_from_the_map_info() {
        let slow = MapInfoDef {
            auto_scroll_speed: Some(100.0),
            max_auto_scroll_time: Some(1.0),
            ..info()
        };
        let mut camera = WorldCamera::new(&slow);
        camera.fly_to([200.0 + 60.0, 800.0], 1.0, 0.0);
        assert_eq!(frames_of_fly(&mut camera), 18, "60 / 100 s");
        camera.fly_to([200.0 + 60.0 + 500.0, 800.0], 1.0, 0.0);
        assert_eq!(frames_of_fly(&mut camera), 30, "capped at 1 s");
    }

    #[test]
    fn a_fly_eases_by_the_hermite_cubic_and_lands_on_the_point_not_the_focus() {
        let mut camera = WorldCamera::new(&info());
        camera.fly_to([200.0 + 470.0, 800.0], 0.8, -0.47);
        for _ in 0..10 {
            camera.tick();
        }
        let eased = 3.0 * (0.25f32 * 0.25) - 2.0 * (0.25f32 * 0.25 * 0.25);
        assert!(close(camera.pan[0], 200.0 + 470.0 * eased));
        assert!(close(camera.zoom(), 1.0 - 0.2 * eased));
        assert_eq!(frames_of_fly(&mut camera), 30);
        assert!(close(camera.pan[0], 670.0) && close(camera.pan[1], 800.0));
        assert!(close(camera.zoom(), 0.8));
        assert!(close(camera.yaw(), -0.47));
        assert!(
            camera.focus()[0] < camera.pan[0] - 1.0,
            "the pull is not undone"
        );
    }

    #[test]
    fn a_fly_to_a_point_outside_the_bounds_stops_at_the_bounds() {
        let mut camera = WorldCamera::new(&info());
        camera.fly_to([1.0e6, 800.0], 1.0, 0.0);
        frames_of_fly(&mut camera);
        assert!(close(camera.pan[0], camera.bounds().1[0]));
    }

    #[test]
    fn a_wheel_line_toward_a_resting_end_adds_no_speed() {
        let mut camera = WorldCamera::new(&info());
        camera.wheel(-3.0);
        camera.tick();
        camera.wheel(1.0);
        camera.tick();
        assert!((camera.zoom() - 0.99).abs() < 1.0e-6);
    }
}
