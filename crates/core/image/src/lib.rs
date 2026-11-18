#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod dds;
pub mod jpeg;
pub mod png;
pub mod tga;

#[cfg(test)]
mod synth;

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) const RGBA_BYTES: usize = 4;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{format} input is {len} bytes, too short for its {needed}-byte header")]
    HeaderTruncated {
        format: &'static str,
        len: usize,
        needed: usize,
    },

    #[error("{format} declares {width}x{height}; neither dimension may be zero")]
    ZeroDimension {
        format: &'static str,
        width: u32,
        height: u32,
    },

    #[error("{format} declares {width}x{height}, whose size in bytes overflows usize")]
    DimensionsOverflow {
        format: &'static str,
        width: u32,
        height: u32,
    },

    #[error(
        "{format}: pixel data at byte {offset} needs {needed} bytes but only {available} remain"
    )]
    DataTruncated {
        format: &'static str,
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error(
        "tga: image type {image_type} is not supported \n         (2 and 10 are true colour, 3 and 11 greyscale)"
    )]
    TgaUnsupportedImageType { image_type: u8 },

    #[error(
        "tga: {bits}-bit pixels are not supported \n         (16, 24 and 32 for true colour, 8 for greyscale)"
    )]
    TgaUnsupportedDepth { bits: u8 },

    #[error("tga: RLE packet at byte {offset} runs {pixels} pixels past the {declared} declared")]
    TgaRleOverrun {
        offset: usize,
        pixels: usize,
        declared: usize,
    },

    #[error("dds: magic is {magic:02x?}, not \"DDS \"")]
    DdsNotDds { magic: [u8; 4] },

    #[error("dds: {structure} size field says {found}, but the format fixes it at {expected}")]
    DdsBadStructureSize {
        structure: &'static str,
        found: u32,
        expected: u32,
    },

    #[error(
        "dds: pixel format flags {flags:#010x} describe an uncompressed {bit_count}-bit \
         surface, not a FourCC block format"
    )]
    DdsNotBlockCompressed { flags: u32, bit_count: u32 },

    #[error("dds: FourCC {name:?} ({fourcc:02x?}) is not supported (only DXT1 and DXT5 are)")]
    DdsUnsupportedFourCc { name: String, fourcc: [u8; 4] },

    #[error("dds: {layout} surfaces are not supported (caps2 {caps2:#010x}, depth {depth})")]
    DdsUnsupportedLayout {
        layout: &'static str,
        caps2: u32,
        depth: u32,
    },

    #[error("dds: {declared} mip levels declared, but {width}x{height} allows at most {possible}")]
    DdsMipCountImplausible {
        declared: u32,
        width: u32,
        height: u32,
        possible: u32,
    },

    #[error("dds: mip level {level} requested, but the file has {levels}")]
    DdsNoSuchLevel { level: usize, levels: usize },

    #[error("jpeg declares {width}x{height}; mint decodes at most {limit} on either axis")]
    JpegTooLarge { width: u16, height: u16, limit: u16 },

    #[error("tga declares {width}x{height}; mint decodes at most {limit} on either axis")]
    TgaTooLarge { width: u16, height: u16, limit: u16 },

    #[error("jpeg: {format} is not supported")]
    JpegUnsupportedPixelFormat { format: &'static str },

    #[error("jpeg: the decoder returned no frame header")]
    JpegNoFrame,

    #[error("jpeg: the decoder panicked on this input")]
    JpegDecoderPanicked,

    #[error("jpeg: {source}")]
    Jpeg {
        #[source]
        source: Box<zune_jpeg::errors::DecodeErrors>,
    },

    #[error("png declares {width}x{height}; mint decodes at most {limit} on either axis")]
    PngTooLarge { width: u32, height: u32, limit: u32 },

    #[error("png: the decoder returned no usable header")]
    PngNoHeader,

    #[error("png: the decoder panicked on this input")]
    PngDecoderPanicked,

    #[error("png: {source}")]
    Png {
        #[source]
        source: Box<zune_png::error::PngDecodeErrors>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub(crate) fn new(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        debug_assert_eq!(
            rgba_len(width, height),
            Some(pixels.len()),
            "RgbaImage built with a buffer that is not width * height * 4"
        );
        Self {
            width,
            height,
            pixels,
        }
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
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn with_alpha_of(mut self, alpha: &RgbaImage) -> Option<Self> {
        if (self.width, self.height) != (alpha.width, alpha.height) {
            return None;
        }
        for (texel, from) in self
            .pixels
            .chunks_exact_mut(RGBA_BYTES)
            .zip(alpha.pixels.chunks_exact(RGBA_BYTES))
        {
            if let (Some(to), Some(&a)) = (texel.get_mut(3), from.get(3)) {
                *to = a;
            }
        }
        Some(self)
    }

    #[must_use]
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.width).ok()?)?
            .checked_add(usize::try_from(x).ok()?)?
            .checked_mul(RGBA_BYTES)?;
        let end = start.checked_add(RGBA_BYTES)?;
        self.pixels.get(start..end)?.try_into().ok()
    }
}

pub(crate) fn rgba_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(RGBA_BYTES)
}

pub(crate) fn expand5(value: u16) -> u8 {
    let five = value & 0x1f;
    let expanded = (five.checked_shl(3).unwrap_or(0)) | (five.checked_shr(2).unwrap_or(0));
    u8::try_from(expanded & 0xff).unwrap_or(u8::MAX)
}

pub(crate) fn expand6(value: u16) -> u8 {
    let six = value & 0x3f;
    let expanded = (six.checked_shl(2).unwrap_or(0)) | (six.checked_shr(4).unwrap_or(0));
    u8::try_from(expanded & 0xff).unwrap_or(u8::MAX)
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::{rgba_len, RgbaImage};

    #[test]
    fn a_pixel_outside_the_image_is_none_rather_than_a_panic() {
        let image = RgbaImage::new(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(image.pixel(0, 0), Some([1, 2, 3, 4]));
        assert_eq!(image.pixel(1, 0), Some([5, 6, 7, 8]));
        assert_eq!(image.pixel(2, 0), None);
        assert_eq!(image.pixel(0, 1), None);
        assert_eq!(image.pixel(u32::MAX, u32::MAX), None);
    }

    #[test]
    fn an_rgba_buffer_size_that_overflows_the_address_space_is_none() {
        assert_eq!(rgba_len(4, 4), Some(64));
        assert_eq!(rgba_len(u32::MAX, u32::MAX), None);
    }
}
