use mint_bytes::{array_at, u32_at};

use crate::{expand5, expand6, rgba_len, Error, Result, RgbaImage, RGBA_BYTES};

const FORMAT: &str = "dds";

const TOTAL_HEADER_LEN: usize = 128;

const HEADER_SIZE: u32 = 124;

const PIXEL_FORMAT_SIZE: u32 = 32;

const DDPF_FOURCC: u32 = 0x4;

const DDSCAPS2_CUBEMAP: u32 = 0x0000_0200;

const DDSCAPS2_VOLUME: u32 = 0x0020_0000;

const BLOCK_EDGE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFormat {
    Bc1,
    Bc3,
}

impl BlockFormat {
    #[must_use]
    pub fn bytes_per_block(self) -> usize {
        match self {
            Self::Bc1 => 8,
            Self::Bc3 => 16,
        }
    }

    #[must_use]
    pub fn four_cc(self) -> [u8; 4] {
        match self {
            Self::Bc1 => *b"DXT1",
            Self::Bc3 => *b"DXT5",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipLevel {
    width: u32,
    height: u32,
    blocks: Vec<u8>,
}

impl MipLevel {
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn blocks(&self) -> &[u8] {
        &self.blocks
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dds {
    format: BlockFormat,
    width: u32,
    height: u32,
    declared_linear_size: u32,
    levels: Vec<MipLevel>,
}

impl Dds {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let truncated = || Error::HeaderTruncated {
            format: FORMAT,
            len: bytes.len(),
            needed: TOTAL_HEADER_LEN,
        };
        if bytes.len() < TOTAL_HEADER_LEN {
            return Err(truncated());
        }

        let magic = array_at::<4>(bytes, 0).ok_or_else(truncated)?;
        if &magic != b"DDS " {
            return Err(Error::DdsNotDds { magic });
        }

        let header_size = u32_at(bytes, 4).ok_or_else(truncated)?;
        if header_size != HEADER_SIZE {
            return Err(Error::DdsBadStructureSize {
                structure: "header",
                found: header_size,
                expected: HEADER_SIZE,
            });
        }
        let pixel_format_size = u32_at(bytes, 76).ok_or_else(truncated)?;
        if pixel_format_size != PIXEL_FORMAT_SIZE {
            return Err(Error::DdsBadStructureSize {
                structure: "pixel format",
                found: pixel_format_size,
                expected: PIXEL_FORMAT_SIZE,
            });
        }

        let pixel_format_flags = u32_at(bytes, 80).ok_or_else(truncated)?;
        if pixel_format_flags & DDPF_FOURCC == 0 {
            return Err(Error::DdsNotBlockCompressed {
                flags: pixel_format_flags,
                bit_count: u32_at(bytes, 88).ok_or_else(truncated)?,
            });
        }
        let four_cc = array_at::<4>(bytes, 84).ok_or_else(truncated)?;
        let format = match &four_cc {
            b"DXT1" => BlockFormat::Bc1,
            b"DXT5" => BlockFormat::Bc3,
            other => {
                return Err(Error::DdsUnsupportedFourCc {
                    name: String::from_utf8_lossy(other).escape_default().to_string(),
                    fourcc: four_cc,
                })
            }
        };

        let height = u32_at(bytes, 12).ok_or_else(truncated)?;
        let width = u32_at(bytes, 16).ok_or_else(truncated)?;
        if width == 0 || height == 0 {
            return Err(Error::ZeroDimension {
                format: FORMAT,
                width,
                height,
            });
        }

        let depth = u32_at(bytes, 24).ok_or_else(truncated)?;
        let caps2 = u32_at(bytes, 112).ok_or_else(truncated)?;
        if caps2 & DDSCAPS2_CUBEMAP != 0 {
            return Err(Error::DdsUnsupportedLayout {
                layout: "cube map",
                caps2,
                depth,
            });
        }
        if caps2 & DDSCAPS2_VOLUME != 0 || depth > 1 {
            return Err(Error::DdsUnsupportedLayout {
                layout: "volume",
                caps2,
                depth,
            });
        }

        let declared_levels = u32_at(bytes, 28).ok_or_else(truncated)?.max(1);
        let possible_levels = max_levels(width, height);
        if declared_levels > possible_levels {
            return Err(Error::DdsMipCountImplausible {
                declared: declared_levels,
                width,
                height,
                possible: possible_levels,
            });
        }

        let payload = bytes.get(TOTAL_HEADER_LEN..).unwrap_or_default();
        let mut levels = Vec::new();
        let mut consumed = 0usize;
        for index in 0..declared_levels {
            let level_width = shrink(width, index);
            let level_height = shrink(height, index);
            let size =
                level_size(level_width, level_height, format).ok_or(Error::DimensionsOverflow {
                    format: FORMAT,
                    width: level_width,
                    height: level_height,
                })?;
            let end = consumed
                .checked_add(size)
                .ok_or(Error::DimensionsOverflow {
                    format: FORMAT,
                    width: level_width,
                    height: level_height,
                })?;
            let blocks = payload.get(consumed..end).ok_or(Error::DataTruncated {
                format: FORMAT,
                offset: TOTAL_HEADER_LEN.saturating_add(consumed),
                needed: size,
                available: payload.len().saturating_sub(consumed),
            })?;
            levels.push(MipLevel {
                width: level_width,
                height: level_height,
                blocks: blocks.to_vec(),
            });
            consumed = end;
        }

        Ok(Self {
            format,
            width,
            height,
            declared_linear_size: u32_at(bytes, 20).ok_or_else(truncated)?,
            levels,
        })
    }

    #[must_use]
    pub fn format(&self) -> BlockFormat {
        self.format
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn levels(&self) -> &[MipLevel] {
        &self.levels
    }

    #[must_use]
    pub fn level(&self, index: usize) -> Option<&MipLevel> {
        self.levels.get(index)
    }

    #[must_use]
    pub fn declared_linear_size(&self) -> u32 {
        self.declared_linear_size
    }

    #[must_use]
    pub fn computed_linear_size(&self) -> usize {
        level_size(self.width, self.height, self.format).unwrap_or(0)
    }

    pub fn decode(&self) -> Result<RgbaImage> {
        self.decode_level(0)
    }

    pub fn decode_level(&self, index: usize) -> Result<RgbaImage> {
        let level = self.levels.get(index).ok_or(Error::DdsNoSuchLevel {
            level: index,
            levels: self.levels.len(),
        })?;

        let width = level.width;
        let height = level.height;
        let out_len = rgba_len(width, height).ok_or(Error::DimensionsOverflow {
            format: FORMAT,
            width,
            height,
        })?;
        let row_bytes = usize::try_from(width)
            .ok()
            .and_then(|w| w.checked_mul(RGBA_BYTES))
            .ok_or(Error::DimensionsOverflow {
                format: FORMAT,
                width,
                height,
            })?;

        let blocks_across = width.div_ceil(BLOCK_EDGE);
        let mut pixels = vec![0u8; out_len];

        for (position, block) in level
            .blocks
            .chunks_exact(self.format.bytes_per_block())
            .enumerate()
        {
            let block_x = u32::try_from(position)
                .ok()
                .and_then(|p| p.checked_rem(blocks_across))
                .unwrap_or(0);
            let block_y = u32::try_from(position)
                .ok()
                .and_then(|p| p.checked_div(blocks_across))
                .unwrap_or(0);

            let decoded = match self.format {
                BlockFormat::Bc1 => decode_bc1_block(block),
                BlockFormat::Bc3 => decode_bc3_block(block),
            };

            write_block(
                &mut pixels,
                &decoded,
                block_x,
                block_y,
                width,
                height,
                row_bytes,
            );
        }

        Ok(RgbaImage::new(width, height, pixels))
    }
}

fn max_levels(width: u32, height: u32) -> u32 {
    width
        .max(height)
        .checked_ilog2()
        .unwrap_or(0)
        .saturating_add(1)
}

fn shrink(value: u32, index: u32) -> u32 {
    value.checked_shr(index).unwrap_or(0).max(1)
}

fn level_size(width: u32, height: u32, format: BlockFormat) -> Option<usize> {
    let across = usize::try_from(width.div_ceil(BLOCK_EDGE)).ok()?;
    let down = usize::try_from(height.div_ceil(BLOCK_EDGE)).ok()?;
    across
        .checked_mul(down)?
        .checked_mul(format.bytes_per_block())
}

fn write_block(
    pixels: &mut [u8],
    block: &[[u8; 4]; 16],
    block_x: u32,
    block_y: u32,
    width: u32,
    height: u32,
    row_bytes: usize,
) {
    for row in 0..BLOCK_EDGE {
        let y = match block_y
            .checked_mul(BLOCK_EDGE)
            .and_then(|top| top.checked_add(row))
        {
            Some(y) if y < height => y,
            _ => return,
        };
        let x0 = match block_x.checked_mul(BLOCK_EDGE) {
            Some(x0) if x0 < width => x0,
            _ => return,
        };
        let span = width.saturating_sub(x0).min(BLOCK_EDGE);

        let mut row_bytes_buffer = [0u8; 16];
        for column in 0..span {
            let source = block.get(
                usize::try_from(row.saturating_mul(BLOCK_EDGE).saturating_add(column)).unwrap_or(0),
            );
            let Some(pixel) = source else { continue };
            let start = match usize::try_from(column)
                .ok()
                .and_then(|c| c.checked_mul(RGBA_BYTES))
            {
                Some(start) => start,
                None => continue,
            };
            if let Some(slot) = row_bytes_buffer.get_mut(start..start.saturating_add(RGBA_BYTES)) {
                slot.copy_from_slice(pixel);
            }
        }

        let copied = usize::try_from(span)
            .ok()
            .and_then(|s| s.checked_mul(RGBA_BYTES))
            .unwrap_or(0);
        let start = match usize::try_from(y)
            .ok()
            .and_then(|y| y.checked_mul(row_bytes))
            .and_then(|row_start| {
                usize::try_from(x0)
                    .ok()
                    .and_then(|x| x.checked_mul(RGBA_BYTES))
                    .and_then(|column_start| row_start.checked_add(column_start))
            }) {
            Some(start) => start,
            None => return,
        };
        let Some(end) = start.checked_add(copied) else {
            return;
        };
        if let (Some(destination), Some(source)) =
            (pixels.get_mut(start..end), row_bytes_buffer.get(..copied))
        {
            destination.copy_from_slice(source);
        }
    }
}

fn unpack565(value: u16) -> [u8; 3] {
    [
        expand5(value.checked_shr(11).unwrap_or(0)),
        expand6(value.checked_shr(5).unwrap_or(0)),
        expand5(value),
    ]
}

fn blend_two_one(a: u8, b: u8) -> u8 {
    let sum = u16::from(a)
        .checked_mul(2)
        .and_then(|scaled| scaled.checked_add(u16::from(b)))
        .unwrap_or(0);
    u8::try_from(sum.checked_div(3).unwrap_or(0)).unwrap_or(u8::MAX)
}

fn blend_half(a: u8, b: u8) -> u8 {
    let sum = u16::from(a).checked_add(u16::from(b)).unwrap_or(0);
    u8::try_from(sum.checked_div(2).unwrap_or(0)).unwrap_or(u8::MAX)
}

fn blend_ratio(a: u8, b: u8, numerator_a: u16, numerator_b: u16, denominator: u16) -> u8 {
    let sum = u16::from(a)
        .checked_mul(numerator_a)
        .and_then(|left| {
            u16::from(b)
                .checked_mul(numerator_b)
                .and_then(|right| left.checked_add(right))
        })
        .unwrap_or(0);
    u8::try_from(sum.checked_div(denominator).unwrap_or(0)).unwrap_or(u8::MAX)
}

fn decode_colour_block(block: &[u8], punch_through: bool) -> [[u8; 4]; 16] {
    let mut out = [[0u8, 0, 0, u8::MAX]; 16];
    let Some(&[c0lo, c0hi, c1lo, c1hi, i0, i1, i2, i3, ..]) = block.get(..8) else {
        return out;
    };
    let raw0 = u16::from_le_bytes([c0lo, c0hi]);
    let raw1 = u16::from_le_bytes([c1lo, c1hi]);
    let [r0, g0, b0] = unpack565(raw0);
    let [r1, g1, b1] = unpack565(raw1);

    let three_colour = punch_through && raw0 <= raw1;
    let (c2, c3) = if three_colour {
        (
            [
                blend_half(r0, r1),
                blend_half(g0, g1),
                blend_half(b0, b1),
                u8::MAX,
            ],
            [0, 0, 0, 0],
        )
    } else {
        (
            [
                blend_two_one(r0, r1),
                blend_two_one(g0, g1),
                blend_two_one(b0, b1),
                u8::MAX,
            ],
            [
                blend_two_one(r1, r0),
                blend_two_one(g1, g0),
                blend_two_one(b1, b0),
                u8::MAX,
            ],
        )
    };
    let c0 = [r0, g0, b0, u8::MAX];
    let c1 = [r1, g1, b1, u8::MAX];

    let mut indices = u32::from_le_bytes([i0, i1, i2, i3]);
    for slot in out.iter_mut() {
        *slot = match indices & 0b11 {
            0 => c0,
            1 => c1,
            2 => c2,
            _ => c3,
        };
        indices = indices.checked_shr(2).unwrap_or(0);
    }
    out
}

fn alpha_palette(alpha0: u8, alpha1: u8) -> [u8; 8] {
    if alpha0 > alpha1 {
        [
            alpha0,
            alpha1,
            blend_ratio(alpha0, alpha1, 6, 1, 7),
            blend_ratio(alpha0, alpha1, 5, 2, 7),
            blend_ratio(alpha0, alpha1, 4, 3, 7),
            blend_ratio(alpha0, alpha1, 3, 4, 7),
            blend_ratio(alpha0, alpha1, 2, 5, 7),
            blend_ratio(alpha0, alpha1, 1, 6, 7),
        ]
    } else {
        [
            alpha0,
            alpha1,
            blend_ratio(alpha0, alpha1, 4, 1, 5),
            blend_ratio(alpha0, alpha1, 3, 2, 5),
            blend_ratio(alpha0, alpha1, 2, 3, 5),
            blend_ratio(alpha0, alpha1, 1, 4, 5),
            0,
            u8::MAX,
        ]
    }
}

fn decode_bc1_block(block: &[u8]) -> [[u8; 4]; 16] {
    decode_colour_block(block, true)
}

fn decode_bc3_block(block: &[u8]) -> [[u8; 4]; 16] {
    let mut out = decode_colour_block(block.get(8..).unwrap_or_default(), false);
    let Some(&[alpha0, alpha1, a2, a3, a4, a5, a6, a7, ..]) = block.get(..8) else {
        return out;
    };
    let palette = alpha_palette(alpha0, alpha1);
    let mut indices = u64::from_le_bytes([a2, a3, a4, a5, a6, a7, 0, 0]);
    for slot in out.iter_mut() {
        let selected = match indices & 0b111 {
            0 => palette.first(),
            1 => palette.get(1),
            2 => palette.get(2),
            3 => palette.get(3),
            4 => palette.get(4),
            5 => palette.get(5),
            6 => palette.get(6),
            _ => palette.get(7),
        };
        if let (Some(alpha), Some(channel)) = (selected, slot.get_mut(3)) {
            *channel = *alpha;
        }
        indices = indices.checked_shr(3).unwrap_or(0);
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]
mod tests;
