use mint_image::RgbaImage;
use mint_render::Vertex;
use mint_vfs::{Vfs, VfsPath};

use crate::water::{WaterBody, WATER_TINT};

const TERRAIN_TEXTURE_DIR: &str = "art/terrain/";

#[derive(Debug, thiserror::Error)]
pub enum RampError {
    #[error("{path}: {source}")]
    Read {
        path: VfsPath,
        source: mint_vfs::Error,
    },
    #[error("{path}: {source}")]
    Decode {
        path: VfsPath,
        source: mint_image::Error,
    },
    #[error("{path}: {width}x{height} is not a 1-D ramp (research 6e: the four real ones are 100x1 and 128x1)")]
    NotARamp {
        path: VfsPath,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct WaterTransparency {
    pub depth: f32,
    pub min_opacity: f32,
    pub map_depth: Option<f32>,
    pub map_alpha: Option<f32>,
    pub source: WaterAlphaSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterAlphaSource {
    Ini,
    #[default]
    Map,
}

impl WaterAlphaSource {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Ini => "ini",
            Self::Map => "map",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "ini" => Some(Self::Ini),
            "map" => Some(Self::Map),
            _ => None,
        }
    }
}

impl Default for WaterTransparency {
    fn default() -> Self {
        Self {
            depth: 3.0,
            min_opacity: 1.0,
            map_depth: None,
            map_alpha: None,
            source: WaterAlphaSource::default(),
        }
    }
}

impl WaterTransparency {
    #[must_use]
    pub fn transparent_depth(self) -> f32 {
        let product = match (self.source, self.map_depth) {
            (WaterAlphaSource::Map, Some(depth)) => depth,
            _ => self.depth * self.min_opacity,
        };
        if product.is_finite() && product > 0.0 {
            product
        } else {
            0.0
        }
    }

    #[must_use]
    pub fn max_alpha(self, material_alpha: f32) -> f32 {
        let material = if material_alpha.is_finite() {
            material_alpha
        } else {
            1.0
        };
        match (self.source, self.map_alpha) {
            (WaterAlphaSource::Map, Some(alpha)) if alpha.is_finite() => material.min(alpha),
            _ => material,
        }
    }
}

#[must_use]
pub fn parse_water_transparency(text: &str) -> WaterTransparency {
    let mut out = WaterTransparency::default();
    let mut inside = false;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.eq_ignore_ascii_case("WaterTransparency") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.eq_ignore_ascii_case("End") {
            break;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Ok(value) = value.trim().parse::<f32>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        match key.trim() {
            k if k.eq_ignore_ascii_case("TransparentWaterDepth") => out.depth = value,
            k if k.eq_ignore_ascii_case("TransparentWaterMinOpacity") => out.min_opacity = value,
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterTint {
    Flat,
    #[default]
    Depth,
    Ramp,
    RampTinted,
}

impl WaterTint {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Depth => "depth",
            Self::Ramp => "ramp",
            Self::RampTinted => "ramp-tinted",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "flat" => Some(Self::Flat),
            "depth" => Some(Self::Depth),
            "ramp" => Some(Self::Ramp),
            "ramp-tinted" | "ramptinted" => Some(Self::RampTinted),
            _ => None,
        }
    }

    #[must_use]
    pub fn uses_depth(self) -> bool {
        self != Self::Flat
    }
}

#[derive(Debug, Clone)]
pub struct BodyShading {
    pub color: [f32; 4],
    pub from_material: bool,
    pub ramp: Option<Vec<[f32; 3]>>,
    pub ramp_name: String,
}

impl Default for BodyShading {
    fn default() -> Self {
        Self {
            color: WATER_TINT,
            from_material: false,
            ramp: None,
            ramp_name: String::new(),
        }
    }
}

pub fn load_ramp(vfs: &Vfs, name: &str) -> Result<Vec<[f32; 3]>, RampError> {
    let path = VfsPath::new(&format!(
        "{TERRAIN_TEXTURE_DIR}{}",
        name.to_ascii_lowercase()
    ));
    let bytes = vfs.read_path(&path).map_err(|source| RampError::Read {
        path: path.clone(),
        source,
    })?;
    let image = mint_image::tga::decode(&bytes).map_err(|source| RampError::Decode {
        path: path.clone(),
        source,
    })?;
    if image.height() != 1 || image.width() < 2 {
        return Err(RampError::NotARamp {
            path,
            width: image.width(),
            height: image.height(),
        });
    }
    Ok(texels(&image))
}

fn texels(image: &RgbaImage) -> Vec<[f32; 3]> {
    (0..image.width())
        .map(|x| {
            let p = image.pixel(x, 0).unwrap_or([255, 255, 255, 255]);
            [
                f32::from(p[0]) / 255.0,
                f32::from(p[1]) / 255.0,
                f32::from(p[2]) / 255.0,
            ]
        })
        .collect()
}

fn sample_ramp(ramp: &[[f32; 3]], t: f32) -> [f32; 3] {
    let last = ramp.len().saturating_sub(1);
    if last == 0 {
        return ramp.first().copied().unwrap_or([1.0, 1.0, 1.0]);
    }
    #[allow(clippy::cast_precision_loss)]
    let scaled = t.clamp(0.0, 1.0) * last as f32;
    let floor = scaled.floor();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = (floor as usize).min(last);
    let next = index.saturating_add(1).min(last);
    let frac = scaled - floor;
    let (Some(a), Some(b)) = (ramp.get(index), ramp.get(next)) else {
        return [1.0, 1.0, 1.0];
    };
    [
        a[0] + (b[0] - a[0]) * frac,
        a[1] + (b[1] - a[1]) * frac,
        a[2] + (b[2] - a[2]) * frac,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RampFit {
    pub extinction: [f32; 3],
    pub rms: [f32; 3],
    pub body: [f32; 3],
}

pub const RAMP_FIT_TOLERANCE: f32 = 2.0 / 255.0;

#[must_use]
pub fn fit_ramp(ramp: &[[f32; 3]], fade_depth: f32) -> Option<RampFit> {
    let last = ramp.len().checked_sub(1)?;
    if last == 0 || !fade_depth.is_finite() || fade_depth <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let span = last as f32;
    let mut extinction = [0.0f32; 3];
    let mut rms = [0.0f32; 3];
    for channel in 0..3 {
        let mut numerator = 0.0f32;
        let mut denominator = 0.0f32;
        for (index, texel) in ramp.iter().enumerate() {
            let Some(value) = texel.get(channel).copied() else {
                continue;
            };
            if !value.is_finite() || value <= 0.0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let depth = index as f32 / span * fade_depth;
            numerator += depth * mint_math::f32::ln(value);
            denominator += depth * depth;
        }
        let sigma = if denominator > 0.0 {
            (-numerator / denominator).max(0.0)
        } else {
            0.0
        };
        let mut total = 0.0f32;
        for (index, texel) in ramp.iter().enumerate() {
            let Some(value) = texel.get(channel).copied() else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let depth = index as f32 / span * fade_depth;
            let error = value - mint_math::f32::exp(-sigma * depth);
            total += error * error;
        }
        #[allow(clippy::cast_precision_loss)]
        let mean = total / (last.saturating_add(1) as f32);
        if let Some(slot) = extinction.get_mut(channel) {
            *slot = sigma;
        }
        if let Some(slot) = rms.get_mut(channel) {
            *slot = mean.sqrt();
        }
    }
    let far = ramp.last().copied().unwrap_or([1.0, 1.0, 1.0]);
    Some(RampFit {
        extinction,
        rms,
        body: far,
    })
}

#[must_use]
pub fn body_absorption(
    shading: &BodyShading,
    transparency: WaterTransparency,
) -> Option<(RampFit, RampSource)> {
    let fade = transparency.transparent_depth();
    if let Some(ramp) = shading.ramp.as_ref() {
        if let Some(fit) = fit_ramp(ramp, fade) {
            return Some((fit, RampSource::Fitted));
        }
    }
    if !fade.is_finite() || fade <= 0.0 {
        return None;
    }
    let mut extinction = [0.0f32; 3];
    let mut far = [1.0f32; 3];
    for channel in 0..3 {
        let value = shading.color.get(channel).copied().unwrap_or(1.0);
        let clamped = value.clamp(1.0 / 255.0, 1.0);
        if let Some(slot) = extinction.get_mut(channel) {
            *slot = (-mint_math::f32::ln(clamped) / fade).max(0.0);
        }
        if let Some(slot) = far.get_mut(channel) {
            *slot = value;
        }
    }
    if extinction.iter().all(|sigma| *sigma <= 0.0) {
        return None;
    }
    Some((
        RampFit {
            extinction,
            rms: [0.0; 3],
            body: far,
        },
        RampSource::MaterialColor,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RampSource {
    Fitted,
    MaterialColor,
}

impl RampSource {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Fitted => "fitted to the depthColors ramp",
            Self::MaterialColor => "derived from MaterialColorDiffuse (no ramp named)",
        }
    }
}

#[must_use]
pub fn shade_body(
    body: &WaterBody,
    shading: &BodyShading,
    transparency: WaterTransparency,
    vertical_scale: f32,
    mode: WaterTint,
) -> (Vec<Vertex>, DepthRange) {
    let mut range = DepthRange::default();
    let scale = transparency.transparent_depth();
    let mut vertices = body.mesh.vertices.clone();
    for (index, vertex) in vertices.iter_mut().enumerate() {
        let ground = body.ground.get(index).copied().unwrap_or(0.0);
        let depth = body.height - ground * vertical_scale;
        vertex.uv2 = [depth.max(0.0), 0.0];
        if !mode.uses_depth() {
            vertex.color = WATER_TINT;
            continue;
        }
        range.add(depth, scale);
        let t = if scale > 0.0 {
            (depth / scale).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let rgb = match (mode, &shading.ramp) {
            (WaterTint::Ramp, Some(ramp)) => sample_ramp(ramp, t),
            (WaterTint::RampTinted, Some(ramp)) => {
                let sampled = sample_ramp(ramp, t);
                [
                    sampled[0] * shading.color[0],
                    sampled[1] * shading.color[1],
                    sampled[2] * shading.color[2],
                ]
            }
            _ => [shading.color[0], shading.color[1], shading.color[2]],
        };
        let alpha = (t * shading.color[3]).clamp(0.0, 1.0);
        vertex.color = if body.additive_blend {
            [rgb[0] * alpha, rgb[1] * alpha, rgb[2] * alpha, alpha]
        } else {
            [rgb[0], rgb[1], rgb[2], alpha]
        };
    }
    (vertices, range)
}

#[derive(Debug, Clone, Copy)]
pub struct DepthRange {
    pub min: f32,
    pub max: f32,
    pub faded: usize,
    pub total: usize,
}

impl Default for DepthRange {
    fn default() -> Self {
        Self {
            min: f32::INFINITY,
            max: f32::NEG_INFINITY,
            faded: 0,
            total: 0,
        }
    }
}

impl DepthRange {
    fn add(&mut self, depth: f32, fade_depth: f32) {
        if !depth.is_finite() {
            return;
        }
        self.min = self.min.min(depth);
        self.max = self.max.max(depth);
        self.total = self.total.saturating_add(1);
        if depth < fade_depth {
            self.faded = self.faded.saturating_add(1);
        }
    }

    pub fn merge(&mut self, other: Self) {
        if other.total == 0 {
            return;
        }
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.faded = self.faded.saturating_add(other.faded);
        self.total = self.total.saturating_add(other.total);
    }
}
