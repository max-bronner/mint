use crate::waves::{WaveParams, WaveScale, WAVE_VELOCITY_UNIT};

pub const WAVE_SHADER_WGSL: &str = concat!(
    include_str!("shroud_scene.wgsl"),
    include_str!("wave_shader.wgsl")
);

pub const WAVE_SCENE_TEXTURE_SLOTS: u32 = crate::shroud_scene::SHROUD_SCENE_TEXTURE_SLOTS;

pub const WAVE_SCENE_UNIFORM_SIZE: u32 = crate::shroud_scene::SHROUD_SCENE_UNIFORM_SIZE;

pub const WAVE_TEXTURE_SLOTS: u32 = 1;

pub const WAVE_UNIFORM_SIZE: u32 = 80;

pub const WAVE_FADE_IN: f32 = 0.15;
pub const WAVE_FADE_OUT: f32 = 0.45;

pub const WAVE_COMPRESSED_DEPTH: f32 = 0.35;

pub const WAVE_BEACH_BAND: f32 = 6.0;

pub const WAVE_ALPHA_GAIN: f32 = 1.6;

pub const WAVE_CREST_HEIGHT: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WavePca {
    Ignore,
    Crest,
    #[default]
    Displace,
}

impl WavePca {
    fn index(self) -> f32 {
        match self {
            Self::Ignore => 1.0,
            Self::Crest => 2.0,
            Self::Displace => 3.0,
        }
    }

    #[must_use]
    pub fn draws_sprite(self, pca: bool) -> bool {
        !(pca && matches!(self, Self::Displace))
    }

    #[must_use]
    pub fn displaces(self) -> bool {
        matches!(self, Self::Displace)
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Ignore => Self::Crest,
            Self::Crest => Self::Displace,
            Self::Displace => Self::Ignore,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ignore => "ignore",
            Self::Crest => "crest",
            Self::Displace => "displace",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "ignore" => Some(Self::Ignore),
            "crest" => Some(Self::Crest),
            "displace" => Some(Self::Displace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveDraw {
    Off,
    #[default]
    On,
}

impl WaveDraw {
    #[must_use]
    pub fn on(self) -> bool {
        matches!(self, Self::On)
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WaveLook {
    pub draw: WaveDraw,
    pub pca: WavePca,
    pub scale: WaveScale,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveUniforms {
    size: [f32; 4],
    life: [f32; 4],
    timing: [f32; 4],
    mode: [f32; 4],
    beach: [f32; 4],
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

impl WaveUniforms {
    #[must_use]
    pub fn new(
        params: &WaveParams,
        look: WaveLook,
        tuning: &crate::water_tuning::WaterTuning,
        seconds: f32,
    ) -> Self {
        let units = finite(look.scale.world_units(), 1.0) * finite(tuning.wave_unit, 1.0);
        let seconds = finite(seconds, 0.0);
        let time = finite(tuning.wave_time, 1.0).max(1e-3);
        Self {
            size: [
                params.final_width * units * finite(tuning.wave_final_width, 1.0),
                params.final_height * units * finite(tuning.wave_final_height, 1.0),
                params.distance_from_shore * units * finite(tuning.wave_distance_from_shore, 1.0),
                params.initial_velocity * units * finite(tuning.wave_velocity, WAVE_VELOCITY_UNIT)
                    / time,
            ],
            life: [
                params.initial_width_fraction,
                params.initial_height_fraction,
                params.time_to_compress * time,
                params.time_to_fade * time,
            ],
            timing: [
                params.time_offset_second_wave * time,
                seconds,
                finite(tuning.wave_compressed_depth, WAVE_COMPRESSED_DEPTH),
                finite(tuning.wave_crest_height, WAVE_CREST_HEIGHT),
            ],
            mode: [
                look.pca.index(),
                if params.pca { 1.0 } else { 0.0 },
                finite(tuning.wave_fade_in, WAVE_FADE_IN),
                finite(tuning.wave_fade_out, WAVE_FADE_OUT),
            ],
            beach: [
                finite(tuning.wave_beach_band, WAVE_BEACH_BAND),
                finite(tuning.wave_alpha_gain, WAVE_ALPHA_GAIN),
                0.0,
                0.0,
            ],
        }
    }

    #[must_use]
    pub fn hidden(mut self) -> Self {
        self.beach[1] = 0.0;
        self
    }

    #[must_use]
    pub fn lifted(mut self, z: f32) -> Self {
        self.beach[2] = finite(z, 0.0);
        self
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WAVE_UNIFORM_SIZE as usize);
        for row in [self.size, self.life, self.timing, self.mode, self.beach] {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::WAVE_SHADER_WGSL;

    #[test]
    fn the_wave_sprite_asks_for_the_shadow() {
        assert!(
            WAVE_SHADER_WGSL.contains("mint_shadow_factor(in.world_pos)"),
            "wave_shader.wgsl must sample the shadow map"
        );
        assert!(
            WAVE_SHADER_WGSL.contains("out.world_pos = world.xyz;"),
            "and the fragment stage can only do that if the vertex stage \
             passes the world position on - the quad is assembled there"
        );
    }
}
