use crate::water_tuning::WaterTuning;

pub const TERRAIN_SHADER_WGSL: &str = include_str!("terrain_shader.wgsl");

pub const TERRAIN_TEXTURE_SLOTS: u32 = 2;

pub const TERRAIN_SCENE_TEXTURE_SLOTS: u32 = 3;

pub const TERRAIN_SCENE_UNIFORM_SIZE: u32 = 144;

pub const TERRAIN_FLAT_NORMAL_TEXEL: [u8; 4] = [128, 128, 0, 255];

pub const TERRAIN_CLOUD_REPEAT: f32 = 660.0;

pub const TERRAIN_CLOUD_SCROLL: [f32; 2] = [-0.012, -0.018];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainCloud {
    Off,
    #[default]
    Texture,
    Factor,
}

impl TerrainCloud {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Texture => "texture",
            Self::Factor => "factor",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "texture" => Some(Self::Texture),
            "factor" => Some(Self::Factor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainShadows {
    Off,
    #[default]
    On,
}

impl TerrainShadows {
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
pub enum TerrainMacro {
    Off,
    #[default]
    On,
}

impl TerrainMacro {
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
pub enum TerrainShroud {
    Revealed,
    #[default]
    On,
}

impl TerrainShroud {
    pub const ALL: [Self; 2] = [Self::Revealed, Self::On];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Revealed => "off (playable area revealed; outside the boundary stays black)",
            Self::On => "on (ShroudAlpha 0 / FogAlpha 127 / ClearAlpha 255)",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Revealed => Self::On,
            Self::On => Self::Revealed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainRelief {
    Off,
    #[default]
    On,
}

impl TerrainRelief {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off (interpolated normal, no specular)",
            Self::On => "on (class _nrm normal maps, specular 0.9 x N.H^1200)",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerrainSun {
    Authored,
    #[default]
    Overbright,
}

impl TerrainSun {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Authored => "authored (1x; the overbright on ambient and accents only)",
            Self::Overbright => "overbright (x the map's overbright, like the rest)",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "authored" => Some(Self::Authored),
            "overbright" => Some(Self::Overbright),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainModulation {
    pub cloud: TerrainCloud,
    pub cloud_loaded: bool,
    pub no_cloud_factor: [f32; 3],
    pub macro_layer: TerrainMacro,
    pub macro_loaded: bool,
    pub macro_stretched: bool,
    pub map_extent: [f32; 4],
    pub macro_repeat: f32,
    pub seconds: f32,
    pub shroud: TerrainShroud,
    pub shroud_loaded: bool,
    pub shroud_extent: [f32; 4],
    pub relief: TerrainRelief,
    pub sun: TerrainSun,
    pub overbright: f32,
}

impl Default for TerrainModulation {
    fn default() -> Self {
        Self {
            cloud: TerrainCloud::default(),
            cloud_loaded: false,
            no_cloud_factor: [1.0; 3],
            macro_layer: TerrainMacro::default(),
            macro_loaded: false,
            macro_stretched: false,
            map_extent: [0.0, 0.0, 1.0, 1.0],
            macro_repeat: TERRAIN_MACRO_REPEAT,
            seconds: 0.0,
            shroud: TerrainShroud::default(),
            shroud_loaded: false,
            shroud_extent: [0.0, 0.0, 1.0, 1.0],
            relief: TerrainRelief::default(),
            sun: TerrainSun::default(),
            overbright: 1.0,
        }
    }
}

pub const TERRAIN_MACRO_REPEAT: f32 = 320.0;

pub const TERRAIN_SHADOW_LIGHTNESS: f32 = 0.0;

fn shroud_uv(extent: [f32; 4]) -> [f32; 4] {
    let [min_x, min_y, size_x, size_y] = extent;
    let size_x = finite(size_x, 1.0);
    let size_y = finite(size_y, 1.0);
    if size_x <= 0.0 || size_y <= 0.0 {
        return [1.0, 1.0, 0.0, 0.0];
    }
    let min_x = finite(min_x, 0.0);
    let min_y = finite(min_y, 0.0);
    [1.0 / size_x, 1.0 / size_y, -min_x / size_x, -min_y / size_y]
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainUniforms {
    cloud: [f32; 4],
    cloud_factor: [f32; 4],
    macro_uv: [f32; 4],
    macro_terms: [f32; 4],
    shadow: [f32; 4],
    shroud_uv: [f32; 4],
    shroud_terms: [f32; 4],
    relief: [f32; 4],
    sun: [f32; 4],
}

impl TerrainUniforms {
    #[must_use]
    pub fn cloud_layer(&self) -> ([f32; 4], [f32; 3]) {
        let [r, g, b, _] = self.cloud_factor;
        (self.cloud, [r, g, b])
    }

    #[must_use]
    pub fn new(modulation: &TerrainModulation, tuning: &WaterTuning) -> Self {
        let seconds = finite(modulation.seconds, 0.0);
        let repeat = finite(tuning.terrain_cloud_scale, TERRAIN_CLOUD_REPEAT);
        let repeat = if repeat > 0.0 {
            repeat
        } else {
            TERRAIN_CLOUD_REPEAT
        };
        let rate = finite(tuning.terrain_cloud_scroll, 1.0);
        let phase = |speed: f32| (speed * rate * seconds).rem_euclid(1.0);
        let strength = match modulation.cloud {
            TerrainCloud::Texture if modulation.cloud_loaded => {
                finite(tuning.terrain_cloud_strength, 1.0)
            }
            _ => 0.0,
        };
        let factor = match modulation.cloud {
            TerrainCloud::Factor => modulation
                .no_cloud_factor
                .map(|c| finite(c, 1.0).clamp(0.0, 1.0)),
            _ => [1.0; 3],
        };
        let macro_strength = match modulation.macro_layer {
            TerrainMacro::On if modulation.macro_loaded => {
                finite(tuning.terrain_macro_strength, 1.0)
            }
            _ => 0.0,
        };
        let [min_x, min_y, size_x, size_y] = modulation.map_extent;
        let macro_uv = if modulation.macro_stretched {
            [1.0 / size_x, 1.0 / size_y, -min_x / size_x, -min_y / size_y]
        } else {
            let tile = finite(modulation.macro_repeat, TERRAIN_MACRO_REPEAT);
            let tile = if tile > 0.0 {
                tile
            } else {
                TERRAIN_MACRO_REPEAT
            };
            [1.0 / tile, 1.0 / tile, 0.0, 0.0]
        };
        Self {
            cloud: [
                1.0 / repeat,
                phase(TERRAIN_CLOUD_SCROLL[0]),
                phase(TERRAIN_CLOUD_SCROLL[1]),
                strength,
            ],
            cloud_factor: [factor[0], factor[1], factor[2], 0.0],
            macro_uv,
            macro_terms: [macro_strength, 0.0, 0.0, 0.0],
            shadow: [
                finite(tuning.terrain_shadow_lightness, TERRAIN_SHADOW_LIGHTNESS).clamp(0.0, 1.0),
                0.0,
                0.0,
                0.0,
            ],
            shroud_uv: shroud_uv(modulation.shroud_extent),
            shroud_terms: [
                if modulation.shroud_loaded { 1.0 } else { 0.0 },
                0.0,
                0.0,
                0.0,
            ],
            relief: [
                match modulation.relief {
                    TerrainRelief::On => 1.0,
                    TerrainRelief::Off => 0.0,
                },
                0.0,
                0.0,
                0.0,
            ],
            sun: [
                match modulation.sun {
                    TerrainSun::Overbright => finite(modulation.overbright, 1.0).max(0.0),
                    TerrainSun::Authored => 1.0,
                },
                0.0,
                0.0,
                0.0,
            ],
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(TERRAIN_SCENE_UNIFORM_SIZE as usize);
        for (_, row) in self.rows() {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }

    fn rows(&self) -> [(&'static str, [f32; 4]); 9] {
        [
            ("cloud", self.cloud),
            ("cloud_factor", self.cloud_factor),
            ("macro_uv", self.macro_uv),
            ("macro_terms", self.macro_terms),
            ("shadow", self.shadow),
            ("shroud_uv", self.shroud_uv),
            ("shroud_terms", self.shroud_terms),
            ("relief", self.relief),
            ("sun", self.sun),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::{
        TerrainModulation, TerrainSun, TerrainUniforms, TERRAIN_SCENE_UNIFORM_SIZE,
        TERRAIN_SHADER_WGSL, TERRAIN_SHADOW_LIGHTNESS,
    };
    use crate::water_tuning::WaterTuning;

    fn declared_rows(source: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut inside = false;
        for line in source.lines() {
            if line.starts_with("struct TerrainBlock {") {
                inside = true;
                continue;
            }
            if !inside {
                continue;
            }
            if line.starts_with('}') {
                break;
            }
            if let Some(name) = line.trim().strip_suffix(": vec4<f32>,") {
                out.push(name);
            }
        }
        out
    }

    fn block(tuning: &WaterTuning) -> TerrainUniforms {
        TerrainUniforms::new(&TerrainModulation::default(), tuning)
    }

    #[test]
    fn every_uniform_row_is_written_in_the_order_the_shader_declares_it() {
        let declared = declared_rows(TERRAIN_SHADER_WGSL);
        let written: Vec<&str> = block(&WaterTuning::default())
            .rows()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            declared, written,
            "terrain_shader.wgsl's `struct TerrainBlock` and              `TerrainUniforms::rows` disagree"
        );
        assert_eq!(
            block(&WaterTuning::default()).to_bytes().len(),
            TERRAIN_SCENE_UNIFORM_SIZE as usize,
            "the block must be exactly what the pipeline was built for"
        );
    }

    #[test]
    fn the_ground_takes_its_own_shadow_floor_and_not_the_scenes() {
        let tuning = WaterTuning::default();
        assert!(
            (tuning.terrain_shadow_lightness - TERRAIN_SHADOW_LIGHTNESS).abs() < f32::EPSILON,
            "the tuning default and the constant it derives from must agree"
        );
        assert!(
            (tuning.shadow_lightness - tuning.terrain_shadow_lightness).abs() > 0.1,
            "the scene's strength must differ from the ground's, or this test              cannot tell them apart"
        );
        assert!(
            (block(&tuning).shadow[0] - TERRAIN_SHADOW_LIGHTNESS).abs() < f32::EPSILON,
            "the block must carry the ground's floor"
        );
    }

    #[test]
    fn a_hand_typed_shadow_floor_is_clamped_and_a_nan_falls_back() {
        let typed = |value: f32| {
            block(&WaterTuning {
                terrain_shadow_lightness: value,
                ..WaterTuning::default()
            })
            .shadow[0]
        };
        assert!((typed(-3.0) - 0.0).abs() < f32::EPSILON);
        assert!((typed(7.5) - 1.0).abs() < f32::EPSILON);
        assert!(
            (typed(f32::NAN) - TERRAIN_SHADOW_LIGHTNESS).abs() < f32::EPSILON,
            "a NaN must fall back to the derived default, not reach the GPU"
        );
    }

    #[test]
    fn the_ground_asks_for_the_unweighted_shadow_and_mixes_it_itself() {
        assert!(
            TERRAIN_SHADER_WGSL.contains("mint_shadow_visibility(in.world_position)"),
            "the ground must take the unweighted four-tap lookup"
        );
        assert!(
            TERRAIN_SHADER_WGSL.contains("clamp(terrain.shadow.x, 0.0, 1.0)"),
            "and mix it to its own floor, clamped on the GPU as well as the CPU"
        );
        assert!(
            !TERRAIN_SHADER_WGSL.contains("mint_shadow_factor(in.world_position)"),
            "and must not take the scene-weighted one, which is what it did              until 2026-09-08"
        );
    }

    #[test]
    fn the_terrain_shadows_the_sun_alone_and_not_the_two_accents() {
        assert!(
            TERRAIN_SHADER_WGSL.contains(
                "if (i == 0) {\n                term = term * terrain.sun.x * sun_shade;"
            ),
            "the shadow must multiply light 0's term inside the loop"
        );
        assert!(
            !TERRAIN_SHADER_WGSL.contains("lightsum = lightsum * mint_shadow_factor"),
            "and must not multiply the summed three afterwards, which is what \
             it did until 2026-09-08"
        );
    }

    #[test]
    fn the_ground_sun_takes_the_maps_overbright_on_its_default_rung() {
        let tuning = WaterTuning::default();
        let mut modulation = TerrainModulation {
            overbright: 2.0,
            ..TerrainModulation::default()
        };
        assert_eq!(modulation.sun, TerrainSun::Overbright);
        assert_eq!(TerrainUniforms::new(&modulation, &tuning).sun[0], 2.0);
        modulation.sun = TerrainSun::Authored;
        assert_eq!(TerrainUniforms::new(&modulation, &tuning).sun[0], 1.0);
        modulation.sun = TerrainSun::Overbright;
        modulation.overbright = f32::NAN;
        assert_eq!(TerrainUniforms::new(&modulation, &tuning).sun[0], 1.0);
    }
}
