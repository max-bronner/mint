#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod ani;
pub mod cur;
mod dib;

#[cfg(test)]
mod synth;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("cursor: {len} bytes do not begin with the ICO/CUR or RIFF ACON magic")]
    NotACursor { len: usize },

    #[error("{format}: {len} bytes is too short for its {needed}-byte header at offset {offset}")]
    HeaderTruncated {
        format: &'static str,
        len: usize,
        needed: usize,
        offset: usize,
    },

    #[error("{format}: {field} is {found}, expected {expected}")]
    BadField {
        format: &'static str,
        field: &'static str,
        found: u32,
        expected: u32,
    },

    #[error("{format}: chunk at byte {offset} needs {needed} bytes but only {available} remain")]
    DataTruncated {
        format: &'static str,
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error("cursor: {width}x{height} is not a usable frame size")]
    BadDimensions { width: u32, height: u32 },

    #[error("cursor: {bits}-bit frames are not supported (1, 4, 8, 24, 32 are)")]
    UnsupportedBitDepth { bits: u16 },

    #[error("cursor: PNG-encoded icon frames are not supported")]
    PngIconUnsupported,

    #[error("cursor: {format} declares zero frames")]
    NoFrames { format: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorImage {
    width: u32,
    height: u32,
    hotspot: (u16, u16),
    pixels: Vec<u8>,
}

impl CursorImage {
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn hotspot(&self) -> (u16, u16) {
        self.hotspot
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Cursor {
    Static(CursorImage),
    Animated(ani::AnimatedCursor),
}

pub fn decode(bytes: &[u8]) -> Result<Cursor> {
    if bytes.get(..4) == Some(b"RIFF") {
        return Ok(Cursor::Animated(ani::decode(bytes)?));
    }
    if is_ico_cur_magic(bytes) {
        return Ok(Cursor::Static(cur::decode(bytes)?));
    }
    Err(Error::NotACursor { len: bytes.len() })
}

pub(crate) fn is_ico_cur_magic(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some([0, 0, 1 | 2, 0]))
}

pub(crate) fn rgba_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrecognised_bytes_are_named_not_panicked_on() {
        assert!(matches!(
            decode(b"not a cursor"),
            Err(Error::NotACursor { .. })
        ));
        assert!(matches!(decode(&[]), Err(Error::NotACursor { len: 0 })));
    }
}
