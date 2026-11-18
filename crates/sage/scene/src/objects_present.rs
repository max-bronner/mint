use std::collections::BTreeMap;

use mint_render::{Instance, Renderer};
use mint_sage_map::Object;
use mint_sage_model::{
    load_animation, CachedModel, Completion, LoadedAnimation, ModelAnimation, ModelCache,
    NoModelReason, ObjectIndex, PartPose, Resolver, Template,
};
use mint_sage_terrain::{ObjectProxy, ROAD_END_FLAG, ROAD_START_FLAG};
use mint_vfs::Vfs;

use std::sync::Arc;

#[must_use]
pub fn rotation_anchors(index: Option<&ObjectIndex>, objects: &[Object]) -> Vec<[f32; 2]> {
    objects
        .iter()
        .map(|object| {
            index
                .and_then(|index| index.rotation_anchor(&object.template_name))
                .unwrap_or([0.0, 0.0])
        })
        .collect()
}

pub struct ModelGroup {
    pub model: Arc<CachedModel>,
    pub placements: Vec<usize>,
    pub span: std::ops::Range<usize>,
    pub animation: Option<ModelAnimation>,
    pub poses: Vec<PartPose>,
    pub sways: bool,
    pub tainted: Vec<bool>,
    pub skins: Vec<Option<crate::units_present::Skin>>,
    pub beside: bool,
}

pub struct TemplateNote {
    pub model: Option<String>,
    pub why: &'static str,
}

pub struct ObjectScene {
    pub groups: Vec<ModelGroup>,
    pub notes: BTreeMap<String, TemplateNote>,
    pub resolved: usize,
    pub drawn: usize,
    pub tainted_still: bool,
    pub still: Vec<bool>,
}

impl Default for ObjectScene {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            notes: BTreeMap::new(),
            resolved: 0,
            drawn: 0,
            tainted_still: true,
            still: Vec::new(),
        }
    }
}

fn note_for(template: Template<'_>) -> TemplateNote {
    let (model, why) = match template {
        Template::Model(name) => (Some(name.to_owned()), ""),
        Template::NoModel(NoModelReason::ModelIsNone) => (None, "the model is NONE"),
        Template::NoModel(NoModelReason::NonDrawingModule) => (None, "no drawing Draw module"),
        Template::NoModel(NoModelReason::NoDrawModule) => (None, "no Draw module"),
        Template::Road => (None, "declared as a road, not an object"),
        Template::Terrain => (None, "declared as a terrain type, not an object"),
        Template::Undeclared => (None, "not declared in any INI document read"),
    };
    TemplateNote { model, why }
}

#[must_use]
pub fn template_notes(
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

pub fn build_object_scene(
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
    let mut beside: BTreeMap<String, (String, Vec<usize>, bool)> = BTreeMap::new();
    let mut attached = 0usize;
    let mut road = 0usize;
    let mut terrain_type = 0usize;
    let mut undeclared = 0usize;
    let mut silent = 0usize;

    for (slot, object) in objects.iter().enumerate() {
        if object.flags & (ROAD_START_FLAG | ROAD_END_FLAG) != 0 {
            road = road.saturating_add(1);
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
                for module in index.model_modules(&object.template_name) {
                    let Some(model) = module.model else {
                        continue;
                    };
                    if module.attach_to_bone.is_some() {
                        attached = attached.saturating_add(1);
                        continue;
                    }
                    beside
                        .entry(model.to_ascii_lowercase())
                        .or_insert_with(|| (model.to_owned(), Vec::new(), module.is_floor()))
                        .1
                        .push(slot);
                }
            }
            Template::NoModel(_) => silent = silent.saturating_add(1),
            Template::Road => road = road.saturating_add(1),
            Template::Terrain => terrain_type = terrain_type.saturating_add(1),
            Template::Undeclared => undeclared = undeclared.saturating_add(1),
        }
    }

    let mut cache = ModelCache::new();
    let (mut floors_drawn, mut modules_drawn) = (0usize, 0usize);
    for (name, placements, floor) in beside.into_values() {
        let Some(model) = cache.get_or_load(renderer, vfs, resolver, &name) else {
            continue;
        };
        let drawn = if floor {
            &mut floors_drawn
        } else {
            &mut modules_drawn
        };
        *drawn = drawn.saturating_add(placements.len());
        scene.groups.push(ModelGroup {
            model,
            tainted: vec![false; placements.len()],
            placements,
            span: 0..0,
            animation: None,
            poses: Vec::new(),
            sways: false,
            skins: Vec::new(),
            beside: true,
        });
    }
    for (name, placements) in wanted.into_values() {
        let Some(model) = cache.get_or_load(renderer, vfs, resolver, &name) else {
            continue;
        };
        scene.drawn = scene.drawn.saturating_add(placements.len());
        let sways = placements.iter().any(|slot| {
            objects
                .get(*slot)
                .is_some_and(|object| index.sways(&object.template_name))
        });
        let tainted = placements
            .iter()
            .map(|slot| {
                objects
                    .get(*slot)
                    .is_some_and(|object| index.tainted_tree(&object.template_name))
            })
            .collect();
        scene.groups.push(ModelGroup {
            model,
            placements,
            span: 0..0,
            animation: None,
            poses: Vec::new(),
            sways,
            tainted,
            skins: Vec::new(),
            beside: false,
        });
    }
    let stats = cache.stats();
    println!(
        "objects: {} placement(s) name a model, {} drawn, {silent} draw nothing, {road} road, \
         {terrain_type} terrain type, {undeclared} undeclared; {floors_drawn} floor module(s) \
         and {modules_drawn} other model module(s) drawn beside a body, {attached} attached to \
         another module's bone not drawn; {} model(s) loaded, {} failed",
        scene.resolved, scene.drawn, stats.loaded, stats.failed,
    );
    for (name, reason) in cache.failures() {
        println!("  FAILED {name}: {reason}");
    }

    bind_object_animations(&mut scene, vfs, resolver, index, objects);
    scene
}

fn bind_object_animations(
    scene: &mut ObjectScene,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    objects: &[Object],
) {
    let mut loaded: BTreeMap<String, Option<LoadedAnimation>> = BTreeMap::new();
    let mut bound = 0usize;
    for group in scene.groups.iter_mut().filter(|group| !group.beside) {
        let declared = group.placements.iter().find_map(|slot| {
            let object = objects.get(*slot)?;
            let animation = index.animation(&object.template_name)?;
            Some((object.template_name.clone(), animation.clone()))
        });
        let Some((template, declared)) = declared else {
            continue;
        };
        let asset = declared.asset().to_ascii_lowercase();
        let entry = loaded.entry(asset).or_insert_with(|| {
            match load_animation(vfs, resolver, declared.asset()) {
                Ok(found) => Some(found),
                Err(error) => {
                    println!("  NOT ANIMATED {template}: {error}");
                    None
                }
            }
        });
        let Some(found) = entry else {
            continue;
        };
        match ModelAnimation::bind(&group.model, &declared, found.clone()) {
            Ok(animation) => {
                bound = bound.saturating_add(1);
                group
                    .poses
                    .resize(group.model.parts.len(), PartPose::default());
                group.animation = Some(animation);
            }
            Err(refusal) => println!(
                "  NOT ANIMATED {template} -> {}: {refusal}",
                declared.asset()
            ),
        }
    }
    println!("  {bound} model(s) bound a default animation");
}

#[allow(clippy::too_many_arguments)]
pub fn apply_object_conditions(
    scene: &mut ObjectScene,
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    objects: &[Object],
    live: crate::animation_state::Conditions,
) -> usize {
    let mut cache = ModelCache::new();
    let mut machines: BTreeMap<String, crate::animation_state::ModelStateMachine> = BTreeMap::new();
    let mut built: BTreeMap<(String, usize), Option<crate::units_present::Skin>> = BTreeMap::new();
    let (mut worn, mut swaps) = (0usize, 0usize);
    let weather = if live.intersects(crate::animation_state::Conditions::SNOW) {
        mint_sage_ini::NuggetWeather::Snowy
    } else {
        mint_sage_ini::NuggetWeather::Normal
    };
    let mut weathered: BTreeMap<(String, String), Option<crate::units_present::Skin>> =
        BTreeMap::new();
    for group in &mut scene.groups {
        group.skins = vec![None; group.placements.len()];
        for (slot, skin) in group.placements.iter().zip(group.skins.iter_mut()) {
            let Some(object) = objects.get(*slot) else {
                continue;
            };
            if group.beside {
                let texture = index
                    .floor_modules(&object.template_name)
                    .into_iter()
                    .filter(|floor| {
                        floor
                            .model
                            .is_some_and(|model| model.eq_ignore_ascii_case(&group.model.name))
                    })
                    .find_map(|floor| {
                        floor
                            .weather_textures
                            .iter()
                            .find(|(of, _)| *of == weather)
                            .map(|(_, texture)| texture.clone())
                    });
                if let Some(texture) = texture {
                    *skin = weathered
                        .entry((group.model.name.to_ascii_lowercase(), texture.clone()))
                        .or_insert_with(|| {
                            crate::units_present::weather_skin(
                                &mut cache,
                                renderer,
                                vfs,
                                resolver,
                                &group.model,
                                &texture,
                            )
                        })
                        .clone();
                    if skin.is_some() {
                        worn = worn.saturating_add(1);
                    }
                }
                continue;
            }
            let key = object.template_name.to_ascii_lowercase();
            let states = index.model_states(&object.template_name);
            let machine = machines
                .entry(key.clone())
                .or_insert_with(|| crate::animation_state::ModelStateMachine::compile(states));
            let Some(selected) = machine.select(live) else {
                continue;
            };
            let Some(state) = states.get(selected) else {
                continue;
            };
            if state.textures.is_empty() {
                continue;
            }
            if !state
                .model
                .as_deref()
                .is_some_and(|model| model.eq_ignore_ascii_case(&group.model.name))
            {
                swaps = swaps.saturating_add(1);
                continue;
            }
            let found = built
                .entry((key, selected))
                .or_insert_with(|| {
                    let skin =
                        cache.substitute(renderer, vfs, resolver, &group.model, &state.textures);
                    skin.iter().any(Option::is_some).then(|| skin.into())
                })
                .clone();
            if found.is_some() {
                worn = worn.saturating_add(1);
            }
            *skin = found;
        }
    }
    println!(
        "objects: conditions {} - {worn} placement(s) wear a texture substitution ({} distinct), \
         {swaps} would swap model (not drawn)",
        live.label(),
        built.values().filter(|skin| skin.is_some()).count()
    );
    worn
}

fn apply_skins(group: &ModelGroup, proxies: &[ObjectProxy], slice: &mut [Instance]) {
    if group.skins.iter().all(Option::is_none) {
        return;
    }
    let placed: Vec<Option<&crate::units_present::Skin>> = group
        .placements
        .iter()
        .enumerate()
        .filter(|(_, slot)| proxies.get(**slot).is_some())
        .map(|(at, _)| group.skins.get(at).and_then(Option::as_ref))
        .collect();
    if placed.is_empty() {
        return;
    }
    for (part, run) in slice.chunks_mut(placed.len()).enumerate() {
        for (instance, skin) in run.iter_mut().zip(&placed) {
            if let Some(texture) = skin.and_then(|skin| skin.get(part).copied().flatten()) {
                instance.texture = Some(texture);
            }
        }
    }
}

pub fn model_instances(scene: &mut ObjectScene, proxies: &[ObjectProxy]) -> Vec<Instance> {
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
        if let Some(slice) = out.get_mut(group.span.clone()) {
            apply_skins(group, proxies, slice);
        }
    }
    out
}

#[must_use]
pub fn placement_groups(scene: &ObjectScene, slots: usize) -> Vec<Vec<(usize, usize)>> {
    let mut found = vec![Vec::new(); slots];
    for (group, model) in scene.groups.iter().enumerate() {
        for (at, slot) in model.placements.iter().enumerate() {
            if let Some(entry) = found.get_mut(*slot) {
                entry.push((group, at));
            }
        }
    }
    found
}

pub fn replace_placement(
    scene: &ObjectScene,
    instances: &mut [Instance],
    (group, at): (usize, usize),
    placement: mint_render::Mat4,
) {
    let Some(model) = scene.groups.get(group) else {
        return;
    };
    let count = model.placements.len();
    let mut placed = Vec::with_capacity(model.model.parts.len());
    model
        .model
        .place_many_posed(&[placement], &model.poses, &mut placed);
    for (part, instance) in placed.into_iter().enumerate() {
        let index = model
            .span
            .start
            .saturating_add(part.saturating_mul(count))
            .saturating_add(at);
        if index >= model.span.end {
            break;
        }
        if let Some(slot) = instances.get_mut(index) {
            slot.transform = instance.transform;
        }
    }
}

pub fn advance_object_animations(
    scene: &mut ObjectScene,
    renderer: &mut Renderer,
    instances: &mut [Instance],
    proxies: &[ObjectProxy],
    seconds: f32,
) {
    let mut placements: Vec<mint_render::Mat4> = Vec::new();
    let mut posed: Vec<Instance> = Vec::new();
    for group in &mut scene.groups {
        let Some(animation) = &group.animation else {
            continue;
        };
        if !animation.moves() {
            continue;
        }
        let frame = animation.frame_at(Completion::Restart, seconds);
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
        apply_skins(group, proxies, &mut posed);
        if let Some(slice) = instances.get_mut(group.span.clone()) {
            slice.clone_from_slice(&posed);
        }
    }
}

pub fn advance_object_uv_mappers(scene: &ObjectScene, instances: &mut [Instance], seconds: f32) {
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
        group.model.apply_uv_mappers(placements, slice, seconds);
    }
}

pub fn advance_object_sway(
    scene: &ObjectScene,
    instances: &mut [Instance],
    proxies: &[ObjectProxy],
    offsets: &[[f32; 3]; mint_sage_asset::SWAY_OFFSET_COUNT],
) {
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
                .enumerate()
                .filter_map(|(at, slot)| proxies.get(*slot).map(|proxy| (at, *slot, proxy)))
                .map(|(at, slot, proxy)| {
                    let [x, y, z, _] = proxy.placement.to_cols()[3];
                    let still = (scene.tainted_still && group.tainted.get(at) == Some(&true))
                        || scene.still.get(slot) == Some(&true);
                    let index = if still {
                        0
                    } else {
                        mint_sage_asset::sway_index(x, y)
                    };
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
    }
}
