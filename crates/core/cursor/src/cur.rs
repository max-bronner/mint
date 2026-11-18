use crate::dib::{self};
use mint_bytes::{u16_at, u32_at};

use crate::{CursorImage, Error, Result};

const ICONDIR_SIZE: usize = 6;
const ICONDIRENTRY_SIZE: usize = 16;

const TYPE_ICON: u16 = 1;
const TYPE_CURSOR: u16 = 2;

pub fn decode(bytes: &[u8]) -> Result<CursorImage> {
    if bytes.len() < ICONDIR_SIZE {
        return Err(Error::HeaderTruncated {
            format: "cur",
            len: bytes.len(),
            needed: ICONDIR_SIZE,
            offset: 0,
        });
    }
    let kind = u16_at(bytes, 2).unwrap_or(0);
    let count = u16_at(bytes, 4).unwrap_or(0);
    if !matches!(kind, TYPE_ICON | TYPE_CURSOR) {
        return Err(Error::BadField {
            format: "cur",
            field: "idType",
            found: u32::from(kind),
            expected: u32::from(TYPE_CURSOR),
        });
    }
    if count == 0 {
        return Err(Error::NoFrames { format: "cur" });
    }

    let entry_offset = ICONDIR_SIZE;
    let entry = bytes
        .get(entry_offset..entry_offset.saturating_add(ICONDIRENTRY_SIZE))
        .ok_or(Error::HeaderTruncated {
            format: "cur",
            len: bytes.len(),
            needed: entry_offset.saturating_add(ICONDIRENTRY_SIZE),
            offset: entry_offset,
        })?;

    let (hotspot_x, hotspot_y) = if kind == TYPE_CURSOR {
        (u16_at(entry, 4).unwrap_or(0), u16_at(entry, 6).unwrap_or(0))
    } else {
        (0, 0)
    };
    let frame_len = u32_at(entry, 8).unwrap_or(0);
    let frame_offset = u32_at(entry, 12).unwrap_or(0);
    let frame_offset = usize::try_from(frame_offset).map_err(|_| Error::DataTruncated {
        format: "cur",
        offset: entry_offset,
        needed: 0,
        available: 0,
    })?;
    let frame_len = usize::try_from(frame_len).map_err(|_| Error::DataTruncated {
        format: "cur",
        offset: frame_offset,
        needed: 0,
        available: 0,
    })?;
    let frame_end = frame_offset
        .checked_add(frame_len)
        .ok_or(Error::DataTruncated {
            format: "cur",
            offset: frame_offset,
            needed: frame_len,
            available: bytes.len().saturating_sub(frame_offset),
        })?;
    let frame = bytes
        .get(frame_offset..frame_end)
        .ok_or(Error::DataTruncated {
            format: "cur",
            offset: frame_offset,
            needed: frame_len,
            available: bytes.len().saturating_sub(frame_offset.min(bytes.len())),
        })?;

    let decoded = dib::decode(frame)?;
    Ok(CursorImage {
        width: decoded.width,
        height: decoded.height,
        hotspot: (hotspot_x, hotspot_y),
        pixels: decoded.rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::CurBuilder;

    #[test]
    fn a_1bpp_two_colour_frame_decodes_to_its_two_colours() {
        let bytes = CurBuilder::new(2, 2, 1)
            .palette(&[(0, 0, 0), (255, 0, 0)])
            .xor_bits(&[0b0100_0000, 0b1000_0000])
            .and_bits(&[0, 0])
            .hotspot(1, 1)
            .build();
        let img = decode(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (2, 2));
        assert_eq!(img.hotspot(), (1, 1));
        assert_eq!(&img.pixels()[0..4], &[0, 0, 0, 255]);
        assert_eq!(&img.pixels()[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn an_and_mask_bit_makes_a_pixel_transparent() {
        let bytes = CurBuilder::new(1, 1, 1)
            .palette(&[(0, 0, 0), (255, 255, 255)])
            .xor_bits(&[0b1000_0000])
            .and_bits(&[0b1000_0000])
            .build();
        let img = decode(&bytes).unwrap();
        assert_eq!(img.pixels(), &[255, 255, 255, 0]);
    }

    #[test]
    fn an_icon_typed_file_has_no_hotspot_of_its_own() {
        let bytes = CurBuilder::new(1, 1, 1)
            .as_icon()
            .palette(&[(0, 0, 0), (255, 255, 255)])
            .xor_bits(&[0b1000_0000])
            .and_bits(&[0])
            .hotspot(9, 9)
            .build();
        let img = decode(&bytes).unwrap();
        assert_eq!(img.hotspot(), (0, 0));
    }

    #[test]
    fn a_zero_entry_count_is_an_error_not_a_panic() {
        let mut bytes = CurBuilder::new(1, 1, 1)
            .palette(&[(0, 0, 0), (0, 0, 0)])
            .xor_bits(&[0])
            .and_bits(&[0])
            .build();
        bytes[4] = 0;
        bytes[5] = 0;
        assert!(matches!(decode(&bytes), Err(Error::NoFrames { .. })));
    }

    #[test]
    fn truncated_bytes_are_an_error_not_a_panic() {
        let bytes = CurBuilder::new(4, 4, 8)
            .palette(&(0..=255).map(|v| (v, v, v)).collect::<Vec<_>>())
            .xor_bits(&[0u8; 16])
            .and_bits(&[0u8; 4])
            .build();
        for len in 0..bytes.len() {
            assert!(
                decode(&bytes[..len]).is_err(),
                "len {len} should not decode"
            );
        }
    }
}
