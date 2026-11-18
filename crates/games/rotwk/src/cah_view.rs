use mint_render::Vec3;
use mint_sage_ini::{ViewInfoDef, ViewPreset};
use mint_sage_scene::camera_transform::BfmeView;

const STEPS: (f32, f32) = (0.1, 0.05);
const SHIFT_STEPS: (f32, f32) = (0.2, 0.1);
const HFOV: f32 = 0.83;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Preset {
    pub(crate) pitch: f32,
    pub(crate) zoom: f32,
    pub(crate) floor: f32,
    pub(crate) dist: f32,
    pub(crate) shift: f32,
}

impl Preset {
    const NEAR: Self = Self {
        pitch: 0.3,
        zoom: 0.2,
        floor: 50.0,
        dist: 30.0,
        shift: 0.0,
    };
    const FAR: Self = Self {
        pitch: 0.4,
        zoom: 0.3,
        floor: 70.0,
        dist: 50.0,
        shift: 0.0,
    };

    fn read(def: &ViewPreset, default: Self) -> Self {
        Self {
            pitch: def.pitch.unwrap_or(default.pitch),
            zoom: def.zoom.unwrap_or(default.zoom),
            floor: def.floor.unwrap_or(default.floor),
            dist: def.dist.unwrap_or(default.dist),
            shift: def.shift.unwrap_or(default.shift),
        }
    }

    fn between(near: Self, far: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self {
            pitch: mix(near.pitch, far.pitch),
            zoom: mix(near.zoom, far.zoom),
            floor: mix(near.floor, far.floor),
            dist: mix(near.dist, far.dist),
            shift: mix(near.shift, far.shift),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewSet {
    pub(crate) near: Preset,
    pub(crate) far: Preset,
    pub(crate) close_up: Preset,
    pub(crate) portrait: Preset,
    pub(crate) normal_cam: f32,
    pub(crate) camera_angle: f32,
    pub(crate) map_location: usize,
}

impl ViewSet {
    pub(crate) fn from_def(def: Option<&ViewInfoDef>) -> Self {
        let none = ViewInfoDef::default();
        let def = def.unwrap_or(&none);
        Self {
            near: Preset::read(&def.near, Preset::NEAR),
            far: Preset::read(&def.far, Preset::FAR),
            close_up: Preset::read(&def.close_up, Preset::NEAR),
            portrait: Preset::read(&def.portrait, Preset::NEAR),
            normal_cam: def.normal_cam.unwrap_or(0.5),
            camera_angle: def.camera_angle.unwrap_or(0.0),
            map_location: def
                .map_location
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CameraMode {
    Free,
    Fixed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) rotate_left: bool,
    pub(crate) rotate_right: bool,
    pub(crate) zoom_in: bool,
    pub(crate) zoom_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditView {
    pub(crate) yaw: f32,
    pub(crate) zoom: f32,
    pub(crate) mode: CameraMode,
}

impl EditView {
    pub(crate) fn new(normal_cam: f32) -> Self {
        Self {
            yaw: 0.0,
            zoom: normal_cam,
            mode: CameraMode::Fixed,
        }
    }

    pub(crate) fn reset(&mut self, normal_cam: f32) {
        self.yaw = 0.0;
        self.zoom = normal_cam;
    }

    pub(crate) fn step(&mut self, held: Held, shift: bool) {
        let (rotate, zoom) = if shift { SHIFT_STEPS } else { STEPS };
        if held.rotate_left {
            self.yaw -= rotate;
        }
        if held.rotate_right {
            self.yaw += rotate;
        }
        if held.zoom_in {
            self.zoom = (self.zoom - zoom).max(0.0);
        }
        if held.zoom_out {
            self.zoom = (self.zoom + zoom).min(1.0);
        }
    }

    pub(crate) fn facing(&self, location_angle: f32) -> f32 {
        self.yaw + location_angle
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub(crate) look_at: [f32; 2],
    pub(crate) floor: f32,
    pub(crate) angle: f32,
    pub(crate) pitch: f32,
    pub(crate) zoom: f32,
}

pub(crate) fn placement(
    set: &ViewSet,
    location_angle: f32,
    object: [f32; 2],
    view: &EditView,
    on_powers: bool,
) -> Placement {
    let preset = match view.mode {
        CameraMode::Fixed if on_powers => set.close_up,
        CameraMode::Fixed => set.portrait,
        CameraMode::Free => {
            let t = if view.zoom < 0.0 {
                set.normal_cam
            } else {
                view.zoom
            };
            Preset::between(set.near, set.far, t)
        }
    };
    let angle = location_angle + set.camera_angle;
    let (sin, cos) = (mint_math::f32::sin(angle), mint_math::f32::cos(angle));
    Placement {
        look_at: [
            object[0] + cos * preset.dist - sin * preset.shift,
            object[1] + sin * preset.dist + cos * preset.shift,
        ],
        floor: preset.floor,
        angle,
        pitch: preset.pitch,
        zoom: preset.zoom,
    }
}

impl Placement {
    pub(crate) fn view(&self, offset: Vec3) -> BfmeView {
        BfmeView {
            look_at: self.look_at,
            ground: self.floor,
            zoom: self.zoom,
            yaw: self.angle + core::f32::consts::FRAC_PI_2,
            fx_pitch: self.pitch,
            roll: 0.0,
            hfov: HFOV,
            offset,
            height_adjust: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> ViewSet {
        let preset = |base: f32| Preset {
            pitch: base,
            zoom: base + 1.0,
            floor: base + 2.0,
            dist: base + 3.0,
            shift: base + 4.0,
        };
        ViewSet {
            near: preset(10.0),
            far: preset(20.0),
            close_up: preset(30.0),
            portrait: preset(40.0),
            normal_cam: 0.5,
            camera_angle: 0.0,
            map_location: 0,
        }
    }

    fn view(zoom: f32, mode: CameraMode) -> EditView {
        EditView {
            yaw: 0.0,
            zoom,
            mode,
        }
    }

    #[test]
    fn free_zoom_blends_near_to_far_and_fixed_modes_pick_a_preset_by_page() {
        let at =
            |zoom: f32, mode, powers| placement(&set(), 0.0, [0.0, 0.0], &view(zoom, mode), powers);
        assert_eq!(at(0.0, CameraMode::Free, false).pitch, 10.0);
        assert_eq!(at(1.0, CameraMode::Free, false).pitch, 20.0);
        assert_eq!(at(0.5, CameraMode::Free, false).floor, 17.0);
        assert_eq!(at(0.5, CameraMode::Fixed, true).pitch, 30.0);
        assert_eq!(at(0.5, CameraMode::Fixed, false).pitch, 40.0);
    }

    #[test]
    fn the_look_at_leads_the_object_by_dist_along_the_angle_and_slides_by_shift() {
        let mut quarter = set();
        quarter.camera_angle = core::f32::consts::FRAC_PI_2;
        let p = placement(
            &quarter,
            0.0,
            [100.0, 50.0],
            &view(0.0, CameraMode::Free),
            false,
        );
        let (dist, shift) = (13.0, 14.0);
        assert!((p.look_at[0] - (100.0 - shift)).abs() < 1e-3);
        assert!((p.look_at[1] - (50.0 + dist)).abs() < 1e-3);
        assert!((p.angle - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn the_object_turns_and_the_camera_angle_ignores_the_turn() {
        let mut v = view(0.5, CameraMode::Free);
        v.yaw = 0.7;
        assert_eq!(v.facing(1.0), 1.7);
        let p = placement(&set(), 1.0, [0.0, 0.0], &v, false);
        assert!((p.angle - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_preview_eye_sits_at_the_placement_angle_from_the_look_at() {
        let angle = 0.9;
        let p = placement(
            &set(),
            angle,
            [10.0, 20.0],
            &view(0.5, CameraMode::Free),
            false,
        );
        let eye = mint_sage_scene::camera_transform::bfme_camera(
            &p.view(Vec3::new(0.0, -80.0, 60.0)),
            1.0,
            1.0,
            100.0,
        )
        .eye;
        let bearing = mint_math::f32::atan2(eye.y - p.look_at[1], eye.x - p.look_at[0]);
        assert!((bearing - angle).abs() < 1e-3);
    }

    #[test]
    fn held_controls_step_per_update_and_zoom_clamps_to_the_unit_range() {
        let mut v = view(0.5, CameraMode::Free);
        let left = Held {
            rotate_left: true,
            ..Held::default()
        };
        for _ in 0..3 {
            v.step(left, false);
        }
        assert!((v.yaw + 0.3).abs() < 1e-6);
        v.step(left, true);
        assert!((v.yaw + 0.5).abs() < 1e-6);
        let out = Held {
            zoom_out: true,
            ..Held::default()
        };
        for _ in 0..30 {
            v.step(out, false);
        }
        assert_eq!(v.zoom, 1.0);
        let mut v = view(0.0, CameraMode::Free);
        v.step(
            Held {
                zoom_in: true,
                ..Held::default()
            },
            false,
        );
        assert_eq!(v.zoom, 0.0);
    }

    #[test]
    fn a_view_info_without_fields_takes_the_constructor_defaults() {
        let set = ViewSet::from_def(None);
        assert_eq!(set.normal_cam, 0.5);
        assert_eq!(set.camera_angle, 0.0);
        assert_eq!(set.map_location, 0);
        assert_eq!(set.far, Preset::FAR);
        assert_eq!(set.portrait, Preset::NEAR);
        let given = ViewInfoDef {
            far: ViewPreset {
                dist: Some(7.0),
                ..ViewPreset::default()
            },
            map_location: Some(3),
            ..ViewInfoDef::default()
        };
        let set = ViewSet::from_def(Some(&given));
        assert_eq!(
            (set.far.dist, set.far.pitch, set.map_location),
            (7.0, 0.4, 3)
        );
    }
}
