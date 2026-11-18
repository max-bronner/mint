use mint_sage_ini::{Block, Field};

use super::Reads;

#[derive(Debug, Clone, PartialEq)]
pub struct FormationPreviewDecal {
    pub texture: String,
    pub width: f32,
    pub height: f32,
}

pub(super) fn formation_preview_decal(
    lineage: &[&Block],
    keyword: &str,
    reads: &Reads<'_>,
) -> Option<FormationPreviewDecal> {
    let block = lineage
        .iter()
        .rev()
        .find_map(|block| block.blocks_named(keyword).next())?;
    Some(FormationPreviewDecal {
        texture: block
            .field("Texture")
            .and_then(Field::value)
            .filter(|value| !value.eq_ignore_ascii_case("NONE"))
            .map(str::to_owned)?,
        width: reads.number(block.field("Width"))?,
        height: reads.number(block.field("Height"))?,
    })
}
