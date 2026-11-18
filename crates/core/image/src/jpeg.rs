use crate::{rgba_len, Error, Result};

pub const MAX_DIMENSION: u16 = 8192;

pub fn decode(bytes: &[u8]) -> Result<crate::RgbaImage> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.read_info().map_err(box_jpeg_error)?;
    let info = decoder.info().ok_or(Error::JpegNoFrame)?;

    if info.width == 0 || info.height == 0 {
        return Err(Error::ZeroDimension {
            format: "jpeg",
            width: u32::from(info.width),
            height: u32::from(info.height),
        });
    }
    if info.width > MAX_DIMENSION || info.height > MAX_DIMENSION {
        return Err(Error::JpegTooLarge {
            width: info.width,
            height: info.height,
            limit: MAX_DIMENSION,
        });
    }

    let width = u32::from(info.width);
    let height = u32::from(info.height);
    let rgba_bytes = rgba_len(width, height).ok_or(Error::DimensionsOverflow {
        format: "jpeg",
        width,
        height,
    })?;

    let samples = info
        .pixel_format
        .pixel_bytes()
        .checked_mul(width as usize)
        .and_then(|row| row.checked_mul(height as usize))
        .ok_or(Error::DimensionsOverflow {
            format: "jpeg",
            width,
            height,
        })?;
    decoder.set_max_decoding_buffer_size(samples);

    let samples = decoder.decode().map_err(box_jpeg_error)?;

    let pixels = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => expand(&samples, 1, rgba_bytes, width, height)?,
        jpeg_decoder::PixelFormat::RGB24 => expand(&samples, 3, rgba_bytes, width, height)?,
        other => {
            return Err(Error::JpegUnsupportedPixelFormat {
                format: match other {
                    jpeg_decoder::PixelFormat::L16 => "16-bit greyscale",
                    jpeg_decoder::PixelFormat::CMYK32 => "CMYK",
                    _ => "an unrecognised sample layout",
                },
            })
        }
    };

    Ok(crate::RgbaImage::new(width, height, pixels))
}

fn expand(
    samples: &[u8],
    channels: usize,
    rgba_bytes: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    let needed = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or(Error::DimensionsOverflow {
            format: "jpeg",
            width,
            height,
        })?;
    if samples.len() < needed {
        return Err(Error::DataTruncated {
            format: "jpeg",
            offset: 0,
            needed,
            available: samples.len(),
        });
    }

    let mut pixels = Vec::with_capacity(rgba_bytes);
    for sample in samples.chunks_exact(channels) {
        match sample {
            [grey] => pixels.extend_from_slice(&[*grey, *grey, *grey, u8::MAX]),
            [r, g, b] => pixels.extend_from_slice(&[*r, *g, *b, u8::MAX]),
            _ => {
                return Err(Error::JpegUnsupportedPixelFormat {
                    format: "an unrecognised sample layout",
                })
            }
        }
        if pixels.len() >= rgba_bytes {
            break;
        }
    }
    if pixels.len() != rgba_bytes {
        return Err(Error::DataTruncated {
            format: "jpeg",
            offset: 0,
            needed,
            available: samples.len(),
        });
    }
    Ok(pixels)
}

fn box_jpeg_error(source: jpeg_decoder::Error) -> Error {
    Error::Jpeg {
        source: Box::new(source),
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::{decode, MAX_DIMENSION};
    use crate::synth;
    use crate::Error;

    #[test]
    fn a_baseline_jpeg_decodes_to_its_declared_size() {
        let file = synth::jpeg_grey_8x8();
        let image = decode(&file).expect("the synthetic baseline JPEG should decode");
        assert_eq!((image.width(), image.height()), (8, 8));
        assert_eq!(image.pixels().len(), 8 * 8 * 4);
        assert!(image.pixels().chunks_exact(4).all(|p| p[3] == 255));
        assert!(image
            .pixels()
            .chunks_exact(4)
            .all(|p| p[0] == p[1] && p[1] == p[2]));
    }

    #[test]
    fn a_frame_header_larger_than_the_cap_is_refused_before_anything_is_allocated() {
        let file = synth::jpeg_with_frame_size(0xFFFF, 0xFFFF);
        match decode(&file) {
            Err(Error::JpegTooLarge {
                width,
                height,
                limit,
            }) => {
                assert_eq!((width, height, limit), (0xFFFF, 0xFFFF, MAX_DIMENSION));
            }
            other => panic!("expected JpegTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn a_frame_header_at_the_cap_is_accepted_by_the_cap_itself() {
        let file = synth::jpeg_with_frame_size(MAX_DIMENSION, MAX_DIMENSION);
        assert!(!matches!(decode(&file), Err(Error::JpegTooLarge { .. })));
    }

    #[test]
    fn every_truncation_of_a_valid_jpeg_returns_rather_than_panicking() {
        let file = synth::jpeg_grey_8x8();
        for cut in 0..file.len() {
            let Some(prefix) = file.get(..cut) else {
                continue;
            };
            let _ = decode(prefix);
        }
    }

    #[test]
    fn every_single_byte_corruption_of_a_valid_jpeg_returns_rather_than_panicking() {
        let file = synth::jpeg_grey_8x8();
        for index in 0..file.len() {
            for mask in [0x01u8, 0x40, 0x80, 0xFF] {
                let mut corrupt = file.clone();
                corrupt[index] ^= mask;
                let _ = decode(&corrupt);
            }
        }
    }

    #[test]
    fn something_that_is_not_a_jpeg_at_all_is_an_error() {
        assert!(decode(b"").is_err());
        assert!(decode(b"DDS ").is_err());
        assert!(decode(&[0xFF, 0xD8]).is_err());
    }
}
