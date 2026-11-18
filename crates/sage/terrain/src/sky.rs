use mint_render::{MeshData, Vertex};

use crate::water_tuning::WaterTuning;

pub const SKY_SHADER_WGSL: &str = include_str!("sky.wgsl");

pub const SKY_TEXTURE_SLOTS: u32 = 1;

pub const SKY_UNIFORM_SIZE: u32 = 80;

pub const SKY_CLOUD_REPEAT: f32 = 320.0;

pub const SKY_CLOUD_TRAVEL: f32 = 2.0;

pub const SKY_CLOUD_HEIGHT: f32 = 600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkyDraw {
    #[default]
    Off,
    Gradient,
    Full,
    Plane,
}

pub const SKY_PLANE_TINT: f32 = 200.0 / 255.0;

impl SkyDraw {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Gradient => "gradient",
            Self::Full => "full",
            Self::Plane => "plane",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "gradient" => Some(Self::Gradient),
            "full" => Some(Self::Full),
            "plane" => Some(Self::Plane),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[must_use]
pub fn sky_mesh() -> MeshData {
    let mut mesh = MeshData::new();
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, -1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    for (normal, u_axis, v_axis) in faces {
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(0);
        for (su, sv) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let position = [
                normal[0] + su * u_axis[0] + sv * v_axis[0],
                normal[1] + su * u_axis[1] + sv * v_axis[1],
                normal[2] + su * u_axis[2] + sv * v_axis[2],
            ];
            let inward = [-normal[0], -normal[1], -normal[2]];
            mesh.vertices
                .push(Vertex::new(position, inward, [0.0, 0.0]));
        }
        let corner = |n: u32| base.saturating_add(n);
        mesh.indices
            .extend_from_slice(&[base, corner(1), corner(2), base, corner(2), corner(3)]);
    }
    mesh
}

#[derive(Debug, Clone, Copy)]
pub struct SkyLook {
    pub draw: SkyDraw,
    pub cloud_loaded: bool,
    pub seconds: f32,
    pub plane: SkyPlaneTerms,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyPlaneTerms {
    pub tint: f32,
    pub travel: f32,
    pub repeat: f32,
}

impl Default for SkyPlaneTerms {
    fn default() -> Self {
        Self {
            tint: SKY_PLANE_TINT,
            travel: SKY_CLOUD_TRAVEL,
            repeat: SKY_CLOUD_REPEAT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SkyUniforms {
    horizon_terms: [f32; 4],
    horizon: [f32; 4],
    zenith: [f32; 4],
    cloud: [f32; 4],
    cloud_shape: [f32; 4],
}

impl SkyUniforms {
    #[must_use]
    pub fn new(look: &SkyLook, tuning: &WaterTuning) -> Self {
        let plane = look.draw == SkyDraw::Plane;
        let clouds = if look.cloud_loaded && matches!(look.draw, SkyDraw::Full | SkyDraw::Plane) {
            tuning.sky_cloud_strength
        } else {
            0.0
        };
        let travel = look.plane.travel * tuning.sky_cloud_scroll * look.seconds;
        let set_repeat = if look.plane.repeat.is_finite() && look.plane.repeat > 0.0 {
            look.plane.repeat
        } else {
            SKY_CLOUD_REPEAT
        };
        let repeat = if tuning.sky_cloud_scale.is_finite() && tuning.sky_cloud_scale > 0.0 {
            tuning.sky_cloud_scale * set_repeat / SKY_CLOUD_REPEAT
        } else {
            set_repeat
        };
        Self {
            horizon_terms: [
                if plane { 1.0 } else { 0.0 },
                look.plane.tint,
                0.0,
                tuning.sky_gradient_power.max(0.01),
            ],
            horizon: [
                tuning.sky_horizon_r,
                tuning.sky_horizon_g,
                tuning.sky_horizon_b,
                0.0,
            ],
            zenith: [
                tuning.sky_zenith_r,
                tuning.sky_zenith_g,
                tuning.sky_zenith_b,
                0.0,
            ],
            cloud: [
                1.0 / repeat,
                (travel / repeat).rem_euclid(1.0),
                (travel / repeat).rem_euclid(1.0),
                clouds,
            ],
            cloud_shape: [
                SKY_CLOUD_HEIGHT,
                tuning.sky_cloud_horizon.max(1.0e-3),
                0.0,
                0.0,
            ],
        }
    }

    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(SKY_UNIFORM_SIZE as usize);
        for row in [
            self.horizon_terms,
            self.horizon,
            self.zenith,
            self.cloud,
            self.cloud_shape,
        ] {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn block(draw: SkyDraw, cloud_loaded: bool) -> Vec<f32> {
        let look = SkyLook {
            draw,
            cloud_loaded,
            seconds: 0.0,
            plane: SkyPlaneTerms::default(),
        };
        SkyUniforms::new(&look, &WaterTuning::default())
            .to_bytes()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    }

    #[test]
    fn only_the_plane_rung_raises_the_plane_flag_and_tint() {
        let plane = block(SkyDraw::Plane, true);
        assert!((plane[0] - 1.0).abs() < 1e-6);
        assert!((plane[1] - SKY_PLANE_TINT).abs() < 1e-6);
        assert!(plane[15] > 0.0);
        for draw in [SkyDraw::Off, SkyDraw::Gradient, SkyDraw::Full] {
            assert!(block(draw, true)[0].abs() < 1e-6, "{}", draw.label());
        }
    }

    #[test]
    fn every_rung_label_parses_back() {
        for draw in [
            SkyDraw::Off,
            SkyDraw::Gradient,
            SkyDraw::Full,
            SkyDraw::Plane,
        ] {
            assert_eq!(SkyDraw::parse(draw.label()), Some(draw));
        }
    }
}
