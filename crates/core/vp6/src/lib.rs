#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod bits;
mod bool_decoder;
mod decoder;
mod header;
mod huffman;
mod idct;
mod mc;
mod models;
#[rustfmt::skip]
mod tables;

#[cfg(test)]
mod tests;

pub use decoder::Decoder;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("VP6 frame truncated: {0}")]
    Truncated(&'static str),
    #[error("VP6 frame header invalid: {0}")]
    InvalidHeader(&'static str),
    #[error("VP6 bitstream version {0} is not supported (6 to 8)")]
    UnsupportedVersion(u8),
    #[error("VP6 profile {0} is not supported (0 or 3)")]
    UnsupportedProfile(u8),
    #[error("VP6 inter frame without a preceding key frame")]
    MissingReference,
    #[error("VP6 frame data invalid: {0}")]
    InvalidData(&'static str),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plane {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Plane {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0; width.saturating_mul(height)],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn row(&self, y: usize) -> Option<&[u8]> {
        let start = y.checked_mul(self.width)?;
        self.data.get(start..start.checked_add(self.width)?)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picture {
    planes: [Plane; 3],
}

impl Picture {
    fn new(mb_w: usize, mb_h: usize) -> Self {
        let (w, h) = (mb_w.saturating_mul(16), mb_h.saturating_mul(16));
        Self {
            planes: [
                Plane::new(w, h),
                Plane::new(w / 2, h / 2),
                Plane::new(w / 2, h / 2),
            ],
        }
    }

    pub fn width(&self) -> usize {
        self.planes[0].width
    }

    pub fn height(&self) -> usize {
        self.planes[0].height
    }

    pub fn y(&self) -> &Plane {
        &self.planes[0]
    }

    pub fn u(&self) -> &Plane {
        &self.planes[1]
    }

    pub fn v(&self) -> &Plane {
        &self.planes[2]
    }
}
