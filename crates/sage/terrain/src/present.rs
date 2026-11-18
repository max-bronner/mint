use mint_render::{Instance, MeshHandle, Renderer, TextureData, TextureHandle};

use crate::chunk::TerrainLayer;
use crate::textures::ClassTextures;
use crate::Terrain;

pub fn upload_class_textures(
    renderer: &mut Renderer,
    loaded: &ClassTextures,
) -> mint_render::Result<Vec<Option<TextureHandle>>> {
    let mut out = Vec::with_capacity(loaded.textures.len());
    for texture in &loaded.textures {
        out.push(match texture {
            Some(data) => Some(renderer.upload_texture(data)?),
            None => None,
        });
    }
    Ok(out)
}

pub fn upload_class_normal_maps(
    renderer: &mut Renderer,
    normal_maps: &[Option<TextureData>],
) -> mint_render::Result<Vec<Option<TextureHandle>>> {
    normal_maps
        .iter()
        .map(|map| {
            map.as_ref()
                .map(|data| renderer.upload_texture(data))
                .transpose()
        })
        .collect()
}

pub fn upload_terrain_plain(
    renderer: &mut Renderer,
    terrain: &Terrain,
    textures: &[Option<TextureHandle>],
) -> mint_render::Result<(Vec<MeshHandle>, Vec<Instance>)> {
    let mut handles = Vec::new();
    let mut instances = Vec::new();
    for part in terrain
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.parts)
        .filter(|part| part.layer == TerrainLayer::Base)
    {
        let handle = renderer.upload_mesh(&part.mesh)?;
        handles.push(handle);
        instances.push(Instance {
            cull: true,
            texture: part
                .texture_class
                .and_then(|class| textures.get(class).copied().flatten()),
            ..Instance::new(handle)
        });
    }
    Ok((handles, instances))
}
