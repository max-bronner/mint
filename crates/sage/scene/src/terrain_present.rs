use mint_render::{
    Aabb, Blend, CustomMaterialHandle, CustomShader, CustomShaderHandle, DepthTest, Instance,
    MeshHandle, Renderer, TextureData, TextureHandle,
};
use mint_sage_model::Resolver;
use mint_sage_terrain::{
    ObjectProxy, Terrain, TerrainCloud, TerrainLayer, TerrainMacro, TerrainModulation, TerrainPart,
    TerrainUniforms, WaterTuning,
};
use mint_vfs::Vfs;

use crate::scene::WATER_INI;

pub const OBJECT_EMISSIVE: [f32; 3] = [0.18, 0.18, 0.18];

#[must_use]
pub fn part_texture(
    textures: &[Option<TextureHandle>],
    class: Option<usize>,
) -> Option<TextureHandle> {
    textures.get(class?).copied().flatten()
}

#[derive(Debug, Default)]
pub struct TerrainShading {
    pub shader: Option<CustomShaderHandle>,
    pub materials: Vec<Option<CustomMaterialHandle>>,
    pub fallback: Option<CustomMaterialHandle>,
    pub modulation: TerrainModulation,
    pub failures: Vec<String>,
    scene_textures: [Option<TextureHandle>; 2],
    pub shroud: Option<TextureHandle>,
}

impl TerrainShading {
    #[must_use]
    pub fn material_for(&self, class: Option<usize>) -> Option<CustomMaterialHandle> {
        match class.and_then(|index| self.materials.get(index).copied()) {
            Some(Some(handle)) => Some(handle),
            _ => self.fallback,
        }
    }

    pub fn bind_shroud(
        &mut self,
        renderer: &mut Renderer,
        tuning: &WaterTuning,
        texture: TextureHandle,
        extent: [f32; 4],
    ) -> mint_render::Result<()> {
        let Some(shader) = self.shader else {
            return Ok(());
        };
        self.shroud = Some(texture);
        self.modulation.shroud_loaded = true;
        self.modulation.shroud_extent = extent;
        let block = TerrainUniforms::new(&self.modulation, tuning).to_bytes();
        let [cloud, macro_texture] = self.scene_textures;
        renderer.set_custom_scene(shader, &[cloud, macro_texture, Some(texture)], &block)
    }

    #[must_use]
    pub fn cloud_cookie(&self, tuning: &WaterTuning) -> Option<mint_render::LightCookie> {
        let texture = self.scene_textures[0]?;
        let (transform, tint) = TerrainUniforms::new(&self.modulation, tuning).cloud_layer();
        Some(mint_render::LightCookie {
            texture,
            transform,
            tint,
        })
    }

    pub fn push(&mut self, renderer: &mut Renderer, tuning: &WaterTuning, seconds: f32) {
        let Some(shader) = self.shader else {
            return;
        };
        self.modulation.seconds = seconds;
        let block = TerrainUniforms::new(&self.modulation, tuning).to_bytes();
        if let Err(error) = renderer.update_custom_scene(shader, &block) {
            crate::once::report_once(&format!(
                "  could not update the terrain scene block: {error}"
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn upload_terrain_shading(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    environment: Option<&mint_sage_map::EnvironmentData>,
    no_cloud_factor: Option<[f32; 3]>,
    overbright: f32,
    bounds: Aabb,
    textures: &[Option<TextureHandle>],
    normal_maps: &[Option<TextureHandle>],
    cloud: TerrainCloud,
    macro_layer: TerrainMacro,
    tuning: &WaterTuning,
) -> TerrainShading {
    let mut shading = TerrainShading {
        modulation: TerrainModulation {
            cloud,
            macro_layer,
            no_cloud_factor: no_cloud_factor.unwrap_or([1.0; 3]),
            overbright,
            macro_stretched: environment.is_some_and(|env| env.is_macro_texture_stretched),
            ..TerrainModulation::default()
        },
        ..TerrainShading::default()
    };
    if !bounds.is_empty() {
        let (min, max) = (bounds.min(), bounds.max());
        let size_x = (max.x - min.x).max(1.0);
        let size_y = (max.y - min.y).max(1.0);
        shading.modulation.map_extent = [min.x, min.y, size_x, size_y];
    }
    let compressed = renderer.supports_block_compression();
    let mut failures: Vec<String> = Vec::new();
    let mut load = |renderer: &mut Renderer, name: &str, what: &str| -> Option<TextureHandle> {
        if name.is_empty() {
            return None;
        }
        match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
            Ok(loaded) => match renderer.upload_texture(&loaded.data) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    failures.push(format!("{what} {name}: {error}"));
                    None
                }
            },
            Err(error) => {
                failures.push(format!("{what} {name}: {error}"));
                None
            }
        }
    };
    let cloud_texture = environment.and_then(|env| load(renderer, &env.cloud_texture, "cloud"));
    let macro_texture = environment.and_then(|env| load(renderer, &env.macro_texture, "macro"));
    shading.modulation.cloud_loaded = cloud_texture.is_some();
    shading.modulation.macro_loaded = macro_texture.is_some();

    let shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme terrain (Default: normal-mapped sun, cloud, macro)",
        source: mint_sage_terrain::TERRAIN_SHADER_WGSL,
        texture_slots: mint_sage_terrain::TERRAIN_TEXTURE_SLOTS,
        uniform_size: 0,
        scene_texture_slots: mint_sage_terrain::TERRAIN_SCENE_TEXTURE_SLOTS,
        scene_uniform_size: mint_sage_terrain::TERRAIN_SCENE_UNIFORM_SIZE,
        writes_alpha: true,
        scene_depth: false,
        scene_color: false,
        ..CustomShader::default()
    }) {
        Ok(handle) => handle,
        Err(error) => {
            println!("  TERRAIN SHADER FAILED TO REGISTER: {error}");
            shading.failures = failures;
            return shading;
        }
    };
    shading.shader = Some(shader);
    shading.scene_textures = [cloud_texture, macro_texture];
    let block = TerrainUniforms::new(&shading.modulation, tuning).to_bytes();
    if let Err(error) =
        renderer.set_custom_scene(shader, &[cloud_texture, macro_texture, None], &block)
    {
        failures.push(format!("terrain scene binding: {error}"));
    }
    let flat_normal =
        match TextureData::rgba8(1, 1, mint_sage_terrain::TERRAIN_FLAT_NORMAL_TEXEL.to_vec()).map(
            |mut data| {
                data.format = mint_render::PixelFormat::linear(data.format);
                data
            },
        ) {
            Some(data) => match renderer.upload_texture(&data) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    failures.push(format!("flat normal texel: {error}"));
                    None
                }
            },
            None => None,
        };
    let mut build = |renderer: &mut Renderer,
                     texture: Option<TextureHandle>,
                     normal_map: Option<TextureHandle>| {
        match renderer.create_custom_material(shader, &[texture, normal_map.or(flat_normal)], &[]) {
            Ok(handle) => Some(handle),
            Err(error) => {
                failures.push(format!("terrain material: {error}"));
                None
            }
        }
    };
    shading.fallback = build(renderer, None, None);
    let mut materials = Vec::with_capacity(textures.len());
    for (index, texture) in textures.iter().enumerate() {
        let normal_map = normal_maps.get(index).copied().flatten();
        materials.push(build(renderer, *texture, normal_map));
    }
    shading.materials = materials;
    for line in &failures {
        println!("  TERRAIN SHADING {line}");
    }
    shading.failures = failures;
    println!(
        "terrain modulation: cloud = {} ({}, {} world units a repeat), macro = {} ({}, {}),          {} material(s), {} with a normal map, relief {}, one scene binding, {} byte(s) a frame",
        shading.modulation.cloud.label(),
        environment.map_or("no EnvironmentData", |env| if env.cloud_texture.is_empty() {
            "no cloud texture named"
        } else if shading.modulation.cloud_loaded {
            "loaded"
        } else {
            "NAMED BUT NOT LOADED - the term is 1.0"
        }),
        tuning.terrain_cloud_scale,
        shading.modulation.macro_layer.label(),
        environment.map_or("no EnvironmentData", |env| if env.macro_texture.is_empty() {
            "no macro texture named"
        } else if shading.modulation.macro_loaded {
            "loaded"
        } else {
            "NAMED BUT NOT LOADED - the term is 1.0"
        }),
        if shading.modulation.macro_stretched {
            "stretched to the map"
        } else {
            "tiled"
        },
        shading.materials.iter().filter(|m| m.is_some()).count(),
        normal_maps.iter().filter(|m| m.is_some()).count(),
        shading.modulation.relief.label(),
        mint_sage_terrain::TERRAIN_SCENE_UNIFORM_SIZE,
    );
    shading
}

pub fn load_sky_cloud(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    compressed: bool,
) -> (Option<TextureHandle>, Vec<String>) {
    let (cloud, _, failures) = load_water_set_sky(renderer, vfs, resolver, compressed, None);
    (cloud, failures)
}

pub fn load_water_set_sky(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    compressed: bool,
    set: Option<&str>,
) -> (
    Option<TextureHandle>,
    Option<mint_sage_terrain::WaterSkySet>,
    Vec<String>,
) {
    let mut failures = Vec::new();
    let parsed = match vfs.read(WATER_INI) {
        Ok(bytes) => mint_sage_terrain::parse_water_set(&String::from_utf8_lossy(&bytes), set),
        Err(error) => {
            failures.push(format!("{WATER_INI}: {error}"));
            None
        }
    };
    let cloud_name = parsed.as_ref().and_then(|set| set.texture.clone());
    let cloud = cloud_name.as_deref().and_then(|name| {
        if name.is_empty() {
            return None;
        }
        match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
            Ok(loaded) => match renderer.upload_texture(&loaded.data) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    failures.push(format!("sky cloud {name}: {error}"));
                    None
                }
            },
            Err(error) => {
                failures.push(format!("sky cloud {name}: {error}"));
                None
            }
        }
    });
    (cloud, parsed, failures)
}

#[must_use]
pub fn part_instance(
    mesh: MeshHandle,
    part: &TerrainPart,
    textures: &[Option<TextureHandle>],
    shading: &TerrainShading,
) -> Instance {
    let overlay = part.layer != TerrainLayer::Base;
    Instance {
        cull: true,
        custom_material: shading.material_for(part.texture_class),
        texture: part_texture(textures, part.texture_class),
        blend: if overlay {
            Blend::AlphaBlend
        } else {
            Blend::Opaque
        },
        depth_test: if overlay {
            DepthTest::NearerOrEqual
        } else {
            DepthTest::Nearer
        },
        sort_level: overlay_sort_level(part.layer),
        surface_overlay: overlay,
        vertex_alpha: overlay,
        bounds: Some(part.mesh.bounds()),
        ..Instance::new(mesh)
    }
}

#[must_use]
fn overlay_sort_level(layer: TerrainLayer) -> Option<u8> {
    match layer {
        TerrainLayer::Base => None,
        TerrainLayer::Blend => Some(32),
        TerrainLayer::ExtraBlend => Some(31),
    }
}

pub fn drawn_parts(terrain: &Terrain, blend: bool) -> impl Iterator<Item = &TerrainPart> + '_ {
    terrain
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.parts)
        .filter(move |part| blend || part.layer == TerrainLayer::Base)
}

pub fn upload_terrain(
    renderer: &mut Renderer,
    terrain: &Terrain,
    textures: &[Option<TextureHandle>],
    blend: bool,
    shading: &TerrainShading,
) -> mint_render::Result<(Vec<MeshHandle>, Vec<Instance>)> {
    let mut handles = Vec::new();
    let mut instances = Vec::new();
    for part in drawn_parts(terrain, blend) {
        let handle = renderer.upload_mesh(&part.mesh)?;
        handles.push(handle);
        instances.push(part_instance(handle, part, textures, shading));
    }
    Ok((handles, instances))
}

#[must_use]
pub fn object_instances(mesh: MeshHandle, proxies: &[ObjectProxy]) -> Vec<Instance> {
    proxies
        .iter()
        .map(|proxy| Instance {
            cull: true,
            transform: proxy.marker_transform(),
            ambient_from_vertex_color: true,
            diffuse_from_vertex_color: true,
            material_emissive: OBJECT_EMISSIVE,
            ..Instance::new(mesh)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_render::MeshData;
    use mint_sage_terrain::{ChunkExtent, TerrainChunk, TileLayout};

    fn part(layer: TerrainLayer) -> TerrainPart {
        TerrainPart {
            layer,
            texture_class: Some(0),
            mesh: MeshData::default(),
        }
    }

    #[test]
    fn the_ground_layers_get_a_sort_level_in_paint_order_and_the_base_layer_does_not() {
        let blend = overlay_sort_level(TerrainLayer::Blend).expect("Blend must be leveled");
        let extra =
            overlay_sort_level(TerrainLayer::ExtraBlend).expect("ExtraBlend must be leveled");
        assert!(
            blend > extra,
            "higher sort_level draws first (further back); Blend must precede ExtraBlend"
        );
        assert_eq!(overlay_sort_level(TerrainLayer::Base), None);
    }

    fn one_chunk_of_each_layer() -> Terrain {
        Terrain {
            width: 2,
            height: 2,
            border_size: 0,
            boundaries: Vec::new(),
            vertical_scale: 1.0,
            chunks: vec![TerrainChunk {
                extent: ChunkExtent {
                    x0: 0,
                    y0: 0,
                    x1: 1,
                    y1: 1,
                },
                parts: vec![
                    part(TerrainLayer::Base),
                    part(TerrainLayer::Blend),
                    part(TerrainLayer::ExtraBlend),
                ],
                bounds: mint_render::Aabb::default(),
            }],
            layout: TileLayout::new(&[]),
            map_cliff_class: None,
        }
    }

    #[test]
    fn the_blended_upload_draws_the_base_layer_and_both_overlays() {
        assert_eq!(drawn_parts(&one_chunk_of_each_layer(), true).count(), 3);
    }

    #[test]
    fn the_unblended_upload_draws_the_base_layer_alone() {
        let terrain = one_chunk_of_each_layer();
        let drawn: Vec<TerrainLayer> = drawn_parts(&terrain, false).map(|p| p.layer).collect();
        assert_eq!(drawn, vec![TerrainLayer::Base]);
    }
}
