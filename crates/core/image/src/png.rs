use zune_png::zune_core::bytestream::ZCursor;
use zune_png::zune_core::colorspace::ColorSpace;
use zune_png::zune_core::options::DecoderOptions;
use zune_png::PngDecoder;

use crate::{rgba_len, Error, Result};

pub const MAX_DIMENSION: u32 = 8192;

pub fn decode(bytes: &[u8]) -> Result<crate::RgbaImage> {
    std::panic::catch_unwind(|| decode_unguarded(bytes)).unwrap_or(Err(Error::PngDecoderPanicked))
}

fn decode_unguarded(bytes: &[u8]) -> Result<crate::RgbaImage> {
    let options = DecoderOptions::default()
        .set_max_width(usize::MAX)
        .set_max_height(usize::MAX)
        .set_strict_mode(true)
        .set_use_unsafe(false)
        .png_set_confirm_crc(true)
        .png_set_strip_to_8bit(true)
        .png_set_add_alpha_channel(true);
    let mut decoder = PngDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(box_png_error)?;
    let (w, h) = decoder.dimensions().ok_or(Error::PngNoHeader)?;
    let width = u32::try_from(w).unwrap_or(u32::MAX);
    let height = u32::try_from(h).unwrap_or(u32::MAX);
    if width == 0 || height == 0 {
        return Err(Error::ZeroDimension {
            format: "png",
            width,
            height,
        });
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::PngTooLarge {
            width,
            height,
            limit: MAX_DIMENSION,
        });
    }
    let rgba_bytes = rgba_len(width, height).ok_or(Error::DimensionsOverflow {
        format: "png",
        width,
        height,
    })?;
    let channels = match decoder.colorspace() {
        Some(ColorSpace::LumaA) => 2,
        Some(ColorSpace::RGBA) => 4,
        _ => return Err(Error::PngNoHeader),
    };
    let samples = decoder.decode_raw().map_err(box_png_error)?;
    let pixel_count = rgba_bytes / crate::RGBA_BYTES;
    let needed = pixel_count.saturating_mul(channels);
    let samples = samples.get(..needed).ok_or(Error::DataTruncated {
        format: "png",
        offset: 0,
        needed,
        available: samples.len(),
    })?;
    let mut pixels = Vec::with_capacity(rgba_bytes);
    for sample in samples.chunks_exact(channels) {
        match *sample {
            [grey, alpha] => pixels.extend_from_slice(&[grey, grey, grey, alpha]),
            [r, g, b, a] => pixels.extend_from_slice(&[r, g, b, a]),
            _ => return Err(Error::PngNoHeader),
        }
    }
    Ok(crate::RgbaImage::new(width, height, pixels))
}

fn box_png_error(source: zune_png::error::PngDecodeErrors) -> Error {
    Error::Png {
        source: Box::new(source),
    }
}

#[cfg(test)]
mod tests;
