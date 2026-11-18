use crate::tree::{Block, Document};
use crate::value::{parse_bool, parse_float, parse_int};

pub const GLOW_EFFECT_KEYWORD: &str = "GlowEffect";

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlowEffectSetting {
    pub enabled: bool,
    pub diameter: u32,
    pub intensity: f32,
    pub texture_width: u32,
    pub radius_scale1: f32,
    pub amplitude1: f32,
    pub radius_scale2: f32,
    pub amplitude2: f32,
}

fn first_arg<'a>(block: &'a Block, keyword: &str) -> Option<&'a str> {
    block.field(keyword)?.args.first().map(String::as_str)
}

fn float(block: &Block, keyword: &str) -> Option<f32> {
    parse_float(first_arg(block, keyword)?)
}

fn count(block: &Block, keyword: &str) -> Option<u32> {
    u32::try_from(parse_int(first_arg(block, keyword)?)?).ok()
}

impl GlowEffectSetting {
    fn apply(&mut self, block: &Block) {
        if let Some(v) = first_arg(block, "GlowEnabled").and_then(parse_bool) {
            self.enabled = v;
        }
        if let Some(v) = count(block, "GlowDiameter") {
            self.diameter = v;
        }
        if let Some(v) = float(block, "GlowIntensity") {
            self.intensity = v;
        }
        if let Some(v) = count(block, "GlowTextureWidth") {
            self.texture_width = v;
        }
        if let Some(v) = float(block, "RadiusScale1") {
            self.radius_scale1 = v;
        }
        if let Some(v) = float(block, "Amplitude1") {
            self.amplitude1 = v;
        }
        if let Some(v) = float(block, "RadiusScale2") {
            self.radius_scale2 = v;
        }
        if let Some(v) = float(block, "Amplitude2") {
            self.amplitude2 = v;
        }
    }
}

#[must_use]
pub fn glow_effect(documents: &[Document]) -> GlowEffectSetting {
    let mut setting = GlowEffectSetting::default();
    for document in documents {
        for block in document.roots_named(GLOW_EFFECT_KEYWORD) {
            setting.apply(block);
        }
    }
    setting
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::BlockTable;

    #[test]
    fn a_later_document_overrides_field_by_field() {
        let table = BlockTable::corpus_path();
        let global = Document::parse(
            "global.ini",
            "GlowEffect\n  GlowEnabled = Yes\n  GlowDiameter = 9\n  GlowIntensity = 2.5\n  GlowTextureWidth = 64\nEnd\n",
            &table,
        )
        .unwrap();
        let map = Document::parse(
            "map.ini",
            "GlowEffect\n  GlowIntensity = 0.5\n  Amplitude1 = 0.25\nEnd\n",
            &table,
        )
        .unwrap();
        let setting = glow_effect(&[global, map]);
        assert!(setting.enabled);
        assert_eq!(setting.diameter, 9);
        assert_eq!(setting.texture_width, 64);
        assert_eq!(setting.intensity, 0.5);
        assert_eq!(setting.amplitude1, 0.25);
    }
}
