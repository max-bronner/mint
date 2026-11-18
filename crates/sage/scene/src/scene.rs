use mint_render::{TextureData, Vec3};
use mint_sage_ini::{FxListIndex, ParticleIndex};
use mint_sage_map::{cell_to_world, BlendTileData, HeightMapData, MapFile};
use mint_sage_model::{load_object_index, ObjectIndex, Resolver};
use mint_sage_terrain::{
    build_rivers, build_standing_water_tuned, build_standing_waves, load_class_normal_maps,
    load_class_textures, parse_water_transparency, place_objects, road_segments, surface_waves,
    ClassTextures, ObjectProxy, RiverNetwork, RoadNetwork, RoadOptions, RoadSegments, Terrain,
    TerrainTypes, WaterAlphaSource, WaterSurface, WaterTransparency, WaterTuning, WaveField,
    WaveLook, WaveSurface, WATER_UV_WORLD_SIZE,
};
use mint_vfs::Vfs;

use crate::objects_present::rotation_anchors;
use crate::report::{
    print_class_census, report, report_cliffs, report_objects, report_rivers, report_water,
    report_waves, ClassNote,
};
use crate::roads::{build_road_network, report_roads};

const TERRAIN_INI: &str = "data/ini/terrain.ini";

pub(crate) const WATER_INI: &str = "data/ini/water.ini";

#[derive(Debug, Clone)]
pub struct SceneOptions {
    pub cliffs: bool,
    pub scale: f32,
    pub mips: bool,
    pub objects: bool,
    pub proxies: bool,
    pub object_animation_label: &'static str,
    pub roads: bool,
    pub road_options: RoadOptions,
    pub alpha_source: WaterAlphaSource,
    pub water_coarse: bool,
    pub water_label: &'static str,
    pub water_on: bool,
    pub rivers_label: &'static str,
    pub rivers_on: bool,
    pub wave_look: WaveLook,
    pub focus: Option<(u32, u32)>,
    pub particle_budget: Option<Option<usize>>,
    pub particle_name: Option<String>,
    pub fx_name: Option<String>,
}

pub struct MapScene {
    pub height_map: HeightMapData,
    pub blend_tile: BlendTileData,
    pub terrain: Terrain,
    pub loaded: ClassTextures,
    pub normal_maps: Vec<Option<TextureData>>,
    pub map: MapFile,
    pub proxies: Vec<ObjectProxy>,
    pub object_index: Option<ObjectIndex>,
    pub particle_index: ParticleIndex,
    pub particle_lists: FxListIndex,
    pub particle_name: String,
    pub fx_name: String,
    pub particle_ceiling: Option<usize>,
    pub particle_skip_mask: u32,
    pub road_network: RoadNetwork,
    pub road_segments: RoadSegments,
    pub water_surface: WaterSurface,
    pub water_transparency: WaterTransparency,
    pub water_shipped_fade: f32,
    pub river_network: RiverNetwork,
    pub wave_field: WaveField,
    pub wave_surface: WaveSurface,
    pub classes: Vec<ClassNote>,
    pub map_label: String,
    pub focus: Option<Vec3>,
    pub resolver: Resolver,
}

pub fn tuning_ceiling(tuning: &WaterTuning) -> Option<usize> {
    let value = tuning.particle_max_count;
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(value.min(1_000_000.0).round() as usize)
}

pub fn tuning_skip_mask(tuning: &WaterTuning) -> u32 {
    let value = tuning.particle_skip_mask;
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let mask = value.min(255.0).round() as u32;
    mask
}

fn read_map_ini(vfs: &Vfs, map_path: &str) -> Option<mint_sage_ini::Document> {
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let path = format!("{folder}/map.ini");
    let bytes = vfs.read(&path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let table = mint_sage_ini::BlockTable::corpus_path();
    match mint_sage_ini::Document::parse(&path, &text, &table) {
        Ok(document) => Some(document),
        Err(error) => {
            println!("map.ini: {path} FAILED: {error}");
            None
        }
    }
}

fn apply_map_ini_objects(document: &mint_sage_ini::Document, index: &mut ObjectIndex) {
    let patched = index.insert_override_document(document, &mint_sage_ini::MacroTable::new());
    println!(
        "object templates: {} patches {patched} template(s)",
        document.name()
    );
}

pub fn load_map_scene(
    vfs: &Vfs,
    map: MapFile,
    map_label: String,
    options: &SceneOptions,
    tuning: &WaterTuning,
    object_index: Option<ObjectIndex>,
) -> crate::Result<MapScene> {
    let height_map = map.height_map.clone();
    let mut blend_tile = map.blend_tile.clone();
    if !options.cliffs {
        blend_tile.cliff_info_index = vec![0; blend_tile.cliff_info_index.len()];
    }

    let ini = vfs.read(TERRAIN_INI).map_err(|source| crate::Error::Vfs {
        path: TERRAIN_INI.into(),
        source,
    })?;
    let types = TerrainTypes::parse(&String::from_utf8_lossy(&ini));
    println!(
        "{TERRAIN_INI}: {} terrain type(s) with a texture, {} redefinition(s)",
        types.len(),
        types.redefinitions()
    );

    let water_transparency = match vfs.read(WATER_INI) {
        Ok(bytes) => parse_water_transparency(&String::from_utf8_lossy(&bytes)),
        Err(error) => {
            println!("{WATER_INI}: {error} - using the shipped defaults");
            WaterTransparency::default()
        }
    };
    println!(
        "{WATER_INI}: TransparentWaterDepth {} x TransparentWaterMinOpacity {} = {} world unit(s)          of depth fade (BaseHeightMap.cpp:1606)",
        water_transparency.depth,
        water_transparency.min_opacity,
        water_transparency.depth * water_transparency.min_opacity,
    );
    let water_shipped_fade = water_transparency.depth;
    let water_transparency = WaterTransparency {
        depth: water_transparency.depth * tuning.water_shore_fade,
        map_depth: map
            .environment
            .as_ref()
            .and_then(|env| env.water_max_alpha_depth)
            .filter(|depth| depth.is_finite() && *depth > 0.0),
        map_alpha: map
            .environment
            .as_ref()
            .and_then(|env| env.deep_water_alpha)
            .filter(|alpha| alpha.is_finite()),
        source: options.alpha_source,
        ..water_transparency
    };
    match &map.environment {
        Some(env) => println!(
            "EnvironmentData v{}: waterMaxAlphaDepth {:?}, deepWaterAlpha {:?}, \
             macro {} (stretched {}), cloud {} - alpha source = {} (mode.water_alpha)",
            env.version,
            env.water_max_alpha_depth,
            env.deep_water_alpha,
            env.macro_texture,
            env.is_macro_texture_stretched,
            env.cloud_texture,
            options.alpha_source.label(),
        ),
        None => println!("EnvironmentData: absent - water.ini's numbers are the only ones"),
    }

    let load_started = std::time::Instant::now();
    let mut loaded = load_class_textures(vfs, &types, &blend_tile.texture_classes);
    let load_time = load_started.elapsed();
    let mip_started = std::time::Instant::now();
    for (texture, class) in loaded
        .textures
        .iter_mut()
        .zip(blend_tile.texture_classes.iter())
    {
        if let Some(texture) = texture.as_mut() {
            let levels =
                mint_sage_terrain::class_mip_levels(texture.width(), texture.height(), class.width);
            texture.generate_mipmaps(levels);
        }
    }
    println!(
        "textures: {:.1} ms to resolve and decode, {:.1} ms of that shape's mip chains",
        load_time.as_secs_f64() * 1000.0,
        mip_started.elapsed().as_secs_f64() * 1000.0
    );
    let (mut normal_maps, normal_failures) =
        load_class_normal_maps(vfs, &types, &blend_tile.texture_classes);
    if !options.mips {
        for texture in loaded
            .textures
            .iter_mut()
            .chain(normal_maps.iter_mut())
            .flatten()
        {
            texture.levels.truncate(1);
        }
    }
    let levels: std::collections::BTreeMap<usize, usize> = loaded.textures.iter().flatten().fold(
        std::collections::BTreeMap::new(),
        |mut acc, texture| {
            *acc.entry(texture.levels.len()).or_default() += 1;
            acc
        },
    );
    let texture_bytes: usize = loaded
        .textures
        .iter()
        .flatten()
        .flat_map(|texture| texture.levels.iter())
        .map(|level| level.data.len())
        .sum();
    println!(
        "texture levels per class: {levels:?}, {:.2} MiB of texture in all",
        texture_bytes as f64 / (1024.0 * 1024.0)
    );
    println!(
        "{} of {} texture class(es) resolved to a texture",
        loaded
            .textures
            .iter()
            .filter(|texture| texture.is_some())
            .count(),
        blend_tile.texture_classes.len()
    );
    for (index, error) in &loaded.failures {
        println!("  class {index}: {error}");
    }
    println!(
        "{} of {} texture class(es) carry a normal map",
        normal_maps.iter().filter(|map| map.is_some()).count(),
        blend_tile.texture_classes.len()
    );
    for (index, error) in &normal_failures {
        println!("  class {index} normal map: {error}");
    }
    print_class_census(&blend_tile, &types, &loaded);

    let build_started = std::time::Instant::now();
    let terrain = Terrain::build(&height_map, &blend_tile, options.scale);
    let build_time = build_started.elapsed();
    report(&map_label, &height_map, &terrain, options.scale, build_time);
    report_cliffs(&height_map, &map.blend_tile);

    let resolver = Resolver::new();
    let handed = object_index;
    let object_index = (options.objects || options.roads || options.proxies).then(|| {
        if let Some(index) = handed {
            println!();
            println!(
                "object templates: from the caller's parse; {} object name(s), {} road(s), \
                 {} terrain type(s), {} redefinition(s)",
                index.len(),
                index.roads(),
                index.terrains(),
                index.redefinitions(),
            );
            return index;
        }
        let started = std::time::Instant::now();
        let (index, report) = load_object_index(vfs);
        println!();
        println!(
            "object templates: {} document(s) under data/ini, {} top-level, {} parsed; \
             {} object name(s), {} road(s), {} terrain type(s), {} redefinition(s), \
             in {:.1} ms",
            report.entries,
            report.top_level,
            report.parsed,
            index.len(),
            index.roads(),
            index.terrains(),
            index.redefinitions(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        for line in report.failures.iter().take(20) {
            println!("  FAILED {line}");
        }
        index
    });
    let map_ini = read_map_ini(vfs, &map_label);
    let object_index = object_index.map(|mut index| {
        if let Some(document) = &map_ini {
            apply_map_ini_objects(document, &mut index);
        }
        index
    });

    let proxies = place_objects(
        &height_map,
        options.scale,
        &map.objects,
        &rotation_anchors(object_index.as_ref(), &map.objects),
    );
    report_objects(
        &proxies,
        options.objects,
        options.proxies,
        options.object_animation_label,
        height_map.border_size,
        &height_map,
        options.scale,
    );

    let (particle_index, particle_lists) = {
        let started = std::time::Instant::now();
        let (systems, lists, report) =
            mint_sage_particle::load_particle_index_over(vfs, map_ini.as_ref());
        println!();
        println!(
            "particle systems: {} of {} document(s) parsed, {} statement(s); {} system(s),              {} FXList(s), {} stray End(s), {} unclosed, in {:.1} ms",
            report.parsed,
            report.documents,
            report.statements,
            systems.len(),
            lists.len(),
            report.stray_ends,
            report.unclosed_blocks,
            started.elapsed().as_secs_f64() * 1000.0
        );
        for line in report.failures.iter().take(20) {
            println!("  FAILED {line}");
        }
        let mut references = 0usize;
        let mut resolved = 0usize;
        for (_, system) in lists.particle_references() {
            references = references.saturating_add(1);
            resolved = resolved.saturating_add(usize::from(systems.get(system).is_some()));
        }
        println!("  {resolved} of {references} FXList particle reference(s) resolve");
        (systems, lists)
    };

    let fx_name = options
        .fx_name
        .clone()
        .unwrap_or_else(|| tuning.particle_fxlist.as_str().to_owned());
    let particle_name = options
        .particle_name
        .clone()
        .unwrap_or_else(|| tuning.particle_system.as_str().to_owned());
    if particle_name.is_empty() {
        println!("  no particle system selected, so nothing will spawn");
        println!("    add `particle.system = SmokeIsengardPit` to the tuning file above,");
        println!("    or pass --particle <name>. Deleting the file regenerates it with");
        println!("    the new section 4 in place.");
    }

    let particle_ceiling = options
        .particle_budget
        .unwrap_or_else(|| tuning_ceiling(tuning));
    let particle_skip_mask = tuning_skip_mask(tuning);

    let road_started = std::time::Instant::now();
    let road_network = build_road_network(
        &height_map,
        options.scale,
        &map.objects,
        object_index.as_ref(),
        &options.road_options,
    );
    let road_segments = road_segments(&map.objects);
    report_roads(
        &road_network,
        object_index.as_ref(),
        &options.road_options,
        options.roads,
        road_started.elapsed(),
    );

    let water_started = std::time::Instant::now();
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let vertex_budget = tuning.water_vertex_budget.clamp(1024.0, 8_000_000.0) as usize;
    let water_surface = build_standing_water_tuned(
        map.water.standing_areas(),
        WATER_UV_WORLD_SIZE,
        if options.water_coarse {
            None
        } else {
            Some(&height_map)
        },
        tuning.water_tessellation_edge,
        vertex_budget,
    );
    report_water(
        &map,
        &water_surface,
        options.water_label,
        options.water_on,
        water_started.elapsed(),
    );

    let river_started = std::time::Instant::now();
    let river_network = build_rivers(map.water.river_areas());
    report_rivers(
        &river_network,
        options.rivers_label,
        options.rivers_on,
        river_started.elapsed(),
    );

    let wave_started = std::time::Instant::now();
    let wave_field = build_standing_waves(
        map.water.wave_areas(),
        map.water.standing_areas(),
        Some(&height_map),
        tuning,
    );
    let wave_surface = surface_waves(&wave_field, options.wave_look, tuning);
    report_waves(
        &wave_field,
        &wave_surface,
        options.wave_look,
        wave_started.elapsed(),
    );

    let focus = options.focus.map(|(fx, fy)| {
        Vec3::new(
            cell_to_world(
                i32::try_from(fx).unwrap_or(i32::MAX),
                height_map.border_size,
            ),
            cell_to_world(
                i32::try_from(fy).unwrap_or(i32::MAX),
                height_map.border_size,
            ),
            f32::from(height_map.sample(fx, fy).unwrap_or(0)) * options.scale,
        )
    });

    let classes: Vec<ClassNote> = blend_tile
        .texture_classes
        .iter()
        .enumerate()
        .map(|(index, class)| ClassNote {
            name: class.name.clone(),
            file: types.texture(&class.name).map(str::to_owned),
            loaded: loaded.textures.get(index).is_some_and(Option::is_some),
        })
        .collect();

    Ok(MapScene {
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
    })
}
