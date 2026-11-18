use std::collections::BTreeMap;

use crate::multiplayer_color::{rgb_field, Rgb8};
use crate::tree::{Block, Document};
use crate::value::{parse_bool, parse_float, tagged_value};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HardwareFog {
    pub enabled: bool,
    pub color: Rgb8,
    pub start: f32,
    pub end: f32,
}

impl Default for HardwareFog {
    fn default() -> Self {
        Self {
            enabled: false,
            color: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            start: 0.0,
            end: 1000.0,
        }
    }
}

impl HardwareFog {
    #[must_use]
    pub fn active(&self) -> Option<Self> {
        (self.enabled && self.end > self.start).then_some(*self)
    }

    fn apply(&mut self, block: &Block) {
        let field = |keyword: &str| block.field(keyword).and_then(crate::tree::Field::value);
        if let Some(value) = field("HardwareFogEnable").and_then(parse_bool) {
            self.enabled = value;
        }
        if let Some(value) = rgb_field(block, "HardwareFogColor") {
            self.color = value;
        }
        let real = |keyword: &str| {
            field(keyword)
                .and_then(parse_float)
                .filter(|value| value.is_finite())
        };
        if let Some(value) = real("HardwareFogStart") {
            self.start = value;
        }
        if let Some(value) = real("HardwareFogEnd") {
            self.end = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnowSetting {
    pub enabled: bool,
    pub snowing: bool,
    pub texture: String,
    pub box_height: f32,
    pub spacing: f32,
    pub tiles: Option<f32>,
    pub amplitude: f32,
    pub frequency: [f32; 2],
    pub speed: f32,
    pub drift: [f32; 2],
    pub quad_size: f32,
    pub point_sprites: bool,
    pub point_size: [f32; 3],
    pub spell_enabled: bool,
    pub spell_duration: f32,
    pub ramp_control: [f32; 2],
    pub ramp_spacing: [f32; 2],
    pub ramp_speed: [f32; 2],
    pub fog: HardwareFog,
}

impl SnowSetting {
    #[must_use]
    pub fn spell_duration_seconds(&self) -> f32 {
        self.spell_duration / 30.0
    }
}

impl Default for SnowSetting {
    fn default() -> Self {
        Self {
            enabled: false,
            snowing: false,
            texture: "EXSnowFlake.tga".to_owned(),
            box_height: 200.0,
            spacing: 1.0,
            tiles: None,
            amplitude: 5.0,
            frequency: [0.0533, 0.0275],
            speed: 4.0,
            drift: [0.0, 0.0],
            quad_size: 0.5,
            point_sprites: true,
            point_size: [1.0, 0.0, 64.0],
            spell_enabled: false,
            spell_duration: 0.0,
            ramp_control: [0.0, 0.0],
            ramp_spacing: [0.0, 0.0],
            ramp_speed: [0.0, 0.0],
            fog: HardwareFog::default(),
        }
    }
}

pub const MIN_SNOW_SPACING: f32 = 4.0;

pub const MAX_SNOW_BOX: f32 = 8000.0;

impl SnowSetting {
    #[must_use]
    pub fn falls(&self) -> bool {
        self.enabled && self.snowing
    }

    pub fn apply(&mut self, block: &Block) {
        self.fog.apply(block);
        let real = |keyword: &str| {
            block
                .field(keyword)
                .and_then(crate::tree::Field::value)
                .and_then(parse_float)
                .filter(|value| value.is_finite())
        };
        let flag = |keyword: &str| {
            block
                .field(keyword)
                .and_then(crate::tree::Field::value)
                .and_then(parse_bool)
        };
        if let Some(value) = flag("SnowEnabled") {
            self.enabled = value;
        }
        if let Some(value) = flag("IsSnowing") {
            self.snowing = value;
        }
        if let Some(value) = block
            .field("SnowTexture")
            .and_then(crate::tree::Field::value)
            .filter(|value| !value.is_empty())
        {
            value.clone_into(&mut self.texture);
        }
        if let Some(value) = real("SnowBoxHeight").filter(|v| *v > 0.0) {
            self.box_height = value.min(MAX_SNOW_BOX);
        }
        if let Some(value) = real("SnowSpacing").filter(|v| *v > 0.0) {
            self.spacing = value.max(MIN_SNOW_SPACING);
        }
        if let Some(value) = real("NumberTiles") {
            self.tiles = Some(value);
        }
        if let Some(value) = real("SnowAmplitude") {
            self.amplitude = value;
        }
        if let Some(value) = real("SnowFrequencyScaleX") {
            self.frequency[0] = value;
        }
        if let Some(value) = real("SnowFrequencyScaleY") {
            self.frequency[1] = value;
        }
        if let Some(value) = real("SnowSpeed") {
            self.speed = value;
        }
        if let Some(field) = block.field("SnowXYSpeed") {
            let axis = |tag: &str| {
                tagged_value(&field.args, tag)
                    .and_then(parse_float)
                    .filter(|value| value.is_finite())
            };
            self.drift = [axis("X").unwrap_or(0.0), axis("Y").unwrap_or(0.0)];
        }
        if let Some(value) = real("SnowQuadSize").filter(|v| *v > 0.0) {
            self.quad_size = value;
        }
        if let Some(value) = flag("SnowPointSprites") {
            self.point_sprites = value;
        }
        for (slot, keyword) in ["SnowPointSize", "SnowMinPointSize", "SnowMaxPointSize"]
            .into_iter()
            .enumerate()
        {
            if let (Some(value), Some(held)) = (real(keyword), self.point_size.get_mut(slot)) {
                *held = value;
            }
        }
        if let Some(value) = flag("SpellEnabled") {
            self.spell_enabled = value;
        }
        if let Some(value) = real("SpellDuration").filter(|v| *v >= 0.0) {
            self.spell_duration = value;
        }
        let axes = |field: &str| {
            block.field(field).map(|field| {
                let axis = |tag: &str| {
                    tagged_value(&field.args, tag)
                        .and_then(parse_float)
                        .filter(|value| value.is_finite())
                };
                (axis("X"), axis("Y"))
            })
        };
        if let Some((x, y)) = axes("RampControl") {
            if let Some(x) = x {
                self.ramp_control[0] = x;
            }
            if let Some(y) = y {
                self.ramp_control[1] = y;
            }
        }
        if let Some((x, y)) = axes("RampSpacing") {
            if let Some(x) = x {
                self.ramp_spacing[0] = x;
            }
            if let Some(y) = y {
                self.ramp_spacing[1] = y;
            }
        }
        if let Some((x, y)) = axes("RampSpeed") {
            if let Some(x) = x {
                self.ramp_speed[0] = x;
            }
            if let Some(y) = y {
                self.ramp_speed[1] = y;
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeatherSound {
    pub sound: Option<String>,
    pub has_lightning: bool,
}

impl WeatherSound {
    fn apply(&mut self, block: &Block) {
        if let Some(value) = block
            .field("WeatherSound")
            .and_then(crate::tree::Field::value)
            .filter(|value| !value.is_empty())
        {
            self.sound = Some(value.to_owned());
        }
        if let Some(value) = block
            .field("HasLightning")
            .and_then(crate::tree::Field::value)
            .and_then(parse_bool)
        {
            self.has_lightning = value;
        }
    }
}

#[must_use]
pub fn weather_sounds(documents: &[Document]) -> BTreeMap<String, WeatherSound> {
    let mut table = BTreeMap::new();
    for document in documents {
        for block in document.roots_named("WeatherData") {
            let Some(kind) = block.name() else { continue };
            table
                .entry(kind.to_ascii_uppercase())
                .or_insert_with(WeatherSound::default)
                .apply(block);
        }
    }
    table
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::{BlockTable, Document};

    fn weather(text: &str) -> Block {
        let document = Document::parse("t.ini", text, &BlockTable::corpus_path()).unwrap();
        let found = document.roots_named("Weather").next().cloned();
        found.unwrap()
    }

    const GLOBAL: &str = "\
Weather
  SnowEnabled = yes
  IsSnowing = no
  SnowTexture = Flake.tga
  SnowBoxHeight = 300
  SnowSpacing = 30
  NumberTiles = 4
  SnowSpeed = 50.0
  SnowQuadSize = 10
  SnowPointSprites = no
End
";

    const MAP: &str = "\
Weather
  IsSnowing = Yes
  SnowSpacing = 60
  SnowSpeed = 120.0
  SnowXYSpeed = X:40 Y:25
  SnowTexture = Flake2.tga
End
";

    #[test]
    fn a_maps_block_overrides_the_global_one_field_by_field() {
        let mut snow = SnowSetting::default();
        snow.apply(&weather(GLOBAL));
        assert!(snow.enabled && !snow.snowing && !snow.falls());
        assert_eq!(snow.box_height, 300.0);
        snow.apply(&weather(MAP));
        assert!(snow.falls(), "the map turns it on");
        assert_eq!(snow.spacing, 60.0);
        assert_eq!(snow.speed, 120.0);
        assert_eq!(snow.drift, [40.0, 25.0]);
        assert_eq!(snow.texture, "Flake2.tga");
        assert_eq!(snow.box_height, 300.0);
        assert_eq!(snow.quad_size, 10.0);
        assert_eq!(snow.tiles, Some(4.0));
        assert!(!snow.point_sprites);
    }

    #[test]
    fn hardware_fog_is_read_and_an_inverted_range_is_inactive() {
        let mut snow = SnowSetting::default();
        assert_eq!(snow.fog.active(), None);
        snow.apply(&weather(
            "Weather\n  HardwareFogEnable = Yes\n  HardwareFogColor = R:10 G:20 B:30\n  \
             HardwareFogStart = 150\n  HardwareFogEnd = 900\nEnd\n",
        ));
        let fog = snow.fog.active().unwrap();
        assert_eq!((fog.color.r, fog.color.g, fog.color.b), (10, 20, 30));
        assert_eq!((fog.start, fog.end), (150.0, 900.0));
        snow.apply(&weather("Weather\n  HardwareFogEnd = 100\nEnd\n"));
        assert_eq!(snow.fog.active(), None);
    }

    #[test]
    fn hostile_numbers_are_bounded() {
        let mut snow = SnowSetting::default();
        snow.apply(&weather(
            "Weather\n  SnowSpacing = 0.001\n  SnowBoxHeight = 1e30\n  SnowSpeed = banana\nEnd\n",
        ));
        assert_eq!(snow.spacing, MIN_SNOW_SPACING);
        assert_eq!(snow.box_height, MAX_SNOW_BOX);
        assert_eq!(snow.speed, SnowSetting::default().speed);
    }

    #[test]
    fn weather_data_is_read_per_kind_with_a_later_document_overriding() {
        let global = Document::parse(
            "t.ini",
            "WeatherData RAINY\n  WeatherSound = TestRainLoop\n  HasLightning = Yes\nEnd\n\
             WeatherData CLOUDY\n  HasLightning = No\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let map = Document::parse(
            "map.ini",
            "WeatherData RAINY\n  HasLightning = No\nEnd\n",
            &BlockTable::corpus_path(),
        )
        .unwrap();
        let table = weather_sounds(&[global, map]);
        let rainy = table.get("RAINY").unwrap();
        assert_eq!(rainy.sound.as_deref(), Some("TestRainLoop"));
        assert!(!rainy.has_lightning, "the map's record overrides it");
        let cloudy = table.get("CLOUDY").unwrap();
        assert_eq!(cloudy.sound, None);
        assert!(!table.contains_key("SUNNY"), "no block names it at all");
    }
}
