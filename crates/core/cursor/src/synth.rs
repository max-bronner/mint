#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::wrong_self_convention
)]

pub(crate) struct CurBuilder {
    width: u32,
    height: u32,
    bpp: u16,
    kind: u16,
    hotspot: (u16, u16),
    palette: Vec<(u8, u8, u8)>,
    xor_bits: Vec<u8>,
    and_bits: Vec<u8>,
}

impl CurBuilder {
    pub(crate) fn new(width: u32, height: u32, bpp: u16) -> Self {
        Self {
            width,
            height,
            bpp,
            kind: 2,
            hotspot: (0, 0),
            palette: Vec::new(),
            xor_bits: Vec::new(),
            and_bits: Vec::new(),
        }
    }

    pub(crate) fn as_icon(mut self) -> Self {
        self.kind = 1;
        self
    }

    pub(crate) fn hotspot(mut self, x: u16, y: u16) -> Self {
        self.hotspot = (x, y);
        self
    }

    pub(crate) fn palette(mut self, entries: &[(u8, u8, u8)]) -> Self {
        self.palette = entries.to_vec();
        self
    }

    pub(crate) fn xor_bits(mut self, rows: &[u8]) -> Self {
        self.xor_bits = rows.to_vec();
        self
    }

    pub(crate) fn and_bits(mut self, rows: &[u8]) -> Self {
        self.and_bits = rows.to_vec();
        self
    }

    fn unpadded_row_len(width: u32, bpp: u32) -> usize {
        (u64::from(width) * u64::from(bpp)).div_ceil(8) as usize
    }

    fn stride(width: u32, bpp: u32) -> usize {
        ((u64::from(width) * u64::from(bpp)).div_ceil(32) * 4) as usize
    }

    fn pack_rows(flat: &[u8], unpadded_row: usize, stride: usize, height: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(stride * height as usize);
        let rows: Vec<&[u8]> = flat.chunks(unpadded_row).collect();
        for row in rows.iter().rev() {
            let mut padded = vec![0u8; stride];
            padded[..row.len().min(stride)].copy_from_slice(&row[..row.len().min(stride)]);
            out.extend_from_slice(&padded);
        }
        out
    }

    pub(crate) fn build(self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());

        let dib_offset = 6 + 16;
        let palette_bytes = if self.bpp <= 8 {
            self.palette.len() * 4
        } else {
            0
        };
        let xor_stride = Self::stride(self.width, u32::from(self.bpp));
        let and_stride = Self::stride(self.width, 1);
        let xor_len = xor_stride * self.height as usize;
        let and_len = and_stride * self.height as usize;
        let dib_len = 40 + palette_bytes + xor_len + and_len;

        out.push(u8::try_from(self.width).unwrap_or(0));
        out.push(u8::try_from(self.height).unwrap_or(0));
        out.push(0);
        out.push(0);
        if self.kind == 2 {
            out.extend_from_slice(&self.hotspot.0.to_le_bytes());
            out.extend_from_slice(&self.hotspot.1.to_le_bytes());
        } else {
            out.extend_from_slice(&1u16.to_le_bytes());
            out.extend_from_slice(&self.bpp.to_le_bytes());
        }
        out.extend_from_slice(&(dib_len as u32).to_le_bytes());
        out.extend_from_slice(&(dib_offset as u32).to_le_bytes());

        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(self.width as i32).to_le_bytes());
        out.extend_from_slice(&((self.height as i32) * 2).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&self.bpp.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&(self.palette.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        for &(r, g, b) in &self.palette {
            out.extend_from_slice(&[b, g, r, 0]);
        }

        let xor_unpadded = Self::unpadded_row_len(self.width, u32::from(self.bpp));
        out.extend_from_slice(&Self::pack_rows(
            &self.xor_bits,
            xor_unpadded,
            xor_stride,
            self.height,
        ));
        let and_unpadded = Self::unpadded_row_len(self.width, 1);
        out.extend_from_slice(&Self::pack_rows(
            &self.and_bits,
            and_unpadded,
            and_stride,
            self.height,
        ));

        out
    }
}

pub(crate) struct AniBuilder {
    disp_rate: u32,
    frames: Vec<Vec<u8>>,
    extra_list: Option<(Vec<u8>, Vec<u8>)>,
}

impl AniBuilder {
    pub(crate) fn new(disp_rate: u32) -> Self {
        Self {
            disp_rate,
            frames: Vec::new(),
            extra_list: None,
        }
    }

    pub(crate) fn frame_1bpp(
        mut self,
        palette: &[(u8, u8, u8)],
        xor_bits: &[u8],
        and_bits: &[u8],
    ) -> Self {
        let frame = CurBuilder::new(1, 1, 1)
            .palette(palette)
            .xor_bits(xor_bits)
            .and_bits(and_bits)
            .build();
        self.frames.push(frame);
        self
    }

    pub(crate) fn with_watermark_list(mut self, list_type: &[u8; 4], text: &[u8]) -> Self {
        let mut data = list_type.to_vec();
        data.extend_from_slice(text);
        self.extra_list = Some((b"LIST".to_vec(), data));
        self
    }

    fn chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(id);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
    }

    pub(crate) fn build(self) -> Vec<u8> {
        let mut anih = Vec::new();
        let count = self.frames.len() as u32;
        anih.extend_from_slice(&36u32.to_le_bytes());
        anih.extend_from_slice(&count.to_le_bytes());
        anih.extend_from_slice(&count.to_le_bytes());
        anih.extend_from_slice(&0u32.to_le_bytes());
        anih.extend_from_slice(&0u32.to_le_bytes());
        anih.extend_from_slice(&0u32.to_le_bytes());
        anih.extend_from_slice(&0u32.to_le_bytes());
        anih.extend_from_slice(&self.disp_rate.to_le_bytes());
        anih.extend_from_slice(&1u32.to_le_bytes());

        let mut fram = Vec::new();
        for frame in &self.frames {
            Self::chunk(&mut fram, b"icon", frame);
        }

        let mut body = Vec::new();
        Self::chunk(&mut body, b"anih", &anih);
        let mut list = b"fram".to_vec();
        list.extend_from_slice(&fram);
        Self::chunk(&mut body, b"LIST", &list);
        if let Some((_, data)) = &self.extra_list {
            Self::chunk(&mut body, b"LIST", data);
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(b"ACON");
        out.extend_from_slice(&body);
        let whole_len = out.len() as u32;
        out[4..8].copy_from_slice(&whole_len.to_le_bytes());
        out
    }
}
