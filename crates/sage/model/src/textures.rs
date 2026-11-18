use mint_image::dds::{BlockFormat, Dds};
use mint_image::{jpeg, tga};
use mint_render::{MipLevel, PixelFormat, TextureData};
use mint_sage_asset::Resolver;
use mint_vfs::{Vfs, VfsPath};

use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct LoadedTexture {
    pub path: VfsPath,
    pub data: TextureData,
    pub compressed_upload: bool,
    pub source_bytes: usize,
}

#[must_use]
pub fn missing_texture() -> TextureData {
    TextureData::single(PixelFormat::Rgba8Unorm, 1, 1, vec![255, 0, 255, 127])
}

pub fn load_texture(
    vfs: &Vfs,
    resolver: &Resolver,
    name: &str,
    allow_compressed: bool,
) -> Result<LoadedTexture> {
    let path = resolver
        .resolve(vfs, name)
        .map_err(|source| Error::Unresolved {
            name: name.to_owned(),
            source,
        })?;
    let bytes = vfs.read_path(&path)?;
    let source_bytes = bytes.len();

    let extension = path.extension().unwrap_or_default().to_ascii_lowercase();
    let (data, compressed_upload) = match extension.as_str() {
        "dds" => decode_dds(&bytes, allow_compressed)?,
        "tga" => (decode_rgba(tga::decode(&bytes)?)?, false),
        "jpg" | "jpeg" => (decode_rgba(jpeg::decode(&bytes)?)?, false),
        other => {
            return Err(Error::UnsupportedTexture {
                name: name.to_owned(),
                path: path.clone(),
                extension: other.to_owned(),
            })
        }
    };

    Ok(LoadedTexture {
        path,
        data,
        compressed_upload,
        source_bytes,
    })
}

pub fn load_normal_map(
    vfs: &Vfs,
    resolver: &Resolver,
    name: &str,
    allow_compressed: bool,
) -> Result<LoadedTexture> {
    let mut loaded = load_texture(vfs, resolver, name, allow_compressed)?;
    loaded.data.format = loaded.data.format.linear();
    Ok(loaded)
}

pub fn load_bump_map(vfs: &Vfs, resolver: &Resolver, name: &str) -> Result<LoadedTexture> {
    let mut loaded = load_texture(vfs, resolver, name, false)?;
    let level = loaded
        .data
        .levels
        .first()
        .ok_or(Error::MalformedDecodedImage)?;
    let offsets =
        bump_offsets(level.width, level.height, &level.data).ok_or(Error::MalformedDecodedImage)?;
    loaded.data = TextureData::single(PixelFormat::Rg8Snorm, level.width, level.height, offsets);
    Ok(loaded)
}

fn bump_offsets(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    if rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let luminance: Vec<i32> = rgba
        .chunks_exact(4)
        .map(|texel| match *texel {
            [r, g, b, _] => {
                (u32::from(b) * 0x1275 + u32::from(g) * 0xB725 + u32::from(r) * 0x3666) >> 16
            }
            _ => 0,
        })
        .map(|l| i32::try_from(l & 0xFF).unwrap_or(0))
        .collect();
    let at = |index: usize| luminance.get(index).copied().unwrap_or(0);
    let last = luminance.len().saturating_sub(1);
    let mut out = Vec::with_capacity(luminance.len() * 2);
    for index in 0..luminance.len() {
        let row = index / width;
        let mid = at(index);
        let left = at(index.saturating_sub(1));
        let right = at(index.saturating_add(1).min(last));
        let above = if row == 0 { mid } else { at(index - width) };
        let below = if row + 1 == height {
            mid
        } else {
            at(index + width)
        };

        let mut du = left - right;
        let dv = below - above;
        if mid < left && mid < right {
            du = (left - mid).max(mid - right);
        }
        let [du, ..] = du.to_le_bytes();
        let [dv, ..] = dv.to_le_bytes();
        out.push(du);
        out.push(dv);
    }
    Some(out)
}

fn decode_dds(bytes: &[u8], allow_compressed: bool) -> Result<(TextureData, bool)> {
    let dds = Dds::parse(bytes)?;
    let format = match dds.format() {
        BlockFormat::Bc1 => PixelFormat::Bc1Srgb,
        BlockFormat::Bc3 => PixelFormat::Bc3Srgb,
    }
    .linear();

    if allow_compressed {
        let levels = dds
            .levels()
            .iter()
            .map(|level| MipLevel {
                width: level.width(),
                height: level.height(),
                data: level.blocks().to_vec(),
            })
            .collect();
        return Ok((
            TextureData {
                format,
                levels,
                address_u: mint_render::AddressMode::Repeat,
                address_v: mint_render::AddressMode::Repeat,
            },
            true,
        ));
    }

    let image = dds.decode_level(0)?;
    let mut data = TextureData::rgba8(image.width(), image.height(), image.into_pixels())
        .ok_or(Error::MalformedDecodedImage)?;
    data.format = data.format.linear();
    Ok((data, false))
}

fn decode_rgba(image: mint_image::RgbaImage) -> Result<TextureData> {
    let mut data = TextureData::rgba8(image.width(), image.height(), image.into_pixels())
        .ok_or(Error::MalformedDecodedImage)?;
    data.format = data.format.linear();
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_offsets_follow_ea_heightmap_conversion() {
        let heights = [10u8, 50, 200, 90, 5, 30];
        let rgba: Vec<u8> = heights.iter().flat_map(|&v| [v, v, v, 255]).collect();
        assert_eq!(
            bump_offsets(3, 2, &rgba),
            Some(vec![216, 80, 66, 211, 216, 86, 195, 80, 85, 211, 231, 86])
        );
        assert_eq!(bump_offsets(3, 2, &rgba[..20]), None);
    }

    #[test]
    fn decoded_colour_textures_are_already_linear_not_srgb() {
        let file = synthetic_dxt1(4, 4, &red_block());
        let (data, _) = decode_dds(&file, true).expect("valid DXT1");
        assert_eq!(data.format, PixelFormat::Bc1Unorm);
        assert!(!data.format.is_srgb());
    }

    #[test]
    fn relabelling_a_decoded_texture_linear_again_does_not_move_its_bytes() {
        let file = synthetic_dxt1(4, 4, &red_block());
        let (data, _) = decode_dds(&file, true).expect("valid DXT1");
        let relabelled = TextureData {
            format: data.format.linear(),
            levels: data.levels.clone(),
            address_u: data.address_u,
            address_v: data.address_v,
        };
        assert_eq!(relabelled.format, data.format);
        assert_eq!(relabelled.levels, data.levels);
    }

    fn synthetic_dxt1(width: u32, height: u32, blocks: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(128 + blocks.len());
        out.extend_from_slice(b"DDS ");
        let mut header = [0u32; 31];
        header[0] = 124;
        header[1] = 0x0000_1007;
        header[2] = height;
        header[3] = width;
        header[4] = u32::try_from(blocks.len()).unwrap_or(0);
        header[18] = 32;
        header[19] = 0x4;
        header[20] = u32::from_le_bytes(*b"DXT1");
        header[26] = 0x1000;
        for value in header {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(blocks);
        out
    }

    fn red_block() -> Vec<u8> {
        let mut block = Vec::with_capacity(8);
        block.extend_from_slice(&0xF800u16.to_le_bytes());
        block.extend_from_slice(&0x0000u16.to_le_bytes());
        block.extend_from_slice(&[0u8; 4]);
        block
    }

    #[test]
    fn a_dds_is_uploaded_as_blocks_when_the_device_can_take_them() {
        let file = synthetic_dxt1(4, 4, &red_block());
        let (data, compressed) = decode_dds(&file, true).expect("valid DXT1");

        assert!(
            compressed,
            "the compressed path is the point of this module"
        );
        assert_eq!(data.format, PixelFormat::Bc1Unorm);
        assert_eq!(data.levels.len(), 1);
        assert_eq!(
            data.levels.first().map(|level| level.data.len()),
            Some(8),
            "the eight bytes on disc must arrive unchanged, not expanded to 64"
        );
    }

    #[test]
    fn the_software_path_expands_the_same_file_eightfold() {
        let file = synthetic_dxt1(4, 4, &red_block());
        let (data, compressed) = decode_dds(&file, false).expect("valid DXT1");

        assert!(!compressed);
        assert_eq!(data.format, PixelFormat::Rgba8Unorm);
        assert_eq!(
            data.levels.first().map(|level| level.data.len()),
            Some(4 * 4 * 4),
            "64 bytes of RGBA against 8 of blocks - which is why the other path exists"
        );
        assert_eq!(
            data.levels
                .first()
                .and_then(|level| level.data.get(0..4))
                .map(<[u8]>::to_vec),
            Some(vec![255, 0, 0, 255])
        );
    }

    #[test]
    fn a_truncated_dds_is_an_error_rather_than_a_panic() {
        let file = synthetic_dxt1(4, 4, &red_block());
        for cut in 0..file.len() {
            let Some(prefix) = file.get(..cut) else {
                continue;
            };
            let _ = decode_dds(prefix, true);
            let _ = decode_dds(prefix, false);
        }
    }

    #[test]
    fn every_single_byte_corruption_of_a_dds_is_handled() {
        let file = synthetic_dxt1(8, 8, &[0x55; 32]);
        for index in 0..file.len() {
            for mask in [0x01u8, 0x80, 0xFF] {
                let mut corrupt = file.clone();
                if let Some(byte) = corrupt.get_mut(index) {
                    *byte ^= mask;
                }
                let _ = decode_dds(&corrupt, true);
            }
        }
    }

    #[test]
    fn a_truncated_tga_is_an_error_rather_than_a_panic() {
        let mut file = vec![0u8; 18];
        file[2] = 2;
        file[12] = 4;
        file[14] = 4;
        file[16] = 32;
        file[17] = 0x28;
        file.extend_from_slice(&[0x7F; 4 * 4 * 4]);

        assert!(tga::decode(&file).map(decode_rgba).is_ok());
        for cut in 0..file.len() {
            let Some(prefix) = file.get(..cut) else {
                continue;
            };
            let _ = tga::decode(prefix).map(decode_rgba);
        }
    }
}
