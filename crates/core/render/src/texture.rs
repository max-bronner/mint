#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    Rgba8Srgb,
    Bc1Srgb,
    Bc3Srgb,
    Rgba8Unorm,
    Bc1Unorm,
    Bc3Unorm,
    Rg8Snorm,
    R8Unorm,
}

impl PixelFormat {
    #[must_use]
    pub fn block_edge(self) -> u32 {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm | Self::Rg8Snorm | Self::R8Unorm => 1,
            Self::Bc1Srgb | Self::Bc3Srgb | Self::Bc1Unorm | Self::Bc3Unorm => 4,
        }
    }

    #[must_use]
    pub fn bytes_per_block(self) -> usize {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm => 4,
            Self::R8Unorm => 1,
            Self::Rg8Snorm => 2,
            Self::Bc1Srgb | Self::Bc1Unorm => 8,
            Self::Bc3Srgb | Self::Bc3Unorm => 16,
        }
    }

    #[must_use]
    pub fn is_block_compressed(self) -> bool {
        !matches!(
            self,
            Self::Rgba8Srgb | Self::Rgba8Unorm | Self::Rg8Snorm | Self::R8Unorm
        )
    }

    #[must_use]
    pub fn linear(self) -> Self {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm => Self::Rgba8Unorm,
            Self::Bc1Srgb | Self::Bc1Unorm => Self::Bc1Unorm,
            Self::Bc3Srgb | Self::Bc3Unorm => Self::Bc3Unorm,
            Self::Rg8Snorm => Self::Rg8Snorm,
            Self::R8Unorm => Self::R8Unorm,
        }
    }

    #[must_use]
    pub fn is_srgb(self) -> bool {
        matches!(self, Self::Rgba8Srgb | Self::Bc1Srgb | Self::Bc3Srgb)
    }

    #[must_use]
    pub fn bytes_per_row(self, width: u32) -> Option<usize> {
        let blocks = blocks_across(width, self.block_edge())?;
        usize::try_from(blocks)
            .ok()?
            .checked_mul(self.bytes_per_block())
    }

    #[must_use]
    pub fn level_size(self, width: u32, height: u32) -> Option<usize> {
        let rows = blocks_across(height, self.block_edge())?;
        self.bytes_per_row(width)?
            .checked_mul(usize::try_from(rows).ok()?)
    }
}

fn halve_rgba8(level: &MipLevel) -> Option<MipLevel> {
    let width = level.width.checked_div(2)?.max(1);
    let height = level.height.checked_div(2)?.max(1);
    if width == level.width && height == level.height {
        return None;
    }
    let (last_x, last_y) = (level.width.checked_sub(1)?, level.height.checked_sub(1)?);
    let capacity = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)?;
    let mut data = Vec::with_capacity(capacity);
    let texel = |x: u32, y: u32| -> Option<&[u8]> {
        let index = usize::try_from(y.min(last_y))
            .ok()?
            .checked_mul(usize::try_from(level.width).ok()?)?
            .checked_add(usize::try_from(x.min(last_x)).ok()?)?
            .checked_mul(4)?;
        level.data.get(index..index.checked_add(4)?)
    };
    for y in 0..height {
        for x in 0..width {
            let (x0, y0) = (x.checked_mul(2)?, y.checked_mul(2)?);
            let mut sums = [0u32; 4];
            for (sx, sy) in [
                (x0, y0),
                (x0.checked_add(1)?, y0),
                (x0, y0.checked_add(1)?),
                (x0.checked_add(1)?, y0.checked_add(1)?),
            ] {
                let source = texel(sx, sy)?;
                for (sum, byte) in sums.iter_mut().zip(source) {
                    *sum = sum.checked_add(u32::from(*byte))?;
                }
            }
            for sum in sums {
                data.push(u8::try_from(sum.checked_add(2)?.checked_div(4)?).unwrap_or(u8::MAX));
            }
        }
    }
    Some(MipLevel {
        width,
        height,
        data,
    })
}

fn blocks_across(pixels: u32, block_edge: u32) -> Option<u32> {
    if block_edge == 0 {
        return None;
    }
    pixels
        .checked_add(block_edge.checked_sub(1)?)?
        .checked_div(block_edge)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressMode {
    #[default]
    Repeat,
    Clamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureData {
    pub format: PixelFormat,
    pub levels: Vec<MipLevel>,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
}

impl TextureData {
    #[must_use]
    pub fn rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        if PixelFormat::Rgba8Unorm.level_size(width, height)? != pixels.len() {
            return None;
        }
        Some(Self::single(PixelFormat::Rgba8Unorm, width, height, pixels))
    }

    #[must_use]
    pub fn single(format: PixelFormat, width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            format,
            levels: vec![MipLevel {
                width,
                height,
                data,
            }],
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
        }
    }

    #[must_use]
    pub fn with_addressing(mut self, u: AddressMode, v: AddressMode) -> Self {
        self.address_u = u;
        self.address_v = v;
        self
    }

    pub fn generate_mipmaps(&mut self, max_levels: usize) -> usize {
        if !matches!(
            self.format,
            PixelFormat::Rgba8Srgb | PixelFormat::Rgba8Unorm
        ) {
            return self.levels.len();
        }
        let Some(base) = self.levels.first().cloned() else {
            return 0;
        };
        if self.format.level_size(base.width, base.height) != Some(base.data.len()) {
            return self.levels.len();
        }
        let mut levels = vec![base];
        while levels.len() < max_levels.max(1) {
            let Some(next) = levels.last().and_then(halve_rgba8) else {
                break;
            };
            levels.push(next);
        }
        self.levels = levels;
        self.levels.len()
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.levels.first().map_or(0, |level| level.width)
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.levels.first().map_or(0, |level| level.height)
    }

    #[must_use]
    pub fn usable_levels(&self) -> usize {
        let edge = self.format.block_edge();
        let mut usable = 0usize;
        for level in &self.levels {
            if level.width < edge || level.height < edge {
                break;
            }
            if self.format.level_size(level.width, level.height) != Some(level.data.len()) {
                break;
            }
            usable = usable.saturating_add(1);
        }
        usable
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn block_sizes_match_the_formats_definitions() {
        assert_eq!(PixelFormat::Bc1Srgb.bytes_per_block(), 8);
        assert_eq!(PixelFormat::Bc3Srgb.bytes_per_block(), 16);
        assert_eq!(PixelFormat::Rgba8Srgb.bytes_per_block(), 4);
        assert!(PixelFormat::Bc1Srgb.is_block_compressed());
        assert!(!PixelFormat::Rgba8Srgb.is_block_compressed());
    }

    #[test]
    fn a_linear_format_has_the_same_layout_as_the_colour_one() {
        for (colour, data) in [
            (PixelFormat::Rgba8Srgb, PixelFormat::Rgba8Unorm),
            (PixelFormat::Bc1Srgb, PixelFormat::Bc1Unorm),
            (PixelFormat::Bc3Srgb, PixelFormat::Bc3Unorm),
        ] {
            assert_eq!(colour.linear(), data);
            assert_eq!(data.linear(), data, "linear() must be idempotent");
            assert_eq!(colour.block_edge(), data.block_edge());
            assert_eq!(colour.bytes_per_block(), data.bytes_per_block());
            assert_eq!(colour.level_size(37, 21), data.level_size(37, 21));
            assert_eq!(colour.is_block_compressed(), data.is_block_compressed());
            assert!(colour.is_srgb() && !data.is_srgb());
        }
    }

    #[test]
    fn a_partial_block_still_costs_a_whole_block() {
        assert_eq!(PixelFormat::Bc1Srgb.level_size(5, 5), Some(4 * 8));
        assert_eq!(PixelFormat::Bc1Srgb.level_size(4, 4), Some(8));
        assert_eq!(
            PixelFormat::Bc3Srgb.level_size(256, 256),
            Some(64 * 64 * 16)
        );
    }

    #[test]
    fn an_rgba_level_is_four_bytes_a_pixel() {
        assert_eq!(PixelFormat::Rgba8Srgb.level_size(3, 2), Some(24));
        assert_eq!(PixelFormat::Rgba8Srgb.bytes_per_row(3), Some(12));
    }

    #[test]
    fn a_size_that_overflows_the_address_space_is_none_rather_than_a_wrap() {
        assert_eq!(PixelFormat::Rgba8Srgb.level_size(u32::MAX, u32::MAX), None);
        assert_eq!(PixelFormat::Bc1Srgb.bytes_per_row(u32::MAX), None);
    }

    #[test]
    fn rgba8_rejects_a_buffer_that_is_not_the_declared_size() {
        assert!(TextureData::rgba8(2, 2, vec![0; 16]).is_some());
        assert!(TextureData::rgba8(2, 2, vec![0; 15]).is_none());
        assert!(TextureData::rgba8(0, 2, vec![]).is_none());
    }

    #[test]
    fn rgba8_builds_unorm_not_srgb() {
        let texture = TextureData::rgba8(2, 2, vec![0; 16]).unwrap();
        assert_eq!(texture.format, PixelFormat::Rgba8Unorm);
    }

    #[test]
    fn compressed_levels_below_one_block_are_dropped_from_the_chain() {
        let texture = TextureData {
            format: PixelFormat::Bc1Srgb,
            levels: vec![
                MipLevel {
                    width: 8,
                    height: 8,
                    data: vec![0; 32],
                },
                MipLevel {
                    width: 4,
                    height: 4,
                    data: vec![0; 8],
                },
                MipLevel {
                    width: 2,
                    height: 2,
                    data: vec![0; 8],
                },
                MipLevel {
                    width: 1,
                    height: 1,
                    data: vec![0; 8],
                },
            ],
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
        };
        assert_eq!(texture.usable_levels(), 2);
        assert_eq!(texture.width(), 8);
    }

    #[test]
    fn a_truncated_level_ends_the_usable_chain() {
        let texture = TextureData {
            format: PixelFormat::Bc3Srgb,
            levels: vec![
                MipLevel {
                    width: 8,
                    height: 8,
                    data: vec![0; 64],
                },
                MipLevel {
                    width: 4,
                    height: 4,
                    data: vec![0; 3],
                },
            ],
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
        };
        assert_eq!(texture.usable_levels(), 1);
    }

    #[test]
    fn a_generated_chain_halves_each_axis_and_every_level_matches_its_own_size() {
        let mut texture = TextureData::rgba8(64, 32, vec![7; 64 * 32 * 4]).unwrap();
        assert_eq!(texture.generate_mipmaps(4), 4);
        let sizes: Vec<(u32, u32)> = texture.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(64, 32), (32, 16), (16, 8), (8, 4)]);
        for level in &texture.levels {
            assert_eq!(
                texture.format.level_size(level.width, level.height),
                Some(level.data.len())
            );
            assert!(
                level.data.iter().all(|byte| *byte == 7),
                "a flat surface stays flat"
            );
        }
        assert_eq!(texture.usable_levels(), 4);

        let mut small = TextureData::rgba8(4, 4, vec![0; 64]).unwrap();
        assert_eq!(small.generate_mipmaps(99), 3);
    }

    #[test]
    fn level_one_averages_a_two_by_two_block_rather_than_two_neighbours() {
        let mut pixels = Vec::new();
        for y in 0..4u32 {
            for x in 0..4u32 {
                let value = if (x + y) % 2 == 0 { 0u8 } else { 255 };
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let mut texture = TextureData::rgba8(4, 4, pixels).unwrap();
        texture.generate_mipmaps(2);
        let level1 = &texture.levels[1];
        assert_eq!((level1.width, level1.height), (2, 2));
        for texel in level1.data.chunks_exact(4) {
            assert_eq!(
                texel,
                [128, 128, 128, 255],
                "2x2 of 0,255,255,0 rounds to 128"
            );
        }
    }

    #[test]
    fn a_block_compressed_texture_is_left_alone() {
        let mut texture = TextureData::single(PixelFormat::Bc1Srgb, 8, 8, vec![0; 32]);
        assert_eq!(texture.generate_mipmaps(8), 1);
        assert_eq!(texture.levels.len(), 1);
    }

    #[test]
    fn a_texture_with_no_levels_has_no_size_and_no_usable_levels() {
        let texture = TextureData {
            format: PixelFormat::Rgba8Srgb,
            levels: Vec::new(),
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
        };
        assert_eq!(texture.width(), 0);
        assert_eq!(texture.height(), 0);
        assert_eq!(texture.usable_levels(), 0);
    }
}
