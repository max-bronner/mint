use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};

pub const HOUSE_COLOR_KEYWORD: &str = "HouseColor";

pub const HOUSE_COLOR_FIELDS: [&str; 2] = ["BaseTexture", "HouseTexture"];

#[must_use]
pub fn texture_key(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem.trim().to_ascii_lowercase()
}

#[derive(Debug, Clone, Default)]
pub struct HouseColorTable {
    masks: BTreeMap<String, String>,
    blocks: usize,
    incomplete: usize,
    conflicts: usize,
}

impl HouseColorTable {
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
        if !block.keyword.eq(HOUSE_COLOR_KEYWORD) {
            return;
        }
        self.blocks = self.blocks.saturating_add(1);
        let base = block.field("BaseTexture").and_then(Field::value);
        let mask = block.field("HouseTexture").and_then(Field::value);
        let (Some(base), Some(mask)) = (base, mask) else {
            self.incomplete = self.incomplete.saturating_add(1);
            return;
        };
        let key = texture_key(base);
        if key.is_empty() || mask.trim().is_empty() {
            self.incomplete = self.incomplete.saturating_add(1);
            return;
        }
        if let Some(previous) = self.masks.get(&key) {
            if !previous.eq_ignore_ascii_case(mask) {
                self.conflicts = self.conflicts.saturating_add(1);
            }
        }
        self.masks.insert(key, mask.to_owned());
    }

    #[must_use]
    pub fn mask_for(&self, base_texture: &str) -> Option<&str> {
        self.masks
            .get(&texture_key(base_texture))
            .map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.masks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.masks.is_empty()
    }

    #[must_use]
    pub fn blocks(&self) -> usize {
        self.blocks
    }

    #[must_use]
    pub fn incomplete(&self) -> usize {
        self.incomplete
    }

    #[must_use]
    pub fn conflicts(&self) -> usize {
        self.conflicts
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
        let document = Document::parse("housecolor.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        document
    }

    #[test]
    fn the_documented_example_maps_its_base_texture_to_its_mask() {
        let document = parse(
            "HouseColor\n  \
                BaseTexture = GUManAtArms.tga\n  \
                HouseTexture = HC_GUManAtArms.tga\n\
            End\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        assert_eq!(table.len(), 1);
        assert_eq!(table.blocks(), 1);
        assert_eq!(
            table.mask_for("GUManAtArms.tga"),
            Some("HC_GUManAtArms.tga")
        );
    }

    #[test]
    fn lookup_ignores_case_and_extension() {
        let document = parse(
            "HouseColor\n  \
                BaseTexture = GUArcher.tga\n  \
                HouseTexture = HC_GUArcher.tga\n\
            End\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        for spelling in ["GUArcher.tga", "guarcher.TGA", "GUARCHER.dds", "guarcher"] {
            assert_eq!(
                table.mask_for(spelling),
                Some("HC_GUArcher.tga"),
                "{spelling} did not resolve"
            );
        }
    }

    #[test]
    fn many_base_textures_may_share_one_mask_without_conflicting() {
        let document = parse(
            "HouseColor\n  BaseTexture = GUHbtShfA.tga\n  HouseTexture = HC_GUHbtShfA.tga\nEnd\n\
             HouseColor\n  BaseTexture = GUHbtShfB.tga\n  HouseTexture = HC_GUHbtShfA.tga\nEnd\n\
             HouseColor\n  BaseTexture = GUHbtShfC.tga\n  HouseTexture = HC_GUHbtShfA.tga\nEnd\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        assert_eq!(table.len(), 3);
        assert_eq!(table.conflicts(), 0);
    }

    #[test]
    fn a_redefinition_to_a_different_mask_is_counted_and_the_later_one_wins() {
        let document = parse(
            "HouseColor\n  BaseTexture = A.tga\n  HouseTexture = HC_One.tga\nEnd\n\
             HouseColor\n  BaseTexture = A.tga\n  HouseTexture = HC_One.tga\nEnd\n\
             HouseColor\n  BaseTexture = A.tga\n  HouseTexture = HC_Two.tga\nEnd\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        assert_eq!(table.len(), 1);
        assert_eq!(table.blocks(), 3);
        assert_eq!(
            table.conflicts(),
            1,
            "the identical repeat is not a conflict"
        );
        assert_eq!(table.mask_for("a.tga"), Some("HC_Two.tga"));
    }

    #[test]
    fn a_block_missing_either_field_is_counted_and_skipped() {
        let document = parse(
            "HouseColor\n  BaseTexture = OnlyBase.tga\nEnd\n\
             HouseColor\n  HouseTexture = HC_OnlyMask.tga\nEnd\n\
             HouseColor\nEnd\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        assert!(table.is_empty());
        assert_eq!(table.blocks(), 3);
        assert_eq!(table.incomplete(), 3);
    }

    #[test]
    fn a_block_of_another_keyword_is_ignored() {
        let document = parse(
            "Upgrade Upgrade_Something\n  BaseTexture = A.tga\n  HouseTexture = B.tga\nEnd\n",
        );
        let mut table = HouseColorTable::new();
        table.insert_document(&document);
        assert!(table.is_empty());
        assert_eq!(table.blocks(), 0);
    }
}
