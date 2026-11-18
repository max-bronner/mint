use mint_bytes::{i32_at, u16_at, u32_at};

use crate::{rgba_len, Error, Result};

const BITMAPINFOHEADER_SIZE: u32 = 40;

pub(crate) struct Dib {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Dib> {
    if bytes.get(..4) == Some(&0x8950_4e47_u32.to_be_bytes()) {
        return Err(Error::PngIconUnsupported);
    }
    let header_size = u32_at(bytes, 0).ok_or(Error::HeaderTruncated {
        format: "dib",
        len: bytes.len(),
        needed: BITMAPINFOHEADER_SIZE as usize,
        offset: 0,
    })?;
    if header_size != BITMAPINFOHEADER_SIZE {
        return Err(Error::BadField {
            format: "dib",
            field: "biSize",
            found: header_size,
            expected: BITMAPINFOHEADER_SIZE,
        });
    }
    let width = i32_at(bytes, 4).ok_or(Error::HeaderTruncated {
        format: "dib",
        len: bytes.len(),
        needed: BITMAPINFOHEADER_SIZE as usize,
        offset: 0,
    })?;
    let combined_height = i32_at(bytes, 8).ok_or(Error::HeaderTruncated {
        format: "dib",
        len: bytes.len(),
        needed: BITMAPINFOHEADER_SIZE as usize,
        offset: 0,
    })?;
    let bit_count = u16_at(bytes, 14).ok_or(Error::HeaderTruncated {
        format: "dib",
        len: bytes.len(),
        needed: BITMAPINFOHEADER_SIZE as usize,
        offset: 0,
    })?;
    let clr_used = u32_at(bytes, 32).unwrap_or(0);

    if width <= 0
        || combined_height <= 0
        || combined_height.unsigned_abs().checked_rem(2) != Some(0)
    {
        return Err(Error::BadDimensions {
            width: width.max(0).unsigned_abs(),
            height: combined_height.max(0).unsigned_abs(),
        });
    }
    let width = width.unsigned_abs();
    let height = combined_height.unsigned_abs().checked_div(2).unwrap_or(0);
    rgba_len(width, height).ok_or(Error::BadDimensions { width, height })?;

    if !matches!(bit_count, 1 | 4 | 8 | 24 | 32) {
        return Err(Error::UnsupportedBitDepth { bits: bit_count });
    }

    let mut offset = BITMAPINFOHEADER_SIZE as usize;
    let palette = if bit_count <= 8 {
        let count = if clr_used != 0 {
            clr_used
        } else {
            1u32.checked_shl(u32::from(bit_count)).unwrap_or(0)
        };
        let table = read_palette(bytes, offset, count)?;
        offset = offset
            .checked_add(table_bytes(count))
            .ok_or(Error::BadDimensions { width, height })?;
        Some(table)
    } else {
        None
    };

    let xor_stride = row_stride(width, u32::from(bit_count));
    let xor_len = xor_stride
        .checked_mul(usize::try_from(height).map_err(|_| Error::BadDimensions { width, height })?)
        .ok_or(Error::BadDimensions { width, height })?;
    let xor = bytes
        .get(
            offset
                ..offset
                    .checked_add(xor_len)
                    .ok_or(Error::BadDimensions { width, height })?,
        )
        .ok_or(Error::DataTruncated {
            format: "dib",
            offset,
            needed: xor_len,
            available: bytes.len().saturating_sub(offset),
        })?;
    offset = offset
        .checked_add(xor_len)
        .ok_or(Error::BadDimensions { width, height })?;

    let and_stride = row_stride(width, 1);
    let and_len = and_stride
        .checked_mul(usize::try_from(height).map_err(|_| Error::BadDimensions { width, height })?)
        .ok_or(Error::BadDimensions { width, height })?;
    let and_mask = bytes.get(
        offset
            ..offset
                .checked_add(and_len)
                .ok_or(Error::BadDimensions { width, height })?,
    );

    let mut rgba =
        vec![0u8; rgba_len(width, height).ok_or(Error::BadDimensions { width, height })?];
    for row in 0..height {
        let dest_row = height.saturating_sub(1).saturating_sub(row);
        let xor_row_start = (row as usize).saturating_mul(xor_stride);
        let and_row_start = and_mask.map(|_| (row as usize).saturating_mul(and_stride));
        for col in 0..width {
            let rgb = sample_pixel(
                xor,
                xor_row_start,
                xor_stride,
                bit_count,
                col,
                palette.as_deref(),
            );
            let alpha = if bit_count == 32 {
                rgb.3
            } else if let (Some(mask), Some(mask_row_start)) = (and_mask, and_row_start) {
                if sample_bit(mask, mask_row_start, and_stride, col) {
                    0
                } else {
                    255
                }
            } else {
                255
            };
            let dest = dest_pixel_offset(width, dest_row, col)
                .ok_or(Error::BadDimensions { width, height })?;
            if let Some(slot) = rgba.get_mut(dest..dest.saturating_add(4)) {
                slot.copy_from_slice(&[rgb.0, rgb.1, rgb.2, alpha]);
            }
        }
    }

    Ok(Dib {
        width,
        height,
        rgba,
    })
}

fn dest_pixel_offset(width: u32, row: u32, col: u32) -> Option<usize> {
    let width = usize::try_from(width).ok()?;
    let row = usize::try_from(row).ok()?;
    let col = usize::try_from(col).ok()?;
    row.checked_mul(width)?.checked_add(col)?.checked_mul(4)
}

fn row_stride(width: u32, bpp: u32) -> usize {
    let bits = u64::from(width).saturating_mul(u64::from(bpp));
    let words = bits.saturating_add(31).checked_div(32).unwrap_or(u64::MAX);
    let bytes = words.saturating_mul(4);
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

type Palette = Vec<(u8, u8, u8)>;

fn table_bytes(count: u32) -> usize {
    usize::try_from(count)
        .unwrap_or(usize::MAX)
        .saturating_mul(4)
}

fn read_palette(bytes: &[u8], offset: usize, count: u32) -> Result<Palette> {
    let needed = table_bytes(count);
    let table = bytes
        .get(
            offset..offset.checked_add(needed).ok_or(Error::DataTruncated {
                format: "dib",
                offset,
                needed,
                available: bytes.len().saturating_sub(offset),
            })?,
        )
        .ok_or(Error::DataTruncated {
            format: "dib",
            offset,
            needed,
            available: bytes.len().saturating_sub(offset),
        })?;
    Ok(table
        .chunks_exact(4)
        .filter_map(|e| match e {
            [b, g, r, _reserved] => Some((*r, *g, *b)),
            _ => None,
        })
        .collect())
}

fn sample_packed(row: &[u8], bpp: u32, col: u32) -> Option<u8> {
    let bit_index = u64::from(col).saturating_mul(u64::from(bpp));
    let byte_index = usize::try_from(bit_index.checked_div(8)?).ok()?;
    let byte = *row.get(byte_index)?;
    let bit_in_byte = u32::try_from(bit_index.checked_rem(8)?).ok()?;
    match bpp {
        1 => {
            let shift = 7u32.checked_sub(bit_in_byte)?;
            Some((byte >> shift) & 0x1)
        }
        4 => {
            let shift = if bit_in_byte == 0 { 4 } else { 0 };
            Some((byte >> shift) & 0xf)
        }
        8 => Some(byte),
        _ => None,
    }
}

fn sample_bit(mask: &[u8], row_start: usize, stride: usize, col: u32) -> bool {
    let Some(row) = mask.get(row_start..row_start.saturating_add(stride)) else {
        return false;
    };
    sample_packed(row, 1, col) == Some(1)
}

fn sample_pixel(
    xor: &[u8],
    row_start: usize,
    stride: usize,
    bit_count: u16,
    col: u32,
    palette: Option<&[(u8, u8, u8)]>,
) -> (u8, u8, u8, u8) {
    let Some(row) = xor.get(row_start..row_start.saturating_add(stride)) else {
        return (0, 0, 0, 0);
    };
    match bit_count {
        1 | 4 | 8 => {
            let Some(index) = sample_packed(row, u32::from(bit_count), col) else {
                return (0, 0, 0, 0);
            };
            palette
                .and_then(|p| p.get(usize::from(index)))
                .map_or((0, 0, 0, 0), |&(r, g, b)| (r, g, b, 255))
        }
        24 => {
            let start = (col as usize).saturating_mul(3);
            match row.get(start..start.saturating_add(3)) {
                Some([b, g, r]) => (*r, *g, *b, 255),
                _ => (0, 0, 0, 0),
            }
        }
        32 => {
            let start = (col as usize).saturating_mul(4);
            match row.get(start..start.saturating_add(4)) {
                Some([b, g, r, a]) => (*r, *g, *b, *a),
                _ => (0, 0, 0, 0),
            }
        }
        _ => (0, 0, 0, 0),
    }
}
