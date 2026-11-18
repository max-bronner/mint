use mint_render::{Instance, Renderer};
use mint_sage_asset::Breeze;
use mint_sage_map::MapFile;
use mint_sage_scene::{
    attach_particle_bones, build_object_scene, model_instances, river_instances, road_instances,
    terrain_rig, upload_rivers, upload_roads, upload_terrain, upload_terrain_shading, upload_water,
    upload_waves, water_instances, wave_instances, ObjectScene, ParticleScene, RiverScene,
    RoadScene, SceneOptions, ShadowPass, TerrainRig, TerrainShading, WaterScene, WaveScene,
};
use mint_sage_terrain::present::{upload_class_normal_maps, upload_class_textures};
use mint_sage_terrain::{
    ObjectProxy, RiverNetwork, Terrain, TerrainCloud, TerrainMacro, WaterDistortion,
    WaterHighlights, WaterReflection, WaterReflectionComposite, WaterReflectionSource, WaterTint,
    WaterTuning, WaveField, WaveLook, WaveSurface,
};
use mint_vfs::Vfs;

const ACTIVE_STATIC_LOD: mint_sage_particle::DetailLevel =
    mint_sage_particle::DetailLevel::UltraHigh;

fn static_lod_ceiling(vfs: &Vfs) -> Option<usize> {
    mint_sage_particle::static_lod_max_particles(&static_lod_documents(vfs)?, ACTIVE_STATIC_LOD)
}

fn static_lod_documents(vfs: &Vfs) -> Option<[mint_sage_ini::Document; 1]> {
    const PATH: &str = "data/ini/gamelod.ini";
    let bytes = vfs.read(PATH).ok()?;
    let text = mint_sage_ini::lex::decode_latin1(&bytes);
    mint_sage_ini::Document::parse(PATH, &text, &mint_sage_ini::BlockTable::corpus_path())
        .ok()
        .map(|document| [document])
}

pub(crate) fn static_lod_shadow_decals(vfs: &Vfs) -> bool {
    let decals = static_lod_documents(vfs).and_then(|documents| {
        mint_sage_particle::static_lod_shadow_decals(&documents, ACTIVE_STATIC_LOD)
    });
    println!(
        "decals: StaticGameLOD {} {}",
        ACTIVE_STATIC_LOD.label(),
        match decals {
            Some(true) => "makes shadow decals",
            Some(false) => "makes no shadow decals; only ShadowOverrideLODVisibility ones draw",
            None => "does not say; every shadow decal draws",
        }
    );
    decals.unwrap_or(true)
}

pub(crate) fn static_lod_effects_index(vfs: &Vfs) -> u8 {
    use mint_sage_particle::DetailLevel;
    let level = static_lod_documents(vfs).and_then(|documents| {
        mint_sage_particle::static_lod_effects(&documents, ACTIVE_STATIC_LOD)
    });
    match level {
        Some(DetailLevel::VeryLow) => 0,
        Some(DetailLevel::Low) => 1,
        Some(DetailLevel::Medium) => 2,
        Some(DetailLevel::High) => 3,
        Some(DetailLevel::UltraHigh | DetailLevel::Off) | None => {
            crate::shell_map::EFFECTS_LOD_ULTRA_HIGH
        }
    }
}

fn static_lod_effects_open(vfs: &Vfs) -> bool {
    static_lod_documents(vfs)
        .and_then(|documents| mint_sage_particle::static_lod_effects(&documents, ACTIVE_STATIC_LOD))
        .is_none_or(|effects| {
            use mint_sage_particle::DetailLevel;
            matches!(
                effects,
                DetailLevel::Medium | DetailLevel::High | DetailLevel::UltraHigh
            )
        })
}

fn scene_options(vfs: &Vfs) -> SceneOptions {
    SceneOptions {
        cliffs: true,
        scale: 5.0 / 128.0,
        mips: true,
        objects: true,
        proxies: false,
        object_animation_label: "play",
        roads: true,
        road_options: mint_sage_terrain::RoadOptions::default(),
        alpha_source: mint_sage_terrain::WaterAlphaSource::default(),
        water_coarse: false,
        water_label: "default",
        water_on: true,
        rivers_label: "shader",
        rivers_on: true,
        wave_look: WaveLook::default(),
        focus: None,
        particle_budget: static_lod_ceiling(vfs).map(Some),
        particle_name: None,
        fx_name: None,
    }
}

const TERRAIN_BLEND: bool = true;

pub(crate) struct MatchScene {
    pub(crate) terrain: Terrain,
    pub(crate) drawn: Vec<Instance>,
    pub(crate) composed_len: usize,
    pub(crate) palette: Vec<mint_render::Mat4>,
    pub(crate) surface_overlays: Vec<usize>,
    pub(crate) object_span: std::ops::Range<usize>,
    pub(crate) tree_pose_drawn: Vec<Option<crate::forest::TreePose>>,
    pub(crate) standing_placement: Vec<Option<mint_render::Mat4>>,
    pub(crate) placement_groups: Vec<Vec<(usize, usize)>>,
    pub(crate) forest_revision_seen: Option<u32>,
    pub(crate) proxy_of_object: Vec<Option<u32>>,
    pub(crate) removed_drawn: Vec<bool>,
    pub(crate) forest_seeded: bool,
    pub(crate) forest_known: Vec<u32>,
    pub(crate) base_tints: Vec<[f32; 4]>,
    pub(crate) object_proxy_slots: Vec<Option<usize>>,
    pub(crate) object_trees: Vec<bool>,

    pub(crate) rig: TerrainRig,
    pub(crate) sun: mint_sage_scene::SunConvention,
    pub(crate) terrain_shading: TerrainShading,
    pub(crate) shadows: Option<ShadowPass>,
    pub(crate) world_bounds: mint_render::Aabb,
    pub(crate) tuning: WaterTuning,
    pub(crate) tuning_file: Option<mint_sage_terrain::TuningFile>,
    pub(crate) model_lights: mint_sage_scene::ModelLights,

    pub(crate) object_scene: Option<ObjectScene>,
    pub(crate) proxies: Vec<ObjectProxy>,
    pub(crate) object_index: Option<mint_sage_model::ObjectIndex>,
    pub(crate) resolver: mint_sage_asset::Resolver,

    pub(crate) water_span: std::ops::Range<usize>,
    pub(crate) fx_span: std::ops::Range<usize>,
    pub(crate) fx_water: mint_sage_scene::water_fx::FxWater,
    pub(crate) wave_tracks: mint_sage_scene::wave_tracks::WaveTracks,
    pub(crate) road_scene: RoadScene,
    pub(crate) shroud_extent: [f32; 4],
    pub(crate) shroud_texture: Option<mint_render::TextureHandle>,
    pub(crate) reflection: crate::reflection::ReflectionRung,
    pub(crate) border_in_mirror: crate::reflection::BorderInMirror,
    pub(crate) wave_rung: mint_sage_scene::WaveRung,
    pub(crate) wave_surface_tall: WaveSurface,
    pub(crate) no_swell: WaveSurface,
    pub(crate) object_reflects: Vec<bool>,
    pub(crate) mirror_sky: Option<mint_sage_terrain::SkyScene>,
    pub(crate) reflected: Vec<Instance>,
    pub(crate) mirror_counts: crate::reflection::MirrorCounts,
    pub(crate) mirror_skip: bool,
    pub(crate) mirror_tally: crate::reflection::MirrorTally,

    pub(crate) water_scene: WaterScene,
    pub(crate) wave_surface: WaveSurface,
    pub(crate) river_scene: RiverScene,
    pub(crate) river_network: RiverNetwork,
    pub(crate) wave_scene: WaveScene,
    pub(crate) wave_field: WaveField,

    pub(crate) particle_scene: ParticleScene,
    pub(crate) condition_particles: mint_sage_scene::ConditionParticles,
    pub(crate) map_path: String,
    pub(crate) height_map: mint_sage_map::HeightMapData,
    pub(crate) water: mint_sage_map::Water,
    pub(crate) wet: mint_sage_passability::water::WetGrid,
    pub(crate) objects: Vec<mint_sage_map::Object>,
    pub(crate) map_scripts: Option<crate::rules::map_scripts::MapScriptEngine>,
    pub(crate) map_script_lines: Vec<String>,
    pub(crate) referenced_unit_names: std::collections::BTreeSet<String>,
    pub(crate) sides: Vec<mint_sage_map::Side>,
    pub(crate) blend_tile: mint_sage_map::BlendTileData,
    pub(crate) particle_seconds: f32,
    pub(crate) snowy: bool,
    pub(crate) snow: mint_sage_scene::SnowScene,
    pub(crate) breeze: BreezeRung,
    pub(crate) cloud_setting: mint_sage_ini::CloudDarkeningSetting,
    pub(crate) cloud_darkening: mint_sage_scene::CloudDarkening,
    pub(crate) cloud_darkening_on: bool,
    pub(crate) fog: Option<mint_render::DistanceFog>,
    pub(crate) fog_on: bool,
    pub(crate) fog_shift: f32,
    pub(crate) post: mint_sage_scene::post_present::PostPresent,
    pub(crate) script_bloom: Option<bool>,
    pub(crate) script_view: crate::rules::map_scripts::ScriptView,
    pub(crate) render_target: Option<mint_render::RenderTargetHandle>,
    pub(crate) script_camera: crate::script_camera::ScriptCamera,
    pub(crate) script_pose: crate::script_pose::ScriptPose,
    pub(crate) camera_freeze: crate::script_camera::FreezeGate,
    pub(crate) camera_animations: Vec<mint_sage_map::CameraAnimation>,
    pub(crate) camera_keys: crate::camera_limits::MapCameraKeys,
    pub(crate) camera_area: Option<crate::camera_area::AreaRestriction>,
    pub(crate) camera_area_seen: Option<u32>,
    pub(crate) terrain_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BreezeRung {
    #[default]
    LaterTitle,
    Generals,
    Still,
}

impl BreezeRung {
    pub(crate) const fn breeze(self) -> Breeze {
        match self {
            Self::LaterTitle => Breeze::LATER_TITLE,
            Self::Generals => Breeze::GENERALS,
            Self::Still => Breeze::STILL,
        }
    }

    pub(crate) const fn next(self) -> Self {
        match self {
            Self::LaterTitle => Self::Generals,
            Self::Generals => Self::Still,
            Self::Still => Self::LaterTitle,
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::LaterTitle => "later-title",
            Self::Generals => "generals",
            Self::Still => "still",
        }
    }
}

pub(crate) type SceneStep = Box<dyn FnOnce(&mut Renderer, &Vfs) -> Result<SceneProgress, String>>;

pub(crate) enum SceneProgress {
    More(SceneStep),
    Done(Box<MatchScene>),
}

fn step(
    next: impl FnOnce(&mut Renderer, &Vfs) -> Result<SceneProgress, String> + 'static,
) -> SceneStep {
    Box::new(next)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn load_match_scene(
    renderer: &mut Renderer,
    vfs: &Vfs,
    map_bytes: &[u8],
    map_label: String,
    templates: &mint_sage_modules::TemplateRecords,
    skirmish: bool,
    allow_ring_heroes: bool,
    object_index: Option<mint_sage_model::ObjectIndex>,
    tuning_file: Option<mint_sage_terrain::TuningFile>,
) -> Result<MatchScene, String> {
    let mut next = begin_match_scene(
        vfs,
        map_bytes,
        map_label,
        templates,
        skirmish,
        allow_ring_heroes,
        object_index,
        tuning_file,
    )?;
    loop {
        match next(renderer, vfs)? {
            SceneProgress::More(step) => next = step,
            SceneProgress::Done(scene) => return Ok(*scene),
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn begin_match_scene(
    vfs: &Vfs,
    map_bytes: &[u8],
    map_label: String,
    templates: &mint_sage_modules::TemplateRecords,
    skirmish: bool,
    allow_ring_heroes: bool,
    object_index: Option<mint_sage_model::ObjectIndex>,
    tuning_file: Option<mint_sage_terrain::TuningFile>,
) -> Result<SceneStep, String> {
    let mut map = MapFile::parse(map_bytes).map_err(|e| format!("{map_label}: {e}"))?;
    let sides = map.sides().map_err(|e| format!("{map_label}: {e}"))?;
    let objects = map.objects.clone();
    let scripts = map.scripts.clone();
    let library_lists = map
        .library_map_lists()
        .map_err(|e| format!("{map_label}: {e}"))?;
    let mut team_defs = map.teams().map_err(|e| format!("{map_label}: {e}"))?;
    let trigger_areas = map
        .trigger_areas()
        .map_err(|e| format!("{map_label}: {e}"))?;
    let named_cameras = map
        .named_cameras()
        .map_err(|e| format!("{map_label}: {e}"))?;
    let camera_animations = map
        .camera_animations()
        .map(|list| list.animations)
        .unwrap_or_default();
    let (map_scripts, library_teams, script_lines) = crate::skirmish::build_map_scripts(
        &objects,
        &sides,
        scripts.as_ref(),
        &team_defs,
        &library_lists,
        &trigger_areas,
        &named_cameras,
        allow_ring_heroes,
        &|path| vfs.read(path).ok(),
    );
    team_defs.extend(library_teams);
    let team_protos = crate::rules::map_scripts::TeamProtos::from_defs(&team_defs)
        .map_err(|e| format!("{map_label}: {e}"))?;
    let path_points = path_points_of(&map, &objects);
    let map_scripts = map_scripts.map(|engine| {
        engine
            .with_team_prototypes(team_protos)
            .with_path_points(path_points)
    });
    let mut script_lines = script_lines;
    let map_scripts = map_scripts.map(|engine| {
        let Some(list) = crate::objectives::read_beside(vfs, &map_label) else {
            return engine;
        };
        script_lines.extend(list.refused.iter().map(|line| {
            format!("script: map.ini MissionObjectiveList refuses the line \"{line}\"")
        }));
        script_lines.push(format!(
            "script: {} mission objective(s) from map.ini",
            list.entries.len()
        ));
        engine.with_objectives(list.entries)
    });
    let map_scripts = map_scripts.map(|engine| {
        let names = engine.finished_audio_names();
        if names.is_empty() {
            return engine;
        }
        let lengths = crate::audio::sound_lengths(vfs, &names);
        script_lines.push(format!(
            "script: {} of {} awaited sound event(s) have a length",
            lengths.len(),
            names.len()
        ));
        engine.with_sound_lengths(lengths)
    });
    let referenced_unit_names = map_scripts
        .as_ref()
        .map(crate::rules::map_scripts::MapScriptEngine::referenced_unit_names)
        .unwrap_or_default();
    let as_entity = |object: &mint_sage_map::Object| {
        skirmish && crate::map_players::places_entity(templates, object, &referenced_unit_names)
    };
    let mut proxy_of_object: Vec<Option<u32>> = vec![None; objects.len()];
    let mut next_proxy = 0u32;
    for (index, object) in objects.iter().enumerate() {
        if !as_entity(object) {
            if let Some(slot) = proxy_of_object.get_mut(index) {
                *slot = Some(next_proxy);
            }
            next_proxy = next_proxy.saturating_add(1);
        }
    }
    map.objects.retain(|object| !as_entity(object));
    let as_entities = objects.len().saturating_sub(map.objects.len());
    let tuning = tuning_file
        .as_ref()
        .map_or_else(WaterTuning::default, |file| *file.tuning());
    let options = scene_options(vfs);
    let global_lighting = map.global_lighting.clone();
    let built = mint_sage_scene::load_map_scene(
        vfs,
        map,
        map_label.clone(),
        &options,
        &tuning,
        object_index,
    )
    .map_err(|e| e.to_string())?;

    Ok(step(move |renderer, vfs| {
        let textures = upload_class_textures(renderer, &built.loaded).map_err(|e| e.to_string())?;
        let normal_maps =
            upload_class_normal_maps(renderer, &built.normal_maps).map_err(|e| e.to_string())?;
        let resolver = built.resolver.clone();

        let terrain_shading = upload_terrain_shading(
            renderer,
            vfs,
            &resolver,
            built.map.environment.as_ref(),
            global_lighting.no_cloud_factor,
            global_lighting.overbright,
            built.terrain.bounds(),
            &textures,
            &normal_maps,
            TerrainCloud::default(),
            TerrainMacro::default(),
            &tuning,
        );
        let (_handles, mut drawn) = upload_terrain(
            renderer,
            &built.terrain,
            &textures,
            TERRAIN_BLEND,
            &terrain_shading,
        )
        .map_err(|e| e.to_string())?;
        let terrain_len = drawn.len();

        Ok(SceneProgress::More(step(move |renderer, vfs| {
            let road_scene = upload_roads(
                renderer,
                vfs,
                &resolver,
                built.object_index.as_ref(),
                &built.road_network,
            )
            .map_err(|e| e.to_string())?;
            drawn.extend(road_instances(&road_scene));

            let water_scene = upload_water(
                renderer,
                vfs,
                &resolver,
                &built.water_surface,
                built.water_transparency,
                options.scale,
                WaterTint::default(),
                WaterReflection::default(),
                WaterReflectionSource::Environment,
                WaterReflectionComposite::default(),
                WaterHighlights::default(),
                WaterDistortion::default(),
                &tuning,
                &built.wave_surface,
            )
            .map_err(|e| e.to_string())?;
            let water_span_start = drawn.len();
            drawn.extend(water_instances(
                &water_scene,
                &built.water_surface,
                WaterTint::default(),
            ));

            Ok(SceneProgress::More(step(move |renderer, vfs| {
                let river_scene = upload_rivers(
                    renderer,
                    vfs,
                    &resolver,
                    &built.river_network,
                    &built.water_surface,
                    &water_scene.planes,
                    mint_sage_terrain::RiverLook::default(),
                    WaterDistortion::default().refraction,
                    &tuning,
                )
                .map_err(|e| e.to_string())?;
                drawn.extend(river_instances(&river_scene, &built.river_network));

                let wave_scene = upload_waves(
                    renderer,
                    vfs,
                    &resolver,
                    &built.wave_field,
                    options.wave_look,
                    &tuning,
                )
                .map_err(|e| e.to_string())?;
                drawn.extend(wave_instances(
                    &wave_scene,
                    &built.wave_field,
                    WaveLook {
                        pca: mint_sage_terrain::WavePca::Ignore,
                        ..options.wave_look
                    },
                ));
                let wave_surface_tall = mint_sage_terrain::surface_waves(
                    &built.wave_field,
                    options.wave_look,
                    &WaterTuning {
                        wave_surface_amplitude: tuning.wave_surface_amplitude
                            * mint_sage_scene::WaveRung::BothTall.swell_gain(),
                        ..tuning
                    },
                );

                Ok(SceneProgress::More(step(move |renderer, vfs| {
                    let fx_rig = terrain_rig(
                        &global_lighting,
                        mint_sage_scene::default_time_of_day(&global_lighting),
                    );
                    let fx_water_diffuse = vfs
                        .read(crate::scene::WATER_INI_PATH)
                        .ok()
                        .and_then(|bytes| {
                            mint_sage_terrain::parse_water_set(
                                &String::from_utf8_lossy(&bytes),
                                Some(mint_sage_scene::water_set_name(
                                    mint_sage_scene::default_time_of_day(&global_lighting),
                                )),
                            )
                        })
                        .map_or([1.0; 4], |set| set.diffuse);
                    let fx_water = mint_sage_scene::water_fx::FxWater::load(
                        renderer,
                        vfs,
                        &resolver,
                        &built.map.water,
                        &built.water_surface,
                        &built.river_network,
                        &built.height_map,
                        options.scale,
                        &fx_rig,
                        built.water_transparency,
                        &water_scene.planes,
                        fx_water_diffuse,
                        mint_sage_scene::water_fx::MeshDensity::Fine,
                    );
                    for line in &fx_water.report {
                        println!("water fx: {line}");
                    }
                    let wave_tracks = mint_sage_scene::wave_tracks::WaveTracks::load(
                        renderer,
                        vfs,
                        &resolver,
                        &built.wave_field,
                    );
                    println!(
        "water fx: {} wave track(s); {} enablePcaWave area(s) left to the PCA displacement",
        wave_tracks.count, wave_tracks.pca_skipped
    );
                    let mint_water_span = water_span_start..drawn.len();
                    drawn.extend(fx_water.instances());
                    drawn.extend(wave_tracks.instances());
                    let fx_span = mint_water_span.end..drawn.len();
                    let water_span = water_span_start..drawn.len();
                    if let Some(slice) = drawn.get_mut(mint_water_span) {
                        for instance in slice {
                            instance.visible = false;
                        }
                    }
                    if let Some(slice) = drawn.get_mut(fx_span.clone()) {
                        let per_surface = fx_water.visibility();
                        for (n, instance) in slice.iter_mut().enumerate() {
                            instance.visible = per_surface.get(n).copied().unwrap_or(true);
                        }
                    }

                    Ok(SceneProgress::More(step(move |renderer, vfs| {
                        let water_set = mint_sage_scene::water_set_name(
                            mint_sage_scene::default_time_of_day(&global_lighting),
                        )
                        .to_owned();
                        let mirror_sky = if water_scene.planes.is_empty() {
                            None
                        } else {
                            let (cloud, set, failures) = mint_sage_scene::load_water_set_sky(
                                renderer,
                                vfs,
                                &resolver,
                                renderer.supports_block_compression(),
                                Some(&water_set),
                            );
                            for line in &failures {
                                println!("  MIRROR SKY {line}");
                            }
                            println!(
                                "water: sky WaterSet {water_set} - {}",
                                set.as_ref().map_or_else(
                                    || "NOT FOUND, morning terms".to_owned(),
                                    |set| format!(
                                        "{} tint {:.3} travel {} repeat {}",
                                        set.texture.as_deref().unwrap_or("no texture"),
                                        set.plane.tint,
                                        set.plane.travel,
                                        set.plane.repeat
                                    )
                                )
                            );
                            match mint_sage_terrain::SkyScene::load(
                                renderer,
                                cloud,
                                mint_sage_terrain::SkyDraw::Plane,
                                0.0,
                                &tuning,
                            )
                            .map(|mut sky| {
                                if let Some(set) = &set {
                                    sky.set_plane_terms(set.plane);
                                }
                                sky
                            }) {
                                Ok(sky) => Some(sky),
                                Err(error) => {
                                    println!("  MIRROR SKY {error}");
                                    None
                                }
                            }
                        };
                        let composed_len_before_objects = drawn.len();
                        let snowy = matches!(
                            built.map.world_info.properties.get("weather"),
                            Some(mint_sage_map::Value::Int(1))
                        );
                        let mut object_scene = built.object_index.as_ref().map(|index| {
                            build_object_scene(renderer, vfs, &resolver, index, &built.map.objects)
                        });
                        if let (Some(scene), Some(index), true) =
                            (object_scene.as_mut(), built.object_index.as_ref(), snowy)
                        {
                            mint_sage_scene::apply_object_conditions(
                                scene,
                                renderer,
                                vfs,
                                &resolver,
                                index,
                                &built.map.objects,
                                mint_sage_scene::Conditions::SNOW,
                            );
                        }
                        let (snow_setting, snow_line) =
                            mint_sage_scene::snow_present::load_snow_setting(vfs, &map_label);
                        println!(
                            "{snow_line}; map weather {}",
                            if snowy { "SNOWY" } else { "NORMAL" }
                        );
                        let fog = mint_sage_scene::distance_fog(&snow_setting.fog);
                        println!(
        "fog: {} - HardwareFogEnable {}, colour {:?}, start {}, end {} (Ctrl+O toggles)",
        if fog.is_some() { "on" } else { "off" },
        snow_setting.fog.enabled,
        snow_setting.fog.color,
        snow_setting.fog.start,
        snow_setting.fog.end
    );
                        let snow =
                            mint_sage_scene::SnowScene::new(renderer, vfs, &resolver, snow_setting);
                        let (cloud_setting, cloud_line) =
                            mint_sage_scene::load_cloud_darkening(vfs, &map_label);
                        println!("{cloud_line}");
                        let (glow_setting, glow_line) =
                            mint_sage_scene::post_present::load_glow_effect(vfs, &map_label);
                        println!("{glow_line}");
                        let lookup = mint_sage_scene::post_present::upload_lookup_table(
                            renderer,
                            vfs,
                            &resolver,
                            &built.map.post_effects,
                        );
                        if let Err(reason) = &lookup {
                            println!("post: {reason}");
                        }
                        let post = mint_sage_scene::post_present::PostPresent::new(
                            glow_setting,
                            &global_lighting,
                            lookup.ok(),
                            static_lod_effects_open(vfs),
                        );
                        println!("{}", post.describe());
                        if let Some(scene) = object_scene.as_mut() {
                            drawn.extend(model_instances(scene, &built.proxies));
                        }
                        let object_span = composed_len_before_objects..drawn.len();
                        let reflecting = crate::reflection::reflecting_templates(vfs);
                        let mut object_reflects = vec![false; object_span.len()];
                        let mut object_proxy_slots = vec![None; object_span.len()];
                        let mut object_trees = vec![false; object_span.len()];
                        if let Some(scene) = object_scene.as_ref() {
                            for group in &scene.groups {
                                let count = group.placements.len().max(1);
                                for i in group.span.clone() {
                                    let n = i.saturating_sub(group.span.start) % count;
                                    if let Some(slot) = object_proxy_slots.get_mut(i) {
                                        *slot = group.placements.get(n).copied();
                                    }
                                    if let Some(tree) = object_trees.get_mut(i) {
                                        *tree = group.sways;
                                    }
                                    let casts = group
                                        .placements
                                        .get(n)
                                        .and_then(|slot| built.proxies.get(*slot))
                                        .and_then(|proxy| built.map.objects.get(proxy.index))
                                        .is_some_and(|o| {
                                            reflecting
                                                .contains(&o.template_name.to_ascii_lowercase())
                                        });
                                    if let Some(flag) = object_reflects.get_mut(i) {
                                        *flag = casts;
                                    }
                                }
                            }
                        }
                        for (offset, tree) in object_trees.iter().enumerate() {
                            if let (true, Some(instance)) = (
                                *tree,
                                drawn.get_mut(object_span.start.saturating_add(offset)),
                            ) {
                                instance.alpha_cutoff = mint_sage_model::cache::ALPHA_CUTOFF;
                            }
                        }
                        println!(
        "water: {} object template(s) cast reflections; {} of {} placed instance(s) do",
        reflecting.len(),
        object_reflects.iter().filter(|f| **f).count(),
        object_reflects.len()
    );
                        let composed_len = drawn.len();
                        let surface_overlays: Vec<usize> = drawn
                            .iter()
                            .enumerate()
                            .filter(|(_, instance)| instance.surface_overlay)
                            .map(|(slot, _)| slot)
                            .collect();
                        let base_tints: Vec<[f32; 4]> = drawn
                            .get(object_span.clone())
                            .unwrap_or_default()
                            .iter()
                            .map(|instance| instance.tint)
                            .collect();

                        let mut particle_scene =
                            ParticleScene::new(built.particle_index, built.particle_lists);
                        particle_scene
                            .set_budget_numbers(built.particle_ceiling, built.particle_skip_mask);
                        particle_scene.hold_attachments_from_birth();
                        if let (Some(index), Some(scene)) =
                            (built.object_index.as_ref(), object_scene.as_ref())
                        {
                            attach_particle_bones(
                                &mut particle_scene,
                                renderer,
                                vfs,
                                &resolver,
                                index,
                                scene,
                                &built.map.objects,
                                &built.proxies,
                            );
                        }

                        let world_bounds = built.terrain.bounds();
                        let wet = mint_sage_passability::water::WetGrid::build(
                            &built.height_map,
                            built.terrain.vertical_scale,
                            &built.map.water,
                            mint_sage_passability::water::WaterDepths::default(),
                        );
                        let time = mint_sage_scene::default_time_of_day(&global_lighting);
                        let rig = terrain_rig(&global_lighting, time);
                        let shadows = ShadowPass::new(renderer, &tuning);

                        println!(
        "{}: {} chunk(s), {terrain_len} terrain part(s), {} road(s), {} water bod(ies), \
         {} river ribbon(s), {} wave(s), {} object placement(s) ({} texture class(es), {} resolved), \
         {as_entities} placement(s) left to the entity path",
        map_label,
        built.terrain.chunks.len(),
        road_scene.handles.len(),
        water_scene.handles.len(),
        river_scene.handles.len(),
        wave_scene.handles.len(),
        object_scene.as_ref().map_or(0, |s| s.drawn),
        textures.len(),
        textures.iter().filter(|t| t.is_some()).count(),
    );

                        let tree_pose_drawn = vec![None; built.proxies.len()];
                        let standing_placement = vec![None; built.proxies.len()];
                        let placement_groups = object_scene
                            .as_ref()
                            .map(|scene| {
                                mint_sage_scene::placement_groups(scene, built.proxies.len())
                            })
                            .unwrap_or_default();
                        let removed_drawn = vec![false; built.proxies.len()];
                        Ok(SceneProgress::Done(Box::new(MatchScene {
                            terrain: built.terrain,
                            drawn,
                            composed_len,
                            palette: Vec::new(),
                            surface_overlays,
                            object_span,
                            tree_pose_drawn,
                            standing_placement,
                            placement_groups,
                            forest_revision_seen: None,
                            proxy_of_object,
                            removed_drawn,
                            forest_seeded: false,
                            forest_known: Vec::new(),
                            base_tints,
                            object_proxy_slots,
                            object_trees,
                            rig,
                            sun: mint_sage_scene::SunConvention::default(),
                            terrain_shading,
                            shadows,
                            world_bounds,
                            tuning,
                            tuning_file,
                            model_lights: mint_sage_scene::ModelLights::default(),
                            object_scene,
                            proxies: built.proxies,
                            object_index: built.object_index,
                            resolver,
                            water_span,
                            fx_span,
                            fx_water,
                            wave_tracks,
                            road_scene,
                            shroud_extent: [0.0, 0.0, 1.0, 1.0],
                            shroud_texture: None,
                            reflection: crate::reflection::ReflectionRung::default(),
                            border_in_mirror: crate::reflection::BorderInMirror::default(),
                            wave_rung: mint_sage_scene::WaveRung::default(),
                            wave_surface_tall,
                            no_swell: WaveSurface::default(),
                            object_reflects,
                            mirror_sky,
                            reflected: Vec::new(),
                            mirror_counts: crate::reflection::MirrorCounts::default(),
                            mirror_skip: true,
                            mirror_tally: crate::reflection::MirrorTally::default(),
                            water_scene,
                            wave_surface: built.wave_surface,
                            river_scene,
                            river_network: built.river_network,
                            wave_scene,
                            wave_field: built.wave_field,
                            particle_scene,
                            condition_particles: mint_sage_scene::ConditionParticles::new(),
                            map_path: map_label,
                            wet,
                            height_map: built.height_map,
                            water: built.map.water.clone(),
                            objects,
                            map_scripts,
                            map_script_lines: script_lines,
                            referenced_unit_names,
                            sides,
                            blend_tile: built.map.blend_tile.clone(),
                            particle_seconds: 0.0,
                            snowy,
                            snow,
                            breeze: BreezeRung::default(),
                            cloud_setting,
                            cloud_darkening: mint_sage_scene::CloudDarkening::new(),
                            cloud_darkening_on: true,
                            fog,
                            fog_on: true,
                            fog_shift: 0.0,
                            post,
                            script_bloom: None,
                            script_view: crate::rules::map_scripts::ScriptView::default(),
                            render_target: None,
                            script_camera: crate::script_camera::ScriptCamera::default(),
                            script_pose: crate::script_pose::ScriptPose::default(),
                            camera_freeze: crate::script_camera::FreezeGate::default(),
                            camera_animations,
                            camera_area: None,
                            camera_area_seen: None,
                            camera_keys: crate::camera_limits::MapCameraKeys::read(
                                &built.map.world_info.properties,
                            ),
                            terrain_len,
                        })))
                    })))
                })))
            })))
        })))
    }))
}

pub(crate) fn game_tuning_file() -> mint_sage_terrain::TuningFile {
    mint_sage_terrain::TuningFile::open(mint_sage_terrain::default_tuning_path("rotwk-tuning.txt"))
}

fn poll_tuning(scene: &mut MatchScene) {
    let Some(file) = scene.tuning_file.as_mut() else {
        return;
    };
    let Some(changes) = file.poll() else {
        return;
    };
    scene.tuning = *file.tuning();
    if changes.is_empty() {
        println!("tuning: re-read, nothing changed");
    } else {
        println!("tuning: {} value(s) changed", changes.len());
        for line in &changes {
            println!("  {line}");
        }
    }
}

pub(crate) fn advance_scene(scene: &mut MatchScene, renderer: &mut Renderer, seconds: f32) {
    poll_tuning(scene);
    let breeze = scene.breeze;
    if let Some(object_scene) = scene.object_scene.as_mut() {
        if let Some(slice) = scene.drawn.get_mut(scene.object_span.clone()) {
            mint_sage_scene::advance_object_animations(
                object_scene,
                renderer,
                slice,
                &scene.proxies,
                seconds,
            );
        }
        if let Some(slice) = scene.drawn.get_mut(scene.object_span.clone()) {
            mint_sage_scene::advance_object_uv_mappers(object_scene, slice, seconds);
        }
        let offsets = breeze.breeze().offsets(seconds);
        if let Some(slice) = scene.drawn.get_mut(scene.object_span.clone()) {
            mint_sage_scene::advance_object_sway(object_scene, slice, &scene.proxies, &offsets);
        }
        mint_sage_scene::follow_particle_bones(
            &mut scene.particle_scene,
            object_scene,
            &scene.proxies,
            seconds,
        );
    }

    scene.terrain_shading.push(renderer, &scene.tuning, seconds);

    let strength = shroud_strength(scene);
    let extent = scene.shroud_extent;
    for shader in [
        scene.road_scene.shader,
        scene.water_scene.shader,
        scene.river_scene.shader,
        scene.wave_scene.shader,
    ] {
        mint_sage_scene::push_shroud_scene(renderer, shader, extent, strength);
    }

    mint_sage_scene::push_water_uniforms(
        renderer,
        &scene.water_scene,
        scene.reflection.source(),
        &scene.tuning,
        if scene.wave_rung == mint_sage_scene::WaveRung::BothTall {
            &scene.wave_surface_tall
        } else if scene.wave_rung.swell() {
            &scene.wave_surface
        } else {
            &scene.no_swell
        },
        seconds,
    );
    mint_sage_scene::push_river_uniforms(
        renderer,
        &scene.river_scene,
        &scene.river_network,
        mint_sage_terrain::RiverLook::default(),
        &scene.tuning,
        seconds,
    );
    if let Some(sky) = scene.mirror_sky.as_mut() {
        sky.push(renderer, &scene.tuning, seconds);
    }
    mint_sage_scene::push_wave_uniforms(
        renderer,
        &scene.wave_scene,
        &scene.wave_field,
        WaveLook::default(),
        match scene.wave_rung {
            mint_sage_scene::WaveRung::Swell => None,
            rung => Some(rung.sprite(true)),
        },
        if scene.wave_rung.swell() {
            scene.wave_rung.swell_gain()
        } else {
            0.0
        },
        &scene.tuning,
        seconds,
    );

    let ground = mint_sage_scene::MapGround::new(&scene.height_map, scene.terrain.vertical_scale);
    let dt = particle_delta(seconds, scene.particle_seconds);
    let (whole, part) = split_steps(dt);
    scene.particle_scene.advance(whole, &ground);
    scene.particle_scene.advance(part, &ground);
    scene.particle_seconds = seconds;
}

pub(crate) fn topple_transform(
    placement: mint_render::Mat4,
    base: mint_render::Vec3,
    pose: &crate::forest::TreePose,
) -> mint_render::Mat4 {
    let (kx, ky) = (-pose.dir.y, pose.dir.x);
    let turn = if kx * kx + ky * ky > f32::EPSILON {
        let (s, c) = mint_math::f32::sin_cos(pose.angle);
        let t = 1.0 - c;
        mint_render::Mat4::from_cols([
            [c + t * kx * kx, t * kx * ky, -s * ky, 0.0],
            [t * kx * ky, c + t * ky * ky, s * kx, 0.0],
            [s * ky, -s * kx, c, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    } else {
        mint_render::Mat4::IDENTITY
    };
    let sunk = mint_render::Vec3::new(base.x, base.y, base.z - pose.depth);
    mint_render::Mat4::from_translation(sunk)
        .mul_mat4(turn)
        .mul_mat4(mint_render::Mat4::from_translation(mint_render::Vec3::new(
            -base.x, -base.y, -base.z,
        )))
        .mul_mat4(placement)
}

fn zero_scale(position: mint_render::Vec3) -> mint_render::Mat4 {
    mint_render::Mat4::from_translation(position)
        .mul_mat4(mint_render::Mat4::from_scale(mint_render::Vec3::splat(0.0)))
}

fn redraw_placement(scene: &mut MatchScene, at: usize) {
    let (Some(object_scene), Some(proxy), Some(groups)) = (
        scene.object_scene.as_ref(),
        scene.proxies.get(at),
        scene.placement_groups.get(at),
    ) else {
        return;
    };
    if let Some(slice) = scene.drawn.get_mut(scene.object_span.clone()) {
        for group in groups {
            mint_sage_scene::replace_placement(object_scene, slice, *group, proxy.placement);
        }
    }
}

fn proxy_at(scene: &MatchScene, source_index: u32) -> Option<usize> {
    let original = usize::try_from(source_index).ok()?;
    scene
        .proxy_of_object
        .get(original)
        .copied()
        .flatten()
        .and_then(|slot| usize::try_from(slot).ok())
}

pub(crate) fn apply_tree_poses(scene: &mut MatchScene, forest: &crate::forest::Forest) -> usize {
    if scene.forest_revision_seen == Some(forest.revision()) {
        return 0;
    }
    scene.forest_revision_seen = Some(forest.revision());
    if !scene.forest_seeded {
        scene.forest_known = forest.present_source_indices();
        scene.forest_seeded = true;
    }
    let mut changed = 0usize;
    for (source_index, pose) in forest.placed_poses() {
        let Some(at) = proxy_at(scene, source_index) else {
            continue;
        };
        if scene.removed_drawn.get(at).copied().unwrap_or(true)
            || scene.tree_pose_drawn.get(at).copied().flatten() == Some(pose)
        {
            continue;
        }
        let Some(proxy) = scene.proxies.get(at) else {
            continue;
        };
        let (position, placement) = (proxy.position, proxy.placement);
        let standing = match scene.standing_placement.get_mut(at) {
            Some(kept) => *kept.get_or_insert(placement),
            None => placement,
        };
        let drawn = if pose.gone {
            zero_scale(position)
        } else {
            topple_transform(standing, position, &pose)
        };
        if let Some(proxy) = scene.proxies.get_mut(at) {
            proxy.placement = drawn;
        }
        if let Some(object_scene) = scene.object_scene.as_mut() {
            if object_scene.still.len() != scene.proxies.len() {
                object_scene.still = vec![false; scene.proxies.len()];
            }
            if let Some(still) = object_scene.still.get_mut(at) {
                *still = pose.toppled;
            }
        }
        if let Some(slot) = scene.tree_pose_drawn.get_mut(at) {
            *slot = Some(pose);
        }
        redraw_placement(scene, at);
        changed = changed.saturating_add(1);
    }
    let present: std::collections::HashSet<u32> =
        forest.present_source_indices().into_iter().collect();
    let gone: Vec<u32> = scene
        .forest_known
        .iter()
        .copied()
        .filter(|source_index| !present.contains(source_index))
        .collect();
    for source_index in gone {
        let Some(at) = proxy_at(scene, source_index) else {
            continue;
        };
        if scene.removed_drawn.get(at).copied().unwrap_or(true) {
            continue;
        }
        let Some(proxy) = scene.proxies.get_mut(at) else {
            continue;
        };
        proxy.placement = zero_scale(proxy.position);
        if let Some(flag) = scene.removed_drawn.get_mut(at) {
            *flag = true;
        }
        redraw_placement(scene, at);
        changed = changed.saturating_add(1);
    }
    changed
}

pub(crate) const WATER_INI_PATH: &str = "data/ini/water.ini";

pub(crate) fn apply_fx_visibility(scene: &mut MatchScene, on: bool) {
    let per_surface = scene.fx_water.visibility();
    if let Some(slice) = scene.drawn.get_mut(scene.fx_span.clone()) {
        for (n, instance) in slice.iter_mut().enumerate() {
            instance.visible = on && per_surface.get(n).copied().unwrap_or(true);
        }
    }
}

fn split_steps(dt: f32) -> (f32, f32) {
    let step = 1.0 / f32::from(u16::try_from(mint_app::SIMULATION_HZ).unwrap_or(30));
    let whole = (dt / step).floor() * step;
    if !whole.is_finite() || whole < 0.0 {
        return (0.0, 0.0);
    }
    (whole, (dt - whole).max(0.0))
}

pub(crate) fn shroud_strength(scene: &MatchScene) -> f32 {
    if scene.terrain_shading.modulation.shroud_loaded {
        1.0
    } else {
        0.0
    }
}

pub(crate) fn drawn_visibility_mask(scene: &MatchScene) -> Option<mint_render::VisibilityMask> {
    scene
        .shroud_texture
        .map(|texture| mint_render::VisibilityMask {
            texture,
            transform: mint_sage_terrain::ShroudSceneBlock::new(scene.shroud_extent, 0.0).uv(),
            strength: shroud_strength(scene),
        })
}

pub(crate) fn drawn_fog(scene: &MatchScene) -> Option<mint_render::DistanceFog> {
    let shift = if scene.fog_shift.is_finite() {
        scene.fog_shift.max(0.0)
    } else {
        0.0
    };
    scene
        .fog
        .filter(|_| scene.fog_on)
        .map(|fog| mint_render::DistanceFog {
            start: fog.start + shift,
            end: fog.end + shift,
            ..fog
        })
}

pub(crate) fn fog_shift(distance: f32, pitch: f32, max_height: f32) -> f32 {
    let sine = mint_math::f32::sin(pitch);
    if !(sine > 1e-3 && max_height > 0.0 && distance.is_finite()) {
        return 0.0;
    }
    (distance - max_height / sine).max(0.0)
}

fn particle_delta(seconds: f32, last: f32) -> f32 {
    (seconds - last).max(0.0)
}

fn path_points_of(
    map: &MapFile,
    objects: &[mint_sage_map::Object],
) -> Vec<crate::rules::map_scripts::PathPoint> {
    let waypoints: Vec<mint_sage_map::Waypoint> = objects
        .iter()
        .filter_map(|object| mint_sage_map::Waypoint::from_object(object).ok().flatten())
        .collect();
    let links = map.waypoint_links().unwrap_or_default();
    let nodes = mint_sage_map::build_links(&waypoints, &links);
    waypoints
        .iter()
        .zip(nodes)
        .map(|(waypoint, node)| crate::rules::map_scripts::PathPoint {
            position: mint_movement::Vec2::new(waypoint.position[0], waypoint.position[1]),
            labels: waypoint.labels.clone(),
            links: node.links,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_clock_reading_advances_by_the_difference() {
        assert_eq!(particle_delta(1.5, 1.0), 0.5);
    }

    #[test]
    fn the_fog_range_moves_out_only_past_the_widest_retail_zoom() {
        let pitch = 37.5f32.to_radians();
        let widest = 300.0 / mint_math::f32::sin(pitch);
        assert_eq!(fog_shift(widest * 0.5, pitch, 300.0), 0.0);
        let shift = fog_shift(widest + 1000.0, pitch, 300.0);
        assert!((shift - 1000.0).abs() < 0.05, "{shift}");
    }

    #[test]
    fn a_clock_that_did_not_move_advances_by_nothing() {
        assert_eq!(particle_delta(2.0, 2.0), 0.0);
    }

    #[test]
    fn a_clock_reset_by_a_rematch_never_goes_backwards() {
        assert_eq!(particle_delta(0.0, 30.0), 0.0);
    }

    #[test]
    fn a_span_splits_into_whole_steps_and_the_fraction_left() {
        let (whole, part) = split_steps(3.0 + 0.5 / 30.0);
        assert!((whole - 3.0).abs() < 1e-4, "{whole}");
        assert!((part - 0.5 / 30.0).abs() < 1e-4, "{part}");
        assert_eq!(split_steps(0.0), (0.0, 0.0));
        assert_eq!(split_steps(f32::NAN), (0.0, 0.0));
        let (whole, part) = split_steps(0.01);
        assert_eq!(whole, 0.0);
        assert!((part - 0.01).abs() < 1e-6);
    }

    #[test]
    fn topple_transform_tips_the_trunk_toward_the_fall_about_its_base() {
        let base = mint_render::Vec3::new(10.0, 20.0, 5.0);
        let standing = mint_render::Mat4::from_translation(base);
        let pose = crate::forest::TreePose {
            toppled: true,
            dir: mint_movement::Vec2::new(0.6, 0.8),
            angle: std::f32::consts::FRAC_PI_2,
            depth: 2.0,
            gone: false,
        };
        let turned = topple_transform(standing, base, &pose);
        let foot = turned.transform_point(mint_render::Vec3::new(0.0, 0.0, 0.0));
        let top = turned.transform_point(mint_render::Vec3::new(0.0, 0.0, 10.0));
        assert!((foot.x - 10.0).abs() < 1e-4 && (foot.y - 20.0).abs() < 1e-4);
        assert!((foot.z - 3.0).abs() < 1e-4, "{foot:?}");
        assert!((top.x - 16.0).abs() < 1e-3, "{top:?}");
        assert!((top.y - 28.0).abs() < 1e-3, "{top:?}");
        assert!((top.z - 3.0).abs() < 1e-3, "{top:?}");
    }
}
