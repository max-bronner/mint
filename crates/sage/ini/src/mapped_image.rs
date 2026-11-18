use std::collections::BTreeMap;

use crate::tree::{Block, Document, Field};
use crate::value::parse_int;

pub const MAPPED_IMAGE_KEYWORD: &str = "MappedImage";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MappedRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl MappedRect {
    #[must_use]
    pub fn width(&self) -> u32 {
        u32::try_from(self.right.saturating_sub(self.left).saturating_add(1)).unwrap_or(0)
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        u32::try_from(self.bottom.saturating_sub(self.top).saturating_add(1)).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MappedImageDef {
    pub texture: Option<String>,
    pub texture_width: Option<u32>,
    pub texture_height: Option<u32>,
    pub coords: Option<MappedRect>,
    pub status: Option<String>,
}

fn parse_coords(field: &Field) -> MappedRect {
    let mut rect = MappedRect::default();
    let mut tokens = field
        .args
        .iter()
        .flat_map(|arg| arg.split(':'))
        .filter(|t| !t.is_empty());
    while let Some(key) = tokens.next() {
        let Some(n) = tokens
            .clone()
            .next()
            .and_then(parse_int)
            .and_then(|n| i32::try_from(n).ok())
        else {
            continue;
        };
        tokens.next();
        if key.eq_ignore_ascii_case("Left") {
            rect.left = n;
        } else if key.eq_ignore_ascii_case("Top") {
            rect.top = n;
        } else if key.eq_ignore_ascii_case("Right") {
            rect.right = n;
        } else if key.eq_ignore_ascii_case("Bottom") {
            rect.bottom = n;
        }
    }
    rect
}

#[must_use]
pub fn read_mapped_image(block: &Block) -> MappedImageDef {
    MappedImageDef {
        texture: block
            .field("Texture")
            .and_then(Field::value)
            .map(str::to_owned),
        texture_width: block
            .field("TextureWidth")
            .and_then(Field::value)
            .and_then(parse_int)
            .and_then(|n| u32::try_from(n).ok()),
        texture_height: block
            .field("TextureHeight")
            .and_then(Field::value)
            .and_then(parse_int)
            .and_then(|n| u32::try_from(n).ok()),
        coords: block.field("Coords").map(parse_coords),
        status: block
            .field("Status")
            .and_then(Field::value)
            .map(str::to_owned),
    }
}

#[derive(Debug, Clone, Default)]
pub struct MappedImageIndex {
    defs: BTreeMap<String, MappedImageDef>,
    definitions: usize,
}

impl MappedImageIndex {
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
        if !block.keyword.eq(MAPPED_IMAGE_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.saturating_add(1);
        self.defs
            .insert(name.to_ascii_lowercase(), read_mapped_image(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MappedImageDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
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
        Document::parse("mappedimage.ini", text, &BlockTable::corpus_path()).unwrap()
    }

    #[test]
    fn a_mapped_image_reads_its_five_fields() {
        let doc = parse(
            "MappedImage UCBerserker_Sword\n  \
                Texture = UCBerserker_Sword.tga\n  \
                TextureWidth = 64\n  \
                TextureHeight = 64\n  \
                Coords = Left:0 Top:0 Right:63 Bottom:63\n  \
                Status = NONE\n\
            End\n",
        );
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let def = read_mapped_image(&doc.roots()[0]);
        assert_eq!(def.texture.as_deref(), Some("UCBerserker_Sword.tga"));
        assert_eq!(def.texture_width, Some(64));
        assert_eq!(def.texture_height, Some(64));
        let rect = def.coords.unwrap();
        assert_eq!(
            rect,
            MappedRect {
                left: 0,
                top: 0,
                right: 63,
                bottom: 63
            }
        );
        assert_eq!(rect.width(), 64);
        assert_eq!(rect.height(), 64);
        assert_eq!(def.status.as_deref(), Some("NONE"));
    }

    #[test]
    fn a_dropped_colon_reads_as_the_engine_reads_it() {
        let doc = parse(
            "MappedImage Odd\n  \
                Coords = Left:0 Top:0 Right:600 Bottom 294\n\
            End\n",
        );
        let def = read_mapped_image(&doc.roots()[0]);
        let rect = def.coords.unwrap();
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (0, 0, 600, 294)
        );
    }

    #[test]
    fn a_bad_token_is_skipped_not_fatal() {
        let doc = parse(
            "MappedImage Odd\n  \
                Coords = Left:4 Wide:9 Top:x Right:600 Bottom:20\n\
            End\n",
        );
        let rect = read_mapped_image(&doc.roots()[0]).coords.unwrap();
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (4, 0, 600, 20)
        );
    }

    #[test]
    fn a_later_document_shadows_an_earlier_one_by_name() {
        let mut index = MappedImageIndex::new();
        index.insert_document(&parse(
            "MappedImage Shared\n  Texture = FromBase.tga\nEnd\n",
        ));
        index.insert_document(&parse(
            "MappedImage Shared\n  Texture = FromPatch.tga\nEnd\n",
        ));
        assert_eq!(
            index.get("shared").and_then(|d| d.texture.as_deref()),
            Some("FromPatch.tga")
        );
        assert_eq!(index.definitions(), 2);
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn lookup_is_case_insensitive_and_absence_is_none() {
        let mut index = MappedImageIndex::new();
        index.insert_document(&parse("MappedImage Foo\n  Texture = Foo.tga\nEnd\n"));
        assert!(index.get("FOO").is_some());
        assert!(index.get("bar").is_none());
    }
}
