use super::object_compare::*;
use super::reflection::*;
use super::shadow::*;
use super::water_scene::*;
use super::*;

pub(crate) struct TerrainInput<'a> {
    pub(crate) environment: Option<&'a mint_sage_map::EnvironmentData>,
    pub(crate) no_cloud_factor: Option<[f32; 3]>,
    pub(crate) cloud: TerrainCloud,
    pub(crate) macro_layer: TerrainMacro,
    pub(crate) normal_maps: &'a [Option<mint_render::TextureData>],
    pub(crate) relief: TerrainRelief,
    pub(crate) sun: TerrainSun,
    pub(crate) overbright: f32,
    pub(crate) seconds: f32,
    pub(crate) sky: SkyDraw,
    pub(crate) shadows: TerrainShadows,
    pub(crate) shadow_fit: ShadowFitMode,
    pub(crate) shadow_alpha: ShadowAlpha,
    pub(crate) reflection_cull: ReflectionCull,
}

pub(crate) struct WaterInput<'a> {
    pub(crate) surface: &'a WaterSurface,
    pub(crate) mode: WaterDraw,
    pub(crate) reflection: WaterReflection,
    pub(crate) source: WaterReflectionSource,
    pub(crate) composite: WaterReflectionComposite,
    pub(crate) highlights: WaterHighlights,
    pub(crate) tint: WaterTint,
    pub(crate) depth: WaterDepthMode,
    pub(crate) distortion: WaterDistortion,
    pub(crate) transparency: WaterTransparency,
    pub(crate) vertical_scale: f32,
    pub(crate) seconds: f32,
    pub(crate) waves: &'a WaveField,
    pub(crate) wave_look: WaveLook,
    pub(crate) wave_surface: &'a WaveSurface,
    pub(crate) tuning: &'a WaterTuning,
}

pub(crate) struct ObjectInput<'a> {
    pub(crate) vfs: &'a Vfs,
    pub(crate) resolver: &'a Resolver,
    pub(crate) index: Option<&'a ObjectIndex>,
    pub(crate) objects: &'a [Object],
    pub(crate) proxies: &'a [ObjectProxy],
    pub(crate) draw: bool,
    pub(crate) markers: bool,
    pub(crate) animation: ObjectAnimation,
    pub(crate) material: ObjectMaterial,
    pub(crate) sway: SwayMode,
}

pub(crate) struct ParticleInput<'a> {
    pub(crate) index: &'a ParticleIndex,
    pub(crate) lists: &'a FxListIndex,
    pub(crate) height_map: &'a HeightMapData,
    pub(crate) name: &'a str,
    pub(crate) draw: bool,
    pub(crate) keys: bool,
    pub(crate) seconds: f32,
    pub(crate) bones: BoneRung,
    pub(crate) motion: bool,
    pub(crate) sort: SortMode,
    pub(crate) billboard: BillboardMode,
    pub(crate) max_particles: Option<usize>,
    pub(crate) skip_mask: u32,
    pub(crate) lod: DetailLevel,
    pub(crate) flipbook: FlipbookMode,
    pub(crate) shape: ShapeMode,
    pub(crate) compound: bool,
    pub(crate) wind: WindMode,
    pub(crate) fx: &'a str,
}

pub(crate) struct RoadInput<'a> {
    pub(crate) network: &'a RoadNetwork,
    pub(crate) draw: bool,
}

pub(crate) struct ModelGroup {
    pub(crate) model: Arc<CachedModel>,
    pub(crate) placements: Vec<usize>,
    pub(crate) span: std::ops::Range<usize>,
    pub(crate) animation: Option<ModelAnimation>,
    pub(crate) poses: Vec<PartPose>,
    pub(crate) sways: bool,
}

pub(crate) struct TemplateNote {
    pub(crate) model: Option<String>,
    pub(crate) parts: Option<usize>,
    pub(crate) why: &'static str,
}

#[derive(Default)]
pub(crate) struct ObjectScene {
    pub(crate) groups: Vec<ModelGroup>,
    pub(crate) notes: BTreeMap<String, TemplateNote>,
    pub(crate) resolved: usize,
    pub(crate) drawn: usize,
    pub(crate) silent: usize,
    pub(crate) road: usize,
    pub(crate) terrain_type: usize,
    pub(crate) undeclared: usize,
    pub(crate) animations: AnimationTally,
    pub(crate) uv_groups: usize,
    pub(crate) uv_placements: usize,
    pub(crate) sway_groups: usize,
    pub(crate) sway_placements: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AnimationTally {
    pub(crate) templates: usize,
    pub(crate) bound: usize,
    pub(crate) placements: usize,
    pub(crate) assets: usize,
    pub(crate) skinned_parts: usize,
    pub(crate) skinned_vertices: usize,
    pub(crate) rigid_parts: usize,
    pub(crate) random_start: usize,
    pub(crate) refused: Vec<String>,
}

pub(crate) fn build_object_scene(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    objects: &[Object],
) -> ObjectScene {
    let mut scene = ObjectScene {
        notes: template_notes(Some(index), objects),
        ..ObjectScene::default()
    };
    let mut wanted: BTreeMap<String, (String, Vec<usize>)> = BTreeMap::new();

    for (slot, object) in objects.iter().enumerate() {
        if object.flags & (mint_sage_terrain::ROAD_START_FLAG | mint_sage_terrain::ROAD_END_FLAG)
            != 0
        {
            scene.road = scene.road.saturating_add(1);
            continue;
        }
        match index.lookup(&object.template_name) {
            Template::Model(model) => {
                scene.resolved = scene.resolved.saturating_add(1);
                wanted
                    .entry(model.to_ascii_lowercase())
                    .or_insert_with(|| (model.to_owned(), Vec::new()))
                    .1
                    .push(slot);
            }
            Template::NoModel(_) => scene.silent = scene.silent.saturating_add(1),
            Template::Road => scene.road = scene.road.saturating_add(1),
            Template::Terrain => scene.terrain_type = scene.terrain_type.saturating_add(1),
            Template::Undeclared => scene.undeclared = scene.undeclared.saturating_add(1),
        }
    }

    let mut cache = ModelCache::new();
    let started = std::time::Instant::now();
    for (name, placements) in wanted.into_values() {
        let Some(model) = cache.get_or_load(renderer, vfs, resolver, &name) else {
            continue;
        };
        scene.drawn = scene.drawn.saturating_add(placements.len());
        let parts = model.parts.len();
        for note in scene.notes.values_mut() {
            if note
                .model
                .as_deref()
                .is_some_and(|wanted| wanted.eq_ignore_ascii_case(&name))
            {
                note.parts = Some(parts);
            }
        }
        let sways = placements.iter().any(|slot| {
            objects
                .get(*slot)
                .is_some_and(|object| index.sways(&object.template_name))
        });
        scene.groups.push(ModelGroup {
            model,
            placements,
            span: 0..0,
            animation: None,
            poses: Vec::new(),
            sways,
        });
    }
    let (uv_groups, uv_placements) = scene
        .groups
        .iter()
        .filter(|group| group.model.has_uv_mappers())
        .fold((0usize, 0usize), |(groups, placements), group| {
            (
                groups.saturating_add(1),
                placements.saturating_add(group.placements.len()),
            )
        });
    scene.uv_groups = uv_groups;
    scene.uv_placements = uv_placements;
    let (sway_groups, sway_placements) = scene.groups.iter().filter(|group| group.sways).fold(
        (0usize, 0usize),
        |(groups, placements), group| {
            (
                groups.saturating_add(1),
                placements.saturating_add(group.placements.len()),
            )
        },
    );
    scene.sway_groups = sway_groups;
    scene.sway_placements = sway_placements;
    report_object_models(&scene, &cache, started.elapsed());
    println!(
        "object materials: {uv_groups} model(s) over {uv_placements} placement(s) scroll          their own texture coordinates (8, /, --object-uv)"
    );
    println!(
        "vegetation: {sway_groups} model(s) over {sway_placements} placement(s) are drawn by a \
         W3DTreeDraw and sway (`,`, --sway)"
    );
    bind_object_animations(&mut scene, vfs, resolver, index, objects);
    scene
}

pub(crate) fn bind_object_animations(
    scene: &mut ObjectScene,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    objects: &[Object],
) {
    let started = std::time::Instant::now();
    let mut loaded: BTreeMap<String, Option<LoadedAnimation>> = BTreeMap::new();
    let mut templates: BTreeSet<String> = BTreeSet::new();

    for group in &mut scene.groups {
        let declared = group.placements.iter().find_map(|slot| {
            let object = objects.get(*slot)?;
            let animation = index.animation(&object.template_name)?;
            Some((object.template_name.clone(), animation.clone()))
        });
        let Some((template, declared)) = declared else {
            continue;
        };
        templates.insert(template.to_ascii_lowercase());
        let asset = declared.asset().to_ascii_lowercase();
        let entry = loaded.entry(asset).or_insert_with(|| {
            match load_animation(vfs, resolver, declared.asset()) {
                Ok(found) => Some(found),
                Err(error) => {
                    scene
                        .animations
                        .refused
                        .push(format!("{template}: {error}"));
                    None
                }
            }
        });
        let Some(found) = entry else {
            continue;
        };
        match ModelAnimation::bind(&group.model, &declared, found.clone()) {
            Ok(animation) => {
                scene.animations.bound = scene.animations.bound.saturating_add(1);
                scene.animations.placements = scene
                    .animations
                    .placements
                    .saturating_add(group.placements.len());
                scene.animations.skinned_parts = scene
                    .animations
                    .skinned_parts
                    .saturating_add(animation.skinned.len());
                scene.animations.rigid_parts = scene
                    .animations
                    .rigid_parts
                    .saturating_add(animation.rigid.len());
                for index in &animation.skinned {
                    let count = group
                        .model
                        .parts
                        .get(*index)
                        .and_then(|part| part.skin.as_ref())
                        .map_or(0, |skin| skin.bone_space.vertices.len());
                    scene.animations.skinned_vertices =
                        scene.animations.skinned_vertices.saturating_add(count);
                }
                if animation.flags.random_start {
                    scene.animations.random_start = scene.animations.random_start.saturating_add(1);
                }
                group
                    .poses
                    .resize(group.model.parts.len(), PartPose::default());
                group.animation = Some(animation);
            }
            Err(refusal) => scene
                .animations
                .refused
                .push(format!("{template} -> {}: {refusal}", declared.asset())),
        }
    }
    scene.animations.templates = templates.len();
    scene.animations.assets = loaded.values().filter(|entry| entry.is_some()).count();
    report_object_animations(&scene.animations, started.elapsed());
}

pub(crate) fn report_object_animations(tally: &AnimationTally, elapsed: std::time::Duration) {
    println!();
    println!(
        "object animations: {} template name(s) declare one; {} model(s) bound it over {}          placement(s), from {} distinct asset(s), in {:.1} ms",
        tally.templates,
        tally.bound,
        tally.placements,
        tally.assets,
        elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "  {} skinned part(s) ({} vertices) re-posed and {} rigid part(s) re-transformed per          frame - one pose per model, so every placement of a model is at the same frame",
        tally.skinned_parts, tally.skinned_vertices, tally.rigid_parts
    );
    if tally.random_start > 0 {
        println!(
            "  {} of them ask for RANDOMSTART, which needs a mesh per placement and is not done",
            tally.random_start
        );
    }
    for line in &tally.refused {
        println!("  NOT ANIMATED {line}");
    }
}

pub(crate) fn note_for(template: Template<'_>) -> TemplateNote {
    let (model, why) = match template {
        Template::Model(name) => (Some(name.to_owned()), ""),
        Template::NoModel(NoModelReason::ModelIsNone) => (None, "the model is NONE"),
        Template::NoModel(NoModelReason::NonDrawingModule) => (None, "no drawing Draw module"),
        Template::NoModel(NoModelReason::NoDrawModule) => (None, "no Draw module"),
        Template::Road => (None, "declared as a road, not an object"),
        Template::Terrain => (None, "declared as a terrain type, not an object"),
        Template::Undeclared => (None, "not declared in any INI document read"),
    };
    TemplateNote {
        model,
        parts: None,
        why,
    }
}

pub(crate) fn template_notes(
    index: Option<&ObjectIndex>,
    objects: &[Object],
) -> BTreeMap<String, TemplateNote> {
    let mut notes = BTreeMap::new();
    let Some(index) = index else {
        return notes;
    };
    for object in objects {
        let key = object.template_name.to_ascii_lowercase();
        if notes.contains_key(&key) {
            continue;
        }
        notes.insert(key, note_for(index.lookup(&object.template_name)));
    }
    notes
}

pub(crate) fn report_object_models(
    scene: &ObjectScene,
    cache: &ModelCache,
    elapsed: std::time::Duration,
) {
    let stats = cache.stats();
    let (vertices, triangles) = scene.groups.iter().fold((0usize, 0usize), |acc, group| {
        (
            acc.0.saturating_add(group.model.vertices),
            acc.1.saturating_add(group.model.triangles),
        )
    });
    let parts: usize = scene
        .groups
        .iter()
        .map(|group| {
            group
                .model
                .parts
                .len()
                .saturating_mul(group.placements.len())
        })
        .sum();
    println!();
    println!(
        "object models: {} placement(s) name a model, {} of them drawn; {} draw nothing on \
         purpose, {} road, {} terrain type, {} undeclared",
        scene.resolved, scene.drawn, scene.silent, scene.road, scene.terrain_type, scene.undeclared
    );
    println!(
        "  {} distinct model(s) asked for, {} loaded, {} failed; {} mesh(es) and {} texture(s) \
         uploaded, {vertices} vertices, {triangles} triangles, in {:.1} ms",
        cache.len(),
        stats.loaded,
        stats.failed,
        stats.meshes,
        stats.textures,
        elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "  {} picking prox(ies) and {} HIDDEN mesh(es) skipped; {parts} part instance(s) over \
         every placement",
        stats.skipped_pick, stats.skipped_hidden
    );
    for (name, reason) in cache.failures() {
        println!("  FAILED {name}: {reason}");
    }
}

pub(crate) fn object_world_bounds(scene: &ObjectScene, proxies: &[ObjectProxy]) -> Aabb {
    let mut out = Aabb::EMPTY;
    for group in &scene.groups {
        let model = group.model.bounds;
        if model.is_empty() {
            continue;
        }
        let (min, max) = (model.min(), model.max());
        for slot in &group.placements {
            let Some(proxy) = proxies.get(*slot) else {
                continue;
            };
            for index in 0..8u8 {
                let pick = |bit: u8, low: f32, high: f32| if index & bit == 0 { low } else { high };
                out.insert(proxy.placement.transform_point(Vec3::new(
                    pick(1, min.x, max.x),
                    pick(2, min.y, max.y),
                    pick(4, min.z, max.z),
                )));
            }
        }
    }
    out
}

pub(crate) fn extend_far_reach(camera: &mut OrbitCamera, bounds: &Aabb) {
    if bounds.is_empty() {
        return;
    }
    let centre = bounds.center();
    let target = camera.target;
    let offset = Vec3::new(
        centre.x - target.x,
        centre.y - target.y,
        centre.z - target.z,
    );
    let reach = offset.length() + bounds.radius();
    if reach.is_finite() && reach > camera.far_reach.unwrap_or(0.0) {
        camera.far_reach = Some(reach);
    }
}

pub(crate) fn model_instances(scene: &mut ObjectScene, proxies: &[ObjectProxy]) -> Vec<Instance> {
    let mut out = Vec::new();
    let mut placements: Vec<mint_render::Mat4> = Vec::new();
    for group in &mut scene.groups {
        placements.clear();
        placements.extend(
            group
                .placements
                .iter()
                .filter_map(|slot| proxies.get(*slot))
                .map(|proxy| proxy.placement),
        );
        let start = out.len();
        group
            .model
            .place_many_posed(&placements, &group.poses, &mut out);
        group.span = start..out.len();
    }
    out
}

pub(crate) fn advance_object_animations(
    scene: &mut ObjectScene,
    renderer: &mut Renderer,
    instances: &mut [Instance],
    proxies: &[ObjectProxy],
    completion: Completion,
    seconds: f32,
) -> Option<std::ops::Range<usize>> {
    let mut touched: Option<std::ops::Range<usize>> = None;
    let mut placements: Vec<mint_render::Mat4> = Vec::new();
    let mut posed: Vec<Instance> = Vec::new();
    for group in &mut scene.groups {
        let Some(animation) = &group.animation else {
            continue;
        };
        if !animation.moves() {
            continue;
        }
        let frame = animation.frame_at(completion, seconds);
        animation.apply(renderer, &group.model, frame, &mut group.poses);

        placements.clear();
        placements.extend(
            group
                .placements
                .iter()
                .filter_map(|slot| proxies.get(*slot))
                .map(|proxy| proxy.placement),
        );
        posed.clear();
        group
            .model
            .place_many_posed(&placements, &group.poses, &mut posed);
        if posed.len() != group.span.len() {
            continue;
        }
        let Some(slice) = instances.get_mut(group.span.clone()) else {
            continue;
        };
        slice.clone_from_slice(&posed);
        touched = Some(match touched {
            Some(span) => span.start.min(group.span.start)..span.end.max(group.span.end),
            None => group.span.clone(),
        });
    }
    touched
}

pub(crate) fn advance_object_uv_mappers(
    scene: &ObjectScene,
    instances: &mut [Instance],
    seconds: f32,
) -> Option<std::ops::Range<usize>> {
    let mut touched: Option<std::ops::Range<usize>> = None;
    for group in &scene.groups {
        if !group.model.has_uv_mappers() {
            continue;
        }
        let Some(slice) = instances.get_mut(group.span.clone()) else {
            continue;
        };
        let parts = group.model.parts.len();
        if parts == 0 {
            continue;
        }
        let placements = slice.len() / parts;
        if !group.model.apply_uv_mappers(placements, slice, seconds) {
            continue;
        }
        touched = Some(match touched {
            Some(span) => span.start.min(group.span.start)..span.end.max(group.span.end),
            None => group.span.clone(),
        });
    }
    touched
}

pub(crate) fn advance_object_sway(
    scene: &ObjectScene,
    instances: &mut [Instance],
    proxies: &[ObjectProxy],
    offsets: &[[f32; 3]; mint_sage_asset::SWAY_OFFSET_COUNT],
) -> Option<std::ops::Range<usize>> {
    let mut touched: Option<std::ops::Range<usize>> = None;
    let mut shears: Vec<[f32; 4]> = Vec::new();
    for group in &scene.groups {
        if !group.sways {
            continue;
        }
        let parts = group.model.parts.len();
        if parts == 0 {
            continue;
        }
        let Some(slice) = instances.get_mut(group.span.clone()) else {
            continue;
        };
        shears.clear();
        shears.extend(
            group
                .placements
                .iter()
                .filter_map(|slot| proxies.get(*slot))
                .map(|proxy| {
                    let [x, y, z, _] = proxy.placement.to_cols()[3];
                    let index = mint_sage_asset::sway_index(x, y);
                    let offset = offsets.get(index).copied().unwrap_or([0.0; 3]);
                    [offset[0], offset[1], offset[2], z]
                }),
        );
        let placements = shears.len();
        if placements == 0 {
            continue;
        }
        for (part, shear) in (0..parts).flat_map(|part| {
            shears
                .iter()
                .enumerate()
                .map(move |(i, shear)| (part.saturating_mul(placements).saturating_add(i), shear))
        }) {
            if let Some(instance) = slice.get_mut(part) {
                instance.height_shear = *shear;
            }
        }
        touched = Some(match touched {
            Some(span) => span.start.min(group.span.start)..span.end.max(group.span.end),
            None => group.span.clone(),
        });
    }
    touched
}

pub(crate) fn attach_particle_bones(
    particles: &mut ParticleScene,
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    scene: &ObjectScene,
    objects: &[Object],
    proxies: &[ObjectProxy],
) -> BoneReport {
    let mut report = BoneReport::default();
    let rung = particles.bones();
    if rung == BoneRung::Off {
        return report;
    }
    for (group_slot, group) in scene.groups.iter().enumerate() {
        for slot in &group.placements {
            let (Some(object), Some(proxy)) = (objects.get(*slot), proxies.get(*slot)) else {
                continue;
            };
            let Some(bones) = index.particle_bones(&object.template_name) else {
                continue;
            };
            let wanted: Vec<&mint_sage_ini::ParticleSysBone> = match rung {
                BoneRung::Off => Vec::new(),
                BoneRung::Default => bones.standing().collect(),
                BoneRung::All => bones.all().collect(),
            };
            if wanted.is_empty() {
                continue;
            }
            report.placements = report.placements.saturating_add(1);
            report.placement_at(
                wanted.len(),
                proxy.placement.transform_point(Vec3::new(0.0, 0.0, 0.0)),
            );
            for attachment in wanted {
                report.wanted = report.wanted.saturating_add(1);
                let pivot = match &attachment.bone {
                    None => {
                        report.bone_none = report.bone_none.saturating_add(1);
                        None
                    }
                    Some(bone) => match group.model.pivot_named(bone) {
                        Some(pivot) => Some(pivot),
                        None => {
                            report.bone_missing = report.bone_missing.saturating_add(1);
                            None
                        }
                    },
                };
                let local = pivot
                    .and_then(|pivot| group.model.bind_bone(pivot))
                    .unwrap_or(mint_render::Mat4::IDENTITY);
                let frame = proxy.placement.mul_mat4(bone_yaw_frame(local));
                let refused_before = particles.lod_refused();
                let attached = particles.attach(renderer, vfs, resolver, &attachment.system, frame);
                let Some(system) = attached else {
                    if particles.lod_refused() > refused_before {
                        report.lod_refused = report.lod_refused.saturating_add(1);
                    } else {
                        report.dangling = report.dangling.saturating_add(1);
                    }
                    continue;
                };
                report.spawned = report.spawned.saturating_add(1);
                report.saw(frame.transform_point(Vec3::new(0.0, 0.0, 0.0)));
                *report
                    .by_system
                    .entry(attachment.system.clone())
                    .or_default() += 1;
                if attachment.follow_bone {
                    report.following = report.following.saturating_add(1);
                }
                particles.follow(FollowingBone {
                    system,
                    group: group_slot,
                    placement: *slot,
                    pivot,
                    follow_bone: attachment.follow_bone,
                });
            }
        }
    }
    report
}

pub(crate) fn report_particle_bones(scene: &ParticleScene, report: &BoneReport, rung: BoneRung) {
    println!(
        "particle bones = {} (x, --particle-bones): {} system(s) on {} placement(s)",
        rung.label(),
        report.spawned,
        report.placements
    );
    if rung == BoneRung::Off {
        return;
    }
    println!(
        "  by draw shape: {} (flipbook {}, compound {})",
        scene.shape_line(),
        scene.flipbook().label(),
        if scene.compound() { "on" } else { "off" }
    );
    println!(
        "  {} attachment(s) selected, {} dangling (no such system), {} naming a bone the model \
         does not carry, {} refused by the detail level",
        report.wanted, report.dangling, report.bone_missing, report.lod_refused
    );
    println!(
        "  {} write the bone NONE and sit at the object's origin, {} write FollowBone",
        report.bone_none, report.following
    );
    let mut by_system: Vec<(&String, &usize)> = report.by_system.iter().collect();
    by_system.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    println!(
        "  {} distinct system(s), most-placed first:",
        by_system.len()
    );
    for (name, count) in by_system.iter().take(12) {
        println!("    {count:>4}  {name}");
    }
    if let Some((count, at)) = report.busiest {
        println!(
            "  the busiest placement starts {count} of them - fly to it with \
             --focus {:.0},{:.0}",
            at[0] / mint_sage_map::MAP_XY_FACTOR,
            at[1] / mint_sage_map::MAP_XY_FACTOR
        );
    }
    match report.bounds {
        None => println!("  nothing was placed"),
        Some((low, high)) => println!(
            "  placed inside ({:.0}, {:.0}, {:.0}) .. ({:.0}, {:.0}, {:.0})",
            low[0], low[1], low[2], high[0], high[1], high[2]
        ),
    }
}

fn bone_yaw_frame(local: mint_render::Mat4) -> mint_render::Mat4 {
    let origin = local.transform_point(Vec3::new(0.0, 0.0, 0.0));
    let forward = local.transform_vector(Vec3::new(1.0, 0.0, 0.0));
    let length = (forward.x * forward.x + forward.y * forward.y).sqrt();
    let (x, y) = if length > f32::EPSILON {
        (forward.x / length, forward.y / length)
    } else {
        (1.0, 0.0)
    };
    mint_render::Mat4::from_cols([
        [x, y, 0.0, 0.0],
        [-y, x, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [origin.x, origin.y, origin.z, 1.0],
    ])
}

pub(crate) fn follow_particle_bones(
    particles: &mut ParticleScene,
    scene: &ObjectScene,
    proxies: &[ObjectProxy],
    completion: Completion,
    seconds: f32,
) {
    if particles.following().is_empty() {
        return;
    }
    let mut bones: Vec<mint_render::Mat4> = Vec::new();
    let mut moved: Vec<(mint_sage_particle::SystemId, mint_render::Mat4)> = Vec::new();
    let mut current = usize::MAX;
    for following in particles.following() {
        let (Some(group), Some(proxy)) = (
            scene.groups.get(following.group),
            proxies.get(following.placement),
        ) else {
            continue;
        };
        let local = match (&group.animation, following.pivot) {
            (Some(animation), Some(pivot)) if following.follow_bone => {
                if following.group != current {
                    animation.bones(animation.frame_at(completion, seconds), &mut bones);
                    current = following.group;
                }
                bones.get(pivot).copied()
            }
            _ => following
                .pivot
                .and_then(|pivot| group.model.bind_bone(pivot)),
        }
        .unwrap_or(mint_render::Mat4::IDENTITY);
        moved.push((
            following.system,
            proxy.placement.mul_mat4(bone_yaw_frame(local)),
        ));
    }
    for (system, frame) in moved {
        particles.move_frame(system, frame);
    }
}
