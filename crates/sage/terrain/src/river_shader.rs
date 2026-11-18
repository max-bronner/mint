pub const RIVER_SHADER_WGSL: &str = concat!(
    include_str!("shroud_scene.wgsl"),
    include_str!("river_shader.wgsl")
);

pub const RIVER_SCENE_TEXTURE_SLOTS: u32 = crate::shroud_scene::SHROUD_SCENE_TEXTURE_SLOTS;

pub const RIVER_SCENE_UNIFORM_SIZE: u32 = crate::shroud_scene::SHROUD_SCENE_UNIFORM_SIZE;

pub const RIVER_TEXTURE_SLOTS: u32 = 6;

pub const RIVER_UNIFORM_SIZE: u32 = 128;

pub const RIVER_REFLECTION_STRENGTH: f32 = 0.4;

pub const RIVER_NOISE_WORLD_SCALE: f32 = 0.0625;

pub const RIVER_SCROLL_UNIT: f32 = 16.0;

pub const RIVER_RIPPLE_REPEAT: f32 = 48.0;

pub const RIVER_RIPPLE_SLOPE: f32 = 0.5;

pub const RIVER_RIPPLE_DETAIL_RATE: f32 = 2.7;

pub const RIVER_RIPPLE_CROSS_RATE: f32 = 0.4;

pub const RIVER_TEXTURE_SCROLL: f32 = 1.0;

pub const RIVER_SPECULAR_GAIN: f32 = 0.7;
pub const RIVER_SPECULAR_EXPONENT: f32 = 24.0;

pub const RIVER_SPARKLE_GAIN: f32 = 2.5;

pub const RIVER_BANK_TURBULENCE: f32 = 1.2;

pub const RIVER_EDGE_BAND: f32 = 4.0;

pub const RIVER_SHALLOW_RIPPLE: f32 = 1.5;

pub const RIVER_REFRACTION_STRENGTH: f32 = 0.02 * crate::water_shader::WATER_REFRACTION_SCALE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiverFlow {
    #[default]
    Channel,
    Diagonal,
}

impl RiverFlow {
    fn index(self) -> f32 {
        match self {
            Self::Channel => 0.0,
            Self::Diagonal => 1.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Channel => Self::Diagonal,
            Self::Diagonal => Self::Channel,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Diagonal => "diagonal",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "channel" => Some(Self::Channel),
            "diagonal" => Some(Self::Diagonal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiverStage {
    Surface,
    Opacity,
    Sparkle,
    #[default]
    All,
    Normal,
}

impl RiverStage {
    fn index(self) -> f32 {
        match self {
            Self::Surface => 0.0,
            Self::Opacity => 1.0,
            Self::Sparkle => 2.0,
            Self::All => 3.0,
            Self::Normal => 5.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Surface => Self::Opacity,
            Self::Opacity => Self::Sparkle,
            Self::Sparkle => Self::All,
            Self::All => Self::Normal,
            Self::Normal => Self::Surface,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Opacity => "opacity",
            Self::Sparkle => "sparkle",
            Self::All => "all",
            Self::Normal => "normal",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "surface" => Some(Self::Surface),
            "opacity" => Some(Self::Opacity),
            "sparkle" => Some(Self::Sparkle),
            "all" => Some(Self::All),
            "normal" => Some(Self::Normal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RiverLook {
    pub stage: RiverStage,
    pub flow: RiverFlow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiverUniforms {
    stage: [f32; 4],
    noise: [f32; 4],
    phases: [f32; 4],
    flow: [f32; 4],
    glint: [f32; 4],
    edge: [f32; 4],
    refraction: [f32; 4],
    reflection: [f32; 4],
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

impl RiverUniforms {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        speed: f32,
        width: f32,
        opacity: f32,
        additive: bool,
        look: RiverLook,
        ripples: bool,
        refraction: crate::water_shader::WaterRefraction,
        reflects: bool,
        tuning: &crate::water_tuning::WaterTuning,
        seconds: f32,
    ) -> Self {
        let speed = finite(speed, 0.0);
        let seconds = finite(seconds, 0.0);
        let scroll = finite(tuning.river_uv_scroll, RIVER_SCROLL_UNIT) / RIVER_SCROLL_UNIT;
        let travelled = speed * scroll * seconds;
        let detail = finite(tuning.river_ripple_detail_rate, RIVER_RIPPLE_DETAIL_RATE);
        let wrap = crate::water::wrap_unit;
        let texture_v =
            wrap(-travelled * finite(tuning.river_texture_scroll, RIVER_TEXTURE_SCROLL));
        Self {
            stage: [
                look.stage.index(),
                finite(opacity, 1.0).clamp(0.0, 1.0),
                if additive { 1.0 } else { 0.0 },
                seconds,
            ],
            noise: [
                wrap(travelled),
                wrap(travelled),
                RIVER_NOISE_WORLD_SCALE,
                0.0,
            ],
            phases: [
                wrap(travelled),
                wrap(travelled * detail),
                wrap(travelled * RIVER_RIPPLE_CROSS_RATE),
                texture_v,
            ],
            flow: [
                look.flow.index(),
                1.0 / finite(tuning.river_ripple_repeat, RIVER_RIPPLE_REPEAT).max(1e-3),
                if ripples {
                    finite(tuning.river_ripple_slope, RIVER_RIPPLE_SLOPE)
                } else {
                    0.0
                },
                finite(width, crate::rivers::FALLBACK_RIVER_WIDTH).max(1e-3),
            ],
            glint: [
                if ripples {
                    finite(tuning.river_specular_gain, RIVER_SPECULAR_GAIN)
                } else {
                    0.0
                },
                finite(tuning.river_specular_exponent, RIVER_SPECULAR_EXPONENT),
                if ripples {
                    finite(tuning.river_sparkle_gain, RIVER_SPARKLE_GAIN)
                } else {
                    1.0
                },
                if ripples {
                    finite(tuning.river_bank_turbulence, RIVER_BANK_TURBULENCE)
                } else {
                    0.0
                },
            ],
            edge: [
                finite(tuning.river_edge_band, RIVER_EDGE_BAND),
                if ripples {
                    finite(tuning.river_shallow_ripple, RIVER_SHALLOW_RIPPLE)
                } else {
                    0.0
                },
                0.0,
                0.0,
            ],
            refraction: [
                refraction.index(),
                0.02 * finite(
                    tuning.water_refraction_scale,
                    crate::water_shader::WATER_REFRACTION_SCALE,
                ),
                finite(
                    tuning.water_refraction_falloff,
                    crate::water_shader::WATER_REFRACTION_FALLOFF,
                ),
                0.0,
            ],
            reflection: [
                if reflects && ripples { 1.0 } else { 0.0 },
                finite(tuning.river_reflection_strength, RIVER_REFLECTION_STRENGTH),
                finite(
                    tuning.water_planar_perturbation,
                    crate::water_shader::WATER_PLANAR_PERTURBATION,
                ),
                finite(
                    tuning.water_fresnel_f0,
                    crate::water_shader::WATER_FRESNEL_F0,
                ),
            ],
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RIVER_UNIFORM_SIZE as usize);
        for row in [
            self.stage,
            self.noise,
            self.phases,
            self.flow,
            self.glint,
            self.edge,
            self.refraction,
            self.reflection,
        ] {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::RIVER_SHADER_WGSL;

    #[test]
    fn the_river_asks_for_the_shadow_and_leaves_lava_alone() {
        assert!(
            RIVER_SHADER_WGSL.contains("rgb = rgb * mint_shadow_factor(in.world_pos);"),
            "river_shader.wgsl must sample the shadow map"
        );
        let at = RIVER_SHADER_WGSL
            .find("rgb = rgb * mint_shadow_factor")
            .expect("the call is there");
        let before = RIVER_SHADER_WGSL.get(..at).unwrap_or_default();
        assert!(
            before.ends_with("if (river.flow.z > 0.0) {\n        "),
            "and must be gated on the switch that is zero for lava"
        );
        assert!(
            before.contains("alpha = clamp(alpha * river.stage.y, 0.0, 1.0);"),
            "after the four-stage composite"
        );
        assert!(
            !before.contains("// ---- W12: the lake's planar reflection"),
            "and before the reflection, which is light arriving rather than \
             light the surface carries"
        );
    }
}
