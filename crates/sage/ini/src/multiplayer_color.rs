use std::collections::BTreeMap;

use crate::tree::{Block, Document};
use crate::value::tagged_value;

pub const MULTIPLAYER_COLOR_KEYWORD: &str = "MultiplayerColor";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    #[must_use]
    pub fn to_unit(self) -> [f32; 3] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        ]
    }
}

#[must_use]
pub fn rgb_field(block: &Block, keyword: &str) -> Option<Rgb8> {
    let args = &block.field(keyword)?.args;
    let channel = |tag: &str| -> Option<u8> {
        let raw = crate::value::parse_int(tagged_value(args, tag)?)?;
        Some(u8::try_from(raw.clamp(0, 255)).unwrap_or(u8::MAX))
    };
    Some(Rgb8 {
        r: channel("R")?,
        g: channel("G")?,
        b: channel("B")?,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MultiplayerColorDef {
    pub rgb: Option<Rgb8>,
    pub rgb_night: Option<Rgb8>,
}

#[must_use]
pub fn read_multiplayer_color(block: &Block) -> MultiplayerColorDef {
    MultiplayerColorDef {
        rgb: rgb_field(block, "RGBColor"),
        rgb_night: rgb_field(block, "RGBNightColor"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct MultiplayerColorIndex {
    defs: BTreeMap<String, MultiplayerColorDef>,
    order: Vec<String>,
}

impl MultiplayerColorIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(MULTIPLAYER_COLOR_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        let key = name.to_ascii_lowercase();
        if !self.defs.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.defs.insert(key, read_multiplayer_color(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MultiplayerColorDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn nth(&self, index: usize) -> Option<(&str, &MultiplayerColorDef)> {
        let key = self.order.get(index)?;
        Some((key.as_str(), self.defs.get(key)?))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.order.iter().map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn parse(text: &str) -> Document {
        let document =
            Document::parse("multiplayer.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        document
    }

    #[test]
    fn the_shipped_blue_and_red_read_as_written() {
        let document = parse(
            "MultiplayerColor ColorBlue\n  \
                RGBColor = R:70 G:91 B:156\n  \
                RGBNightColor = R:70 G:91 B:156\n  \
                LivingWorldColor = R:68 G:40 B:255\n  \
                TooltipName = Color:Blue\n\
             End\n\
             MultiplayerColor ColorRed\n  \
                RGBColor = R:158 G:56 B:42\n  \
                RGBNightColor = R:158 G:56 B:42\n\
             End\n",
        );
        let mut index = MultiplayerColorIndex::new();
        index.insert_document(&document);
        assert_eq!(index.len(), 2);
        let blue = index.get("ColorBlue").unwrap();
        assert_eq!(
            blue.rgb,
            Some(Rgb8 {
                r: 70,
                g: 91,
                b: 156
            })
        );
        assert_eq!(blue.rgb, blue.rgb_night);
        assert_eq!(
            index.get("colorRED").unwrap().rgb,
            Some(Rgb8 {
                r: 158,
                g: 56,
                b: 42
            })
        );
        assert_eq!(index.nth(0).unwrap().0, "colorblue");
        assert_eq!(index.nth(1).unwrap().0, "colorred");
        assert!(index.nth(2).is_none());
    }

    #[test]
    fn declaration_order_is_kept_and_is_not_alphabetical() {
        let document = parse(
            "MultiplayerColor ColorZebra\n  RGBColor = R:1 G:2 B:3\nEnd\n\
             MultiplayerColor ColorApple\n  RGBColor = R:4 G:5 B:6\nEnd\n",
        );
        let mut index = MultiplayerColorIndex::new();
        index.insert_document(&document);
        let names: Vec<&str> = index.names().collect();
        assert_eq!(names, ["colorzebra", "colorapple"]);
    }

    #[test]
    fn a_redefinition_keeps_one_place_in_the_order() {
        let document = parse(
            "MultiplayerColor ColorBlue\n  RGBColor = R:1 G:1 B:1\nEnd\n\
             MultiplayerColor ColorBlue\n  RGBColor = R:9 G:9 B:9\nEnd\n",
        );
        let mut index = MultiplayerColorIndex::new();
        index.insert_document(&document);
        assert_eq!(index.len(), 1);
        assert_eq!(index.names().count(), 1);
        assert_eq!(
            index.get("ColorBlue").unwrap().rgb,
            Some(Rgb8 { r: 9, g: 9, b: 9 })
        );
    }

    #[test]
    fn a_field_missing_a_channel_reads_as_absent() {
        let document = parse("MultiplayerColor ColorBroken\n  RGBColor = R:10 B:30\nEnd\n");
        let mut index = MultiplayerColorIndex::new();
        index.insert_document(&document);
        assert_eq!(index.get("ColorBroken").unwrap().rgb, None);
    }

    #[test]
    fn a_channel_outside_0_255_clamps_rather_than_wrapping() {
        let document = parse("MultiplayerColor ColorLoud\n  RGBColor = R:300 G:-40 B:255\nEnd\n");
        let mut index = MultiplayerColorIndex::new();
        index.insert_document(&document);
        assert_eq!(
            index.get("ColorLoud").unwrap().rgb,
            Some(Rgb8 {
                r: 255,
                g: 0,
                b: 255
            })
        );
    }

    #[test]
    fn unit_conversion_is_a_plain_divide_with_no_transfer_curve() {
        let white = Rgb8 {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(white.to_unit(), [1.0, 1.0, 1.0]);
        let half = Rgb8 { r: 128, g: 0, b: 0 };
        assert!((half.to_unit()[0] - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(half.to_unit()[1], 0.0);
    }
}
