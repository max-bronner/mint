use std::collections::BTreeMap;

use crate::diagnostic::{ValueError, ValueErrors};
use crate::tree::{Block, Document};
use crate::value::{parse_bool, parse_int};

pub const VIDEO_KEYWORD: &str = "Video";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoDef {
    pub filename: Option<String>,
    pub comment: Option<String>,
    pub has_subtitles: Option<bool>,
    pub volume: Option<i64>,
    pub is_default: Option<bool>,
}

#[must_use]
pub fn read_video(block: &Block, errors: &ValueErrors) -> VideoDef {
    let context = block.name().unwrap_or(VIDEO_KEYWORD);
    let text = |keyword: &str| {
        block
            .field(keyword)
            .map(|field| field.args.join(" "))
            .filter(|value| !value.is_empty())
    };
    let boolean = |keyword: &str| {
        let field = block.field(keyword)?;
        let read = field.value().and_then(parse_bool);
        if read.is_none() {
            errors.field(context, field, "a boolean");
        }
        read
    };
    let volume = block.field("Volume").and_then(|field| {
        let read = field.value().and_then(parse_int);
        if read.is_none() {
            errors.field(context, field, "an integer");
        }
        read
    });
    for field in block.fields() {
        let known = ["filename", "comment", "hassubtitles", "volume", "isdefault"];
        if !known.contains(&field.keyword.to_ascii_lowercase().as_str()) {
            errors.field(context, field, "a Video field");
        }
    }
    VideoDef {
        filename: text("Filename"),
        comment: text("Comment"),
        has_subtitles: boolean("HasSubtitles"),
        volume,
        is_default: boolean("IsDefault"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct VideoIndex {
    defs: BTreeMap<String, VideoDef>,
    errors: Vec<ValueError>,
}

impl VideoIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots_named(VIDEO_KEYWORD) {
            let Some(name) = block.name() else {
                continue;
            };
            let errors = ValueErrors::new();
            let def = read_video(block, &errors);
            self.errors.append(&mut errors.take());
            self.defs.insert(name.to_ascii_lowercase(), def);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&VideoDef> {
        self.defs.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn errors(&self) -> &[ValueError] {
        &self.errors
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
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_block_reads_its_filename_and_an_unknown_field_is_an_error() {
        let text = "Video MovieAlpha
  Filename = stem_a // note
End
                    Video MovieBeta
  Filename = stem_b
  Frames = 3
End
";
        let doc = Document::parse("video.ini", text, &BlockTable::corpus_path()).unwrap();
        let mut videos = VideoIndex::new();
        videos.insert_document(&doc);
        assert_eq!(
            videos.get("moviealpha").unwrap().filename.as_deref(),
            Some("stem_a")
        );
        assert_eq!(videos.errors().len(), 1, "{:?}", videos.errors());
    }
}
