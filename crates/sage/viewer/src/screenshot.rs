use std::path::Path;

use crate::{Error, Result};

const HEADER_LEN: usize = 18;

const TYPE_TRUE_COLOR: u8 = 2;

const DESCRIPTOR_TOP_ORIGIN_RGBA: u8 = 0x28;

pub fn encode_tga(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let expected = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(usize::try_from(height).ok()?))
        .and_then(|pixels| pixels.checked_mul(4));
    if expected != Some(rgba.len()) || width == 0 || height == 0 {
        return Err(Error::ScreenshotSize {
            width,
            height,
            len: rgba.len(),
        });
    }
    let (w16, h16) = match (u16::try_from(width), u16::try_from(height)) {
        (Ok(w), Ok(h)) => (w, h),
        _ => {
            return Err(Error::ScreenshotSize {
                width,
                height,
                len: rgba.len(),
            })
        }
    };

    let mut out = Vec::with_capacity(HEADER_LEN.saturating_add(rgba.len()));
    out.push(0);
    out.push(0);
    out.push(TYPE_TRUE_COLOR);
    out.extend_from_slice(&[0; 5]);
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&w16.to_le_bytes());
    out.extend_from_slice(&h16.to_le_bytes());
    out.push(32);
    out.push(DESCRIPTOR_TOP_ORIGIN_RGBA);

    for pixel in rgba.chunks_exact(4) {
        match pixel {
            [r, g, b, a] => out.extend_from_slice(&[*b, *g, *r, *a]),
            _ => {
                return Err(Error::ScreenshotSize {
                    width,
                    height,
                    len: rgba.len(),
                })
            }
        }
    }
    Ok(out)
}

#[must_use]
pub fn first_difference(encoded: &[u8], rgba: &[u8]) -> Option<usize> {
    let Ok(decoded) = mint_image::tga::decode(encoded) else {
        return Some(0);
    };
    if decoded.pixels().len() != rgba.len() {
        return Some(decoded.pixels().len().min(rgba.len()));
    }
    decoded
        .pixels()
        .iter()
        .zip(rgba.iter())
        .position(|(a, b)| a != b)
}

pub fn write_tga(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    let bytes = encode_tga(width, height, rgba)?;
    if let Some(at) = first_difference(&bytes, rgba) {
        return Err(Error::ScreenshotMismatch { at });
    }
    std::fs::write(path, bytes).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    fn asymmetric(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let left = x * 2 < width;
                let top = y * 2 < height;
                out.extend_from_slice(match (top, left) {
                    (true, true) => &[255, 0, 0, 255],
                    (true, false) => &[0, 255, 0, 255],
                    (false, true) => &[0, 0, 255, 255],
                    (false, false) => &[255, 255, 0, 255],
                });
            }
        }
        out
    }

    #[test]
    fn the_output_round_trips_through_mint_images_decoder_unchanged() {
        let (width, height) = (8u32, 6u32);
        let original = asymmetric(width, height);
        let encoded = encode_tga(width, height, &original).expect("encode");
        let decoded = mint_image::tga::decode(&encoded).expect("decode");

        assert_eq!(decoded.width(), width);
        assert_eq!(decoded.height(), height);
        assert_eq!(
            decoded.pixels(),
            original.as_slice(),
            "every pixel must survive the round trip, in RGBA order"
        );
    }

    #[test]
    fn the_corners_stay_in_their_corners() {
        let (width, height) = (8u32, 6u32);
        let encoded = encode_tga(width, height, &asymmetric(width, height)).expect("encode");
        let decoded = mint_image::tga::decode(&encoded).expect("decode");

        assert_eq!(decoded.pixel(0, 0), Some([255, 0, 0, 255]), "top-left");
        assert_eq!(
            decoded.pixel(width - 1, 0),
            Some([0, 255, 0, 255]),
            "top-right"
        );
        assert_eq!(
            decoded.pixel(0, height - 1),
            Some([0, 0, 255, 255]),
            "bottom-left"
        );
        assert_eq!(
            decoded.pixel(width - 1, height - 1),
            Some([255, 255, 0, 255]),
            "bottom-right"
        );
    }

    #[test]
    fn the_header_says_what_the_decoder_needs_it_to_say() {
        let encoded = encode_tga(2, 2, &[0u8; 16]).expect("encode");
        let info = mint_image::tga::info(&encoded).expect("header");

        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert_eq!(info.bits_per_pixel, 32);
        assert_eq!(info.image_type, TYPE_TRUE_COLOR);
        assert!(info.top_origin, "rows are written top first");
        assert!(!info.right_origin);
        assert_eq!(info.attribute_bits, 8);
        assert_eq!(info.data_offset, HEADER_LEN);
    }

    #[test]
    fn blue_and_red_are_actually_swapped_on_disc() {
        let encoded = encode_tga(1, 1, &[10, 20, 30, 40]).expect("encode");
        assert_eq!(&encoded[HEADER_LEN..], &[30, 20, 10, 40]);
    }

    #[test]
    fn the_round_trip_check_notices_a_corrupted_byte() {
        let original = asymmetric(4, 4);
        let mut encoded = encode_tga(4, 4, &original).expect("encode");
        assert_eq!(first_difference(&encoded, &original), None);

        encoded[HEADER_LEN + 5] ^= 0xFF;
        assert!(first_difference(&encoded, &original).is_some());
    }

    #[test]
    fn the_round_trip_check_notices_a_truncated_file() {
        let original = asymmetric(4, 4);
        let encoded = encode_tga(4, 4, &original).expect("encode");
        assert!(first_difference(&encoded[..HEADER_LEN + 8], &original).is_some());
        assert!(first_difference(&[], &original).is_some());
    }

    #[test]
    fn a_buffer_that_does_not_match_the_dimensions_is_refused() {
        assert!(matches!(
            encode_tga(4, 4, &[0u8; 63]),
            Err(Error::ScreenshotSize { .. })
        ));
        assert!(matches!(
            encode_tga(0, 4, &[]),
            Err(Error::ScreenshotSize { .. })
        ));
        assert!(matches!(
            encode_tga(70_000, 1, &vec![0u8; 280_000]),
            Err(Error::ScreenshotSize { .. }),
        ));
    }

    #[test]
    fn the_encoded_length_is_the_header_plus_four_bytes_a_pixel() {
        let encoded = encode_tga(16, 9, &vec![0u8; 16 * 9 * 4]).expect("encode");
        assert_eq!(encoded.len(), HEADER_LEN + 16 * 9 * 4);
    }
}
