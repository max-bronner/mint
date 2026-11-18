use crate::tree::{Block, Document, Field};
use std::collections::BTreeMap;

pub const MOUSE_CURSOR_KEYWORD: &str = "MouseCursor";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MouseCursorDef {
    pub texture: Option<String>,
    pub image: Option<String>,
    pub hot_spot: Option<(i64, i64)>,
    pub directions: Option<u32>,
}

#[must_use]
pub fn read_mouse_cursor(block: &Block) -> MouseCursorDef {
    let hot_spot = block.field("HotSpot").and_then(hot_spot_of);
    MouseCursorDef {
        texture: field_string(block, "Texture"),
        image: field_string(block, "Image"),
        hot_spot,
        directions: block
            .field("Directions")
            .and_then(Field::value)
            .and_then(crate::value::parse_int)
            .and_then(|v| u32::try_from(v).ok()),
    }
}

fn field_string(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .map(str::to_owned)
}

fn hot_spot_of(field: &Field) -> Option<(i64, i64)> {
    let x = labelled_int(field.args.first()?, 'X')?;
    let y = labelled_int(field.args.get(1)?, 'Y')?;
    Some((x, y))
}

fn labelled_int(token: &str, label: char) -> Option<i64> {
    let rest = token
        .strip_prefix(label)
        .or_else(|| token.strip_prefix(label.to_ascii_lowercase()))
        .and_then(|r| r.strip_prefix(':'))
        .unwrap_or(token);
    rest.parse().ok()
}

#[derive(Debug, Clone, Default)]
pub struct MouseCursorIndex {
    defs: BTreeMap<String, MouseCursorDef>,
}

impl MouseCursorIndex {
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
        if !block.keyword.eq(MOUSE_CURSOR_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.defs
            .insert(name.to_ascii_lowercase(), read_mouse_cursor(block));
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MouseCursorDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.defs.keys().map(String::as_str)
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

#[must_use]
pub fn cursor_file(def: &MouseCursorDef, direction: u32) -> Option<String> {
    let texture = def.texture.as_deref()?;
    let cur_stem = texture
        .len()
        .checked_sub(CUR_SUFFIX.len())
        .and_then(|at| Some((texture.get(..at)?, texture.get(at..)?)))
        .filter(|(_, tail)| tail.eq_ignore_ascii_case(CUR_SUFFIX))
        .map(|(stem, _)| stem);
    let (stem, extension) = match cur_stem {
        Some(stem) => (stem, "cur"),
        None => (texture, "ani"),
    };
    Some(if def.directions.is_some_and(|d| d > 1) {
        format!("{stem}{direction}.{extension}")
    } else {
        format!("{stem}.{extension}")
    })
}

const CUR_SUFFIX: &str = ".cur";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    fn parse(src: &str) -> Document {
        let doc = Document::parse("mouse.ini", src, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        doc
    }

    #[test]
    fn the_four_real_fields_are_read() {
        let doc = parse(
            "MouseCursor Scroll\n  \
                Texture = SCCScroll\n  \
                Directions = 8\n\
            End\n",
        );
        let mut index = MouseCursorIndex::new();
        index.insert_document(&doc);
        let def = index.get("scroll").unwrap();
        assert_eq!(def.texture.as_deref(), Some("SCCScroll"));
        assert_eq!(def.directions, Some(8));
        assert_eq!(def.hot_spot, None);
    }

    #[test]
    fn a_labelled_hotspot_field_becomes_a_pair() {
        let doc = parse(
            "MouseCursor PlaceBeacon\n  \
                Texture = beacon.cur\n  \
                HotSpot = X:4 Y:5\n\
            End\n",
        );
        let mut index = MouseCursorIndex::new();
        index.insert_document(&doc);
        assert_eq!(index.get("PlaceBeacon").unwrap().hot_spot, Some((4, 5)));
    }

    #[test]
    fn a_cur_texture_loads_the_cur_file() {
        let def = MouseCursorDef {
            texture: Some("SCCPointer.cur".to_owned()),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 0).as_deref(), Some("SCCPointer.cur"));
    }

    #[test]
    fn a_bare_texture_loads_the_ani_file() {
        let def = MouseCursorDef {
            texture: Some("SCCAttack".to_owned()),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 0).as_deref(), Some("SCCAttack.ani"));
    }

    #[test]
    fn the_cur_suffix_is_matched_in_any_case() {
        let def = MouseCursorDef {
            texture: Some("beacon.CUR".to_owned()),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 0).as_deref(), Some("beacon.cur"));
    }

    #[test]
    fn a_directional_cur_texture_puts_the_index_before_the_extension() {
        let def = MouseCursorDef {
            texture: Some("SCCScroll.cur".to_owned()),
            directions: Some(8),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 3).as_deref(), Some("SCCScroll3.cur"));
    }

    #[test]
    fn a_single_direction_ignores_the_index() {
        let def = MouseCursorDef {
            texture: Some("SCCMove".to_owned()),
            directions: Some(1),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 5).as_deref(), Some("SCCMove.ani"));
    }

    #[test]
    fn a_name_shorter_than_the_suffix_is_an_ani() {
        let def = MouseCursorDef {
            texture: Some("ab".to_owned()),
            ..MouseCursorDef::default()
        };
        assert_eq!(cursor_file(&def, 0).as_deref(), Some("ab.ani"));
    }

    #[test]
    fn a_multibyte_name_does_not_split_a_character() {
        let def = MouseCursorDef {
            texture: Some("\u{e9}\u{e9}\u{e9}".to_owned()),
            ..MouseCursorDef::default()
        };
        assert_eq!(
            cursor_file(&def, 0).as_deref(),
            Some("\u{e9}\u{e9}\u{e9}.ani")
        );
    }

    #[test]
    fn no_texture_at_all_has_no_file() {
        assert_eq!(cursor_file(&MouseCursorDef::default(), 0), None);
    }
}
