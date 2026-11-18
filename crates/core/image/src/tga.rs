use mint_bytes::u16_at;

use crate::{expand5, rgba_len, Error, Result, RgbaImage, RGBA_BYTES};

const FORMAT: &str = "tga";

const HEADER_LEN: usize = 18;

const TYPE_TRUE_COLOR: u8 = 2;

const TYPE_TRUE_COLOR_RLE: u8 = 10;

const TYPE_GREYSCALE: u8 = 3;

const TYPE_GREYSCALE_RLE: u8 = 11;

const DESCRIPTOR_TOP_ORIGIN: u8 = 0x20;

const DESCRIPTOR_RIGHT_ORIGIN: u8 = 0x10;

const DESCRIPTOR_ATTRIBUTE_BITS: u8 = 0x0f;

const MAX_RUN: usize = 128;

const MAX_RLE_RESERVE: usize = 1 << 20;

pub const MAX_DIMENSION: u16 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub width: u16,
    pub height: u16,
    pub bits_per_pixel: u8,
    pub image_type: u8,
    pub top_origin: bool,
    pub right_origin: bool,
    pub attribute_bits: u8,
    pub data_offset: usize,
}

pub fn info(bytes: &[u8]) -> Result<Info> {
    Header::parse(bytes).map(|header| header.info)
}

pub fn decode(bytes: &[u8]) -> Result<RgbaImage> {
    let header = Header::parse(bytes)?;
    let Info { width, height, .. } = header.info;

    let pixel_count =
        usize::from(width)
            .checked_mul(usize::from(height))
            .ok_or(Error::DimensionsOverflow {
                format: FORMAT,
                width: u32::from(width),
                height: u32::from(height),
            })?;
    let decoded_len =
        rgba_len(u32::from(width), u32::from(height)).ok_or(Error::DimensionsOverflow {
            format: FORMAT,
            width: u32::from(width),
            height: u32::from(height),
        })?;

    let stride = header.kind.stride();
    let needed = pixel_count
        .checked_mul(stride)
        .ok_or(Error::DimensionsOverflow {
            format: FORMAT,
            width: u32::from(width),
            height: u32::from(height),
        })?;

    let payload = bytes
        .get(header.info.data_offset..)
        .ok_or(Error::DataTruncated {
            format: FORMAT,
            offset: header.info.data_offset,
            needed,
            available: 0,
        })?;

    let pixels = if header.run_length_encoded {
        decode_rle(
            payload,
            header.info.data_offset,
            header.kind,
            pixel_count,
            decoded_len,
        )?
    } else {
        decode_uncompressed(
            payload,
            header.info.data_offset,
            header.kind,
            needed,
            decoded_len,
        )?
    };

    let pixels = orient(pixels, &header.info);
    Ok(RgbaImage::new(u32::from(width), u32::from(height), pixels))
}

struct Header {
    info: Info,
    kind: PixelKind,
    run_length_encoded: bool,
}

impl Header {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let truncated = || Error::HeaderTruncated {
            format: FORMAT,
            len: bytes.len(),
            needed: HEADER_LEN,
        };

        let id_length = bytes.first().copied().ok_or_else(truncated)?;
        let colour_map_type = bytes.get(1).copied().ok_or_else(truncated)?;
        let image_type = bytes.get(2).copied().ok_or_else(truncated)?;
        let map_length = u16_at(bytes, 5).ok_or_else(truncated)?;
        let map_entry_bits = bytes.get(7).copied().ok_or_else(truncated)?;
        let width = u16_at(bytes, 12).ok_or_else(truncated)?;
        let height = u16_at(bytes, 14).ok_or_else(truncated)?;
        let bits_per_pixel = bytes.get(16).copied().ok_or_else(truncated)?;
        let descriptor = bytes.get(17).copied().ok_or_else(truncated)?;

        let run_length_encoded =
            image_type == TYPE_TRUE_COLOR_RLE || image_type == TYPE_GREYSCALE_RLE;
        let kind = match (image_type, bits_per_pixel) {
            (TYPE_TRUE_COLOR | TYPE_TRUE_COLOR_RLE, 16) => PixelKind::Bgra5551 {
                alpha_bit: descriptor & DESCRIPTOR_ATTRIBUTE_BITS != 0,
            },
            (TYPE_TRUE_COLOR | TYPE_TRUE_COLOR_RLE, 24) => PixelKind::Bgr24,
            (TYPE_TRUE_COLOR | TYPE_TRUE_COLOR_RLE, 32) => PixelKind::Bgra32,
            (TYPE_GREYSCALE | TYPE_GREYSCALE_RLE, 8) => PixelKind::Grey8,
            (TYPE_TRUE_COLOR | TYPE_TRUE_COLOR_RLE | TYPE_GREYSCALE | TYPE_GREYSCALE_RLE, bits) => {
                return Err(Error::TgaUnsupportedDepth { bits })
            }
            _ => return Err(Error::TgaUnsupportedImageType { image_type }),
        };
        if width == 0 || height == 0 {
            return Err(Error::ZeroDimension {
                format: FORMAT,
                width: u32::from(width),
                height: u32::from(height),
            });
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(Error::TgaTooLarge {
                width,
                height,
                limit: MAX_DIMENSION,
            });
        }

        let map_bytes = if colour_map_type == 0 {
            0
        } else {
            let entry_bytes = usize::from(map_entry_bits).saturating_add(7) / 8;
            usize::from(map_length).saturating_mul(entry_bytes)
        };
        let data_offset = HEADER_LEN
            .saturating_add(usize::from(id_length))
            .saturating_add(map_bytes);

        Ok(Self {
            info: Info {
                width,
                height,
                bits_per_pixel,
                image_type,
                top_origin: descriptor & DESCRIPTOR_TOP_ORIGIN != 0,
                right_origin: descriptor & DESCRIPTOR_RIGHT_ORIGIN != 0,
                attribute_bits: descriptor & DESCRIPTOR_ATTRIBUTE_BITS,
                data_offset,
            },
            kind,
            run_length_encoded,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PixelKind {
    Grey8,
    Bgra5551 { alpha_bit: bool },
    Bgr24,
    Bgra32,
}

impl PixelKind {
    fn stride(self) -> usize {
        match self {
            Self::Grey8 => 1,
            Self::Bgra5551 { .. } => 2,
            Self::Bgr24 => 3,
            Self::Bgra32 => 4,
        }
    }

    fn to_rgba(self, chunk: &[u8]) -> [u8; 4] {
        match (self, chunk) {
            (Self::Grey8, [value]) => [*value, *value, *value, u8::MAX],
            (Self::Bgra5551 { alpha_bit }, [low, high]) => {
                let packed = u16::from_le_bytes([*low, *high]);
                let alpha = if alpha_bit && packed & 0x8000 == 0 {
                    0
                } else {
                    u8::MAX
                };
                [
                    expand5(packed.checked_shr(10).unwrap_or(0)),
                    expand5(packed.checked_shr(5).unwrap_or(0)),
                    expand5(packed),
                    alpha,
                ]
            }
            (Self::Bgr24, [b, g, r]) => [*r, *g, *b, u8::MAX],
            (Self::Bgra32, [b, g, r, a]) => [*r, *g, *b, *a],
            _ => [0, 0, 0, u8::MAX],
        }
    }
}

fn decode_uncompressed(
    payload: &[u8],
    offset: usize,
    kind: PixelKind,
    needed: usize,
    decoded_len: usize,
) -> Result<Vec<u8>> {
    let stride = kind.stride();
    if payload.len() < needed {
        return Err(Error::DataTruncated {
            format: FORMAT,
            offset,
            needed,
            available: payload.len(),
        });
    }

    let mut out = Vec::with_capacity(decoded_len);
    let source = payload.get(..needed).unwrap_or_default();
    for chunk in source.chunks_exact(stride) {
        out.extend_from_slice(&kind.to_rgba(chunk));
    }
    Ok(out)
}

fn decode_rle(
    payload: &[u8],
    offset: usize,
    kind: PixelKind,
    pixel_count: usize,
    decoded_len: usize,
) -> Result<Vec<u8>> {
    let stride = kind.stride();
    let packet_cost = stride.saturating_add(1);
    let max_pixels = payload
        .len()
        .checked_div(packet_cost)
        .unwrap_or(0)
        .saturating_mul(MAX_RUN);
    if pixel_count > max_pixels {
        return Err(Error::DataTruncated {
            format: FORMAT,
            offset,
            needed: pixel_count.div_ceil(MAX_RUN).saturating_mul(packet_cost),
            available: payload.len(),
        });
    }

    let mut out = Vec::with_capacity(decoded_len.min(MAX_RLE_RESERVE));
    let mut position = 0usize;
    let mut written = 0usize;

    while written < pixel_count {
        let short = |need: usize, at: usize| Error::DataTruncated {
            format: FORMAT,
            offset: offset.saturating_add(at),
            needed: need,
            available: payload.len().saturating_sub(at),
        };

        let packet_at = position;
        let packet = payload
            .get(packet_at)
            .copied()
            .ok_or_else(|| short(1, packet_at))?;
        let run = usize::from(packet & 0x7f).saturating_add(1);
        let repeated = packet & 0x80 != 0;
        position = packet_at.saturating_add(1);

        let after = written.saturating_add(run);
        if after > pixel_count {
            return Err(Error::TgaRleOverrun {
                offset: offset.saturating_add(packet_at),
                pixels: after.saturating_sub(pixel_count),
                declared: pixel_count,
            });
        }

        let span = if repeated {
            stride
        } else {
            run.saturating_mul(stride)
        };
        let end = position
            .checked_add(span)
            .ok_or_else(|| short(span, position))?;
        let chunk = payload
            .get(position..end)
            .ok_or_else(|| short(span, position))?;

        if repeated {
            let pixel = kind.to_rgba(chunk);
            for _ in 0..run {
                out.extend_from_slice(&pixel);
            }
        } else {
            for source in chunk.chunks_exact(stride) {
                out.extend_from_slice(&kind.to_rgba(source));
            }
        }

        position = end;
        written = after;
    }

    Ok(out)
}

fn orient(pixels: Vec<u8>, info: &Info) -> Vec<u8> {
    if info.top_origin && !info.right_origin {
        return pixels;
    }
    let row_bytes = usize::from(info.width).saturating_mul(RGBA_BYTES);
    if row_bytes == 0 {
        return pixels;
    }

    let mut rows: Vec<&[u8]> = pixels.chunks_exact(row_bytes).collect();
    if !info.top_origin {
        rows.reverse();
    }

    let mut out = Vec::with_capacity(pixels.len());
    for row in rows {
        if info.right_origin {
            for pixel in row.chunks_exact(RGBA_BYTES).rev() {
                out.extend_from_slice(pixel);
            }
        } else {
            out.extend_from_slice(row);
        }
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
