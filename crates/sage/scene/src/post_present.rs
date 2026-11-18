use mint_image::RgbaImage;
use mint_math::f32::exp;
use mint_render::{ColorLut, ColorVolumeHandle, Glow, GlowTap, PostEffects, Renderer};
use mint_sage_ini::GlowEffectSetting;
use mint_sage_map::{GlobalLighting, PostEffect};
use mint_vfs::Vfs;

use crate::lighting::ENVIRONMENT_INI;

const MIN_TAP_WEIGHT: f32 = 0.01;
const MAX_TAPS: u32 = 64;
const LOOKUP_TABLE_EFFECT: &str = "LookupTablePostEffect";
const VOLUME_EDGE: u32 = 32;

#[must_use]
pub fn glow_taps(setting: &GlowEffectSetting) -> Vec<GlowTap> {
    let diameter = setting.diameter.min(MAX_TAPS);
    if diameter < 2 {
        return Vec::new();
    }
    let d = diameter as f32;
    let mut scale = 15.0 / d;
    if scale > 1.0 {
        scale = (1.0 + scale) / 2.0;
    }
    let centre = (d - 1.0) / 2.0;
    (0..diameter)
        .filter_map(|i| {
            let x = i as f32 - centre - 0.1;
            let q = x * x / (centre * centre);
            let weight = setting.amplitude1 * scale * exp(-setting.radius_scale1 * q)
                + setting.amplitude2 * scale * exp(-setting.radius_scale2 * q);
            (weight > MIN_TAP_WEIGHT).then_some(GlowTap { offset: x, weight })
        })
        .collect()
}

#[must_use]
pub fn load_glow_effect(vfs: &Vfs, map_path: &str) -> (GlowEffectSetting, String) {
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let map_ini = format!("{folder}/map.ini");
    let mut documents = Vec::new();
    for path in [ENVIRONMENT_INI, map_ini.as_str()] {
        let Ok(bytes) = vfs.read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let table = mint_sage_ini::BlockTable::corpus_path();
        if let Ok(document) = mint_sage_ini::Document::parse(path, &text, &table) {
            documents.push(document);
        }
    }
    let setting = mint_sage_ini::glow_effect(&documents);
    let line = format!(
        "glow effect: enabled {}, diameter {}, intensity {}, texture {}, kernel {} taps",
        setting.enabled,
        setting.diameter,
        setting.intensity,
        setting.texture_width,
        glow_taps(&setting).len()
    );
    (setting, line)
}

#[must_use]
pub fn color_volume_from_strip(image: &RgbaImage) -> Option<Vec<u8>> {
    let edge = usize::try_from(VOLUME_EDGE).ok()?;
    if image.width() != VOLUME_EDGE.checked_mul(VOLUME_EDGE)? || image.height() != VOLUME_EDGE {
        return None;
    }
    let mut cube = Vec::with_capacity(edge.checked_pow(3)?.checked_mul(4)?);
    for blue in 0..VOLUME_EDGE {
        for green in 0..VOLUME_EDGE {
            for red in 0..VOLUME_EDGE {
                cube.extend_from_slice(&image.pixel(blue * VOLUME_EDGE + red, green)?);
            }
        }
    }
    Some(cube)
}

pub fn upload_lookup_table(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &mint_sage_asset::Resolver,
    effects: &[PostEffect],
) -> Result<ColorLut, String> {
    let effect = effects
        .iter()
        .find(|e| e.name == LOOKUP_TABLE_EFFECT)
        .ok_or_else(|| "no lookup-table post effect".to_owned())?;
    let name = &effect.lookup_image;
    let path = resolver
        .resolve(vfs, name)
        .map_err(|e| format!("lookup table {name:?}: {e}"))?;
    let bytes = vfs
        .read_path(&path)
        .map_err(|e| format!("lookup table {name:?}: {e}"))?;
    let image =
        mint_image::tga::decode(&bytes).map_err(|e| format!("lookup table {name:?}: {e}"))?;
    let cube = color_volume_from_strip(&image)
        .ok_or_else(|| format!("lookup table {name:?} is not a 1024x32 strip"))?;
    let volume = renderer
        .upload_color_volume(VOLUME_EDGE, &cube)
        .map_err(|e| format!("lookup table {name:?}: {e}"))?;
    Ok(ColorLut {
        volume,
        blend: effect.blend_factor.clamp(0.0, 1.0),
    })
}

#[derive(Debug, Clone)]
pub struct PostPresent {
    setting: GlowEffectSetting,
    taps: Vec<GlowTap>,
    bloom_colour: [f32; 3],
    map_bloom: bool,
    lut: Option<ColorLut>,
    effects_lod_allows: bool,
}

impl PostPresent {
    #[must_use]
    pub fn new(
        setting: GlowEffectSetting,
        lighting: &GlobalLighting,
        lut: Option<ColorLut>,
        effects_lod_allows: bool,
    ) -> Self {
        Self {
            taps: glow_taps(&setting),
            setting,
            bloom_colour: lighting.unnamed.colors[0],
            map_bloom: lighting.unnamed.bloom_enabled != 0,
            lut,
            effects_lod_allows,
        }
    }

    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "post: map bloom flag {}, bloom colour {:?}, lookup table {}, EffectsLOD gate {}",
            self.map_bloom,
            self.bloom_colour,
            self.lut
                .map_or_else(|| "none".to_owned(), |lut| format!("blend {}", lut.blend)),
            if self.effects_lod_allows {
                "open"
            } else {
                "closed (EffectsLOD Low or below)"
            }
        )
    }

    #[must_use]
    pub fn lut_volume(&self) -> Option<ColorVolumeHandle> {
        self.lut.map(|lut| lut.volume)
    }

    #[must_use]
    pub fn effects(&self, script_bloom: Option<bool>) -> PostEffects<'_> {
        if !self.effects_lod_allows {
            return PostEffects {
                glow: None,
                lut: None,
            };
        }
        if let Some(lut) = self.lut {
            return PostEffects {
                glow: None,
                lut: Some(lut),
            };
        }
        let bloom = script_bloom.unwrap_or(self.map_bloom);
        let glow = (bloom && self.setting.enabled).then(|| Glow {
            bright_offset: self.bloom_colour.map(|c| c - 1.0),
            texture_size: self.setting.texture_width,
            taps: &self.taps,
            intensity: self.setting.intensity,
        });
        PostEffects { glow, lut: None }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;

    fn defaults() -> GlowEffectSetting {
        GlowEffectSetting {
            enabled: true,
            diameter: 15,
            intensity: 1.0,
            texture_width: 64,
            radius_scale1: 0.5,
            amplitude1: 0.4,
            radius_scale2: 3.0,
            amplitude2: 0.2,
        }
    }

    #[test]
    fn the_kernel_samples_a_tenth_of_a_texel_before_the_centre_and_drops_thin_taps() {
        let taps = glow_taps(&defaults());
        assert!((taps.first().unwrap().offset - -7.1).abs() < 1e-5);
        assert!((taps.last().unwrap().offset - 6.9).abs() < 1e-5);
        let narrow = GlowEffectSetting {
            amplitude1: 0.011,
            amplitude2: 0.0,
            ..defaults()
        };
        let thin = glow_taps(&narrow);
        assert!(thin.iter().all(|t| t.weight > MIN_TAP_WEIGHT));
        assert!(thin.len() < 15);
    }

    #[test]
    fn a_diameter_above_fifteen_scales_the_weights_by_half_the_excess() {
        let centre_tap = |taps: &[GlowTap]| {
            taps.iter()
                .min_by(|a, b| a.offset.abs().total_cmp(&b.offset.abs()))
                .unwrap()
                .weight
        };
        let at = |diameter| {
            centre_tap(&glow_taps(&GlowEffectSetting {
                diameter,
                ..defaults()
            }))
        };
        let ratio = at(7) / at(15);
        assert!((ratio - 1.57).abs() < 0.1, "ratio {ratio}");
    }

    #[test]
    fn the_last_bloom_writer_wins_a_lookup_table_replaces_it_and_effects_lod_gates_both() {
        let lut = ColorLut {
            volume: ColorVolumeHandle::default(),
            blend: 0.5,
        };
        let post = |map_bloom, lut: Option<ColorLut>, effects_lod_allows| PostPresent {
            taps: glow_taps(&defaults()),
            setting: defaults(),
            bloom_colour: [0.5; 3],
            map_bloom,
            lut,
            effects_lod_allows,
        };
        let state = |p: &PostPresent, script| {
            let e = p.effects(script);
            (e.glow.is_some(), e.lut.is_some())
        };
        assert_eq!(state(&post(false, None, true), None), (false, false));
        assert_eq!(state(&post(true, None, true), None), (true, false));
        assert_eq!(state(&post(false, None, true), Some(true)), (true, false));
        assert_eq!(state(&post(true, None, true), Some(false)), (false, false));
        assert_eq!(
            state(&post(true, Some(lut), true), Some(true)),
            (false, true)
        );
        assert_eq!(state(&post(true, None, false), Some(true)), (false, false));
        assert_eq!(state(&post(false, Some(lut), false), None), (false, false));
    }

    #[test]
    fn a_strip_becomes_a_cube_indexed_blue_slice_then_green_row_then_red_texel() {
        let mut pixels = vec![0u8; 1024 * 32 * 4];
        for (b, g, r) in [(3u32, 5u32, 7u32), (31, 0, 31)] {
            let at = ((g * 1024 + b * 32 + r) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[r as u8, g as u8, b as u8, 255]);
        }
        let image = mint_image::tga::decode(&tga_of(1024, 32, &pixels)).unwrap();
        let cube = color_volume_from_strip(&image).unwrap();
        let index = |b: u32, g: u32, r: u32| (((b * 32 + g) * 32 + r) * 4) as usize;
        assert_eq!(cube[index(3, 5, 7)..index(3, 5, 7) + 4], [7, 5, 3, 255]);
        assert_eq!(
            cube[index(31, 0, 31)..index(31, 0, 31) + 4],
            [31, 0, 31, 255]
        );
    }

    fn tga_of(width: u16, height: u16, rgba: &[u8]) -> Vec<u8> {
        let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        out.extend_from_slice(&width.to_le_bytes());
        out.extend_from_slice(&height.to_le_bytes());
        out.extend_from_slice(&[32, 0x28]);
        for px in rgba.chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
        out
    }
}
