use std::time::Duration;

use mint_render::{
    Blend, CustomMaterialHandle, CustomShader, CustomShaderHandle, DepthTest, Instance, MeshHandle,
    Renderer, TextureHandle,
};
use mint_sage_map::{HeightMapData, Object};
use mint_sage_model::{ObjectIndex, Resolver};
use mint_sage_terrain::{build_roads, RoadNetwork, RoadOptions, RoadSkip, RoadStyle};
use mint_vfs::Vfs;

fn road_style(index: Option<&ObjectIndex>, name: &str) -> Option<RoadStyle> {
    let road = index?.road(name)?;
    Some(RoadStyle {
        width: road.width.unwrap_or(0.0),
        width_in_texture: road.width_in_texture.unwrap_or(0.0),
    })
}

pub fn build_road_network(
    height_map: &HeightMapData,
    scale: f32,
    objects: &[Object],
    index: Option<&ObjectIndex>,
    options: &RoadOptions,
) -> RoadNetwork {
    build_roads(height_map, scale, objects, options, |name| {
        road_style(index, name)
    })
}

pub fn road_texture_name(index: Option<&ObjectIndex>, name: &str) -> Option<String> {
    index?.road(name)?.texture.clone()
}

pub(crate) fn report_roads(
    network: &RoadNetwork,
    index: Option<&ObjectIndex>,
    options: &RoadOptions,
    drawn: bool,
    elapsed: Duration,
) {
    let stats = &network.stats;
    println!();
    println!(
        "roads: {} placement(s), {} segment(s), {} drawn, {} skipped, in {:.1} ms",
        stats.placements,
        stats.segments,
        stats.drawn,
        stats.skipped_total(),
        elapsed.as_secs_f64() * 1000.0
    );
    for (reason, count) in &stats.skipped {
        let why = match reason {
            RoadSkip::UndeclaredType => "no Road declaration of that name",
            RoadSkip::UnusableWidth => "the declaration's width is missing or not positive",
            RoadSkip::NonFinite => "an endpoint coordinate is not finite",
        };
        println!("  skipped {count:>5}  {why}");
    }
    let [free, two_way, tee, cross, more] = stats.endpoint_classes;
    println!(
        "  endpoints: {free} free, {two_way} two-way ({} mitred, {} left square by the cap), \
         {tee} tee/Y, {cross} crossroads, {more} of five or more",
        stats.mitred, stats.mitre_capped
    );
    println!(
        "  {} zero-length (nudged), {} touching ground off the heightmap, {} name mismatch(es)",
        stats.degenerate, stats.off_map, stats.name_mismatch
    );
    println!(
        "  {} road type(s), {} vertices, {} triangles",
        network.meshes.len(),
        stats.vertices,
        stats.triangles
    );
    for road in &network.meshes {
        println!(
            "    {:<28} width {:>6.1} x {:<5} {:>4} segment(s)  texture {}",
            road.name,
            road.style.width,
            road.style.width_in_texture,
            road.segments,
            road_texture_name(index, &road.name).unwrap_or_else(|| "(none)".to_owned())
        );
    }
    for name in &network.undeclared {
        println!("  UNDECLARED road type {name} - placed but no Road block declares it");
    }
    println!("  {}", road_state_line(options, drawn));
    println!(
        "  junction pieces: {} tee of {tee} three-way, {} crossroads of {cross} four-way",
        stats.tees, stats.crossroads
    );
    let square = stats
        .junction_y
        .saturating_add(stats.junction_h)
        .saturating_add(stats.junction_degenerate)
        .saturating_add(more);
    if square > 0 {
        println!(
            "  note: {square} junction(s) get no piece - {} Y, {} slanted H, {} unusable, \
             {more} of five or more; those ends stay square",
            stats.junction_y, stats.junction_h, stats.junction_degenerate
        );
    }
}

pub fn road_state_line(options: &RoadOptions, drawn: bool) -> String {
    format!(
        "roads = {} (n), lift = {} world units (b), height = {} (h), tessellation = {} (v), chain order = {} (c)",
        if drawn { "on" } else { "off" },
        options.lift,
        options.height.label(),
        options.tessellation.label(),
        options.chain_order.label()
    )
}

pub struct RoadScene {
    pub handles: Vec<MeshHandle>,
    pub textures: Vec<Option<TextureHandle>>,
    pub shader: Option<CustomShaderHandle>,
    pub materials: Vec<Option<CustomMaterialHandle>>,
    pub failures: Vec<String>,
}

pub fn upload_roads(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    index: Option<&ObjectIndex>,
    network: &RoadNetwork,
) -> mint_render::Result<RoadScene> {
    let shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme road (mint's lit term, plus the shroud)",
        source: mint_sage_terrain::ROAD_SHADER_WGSL,
        texture_slots: mint_sage_terrain::ROAD_TEXTURE_SLOTS,
        uniform_size: 0,
        scene_texture_slots: mint_sage_terrain::ROAD_SCENE_TEXTURE_SLOTS,
        scene_uniform_size: mint_sage_terrain::ROAD_SCENE_UNIFORM_SIZE,
        writes_alpha: true,
        scene_depth: false,
        scene_color: false,
        ..CustomShader::default()
    }) {
        Ok(handle) => Some(handle),
        Err(error) => {
            println!("  ROAD SHADER FAILED TO REGISTER: {error}");
            None
        }
    };
    if let Err(error) =
        crate::bind_shroud_scene(renderer, shader, None, crate::NO_SHROUD_EXTENT, 0.0)
    {
        println!("  road shroud binding: {error}");
    }
    let mut scene = RoadScene {
        handles: Vec::with_capacity(network.meshes.len()),
        textures: Vec::with_capacity(network.meshes.len()),
        shader,
        materials: Vec::with_capacity(network.meshes.len()),
        failures: Vec::new(),
    };
    let compressed = renderer.supports_block_compression();
    for road in &network.meshes {
        scene.handles.push(renderer.upload_mesh(&road.mesh)?);
        let Some(name) = road_texture_name(index, &road.name) else {
            scene
                .failures
                .push(format!("{}: names no Texture", road.name));
            scene.textures.push(None);
            continue;
        };
        match mint_sage_model::load_texture(vfs, resolver, &name, compressed) {
            Ok(loaded) => scene
                .textures
                .push(Some(renderer.upload_texture(&loaded.data)?)),
            Err(error) => {
                scene
                    .failures
                    .push(format!("{} -> {name}: {error}", road.name));
                scene.textures.push(None);
            }
        }
    }
    for slot in 0..scene.handles.len() {
        let texture = scene.textures.get(slot).copied().flatten();
        let built = shader.and_then(|shader| {
            match renderer.create_custom_material(shader, &[texture], &[]) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    scene.failures.push(format!("road material: {error}"));
                    None
                }
            }
        });
        scene.materials.push(built);
    }
    for line in &scene.failures {
        println!("  ROAD TEXTURE FAILED {line}");
    }
    Ok(scene)
}

#[must_use]
pub fn road_instances(scene: &RoadScene) -> Vec<Instance> {
    scene
        .handles
        .iter()
        .enumerate()
        .map(|(slot, handle)| Instance {
            cull: true,
            texture: scene.textures.get(slot).copied().flatten(),
            custom_material: scene.materials.get(slot).copied().flatten(),
            blend: Blend::AlphaBlend,
            depth_test: DepthTest::NearerOrEqual,
            surface_overlay: true,
            ..Instance::new(*handle)
        })
        .collect()
}
