use crate::blend_tile::blend_layout;
use crate::container::NameTable;

pub(crate) struct MapBuilder {
    names: Vec<String>,
    top_chunks: Vec<u8>,
}

impl MapBuilder {
    pub(crate) fn new() -> Self {
        Self {
            names: Vec::new(),
            top_chunks: Vec::new(),
        }
    }

    pub(crate) fn intern(&mut self, name: &str) -> u32 {
        if let Some(pos) = self.names.iter().position(|n| n == name) {
            return u32::try_from(pos.saturating_add(1)).expect("test tables are small");
        }
        self.names.push(name.to_string());
        u32::try_from(self.names.len()).expect("test tables are small")
    }

    pub(crate) fn name_table(&self) -> NameTable {
        NameTable::from_pairs(self.names.iter().enumerate().map(|(i, n)| {
            (
                u32::try_from(i.saturating_add(1)).unwrap_or(u32::MAX),
                n.clone(),
            )
        }))
    }

    pub(crate) fn nested_chunk(&mut self, name: &str, version: u16, payload: &[u8]) -> Vec<u8> {
        let index = self.intern(name);
        let mut out = index.to_le_bytes().to_vec();
        out.extend_from_slice(&version.to_le_bytes());
        let len = u32::try_from(payload.len()).expect("test payloads are small");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    pub(crate) fn chunk(&mut self, name: &str, version: u16, payload: &[u8]) {
        let bytes = self.nested_chunk(name, version, payload);
        self.top_chunks.extend_from_slice(&bytes);
    }

    pub(crate) fn ascii(s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let len = u16::try_from(s.len()).expect("test strings are short");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(s.as_bytes());
        out
    }

    pub(crate) fn name_key(&mut self, name: &str) -> u32 {
        (self.intern(name) << 8) | 3
    }

    fn dict_entry(&mut self, key: &str, kind: u8, value: &[u8]) -> Vec<u8> {
        let index = self.intern(key);
        let packed = (index << 8) | u32::from(kind);
        let mut out = packed.to_le_bytes().to_vec();
        out.extend_from_slice(value);
        out
    }

    pub(crate) fn dict_bool(&mut self, key: &str, value: bool) -> Vec<u8> {
        self.dict_entry(key, 0, &[u8::from(value)])
    }

    pub(crate) fn dict_int(&mut self, key: &str, value: i32) -> Vec<u8> {
        self.dict_entry(key, 1, &value.to_le_bytes())
    }

    pub(crate) fn dict_float(&mut self, key: &str, value: f32) -> Vec<u8> {
        self.dict_entry(key, 2, &value.to_le_bytes())
    }

    pub(crate) fn dict_str(&mut self, key: &str, value: &str) -> Vec<u8> {
        let bytes = Self::ascii(value);
        self.dict_entry(key, 3, &bytes)
    }

    pub(crate) fn dict_wide(&mut self, key: &str, value: &str) -> Vec<u8> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let mut bytes = u16::try_from(units.len())
            .expect("test strings are short")
            .to_le_bytes()
            .to_vec();
        for unit in units {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        self.dict_entry(key, 4, &bytes)
    }

    pub(crate) fn dict(&self, entries: Vec<Vec<u8>>) -> Vec<u8> {
        let mut out = u16::try_from(entries.len())
            .expect("test dictionaries are small")
            .to_le_bytes()
            .to_vec();
        for entry in entries {
            out.extend_from_slice(&entry);
        }
        out
    }

    pub(crate) fn object_fields(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        angle: f32,
        flags: i32,
        template: &str,
        entries: Vec<Vec<u8>>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        out.extend_from_slice(&z.to_le_bytes());
        out.extend_from_slice(&angle.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&Self::ascii(template));
        let dict = self.dict(entries);
        out.extend_from_slice(&dict);
        out
    }

    pub(crate) fn build(&self) -> Vec<u8> {
        let mut out = b"CkMp".to_vec();
        let count = u32::try_from(self.names.len()).expect("test tables are small");
        out.extend_from_slice(&count.to_le_bytes());
        for (i, name) in self.names.iter().enumerate() {
            let len = u8::try_from(name.len()).expect("test names are short");
            out.push(len);
            out.extend_from_slice(name.as_bytes());
            let index = u32::try_from(i.saturating_add(1)).expect("test tables are small");
            out.extend_from_slice(&index.to_le_bytes());
        }
        out.extend_from_slice(&self.top_chunks);
        out
    }
}

pub(crate) fn height_map_payload(
    width: u32,
    height: u32,
    border: u32,
    boundaries: &[(i32, i32)],
    samples: &[u16],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&border.to_le_bytes());
    let boundary_count = u32::try_from(boundaries.len()).expect("test boundaries are few");
    out.extend_from_slice(&boundary_count.to_le_bytes());
    for (x, y) in boundaries {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
    }
    let sample_count = u32::try_from(samples.len()).expect("test samples are few");
    out.extend_from_slice(&sample_count.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

pub(crate) struct ShapeParams {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tile: Vec<u16>,
    pub(crate) blend: Vec<u32>,
    pub(crate) extra: Vec<u32>,
    pub(crate) cliff: Vec<u32>,
    pub(crate) unknown_byte: Option<Vec<u8>>,
    pub(crate) bit_planes: Option<Vec<Vec<u8>>>,
    pub(crate) num_bitmap_tiles: i32,
    pub(crate) num_blended_tiles: i32,
    pub(crate) num_cliff_info: i32,
    pub(crate) texture_classes: Vec<(i32, i32, i32, String)>,
    pub(crate) num_edge_tiles: i32,
    pub(crate) edge_texture_classes: Vec<(i32, i32, i32, String)>,
    pub(crate) blend_record_tile: i32,
    pub(crate) cliff_record_tile: i32,
    pub(crate) blend_marker: Option<u32>,
}

impl ShapeParams {
    pub(crate) fn minimal(width: u32, height: u32) -> Self {
        let cells = usize::try_from(width.saturating_mul(height)).unwrap_or(0);
        Self {
            width,
            height,
            tile: vec![0; cells],
            blend: vec![0; cells],
            extra: vec![0; cells],
            cliff: vec![0; cells],
            unknown_byte: None,
            bit_planes: None,
            num_bitmap_tiles: 1,
            num_blended_tiles: 1,
            num_cliff_info: 1,
            texture_classes: Vec::new(),
            num_edge_tiles: 0,
            edge_texture_classes: Vec::new(),
            blend_record_tile: 0,
            cliff_record_tile: 0,
            blend_marker: None,
        }
    }
}

const BLEND_RECORD_MARKER: u32 = 0x7ADA_0000;

pub(crate) fn blend_tile_payload(version: u16, params: &ShapeParams) -> Vec<u8> {
    let (bytes_per_cell, planes) =
        blend_layout(version).expect("test uses a version present in BLEND_LAYOUT");
    let cell_count = params.width.saturating_mul(params.height);

    let mut out = cell_count.to_le_bytes().to_vec();
    let push_u16 = |out: &mut Vec<u8>, arr: &[u16]| {
        for v in arr {
            out.extend_from_slice(&v.to_le_bytes());
        }
    };
    let push_index = |out: &mut Vec<u8>, arr: &[u32], wide_from: u32| {
        for v in arr {
            if bytes_per_cell >= wide_from {
                out.extend_from_slice(&v.to_le_bytes());
            } else {
                let narrowed = u16::try_from(*v).expect("test blend indices fit a u16");
                out.extend_from_slice(&narrowed.to_le_bytes());
            }
        }
    };
    push_u16(&mut out, &params.tile);
    push_index(&mut out, &params.blend, 12);
    push_index(&mut out, &params.extra, 12);
    push_index(&mut out, &params.cliff, 14);

    let row_stride = params.width.saturating_add(7).checked_div(8).unwrap_or(0);
    let plane_len = usize::try_from(row_stride.saturating_mul(params.height)).unwrap_or(0);
    let push_plane = |out: &mut Vec<u8>, slot: u32| match params
        .bit_planes
        .as_ref()
        .and_then(|planes| planes.get(usize::try_from(slot).unwrap_or(usize::MAX)))
    {
        Some(bytes) => {
            assert_eq!(
                bytes.len(),
                plane_len,
                "test bit plane {slot} has the wrong length"
            );
            out.extend_from_slice(bytes);
        }
        None => out.extend_from_slice(&vec![0u8; plane_len]),
    };
    let planes_before_byte_array = planes.min(5);
    for slot in 0..planes_before_byte_array {
        push_plane(&mut out, slot);
    }
    if bytes_per_cell == 15 {
        let default_u8 = vec![0u8; params.tile.len()];
        let bytes = params.unknown_byte.as_deref().unwrap_or(&default_u8);
        out.extend_from_slice(bytes);
    }
    for slot in planes_before_byte_array..planes {
        push_plane(&mut out, slot);
    }

    out.extend_from_slice(&params.num_bitmap_tiles.to_le_bytes());
    out.extend_from_slice(&params.num_blended_tiles.to_le_bytes());
    out.extend_from_slice(&params.num_cliff_info.to_le_bytes());
    let num_classes = i32::try_from(params.texture_classes.len()).expect("small");
    out.extend_from_slice(&num_classes.to_le_bytes());
    for (first, tiles, w, name) in &params.texture_classes {
        out.extend_from_slice(&first.to_le_bytes());
        out.extend_from_slice(&tiles.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&MapBuilder::ascii(name));
    }

    out.extend_from_slice(&params.num_edge_tiles.to_le_bytes());
    let num_edge_classes = i32::try_from(params.edge_texture_classes.len()).expect("small");
    out.extend_from_slice(&num_edge_classes.to_le_bytes());
    for (first, tiles, w, name) in &params.edge_texture_classes {
        out.extend_from_slice(&first.to_le_bytes());
        out.extend_from_slice(&tiles.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&MapBuilder::ascii(name));
    }

    let blend_records = params.num_blended_tiles.saturating_sub(1).max(0);
    for _ in 0..blend_records {
        out.extend_from_slice(&params.blend_record_tile.to_le_bytes());
        out.extend_from_slice(&[0u8; 5]);
        out.push(0);
        out.extend_from_slice(&0i32.to_le_bytes());
        let marker = params.blend_marker.unwrap_or(BLEND_RECORD_MARKER);
        out.extend_from_slice(&marker.to_le_bytes());
    }

    let cliff_records = params.num_cliff_info.saturating_sub(1).max(0);
    for _ in 0..cliff_records {
        out.extend_from_slice(&params.cliff_record_tile.to_le_bytes());
        for _ in 0..4 {
            out.extend_from_slice(&0f32.to_le_bytes());
            out.extend_from_slice(&0f32.to_le_bytes());
        }
        out.push(0);
        out.push(0);
    }

    out
}

pub(crate) fn global_lighting_payload(version: u16, time_of_day: u32) -> Vec<u8> {
    let mut out = time_of_day.to_le_bytes().to_vec();
    for c in 0..4usize {
        for l in 0..9usize {
            for f in 0..9usize {
                let value = (c as f32) * 100.0 + (l as f32) * 10.0 + (f as f32);
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    out.extend_from_slice(&0x8080_4020u32.to_le_bytes());
    out.extend_from_slice(&[0xAB; 44]);
    if version >= 8 {
        for v in [1.0f32, 2.0, 3.0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}
