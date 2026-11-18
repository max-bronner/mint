use mint_math::f32::{cos, sin, tan};
use mint_render::Vec3;
use mint_sage_map::{Dictionary, Value};

use crate::game_data::CameraDefaults;

const OFFSET_PER_MAX_HEIGHT: f32 = 1.642_665;
const PLAIN_PITCH_DEGREES: f32 = 37.5;
const PLAIN_MIN_HEIGHT: f32 = 120.0;
const PLAIN_MAX_HEIGHT: f32 = 300.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MapCameraKeys {
    min_height: Option<f32>,
    max_height: Option<f32>,
    pitch_degrees: Option<f32>,
    yaw_degrees: Option<f32>,
}

impl MapCameraKeys {
    pub(crate) fn read(world_info: &Dictionary) -> Self {
        let key = |name: &str| match world_info.get(name) {
            Some(Value::Float(v)) if v.is_finite() => Some(*v),
            _ => None,
        };
        Self {
            min_height: key("cameraMinHeight"),
            max_height: key("cameraMaxHeight"),
            pitch_degrees: key("cameraPitchAngle"),
            yaw_degrees: key("cameraYawAngle"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraLimits {
    max_height: f32,
    pitch_degrees: f32,
    yaw_degrees: f32,
    custom: bool,
}

impl CameraLimits {
    pub(crate) fn resolve(keys: MapCameraKeys, defaults: CameraDefaults) -> Self {
        let max_height = keys.max_height.unwrap_or(defaults.max_height);
        let min_height = keys.min_height.unwrap_or(defaults.min_height);
        let pitch_degrees = keys.pitch_degrees.unwrap_or(defaults.pitch_degrees);
        let yaw_degrees = keys.yaw_degrees.unwrap_or(0.0);
        let custom = pitch_degrees != PLAIN_PITCH_DEGREES
            || yaw_degrees != 0.0
            || min_height != PLAIN_MIN_HEIGHT
            || max_height != PLAIN_MAX_HEIGHT;
        Self {
            max_height,
            pitch_degrees,
            yaw_degrees,
            custom,
        }
    }

    pub(crate) fn reset_offset(&self) -> (Vec3, f32) {
        let pitch = self.pitch_degrees.to_radians();
        let height = self.max_height;
        if self.custom {
            let length = height * OFFSET_PER_MAX_HEIGHT;
            return (
                Vec3::new(0.0, -cos(pitch) * length, sin(pitch) * length),
                self.yaw_degrees.to_radians(),
            );
        }
        let back = -height / tan(pitch);
        let yaw = self.yaw_degrees.to_radians();
        (Vec3::new(-tan(yaw) * back, back, height), 0.0)
    }

    pub(crate) const fn max_height(&self) -> f32 {
        self.max_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> CameraDefaults {
        CameraDefaults::SHIPPED
    }

    #[test]
    fn a_custom_pitch_turns_the_offset_at_the_default_distance() {
        let plain = CameraLimits::resolve(MapCameraKeys::default(), defaults());
        let steep = CameraLimits::resolve(
            MapCameraKeys {
                pitch_degrees: Some(60.0),
                ..MapCameraKeys::default()
            },
            defaults(),
        );
        let (a, _) = plain.reset_offset();
        let (b, _) = steep.reset_offset();
        let length = |v: Vec3| (v.y * v.y + v.z * v.z).sqrt();
        assert!((length(a) - length(b)).abs() < 0.05, "{a:?} {b:?}");
        assert!(b.z > a.z && (a.z - 300.0).abs() < 0.1);
    }

    #[test]
    fn a_map_yaw_is_the_reset_angle_and_a_plain_map_resets_to_zero() {
        let yawed = CameraLimits::resolve(
            MapCameraKeys {
                yaw_degrees: Some(90.0),
                ..MapCameraKeys::default()
            },
            defaults(),
        );
        assert!((yawed.reset_offset().1 - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let plain = CameraLimits::resolve(MapCameraKeys::default(), defaults());
        assert_eq!(plain.reset_offset().1, 0.0);
    }
}
