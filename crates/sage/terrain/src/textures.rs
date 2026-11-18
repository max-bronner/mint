use mint_render::{PixelFormat, TextureData};
use mint_vfs::{Vfs, VfsPath};

use crate::{class_mip_levels, TerrainTypes};

const TERRAIN_TEXTURE_DIR: &str = "art/terrain/";

#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    #[error("terrain type {name} is not declared in terrain.ini")]
    UnknownType { name: String },
    #[error("{path}: not present in the mounted archives (terrain type {name})")]
    Missing { name: String, path: VfsPath },
    #[error("{path}: {source}")]
    Read {
        path: VfsPath,
        source: mint_vfs::Error,
    },
    #[error("{path}: {source}")]
    Decode {
        path: VfsPath,
        source: mint_image::Error,
    },
    #[error("{path}: decoded {width}x{height} image is not a usable texture")]
    Unusable {
        path: VfsPath,
        width: u32,
        height: u32,
    },
}

#[derive(Debug)]
pub struct ClassTextures {
    pub textures: Vec<Option<TextureData>>,
    pub failures: Vec<(usize, TextureError)>,
    pub alpha_discarded: Vec<usize>,
}

#[must_use]
pub fn load_class_textures(
    vfs: &Vfs,
    types: &TerrainTypes,
    classes: &[mint_sage_map::TextureClass],
) -> ClassTextures {
    let mut textures = Vec::with_capacity(classes.len());
    let mut failures = Vec::new();
    let mut alpha_discarded = Vec::new();
    let mut cache: Vec<((VfsPath, usize), Option<Loaded>)> = Vec::new();

    for (index, class) in classes.iter().enumerate() {
        match load_one(vfs, types, class, &mut cache) {
            Ok((texture, discarded)) => {
                if discarded {
                    alpha_discarded.push(index);
                }
                textures.push(Some(texture));
            }
            Err(error) => {
                failures.push((index, error));
                textures.push(None);
            }
        }
    }

    ClassTextures {
        textures,
        failures,
        alpha_discarded,
    }
}

pub const TERRAIN_NORMAL_MAP_SUFFIX: &str = "_nrm";

#[must_use]
pub fn load_class_normal_maps(
    vfs: &Vfs,
    types: &TerrainTypes,
    classes: &[mint_sage_map::TextureClass],
) -> (Vec<Option<TextureData>>, Vec<(usize, TextureError)>) {
    let mut maps = Vec::with_capacity(classes.len());
    let mut failures = Vec::new();
    let mut cache: Vec<((VfsPath, usize), Option<Loaded>)> = Vec::new();
    for (index, class) in classes.iter().enumerate() {
        let Some(path) = types.texture(&class.name).map(normal_map_path) else {
            maps.push(None);
            continue;
        };
        if !vfs.contains(path.as_str()) {
            maps.push(None);
            continue;
        }
        let key = (
            path.clone(),
            usize::try_from(class.width.max(0)).unwrap_or(0),
        );
        let loaded = match cache.iter().find(|(existing, _)| existing == &key) {
            Some((_, cached)) => cached.clone(),
            None => {
                let decoded = decode(vfs, &path, &class.name, class.width);
                let kept = match decoded {
                    Ok(loaded) => Some(loaded),
                    Err(error) => {
                        failures.push((index, error));
                        None
                    }
                };
                cache.push((key, kept.clone()));
                kept
            }
        };
        maps.push(loaded.map(|(data, _)| data));
    }
    (maps, failures)
}

fn normal_map_path(file: &str) -> VfsPath {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    VfsPath::new(&format!(
        "{TERRAIN_TEXTURE_DIR}{stem}{TERRAIN_NORMAL_MAP_SUFFIX}.tga"
    ))
}

#[must_use]
pub fn force_opaque(texture: &mut TextureData) -> bool {
    let mut discarded = false;
    for level in &mut texture.levels {
        for texel in level.data.chunks_exact_mut(4) {
            let Some(alpha) = texel.get_mut(3) else {
                continue;
            };
            if *alpha != u8::MAX {
                discarded = true;
                *alpha = u8::MAX;
            }
        }
    }
    discarded
}

type Loaded = (TextureData, bool);

fn load_one(
    vfs: &Vfs,
    types: &TerrainTypes,
    class: &mint_sage_map::TextureClass,
    cache: &mut Vec<((VfsPath, usize), Option<Loaded>)>,
) -> Result<Loaded, TextureError> {
    let file = types
        .texture(&class.name)
        .ok_or_else(|| TextureError::UnknownType {
            name: class.name.clone(),
        })?;
    let stem = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let path = VfsPath::new(&format!("{TERRAIN_TEXTURE_DIR}{stem}"));

    let key = (
        path.clone(),
        usize::try_from(class.width.max(0)).unwrap_or(0),
    );
    if let Some((_, cached)) = cache.iter().find(|(existing, _)| existing == &key) {
        return cached.clone().ok_or(TextureError::Missing {
            name: class.name.clone(),
            path,
        });
    }

    let loaded = decode(vfs, &path, &class.name, class.width);
    cache.push((key, loaded.as_ref().ok().cloned()));
    loaded
}

fn decode(vfs: &Vfs, path: &VfsPath, name: &str, tiles: i32) -> Result<Loaded, TextureError> {
    if !vfs.contains(path.as_str()) {
        return Err(TextureError::Missing {
            name: name.to_owned(),
            path: path.clone(),
        });
    }
    let bytes = vfs.read_path(path).map_err(|source| TextureError::Read {
        path: path.clone(),
        source,
    })?;
    let image = mint_image::tga::decode(&bytes).map_err(|source| TextureError::Decode {
        path: path.clone(),
        source,
    })?;
    let (width, height) = (image.width(), image.height());
    let mut data =
        TextureData::rgba8(width, height, image.into_pixels()).ok_or(TextureError::Unusable {
            path: path.clone(),
            width,
            height,
        })?;
    data.format = PixelFormat::linear(data.format);
    let discarded = force_opaque(&mut data);
    data.generate_mipmaps(class_mip_levels(width, height, tiles));
    Ok((data, discarded))
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_map::TextureClass;

    fn class(name: &str) -> TextureClass {
        TextureClass {
            first_tile: 0,
            num_tiles: 16,
            width: 4,
            name: name.to_owned(),
        }
    }

    #[test]
    fn a_textures_own_alpha_channel_is_discarded_at_every_level() {
        let mut texture = TextureData::rgba8(2, 1, vec![1, 2, 3, 40, 4, 5, 6, 255])
            .expect("2x1 rgba8 is a usable texture");
        texture.levels.push(mint_render::MipLevel {
            width: 1,
            height: 1,
            data: vec![7, 8, 9, 0],
        });

        assert!(force_opaque(&mut texture), "a masked channel is reported");
        for level in &texture.levels {
            assert!(
                level.data.chunks_exact(4).all(|texel| texel[3] == 255),
                "every level is opaque"
            );
        }
        assert_eq!(texture.levels[0].data[..3], [1, 2, 3]);

        assert!(
            !force_opaque(&mut texture),
            "a second pass has nothing left to discard"
        );
    }

    #[test]
    fn a_class_that_cannot_be_loaded_keeps_its_slot_and_is_reported() {
        let vfs = Vfs::new();
        let types = TerrainTypes::parse("Terrain Known\n  Texture = known.tga\nEnd\n");
        let loaded = load_class_textures(&vfs, &types, &[class("Known"), class("Unknown")]);

        assert_eq!(loaded.textures.len(), 2);
        assert!(loaded.textures.iter().all(Option::is_none));
        assert_eq!(loaded.failures.len(), 2);
        assert!(matches!(loaded.failures[0].1, TextureError::Missing { .. }));
        assert!(matches!(
            loaded.failures[1].1,
            TextureError::UnknownType { .. }
        ));
    }

    struct Files(Vec<(VfsPath, Vec<u8>)>);

    impl mint_vfs::Mount for Files {
        fn label(&self) -> &str {
            "synthetic"
        }

        fn entries(&self) -> Vec<VfsPath> {
            self.0.iter().map(|(path, _)| path.clone()).collect()
        }

        fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
            self.0
                .iter()
                .find(|(name, _)| name == path)
                .map(|(_, body)| body.clone())
                .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })
        }
    }

    fn flat_normal_tga() -> Vec<u8> {
        let mut bytes = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 24, 0x20];
        for _ in 0..4 {
            bytes.extend_from_slice(&[255, 128, 128]);
        }
        bytes
    }

    #[test]
    fn a_normal_map_is_the_textures_nrm_sibling_and_its_absence_is_no_failure() {
        let mut vfs = Vfs::new();
        vfs.mount(std::sync::Arc::new(Files(vec![
            (VfsPath::new("art/terrain/bumpy_nrm.tga"), flat_normal_tga()),
            (VfsPath::new("art/terrain/broken_nrm.tga"), vec![1, 2, 3]),
        ])));
        let types = TerrainTypes::parse(
            "Terrain Bumpy\n  Texture = Bumpy.tga\nEnd\n\
             Terrain Plain\n  Texture = plain.tga\nEnd\n\
             Terrain Broken\n  Texture = broken.tga\nEnd\n",
        );
        let (maps, failures) = load_class_normal_maps(
            &vfs,
            &types,
            &[class("Bumpy"), class("Plain"), class("Broken")],
        );

        assert_eq!(maps.len(), 3);
        let bumpy = maps[0]
            .as_ref()
            .expect("bumpy_nrm.tga is Bumpy.tga's sibling");
        assert_eq!(bumpy.levels[0].data[..3], [128, 128, 255]);
        assert!(maps[1].is_none(), "plain.tga has no sibling");
        assert!(maps[2].is_none(), "broken_nrm.tga does not decode");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, 2);
        assert!(matches!(failures[0].1, TextureError::Decode { .. }));
    }
}
