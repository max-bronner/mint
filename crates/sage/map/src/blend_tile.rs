use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

const BLEND_RECORD_MARKER: u32 = 0x7ADA_0000;

const BLEND_LAYOUT: &[(u16, u32, u32)] = &[
    (8, 8, 1),
    (9, 8, 1),
    (10, 8, 2),
    (11, 8, 3),
    (12, 12, 3),
    (13, 12, 4),
    (14, 14, 4),
    (15, 14, 5),
    (16, 15, 5),
    (17, 15, 6),
    (18, 15, 6),
];

pub(crate) fn blend_layout(version: u16) -> Option<(u32, u32)> {
    BLEND_LAYOUT
        .iter()
        .find(|(v, _, _)| *v == version)
        .map(|(_, bytes, planes)| (*bytes, *planes))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureClass {
    pub first_tile: i32,
    pub num_tiles: i32,
    pub width: i32,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendRecord {
    pub tile_index: i32,
    pub horizontal: bool,
    pub vertical: bool,
    pub right_diagonal: bool,
    pub left_diagonal: bool,
    pub inverted: bool,
    pub long_diagonal: u8,
    pub custom_blend_edge_class: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CliffRecord {
    pub tile_index: i32,
    pub tex_coords: [(f32, f32); 4],
    pub flip: u8,
    pub unknown: u8,
}

#[derive(Debug, Clone)]
pub struct BlendTileData {
    pub version: u16,
    pub width: u32,
    pub height: u32,
    pub cell_count: u32,
    pub tile_index: Vec<u16>,
    pub blend_index: Vec<u32>,
    pub extra_blend_index: Vec<u32>,
    pub cliff_info_index: Vec<u32>,
    pub unknown_cell_byte: Option<Vec<u8>>,
    pub bit_planes: Vec<Vec<u8>>,
    pub num_bitmap_tiles: i32,
    pub texture_classes: Vec<TextureClass>,
    pub num_edge_tiles: i32,
    pub edge_texture_classes: Vec<TextureClass>,
    pub blend_records: Vec<BlendRecord>,
    pub cliff_records: Vec<CliffRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flammability {
    FireResistant,
    Grass,
    HighlyFlammable,
    Undefined,
}

impl Flammability {
    fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::FireResistant),
            1 => Some(Self::Grass),
            2 => Some(Self::HighlyFlammable),
            3 => Some(Self::Undefined),
            _ => None,
        }
    }
}

const PLANE_GATES: [u16; 6] = [7, 11, 10, 13, 15, 17];

fn plane_index_for(version: u16, threshold: u16) -> Option<usize> {
    if version < threshold {
        return None;
    }
    let position = PLANE_GATES.iter().position(|&gate| gate == threshold)?;
    Some(
        PLANE_GATES
            .iter()
            .take(position)
            .filter(|&&gate| gate <= version)
            .count(),
    )
}

fn plane_row_stride(width: u32) -> Result<u32> {
    width
        .checked_add(7)
        .and_then(|w| w.checked_div(8))
        .ok_or(Error::Malformed {
            name: "BlendTileData",
            reason: "bit-plane row stride overflowed computing ceil(width / 8)",
        })
}

fn read_u16_array(r: &mut Reader<'_>, count: u32) -> Result<Vec<u16>> {
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(r.u16()?);
    }
    Ok(out)
}

fn read_u16_as_u32_array(r: &mut Reader<'_>, count: u32) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(u32::from(r.u16()?));
    }
    Ok(out)
}

fn read_u32_array(r: &mut Reader<'_>, count: u32) -> Result<Vec<u32>> {
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(r.u32()?);
    }
    Ok(out)
}

fn read_u8_array(r: &mut Reader<'_>, count: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for _ in 0..count {
        out.push(r.u8()?);
    }
    Ok(out)
}

fn read_count(r: &mut Reader<'_>, field: &'static str) -> Result<i32> {
    let value = r.i32()?;
    if value < 0 {
        return Err(Error::Malformed {
            name: "BlendTileData",
            reason: field,
        });
    }
    Ok(value)
}

fn read_texture_classes(
    r: &mut Reader<'_>,
    count: i32,
    has_unused_field: bool,
) -> Result<Vec<TextureClass>> {
    let mut out = Vec::new();
    for _ in 0..count {
        let first_tile = r.i32()?;
        let num_tiles = r.i32()?;
        let width = r.i32()?;
        if has_unused_field {
            r.i32()?;
        }
        let name = r.ascii_string()?;
        out.push(TextureClass {
            first_tile,
            num_tiles,
            width,
            name,
        });
    }
    Ok(out)
}

fn read_blend_records(r: &mut Reader<'_>, count: i32) -> Result<Vec<BlendRecord>> {
    let mut out = Vec::new();
    for _ in 0..count {
        let offset = r.position();
        let tile_index = r.i32()?;
        let horizontal = r.u8()? != 0;
        let vertical = r.u8()? != 0;
        let right_diagonal = r.u8()? != 0;
        let left_diagonal = r.u8()? != 0;
        let inverted = r.u8()? != 0;
        let long_diagonal = r.u8()?;
        let custom_blend_edge_class = r.i32()?;
        let marker = r.u32()?;
        if marker != BLEND_RECORD_MARKER {
            return Err(Error::BadBlendMarker {
                offset,
                found: marker,
            });
        }
        out.push(BlendRecord {
            tile_index,
            horizontal,
            vertical,
            right_diagonal,
            left_diagonal,
            inverted,
            long_diagonal,
            custom_blend_edge_class,
        });
    }
    Ok(out)
}

fn read_cliff_records(r: &mut Reader<'_>, count: i32) -> Result<Vec<CliffRecord>> {
    let mut out = Vec::new();
    for _ in 0..count {
        let tile_index = r.i32()?;
        let mut tex_coords = [(0.0f32, 0.0f32); 4];
        for slot in &mut tex_coords {
            let u = r.f32()?;
            let v = r.f32()?;
            *slot = (u, v);
        }
        let flip = r.u8()?;
        let unknown = r.u8()?;
        out.push(CliffRecord {
            tile_index,
            tex_coords,
            flip,
            unknown,
        });
    }
    Ok(out)
}

pub(crate) fn read_blend_tile_data(
    body: &[u8],
    chunk: &RawChunk,
    width: u32,
    height: u32,
) -> Result<BlendTileData> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let cell_count = r.u32()?;
    if width.checked_mul(height) != Some(cell_count) {
        return Err(Error::CellCountMismatch {
            cell_count,
            width,
            height,
        });
    }

    let (bytes_per_cell, planes) =
        blend_layout(chunk.version).ok_or(Error::UnknownBlendVersion {
            version: chunk.version,
        })?;

    let tile_index = read_u16_array(&mut r, cell_count)?;

    let blend_index;
    let extra_blend_index;
    let cliff_info_index;
    match bytes_per_cell {
        8 => {
            blend_index = read_u16_as_u32_array(&mut r, cell_count)?;
            extra_blend_index = read_u16_as_u32_array(&mut r, cell_count)?;
            cliff_info_index = read_u16_as_u32_array(&mut r, cell_count)?;
        }
        12 => {
            blend_index = read_u32_array(&mut r, cell_count)?;
            extra_blend_index = read_u32_array(&mut r, cell_count)?;
            cliff_info_index = read_u16_as_u32_array(&mut r, cell_count)?;
        }
        14 | 15 => {
            blend_index = read_u32_array(&mut r, cell_count)?;
            extra_blend_index = read_u32_array(&mut r, cell_count)?;
            cliff_info_index = read_u32_array(&mut r, cell_count)?;
        }
        _ => {
            return Err(Error::UnknownBlendVersion {
                version: chunk.version,
            });
        }
    }

    let row_stride = plane_row_stride(width)?;
    let plane_len = row_stride.checked_mul(height).ok_or(Error::Malformed {
        name: "BlendTileData",
        reason: "bit-plane size overflowed computing stride * height",
    })?;
    let plane_len = usize::try_from(plane_len).unwrap_or(usize::MAX);

    let planes_before_byte_array = planes.min(5);
    let mut bit_planes = Vec::new();
    for _ in 0..planes_before_byte_array {
        bit_planes.push(r.take(plane_len)?.to_vec());
    }
    let unknown_cell_byte = if bytes_per_cell == 15 {
        Some(read_u8_array(&mut r, cell_count)?)
    } else {
        None
    };
    for _ in planes_before_byte_array..planes {
        bit_planes.push(r.take(plane_len)?.to_vec());
    }

    let num_bitmap_tiles = read_count(&mut r, "numBitmapTiles")?;
    let num_blended_tiles = read_count(&mut r, "numBlendedTiles")?;
    let num_cliff_info = read_count(&mut r, "numCliffInfo")?;
    let num_texture_classes = read_count(&mut r, "numTextureClasses")?;
    let texture_classes = read_texture_classes(&mut r, num_texture_classes, true)?;

    let num_edge_tiles = r.i32()?;
    let num_edge_texture_classes = read_count(&mut r, "numEdgeTextureClasses")?;
    let edge_texture_classes = read_texture_classes(&mut r, num_edge_texture_classes, false)?;

    let blend_record_count = num_blended_tiles.saturating_sub(1);
    let cliff_record_count = num_cliff_info.saturating_sub(1);
    let blend_records = read_blend_records(&mut r, blend_record_count)?;
    let cliff_records = read_cliff_records(&mut r, cliff_record_count)?;

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: "BlendTileData",
            offset: r.position(),
            remaining: r.remaining(),
        });
    }

    Ok(BlendTileData {
        version: chunk.version,
        width,
        height,
        cell_count,
        tile_index,
        blend_index,
        extra_blend_index,
        cliff_info_index,
        unknown_cell_byte,
        bit_planes,
        num_bitmap_tiles,
        texture_classes,
        num_edge_tiles,
        edge_texture_classes,
        blend_records,
        cliff_records,
    })
}

impl BlendTileData {
    #[must_use]
    pub fn impassable(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 7)?, x, y)
    }

    #[must_use]
    pub fn impassable_to_players(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 11)?, x, y)
    }

    #[must_use]
    pub fn passage_width_narrow(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 10)?, x, y)
    }

    #[must_use]
    pub fn not_taintable(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 13)?, x, y)
    }

    #[must_use]
    pub fn extra_passable(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 15)?, x, y)
    }

    #[must_use]
    pub fn visible(&self, x: u32, y: u32) -> Option<bool> {
        self.plane_bit(plane_index_for(self.version, 17)?, x, y)
    }

    #[must_use]
    pub fn flammability(&self, x: u32, y: u32) -> Option<Flammability> {
        if self.version < 16 {
            return None;
        }
        if x >= self.width || y >= self.height {
            return None;
        }
        let bytes = self.unknown_cell_byte.as_ref()?;
        let index = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.width).ok()?)?
            .checked_add(usize::try_from(x).ok()?)?;
        Flammability::from_raw(*bytes.get(index)?)
    }

    #[must_use]
    pub fn plane_bit(&self, plane: usize, x: u32, y: u32) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let plane = self.bit_planes.get(plane)?;
        let row_stride = plane_row_stride(self.width).ok()?;
        let row_start = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(row_stride).ok()?)?;
        let byte_index = row_start.checked_add(usize::try_from(x.checked_div(8)?).ok()?)?;
        let byte = *plane.get(byte_index)?;
        let shift = u8::try_from(x & 7).ok()?;
        Some((byte >> shift) & 1 != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::{blend_tile_payload, MapBuilder, ShapeParams};

    fn one_blend_tile_chunk(version: u16, payload: &[u8]) -> (Vec<u8>, RawChunk) {
        let mut b = MapBuilder::new();
        b.chunk("BlendTileData", version, payload);
        let body = b.build();
        let (_names, mut chunks) = parse_top_level(&body).expect("valid container");
        (body, chunks.remove(0))
    }

    #[test]
    fn version_8_four_array_shape_round_trips() {
        let params = ShapeParams {
            width: 2,
            height: 2,
            tile: vec![0, 4, 8, 12],
            blend: vec![1, 1, 1, 1],
            extra: vec![0, 0, 0, 0],
            cliff: vec![0, 0, 0, 0],
            ..ShapeParams::minimal(2, 2)
        };
        let payload = blend_tile_payload(8, &params);
        let (body, chunk) = one_blend_tile_chunk(8, &payload);

        let bt = read_blend_tile_data(&body, &chunk, 2, 2).expect("valid v8 chunk");
        assert_eq!(bt.tile_index, vec![0, 4, 8, 12]);
        assert_eq!(bt.blend_index, vec![1, 1, 1, 1]);
        assert!(bt.unknown_cell_byte.is_none());
        assert_eq!(bt.bit_planes.len(), 1);
        assert_eq!(bt.blend_records.len(), 0);
        assert_eq!(bt.cliff_records.len(), 0);
    }

    #[test]
    fn version_18_seven_array_shape_round_trips_with_records() {
        let mut params = ShapeParams::minimal(2, 2);
        params.tile = vec![0, 4, 8, 12];
        params.blend = vec![1, 0, 70_000, 0];
        params.extra = vec![0, 0, 0, 0];
        params.cliff = vec![0, 0, 0, 0];
        params.unknown_byte = Some(vec![9, 9, 9, 9]);
        params.num_bitmap_tiles = 4;
        params.num_blended_tiles = 2;
        params.texture_classes = vec![(0, 4, 1, "grass".to_string())];
        params.blend_record_tile = 0;

        let payload = blend_tile_payload(18, &params);
        let (body, chunk) = one_blend_tile_chunk(18, &payload);

        let bt = read_blend_tile_data(&body, &chunk, 2, 2).expect("valid v18 chunk");
        assert_eq!(bt.blend_index, vec![1, 0, 70_000, 0]);
        assert_eq!(bt.unknown_cell_byte, Some(vec![9, 9, 9, 9]));
        assert_eq!(bt.bit_planes.len(), 6);
        assert_eq!(bt.blend_records.len(), 1);
        assert_eq!(bt.texture_classes.len(), 1);
        assert_eq!(bt.texture_classes[0].name, "grass");
    }

    #[test]
    fn impassable_reads_the_right_bit_lsb_first_with_the_right_row_stride() {
        let mut params = ShapeParams::minimal(10, 2);
        params.bit_planes = Some(vec![vec![0x08, 0x00, 0x00, 0x02]]);
        let payload = blend_tile_payload(9, &params);
        let (body, chunk) = one_blend_tile_chunk(9, &payload);
        let bt = read_blend_tile_data(&body, &chunk, 10, 2).expect("valid v9 chunk");

        assert_eq!(bt.impassable(3, 0), Some(true));
        assert_eq!(bt.impassable(2, 0), Some(false));
        assert_eq!(bt.impassable(4, 0), Some(false));
        assert_eq!(bt.impassable(0, 0), Some(false));
        assert_eq!(bt.impassable(9, 1), Some(true));
        assert_eq!(bt.impassable(8, 1), Some(false));
        assert_eq!(bt.impassable(10, 0), None);
        assert_eq!(bt.impassable(0, 2), None);
    }

    #[test]
    fn version_18_byte_array_sits_between_plane_4_and_plane_5() {
        let mut params = ShapeParams::minimal(10, 2);
        params.num_bitmap_tiles = 4;
        params.texture_classes = vec![(0, 20, 1, "grass".to_string())];
        let row_stride = usize::try_from(10u32.div_ceil(8)).unwrap_or(0);
        let mut plane0 = vec![0u8; row_stride * 2];
        plane0[0] = 0x08;
        let mut plane5 = vec![0u8; row_stride * 2];
        plane5[1] = 0x02;
        params.bit_planes = Some(vec![
            plane0,
            vec![0u8; row_stride * 2],
            vec![0u8; row_stride * 2],
            vec![0u8; row_stride * 2],
            vec![0u8; row_stride * 2],
            plane5,
        ]);
        params.unknown_byte = Some(vec![2u8; 20]);
        let payload = blend_tile_payload(18, &params);
        let (body, chunk) = one_blend_tile_chunk(18, &payload);
        let bt = read_blend_tile_data(&body, &chunk, 10, 2).expect("valid v18 chunk");

        assert_eq!(bt.impassable(3, 0), Some(true));
        assert_eq!(bt.impassable(0, 0), Some(false));
        assert_eq!(bt.visible(9, 0), Some(true));
        assert_eq!(bt.visible(0, 0), Some(false));
        assert_eq!(bt.flammability(0, 0), Some(Flammability::HighlyFlammable));
    }

    #[test]
    fn version_thresholds_gate_each_named_property() {
        let params9 = ShapeParams::minimal(2, 2);
        let payload9 = blend_tile_payload(9, &params9);
        let (body9, chunk9) = one_blend_tile_chunk(9, &payload9);
        let bt9 = read_blend_tile_data(&body9, &chunk9, 2, 2).expect("valid v9 chunk");
        assert!(bt9.impassable(0, 0).is_some());
        assert!(bt9.impassable_to_players(0, 0).is_none());
        assert!(bt9.passage_width_narrow(0, 0).is_none());
        assert!(bt9.not_taintable(0, 0).is_none());
        assert!(bt9.extra_passable(0, 0).is_none());
        assert!(bt9.visible(0, 0).is_none());
        assert!(bt9.flammability(0, 0).is_none());

        let params11 = ShapeParams::minimal(2, 2);
        let payload11 = blend_tile_payload(11, &params11);
        let (body11, chunk11) = one_blend_tile_chunk(11, &payload11);
        let bt11 = read_blend_tile_data(&body11, &chunk11, 2, 2).expect("valid v11 chunk");
        assert!(bt11.impassable_to_players(0, 0).is_some());
        assert!(bt11.passage_width_narrow(0, 0).is_some());
        assert!(bt11.not_taintable(0, 0).is_none());

        let payload13 = blend_tile_payload(13, &ShapeParams::minimal(2, 2));
        let (body13, chunk13) = one_blend_tile_chunk(13, &payload13);
        let bt13 = read_blend_tile_data(&body13, &chunk13, 2, 2).expect("valid v13 chunk");
        assert!(bt13.not_taintable(0, 0).is_some());
        assert!(bt13.extra_passable(0, 0).is_none());

        let params18 = ShapeParams::minimal(2, 2);
        let payload18 = blend_tile_payload(18, &params18);
        let (body18, chunk18) = one_blend_tile_chunk(18, &payload18);
        let bt18 = read_blend_tile_data(&body18, &chunk18, 2, 2).expect("valid v18 chunk");
        assert!(bt18.not_taintable(0, 0).is_some());
        assert!(bt18.extra_passable(0, 0).is_some());
        assert!(bt18.visible(0, 0).is_some());
        assert_eq!(bt18.flammability(0, 0), Some(Flammability::FireResistant));
    }

    #[test]
    fn unknown_version_is_an_error() {
        let params = ShapeParams::minimal(2, 2);
        let payload = blend_tile_payload(8, &params);
        let (body, chunk) = one_blend_tile_chunk(19, &payload);
        let err = read_blend_tile_data(&body, &chunk, 2, 2).unwrap_err();
        assert!(matches!(err, Error::UnknownBlendVersion { version: 19 }));
    }

    #[test]
    fn cell_count_mismatch_is_an_error() {
        let params = ShapeParams::minimal(2, 2);
        let payload = blend_tile_payload(8, &params);
        let (body, chunk) = one_blend_tile_chunk(8, &payload);
        let err = read_blend_tile_data(&body, &chunk, 3, 3).unwrap_err();
        assert!(matches!(err, Error::CellCountMismatch { .. }));
    }

    #[test]
    fn bad_blend_marker_is_an_error() {
        let mut params = ShapeParams::minimal(2, 2);
        params.num_blended_tiles = 2;
        params.blend_marker = Some(0xDEAD_BEEF);
        let payload = blend_tile_payload(8, &params);
        let (body, chunk) = one_blend_tile_chunk(8, &payload);
        let err = read_blend_tile_data(&body, &chunk, 2, 2).unwrap_err();
        assert!(matches!(err, Error::BadBlendMarker { .. }));
    }

    #[test]
    fn declared_blend_count_of_zero_or_one_both_mean_no_records() {
        for declared in [0, 1] {
            let mut params = ShapeParams::minimal(2, 2);
            params.num_blended_tiles = declared;
            let payload = blend_tile_payload(8, &params);
            let (body, chunk) = one_blend_tile_chunk(8, &payload);
            let bt = read_blend_tile_data(&body, &chunk, 2, 2).expect("valid");
            assert_eq!(bt.blend_records.len(), 0, "declared count {declared}");
        }
    }
}
