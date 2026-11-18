#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

pub(crate) struct TgaBuilder {
    width: u16,
    height: u16,
    image_type: u8,
    bits_per_pixel: u8,
    top_origin: bool,
    right_origin: bool,
    alpha_bits: u8,
    id_field: Vec<u8>,
    colour_map: Option<(u16, u8)>,
}

impl TgaBuilder {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            image_type: 2,
            bits_per_pixel: 32,
            top_origin: false,
            right_origin: false,
            alpha_bits: 0,
            id_field: Vec::new(),
            colour_map: None,
        }
    }

    pub(crate) fn image_type(mut self, image_type: u8) -> Self {
        self.image_type = image_type;
        self
    }

    pub(crate) fn bits_per_pixel(mut self, bits: u8) -> Self {
        self.bits_per_pixel = bits;
        self
    }

    pub(crate) fn top_origin(mut self) -> Self {
        self.top_origin = true;
        self
    }

    pub(crate) fn right_origin(mut self) -> Self {
        self.right_origin = true;
        self
    }

    pub(crate) fn alpha_bits(mut self, bits: u8) -> Self {
        self.alpha_bits = bits;
        self
    }

    pub(crate) fn id_field(mut self, id: &[u8]) -> Self {
        self.id_field = id.to_vec();
        self
    }

    pub(crate) fn colour_map(mut self, entries: u16, entry_bits: u8) -> Self {
        self.colour_map = Some((entries, entry_bits));
        self
    }

    pub(crate) fn data(self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(u8::try_from(self.id_field.len()).unwrap());
        out.push(u8::from(self.colour_map.is_some()));
        out.push(self.image_type);
        out.extend_from_slice(&0u16.to_le_bytes());
        let (map_entries, map_entry_bits) = self.colour_map.unwrap_or((0, 0));
        out.extend_from_slice(&map_entries.to_le_bytes());
        out.push(map_entry_bits);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.push(self.bits_per_pixel);

        let mut descriptor = self.alpha_bits & 0x0f;
        if self.right_origin {
            descriptor |= 0x10;
        }
        if self.top_origin {
            descriptor |= 0x20;
        }
        out.push(descriptor);

        out.extend_from_slice(&self.id_field);
        if self.colour_map.is_some() {
            let entry_bytes = usize::from(map_entry_bits).div_ceil(8);
            out.extend(std::iter::repeat_n(
                0u8,
                usize::from(map_entries) * entry_bytes,
            ));
        }
        out.extend_from_slice(data);
        out
    }
}

pub(crate) struct DdsBuilder {
    magic: [u8; 4],
    header_size: u32,
    flags: u32,
    width: u32,
    height: u32,
    linear_size: u32,
    depth: u32,
    mip_count: u32,
    pixel_format_size: u32,
    pixel_format_flags: u32,
    four_cc: [u8; 4],
    bit_count: u32,
    caps2: u32,
}

pub(crate) const DDSD_MIPMAPCOUNT: u32 = 0x0002_0000;

pub(crate) const DDPF_FOURCC: u32 = 0x4;

impl DdsBuilder {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            magic: *b"DDS ",
            header_size: 124,
            flags: 0x0000_1007 | DDSD_MIPMAPCOUNT,
            width,
            height,
            linear_size: 0,
            depth: 0,
            mip_count: 1,
            pixel_format_size: 32,
            pixel_format_flags: DDPF_FOURCC,
            four_cc: *b"DXT1",
            bit_count: 0,
            caps2: 0,
        }
    }

    pub(crate) fn magic(mut self, magic: [u8; 4]) -> Self {
        self.magic = magic;
        self
    }

    pub(crate) fn header_size(mut self, size: u32) -> Self {
        self.header_size = size;
        self
    }

    pub(crate) fn pixel_format_size(mut self, size: u32) -> Self {
        self.pixel_format_size = size;
        self
    }

    pub(crate) fn pixel_format_flags(mut self, flags: u32) -> Self {
        self.pixel_format_flags = flags;
        self
    }

    pub(crate) fn bit_count(mut self, bits: u32) -> Self {
        self.bit_count = bits;
        self
    }

    pub(crate) fn four_cc(mut self, four_cc: [u8; 4]) -> Self {
        self.four_cc = four_cc;
        self
    }

    pub(crate) fn mip_count(mut self, count: u32) -> Self {
        self.mip_count = count;
        self
    }

    pub(crate) fn linear_size(mut self, size: u32) -> Self {
        self.linear_size = size;
        self
    }

    pub(crate) fn depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    pub(crate) fn caps2(mut self, caps2: u32) -> Self {
        self.caps2 = caps2;
        self
    }

    pub(crate) fn data(self, blocks: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.magic);
        out.extend_from_slice(&self.header_size.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.linear_size.to_le_bytes());
        out.extend_from_slice(&self.depth.to_le_bytes());
        out.extend_from_slice(&self.mip_count.to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, 11 * 4));
        out.extend_from_slice(&self.pixel_format_size.to_le_bytes());
        out.extend_from_slice(&self.pixel_format_flags.to_le_bytes());
        out.extend_from_slice(&self.four_cc);
        out.extend_from_slice(&self.bit_count.to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, 4 * 4));
        out.extend_from_slice(&0x0000_1000u32.to_le_bytes());
        out.extend_from_slice(&self.caps2.to_le_bytes());
        out.extend(std::iter::repeat_n(0u8, 3 * 4));
        assert_eq!(out.len(), 128, "DDS header must be exactly 128 bytes");
        out.extend_from_slice(blocks);
        out
    }

    pub(crate) fn zero_filled(self) -> Vec<u8> {
        let bytes_per_block = if self.four_cc == *b"DXT1" { 8 } else { 16 };
        let mut total = 0usize;
        for level in 0..self.mip_count.max(1) {
            let w = usize::try_from((self.width >> level).max(1)).unwrap();
            let h = usize::try_from((self.height >> level).max(1)).unwrap();
            total += w.div_ceil(4) * h.div_ceil(4) * bytes_per_block;
        }
        let blocks = vec![0u8; total];
        self.data(&blocks)
    }
}

pub(crate) fn colour_block(colour0: u16, colour1: u16, indices: u32) -> [u8; 8] {
    let mut block = [0u8; 8];
    block[0..2].copy_from_slice(&colour0.to_le_bytes());
    block[2..4].copy_from_slice(&colour1.to_le_bytes());
    block[4..8].copy_from_slice(&indices.to_le_bytes());
    block
}

pub(crate) fn alpha_block(alpha0: u8, alpha1: u8, indices: u64) -> [u8; 8] {
    let mut block = [0u8; 8];
    block[0] = alpha0;
    block[1] = alpha1;
    block[2..8].copy_from_slice(&indices.to_le_bytes()[..6]);
    block
}

pub(crate) fn jpeg_grey_8x8() -> Vec<u8> {
    let mut out = jpeg_headers(8, 8);

    out.push(0b0000_1111);

    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn jpeg_headers(width: u16, height: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0xFF, 0xD8]);

    out.extend_from_slice(&[0xFF, 0xDB]);
    out.extend_from_slice(&(2u16 + 1 + 64).to_be_bytes());
    out.push(0x00);
    out.extend_from_slice(&[16u8; 64]);

    out.extend_from_slice(&jpeg_sof0(width, height));

    for class_and_id in [0x00u8, 0x10] {
        out.extend_from_slice(&[0xFF, 0xC4]);
        out.extend_from_slice(&(2u16 + 1 + 16 + 1).to_be_bytes());
        out.push(class_and_id);
        let mut counts = [0u8; 16];
        counts[1] = 1;
        out.extend_from_slice(&counts);
        out.push(0x00);
    }

    out.extend_from_slice(&[0xFF, 0xDA]);
    out.extend_from_slice(&(2u16 + 1 + 2 + 3).to_be_bytes());
    out.push(0x01);
    out.extend_from_slice(&[0x01, 0x00]);
    out.extend_from_slice(&[0x00, 0x3F, 0x00]);
    out
}

pub(crate) fn jpeg_with_frame_size(width: u16, height: u16) -> Vec<u8> {
    jpeg_headers(width, height)
}

fn jpeg_sof0(width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0xFF, 0xC0];
    out.extend_from_slice(&(2u16 + 6 + 3).to_be_bytes());
    out.push(0x08);
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.push(0x01);
    out.extend_from_slice(&[0x01, 0x11, 0x00]);
    out
}
