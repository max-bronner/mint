use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use mint_app::{App, InputEvent, Key, OrbitCamera, PointerButton, WindowConfig, SIMULATION_HZ};
use mint_render::{
    directional_light_view_proj, Aabb, Blend, Camera, CustomMaterialHandle, CustomShader,
    CustomShaderHandle, DepthDirection, DepthTest, DirectionalLight, Instance, MeshData,
    MeshHandle, Renderer, Scene, ShadowMapHandle, ShadowSample, TextureHandle, Vec3,
};
use mint_sage_big::BigArchive;
use mint_sage_ini::{FxListIndex, ParticleIndex};
use mint_sage_map::{BlendTileData, GlobalLighting, HeightMapData, MapFile, Object, TimeOfDay};
use mint_sage_model::{
    load_animation, CachedModel, Completion, LoadedAnimation, ModelAnimation, ModelCache,
    NoModelReason, ObjectIndex, PartPose, Resolver, Template,
};
use mint_sage_particle::{BillboardMode, DetailLevel, FlipbookMode, ShapeMode, SortMode, WindMode};
use mint_sage_scene::{
    build_road_network, drawn_parts, load_sky_cloud, object_instances, road_instances,
    road_state_line, tuning_ceiling, tuning_skip_mask, upload_roads, upload_terrain,
    upload_terrain_shading, RoadScene, TerrainShading,
};
use mint_sage_terrain::present::{upload_class_normal_maps, upload_class_textures};
use mint_sage_terrain::{
    body_absorption, load_ramp, place_objects, proxy_mesh, river_scroll_offset, shade_body,
    surface_waves, BodyShading, DepthRange, ObjectProxy, RampSource, RiverFlow, RiverLook,
    RiverNetwork, RiverRibbon, RiverStage, RiverUniforms, RoadChainOrder, RoadHeight, RoadNetwork,
    RoadOptions, RoadSegments, RoadTessellation, SkyDraw, SkyScene, StepsPerCell, Terrain,
    TerrainCloud, TerrainMacro, TerrainRelief, TerrainShadows, TerrainSun, WaterAlphaSource,
    WaterDepth, WaterDepthMode, WaterDistortion, WaterDrift, WaterHighlights, WaterNormal,
    WaterReflection, WaterReflectionComposite, WaterReflectionSource, WaterRefraction,
    WaterShaderParams, WaterSurface, WaterSurfaceLook, WaterTechnique, WaterTint,
    WaterTransparency, WaterTuning, WaterUniforms, WaveDraw, WaveField, WaveLook, WavePca,
    WaveScale, WaveSurface, WaveUniforms, RAMP_FIT_TOLERANCE, RIVER_EDGE_BAND, RIVER_RIPPLE_REPEAT,
    RIVER_RIPPLE_SLOPE, RIVER_SCENE_TEXTURE_SLOTS, RIVER_SCENE_UNIFORM_SIZE, RIVER_SCROLL_UNIT,
    RIVER_SHADER_WGSL, RIVER_SPARKLE_GAIN, RIVER_SPECULAR_GAIN, RIVER_TEXTURE_SLOTS,
    RIVER_UNIFORM_SIZE, ROAD_LIFT_HEIGHT_STEP_EIGHTH, ROAD_LIFT_XY_128, WATER_BUMP_SLOPE,
    WATER_FRESNEL_F0, WATER_REFRACTION_FALLOFF, WATER_REFRACTION_SCALE, WATER_SCENE_TEXTURE_SLOTS,
    WATER_SCENE_UNIFORM_SIZE, WATER_SHADER_WGSL, WATER_TEXTURE_SLOTS, WATER_TINT,
    WATER_UNIFORM_SIZE, WATER_UNIT_SLOPE, WATER_UV_BASE, WAVE_SCENE_TEXTURE_SLOTS,
    WAVE_SCENE_UNIFORM_SIZE, WAVE_SHADER_WGSL, WAVE_TEXTURE_SLOTS, WAVE_UNIFORM_SIZE,
};
use mint_vfs::Vfs;

mod alias_harness;
mod camera;
mod cli;
mod inspector;
mod lighting;
mod map_lookup;
mod object_assembly;
mod object_compare;
mod particles;
mod reflection;
mod shadow;
mod terrain_shading;
mod tuning;
mod water_scene;

use alias_harness::*;
use camera::*;
use cli::*;
use inspector::{CellContext, ClassNote, RoadReport};
use lighting::*;
use map_lookup::*;
use object_assembly::*;
use object_compare::*;
use particles::{BoneReport, BoneRung, FollowingBone, MapGround, ParticleScene};
use reflection::*;
use shadow::*;
use terrain_shading::*;
use water_scene::*;

const MAX_PLANAR_PLANES: usize = 4;

const RIVER_PLANE_TOLERANCE: f32 = 4.0;

const PLANAR_HEIGHT_EPSILON: f32 = 0.01;

const PLANAR_CLIP_BIAS: f32 = 2.0;

const DEFAULT_VERTICAL_SCALE: f32 = 0.039_062_5;

const ROAD_LIFTS: [f32; 6] = [
    ROAD_LIFT_XY_128,
    ROAD_LIFT_HEIGHT_STEP_EIGHTH,
    0.0,
    0.312_5,
    1.25,
    5.0,
];

const ROAD_TESSELLATIONS: [RoadTessellation; 4] = [
    RoadTessellation::Grid(StepsPerCell::Two),
    RoadTessellation::Ribbon,
    RoadTessellation::Grid(StepsPerCell::One),
    RoadTessellation::Grid(StepsPerCell::Four),
];

const ROAD_CHAIN_ORDERS: [RoadChainOrder; 3] = [
    RoadChainOrder::Chained,
    RoadChainOrder::AsFile,
    RoadChainOrder::Reversed,
];

const OFFSCREEN_SIZE: (u32, u32) = (1280, 720);

const EXPOSURE_STEP: f32 = 1.1;

const SCALE_STEP: f32 = 1.25;

const PAN_PER_SECOND: f32 = 0.9;

const CLICK_SLOP: f32 = 3.0;

const DRAG_PAN_PER_PIXEL: f32 = 0.0015;

const MIN_CAMERA_DISTANCE: f32 = 0.5;

const MIN_PAN_PER_SECOND: f32 = 60.0;

const PAN_MARGIN: f32 = 0.25;

fn main() -> ExitCode {
    let tuning_file = tuning::open(tuning_path_arg().as_deref());
    let options = match parse_args(tuning_file.tuning()) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match run(options, tuning_file) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn scene_options(options: &Options) -> mint_sage_scene::SceneOptions {
    mint_sage_scene::SceneOptions {
        cliffs: options.cliffs,
        scale: options.scale,
        mips: options.mips,
        objects: options.objects,
        proxies: options.proxies,
        object_animation_label: options.object_animation.label(),
        roads: options.roads,
        road_options: options.road_options,
        alpha_source: options.alpha_source,
        water_coarse: options.water_coarse,
        water_label: options.water.label(),
        water_on: options.water.on(),
        rivers_label: options.rivers.label(),
        rivers_on: options.rivers.on(),
        wave_look: options.wave_look,
        focus: options.focus,
        particle_budget: options.particle_budget,
        particle_name: options.particle_name.clone(),
        fx_name: options.fx_name.clone(),
    }
}

fn run(options: Options, tuning_file: tuning::TuningFile) -> Result<(), String> {
    let Some(primary) = options.installs.first() else {
        return Err("no installation given".to_owned());
    };
    let mut vfs = Vfs::new();
    for install in options.installs.iter().skip(1).rev().chain([primary]) {
        let mut archives = 0usize;
        for path in mint_sage_big::archives_under(install) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                archives = archives.saturating_add(1);
            }
        }
        println!("mounted {archives} archive(s) from {}", install.display());
    }
    println!("{} entries visible\n", vfs.len());

    let matches = find_maps(primary, &options.needle);
    let Some(chosen) = matches.first() else {
        return Err(format!(
            "no .map/.bse entry matching {:?} found under {}",
            options.needle,
            primary.display()
        ));
    };
    println!("{} match(es) for {:?}:", matches.len(), options.needle);
    for m in matches.iter().take(20) {
        println!(
            "  {} ({})",
            m.entry_name,
            m.archive
                .as_ref()
                .map_or_else(|| "loose file".to_owned(), |a| a.display().to_string())
        );
    }
    if matches.len() > 20 {
        println!("  ... and {} more", matches.len().saturating_sub(20));
    }
    println!("using {}\n", chosen.entry_name);

    let raw = match &chosen.archive {
        Some(archive) => read_entry(archive, &chosen.entry_name)?,
        None => {
            std::fs::read(&chosen.entry_name).map_err(|e| format!("{}: {e}", chosen.entry_name))?
        }
    };
    let map = MapFile::parse(&raw).map_err(|e| format!("{}: {e}", chosen.entry_name))?;

    let scene = mint_sage_scene::load_map_scene(
        &vfs,
        map,
        chosen.entry_name.clone(),
        &scene_options(&options),
        tuning_file.tuning(),
        None,
    )
    .map_err(|e| e.to_string())?;
    let mint_sage_scene::MapScene {
        height_map,
        blend_tile,
        terrain,
        loaded,
        normal_maps,
        map,
        proxies,
        object_index,
        particle_index,
        particle_lists,
        particle_name,
        fx_name,
        particle_ceiling,
        particle_skip_mask,
        road_network,
        road_segments,
        water_surface,
        water_transparency,
        water_shipped_fade,
        river_network,
        wave_field,
        wave_surface,
        classes,
        map_label,
        focus,
        resolver,
    } = scene;

    if !options.inspect.is_empty() || !options.pick.is_empty() {
        let notes = template_notes(object_index.as_ref(), &map.objects);
        let cells = CellContext {
            height_map: &height_map,
            blend_tile: &blend_tile,
            terrain: &terrain,
            classes: &classes,
            map_label: &map_label,
            draw_blend: options.blend,
            objects: &map.objects,
            proxies: &proxies,
            templates: Some(&notes),
            roads: Some(RoadReport {
                segments: &road_segments,
                network: &road_network,
                options: options.road_options,
            }),
        };
        for (x, y) in &options.inspect {
            cells.report(*x, *y, None);
        }
        if !options.pick.is_empty() {
            let mut camera = frame_terrain_camera(
                &terrain,
                OFFSCREEN_SIZE.0 as f32 / OFFSCREEN_SIZE.1 as f32,
                options.pitch,
                options.zoom,
            );
            camera.yaw = options.yaw;
            if let Some(target) = focus {
                camera.target = target;
            }
            for (px, py) in &options.pick {
                cells.pick(&camera.camera(), OFFSCREEN_SIZE, *px, *py);
            }
        }
        return Ok(());
    }

    let rigs = [
        map_rigs(
            &map.global_lighting,
            false,
            options.terrain_overbright,
            options.terrain_ceiling,
        ),
        map_rigs(
            &map.global_lighting,
            true,
            options.terrain_overbright,
            options.terrain_ceiling,
        ),
    ];
    let time = options
        .time
        .or(map.global_lighting.time_of_day)
        .unwrap_or_else(|| {
            println!(
                "GlobalLighting names time of day {}, which is outside 1-4; using afternoon",
                map.global_lighting.time_of_day_code
            );
            TimeOfDay::Afternoon
        });
    report_lighting(
        &map.global_lighting,
        &rigs,
        time,
        options.map_light,
        options.time.is_none(),
        options.alt_set,
        options.terrain_overbright,
        options.terrain_ceiling,
    );
    let rig = current_rig(&rigs, time, options.map_light, options.alt_set);

    if let Some(frames) = options.alias {
        return alias_check(&terrain, &loaded, &options, focus, frames, &rig);
    }
    println!(
        "map cliff texture (cliff record 0) = {}",
        map_cliff_note(&terrain, &blend_tile)
    );
    if options.screenshot.is_some() || options.frames.is_some() || options.sweep.is_some() {
        return offscreen(
            &terrain,
            &loaded,
            options.pitch,
            options.yaw,
            options.zoom,
            options.blend,
            options.screenshot.as_deref(),
            options.frames,
            options.sweep,
            focus,
            options.aniso,
            options.frustum_cull,
            if options.standard_z {
                DepthDirection::Standard
            } else {
                DepthDirection::Reversed
            },
            &rig,
            &RoadInput {
                network: &road_network,
                draw: options.roads,
            },
            &WaterInput {
                surface: &water_surface,
                mode: options.water,
                reflection: options.reflection,
                source: options.reflection_source,
                composite: options.reflection_composite,
                highlights: options.highlights,
                tint: options.water_tint,
                depth: options.water_depth,
                distortion: options.distortion,
                transparency: water_transparency,
                vertical_scale: options.scale,
                seconds: options.water_time,
                waves: &wave_field,
                wave_look: options.wave_look,
                wave_surface: &wave_surface,
                tuning: tuning_file.tuning(),
            },
            &RiverInput {
                network: &river_network,
                mode: options.rivers,
                look: options.river_look,
                seconds: options.water_time,
            },
            &ObjectInput {
                vfs: &vfs,
                resolver: &resolver,
                index: object_index.as_ref(),
                objects: &map.objects,
                proxies: &proxies,
                draw: options.objects,
                markers: options.proxies,
                animation: options.object_animation,
                material: options.object_material,
                sway: options.sway,
            },
            &TerrainInput {
                environment: map.environment.as_ref(),
                no_cloud_factor: map.global_lighting.no_cloud_factor,
                cloud: options.terrain_cloud,
                macro_layer: options.terrain_macro,
                normal_maps: &normal_maps,
                relief: options.terrain_relief,
                sun: options.terrain_sun,
                overbright: map.global_lighting.overbright,
                seconds: options.water_time,
                sky: options.sky,
                shadows: options.shadows,
                shadow_fit: options.shadow_fit,
                shadow_alpha: options.shadow_alpha,
                reflection_cull: options.reflection_cull,
            },
            &ParticleInput {
                index: &particle_index,
                lists: &particle_lists,
                height_map: &height_map,
                name: &particle_name,
                draw: options.particles,
                keys: options.particle_keys,
                seconds: options.particle_time,
                bones: options.particle_bones,
                motion: options.particle_motion,
                sort: options.particle_sort,
                billboard: options.particle_billboard,
                max_particles: particle_ceiling,
                skip_mask: particle_skip_mask,
                lod: options.particle_lod,
                flipbook: options.particle_flipbook,
                shape: options.particle_shape,
                compound: options.particle_compound,
                wind: options.particle_wind,
                fx: &fx_name,
            },
        );
    }
    println!("\ncontrols:");
    println!("  middle drag rotate (vertical drag is inverted, by preference)");
    println!("  right drag  pan the view, dragging the ground under the pointer");
    println!("  left click  print everything mint knows about the cell under it");
    println!("  scroll      zoom");
    println!(
        "  `           open/close the debug console (D1's I7) - every other key below is \
         swallowed by it while it is open"
    );
    println!(
        "  [           scale terrain height down (x{:.2})",
        1.0 / SCALE_STEP
    );
    println!("  ]           scale terrain height up (x{SCALE_STEP:.2})");
    println!(
        "  r           reset the scale to its starting value ({})",
        options.scale
    );
    println!("  t           step through the map's four times of day");
    println!("  y           switch between the two candidate terrain light sets");
    println!(
        "  j / k       dial the lighting exposure down/up (x{:.2}), u resets",
        EXPOSURE_STEP
    );
    println!("  1 / 2 / 3 / 4  the ambient term, the sun and the two accent lights on/off");
    println!("  arrows      pan the view the way the camera is facing (WASD no longer pans)");
    println!("  o           objects on/off");
    println!("  p           proxy markers instead of the resolved models");
    println!("  n           roads on/off");
    println!("  NO WATER KEY IS BOUND. Every water comparison lives in the tuning");
    println!("              file now, at the path printed above, and it is re-read");
    println!("              while this runs - so a mode changes with the camera, the");
    println!("              time of day and the clock exactly where they were. Its");
    println!("              third section is the sixteen rungs that used to be on");
    println!("              g q f 5 6 7 8 w 9 0 i x d s a e m, each with the same");
    println!("              named values and a line saying what it compares. The");
    println!("              first two sections are every scale factor and constant.");
    println!("              The matching --flags still exist, for a scripted still.");
    println!("  b           step the road lift through {ROAD_LIFTS:?} world units");
    println!("  h           road height rule: each vertex on the drawn ground, or a station's max");
    println!("  v           step the road tessellation ladder");
    println!("  c           step the road chain order");
    println!("  z           depth convention: reversed-Z (the default) or standard-Z, and");
    println!("              print the ulps of depth separating the road from the terrain");
    println!("  q           particles on/off");
    println!("  e           particle colour and alpha keys on/off (step 4 against step 3)");
    println!("  x           particle-bones off | default | all (step 7 - the map's own)");
    println!("  s           particle-sort none | level | depth (step 10 - no measurement");
    println!("              decides it, so this key is the whole of the decision)");
    println!("  d           particle-billboard data | screen | ground (step 9)");
    println!("  l           particle-lod off | verylow | low | medium | high | ultrahigh");
    println!("              (step 15 - which whole systems MinParticlePriority admits)");
    println!("  i           particle-flipbook off | frame | life | blend (step 12 - the 103");
    println!("              GpuDraw systems' sprite sheets. `off` is the static picture,");
    println!("              `frame` and `life` are the two readings of SpeedMultiplier that");
    println!("              the research does not decide between, and `blend` adds the");
    println!("              cross-fade the GPU shader does)");
    println!("  5           particle-shape data | sprite (step 13 - each system as its");
    println!("              Type asks, or every one of them as the plain quad batch they");
    println!("              were all drawn as before it: a STREAK as loose sprites, a");
    println!("              VOLUME_PARTICLE as one copy, a DRAWABLE as a white square)");
    println!("  w           particle-compound off | on (step 14 - SlaveSystem and");
    println!("              PerParticleAttachedSystem. `on` is the one that multiplies");
    println!("              system count, so this is the key that shows what it costs)");
    println!("  m           particle motion on/off (step 6 - gravity, drift, damping, size)");
    println!("  .           particle-wind retail | off | doubled (step 11 - the ping-pong");
    println!("              sweep and the distance falloff, on the 234 systems whose");
    println!("              WindMotion moves. `retail` applies WindStrength as written;");
    println!("              `doubled` is what mint drew while it scaled EA's 2.0)");
    println!("  0           fire the FXList named in the tuning file (step 8). There are");
    println!("              no game events yet, so this key is the only thing that fires");
    println!("              one - and the effects with a SortLevel above 1 are reachable");
    println!("              no other way, so this is what makes the s key mean anything");
    println!("  g           respawn the named particle system at what the camera looks at");
    println!("              The system is named in the tuning file, `particle.system` -");
    println!("              a key can step a ladder, it cannot pick one of 1,923 names.");

    let config = WindowConfig {
        title: format!("mint - terrain - {}", chosen.entry_name),
        ..WindowConfig::default()
    };
    let starting_scale = options.scale;
    let pitch = options.pitch;
    let zoom = options.zoom;
    let blend = options.blend;
    let aniso = options.aniso;
    let depth = if options.standard_z {
        DepthDirection::Standard
    } else {
        DepthDirection::Reversed
    };
    let map_light = options.map_light;
    let alt_set = options.alt_set;
    let draw_objects = options.objects;
    let draw_models = !options.proxies;
    let draw_roads = options.roads;
    let road_options = options.road_options;
    let water_draw = options.water;
    let water_reflection = options.reflection;
    let water_reflection_source = options.reflection_source;
    let water_reflection_composite = options.reflection_composite;
    let water_highlights = options.highlights;
    let water_tint = options.water_tint;
    let water_depth = options.water_depth;
    let water_distortion = options.distortion;
    let river_draw = options.rivers;
    let river_look = options.river_look;
    let objects = map.objects;
    let frustum_cull = options.frustum_cull;
    mint_app::run(&config, move |renderer| {
        renderer.set_anisotropy(aniso);
        renderer.set_depth_direction(depth);
        renderer.set_frustum_culling(frustum_cull);
        let textures = upload_class_textures(renderer, &loaded)?;
        let normal_handles = upload_class_normal_maps(renderer, &normal_maps)?;
        let mut terrain_shading = upload_terrain_shading(
            renderer,
            &vfs,
            &resolver,
            map.environment.as_ref(),
            map.global_lighting.no_cloud_factor,
            map.global_lighting.overbright,
            terrain.bounds(),
            &textures,
            &normal_handles,
            options.terrain_cloud,
            options.terrain_macro,
            tuning_file.tuning(),
        );
        terrain_shading.modulation.relief = options.terrain_relief;
        terrain_shading.modulation.sun = options.terrain_sun;
        let sky = if options.sky.is_on() {
            let (cloud, failures) = load_sky_cloud(renderer, &vfs, &resolver, true);
            for line in &failures {
                println!("  SKY {line}");
            }
            match SkyScene::load(renderer, cloud, options.sky, 0.0, tuning_file.tuning()) {
                Ok(sky) => {
                    println!("{}", sky.report());
                    Some(sky)
                }
                Err(error) => {
                    println!("  SKY {error}");
                    None
                }
            }
        } else {
            println!("sky = {} (mode.sky)", options.sky.label());
            None
        };
        let (handles, instances) =
            upload_terrain(renderer, &terrain, &textures, blend, &terrain_shading)?;
        let proxy_handle = renderer.upload_mesh(&proxy_mesh())?;
        let object_instances = object_instances(proxy_handle, &proxies);
        let mut model_scene = object_index
            .as_ref()
            .map(|index| build_object_scene(renderer, &vfs, &resolver, index, &objects));
        let model_instances = model_scene
            .as_mut()
            .map(|scene| model_instances(scene, &proxies))
            .unwrap_or_default();
        let road_scene = upload_roads(
            renderer,
            &vfs,
            &resolver,
            object_index.as_ref(),
            &road_network,
        )?;
        let road_instances = road_instances(&road_scene);
        let water_scene = upload_water(
            renderer,
            &vfs,
            &resolver,
            &water_surface,
            water_transparency,
            starting_scale,
            water_tint,
            water_reflection,
            water_reflection_source,
            water_reflection_composite,
            water_highlights,
            water_depth,
            water_distortion,
            tuning_file.tuning(),
            &wave_surface,
        )?;
        let water_instances = water_instances(&water_scene, &water_surface, water_draw, water_tint);
        let river_scene = upload_rivers(
            renderer,
            &vfs,
            &resolver,
            &river_network,
            &water_surface,
            &water_scene.planes,
            river_look,
            water_distortion.refraction,
            tuning_file.tuning(),
        )?;
        let river_instances = river_instances(
            &river_scene,
            &river_network,
            river_draw,
            0.0,
            river_scroll_factor(tuning_file.tuning()),
        );
        let wave_scene = upload_waves(
            renderer,
            &vfs,
            &resolver,
            &wave_field,
            options.wave_look,
            tuning_file.tuning(),
        )?;
        let wave_instances = wave_instances(&wave_scene, &wave_field, options.wave_look);

        let shadows = if options.shadows == TerrainShadows::On {
            ShadowPass::new(renderer, tuning_file.tuning())
        } else {
            println!("shadows = off (mode.shadows)");
            None
        };

        let mut plane_bounds =
            water_sampler_bounds(&water_scene.planes, &water_scene, &water_surface);
        for (index, bounds) in
            river_sampler_bounds(&water_scene.planes, &river_scene, &river_network)
                .into_iter()
                .enumerate()
        {
            if let Some(slot) = plane_bounds.get_mut(index) {
                slot.union(&bounds);
            }
        }

        let (width, height) = renderer.size();
        let aspect = width as f32 / (height.max(1) as f32);
        let mut camera = frame_terrain_camera(&terrain, aspect, pitch, zoom);
        if let Some(scene) = &model_scene {
            extend_far_reach(&mut camera, &object_world_bounds(scene, &proxies));
        }
        let pan_limit = pan_limit(&terrain);

        let console_texture = renderer
            .upload_texture(
                &mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).ok_or_else(
                    || "unreachable: a 1x1 rgba8 buffer is always the declared size".to_owned(),
                )?,
            )
            .map_err(|e| e.to_string())?;
        let console_atlas = mint_ui::GlyphAtlas::new(mint_ui::GlyphAtlasConfig::default_config())
            .map_err(|e| e.to_string())?;

        let mut viewer = Viewer {
            height_map,
            blend_tile,
            terrain,
            handles,
            instances,
            camera,
            buttons: [false; 2],
            pointer: None,
            pending_frustum: 0,
            pending_shadow_fit: 0,
            pending_shadow_alpha: 0,
            pending_reflection_cull: 0,
            pending_scale: Vec::new(),
            starting_scale,
            pan_held: [false; 4],
            pan_limit,
            viewport: (width, height),
            classes,
            map_label,
            draw_blend: blend,
            rigs,
            global_lighting: map.global_lighting.clone(),
            terrain_overbright: options.terrain_overbright,
            terrain_ceiling: options.terrain_ceiling,
            time,
            map_light,
            alt_set,
            exposure: 1.0,
            overbright: map.global_lighting.overbright,
            mask: LightMask::default(),
            click_anchor: None,
            pending_time: 0,
            pending_set: 0,
            objects,
            proxies,
            proxy_handle,
            object_instances,
            model_scene,
            model_instances,
            model_span: 0..0,
            object_animation: options.object_animation,
            object_material: options.object_material,
            pending_object_material: 0,
            pending_material_step: 0,
            sway: options.sway,
            pending_sway: 0,
            sway_seconds: 0.0,
            sway_written: false,
            object_seconds: 0.0,
            material_seconds: 0.0,
            object_last_frame: std::time::Instant::now(),
            pending_object_animation: 0,
            draw_models,
            drawn: Vec::new(),
            draw_objects,
            object_index,
            road_network,
            road_segments,
            road_scene,
            road_instances,
            road_options,
            draw_roads,
            water_surface,
            water_scene,
            water_instances,
            water_span: 0..0,
            river_network,
            river_scene,
            reflected: Vec::new(),
            river_instances,
            river_span: 0..0,
            river_draw,
            river_look,
            wave_field,
            wave_surface,
            wave_scene,
            wave_instances,
            wave_span: 0..0,
            wave_look: options.wave_look,
            water_seconds: 0.0,
            water_last_frame: std::time::Instant::now(),
            water_moving: options.water_moving,
            water_draw,
            water_reflection,
            water_source: water_reflection_source,
            water_composite: water_reflection_composite,
            water_highlights,
            water_drift: tuning_file.tuning().drift_mode(),
            water_map_depth: water_transparency.map_depth,
            water_tint,
            water_depth,
            water_distortion,
            water_transparency,
            water_shipped_fade,
            pending_road: Vec::new(),
            pending_depth: 0,
            vfs,
            resolver,
            particle_scene: {
                let mut scene = ParticleScene::new(particle_index, particle_lists, &particle_name);
                scene.set_drawn(options.particles);
                scene.set_keys(options.particle_keys);
                scene.set_motion(options.particle_motion);
                scene.set_billboard(options.particle_billboard);
                scene.set_sort(options.particle_sort);
                scene.set_detail(options.particle_lod);
                scene.set_flipbook(options.particle_flipbook);
                scene.set_shape(options.particle_shape);
                scene.set_compound(options.particle_compound);
                scene.set_wind(options.particle_wind);
                scene.set_fx_name(&fx_name);
                scene.set_budget_numbers(particle_ceiling, particle_skip_mask);
                scene.set_bones(options.particle_bones);
                scene
            },
            particle_last_frame: std::time::Instant::now(),
            pending_particles: 0,
            pending_particle_keys: 0,
            particle_bones: options.particle_bones,
            pending_particle_bones: 0,
            pending_particle_motion: 0,
            pending_particle_smooth: 0,
            pending_particle_sort: 0,
            pending_particle_billboard: 0,
            pending_particle_lod: 0,
            pending_particle_flipbook: 0,
            pending_particle_shape: 0,
            pending_particle_compound: 0,
            pending_particle_wind: 0,
            pending_particle_fx: 0,
            pending_particle_attach: 1,
            pending_particle_spawn: u32::from(!particle_name.is_empty()),
            composed_len: 0,
            tuning_file,
            terrain_shading,
            sky,
            sky_slot: None,
            sky_draw: options.sky,
            terrain_cloud: options.terrain_cloud,
            terrain_macro: options.terrain_macro,
            shadows,
            shadow_draw: options.shadows,
            shadow_fit: options.shadow_fit,
            shadow_alpha: options.shadow_alpha,
            reflection_cull: options.reflection_cull,
            plane_bounds,
            console: mint_ui::Console::new(),
            console_field: mint_ui::TextInput::new(
                mint_ui::WidgetId(1),
                mint_ui::Rect {
                    min: [0.0, 0.0],
                    max: [0.0, 0.0],
                },
            ),
            console_state: mint_ui::WidgetState::new(),
            console_texture,
            console_atlas,
        };
        viewer.compose();
        Ok(viewer)
    })
    .map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
fn offscreen(
    terrain: &Terrain,
    loaded: &mint_sage_terrain::ClassTextures,
    pitch: f32,
    yaw: f32,
    zoom: f32,
    blend: bool,
    path: Option<&Path>,
    frames: Option<u32>,
    sweep: Option<u32>,
    focus: Option<Vec3>,
    aniso: u16,
    frustum_cull: bool,
    depth: DepthDirection,
    rig: &TerrainRig,
    roads: &RoadInput<'_>,
    water: &WaterInput<'_>,
    rivers: &RiverInput<'_>,
    input: &ObjectInput<'_>,
    ground: &TerrainInput<'_>,
    particles: &ParticleInput<'_>,
) -> Result<(), String> {
    let mut renderer =
        Renderer::offscreen(OFFSCREEN_SIZE.0, OFFSCREEN_SIZE.1).map_err(|e| e.to_string())?;
    renderer.set_anisotropy(aniso);
    renderer.set_depth_direction(depth);
    renderer.set_frustum_culling(frustum_cull);
    let device = renderer.info();
    println!(
        "device: adapter \"{}\" backend {} driver \"{}\"",
        device.adapter, device.backend, device.driver
    );
    println!("anisotropy: {}x", renderer.anisotropy());
    println!(
        "frustum cull = {} (--frustum-cull)",
        if frustum_cull { "on" } else { "off" }
    );
    let textures = upload_class_textures(&mut renderer, loaded).map_err(|e| e.to_string())?;
    let normal_handles =
        upload_class_normal_maps(&mut renderer, ground.normal_maps).map_err(|e| e.to_string())?;
    let mut terrain_shading = upload_terrain_shading(
        &mut renderer,
        input.vfs,
        input.resolver,
        ground.environment,
        ground.no_cloud_factor,
        ground.overbright,
        terrain.bounds(),
        &textures,
        &normal_handles,
        ground.cloud,
        ground.macro_layer,
        water.tuning,
    );
    terrain_shading.modulation.relief = ground.relief;
    terrain_shading.modulation.sun = ground.sun;
    terrain_shading.push(&mut renderer, water.tuning, ground.seconds);
    let (_handles, mut instances) =
        upload_terrain(&mut renderer, terrain, &textures, blend, &terrain_shading)
            .map_err(|e| e.to_string())?;
    let terrain_instances = instances.len();
    let mut road_instance_count = 0usize;
    if roads.draw {
        let scene = upload_roads(
            &mut renderer,
            input.vfs,
            input.resolver,
            input.index,
            roads.network,
        )
        .map_err(|e| e.to_string())?;
        let built = road_instances(&scene);
        road_instance_count = built.len();
        instances.extend(built);
    }
    let water_block_start = instances.len();
    let mut water_planes: Vec<(f32, mint_render::RenderTargetHandle)> = Vec::new();
    let mut plane_bounds: Vec<Aabb> = Vec::new();
    if water.mode.on() && !water.surface.bodies.is_empty() {
        let scene = upload_water(
            &mut renderer,
            input.vfs,
            input.resolver,
            water.surface,
            water.transparency,
            water.vertical_scale,
            water.tint,
            water.reflection,
            water.source,
            water.composite,
            water.highlights,
            water.depth,
            water.distortion,
            water.tuning,
            water.wave_surface,
        )
        .map_err(|e| e.to_string())?;
        for (slot, params) in scene.params.iter().enumerate() {
            let Some(pair) = scene.materials.get(slot).copied().flatten() else {
                continue;
            };
            for (handle, technique) in pair
                .iter()
                .zip([WaterTechnique::Low, WaterTechnique::Default])
            {
                let block = WaterUniforms::new(
                    params,
                    technique,
                    scene.reflection(slot, water.reflection),
                    water.distortion,
                    WaterSurfaceLook {
                        source: scene.source(slot, water.source),
                        composite: water.composite,
                        highlights: water.highlights,
                    },
                    scene.depth(slot, water.depth, water.tuning),
                    water.tuning,
                    water.seconds,
                )
                .to_bytes(water.wave_surface, water.tuning, water.seconds);
                renderer
                    .update_custom_material(*handle, &block)
                    .map_err(|e| e.to_string())?;
            }
        }
        instances.extend(water_instances(
            &scene,
            water.surface,
            water.mode,
            water.tint,
        ));
        water_planes = scene.planes.clone();
        plane_bounds = water_sampler_bounds(&water_planes, &scene, water.surface);
        println!("water depth = {} (--water-depth)", water.depth.label());
        println!(
            "refraction = {} (--refraction), water normal = {} (--water-normal)",
            water.distortion.refraction.label(),
            water.distortion.normal.label()
        );
        println!(
            "water time: {:.2} s (--water-time), reflection = {} (--reflection)",
            water.seconds,
            water.reflection.label()
        );
    }
    if rivers.mode.on() && !rivers.network.ribbons.is_empty() {
        let scene = upload_rivers(
            &mut renderer,
            input.vfs,
            input.resolver,
            rivers.network,
            water.surface,
            &water_planes,
            rivers.look,
            water.distortion.refraction,
            water.tuning,
        )
        .map_err(|e| e.to_string())?;
        for (index, bounds) in river_sampler_bounds(&water_planes, &scene, rivers.network)
            .into_iter()
            .enumerate()
        {
            if let Some(slot) = plane_bounds.get_mut(index) {
                slot.union(&bounds);
            }
        }
        push_river_uniforms(
            &mut renderer,
            &scene,
            rivers.network,
            rivers.look,
            water.distortion.refraction,
            water.tuning,
            rivers.seconds,
        );
        instances.extend(river_instances(
            &scene,
            rivers.network,
            rivers.mode,
            rivers.seconds,
            river_scroll_factor(water.tuning),
        ));
        report_river_shading(&scene, rivers.network, rivers.mode, rivers.look);
    }
    if water.wave_look.draw.on() && !water.waves.areas.is_empty() {
        let scene = upload_waves(
            &mut renderer,
            input.vfs,
            input.resolver,
            water.waves,
            water.wave_look,
            water.tuning,
        )
        .map_err(|e| e.to_string())?;
        push_wave_uniforms(
            &mut renderer,
            &scene,
            water.waves,
            water.wave_look,
            water.tuning,
            water.seconds,
        );
        let built = wave_instances(&scene, water.waves, water.wave_look);
        println!(
            "waves = {} ({} instance(s) drawn), pca = {} (--wave-pca), scale = {} (--wave-scale)",
            water.wave_look.draw.label(),
            built.len(),
            water.wave_look.pca.label(),
            water.wave_look.scale.label()
        );
        instances.extend(built);
    }
    let water_block_end = instances.len();
    let mut object_bounds = Aabb::EMPTY;
    let mut model_scene: Option<ObjectScene> = None;
    let mut model_span = 0..0;
    if input.draw && !input.proxies.is_empty() {
        if input.markers {
            let mesh = renderer
                .upload_mesh(&proxy_mesh())
                .map_err(|e| e.to_string())?;
            instances.extend(object_instances(mesh, input.proxies));
        } else if let Some(index) = input.index {
            let mut scene = build_object_scene(
                &mut renderer,
                input.vfs,
                input.resolver,
                index,
                input.objects,
            );
            object_bounds = object_world_bounds(&scene, input.proxies);
            let mut built = model_instances(&mut scene, input.proxies);
            if input.animation.on() {
                advance_object_animations(
                    &mut scene,
                    &mut renderer,
                    &mut built,
                    input.proxies,
                    input.animation.completion(),
                    ground.seconds,
                );
            }
            advance_object_uv_mappers(&scene, &mut built, input.material.seconds(ground.seconds));
            if let Some(breeze) = input.sway.breeze() {
                advance_object_sway(
                    &scene,
                    &mut built,
                    input.proxies,
                    &breeze.offsets(ground.seconds),
                );
            }
            println!("{}", input.sway.report());
            println!(
                "object animation = {} at t = {:.3} s (--object-animation, --water-time)",
                input.animation.label(),
                ground.seconds
            );
            println!(
                "object material scroll = {} at t = {:.3} s (8, --object-uv, --water-time)",
                input.material.label(),
                ground.seconds
            );
            model_span = instances.len()..instances.len().saturating_add(built.len());
            instances.extend(built);
            model_scene = Some(scene);
        }
    }
    let mut sky = if ground.sky.is_on() {
        let (cloud, failures) = load_sky_cloud(&mut renderer, input.vfs, input.resolver, true);
        for line in &failures {
            println!("  SKY {line}");
        }
        match SkyScene::load(
            &mut renderer,
            cloud,
            ground.sky,
            ground.seconds,
            water.tuning,
        ) {
            Ok(sky) => {
                println!("{}", sky.report());
                Some(sky)
            }
            Err(error) => {
                println!("  SKY {error}");
                None
            }
        }
    } else {
        println!("sky = {} (mode.sky)", ground.sky.label());
        None
    };

    let (width, height) = renderer.size();
    let aspect = width as f32 / (height.max(1) as f32);
    let mut camera = frame_terrain_camera(terrain, aspect, pitch, zoom);
    camera.yaw = yaw;
    if let Some(target) = focus {
        camera.target = target;
    }
    extend_far_reach(&mut camera, &object_bounds);
    let eye = camera.camera().eye;
    println!(
        "camera resolved: eye ({:.3}, {:.3}, {:.3}) target ({:.3}, {:.3}, {:.3}) \
         pitch {pitch} yaw {yaw} distance {:.3} (--pitch --yaw --zoom --focus)",
        eye.x, eye.y, eye.z, camera.target.x, camera.target.y, camera.target.z, camera.distance
    );
    if let Some(sky) = &sky {
        instances.push(sky.instance(camera.camera().eye));
    }

    report_offscreen_depth(
        &camera.camera(),
        &terrain.bounds(),
        aspect,
        depth,
        RoadOptions::default().lift,
    );

    let shadows = match ground.shadows {
        TerrainShadows::On => ShadowPass::new(&mut renderer, water.tuning),
        TerrainShadows::Off => {
            println!("shadows = off (mode.shadows)");
            None
        }
    };

    let mut reflected: Vec<Instance> = Vec::new();
    if let (Some(head), Some(tail)) = (
        instances.get(..water_block_start),
        instances.get(water_block_end..),
    ) {
        reflected.extend_from_slice(head);
        reflected.extend_from_slice(tail);
    }
    let sky_slot = sky.as_ref().map(|_| reflected.len().saturating_sub(1));

    let mut particle_scene = ParticleScene::new(
        particles.index.clone(),
        particles.lists.clone(),
        particles.name,
    );
    particle_scene.set_drawn(particles.draw);
    particle_scene.set_keys(particles.keys);
    particle_scene.set_motion(particles.motion);
    particle_scene.set_billboard(particles.billboard);
    particle_scene.set_sort(particles.sort);
    particle_scene.set_detail(particles.lod);
    particle_scene.set_flipbook(particles.flipbook);
    particle_scene.set_shape(particles.shape);
    particle_scene.set_compound(particles.compound);
    particle_scene.set_wind(particles.wind);
    particle_scene.set_fx_name(particles.fx);
    particle_scene.set_budget_numbers(particles.max_particles, particles.skip_mask);
    particle_scene.set_bones(particles.bones);
    let particle_base = instances.len();
    if let (Some(scene), Some(index)) = (model_scene.as_ref(), input.index) {
        let report = attach_particle_bones(
            &mut particle_scene,
            &mut renderer,
            input.vfs,
            input.resolver,
            index,
            scene,
            input.objects,
            input.proxies,
        );
        report_particle_bones(&particle_scene, &report, particles.bones);
    }
    let fx_playing = particles.draw && !particles.fx.is_empty();
    if fx_playing {
        particle_scene.fire_fx(
            &mut renderer,
            input.vfs,
            input.resolver,
            camera.target,
            &MapGround::new(particles.height_map, terrain.vertical_scale),
        );
    }
    let particles_playing = particles.draw && !particles.name.is_empty();
    let bones_playing = particles.draw && particles.bones != BoneRung::Off;
    if (bones_playing || fx_playing) && !particles_playing {
        particle_scene.warm_up(
            particles.seconds,
            &MapGround::new(particles.height_map, terrain.vertical_scale),
        );
        particle_scene
            .draw(&mut renderer, &camera.camera(), &mut instances)
            .map_err(|e| e.to_string())?;
        let (live_systems, live_particles) = particle_scene.live();
        println!(
            "particles: {live_systems} system(s), {live_particles} live particle(s) at t = {:.2} s (--particle-time)",
            particles.seconds
        );
        println!("{}", particle_scene.budget_line());
    }
    if particles_playing {
        particle_scene.respawn(
            &mut renderer,
            input.vfs,
            input.resolver,
            particles.name,
            camera.target,
        );
        particle_scene.warm_up(
            particles.seconds,
            &MapGround::new(particles.height_map, terrain.vertical_scale),
        );
        particle_scene
            .draw(&mut renderer, &camera.camera(), &mut instances)
            .map_err(|e| e.to_string())?;
        let (live_systems, live_particles) = particle_scene.live();
        println!(
            "particles: {live_systems} system(s), {live_particles} live particle(s) at t = {:.2} s (--particle-time)",
            particles.seconds
        );
        println!("{}", particle_scene.budget_line());
    }
    let shadow_pass = |renderer: &mut Renderer,
                       instances: &[Instance],
                       eye: mint_render::Camera|
     -> Result<Option<ShadowSample>, String> {
        match &shadows {
            Some(pass) => pass
                .render(
                    renderer,
                    eye,
                    rig,
                    instances,
                    water.tuning,
                    ShadowFrame {
                        fit: ground.shadow_fit,
                        alpha: ground.shadow_alpha,
                        world: terrain.bounds(),
                    },
                )
                .map_err(|e| e.to_string()),
            None => Ok(None),
        }
    };
    let planar = |renderer: &mut Renderer,
                  reflected: &mut [Instance],
                  shadow: Option<ShadowSample>,
                  eye: mint_render::Camera|
     -> Result<PlanarPassCount, String> {
        if water.source != WaterReflectionSource::Planar {
            return Ok(PlanarPassCount::default());
        }
        render_planar_reflections(
            renderer,
            &water_planes,
            &plane_bounds,
            ground.reflection_cull,
            reflected,
            sky_slot,
            eye,
            rig,
            1.0,
            LightMask::default(),
            shadow,
        )
        .map_err(|e| e.to_string())
    };
    println!(
        "reflection source = {} ({} planar target(s), one per distinct water height),          highlights = {} (--water-highlights)",
        water.source.label(),
        water_planes.len(),
        water.highlights.label()
    );
    println!(
        "reflection cull = {} (7, --reflection-cull)",
        ground.reflection_cull.label()
    );
    println!(
        "shadow alpha = {} (9, --shadow-alpha)",
        ground.shadow_alpha.label()
    );
    if let Some(pass) = &shadows {
        println!(
            "{}",
            pass.report(
                &renderer,
                camera.camera(),
                rig,
                water.tuning,
                ShadowFrame {
                    fit: ground.shadow_fit,
                    alpha: ground.shadow_alpha,
                    world: terrain.bounds(),
                },
            )
        );
    }

    if let Some(sky) = &mut sky {
        sky.push(&mut renderer, water.tuning, ground.seconds);
    }
    let shadow = shadow_pass(&mut renderer, &instances, camera.camera())?;
    let scene = terrain_scene(
        camera.camera(),
        &instances,
        rig,
        1.0,
        LightMask::default(),
        shadow,
    );
    let passes = planar(&mut renderer, &mut reflected, shadow, camera.camera())?;
    if !water_planes.is_empty() {
        println!(
            "  reflection: {} plane(s) drawn, {} skipped with no water on screen",
            passes.drawn, passes.skipped
        );
    }
    renderer.render(&scene).map_err(|e| e.to_string())?;
    let rgba = renderer.read_pixels().map_err(|e| e.to_string())?;
    if let Some(path) = path {
        write_tga(path, width, height, &rgba)?;
        println!("wrote {width}x{height} screenshot to {}", path.display());
    }

    if let Some(steps) = sweep.filter(|count| *count > 0) {
        let Some(path) = path else {
            return Err("--sweep needs --screenshot to name the frames".to_owned());
        };
        let turns = f32::from(u16::try_from(steps).unwrap_or(u16::MAX)).max(1.0);
        let step = std::f32::consts::TAU / turns;
        let stem = path.with_extension("");
        println!(
            "sweep: {steps} frame(s), yaw + {step:.4} rad each, written as {}_NNN.tga",
            stem.display()
        );
        for frame in 0..steps {
            camera.yaw += step;
            let eye = camera.camera();
            let seconds = f32::from(u16::try_from(frame).unwrap_or(u16::MAX)) / 60.0;
            if let Some(scene) = model_scene.as_mut() {
                if input.animation.on() {
                    if let Some(slice) = instances.get_mut(model_span.clone()) {
                        advance_object_animations(
                            scene,
                            &mut renderer,
                            slice,
                            input.proxies,
                            input.animation.completion(),
                            seconds,
                        );
                    }
                }
                if let Some(slice) = instances.get_mut(model_span.clone()) {
                    advance_object_uv_mappers(scene, slice, input.material.seconds(seconds));
                }
                if let Some(breeze) = input.sway.breeze() {
                    if let Some(slice) = instances.get_mut(model_span.clone()) {
                        advance_object_sway(scene, slice, input.proxies, &breeze.offsets(seconds));
                    }
                }
            }
            reflected.clear();
            if let (Some(head), Some(tail)) = (
                instances.get(..water_block_start),
                instances.get(water_block_end..particle_base),
            ) {
                reflected.extend_from_slice(head);
                reflected.extend_from_slice(tail);
            }
            if let (Some(sky), Some(slot)) = (&sky, particle_base.checked_sub(1)) {
                if let Some(instance) = instances.get_mut(slot) {
                    *instance = sky.instance(eye.eye);
                }
            }
            if let (Some(sky), Some(slot)) = (&sky, sky_slot) {
                if let Some(instance) = reflected.get_mut(slot) {
                    *instance = sky.instance(eye.eye);
                }
            }
            let shadow = shadow_pass(&mut renderer, &instances, eye)?;
            let passes = planar(&mut renderer, &mut reflected, shadow, eye)?;
            let scene = terrain_scene(eye, &instances, rig, 1.0, LightMask::default(), shadow);
            renderer.render(&scene).map_err(|e| e.to_string())?;
            let rgba = renderer.read_pixels().map_err(|e| e.to_string())?;
            let out = PathBuf::from(format!("{}_{frame:03}.tga", stem.display()));
            write_tga(&out, width, height, &rgba)?;
            println!(
                "  sweep {frame:03}: yaw {}, {} reflection plane(s) drawn, {} skipped",
                camera.yaw, passes.drawn, passes.skipped
            );
        }
    }

    if let Some(count) = frames.filter(|count| *count > 0) {
        let mut submit = std::time::Duration::ZERO;
        let started = std::time::Instant::now();
        for index in 0..count {
            let call = std::time::Instant::now();
            if let Some(scene) = model_scene.as_mut() {
                let seconds = f32::from(u16::try_from(index).unwrap_or(u16::MAX)) / 60.0;
                if input.animation.on() {
                    if let Some(slice) = instances.get_mut(model_span.clone()) {
                        advance_object_animations(
                            scene,
                            &mut renderer,
                            slice,
                            input.proxies,
                            input.animation.completion(),
                            seconds,
                        );
                    }
                }
                if let Some(slice) = instances.get_mut(model_span.clone()) {
                    advance_object_uv_mappers(scene, slice, input.material.seconds(seconds));
                }
                if let Some(breeze) = input.sway.breeze() {
                    if let Some(slice) = instances.get_mut(model_span.clone()) {
                        advance_object_sway(scene, slice, input.proxies, &breeze.offsets(seconds));
                    }
                }
            }
            if particles_playing || bones_playing {
                instances.truncate(particle_base);
                particle_scene.advance(
                    1.0 / 60.0,
                    &MapGround::new(particles.height_map, terrain.vertical_scale),
                );
                particle_scene
                    .draw(&mut renderer, &camera.camera(), &mut instances)
                    .map_err(|e| e.to_string())?;
            }
            let scene = terrain_scene(
                camera.camera(),
                &instances,
                rig,
                1.0,
                LightMask::default(),
                shadow,
            );
            shadow_pass(&mut renderer, &instances, camera.camera())?;
            planar(&mut renderer, &mut reflected, shadow, camera.camera())?;
            renderer.render(&scene).map_err(|e| e.to_string())?;
            submit += call.elapsed();
        }
        renderer.read_pixels().map_err(|e| e.to_string())?;
        let wall = started.elapsed();
        let per = |d: std::time::Duration| d.as_secs_f64() * 1000.0 / f64::from(count);
        println!(
            "{count} frames at {width}x{height}, {} terrain + {} road + {} object = {} \
             instance(s) in {} draw call(s): {:.2} ms/frame ({:.1} fps), of which {:.2} ms is \
             command submission",
            terrain_instances,
            road_instance_count,
            instances
                .len()
                .saturating_sub(terrain_instances)
                .saturating_sub(road_instance_count),
            instances.len(),
            renderer.draw_calls(),
            per(wall),
            f64::from(count) / wall.as_secs_f64(),
            per(submit)
        );
        println!(
            "  main pass: {} culled out of frustum",
            renderer.culled_instances()
        );
        if renderer.shadow_drawn_instances() > 0 {
            println!(
                "  shadow pass: {} caster(s) in {} draw call(s), {} culled out of the light's                  frustum; {} alpha-tested caster(s) in {} of those calls",
                renderer.shadow_drawn_instances(),
                renderer.shadow_draw_calls(),
                renderer.shadow_culled_instances(),
                renderer.shadow_alpha_instances().0,
                renderer.shadow_alpha_instances().1,
            );
        }
        if renderer.target_drawn_instances() > 0 {
            println!(
                "  reflection (last plane): {} instance(s) in {} draw call(s), {} culled out of                  the mirrored frustum",
                renderer.target_drawn_instances(),
                renderer.target_draw_calls(),
                renderer.target_culled_instances()
            );
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScaleAction {
    Down,
    Up,
    Reset,
}

struct Viewer {
    height_map: HeightMapData,
    blend_tile: BlendTileData,
    terrain: Terrain,
    handles: Vec<MeshHandle>,
    instances: Vec<Instance>,
    camera: OrbitCamera,
    buttons: [bool; 2],
    pointer: Option<(f32, f32)>,
    pending_frustum: u32,
    pending_shadow_fit: u32,
    pending_shadow_alpha: u32,
    pending_reflection_cull: u32,
    pending_scale: Vec<ScaleAction>,
    starting_scale: f32,
    pan_held: [bool; 4],
    pan_limit: Aabb,
    viewport: (u32, u32),
    classes: Vec<ClassNote>,
    map_label: String,
    draw_blend: bool,
    rigs: [[TerrainRig; 4]; 2],
    time: TimeOfDay,
    map_light: bool,
    alt_set: bool,
    exposure: f32,
    overbright: f32,
    mask: LightMask,
    click_anchor: Option<(f32, f32)>,
    pending_time: u32,
    pending_set: u32,
    objects: Vec<Object>,
    proxies: Vec<ObjectProxy>,
    proxy_handle: MeshHandle,
    object_instances: Vec<Instance>,
    model_scene: Option<ObjectScene>,
    model_instances: Vec<Instance>,
    model_span: std::ops::Range<usize>,
    object_animation: ObjectAnimation,
    object_material: ObjectMaterial,
    pending_object_material: u32,
    pending_material_step: u32,
    sway: SwayMode,
    pending_sway: u32,
    sway_seconds: f32,
    sway_written: bool,
    object_seconds: f32,
    material_seconds: f32,
    object_last_frame: std::time::Instant,
    pending_object_animation: u32,
    draw_models: bool,
    drawn: Vec<Instance>,
    draw_objects: bool,
    object_index: Option<ObjectIndex>,
    road_network: RoadNetwork,
    road_segments: RoadSegments,
    road_scene: RoadScene,
    road_instances: Vec<Instance>,
    road_options: RoadOptions,
    draw_roads: bool,
    water_surface: WaterSurface,
    water_scene: WaterScene,
    water_instances: Vec<Instance>,
    water_span: std::ops::Range<usize>,
    water_seconds: f32,
    water_last_frame: std::time::Instant,
    water_moving: bool,
    water_draw: WaterDraw,
    water_reflection: WaterReflection,
    water_tint: WaterTint,
    water_depth: WaterDepthMode,
    water_distortion: WaterDistortion,
    water_transparency: WaterTransparency,
    water_shipped_fade: f32,
    water_source: WaterReflectionSource,
    water_composite: WaterReflectionComposite,
    water_highlights: WaterHighlights,
    water_drift: WaterDrift,
    water_map_depth: Option<f32>,
    river_network: RiverNetwork,
    river_scene: RiverScene,
    reflected: Vec<Instance>,
    river_instances: Vec<Instance>,
    river_span: std::ops::Range<usize>,
    river_draw: RiverDraw,
    river_look: RiverLook,
    wave_field: WaveField,
    wave_scene: WaveScene,
    wave_instances: Vec<Instance>,
    wave_span: std::ops::Range<usize>,
    wave_look: WaveLook,
    pending_road: Vec<RoadAction>,
    vfs: Vfs,
    resolver: Resolver,
    particle_scene: ParticleScene,
    particle_last_frame: std::time::Instant,
    particle_bones: BoneRung,
    pending_particle_bones: u32,
    pending_particle_motion: u32,
    pending_particle_smooth: u32,
    pending_particle_sort: u32,
    pending_particle_billboard: u32,
    pending_particle_lod: u32,
    pending_particle_flipbook: u32,
    pending_particle_compound: u32,
    pending_particle_shape: u32,
    pending_particle_wind: u32,
    pending_particle_fx: u32,
    pending_particle_attach: u32,
    pending_particles: u32,
    pending_particle_keys: u32,
    pending_particle_spawn: u32,
    composed_len: usize,
    pending_depth: u32,
    wave_surface: WaveSurface,
    tuning_file: tuning::TuningFile,
    global_lighting: GlobalLighting,
    terrain_overbright: TerrainOverbright,
    terrain_ceiling: TerrainCeiling,
    terrain_shading: TerrainShading,
    sky: Option<SkyScene>,
    sky_slot: Option<usize>,
    sky_draw: SkyDraw,
    terrain_cloud: TerrainCloud,
    terrain_macro: TerrainMacro,
    shadows: Option<ShadowPass>,
    shadow_fit: ShadowFitMode,
    shadow_alpha: ShadowAlpha,
    reflection_cull: ReflectionCull,
    plane_bounds: Vec<Aabb>,
    shadow_draw: TerrainShadows,
    console: mint_ui::Console,
    console_field: mint_ui::TextInput,
    console_state: mint_ui::WidgetState,
    console_texture: mint_render::TextureHandle,
    console_atlas: mint_ui::GlyphAtlas,
}

impl Viewer {
    fn toggle_term(&mut self, slot: Option<usize>) {
        match slot {
            None => self.mask.ambient = !self.mask.ambient,
            Some(n) => {
                if let Some(flag) = self.mask.lights.get_mut(n) {
                    *flag = !*flag;
                }
            }
        }
        let on = |b: bool| if b { "on " } else { "OFF" };
        println!(
            "ambient {}  sun {}  accent1 {}  accent2 {}",
            on(self.mask.ambient),
            on(self.mask.lights.first().copied().unwrap_or(false)),
            on(self.mask.lights.get(1).copied().unwrap_or(false)),
            on(self.mask.lights.get(2).copied().unwrap_or(false)),
        );
    }

    fn nudge_exposure(&mut self, factor: f32) {
        self.exposure = if factor == 0.0 {
            1.0
        } else {
            (self.exposure * factor).clamp(0.05, 20.0)
        };
        println!(
            "exposure = {:.3}  (map overbright {:.1} -> effective {:.3})",
            self.exposure,
            self.overbright,
            self.overbright * self.exposure
        );
    }

    fn compose(&mut self) {
        self.drawn.clear();
        self.drawn.extend_from_slice(&self.instances);
        if self.draw_roads {
            self.drawn.extend_from_slice(&self.road_instances);
        }
        let water_start = self.drawn.len();
        self.drawn.extend_from_slice(&self.water_instances);
        self.water_span = water_start..self.drawn.len();
        let river_start = self.drawn.len();
        self.drawn.extend_from_slice(&self.river_instances);
        self.river_span = river_start..self.drawn.len();
        let wave_start = self.drawn.len();
        self.drawn.extend_from_slice(&self.wave_instances);
        self.wave_span = wave_start..self.drawn.len();
        self.model_span = 0..0;
        if self.draw_objects {
            if self.models_on() {
                let start = self.drawn.len();
                self.drawn.extend_from_slice(&self.model_instances);
                self.model_span = start..self.drawn.len();
            } else {
                self.drawn.extend_from_slice(&self.object_instances);
            }
        }
        if let Some(sky) = &self.sky {
            if self.sky_draw.is_on() {
                self.drawn.push(sky.instance(self.camera.camera().eye));
            }
        }
        self.reflected.clear();
        if let (Some(head), Some(tail)) = (
            self.drawn.get(..self.water_span.start),
            self.drawn.get(self.wave_span.end..),
        ) {
            self.reflected.extend_from_slice(head);
            self.reflected.extend_from_slice(tail);
        }
        self.sky_slot = if self.sky.is_some() && self.sky_draw.is_on() {
            self.reflected.len().checked_sub(1)
        } else {
            None
        };
        self.composed_len = self.drawn.len();
    }

    fn set_river_draw(&mut self, mode: RiverDraw) {
        self.river_draw = mode;
        self.river_instances = river_instances(
            &self.river_scene,
            &self.river_network,
            self.river_draw,
            self.water_seconds,
            river_scroll_factor(self.tuning_file.tuning()),
        );
        self.compose();
        println!(
            "rivers = {} ({} ribbon(s), {} triangles, {} additive, motion {})",
            self.river_draw.label(),
            self.river_instances.len(),
            self.river_network.stats.triangles,
            self.river_network.stats.additive,
            self.water_motion_label()
        );
    }

    fn river_look_key(&mut self, renderer: &mut Renderer) {
        push_river_uniforms(
            renderer,
            &self.river_scene,
            &self.river_network,
            self.river_look,
            self.water_distortion.refraction,
            self.tuning_file.tuning(),
            self.water_seconds,
        );
        println!(
            "river stage = {} (--river-stage), river flow = {} (--river-flow)",
            self.river_look.stage.label(),
            self.river_look.flow.label()
        );
    }

    fn set_water_draw(&mut self, mode: WaterDraw) {
        self.water_draw = mode;
        self.water_instances = water_instances(
            &self.water_scene,
            &self.water_surface,
            self.water_draw,
            self.water_tint,
        );
        self.compose();
        let shaded = self
            .water_instances
            .iter()
            .filter(|instance| instance.custom_material.is_some())
            .count();
        println!(
            "water = {} ({} area(s), {shaded} on the ported shader, {} triangles, motion {}){}",
            self.water_draw.label(),
            self.water_instances.len(),
            self.water_surface.stats.triangles,
            self.water_motion_label(),
            match self.water_draw {
                WaterDraw::Default => " - reflection absent (water-shader.md 9d)",
                WaterDraw::Low => " - complete: this tier samples no reflection",
                _ => "",
            }
        );
    }

    fn set_reflection(&mut self, renderer: &mut Renderer, mode: WaterReflection) {
        self.water_reflection = mode;
        self.push_water_uniforms(renderer);
        let reflecting = self.water_scene.reflects.iter().filter(|on| **on).count();
        println!(
            "reflection = {} (max(Schlick F0 {WATER_FRESNEL_F0}, engine.water_reflection_floor)              x ReflectionStrength,              {reflecting} of {} bodies with a WaterEnvTexture){}",
            self.water_reflection.label(),
            self.water_scene.reflects.len(),
            match self.water_draw {
                WaterDraw::Default => "",
                _ => " - only the `default` rung reads it; press g",
            }
        );
    }

    fn set_refraction(&mut self, renderer: &mut Renderer, mode: WaterRefraction) {
        self.water_distortion.refraction = mode;
        self.push_water_uniforms(renderer);
        self.push_river_uniforms(renderer);
        println!(
            "refraction = {} (--refraction; strength = ReflectionPerturbation x \
             {WATER_REFRACTION_SCALE}, falloff {WATER_REFRACTION_FALLOFF} world units){}",
            self.water_distortion.refraction.label(),
            match self.water_draw {
                WaterDraw::Default => "",
                _ => " - only the `default` rung reads it; press g",
            }
        );
    }

    fn set_water_normal(&mut self, renderer: &mut Renderer, mode: WaterNormal) {
        self.water_distortion.normal = mode;
        self.push_water_uniforms(renderer);
        println!(
            "water normal = {} (--water-normal; unit = the original's constant \
             {WATER_UNIT_SLOPE} slope, slope = the bump texture's own magnitude x \
             {WATER_BUMP_SLOPE})",
            self.water_distortion.normal.label()
        );
    }

    fn water_motion_label(&self) -> String {
        format!(
            "{} at t = {:.2} s{}",
            if self.water_moving {
                "running"
            } else {
                "FROZEN"
            },
            self.water_seconds,
            if self.water_draw.animates() {
                ""
            } else {
                ", not applied in this mode"
            }
        )
    }

    fn set_water_motion(&mut self, moving: bool) {
        self.water_moving = moving;
        println!("water motion = {}", self.water_motion_label());
    }

    fn poll_tuning(&mut self, renderer: &mut Renderer) {
        let Some(changes) = self.tuning_file.poll() else {
            return;
        };
        if changes.is_empty() {
            println!("water tuning: re-read, nothing changed");
        } else {
            println!("water tuning: {} value(s) changed", changes.len());
            for line in &changes {
                println!("  {line}");
            }
        }
        self.apply_tuning_modes(renderer);
        self.advance_river_offsets();
        self.wave_surface =
            surface_waves(&self.wave_field, self.wave_look, self.tuning_file.tuning());
        self.push_water_uniforms(renderer);
        self.push_river_uniforms(renderer);
        self.push_wave_uniforms(renderer);
    }

    fn apply_tuning_modes(&mut self, renderer: &mut Renderer) {
        let tuning = *self.tuning_file.tuning();

        let named = tuning.particle_system.as_str();
        if named != self.particle_scene.name() {
            let at = self.camera.target;
            let name = named.to_owned();
            self.particle_scene
                .respawn(renderer, &self.vfs, &self.resolver, &name, at);
        }

        let list = tuning.particle_fxlist.as_str();
        if list != self.particle_scene.fx_name() {
            let name = list.to_owned();
            self.particle_scene.set_fx_name(&name);
        }

        if self
            .particle_scene
            .set_budget_numbers(tuning_ceiling(&tuning), tuning_skip_mask(&tuning))
        {
            println!("{}", self.particle_scene.budget_line());
        }

        let water = tuned(&tuning, "mode.water", WaterDraw::parse);
        if water != self.water_draw {
            self.set_water_draw(water);
        }
        let tint = tuned(&tuning, "mode.water_tint", WaterTint::parse);
        if tint != self.water_tint {
            self.set_water_tint(renderer, tint);
        }
        let fade = finite_positive(tuning.water_shore_fade);
        if (self.water_transparency.depth - self.water_shipped_fade * fade).abs() > f32::EPSILON {
            self.set_water_fade(renderer, fade);
        }
        let alpha = tuned(&tuning, "mode.water_alpha", WaterAlphaSource::parse);
        if alpha != self.water_transparency.source {
            self.water_transparency.source = alpha;
            self.refit_water_depth();
            self.reshade_water(renderer);
            println!(
                "water alpha source = {} (mode.water_alpha; band {:.1} world units,                  map deepWaterAlpha {:?})",
                self.water_transparency.source.label(),
                self.water_transparency.transparent_depth(),
                self.water_transparency.map_alpha
            );
        }
        let depth = tuned(&tuning, "mode.water_depth", WaterDepthMode::parse);
        if depth != self.water_depth {
            self.set_water_depth(renderer, depth);
        }
        let sky = tuned(&tuning, "mode.sky", SkyDraw::parse);
        if sky != self.sky_draw {
            self.sky_draw = sky;
            if let Some(scene) = &mut self.sky {
                scene.look.draw = sky;
                println!("{}", scene.report());
            } else {
                println!(
                    "sky = {} (mode.sky) - no sky was loaded, restart to build one",
                    sky.label()
                );
            }
            self.compose();
        }
        let shadows = tuned(&tuning, "mode.shadows", TerrainShadows::parse);
        if shadows != self.shadow_draw {
            self.shadow_draw = shadows;
            match (&self.shadows, shadows) {
                (Some(pass), TerrainShadows::On) => println!(
                    "shadows = on (mode.shadows), {}x{} shadow map",
                    pass.size, pass.size
                ),
                (None, TerrainShadows::On) => println!(
                    "shadows = on (mode.shadows) - no shadow map was allocated, restart to build one"
                ),
                (_, TerrainShadows::Off) => println!("shadows = off (mode.shadows)"),
            }
        }
        let reflection = tuned(&tuning, "mode.reflection", WaterReflection::parse);
        if reflection != self.water_reflection {
            self.set_reflection(renderer, reflection);
        }
        let source = tuned(
            &tuning,
            "mode.reflection_source",
            WaterReflectionSource::parse,
        );
        if source != self.water_source {
            self.water_source = source;
            println!(
                "reflection source = {} (mode.reflection_source; {} planar target(s))",
                self.water_source.label(),
                self.water_scene.planes.len()
            );
        }
        let composite = tuned(
            &tuning,
            "mode.reflection_composite",
            WaterReflectionComposite::parse,
        );
        if composite != self.water_composite {
            self.water_composite = composite;
            self.reshade_water(renderer);
            println!(
                "reflection composite = {} (mode.reflection_composite; the                  refracting path only)",
                self.water_composite.label()
            );
        }
        let highlights = tuned(&tuning, "mode.water_highlights", WaterHighlights::parse);
        if highlights != self.water_highlights {
            self.water_highlights = highlights;
            println!(
                "water highlights = {} (mode.water_highlights; the band, the threshold                  and the gain are engine.water_foam_* in the same file)",
                self.water_highlights.label()
            );
        }
        let refraction = tuned(&tuning, "mode.refraction", WaterRefraction::parse);
        if refraction != self.water_distortion.refraction {
            self.set_refraction(renderer, refraction);
        }
        let normal = tuned(&tuning, "mode.water_normal", WaterNormal::parse);
        if normal != self.water_distortion.normal {
            self.set_water_normal(renderer, normal);
        }
        let drift = tuning.drift_mode();
        if drift != self.water_drift {
            self.water_drift = drift;
            self.push_water_uniforms(renderer);
            println!(
                "water drift = {} (mode.water_drift; orbit {} repeat(s),                  rate cap {} repeat(s)/s, travel cap {} world units/s)",
                self.water_drift.label(),
                tuning.water_still_orbit,
                tuning.water_drift_rate_cap,
                tuning.water_drift_speed_cap
            );
        }
        let moving = tuning.mode("mode.water_motion") != Some("freeze");
        if moving != self.water_moving {
            self.set_water_motion(moving);
        }
        let rivers = tuned(&tuning, "mode.rivers", RiverDraw::parse);
        if rivers != self.river_draw {
            self.set_river_draw(rivers);
        }
        let look = RiverLook {
            stage: tuned(&tuning, "mode.river_stage", RiverStage::parse),
            flow: tuned(&tuning, "mode.river_flow", RiverFlow::parse),
        };
        if look != self.river_look {
            self.river_look = look;
            self.river_look_key(renderer);
        }
        let waves = WaveLook {
            draw: tuned(&tuning, "mode.waves", WaveDraw::parse),
            pca: tuned(&tuning, "mode.wave_pca", WavePca::parse),
            scale: tuned(&tuning, "mode.wave_scale", WaveScale::parse),
        };
        if waves != self.wave_look {
            self.wave_look = waves;
            self.wave_look_key(renderer);
        }

        let overbright = tuned(&tuning, "mode.terrain_overbright", TerrainOverbright::parse);
        let ceiling = tuned(&tuning, "mode.terrain_ceiling", TerrainCeiling::parse);
        if overbright != self.terrain_overbright || ceiling != self.terrain_ceiling {
            self.terrain_overbright = overbright;
            self.terrain_ceiling = ceiling;
            self.rebuild_rigs();
        }

        let cloud = tuned(&tuning, "mode.terrain_cloud", TerrainCloud::parse);
        let macro_layer = tuned(&tuning, "mode.terrain_macro", TerrainMacro::parse);
        if cloud != self.terrain_cloud || macro_layer != self.terrain_macro {
            self.terrain_cloud = cloud;
            self.terrain_macro = macro_layer;
            self.terrain_shading.modulation.cloud = cloud;
            self.terrain_shading.modulation.macro_layer = macro_layer;
            println!(
                "terrain cloud = {} (mode.terrain_cloud; texture {}, no_cloud_factor {:?}),                  macro = {} (mode.terrain_macro; texture {}, {})",
                cloud.label(),
                if self.terrain_shading.modulation.cloud_loaded {
                    "loaded"
                } else {
                    "MISSING - the term is 1.0"
                },
                self.terrain_shading.modulation.no_cloud_factor,
                macro_layer.label(),
                if self.terrain_shading.modulation.macro_loaded {
                    "loaded"
                } else {
                    "MISSING - the term is 1.0"
                },
                if self.terrain_shading.modulation.macro_stretched {
                    "stretched to the map"
                } else {
                    "tiled"
                },
            );
        }
        self.terrain_shading
            .push(renderer, &tuning, self.water_seconds);
    }

    fn rebuild_rigs(&mut self) {
        self.rigs = [
            map_rigs(
                &self.global_lighting,
                false,
                self.terrain_overbright,
                self.terrain_ceiling,
            ),
            map_rigs(
                &self.global_lighting,
                true,
                self.terrain_overbright,
                self.terrain_ceiling,
            ),
        ];
        let rig = current_rig(&self.rigs, self.time, self.map_light, self.alt_set);
        println!(
            "terrain lighting: overbright = {} (mode.terrain_overbright),              ceiling = {} (mode.terrain_ceiling); map overbright {:.1}x,              sun {:.3},{:.3},{:.3}, ambient {:.3},{:.3},{:.3}",
            self.terrain_overbright.label(),
            self.terrain_ceiling.label(),
            self.overbright,
            rig.lights[0].color[0],
            rig.lights[0].color[1],
            rig.lights[0].color[2],
            rig.ambient[0],
            rig.ambient[1],
            rig.ambient[2],
        );
    }

    fn advance_water(&mut self, renderer: &mut Renderer) {
        let now = std::time::Instant::now();
        let elapsed = now.saturating_duration_since(self.water_last_frame);
        self.water_last_frame = now;
        if !self.water_moving
            || (!self.water_draw.animates() && !self.river_draw.on() && !self.wave_look.draw.on())
        {
            return;
        }
        self.water_seconds += elapsed.as_secs_f32();
        self.push_water_uniforms(renderer);
        self.push_river_uniforms(renderer);
        self.push_wave_uniforms(renderer);
        self.advance_river_offsets();
    }

    fn advance_river_offsets(&mut self) {
        let factor = river_scroll_factor(self.tuning_file.tuning());
        for (slot, instance) in self.river_instances.iter_mut().enumerate() {
            let speed = self
                .river_network
                .ribbons
                .get(slot)
                .map_or(0.0, |ribbon| ribbon.uv_scroll_speed);
            instance.uv_offset = river_scroll_offset(speed * factor, self.water_seconds);
        }
        if let Some(drawn) = self.drawn.get_mut(self.river_span.clone()) {
            for (target, source) in drawn.iter_mut().zip(&self.river_instances) {
                target.uv_offset = source.uv_offset;
            }
        }
    }

    fn push_river_uniforms(&mut self, renderer: &mut Renderer) {
        push_river_uniforms(
            renderer,
            &self.river_scene,
            &self.river_network,
            self.river_look,
            self.water_distortion.refraction,
            self.tuning_file.tuning(),
            self.water_seconds,
        );
    }

    fn push_wave_uniforms(&mut self, renderer: &mut Renderer) {
        push_wave_uniforms(
            renderer,
            &self.wave_scene,
            &self.wave_field,
            self.wave_look,
            self.tuning_file.tuning(),
            self.water_seconds,
        );
    }

    fn wave_look_key(&mut self, renderer: &mut Renderer) {
        self.wave_surface =
            surface_waves(&self.wave_field, self.wave_look, self.tuning_file.tuning());
        self.wave_instances = wave_instances(&self.wave_scene, &self.wave_field, self.wave_look);
        self.compose();
        self.push_wave_uniforms(renderer);
        self.push_water_uniforms(renderer);
        println!(
            "waves = {} ({} sprite instance(s), {} surface swell(s)), pca = {} \
             (--wave-pca), scale = {} (--wave-scale)",
            self.wave_look.draw.label(),
            self.wave_instances.len(),
            self.wave_surface.waves.len(),
            self.wave_look.pca.label(),
            self.wave_look.scale.label()
        );
    }

    fn push_water_uniforms(&mut self, renderer: &mut Renderer) {
        for (slot, params) in self.water_scene.params.iter().enumerate() {
            let Some(pair) = self.water_scene.materials.get(slot).copied().flatten() else {
                continue;
            };
            for (handle, technique) in pair
                .iter()
                .zip([WaterTechnique::Low, WaterTechnique::Default])
            {
                let block = WaterUniforms::new(
                    params,
                    technique,
                    self.water_scene.reflection(slot, self.water_reflection),
                    self.water_distortion,
                    WaterSurfaceLook {
                        source: self.water_scene.source(slot, self.water_source),
                        composite: self.water_composite,
                        highlights: self.water_highlights,
                    },
                    self.water_scene
                        .depth(slot, self.water_depth, self.tuning_file.tuning()),
                    self.tuning_file.tuning(),
                    self.water_seconds,
                )
                .to_bytes(
                    &self.wave_surface,
                    self.tuning_file.tuning(),
                    self.water_seconds,
                );
                if let Err(error) = renderer.update_custom_material(*handle, &block) {
                    eprintln!("  could not update a water material: {error}");
                }
            }
        }
    }

    fn reset_object_poses(&mut self, renderer: &mut Renderer) {
        if let Some(scene) = self.model_scene.as_mut() {
            for group in &mut scene.groups {
                let Some(animation) = &group.animation else {
                    continue;
                };
                animation.reset(renderer, &group.model, &mut group.poses);
            }
        }
        self.replace_objects();
    }

    fn advance_objects(&mut self, renderer: &mut Renderer) {
        let now = std::time::Instant::now();
        let elapsed = now.saturating_duration_since(self.object_last_frame);
        self.object_last_frame = now;
        if !self.draw_objects || !self.models_on() {
            return;
        }
        if self.object_animation.on() {
            self.object_seconds += elapsed.as_secs_f32();
        }
        if self.object_material == ObjectMaterial::Scroll {
            self.material_seconds += elapsed.as_secs_f32();
        }
        let breeze = self.sway.breeze();
        if breeze.is_some() {
            self.sway_seconds += elapsed.as_secs_f32();
        }
        let sway_offsets = (breeze.is_some() || !self.sway_written).then(|| {
            self.sway_written = true;
            breeze.map_or([[0.0; 3]; mint_sage_asset::SWAY_OFFSET_COUNT], |breeze| {
                breeze.offsets(self.sway_seconds)
            })
        });
        let completion = self.object_animation.completion();
        let seconds = self.object_seconds;
        let material_seconds = self.material_seconds;
        let animating = self.object_animation.on();
        let Some(scene) = self.model_scene.as_mut() else {
            return;
        };
        let posed = animating.then(|| {
            advance_object_animations(
                scene,
                renderer,
                &mut self.model_instances,
                &self.proxies,
                completion,
                seconds,
            )
        });
        let scrolled = advance_object_uv_mappers(
            scene,
            &mut self.model_instances,
            live_material_seconds(self.object_material, material_seconds),
        );
        let swayed = sway_offsets.and_then(|offsets| {
            advance_object_sway(scene, &mut self.model_instances, &self.proxies, &offsets)
        });
        let union =
            |a: Option<std::ops::Range<usize>>, b: Option<std::ops::Range<usize>>| match (a, b) {
                (Some(a), Some(b)) => Some(a.start.min(b.start)..a.end.max(b.end)),
                (Some(span), None) | (None, Some(span)) => Some(span),
                (None, None) => None,
            };
        let touched = union(union(posed.flatten(), scrolled), swayed);
        let Some(span) = touched else {
            return;
        };
        if self.model_span.len() != self.model_instances.len() {
            return;
        }
        let start = self.model_span.start.saturating_add(span.start);
        let end = self.model_span.start.saturating_add(span.end);
        let (Some(source), Some(target)) = (
            self.model_instances.get(span),
            self.drawn.get_mut(start..end),
        ) else {
            return;
        };
        target.clone_from_slice(source);
    }

    fn models_on(&self) -> bool {
        self.draw_models && self.model_scene.is_some()
    }

    fn replace_objects(&mut self) {
        self.proxies = place_objects(
            &self.height_map,
            self.terrain.vertical_scale,
            &self.objects,
            &mint_sage_scene::rotation_anchors(self.object_index.as_ref(), &self.objects),
        );
        self.object_instances = object_instances(self.proxy_handle, &self.proxies);
        self.model_instances = self
            .model_scene
            .as_mut()
            .map(|scene| model_instances(scene, &self.proxies))
            .unwrap_or_default();
        self.compose();
    }

    fn rebuild_roads(&mut self, renderer: &mut Renderer) {
        self.road_network = build_road_network(
            &self.height_map,
            self.terrain.vertical_scale,
            &self.objects,
            self.object_index.as_ref(),
            &self.road_options,
        );
        for (road, &handle) in self
            .road_network
            .meshes
            .iter()
            .zip(&self.road_scene.handles)
        {
            if let Err(error) = renderer.update_mesh_vertices(handle, &road.mesh) {
                eprintln!("  could not update a road's vertices: {error}");
            }
        }
        self.compose();
    }

    fn reupload_roads(&mut self, renderer: &mut Renderer) {
        self.road_network = build_road_network(
            &self.height_map,
            self.terrain.vertical_scale,
            &self.objects,
            self.object_index.as_ref(),
            &self.road_options,
        );
        for (road, handle) in self
            .road_network
            .meshes
            .iter()
            .zip(self.road_scene.handles.iter_mut())
        {
            match renderer.upload_mesh(&road.mesh) {
                Ok(fresh) => *handle = fresh,
                Err(error) => eprintln!("  could not re-upload a road's mesh: {error}"),
            }
        }
        self.road_instances = road_instances(&self.road_scene);
        self.compose();
    }

    fn report_depth_precision(&self, direction: DepthDirection) {
        let camera = self.camera.camera();
        let (width, height) = self.viewport;
        let aspect = width as f32 / (height.max(1) as f32);
        let lift = self.road_options.lift;
        println!(
            "depth = {} (near -> {}, far -> {}, clear {}, compares {})",
            match direction {
                DepthDirection::Standard => "standard-Z",
                DepthDirection::Reversed => "reversed-Z",
            },
            match direction {
                DepthDirection::Standard => "0",
                DepthDirection::Reversed => "1",
            },
            match direction {
                DepthDirection::Standard => "1",
                DepthDirection::Reversed => "0",
            },
            direction.clear_value(),
            match direction {
                DepthDirection::Standard => "Less/LessEqual",
                DepthDirection::Reversed => "Greater/GreaterEqual",
            },
        );
        println!(
            "  near = {:.4}  far = {:.1}  far/near = {:.0}  lift = {lift}",
            camera.near,
            camera.far,
            camera.far / camera.near.max(f32::MIN_POSITIVE),
        );
        for (label, depth) in depth_probes(&camera, &self.terrain.bounds()) {
            let standard =
                lift_separation_ulps(&camera, DepthDirection::Standard, aspect, depth, lift);
            let reversed =
                lift_separation_ulps(&camera, DepthDirection::Reversed, aspect, depth, lift);
            println!(
                "  {label:<10} d = {depth:>9.1}   ulps: standard {standard:>9}   reversed {reversed:>9}"
            );
        }
    }

    fn road_key(&mut self, renderer: &mut Renderer, action: RoadAction) {
        match action {
            RoadAction::Toggle => {
                self.draw_roads = !self.draw_roads;
                self.compose();
            }
            RoadAction::NextLift => {
                let next = ROAD_LIFTS
                    .iter()
                    .position(|lift| *lift == self.road_options.lift)
                    .map_or(0, |slot| slot.saturating_add(1) % ROAD_LIFTS.len());
                self.road_options.lift = ROAD_LIFTS.get(next).copied().unwrap_or(0.0);
                self.rebuild_roads(renderer);
            }
            RoadAction::NextHeight => {
                self.road_options.height = match self.road_options.height {
                    RoadHeight::CellMax => RoadHeight::Surface,
                    RoadHeight::Surface => RoadHeight::StationMax,
                    RoadHeight::StationMax => RoadHeight::CellMax,
                };
                self.rebuild_roads(renderer);
            }
            RoadAction::NextChainOrder => {
                let next = ROAD_CHAIN_ORDERS
                    .iter()
                    .position(|step| *step == self.road_options.chain_order)
                    .map_or(0, |slot| slot.saturating_add(1) % ROAD_CHAIN_ORDERS.len());
                self.road_options.chain_order = ROAD_CHAIN_ORDERS
                    .get(next)
                    .copied()
                    .unwrap_or(RoadChainOrder::Chained);
                self.reupload_roads(renderer);
            }
            RoadAction::NextTessellation => {
                let next = ROAD_TESSELLATIONS
                    .iter()
                    .position(|step| *step == self.road_options.tessellation)
                    .map_or(0, |slot| slot.saturating_add(1) % ROAD_TESSELLATIONS.len());
                self.road_options.tessellation = ROAD_TESSELLATIONS
                    .get(next)
                    .copied()
                    .unwrap_or(RoadTessellation::Ribbon);
                self.reupload_roads(renderer);
            }
        }
        println!(
            "{}  ({} type(s), {} triangle(s))",
            road_state_line(&self.road_options, self.draw_roads),
            self.road_network.meshes.len(),
            self.road_network.stats.triangles
        );
    }

    fn object_key(&mut self, action: ObjectAction) {
        match action {
            ObjectAction::Toggle => {
                self.draw_objects = !self.draw_objects;
                self.compose();
            }
            ObjectAction::ToggleModels => {
                self.draw_models = !self.draw_models;
                if self.draw_models && self.model_scene.is_none() {
                    println!("no template index was built, so there are no models to draw");
                }
                self.compose();
            }
        }
        println!(
            "objects = {} drawn as {} ({} instance(s))",
            if self.draw_objects { "on" } else { "off" },
            if self.models_on() {
                "models"
            } else {
                "markers"
            },
            if self.draw_objects {
                if self.models_on() {
                    self.model_instances.len()
                } else {
                    self.object_instances.len()
                }
            } else {
                0
            }
        );
    }

    fn report_object_material(&self) {
        let clock = live_material_seconds(self.object_material, self.material_seconds);
        println!(
            "object material = {} at t = {clock:.3}s -> {} (8, /, --object-uv)",
            self.object_material.label(),
            grid_cells_at(clock)
        );
    }

    fn set_scale(&mut self, renderer: &mut Renderer, new_scale: f32) {
        self.terrain
            .set_vertical_scale(&self.height_map, &self.blend_tile, new_scale);
        for ((part, &handle), instance) in drawn_parts(&self.terrain, self.draw_blend)
            .zip(&self.handles)
            .zip(self.instances.iter_mut())
        {
            if let Err(error) = renderer.update_mesh_vertices(handle, &part.mesh) {
                eprintln!("  could not update a chunk's vertices: {error}");
            }
            instance.bounds = Some(part.mesh.bounds());
        }
        self.replace_objects();
        self.rebuild_roads(renderer);
        self.reshade_water(renderer);
        println!("vertical scale = {}", self.terrain.vertical_scale);
    }

    fn set_water_fade(&mut self, renderer: &mut Renderer, next: f32) {
        self.water_transparency.depth = self.water_shipped_fade * next;
        self.water_transparency.map_depth = self.water_map_depth.map(|depth| depth * next);
        self.refit_water_depth();
        println!(
            "water fade depth = {:.1} world units ({next}x water.ini's {:.1});              map.water_shore_fade",
            self.water_transparency.depth, self.water_shipped_fade,
        );
        self.reshade_water(renderer);
    }

    fn set_water_tint(&mut self, renderer: &mut Renderer, mode: WaterTint) {
        self.water_tint = mode;
        self.reshade_water(renderer);
        self.water_instances = water_instances(
            &self.water_scene,
            &self.water_surface,
            self.water_draw,
            self.water_tint,
        );
        self.compose();
    }

    fn set_water_depth(&mut self, renderer: &mut Renderer, mode: WaterDepthMode) {
        self.water_depth = mode;
        self.push_water_uniforms(renderer);
        let fitted = self
            .water_scene
            .depth
            .iter()
            .filter(|terms| terms.extinction.iter().any(|sigma| *sigma > 0.0))
            .count();
        println!(
            "water depth = {} (--water-depth); {fitted} of {} bodies carry a fitted ramp",
            self.water_depth.label(),
            self.water_scene.depth.len(),
        );
    }

    fn refit_water_depth(&mut self) {
        for (shading, terms) in self
            .water_scene
            .shading
            .iter()
            .zip(self.water_scene.depth.iter_mut())
        {
            terms.fade_depth = self.water_transparency.transparent_depth();
            terms.max_alpha = self.water_transparency.max_alpha(shading.color[3]);
            if let Some((fit, _)) = body_absorption(shading, self.water_transparency) {
                terms.extinction = fit.extinction;
                terms.body = fit.body;
            }
        }
    }

    fn reshade_water(&mut self, renderer: &mut Renderer) {
        let mut range = DepthRange::default();
        for ((body, shading), &handle) in self
            .water_surface
            .bodies
            .iter()
            .zip(self.water_scene.shading.iter())
            .zip(self.water_scene.handles.iter())
        {
            let (vertices, body_range) = shade_body(
                body,
                shading,
                self.water_transparency,
                self.terrain.vertical_scale,
                self.water_tint,
            );
            range.merge(body_range);
            let data = MeshData {
                vertices,
                indices: Vec::new(),
            };
            if let Err(error) = renderer.update_mesh_vertices(handle, &data) {
                eprintln!("  could not update a water body's vertices: {error}");
            }
        }
        println!(
            "water tint = {} (depth {:.1} to {:.1} world units, {} of {} vertices inside the              {:.1}-unit fade band)",
            self.water_tint.label(),
            range.min,
            range.max,
            range.faded,
            range.total,
            self.water_transparency.depth * self.water_transparency.min_opacity,
        );
    }

    fn inspect(&self, px: f32, py: f32) {
        self.cells()
            .pick(&self.camera.camera(), self.viewport, px, py);
    }

    fn cells(&self) -> CellContext<'_> {
        CellContext {
            height_map: &self.height_map,
            blend_tile: &self.blend_tile,
            terrain: &self.terrain,
            classes: &self.classes,
            map_label: &self.map_label,
            draw_blend: self.draw_blend,
            objects: &self.objects,
            proxies: &self.proxies,
            templates: self.model_scene.as_ref().map(|scene| &scene.notes),
            roads: Some(RoadReport {
                segments: &self.road_segments,
                network: &self.road_network,
                options: self.road_options,
            }),
        }
    }

    fn console_quads(&mut self) -> Vec<mint_render::UiQuad> {
        if !self.console.is_open() {
            return Vec::new();
        }
        self.console_atlas.begin_frame();
        let (width, _height) = self.viewport;
        let width = width as f32;
        let panel_height = 220.0f32;
        let size_px = 14u32;
        let line_height = 18.0f32;
        const VISIBLE_LINES: usize = 11;
        let mut quads = vec![mint_render::UiQuad::new(0.0, 0.0, width, panel_height)
            .with_color_transform([0.05, 0.05, 0.08, 0.85], [0.0; 4])];

        let total = self.console.line_count();
        let start = total.saturating_sub(VISIBLE_LINES);
        let mut y = size_px as f32 + 4.0;
        for index in start..total {
            if let Some(line) = self.console.line(index) {
                quads.extend(mint_ui::layout_text(
                    &line.text,
                    [8.0, y],
                    line.severity.color(),
                    mint_ui::FontId(0),
                    None,
                    &mut self.console_atlas,
                    size_px,
                    self.console_texture,
                ));
            }
            y += line_height;
        }

        let prompt = format!("> {}", self.console.input());
        quads.extend(mint_ui::layout_text(
            &prompt,
            [8.0, panel_height - 6.0],
            [0.85, 0.90, 1.0, 1.0],
            mint_ui::FontId(0),
            None,
            &mut self.console_atlas,
            size_px,
            self.console_texture,
        ));
        quads
    }
}

impl App for Viewer {
    fn update(&mut self, _step: u64) {
        let held =
            |slot: usize| f32::from(u8::from(self.pan_held.get(slot).copied().unwrap_or(false)));
        let seconds = 1.0 / f32::from(u16::try_from(SIMULATION_HZ).unwrap_or(30));
        pan_camera(
            &mut self.camera,
            (held(0) - held(1)) * seconds,
            (held(3) - held(2)) * seconds,
            &self.pan_limit,
        );
    }

    fn input(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::PointerButton { button, pressed } => {
                let slot = match button {
                    PointerButton::Middle => Some(0),
                    PointerButton::Secondary => Some(1),
                    _ => None,
                };
                if button == PointerButton::Primary {
                    if pressed {
                        self.click_anchor = self.pointer;
                    } else if let (Some((ax, ay)), Some((px, py))) =
                        (self.click_anchor.take(), self.pointer)
                    {
                        if (px - ax).abs() <= CLICK_SLOP && (py - ay).abs() <= CLICK_SLOP {
                            self.inspect(px, py);
                        }
                    }
                }
                if let Some(held) = slot.and_then(|slot| self.buttons.get_mut(slot)) {
                    *held = pressed;
                }
                if !pressed {
                    self.pointer = None;
                }
            }
            InputEvent::PointerMoved { x, y } => {
                if let Some((last_x, last_y)) = self.pointer {
                    let (dx, dy) = (x - last_x, y - last_y);
                    if self.buttons[1] {
                        let scale = DRAG_PAN_PER_PIXEL / PAN_PER_SECOND;
                        pan_camera(&mut self.camera, dy * scale, -dx * scale, &self.pan_limit);
                    } else if self.buttons[0] {
                        self.camera.orbit(dx, dy);
                    }
                }
                self.pointer = Some((x, y));
            }
            InputEvent::Scroll { lines } => self.camera.zoom(lines),
            InputEvent::Key { key, pressed } => {
                if key == Key::Character('`') {
                    if pressed {
                        self.console.toggle_open();
                    }
                    return;
                }
                if self.console.is_open() {
                    if pressed {
                        match key {
                            Key::Backspace => self
                                .console_field
                                .backspace(&mut self.console_state, self.console.input_mut()),
                            Key::Enter => {
                                let submitted = self
                                    .console_field
                                    .submit(&mut self.console_state, self.console.input_mut());
                                if !submitted.is_empty() {
                                    self.console.push_line(
                                        mint_ui::Severity::Info,
                                        format!("> {submitted}"),
                                    );
                                }
                            }
                            Key::Escape => self.console.close(),
                            _ => {}
                        }
                    }
                    return;
                }
                if let Some(slot) = pan_slot(key) {
                    if let Some(held) = self.pan_held.get_mut(slot) {
                        *held = pressed;
                    }
                    return;
                }
                if !pressed {
                    return;
                }
                match key {
                    Key::Character('[') => self.pending_scale.push(ScaleAction::Down),
                    Key::Character(']') => self.pending_scale.push(ScaleAction::Up),
                    Key::Character('r') => self.pending_scale.push(ScaleAction::Reset),
                    Key::Character('t') => self.pending_time = self.pending_time.saturating_add(1),
                    Key::Character('y') => self.pending_set = self.pending_set.saturating_add(1),
                    Key::Character('j') => self.nudge_exposure(1.0 / EXPOSURE_STEP),
                    Key::Character('k') => self.nudge_exposure(EXPOSURE_STEP),
                    Key::Character('u') => self.nudge_exposure(0.0),
                    Key::Character('1') => self.toggle_term(None),
                    Key::Character('2') => self.toggle_term(Some(0)),
                    Key::Character('3') => self.toggle_term(Some(1)),
                    Key::Character('4') => self.toggle_term(Some(2)),
                    Key::Character('o') => self.object_key(ObjectAction::Toggle),
                    Key::Character('p') => self.object_key(ObjectAction::ToggleModels),
                    Key::Character('z') => {
                        self.pending_depth = self.pending_depth.saturating_add(1);
                    }
                    Key::Character('n') => self.pending_road.push(RoadAction::Toggle),
                    Key::Character('b') => self.pending_road.push(RoadAction::NextLift),
                    Key::Character('h') => self.pending_road.push(RoadAction::NextHeight),
                    Key::Character('v') => {
                        self.pending_road.push(RoadAction::NextTessellation);
                    }
                    Key::Character('c') => {
                        self.pending_road.push(RoadAction::NextChainOrder);
                    }
                    Key::Character('f') => {
                        self.pending_frustum = self.pending_frustum.saturating_add(1);
                    }
                    Key::Character('6') => {
                        self.pending_shadow_fit = self.pending_shadow_fit.saturating_add(1);
                    }
                    Key::Character('7') => {
                        self.pending_reflection_cull =
                            self.pending_reflection_cull.saturating_add(1);
                    }
                    Key::Character('9') => {
                        self.pending_shadow_alpha = self.pending_shadow_alpha.saturating_add(1);
                    }
                    Key::Character('a') => {
                        self.pending_object_animation =
                            self.pending_object_animation.saturating_add(1);
                    }
                    Key::Character('8') => {
                        self.pending_object_material =
                            self.pending_object_material.saturating_add(1);
                    }
                    Key::Character('/') => {
                        self.pending_material_step = self.pending_material_step.saturating_add(1);
                    }
                    Key::Character(',') => {
                        self.pending_sway = self.pending_sway.saturating_add(1);
                    }
                    Key::Character('q') => {
                        self.pending_particles = self.pending_particles.saturating_add(1);
                    }
                    Key::Character('e') => {
                        self.pending_particle_keys = self.pending_particle_keys.saturating_add(1);
                    }
                    Key::Character('g') => {
                        self.pending_particle_spawn = self.pending_particle_spawn.saturating_add(1);
                    }
                    Key::Character('x') => {
                        self.pending_particle_bones = self.pending_particle_bones.saturating_add(1);
                    }
                    Key::Character('m') => {
                        self.pending_particle_motion =
                            self.pending_particle_motion.saturating_add(1);
                    }
                    Key::Character(';') => {
                        self.pending_particle_smooth =
                            self.pending_particle_smooth.saturating_add(1);
                    }
                    Key::Character('s') => {
                        self.pending_particle_sort = self.pending_particle_sort.saturating_add(1);
                    }
                    Key::Character('d') => {
                        self.pending_particle_billboard =
                            self.pending_particle_billboard.saturating_add(1);
                    }
                    Key::Character('l') => {
                        self.pending_particle_lod = self.pending_particle_lod.saturating_add(1);
                    }
                    Key::Character('i') => {
                        self.pending_particle_flipbook =
                            self.pending_particle_flipbook.saturating_add(1);
                    }
                    Key::Character('5') => {
                        self.pending_particle_shape = self.pending_particle_shape.saturating_add(1);
                    }
                    Key::Character('w') => {
                        self.pending_particle_compound =
                            self.pending_particle_compound.saturating_add(1);
                    }
                    Key::Character('.') => {
                        self.pending_particle_wind = self.pending_particle_wind.saturating_add(1);
                    }
                    Key::Character('0') => {
                        self.pending_particle_fx = self.pending_particle_fx.saturating_add(1);
                    }
                    _ => {}
                }
            }
            InputEvent::Resized { width, height } => self.viewport = (width, height),
            InputEvent::TextInput { character } if self.console.is_open() => {
                self.console_field.insert_char(
                    &mut self.console_state,
                    self.console.input_mut(),
                    character,
                );
            }
            _ => {}
        }
    }

    fn draw(&mut self, renderer: &mut Renderer, _blend: f32) -> mint_render::Result<()> {
        self.drawn.truncate(self.composed_len);

        for action in std::mem::take(&mut self.pending_scale) {
            match action {
                ScaleAction::Down => {
                    let scale = self.terrain.vertical_scale / SCALE_STEP;
                    self.set_scale(renderer, scale);
                }
                ScaleAction::Up => {
                    let scale = self.terrain.vertical_scale * SCALE_STEP;
                    self.set_scale(renderer, scale);
                }
                ScaleAction::Reset => {
                    let scale = self.starting_scale;
                    self.set_scale(renderer, scale);
                }
            }
        }

        for action in std::mem::take(&mut self.pending_road) {
            self.road_key(renderer, action);
        }

        for _ in 0..std::mem::take(&mut self.pending_frustum) {
            let next = !renderer.frustum_culling();
            renderer.set_frustum_culling(next);
            println!(
                "frustum cull = {} (--frustum-cull)",
                if next { "on" } else { "off" }
            );
        }

        for _ in 0..std::mem::take(&mut self.pending_shadow_fit) {
            self.shadow_fit = self.shadow_fit.next();
            let rig = current_rig(&self.rigs, self.time, self.map_light, self.alt_set);
            match (&self.shadows, self.shadow_draw) {
                (Some(pass), TerrainShadows::On) => println!(
                    "{}",
                    pass.report(
                        renderer,
                        self.camera.camera(),
                        &rig,
                        self.tuning_file.tuning(),
                        ShadowFrame {
                            fit: self.shadow_fit,
                            alpha: self.shadow_alpha,
                            world: self.terrain.bounds(),
                        },
                    )
                ),
                _ => println!(
                    "shadow fit = {} (6, --shadow-fit), but shadows are off (mode.shadows)",
                    self.shadow_fit.label()
                ),
            }
        }

        for _ in 0..std::mem::take(&mut self.pending_shadow_alpha) {
            self.shadow_alpha = self.shadow_alpha.next();
            let (instances, calls) = renderer.shadow_alpha_instances();
            println!(
                "shadow alpha = {} (9, --shadow-alpha), {instances} alpha-cut caster(s)                  in {calls} draw call(s) on the frame before this one",
                self.shadow_alpha.label()
            );
        }

        for _ in 0..std::mem::take(&mut self.pending_reflection_cull) {
            self.reflection_cull = self.reflection_cull.next();
            println!(
                "reflection cull = {} (7, --reflection-cull), {} planar target(s)",
                self.reflection_cull.label(),
                self.water_scene.planes.len()
            );
        }

        for _ in 0..std::mem::take(&mut self.pending_depth) {
            let next = match renderer.depth_direction() {
                DepthDirection::Standard => DepthDirection::Reversed,
                DepthDirection::Reversed => DepthDirection::Standard,
            };
            renderer.set_depth_direction(next);
            self.report_depth_precision(next);
        }

        for _ in 0..std::mem::take(&mut self.pending_time) {
            self.time = next_time(self.time);
            println!("time of day = {}", self.time.name());
        }

        for _ in 0..std::mem::take(&mut self.pending_set) {
            self.alt_set = !self.alt_set;
            println!(
                "terrain light set = stored slots {}",
                if self.alt_set { "6,7,8" } else { "0,4,5" }
            );
        }

        for _ in 0..std::mem::take(&mut self.pending_object_material) {
            self.object_material = self.object_material.next();
            self.report_object_material();
        }
        for _ in 0..std::mem::take(&mut self.pending_material_step) {
            self.object_material = self.object_material.stepped();
            self.report_object_material();
        }
        for _ in 0..std::mem::take(&mut self.pending_sway) {
            self.sway = self.sway.next();
            self.sway_written = false;
            println!("{}", self.sway.report());
        }
        for _ in 0..std::mem::take(&mut self.pending_object_animation) {
            self.object_animation = self.object_animation.next();
            println!(
                "object animation = {} (--object-animation)",
                self.object_animation.label()
            );
            self.reset_object_poses(renderer);
        }
        self.advance_objects(renderer);

        self.poll_tuning(renderer);
        self.advance_water(renderer);
        self.terrain_shading
            .push(renderer, self.tuning_file.tuning(), self.water_seconds);
        if let Some(sky) = &mut self.sky {
            sky.push(renderer, self.tuning_file.tuning(), self.water_seconds);
        }
        let eye = self.camera.camera().eye;
        if let (Some(sky), Some(instance)) = (&self.sky, self.drawn.last_mut()) {
            if self.sky_draw.is_on() {
                *instance = sky.instance(eye);
            }
        }
        if let (Some(sky), Some(slot)) = (&self.sky, self.sky_slot) {
            if let Some(instance) = self.reflected.get_mut(slot) {
                *instance = sky.instance(eye);
            }
        }

        for _ in 0..std::mem::take(&mut self.pending_particles) {
            let next = !self.particle_scene.drawn();
            self.particle_scene.set_drawn(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_keys) {
            let next = !self.particle_scene.keys();
            self.particle_scene.set_keys(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_spawn) {
            let at = self.camera.target;
            let name = self.particle_scene.name().to_owned();
            self.particle_scene
                .respawn(renderer, &self.vfs, &self.resolver, &name, at);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_bones) {
            self.particle_bones = self.particle_bones.next();
            self.particle_scene.set_bones(self.particle_bones);
            self.pending_particle_attach = 1;
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_sort) {
            let next = self.particle_scene.sort().next();
            self.particle_scene.set_sort(next);
            println!("{}", self.particle_scene.budget_line());
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_billboard) {
            let next = self.particle_scene.billboard().next();
            self.particle_scene.set_billboard(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_lod) {
            let next = self.particle_scene.detail().next();
            self.particle_scene.set_detail(next);
            self.pending_particle_attach = 1;
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_flipbook) {
            let next = self.particle_scene.flipbook().next();
            self.particle_scene.set_flipbook(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_shape) {
            let next = self.particle_scene.shape().next();
            self.particle_scene.set_shape(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_wind) {
            let next = self.particle_scene.wind().next();
            self.particle_scene.set_wind(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_fx) {
            let at = self.camera.target;
            let ground = MapGround::new(&self.height_map, self.terrain.vertical_scale);
            self.particle_scene
                .fire_fx(renderer, &self.vfs, &self.resolver, at, &ground);
            println!("{}", self.particle_scene.budget_line());
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_compound) {
            let next = !self.particle_scene.compound();
            self.particle_scene.set_compound(next);
            self.pending_particle_attach = 1;
            self.pending_particle_spawn = self.pending_particle_spawn.saturating_add(1);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_smooth) {
            let next = !self.particle_scene.smooth();
            self.particle_scene.set_smooth(next);
        }
        for _ in 0..std::mem::take(&mut self.pending_particle_motion) {
            let next = !self.particle_scene.motion();
            self.particle_scene.set_motion(next);
            self.pending_particle_attach = 1;
            self.pending_particle_spawn = self.pending_particle_spawn.saturating_add(1);
        }
        if std::mem::take(&mut self.pending_particle_attach) > 0 {
            self.particle_scene.clear();
            if let (Some(scene), Some(index)) = (&self.model_scene, &self.object_index) {
                let report = attach_particle_bones(
                    &mut self.particle_scene,
                    renderer,
                    &self.vfs,
                    &self.resolver,
                    index,
                    scene,
                    &self.objects,
                    &self.proxies,
                );
                report_particle_bones(&self.particle_scene, &report, self.particle_bones);
            }
            self.pending_particle_spawn = self.pending_particle_spawn.saturating_add(1);
        }
        if self.object_animation.on() {
            if let Some(scene) = &self.model_scene {
                follow_particle_bones(
                    &mut self.particle_scene,
                    scene,
                    &self.proxies,
                    self.object_animation.completion(),
                    self.object_seconds,
                );
            }
        }
        let now = std::time::Instant::now();
        let elapsed = now
            .duration_since(self.particle_last_frame)
            .as_secs_f32()
            .min(1.0);
        self.particle_last_frame = now;
        self.particle_scene.advance(
            elapsed,
            &MapGround::new(&self.height_map, self.terrain.vertical_scale),
        );
        self.particle_scene
            .draw(renderer, &self.camera.camera(), &mut self.drawn)?;

        let rig = current_rig(&self.rigs, self.time, self.map_light, self.alt_set);
        let shadow = match (&self.shadows, self.shadow_draw) {
            (Some(pass), TerrainShadows::On) => pass.render(
                renderer,
                self.camera.camera(),
                &rig,
                &self.drawn,
                self.tuning_file.tuning(),
                ShadowFrame {
                    fit: self.shadow_fit,
                    alpha: self.shadow_alpha,
                    world: self.terrain.bounds(),
                },
            )?,
            _ => None,
        };
        if self.water_source == WaterReflectionSource::Planar && self.water_draw.on() {
            render_planar_reflections(
                renderer,
                &self.water_scene.planes,
                &self.plane_bounds,
                self.reflection_cull,
                &mut self.reflected,
                self.sky_slot,
                self.camera.camera(),
                &rig,
                self.exposure,
                self.mask,
                shadow,
            )?;
        }
        let console_quads = self.console_quads();
        renderer.render(
            &terrain_scene(
                self.camera.camera(),
                &self.drawn,
                &rig,
                self.exposure,
                self.mask,
                shadow,
            )
            .with_ui_quads(&console_quads),
        )
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::{
        group_by_height, plane_of_body, WaterReflectionSource, WaterScene, MAX_PLANAR_PLANES,
    };

    const RIVENDELL: [f32; 5] = [295.0, 30.0, 295.0, 295.0, 382.0];

    fn scene(plane_of: Vec<Option<usize>>) -> WaterScene {
        WaterScene {
            handles: Vec::new(),
            sky: Vec::new(),
            bump: Vec::new(),
            wave: Vec::new(),
            sky_source: Vec::new(),
            params: Vec::new(),
            shader: None,
            materials: Vec::new(),
            failures: Vec::new(),
            shading: Vec::new(),
            reflects: Vec::new(),
            depth: Vec::new(),
            planes: Vec::new(),
            plane_of,
        }
    }

    fn assign(heights: &[f32], available: usize) -> Vec<Option<usize>> {
        let (distinct, of_body) = group_by_height(heights);
        let created = distinct.len().min(available).min(MAX_PLANAR_PLANES);
        plane_of_body(&of_body, created)
    }

    #[test]
    fn bodies_at_the_same_height_share_a_plane_and_the_order_is_first_seen() {
        let (distinct, of_body) = group_by_height(&RIVENDELL);
        assert_eq!(distinct, vec![295.0, 30.0, 382.0]);
        assert_eq!(of_body, vec![0, 1, 0, 0, 2]);
        assert_eq!(of_body.len(), RIVENDELL.len(), "one entry per body");
    }

    #[test]
    fn heights_within_the_epsilon_are_one_plane_and_just_outside_it_are_two() {
        let (distinct, of_body) = group_by_height(&[100.0, 100.005, 100.02]);
        assert_eq!(
            distinct.len(),
            2,
            "0.005 is inside the epsilon, 0.02 is not"
        );
        assert_eq!(of_body, vec![0, 0, 1]);
    }

    #[test]
    fn every_body_reads_the_plane_at_its_own_height_and_not_the_last_bodys() {
        let scene = scene(assign(&RIVENDELL, 3));
        let read: Vec<Option<usize>> = (0..RIVENDELL.len())
            .map(|slot| scene.plane_index(slot))
            .collect();
        assert_eq!(read, vec![Some(0), Some(1), Some(0), Some(0), Some(2)]);
        for (a, height_a) in RIVENDELL.iter().enumerate() {
            for (b, height_b) in RIVENDELL.iter().enumerate() {
                assert_eq!(
                    scene.plane_index(a) == scene.plane_index(b),
                    (height_a - height_b).abs() < 0.01,
                    "bodies {a} (z {height_a}) and {b} (z {height_b})"
                );
            }
        }
    }

    #[test]
    fn a_body_past_the_plane_cap_falls_back_to_the_environment_source() {
        let heights = [10.0, 20.0, 30.0, 40.0, 50.0];
        let scene = scene(assign(&heights, MAX_PLANAR_PLANES));
        assert_eq!(scene.plane_index(MAX_PLANAR_PLANES), None);
        assert_eq!(
            scene.source(MAX_PLANAR_PLANES, WaterReflectionSource::Planar),
            WaterReflectionSource::Environment
        );
        for slot in 0..MAX_PLANAR_PLANES {
            assert_eq!(scene.plane_index(slot), Some(slot));
            assert_eq!(
                scene.source(slot, WaterReflectionSource::Planar),
                WaterReflectionSource::Planar
            );
        }
    }

    #[test]
    fn a_slot_past_the_end_is_none() {
        let scene = scene(assign(&RIVENDELL, 3));
        assert_eq!(scene.plane_index(RIVENDELL.len()), None);
        assert_eq!(scene.plane_index(usize::MAX), None);
    }
}
