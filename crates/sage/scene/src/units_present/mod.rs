use std::collections::BTreeMap;
use std::sync::Arc;

use mint_render::{
    DepthTest, DetailAlphaCombine, DetailCombine, Frustum, Instance, Mat4, Renderer, TextureHandle,
};
use mint_sage_ini::{
    AnimationClip, AnimationStateDef, DefaultAnimation, DrawModuleView, HouseColorTable,
    ModelStateDef, NuggetWeather, ObjectIndex, StateDecal,
};
use mint_sage_model::{
    load_animation, CachedModel, GpuSkin, LoadedAnimation, ModelAnimation, ModelCache, PartPose,
    PoseBuffers, Resolver, Template,
};
use mint_vfs::Vfs;

use crate::animation_state::{
    pick_body, variant, CompiledState, Conditions, ExtraTokens, MachineReport, ModelStateMachine,
    StateMachine,
};

mod blend;
mod fx;
mod index;
mod pose;
mod ride;
mod tint;

pub use tint::{TintEnvelope, TintSlot};

use blend::Blend;
pub use blend::{blend_seconds, Blending, DEFAULT_BLEND_FRAMES};
pub use pose::{
    advance_phase, carried_start_frame, manual_frame, may_be_seen, natural_seconds, stretch_bound,
};
use pose::{pair_drawables, pose_all};

pub type Skin = Arc<[Option<TextureHandle>]>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitPose {
    pub id: u32,
    pub generation: u32,
    pub template: usize,
    pub placement: Mat4,
    pub conditions: Conditions,
    pub attack_cycle: u32,
    pub attack_slot: Option<usize>,
    pub speed: f32,
    pub weapon_cycle: f32,
    pub house_color: [f32; 4],
    pub hero_colours: Option<[[f32; 4]; 3]>,
    pub shroud_masked: bool,
    pub progress: Option<f32>,
    pub ghost: Option<GhostLook>,
    pub casts_shadow: bool,
    pub opacity: f32,
    pub look: LookKey,
    pub ride: Option<Ride>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ride {
    pub container: u32,
    pub pivot: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookKey(u32);

impl LookKey {
    pub const PLAIN: Self = Self(0);
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookTexture {
    pub from: Arc<str>,
    pub to: Arc<str>,
    pub exclude: Vec<Arc<str>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LookSubObject {
    pub name: Arc<str>,
    pub shown: bool,
    pub permanent: bool,
    pub rewrites: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitLook {
    pub conditions: Vec<Arc<str>>,
    pub sub_objects: Vec<LookSubObject>,
    pub textures: Vec<LookTexture>,
}

#[derive(Debug, Clone, Default)]
pub struct LookArt {
    pub writes: Arc<[LookSubObject]>,
    pub skin: Option<Skin>,
    pub conditions: Conditions,
}

#[must_use]
pub fn names_part(pattern: &str, part: &str) -> bool {
    let part = part.trim_end_matches(char::from(0));
    let suffix = part.rsplit_once('.').map_or(part, |(_, tail)| tail);
    match pattern.strip_suffix('*') {
        Some(prefix) => [part, suffix].iter().any(|name| {
            name.get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        }),
        None => part.eq_ignore_ascii_case(pattern) || suffix.eq_ignore_ascii_case(pattern),
    }
}

#[must_use]
pub fn part_hidden(overrides: &[(Arc<str>, bool)], part: &str) -> bool {
    part_override(overrides, part) == Some(false)
}

#[must_use]
pub fn part_override<S: AsRef<str>>(overrides: &[(S, bool)], part: &str) -> Option<bool> {
    overrides
        .iter()
        .rev()
        .find(|(pattern, _)| names_part(pattern.as_ref(), part))
        .map(|(_, shown)| *shown)
}

pub(super) fn tire_pivots_for(
    tires: Option<&mint_sage_ini::TruckTires>,
    model: &CachedModel,
) -> Vec<usize> {
    let Some(tires) = tires.filter(|tires| tires.rotation_multiplier != 0.0) else {
        return Vec::new();
    };
    tires
        .bones
        .iter()
        .filter_map(|bone| model.pivot_named(bone))
        .collect()
}

#[must_use]
pub(super) fn model_part_overrides<S: AsRef<str>>(
    overrides: &[(S, bool)],
    model: &CachedModel,
) -> Vec<Option<bool>> {
    let parts: Vec<(&str, Option<usize>)> = model
        .parts
        .iter()
        .map(|part| (part.name.as_str(), part.pivot))
        .collect();
    let pivots = model
        .hierarchy
        .as_ref()
        .map_or(&[][..], |hierarchy| hierarchy.pivots.as_slice());
    bone_part_overrides(overrides, &parts, pivots)
}

fn bone_part_overrides<S: AsRef<str>>(
    overrides: &[(S, bool)],
    parts: &[(&str, Option<usize>)],
    pivots: &[mint_sage_w3d::Pivot],
) -> Vec<Option<bool>> {
    let below = |bone: usize, ancestor: usize| {
        let mut at = bone;
        for _ in 0..pivots.len() {
            match pivots
                .get(at)
                .and_then(|pivot| pivot.parent)
                .and_then(|parent| usize::try_from(parent).ok())
            {
                Some(parent) if parent == ancestor => return true,
                Some(parent) if parent != 0 => at = parent,
                _ => return false,
            }
        }
        false
    };
    let mut resolved = vec![None; parts.len()];
    for (pattern, shown) in overrides {
        for (named, (name, bone)) in parts.iter().enumerate() {
            if !names_part(pattern.as_ref(), name) {
                continue;
            }
            let bone = bone.filter(|&bone| bone != 0 && bone < pivots.len());
            for (index, (slot, (_, own))) in resolved.iter_mut().zip(parts).enumerate() {
                let cascades = bone
                    .zip(*own)
                    .is_some_and(|(bone, own)| own != 0 && below(own, bone));
                if index == named || cascades {
                    *slot = Some(*shown);
                }
            }
        }
    }
    resolved
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostLook {
    pub opacity: f32,
    pub tint: [f32; 3],
    pub light_add: [f32; 3],
    pub state_particles: bool,
}

#[derive(Debug, Clone, Default)]
struct Scratch {
    pose: PoseBuffers,
    script_env: mint_sage_lua::Env,
    refused: crate::anim_script::Refusals,
    profiling: bool,
    sample_ns: u128,
    entries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateBody {
    Pool,
    Body(usize),
    Nothing,
    Unloaded,
}

pub struct CompanionModule {
    pub states: Vec<AnimationStateDef>,
    pub machine: StateMachine,
    pub model_states: Vec<ModelStateDef>,
    pub model_machine: ModelStateMachine,
    pub has_model: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FloorFlags {
    pub(crate) hidden: bool,
    pub(crate) lock: bool,
    pub(crate) changed: bool,
}

pub struct FloorModule {
    pub model: Arc<CachedModel>,
    pub hide_if: Conditions,
    pub tag: Option<String>,
    pub start_hidden: bool,
    weather: Vec<(NuggetWeather, Skin)>,
    force_to_back: bool,
}

pub struct UnitTemplate {
    pub name: String,
    pub light_set: u8,
    swirl_host: bool,
    pub model: Option<String>,
    pub bodies: Vec<String>,
    pub pool: usize,
    pub state_bodies: Vec<StateBody>,
    pub clip_bodies: Vec<Option<usize>>,
    pub states: Vec<AnimationStateDef>,
    scripts: Vec<Option<mint_sage_lua::Script>>,
    transition_names: BTreeMap<String, usize>,
    pub machine: StateMachine,
    pub model_states: Vec<ModelStateDef>,
    pub model_machine: ModelStateMachine,
    pub hide_if: Conditions,
    pub ok_to_change_model_color: bool,
    pub truck_tires: Option<mint_sage_ini::TruckTires>,
    pub extras: ExtraTokens,
    pub companions: Vec<CompanionModule>,
    pub floors: Vec<FloorModule>,
    pub modules: Vec<usize>,
    pub module_tags: Vec<Option<String>>,
    body_rank: usize,
    pub module_of: Option<usize>,
    pub attach: Option<mint_render::Vec3>,
    look_conditions: Vec<Option<Conditions>>,
    looks: BTreeMap<(usize, LookKey), Arc<LookArt>>,
    skins: BTreeMap<(usize, usize), Skin>,
    bound: BTreeMap<(usize, usize, usize), Arc<ModelAnimation>>,
    failures: Vec<(String, String)>,
    art: Vec<Option<Arc<BodyArt>>>,
    radius: BTreeMap<(usize, usize, usize), f32>,
    animation_sounds: Vec<mint_sage_ini::AnimationSoundEntry>,
    pub max_update_range_cap: Option<f32>,
    sound_cues: BTreeMap<(usize, usize), Vec<(String, Vec<u32>)>>,
    frame_event_parts: BTreeMap<String, Vec<(String, bool, bool)>>,
}

pub struct BodyArt {
    pub model: Arc<CachedModel>,
    pub gpu: GpuSkin,
}

impl BodyArt {
    fn nothing() -> Self {
        Self {
            model: Arc::new(CachedModel {
                name: String::new(),
                hierarchy: None,
                parts: Vec::new(),
                bounds: mint_render::Aabb::EMPTY,
                vertices: 0,
                triangles: 0,
            }),
            gpu: GpuSkin::default(),
        }
    }
}

impl UnitTemplate {
    fn attaches_particles(&self) -> bool {
        self.states.iter().any(|state| !state.particles.is_empty())
            || self
                .model_states
                .iter()
                .any(|state| !state.particles.is_empty())
            || self.companions.iter().any(|companion| {
                companion
                    .states
                    .iter()
                    .any(|state| !state.particles.is_empty())
                    || companion
                        .model_states
                        .iter()
                        .any(|state| !state.particles.is_empty())
            })
    }

    #[must_use]
    pub fn animation_prefix(&self, body: usize) -> &str {
        let Some(name) = self.bodies.get(body) else {
            return "";
        };
        self.model_states
            .iter()
            .filter(|state| {
                state
                    .model
                    .as_deref()
                    .is_some_and(|model| model.eq_ignore_ascii_case(name))
            })
            .filter_map(|state| state.model_animation_prefix.as_deref())
            .find(|prefix| !prefix.is_empty())
            .unwrap_or("")
    }

    #[must_use]
    pub fn clip(&self, body: usize, state: usize, clip: usize) -> Option<&Arc<ModelAnimation>> {
        self.bound.get(&(body, state, clip))
    }

    #[must_use]
    pub fn body_count(&self) -> usize {
        self.pool
    }

    #[must_use]
    pub fn body_for(&self, state: Option<usize>, pool_body: usize) -> Option<usize> {
        let pool = (pool_body < self.pool).then_some(pool_body);
        let Some(state) = state else {
            return pool;
        };
        match self.state_bodies.get(state) {
            Some(StateBody::Body(body)) if self.art.get(*body).is_some_and(Option::is_some) => {
                Some(*body)
            }
            Some(StateBody::Nothing) => None,
            _ => pool,
        }
    }

    #[must_use]
    pub fn rebuilds_model(&self, from: Option<usize>, to: Option<usize>) -> bool {
        let def = |state: Option<usize>| state.and_then(|state| self.model_states.get(state));
        let (Some(from), Some(to)) = (def(from), def(to)) else {
            return true;
        };
        let same_model = match (&from.model, &to.model) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        };
        !same_model || !from.textures.is_empty() || !to.textures.is_empty()
    }

    #[must_use]
    pub fn retains_sub_objects(&self, state: Option<usize>) -> bool {
        state
            .and_then(|state| self.model_states.get(state))
            .is_some_and(|def| def.retain_sub_objects)
    }

    #[must_use]
    pub fn clip_body(&self, state: usize) -> Option<usize> {
        self.clip_bodies.get(state).copied().flatten()
    }

    #[must_use]
    pub fn bound_clips(&self) -> usize {
        self.bound.len()
    }

    #[must_use]
    pub fn failures(&self) -> &[(String, String)] {
        &self.failures
    }

    fn look_conditions_of(&self, key: LookKey) -> Option<Conditions> {
        usize::try_from(key.0)
            .ok()
            .and_then(|at| self.look_conditions.get(at).copied().flatten())
    }

    #[must_use]
    pub(crate) fn script_of(&self, state: usize) -> Option<&mint_sage_lua::Script> {
        self.scripts.get(state)?.as_ref()
    }

    #[must_use]
    pub(crate) fn transition_by_name(&self, name: &str) -> Option<usize> {
        self.transition_names
            .get(&name.to_ascii_uppercase())
            .copied()
    }

    #[must_use]
    pub(crate) fn identity_of(&self, state: Option<usize>) -> String {
        let Some(def) = state.and_then(|state| self.states.get(state)) else {
            return String::new();
        };
        def.state_name
            .clone()
            .or_else(|| def.transition_name().map(str::to_owned))
            .unwrap_or_default()
    }

    fn set_look_conditions(&mut self, key: LookKey, conditions: Conditions) {
        let Ok(at) = usize::try_from(key.0) else {
            return;
        };
        if self.look_conditions.len() <= at {
            self.look_conditions.resize(at.saturating_add(1), None);
        }
        if let Some(slot) = self.look_conditions.get_mut(at) {
            *slot = Some(conditions);
        }
    }
}

pub const LOOK_STAMP_IDS: usize = 1 << 20;

pub struct UnitDrawable {
    pub template: usize,
    pub body: usize,
    pub pool_body: usize,
    pub hidden: bool,
    body_missing: bool,
    pub id: u32,
    pub generation: u32,
    pub model: Arc<CachedModel>,
    art: Arc<BodyArt>,
    poses: Vec<PartPose>,
    bones: Vec<Mat4>,
    bone_base: Option<u32>,
    pending: Option<(usize, usize, f32)>,
    sampled: Option<(usize, usize, u32)>,
    placement: Mat4,
    shroud_masked: bool,
    opacity: f32,
    tints: tint::Tints,
    drawn: bool,
    present: bool,
    state: Option<usize>,
    last_conditions: Conditions,
    last_attack_cycle: u32,
    swing_queued: Option<f32>,
    swing_repicked: bool,
    swing_late_by: Option<f32>,
    cut_swings: u32,
    last_cut: Option<usize>,
    last_cut_at: f32,
    moving_until: f32,
    drawn_speed: f32,
    tire_pivots: Vec<usize>,
    tire_angle: f32,
    tire_angle_applied: f32,
    clip: usize,
    picks: u32,
    mid_transition: bool,
    queue: Vec<usize>,
    transition_target: Option<usize>,
    model_state: Option<usize>,
    companion_states: Vec<(Option<usize>, Option<usize>)>,
    skin: Option<Skin>,
    entered_at: f32,
    phase: Option<f32>,
    fit_cycle: Option<f32>,
    advanced_at: f32,
    house_color: [f32; 4],
    hero_colours: Option<[[f32; 4]; 3]>,
    light_set: u8,
    ok_to_change_model_color: bool,
    ghost: Option<GhostLook>,
    casts_shadow: bool,
    blend: Option<Blend>,
    blend_request: Option<(usize, usize, f32)>,
    look_at: (usize, LookKey),
    look: Option<Arc<LookArt>>,
    stand_in_at: Option<(usize, Conditions, u32)>,
    stand_in_choice: Option<(usize, usize)>,
    script_state: Option<usize>,
    parts_body: usize,
    state_parts: Option<Arc<[Option<bool>]>>,
    plain_parts: Vec<(String, bool)>,
    permanent_parts: Vec<(String, bool)>,
    look_writes: Arc<[LookSubObject]>,
    floor_flags: Vec<FloorFlags>,
    module_hidden: Vec<bool>,
    fx_frame: Option<f32>,
    fx_pending: Vec<String>,
    bone_fx_pending: Vec<fx::BoneFxCue>,
    sound_pending: Vec<String>,
}

impl UnitDrawable {
    fn house_layer(&self) -> Option<([f32; 4], [[f32; 4]; 2])> {
        match self.hero_colours {
            Some(list) => house_colour_layer(&list),
            None if self.ok_to_change_model_color => house_colour_layer(&[self.house_color]),
            None => None,
        }
    }

    #[must_use]
    pub const fn entered_at(&self) -> f32 {
        self.entered_at
    }

    #[must_use]
    pub const fn state(&self) -> Option<usize> {
        self.state
    }

    #[must_use]
    pub const fn phase(&self) -> Option<f32> {
        self.phase
    }

    #[must_use]
    pub const fn model_state(&self) -> Option<usize> {
        self.model_state
    }

    #[must_use]
    pub fn companion_states(&self) -> &[(Option<usize>, Option<usize>)] {
        &self.companion_states
    }

    #[must_use]
    pub const fn starts_state_particles(&self) -> bool {
        match self.ghost {
            Some(ghost) => ghost.state_particles,
            None => true,
        }
    }

    #[must_use]
    pub const fn placement(&self) -> Mat4 {
        self.placement
    }

    #[must_use]
    pub fn bones(&self) -> &[Mat4] {
        &self.bones
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitSceneReport {
    pub templates: usize,
    pub templates_drawn: usize,
    pub bodies: usize,
    pub bodies_unloadable: usize,
    pub particle_only: usize,
    pub drawables: usize,
    pub vertices: usize,
    pub meshes: usize,
    pub skins_refused: usize,
    pub clips_bound: usize,
    pub animation_sound_cues: usize,
    pub clips_failed: usize,
    pub machines: MachineReport,
    pub templates_without_fallback: usize,
    pub model_states: usize,
    pub model_states_reachable: usize,
    pub skins: usize,
    pub model_swaps: usize,
    pub model_swaps_unbuilt: usize,
    pub extra_tokens: usize,
    pub extra_tokens_refused: usize,
    pub looks: usize,
    pub upgrade_skins: usize,
    pub floors: usize,
    pub floors_unloadable: usize,
    pub model_modules: usize,
    pub model_modules_unloadable: usize,
    pub model_modules_bone_missing: usize,
    pub clip_bodies_rescued: usize,
    pub scripts_parsed: usize,
    pub scripts_failed: usize,
    pub script_failures: Vec<String>,
    pub host_calls_refused: crate::anim_script::Refusals,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UnitDrawStats {
    pub drawables: usize,
    pub sampled: usize,
    pub out_of_view: usize,
    pub reused: usize,
    pub skinned_vertices: usize,
    pub uploaded_bytes: usize,
    pub instances: usize,
    pub floors: usize,
    pub model_modules: usize,
    pub sample_s: f64,
    pub pair_s: f64,
    pub pose_threads: usize,
    pub pose_cpu_s: f64,
    pub pose_worst_s: f64,
    pub pose_sample_s: f64,
    pub state_entries: usize,
    pub skin_s: f64,
    pub place_s: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoneFxFire {
    pub fx_list: String,
    pub bone: Option<String>,
    pub frame_stop: u32,
    pub slot: usize,
}

#[derive(Default)]
pub struct UnitScene {
    pub templates: Vec<UnitTemplate>,
    drawables: Vec<UnitDrawable>,
    keys: index::DrawableKeys,
    drawing_nothing: std::sync::OnceLock<Vec<u32>>,
    by_name: BTreeMap<String, usize>,
    cache: ModelCache,
    body_art: BTreeMap<String, Option<Arc<BodyArt>>>,
    clips: BTreeMap<String, Result<LoadedAnimation, String>>,
    report: UnitSceneReport,
    profiling: bool,
    stats: UnitDrawStats,
    order: Vec<usize>,
    palette: Vec<Mat4>,
    paired: Vec<Option<usize>>,
    threads: usize,
    blending: Blending,
    transitions_enabled: bool,
    looks: Vec<UnitLook>,
    look_keys: BTreeMap<UnitLook, LookKey>,
    look_stamps: Vec<Option<(u64, LookKey)>>,
    look_misses: std::collections::BTreeSet<String>,
    pending_fx: Vec<(u32, String)>,
    pending_bone_fx: Vec<(u32, BoneFxFire)>,
    pending_sound: Vec<(u32, String, Option<f32>)>,
    expanded: Vec<UnitPose>,
}

impl UnitScene {
    #[must_use]
    pub fn new() -> Self {
        Self {
            transitions_enabled: true,
            ..Self::default()
        }
    }

    pub fn set_house_colors(&mut self, table: std::sync::Arc<HouseColorTable>) {
        self.cache.set_house_colors(table);
    }

    #[must_use]
    pub fn frame_event_data(&self, slot: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let Some(template) = self.templates.get(slot) else {
            return out;
        };
        for event in template
            .states
            .iter()
            .flat_map(|state| &state.lua_frame_events)
        {
            if let Some(data) = &event.data {
                if !out.contains(data) {
                    out.push(data.clone());
                }
            }
        }
        out
    }

    pub fn set_frame_event_parts(
        &mut self,
        slot: usize,
        data: &str,
        parts: Vec<(String, bool, bool)>,
    ) {
        if let Some(template) = self.templates.get_mut(slot) {
            template.frame_event_parts.insert(data.to_owned(), parts);
        }
    }

    #[must_use]
    pub fn house_color_entries(&self) -> usize {
        self.cache.house_color_entries()
    }

    #[must_use]
    pub fn report(&self) -> &UnitSceneReport {
        &self.report
    }

    pub fn set_profiling(&mut self, on: bool) {
        self.profiling = on;
    }

    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads;
    }

    #[must_use]
    pub fn blending(&self) -> Blending {
        self.blending
    }

    pub fn set_blending(&mut self, blending: Blending) {
        self.blending = blending;
    }

    #[must_use]
    pub fn transitions_enabled(&self) -> bool {
        self.transitions_enabled
    }

    pub fn set_transitions_enabled(&mut self, on: bool) {
        self.transitions_enabled = on;
    }

    #[must_use]
    pub fn draw_stats(&self) -> UnitDrawStats {
        self.stats
    }

    #[must_use]
    pub fn bones(&self) -> &[Mat4] {
        &self.palette
    }

    #[must_use]
    pub fn drawables(&self) -> &[UnitDrawable] {
        &self.drawables
    }

    #[must_use]
    pub fn ids_drawing_nothing(&self) -> Vec<u32> {
        self.drawing_nothing
            .get_or_init(|| {
                let mut shown: Vec<(u32, bool)> = self
                    .drawables
                    .iter()
                    .filter(|drawable| drawable.ghost.is_none())
                    .map(|drawable| (drawable.id, !drawable.hidden))
                    .collect();
                shown.sort_unstable();
                shown
                    .chunk_by(|a, b| a.0 == b.0)
                    .filter(|modules| modules.iter().all(|(_, shown)| !shown))
                    .filter_map(|modules| modules.first().map(|(id, _)| *id))
                    .collect()
            })
            .clone()
    }

    pub fn state_decals(&self) -> impl Iterator<Item = (u32, Mat4, &StateDecal)> + '_ {
        self.drawables
            .iter()
            .filter(|drawable| !drawable.hidden)
            .filter_map(|drawable| Some((drawable, self.templates.get(drawable.template)?)))
            .flat_map(|(drawable, template)| {
                let body = drawable
                    .model_state
                    .and_then(|state| template.model_states.get(state));
                let companions = template
                    .companions
                    .iter()
                    .zip(&drawable.companion_states)
                    .map(|(companion, (_, state))| {
                        state.and_then(|state| companion.model_states.get(state))
                    });
                std::iter::once(body)
                    .chain(companions)
                    .flatten()
                    .filter_map(|state| state.decal.as_ref())
                    .map(|decal| (drawable.id, drawable.placement, decal))
            })
    }

    #[must_use]
    pub fn drawable_by_id(&self, id: u32) -> Option<&UnitDrawable> {
        self.drawables.get(self.keys.by_id(id)?)
    }

    pub fn active_states(&self, id: u32) -> impl Iterator<Item = &ModelStateDef> {
        self.drawables
            .iter()
            .filter(move |drawable| drawable.id == id)
            .filter_map(|drawable| {
                self.templates
                    .get(drawable.template)?
                    .model_states
                    .get(drawable.model_state?)
            })
    }

    #[must_use]
    pub fn passenger_pivot(&self, container: u32, prefix: &str, slot: usize) -> Option<u16> {
        let model = &self.drawable_by_id(container)?.model;
        let pivot = if model.pivot_named(&format!("{prefix}01")).is_some() {
            model.pivot_named(&format!("{prefix}{:02}", slot.checked_add(1)?))
        } else if slot == 0 {
            model.pivot_named(prefix)
        } else {
            None
        }?;
        u16::try_from(pivot).ok()
    }

    pub fn play_tint(
        &mut self,
        id: u32,
        slot: TintSlot,
        envelope: TintEnvelope,
        now: f32,
    ) -> usize {
        let mut reached = 0usize;
        for drawable in self
            .drawables
            .iter_mut()
            .filter(|drawable| drawable.id == id && drawable.ghost.is_none())
        {
            drawable.tints.play(slot, envelope, now);
            reached = reached.saturating_add(1);
        }
        reached
    }

    #[must_use]
    pub fn tint_at(&self, id: u32, now: f32) -> Option<[f32; 3]> {
        self.drawables
            .iter()
            .find(|drawable| drawable.id == id && drawable.ghost.is_none())
            .map(|drawable| drawable.tints.at(now).unwrap_or([0.0; 3]))
    }

    #[must_use]
    pub(crate) fn drawable_by_key(
        &self,
        id: u32,
        generation: u32,
        template: usize,
    ) -> Option<&UnitDrawable> {
        self.drawables
            .get(self.keys.by_key(id, generation, template)?)
    }

    pub(crate) fn module_drawables<'a>(
        &'a self,
        drawable: &'a UnitDrawable,
    ) -> impl Iterator<Item = &'a UnitDrawable> {
        let owner = self
            .templates
            .get(drawable.template)
            .and_then(|template| template.module_of)
            .unwrap_or(drawable.template);
        let (modules, rank) = self.templates.get(owner).map_or((&[][..], 0), |template| {
            (template.modules.as_slice(), template.body_rank)
        });
        let rank = rank.min(modules.len());
        let (before, after) = modules.split_at(rank);
        before
            .iter()
            .copied()
            .chain(std::iter::once(owner))
            .chain(after.iter().copied())
            .filter_map(move |slot| self.drawable_by_key(drawable.id, drawable.generation, slot))
    }

    pub fn intern_look(&mut self, look: UnitLook) -> LookKey {
        if look == UnitLook::default() {
            return LookKey::PLAIN;
        }
        if let Some(found) = self.look_keys.get(&look) {
            return *found;
        }
        if self.looks.is_empty() {
            self.looks.push(UnitLook::default());
        }
        let Ok(next) = u32::try_from(self.looks.len()) else {
            return LookKey::PLAIN;
        };
        let key = LookKey(next);
        self.looks.push(look.clone());
        self.look_keys.insert(look, key);
        key
    }

    pub fn look_for(&mut self, id: u32, stamp: u64, build: impl FnOnce() -> UnitLook) -> LookKey {
        let slot = usize::try_from(id).ok().filter(|at| *at < LOOK_STAMP_IDS);
        if let Some(Some((held, key))) = slot.and_then(|at| self.look_stamps.get(at)) {
            if *held == stamp {
                return *key;
            }
        }
        let key = self.intern_look(build());
        if let Some(at) = slot {
            if self.look_stamps.len() <= at {
                self.look_stamps.resize(at.saturating_add(1), None);
            }
            if let Some(entry) = self.look_stamps.get_mut(at) {
                *entry = Some((stamp, key));
            }
        }
        key
    }

    #[must_use]
    pub fn look(&self, key: LookKey) -> UnitLook {
        usize::try_from(key.0)
            .ok()
            .and_then(|at| self.looks.get(at))
            .cloned()
            .unwrap_or_default()
    }

    pub fn prepare_looks(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        poses: &[UnitPose],
    ) -> usize {
        let templates = &self.templates;
        let mut wanted: Vec<(usize, LookKey)> = poses
            .iter()
            .filter(|pose| pose.look != LookKey::PLAIN)
            .flat_map(|pose| {
                let modules = templates
                    .get(pose.template)
                    .map_or(&[][..], |art| art.modules.as_slice());
                std::iter::once(pose.template)
                    .chain(modules.iter().copied())
                    .map(|template| (template, pose.look))
            })
            .filter(|(template, look)| {
                templates
                    .get(*template)
                    .is_some_and(|art| art.look_conditions_of(*look).is_none())
            })
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        let mut built = 0usize;
        let mut by_owner: BTreeMap<(String, LookKey), Vec<bool>> = BTreeMap::new();
        for (template, key) in wanted {
            let Some(art) = self.templates.get(template) else {
                continue;
            };
            let look = self.look(key);
            let name = art.name.clone();
            let module_of = art.module_of;
            let conditions = art
                .extras
                .bits_of(look.conditions.iter().map(|flag| &**flag));
            let bodies: Vec<(usize, Arc<BodyArt>)> = art
                .art
                .iter()
                .enumerate()
                .filter_map(|(body, loaded)| loaded.clone().map(|loaded| (body, loaded)))
                .collect();
            let mut matched = vec![false; look.sub_objects.len()];
            let mut resolved = Vec::with_capacity(bodies.len());
            let writes: Arc<[LookSubObject]> = look.sub_objects.iter().cloned().collect();
            for (body, body_art) in bodies {
                let parts = &body_art.model.parts;
                for (seen, write) in matched.iter_mut().zip(&look.sub_objects) {
                    *seen |= parts.iter().any(|part| names_part(&write.name, &part.name));
                }
                let skin = self.look_skin(renderer, vfs, resolver, &body_art.model, &look.textures);
                resolved.push((
                    (body, key),
                    Arc::new(LookArt {
                        writes: Arc::clone(&writes),
                        skin,
                        conditions,
                    }),
                ));
            }
            let owner = match module_of {
                Some(owner) => self
                    .templates
                    .get(owner)
                    .map_or(name, |art| art.name.clone()),
                None => name,
            };
            let seen = by_owner
                .entry((owner, key))
                .or_insert_with(|| vec![false; matched.len()]);
            for (seen, matched) in seen.iter_mut().zip(&matched) {
                *seen |= matched;
            }
            if let Some(art) = self.templates.get_mut(template) {
                art.set_look_conditions(key, conditions);
                art.looks.extend(resolved);
            }
            built = built.saturating_add(1);
        }
        for ((owner, key), seen) in by_owner {
            let look = self.look(key);
            for (seen, write) in seen.iter().zip(&look.sub_objects) {
                if !seen {
                    self.look_misses.insert(format!("{owner}: {}", write.name));
                }
            }
        }
        self.report.looks = self.report.looks.saturating_add(built);
        built
    }

    fn look_skin(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        model: &CachedModel,
        swaps: &[LookTexture],
    ) -> Option<Skin> {
        if swaps.is_empty() {
            return None;
        }
        let mut skin: Vec<Option<TextureHandle>> = vec![None; model.parts.len()];
        for swap in swaps {
            let pair = [(swap.from.to_string(), swap.to.to_string())];
            let found = self.cache.substitute(renderer, vfs, resolver, model, &pair);
            for ((slot, found), part) in skin.iter_mut().zip(found).zip(&model.parts) {
                let excluded = swap
                    .exclude
                    .iter()
                    .any(|pattern| names_part(pattern, &part.name));
                if found.is_some() && !excluded {
                    *slot = found;
                }
            }
        }
        skin.iter().any(Option::is_some).then(|| skin.into())
    }

    #[must_use]
    pub fn look_lines(&self) -> Vec<String> {
        let r = &self.report;
        let mut lines = vec![format!(
            "looks: {} upgrade look(s) interned, {} (template, look) pair(s) resolved; \
             {} template-own condition token(s), {} refused; \
             {} texture swap(s) for upgrade-only states; \
             {} sub-object name(s) matched no part",
            self.looks.len().saturating_sub(1),
            r.looks,
            r.extra_tokens,
            r.extra_tokens_refused,
            r.upgrade_skins,
            self.look_misses.len()
        )];
        lines.extend(
            self.look_misses
                .iter()
                .map(|miss| format!("  NO PART {miss}")),
        );
        lines
    }

    pub fn set_light_set(&mut self, slot: usize, light_set: u8) {
        if let Some(template) = self.templates.get_mut(slot) {
            template.light_set = light_set;
        }
        for drawable in self.drawables.iter_mut().filter(|d| d.template == slot) {
            drawable.light_set = light_set;
        }
    }

    pub fn release_load_caches(&mut self) {
        self.clips = BTreeMap::new();
    }

    pub fn set_swirl_host(&mut self, slot: usize, host: bool) {
        if let Some(template) = self.templates.get_mut(slot) {
            template.swirl_host = host;
        }
    }

    pub(crate) fn is_swirl_host(&self, slot: usize) -> bool {
        self.templates.get(slot).is_some_and(|t| t.swirl_host)
    }

    pub fn template_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        index: &ObjectIndex,
        name: &str,
    ) -> Option<usize> {
        let key = name.to_ascii_lowercase();
        if let Some(found) = self.by_name.get(&key) {
            return Some(*found);
        }
        self.report.templates = self.report.templates.saturating_add(1);
        let tree_body = index.tree_drawable_body(name);
        let mut model_states = tree_body.as_ref().map_or_else(
            || index.model_states(name).to_vec(),
            |view| view.model_states.to_vec(),
        );
        if let Some(decal) = index.object_decal(name) {
            for state in model_states
                .iter_mut()
                .filter(|state| state.decal.is_none() && state.model.is_some())
            {
                state.decal = Some(decal.clone());
            }
        }
        let body = match tree_body {
            Some(view) => ModuleSource {
                model: view.model.map(str::to_owned),
                alternates: view.alternates.to_vec(),
                states: view.animation_states.to_vec(),
                model_states,
                hide_if: view.hide_if.to_vec(),
                ok_to_change_model_color: view.ok_to_change_model_color,
                truck_tires: None,
            },
            None => ModuleSource {
                model: index.lookup(name).model().map(str::to_owned),
                alternates: index.model_alternates(name).to_vec(),
                states: index.animation_states(name).to_vec(),
                model_states,
                hide_if: index.hide_if_model_conditions(name).to_vec(),
                ok_to_change_model_color: index.ok_to_change_model_color(name),
                truck_tires: index.truck_tires(name).cloned(),
            },
        };
        let companions: Vec<DrawModuleView<'_>> = index
            .draw_modules(name)
            .into_iter()
            .filter(|module| !module.is_model_module && draw_module_has_attachments(module))
            .collect();
        let slot = self.build_template(
            renderer,
            vfs,
            resolver,
            name.to_owned(),
            body,
            &index.floor_modules(name),
            &companions,
        )?;
        if let Some(behavior) = index.animation_sound_behavior(name) {
            if let Some(template) = self.templates.get_mut(slot) {
                template.animation_sounds = behavior.sounds.clone();
                #[allow(clippy::cast_precision_loss)]
                {
                    template.max_update_range_cap =
                        behavior.max_update_range_cap.map(|cap| cap as f32);
                }
            }
            self.resolve_sound_cues(slot);
        }
        let mut modules = Vec::new();
        let mut module_tags = Vec::new();
        let declared_before_body = index.model_modules_before_body(name);
        let mut body_rank = 0;
        for (declared, view) in index.model_modules(name).into_iter().enumerate() {
            if view.is_floor() {
                continue;
            }
            let module_name = format!("{name}/{}", view.tag.unwrap_or_default());
            let source = ModuleSource {
                model: view.model.map(str::to_owned),
                alternates: view.alternates.to_vec(),
                states: view.animation_states.to_vec(),
                model_states: view.model_states.to_vec(),
                hide_if: view.hide_if.to_vec(),
                ok_to_change_model_color: view.ok_to_change_model_color,
                truck_tires: None,
            };
            let Some(module) =
                self.build_template(renderer, vfs, resolver, module_name, source, &[], &[])
            else {
                self.report.model_modules_unloadable =
                    self.report.model_modules_unloadable.saturating_add(1);
                continue;
            };
            let attach = view.attach_to_bone.map(|bone| {
                let offset = std::iter::once(slot)
                    .chain(modules.iter().copied())
                    .find_map(|owner| self.bind_bone_position(owner, bone));
                if offset.is_none() {
                    self.report.model_modules_bone_missing =
                        self.report.model_modules_bone_missing.saturating_add(1);
                }
                offset.unwrap_or(mint_render::Vec3::ZERO)
            });
            if let Some(template) = self.templates.get_mut(module) {
                template.module_of = Some(slot);
                template.attach = attach;
            }
            modules.push(module);
            module_tags.push(view.tag_as_written.map(str::to_owned));
            if declared < declared_before_body {
                body_rank = modules.len();
            }
        }
        self.report.model_modules = self.report.model_modules.saturating_add(modules.len());
        if let Some(template) = self.templates.get_mut(slot) {
            template.modules = modules;
            template.module_tags = module_tags;
            template.body_rank = body_rank;
        }
        self.by_name.insert(key, slot);
        self.report.templates_drawn = self.report.templates_drawn.saturating_add(1);
        Some(slot)
    }

    fn bind_bone_position(&self, slot: usize, bone: &str) -> Option<mint_render::Vec3> {
        let model = &self.templates.get(slot)?.art.first()?.as_ref()?.model;
        Some(
            model
                .bind_bone(model.pivot_named(bone)?)?
                .transform_point(mint_render::Vec3::ZERO),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_template(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: String,
        source: ModuleSource,
        floors: &[DrawModuleView<'_>],
        companions: &[DrawModuleView<'_>],
    ) -> Option<usize> {
        let ModuleSource {
            model,
            alternates,
            states,
            model_states,
            hide_if,
            ok_to_change_model_color,
            truck_tires,
        } = source;
        let bodies: Vec<String> = model.iter().cloned().chain(alternates).collect();
        let (scripts, script_report) = crate::anim_script::compile_states(&states);
        self.report.scripts_parsed = self
            .report
            .scripts_parsed
            .saturating_add(script_report.parsed);
        self.report.scripts_failed = self
            .report
            .scripts_failed
            .saturating_add(script_report.failed);
        for failure in &script_report.failures {
            if self.report.script_failures.len() < 64 {
                self.report.script_failures.push(failure.clone());
            }
        }
        let mut transition_names: BTreeMap<String, usize> = BTreeMap::new();
        for (index, state) in states.iter().enumerate() {
            if let Some(name) = state.transition_name() {
                transition_names
                    .entry(name.to_ascii_uppercase())
                    .or_insert(index);
            }
        }
        let mut extras = ExtraTokens::default();
        let machine = StateMachine::compile_with(&states, Some(&mut extras));
        let model_machine = ModelStateMachine::compile_with(&model_states, Some(&mut extras));
        let hide_if = conditions_of(&hide_if, &mut extras);
        let floor_specs: Vec<FloorSpec> = floors
            .iter()
            .filter_map(|module| {
                Some(FloorSpec {
                    model: module.model?.to_owned(),
                    hide_if: conditions_of(module.hide_if, &mut extras),
                    weather: module.weather_textures.to_vec(),
                    force_to_back: module.force_to_back,
                    tag: module.tag_as_written.map(str::to_owned),
                    start_hidden: module.start_hidden,
                })
            })
            .collect();
        let companions: Vec<CompanionModule> = companions
            .iter()
            .map(|module| {
                let states = module.animation_states.to_vec();
                let machine = StateMachine::compile_with(&states, Some(&mut extras));
                let model_states = module.model_states.to_vec();
                let model_machine =
                    ModelStateMachine::compile_with(&model_states, Some(&mut extras));
                CompanionModule {
                    states,
                    machine,
                    model_states,
                    model_machine,
                    has_model: module.model.is_some(),
                }
            })
            .collect();
        self.report.extra_tokens = self.report.extra_tokens.saturating_add(extras.len());
        self.report.extra_tokens_refused = self
            .report
            .extra_tokens_refused
            .saturating_add(extras.refused());
        let machines = &mut self.report.machines;
        let found = machine.report();
        machines.states = machines.states.saturating_add(found.states);
        machines.unreachable = machines.unreachable.saturating_add(found.unreachable);
        machines.transitions = machines.transitions.saturating_add(found.transitions);
        machines.scripted = machines.scripted.saturating_add(found.scripted);
        machines.clipless = machines.clipless.saturating_add(found.clipless);
        machines.unmodelled_tokens = machines
            .unmodelled_tokens
            .saturating_add(found.unmodelled_tokens);
        if !found.has_fallback {
            self.report.templates_without_fallback =
                self.report.templates_without_fallback.saturating_add(1);
        }

        let pool = bodies.len();
        let mut bodies = bodies;
        let state_bodies = resolve_state_bodies(&model_states, &model_machine, &mut bodies);

        let slot = self.templates.len();
        let variety = bodies.len();
        self.templates.push(UnitTemplate {
            name,
            light_set: mint_sage_model::OBJECT_LIGHT_SET,
            swirl_host: false,
            model,
            bodies,
            pool,
            state_bodies,
            clip_bodies: Vec::new(),
            states,
            scripts,
            transition_names,
            machine,
            model_states,
            model_machine,
            hide_if,
            ok_to_change_model_color,
            truck_tires,
            extras,
            companions,
            floors: Vec::new(),
            modules: Vec::new(),
            module_tags: Vec::new(),
            body_rank: 0,
            module_of: None,
            attach: None,
            look_conditions: Vec::new(),
            looks: BTreeMap::new(),
            skins: BTreeMap::new(),
            bound: BTreeMap::new(),
            failures: Vec::new(),
            art: Vec::new(),
            radius: BTreeMap::new(),
            animation_sounds: Vec::new(),
            max_update_range_cap: None,
            sound_cues: BTreeMap::new(),
            frame_event_parts: BTreeMap::new(),
        });
        let mut art = Vec::with_capacity(variety);
        for body in 0..variety {
            let loaded = self.load_body(renderer, vfs, resolver, slot, body);
            if loaded.is_none() {
                self.report.bodies_unloadable = self.report.bodies_unloadable.saturating_add(1);
            }
            art.push(loaded);
        }
        if art.is_empty()
            && self
                .templates
                .get(slot)
                .is_some_and(UnitTemplate::attaches_particles)
        {
            art.push(Some(Arc::new(BodyArt::nothing())));
            self.report.particle_only = self.report.particle_only.saturating_add(1);
        }
        let loaded = art.iter().filter(|body| body.is_some()).count();
        let wearable = if pool == 0 { art.len() } else { pool };
        if art.iter().take(wearable).all(Option::is_none) {
            return None;
        }
        let swaps = art.len().saturating_sub(pool);
        let swaps_loaded = art.iter().skip(pool).filter(|body| body.is_some()).count();
        self.report.model_swaps = self.report.model_swaps.saturating_add(swaps_loaded);
        self.report.model_swaps_unbuilt = self
            .report
            .model_swaps_unbuilt
            .saturating_add(swaps.saturating_sub(swaps_loaded));
        for (body, entry) in art.iter().enumerate() {
            if let Some(entry) = entry {
                self.bind_clips(vfs, resolver, slot, body, entry);
                self.build_skins(renderer, vfs, resolver, slot, body, entry);
            }
        }
        if let Some(template) = self.templates.get_mut(slot) {
            template.art = art;
        }
        let clip_bodies = self.rescue_clip_bodies(renderer, vfs, resolver, slot);
        self.report.clip_bodies_rescued = self
            .report
            .clip_bodies_rescued
            .saturating_add(clip_bodies.iter().filter(|body| body.is_some()).count());
        if let Some(template) = self.templates.get_mut(slot) {
            template.clip_bodies = clip_bodies;
            let bound_assets: Vec<String> = template
                .bound
                .values()
                .map(|animation| animation.asset.to_ascii_lowercase())
                .collect();
            template
                .failures
                .retain(|(asset, _)| !bound_assets.contains(&asset.to_ascii_lowercase()));
            template.failures.dedup();
            self.report.clips_bound = self.report.clips_bound.saturating_add(template.bound.len());
            self.report.clips_failed = self
                .report
                .clips_failed
                .saturating_add(template.failures.len());
        }
        let floors = self.load_floors(renderer, vfs, resolver, floor_specs);
        if let Some(template) = self.templates.get_mut(slot) {
            template.floors = floors;
        }
        self.report.bodies = self.report.bodies.saturating_add(loaded);
        Some(slot)
    }

    fn load_floors(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        specs: Vec<FloorSpec>,
    ) -> Vec<FloorModule> {
        let mut floors = Vec::with_capacity(specs.len());
        for spec in specs {
            let FloorSpec {
                model: name,
                hide_if,
                weather: textures,
                force_to_back,
                tag,
                start_hidden,
            } = spec;
            let Some(model) = self.cache.get_or_load(renderer, vfs, resolver, &name) else {
                self.report.floors_unloadable = self.report.floors_unloadable.saturating_add(1);
                continue;
            };
            let weather = textures
                .into_iter()
                .filter_map(|(weather, texture)| {
                    let skin =
                        weather_skin(&mut self.cache, renderer, vfs, resolver, &model, &texture)?;
                    Some((weather, skin))
                })
                .collect();
            floors.push(FloorModule {
                model,
                hide_if,
                tag,
                start_hidden,
                weather,
                force_to_back,
            });
        }
        self.report.floors = self.report.floors.saturating_add(floors.len());
        floors
    }

    fn load_body(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        template: usize,
        body: usize,
    ) -> Option<Arc<BodyArt>> {
        let name = self.templates.get(template)?.bodies.get(body)?.clone();
        let key = name.to_ascii_lowercase();
        if let Some(found) = self.body_art.get(&key) {
            return found.clone();
        }
        let loaded = self
            .cache
            .get_or_load(renderer, vfs, resolver, &name)
            .map(|model| {
                let gpu = GpuSkin::upload(renderer, &model);
                let skinned = model
                    .parts
                    .iter()
                    .filter(|part| part.skin.is_some())
                    .count();
                let uploaded = gpu.parts.iter().filter(|part| part.is_some()).count();
                self.report.skins_refused = self
                    .report
                    .skins_refused
                    .saturating_add(skinned.saturating_sub(uploaded));
                self.report.vertices = self.report.vertices.saturating_add(model.vertices);
                self.report.meshes = self.report.meshes.saturating_add(model.parts.len());
                Arc::new(BodyArt { model, gpu })
            });
        self.body_art.insert(key, loaded.clone());
        loaded
    }

    fn build_skins(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        template: usize,
        body: usize,
        body_art: &BodyArt,
    ) {
        let Some(art) = self.templates.get(template) else {
            return;
        };
        let Some(body_name) = art.bodies.get(body).cloned() else {
            return;
        };
        let mut built: Vec<((usize, usize), Skin)> = Vec::new();
        let (mut states, mut reachable, mut upgrade_skins) = (0usize, 0usize, 0usize);
        for (index, compiled) in art.model_machine.states().iter().enumerate() {
            let Some(state) = art.model_states.get(compiled.source) else {
                continue;
            };
            if body == 0 {
                states = states.saturating_add(1);
            }
            if !compiled.selectable {
                continue;
            }
            if body == 0 && compiled.reachable {
                reachable = reachable.saturating_add(1);
            }
            let same_model = state
                .model
                .as_deref()
                .is_some_and(|model| model.eq_ignore_ascii_case(&body_name));
            if !same_model || state.textures.is_empty() {
                continue;
            }
            let skin =
                self.cache
                    .substitute(renderer, vfs, resolver, &body_art.model, &state.textures);
            if skin.iter().any(Option::is_some) {
                if !compiled.reachable {
                    upgrade_skins = upgrade_skins.saturating_add(1);
                }
                built.push(((body, index), skin.into()));
            }
        }
        self.report.model_states = self.report.model_states.saturating_add(states);
        self.report.model_states_reachable =
            self.report.model_states_reachable.saturating_add(reachable);
        self.report.skins = self
            .report
            .skins
            .saturating_add(built.len().saturating_sub(upgrade_skins));
        self.report.upgrade_skins = self.report.upgrade_skins.saturating_add(upgrade_skins);
        if let Some(art) = self.templates.get_mut(template) {
            art.skins.extend(built);
        }
    }

    fn bind_clips(
        &mut self,
        vfs: &Vfs,
        resolver: &Resolver,
        template: usize,
        body: usize,
        body_art: &BodyArt,
    ) {
        let model = &body_art.model;
        let Some(art) = self.templates.get(template) else {
            return;
        };
        let prefix = art.animation_prefix(body).to_owned();
        let mut spelling: BTreeMap<String, String> = BTreeMap::new();
        for clip in art.states.iter().flat_map(|state| &state.clips) {
            let clip_asset = clip.asset_for(&prefix);
            spelling
                .entry(clip_asset.to_ascii_lowercase())
                .or_insert(clip_asset);
        }
        let missing: Vec<&String> = spelling
            .values()
            .filter(|asset| !self.clips.contains_key(asset.as_str()))
            .collect();
        let decoded = mint_pool::map_ordered(&missing, |asset| {
            load_animation(vfs, resolver, asset).map_err(|error| error.to_string())
        });
        for (asset, result) in missing.iter().zip(decoded) {
            self.clips.insert((*asset).clone(), result);
        }
        let Some(art) = self.templates.get_mut(template) else {
            return;
        };
        let mut reported: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for (state_index, state) in art.states.iter().enumerate() {
            for (clip_index, clip) in state.clips.iter().enumerate() {
                let clip_asset = clip.asset_for(&prefix);
                let Some(first) = spelling.get(&clip_asset.to_ascii_lowercase()) else {
                    continue;
                };
                let found = match self.clips.get(first) {
                    Some(Ok(found)) => found,
                    Some(Err(reason)) => {
                        if reported.insert(first) {
                            art.failures.push((first.clone(), reason.clone()));
                        }
                        continue;
                    }
                    None => continue,
                };
                let declared = DefaultAnimation {
                    name: clip.name_for(&prefix),
                    mode: clip.mode,
                    blend_frames: None,
                    flags: state.flags,
                    source: mint_sage_ini::AnimationSource::IdleAnimationState,
                };
                match ModelAnimation::bind(model, &declared, found.clone()) {
                    Ok(bound) => {
                        let radius =
                            mint_sage_model::gpu_skin::posed_radius(model, &body_art.gpu, &bound);
                        art.radius.insert((body, state_index, clip_index), radius);
                        art.bound
                            .insert((body, state_index, clip_index), Arc::new(bound));
                    }
                    Err(_) if body >= art.pool => {}
                    Err(refusal) => art.failures.push((clip_asset.clone(), refusal.to_string())),
                }
            }
        }
    }

    fn resolve_sound_cues(&mut self, slot: usize) {
        let Some(template) = self.templates.get(slot) else {
            return;
        };
        if template.animation_sounds.is_empty() {
            return;
        }
        let mut cues: BTreeMap<(usize, usize), Vec<(String, Vec<u32>)>> = BTreeMap::new();
        for (state_index, state) in template.states.iter().enumerate() {
            for (clip_index, clip) in state.clips.iter().enumerate() {
                for entry in &template.animation_sounds {
                    if entry
                        .animations
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&clip.name))
                    {
                        cues.entry((state_index, clip_index))
                            .or_default()
                            .push((entry.sound.clone(), entry.frames.clone()));
                    }
                }
            }
        }
        let matched = cues.values().map(Vec::len).sum();
        if let Some(template) = self.templates.get_mut(slot) {
            template.sound_cues = cues;
        }
        self.report.animation_sound_cues = self.report.animation_sound_cues.saturating_add(matched);
    }

    fn add_body(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        template: usize,
        name: &str,
    ) -> Option<(usize, Arc<BodyArt>)> {
        let art = self.templates.get(template)?;
        if let Some(index) = art
            .bodies
            .iter()
            .position(|body| body.eq_ignore_ascii_case(name))
        {
            return art
                .art
                .get(index)
                .cloned()
                .flatten()
                .map(|found| (index, found));
        }
        let index = art.bodies.len();
        self.templates
            .get_mut(template)?
            .bodies
            .push(name.to_owned());
        let loaded = self.load_body(renderer, vfs, resolver, template, index);
        self.templates.get_mut(template)?.art.push(loaded.clone());
        loaded.map(|found| (index, found))
    }

    fn rescue_clip_bodies(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        template: usize,
    ) -> Vec<Option<usize>> {
        let Some(art) = self.templates.get(template) else {
            return Vec::new();
        };
        let needed = states_needing_a_rescue_body(&art.states, art.machine.states(), |index| {
            art.bound.keys().any(|&(_, s, _)| s == index)
        });
        let mut clip_bodies = vec![None; art.states.len()];
        for (state_index, hierarchy) in needed {
            let Some((body, body_art)) =
                self.add_body(renderer, vfs, resolver, template, &hierarchy)
            else {
                continue;
            };
            self.bind_clips(vfs, resolver, template, body, &body_art);
            self.build_skins(renderer, vfs, resolver, template, body, &body_art);
            let Some(art) = self.templates.get(template) else {
                break;
            };
            if art
                .bound
                .keys()
                .any(|&(b, s, _)| b == body && s == state_index)
            {
                if let Some(slot) = clip_bodies.get_mut(state_index) {
                    *slot = Some(body);
                }
            }
        }
        clip_bodies
    }

    pub fn sync(&mut self, poses: &[UnitPose], now: f32) -> usize {
        let mut expanded = std::mem::take(&mut self.expanded);
        expand_poses(&self.templates, poses, &mut expanded);
        let created = self.sync_expanded(&expanded, now);
        self.expanded = expanded;
        created
    }

    fn sync_expanded(&mut self, poses: &[UnitPose], now: f32) -> usize {
        self.drawing_nothing.take();
        if self.drawables.len() == poses.len()
            && self
                .drawables
                .iter()
                .zip(poses)
                .all(|(d, pose)| is_drawable_for(d.id, d.generation, d.template, pose))
        {
            self.report.drawables = self.drawables.len();
            return 0;
        }
        let mut kept: Vec<UnitDrawable> = Vec::with_capacity(poses.len());
        let mut created = 0usize;
        let mut old: Vec<Option<UnitDrawable>> = std::mem::take(&mut self.drawables)
            .into_iter()
            .map(Some)
            .collect();
        let mut keys: Vec<((u32, usize, u32), usize)> = old
            .iter()
            .enumerate()
            .filter_map(|(at, d)| d.as_ref().map(|d| ((d.id, d.template, d.generation), at)))
            .collect();
        keys.sort_unstable();
        for pose in poses {
            let found = keys
                .binary_search_by_key(&(pose.id, pose.template, pose.generation), |(key, _)| *key)
                .ok()
                .and_then(|at| keys.get(at))
                .and_then(|(_, slot)| old.get_mut(*slot))
                .and_then(Option::take);
            if let Some(drawable) = found {
                kept.push(drawable);
                continue;
            }
            let Some(template) = self.templates.get(pose.template) else {
                continue;
            };
            let body = if template.pool == 0 {
                template.art.iter().position(Option::is_some).unwrap_or(0)
            } else {
                pick_body(template.pool, pose.id)
            };
            let Some(art) = template.art.get(body).cloned().flatten() else {
                continue;
            };
            created = created.saturating_add(1);
            let model = Arc::clone(&art.model);
            let parts = model.parts.len();
            let bones = if art.gpu.any() {
                art.gpu.bind.clone()
            } else {
                Vec::new()
            };
            let tire_pivots = tire_pivots_for(template.truck_tires.as_ref(), &model);
            kept.push(UnitDrawable {
                template: pose.template,
                body,
                pool_body: body,
                hidden: template.body_for(None, body).is_none(),
                body_missing: template.body_for(None, body).is_none(),
                id: pose.id,
                generation: pose.generation,
                model,
                art,
                poses: vec![PartPose::default(); parts],
                bones,
                bone_base: None,
                pending: None,
                sampled: None,
                placement: Mat4::IDENTITY,
                shroud_masked: false,
                opacity: 1.0,
                tints: tint::Tints::default(),
                drawn: false,
                present: false,
                state: None,
                last_conditions: Conditions::NONE,
                last_attack_cycle: 0,
                swing_queued: None,
                swing_repicked: false,
                swing_late_by: None,
                cut_swings: 0,
                last_cut: None,
                last_cut_at: 0.0,
                moving_until: f32::NEG_INFINITY,
                drawn_speed: 0.0,
                tire_pivots,
                tire_angle: 0.0,
                tire_angle_applied: 0.0,
                clip: 0,
                picks: 0,
                mid_transition: false,
                queue: Vec::new(),
                transition_target: None,
                model_state: None,
                companion_states: Vec::new(),
                skin: None,
                entered_at: now,
                phase: None,
                fit_cycle: None,
                advanced_at: now,
                house_color: [0.0, 0.0, 0.0, 1.0],
                hero_colours: None,
                light_set: template.light_set,
                ok_to_change_model_color: template.ok_to_change_model_color,
                ghost: None,
                casts_shadow: true,
                blend: None,
                blend_request: None,
                look_at: (body, LookKey::PLAIN),
                look: None,
                stand_in_at: None,
                stand_in_choice: None,
                script_state: None,
                parts_body: body,
                state_parts: None,
                plain_parts: Vec::new(),
                permanent_parts: Vec::new(),
                look_writes: Arc::from([]),
                floor_flags: Vec::new(),
                module_hidden: Vec::new(),
                fx_frame: None,
                fx_pending: Vec::new(),
                bone_fx_pending: Vec::new(),
                sound_pending: Vec::new(),
            });
        }
        self.report.drawables = kept.len();
        self.keys
            .rebuild(kept.iter().map(|d| (d.id, d.generation, d.template)));
        self.drawables = kept;
        created
    }

    pub fn draw(
        &mut self,
        poses: &[UnitPose],
        now: f32,
        out: &mut Vec<Instance>,
        views: &[Frustum],
    ) {
        let mut expanded = std::mem::take(&mut self.expanded);
        expand_poses(&self.templates, poses, &mut expanded);
        self.draw_expanded(&expanded, now, out, views);
        self.expanded = expanded;
    }

    fn draw_expanded(
        &mut self,
        poses: &[UnitPose],
        now: f32,
        out: &mut Vec<Instance>,
        views: &[Frustum],
    ) {
        self.drawing_nothing.take();
        let profiling = self.profiling;
        let mut bones = std::mem::take(&mut self.palette);
        bones.clear();
        let mut stats = UnitDrawStats::default();
        let instances_before = out.len();

        let started = profiling.then(std::time::Instant::now);
        let mut paired = std::mem::take(&mut self.paired);
        pair_drawables(&self.drawables, poses, &mut paired);
        let paired_at = profiling.then(std::time::Instant::now);
        let (counts, refused) = pose_all(
            &mut self.drawables,
            &paired,
            poses,
            &self.templates,
            now,
            self.threads,
            views,
            self.blending,
            self.transitions_enabled,
            profiling,
        );
        self.report.host_calls_refused.add(&refused);
        ride::carry_riders(&mut self.drawables, &self.keys, &paired, poses);
        hide_modules_by_name(&mut self.drawables, &self.templates, &self.keys);
        self.paired = paired;
        stats.drawables = counts.paired;
        stats.sampled = counts.sampled;
        stats.out_of_view = counts.out_of_view;
        stats.reused = counts.reused;
        if let Some(started) = started {
            stats.sample_s = started.elapsed().as_secs_f64();
            let seconds = |ns: u128| {
                #[allow(clippy::cast_precision_loss)]
                {
                    ns as f64 / 1.0e9
                }
            };
            stats.pair_s = started.elapsed().as_secs_f64()
                - paired_at.map_or(0.0, |at| at.elapsed().as_secs_f64());
            stats.pose_threads = counts.threads;
            stats.pose_cpu_s = seconds(counts.cpu_ns);
            stats.pose_worst_s = seconds(counts.worst_ns);
            stats.pose_sample_s = seconds(counts.sample_ns);
            stats.state_entries = counts.entries;
        }

        for drawable in &mut self.drawables {
            if drawable.fx_pending.is_empty() {
                continue;
            }
            let id = drawable.id;
            self.pending_fx
                .extend(drawable.fx_pending.drain(..).map(|list| (id, list)));
        }
        for drawable in &mut self.drawables {
            if drawable.bone_fx_pending.is_empty() {
                continue;
            }
            let id = drawable.id;
            self.pending_bone_fx
                .extend(drawable.bone_fx_pending.drain(..).map(|cue| {
                    (
                        id,
                        BoneFxFire {
                            fx_list: cue.fx_list,
                            bone: cue.bone,
                            frame_stop: cue.frame_stop,
                            slot: cue.slot,
                        },
                    )
                }));
        }
        for drawable in &mut self.drawables {
            if drawable.sound_pending.is_empty() {
                continue;
            }
            let id = drawable.id;
            let cap = self
                .templates
                .get(drawable.template)
                .and_then(|template| template.max_update_range_cap);
            self.pending_sound.extend(
                drawable
                    .sound_pending
                    .drain(..)
                    .map(|sound| (id, sound, cap)),
            );
        }

        let started = profiling.then(std::time::Instant::now);
        for drawable in &mut self.drawables {
            drawable.bone_base = None;
            if !drawable.drawn || !drawable.art.gpu.any() || drawable.bones.is_empty() {
                continue;
            }
            drawable.bone_base = u32::try_from(bones.len()).ok();
            bones.extend_from_slice(&drawable.bones);
            stats.skinned_vertices = stats.skinned_vertices.saturating_add(
                drawable
                    .model
                    .parts
                    .iter()
                    .zip(&drawable.art.gpu.parts)
                    .filter(|(_, gpu)| gpu.is_some())
                    .map(|(part, _)| {
                        part.skin
                            .as_ref()
                            .map_or(0, |s| s.bone_space.vertices.len())
                    })
                    .sum::<usize>(),
            );
        }
        stats.uploaded_bytes = bones.len().saturating_mul(48);
        self.palette = bones;
        if let Some(started) = started {
            stats.skin_s = started.elapsed().as_secs_f64();
        }

        let started = profiling.then(std::time::Instant::now);
        let mut order = std::mem::take(&mut self.order);
        order.clear();
        order.extend(
            self.drawables
                .iter()
                .enumerate()
                .filter(|(_, d)| d.drawn)
                .map(|(index, _)| index),
        );
        order.sort_by_key(|index| {
            self.drawables
                .get(*index)
                .map_or((usize::MAX, usize::MAX), |d| (d.template, d.body))
        });
        let mut group_start = 0usize;
        while group_start < order.len() {
            let key = |at: usize| {
                order
                    .get(at)
                    .and_then(|index| self.drawables.get(*index))
                    .map(|d| (d.template, d.body))
            };
            let group_key = key(group_start);
            let mut group_end = group_start.saturating_add(1);
            while group_end < order.len() && key(group_end) == group_key {
                group_end = group_end.saturating_add(1);
            }
            let group = order.get(group_start..group_end).unwrap_or(&[]);
            emit_group(&self.drawables, group, now, out);
            group_start = group_end;
        }
        self.order = order;
        stats.floors = emit_floors(&self.drawables, &self.templates, out);
        stats.model_modules = self
            .drawables
            .iter()
            .filter(|drawable| {
                drawable.drawn
                    && self
                        .templates
                        .get(drawable.template)
                        .is_some_and(|template| template.module_of.is_some())
            })
            .count();
        if let Some(started) = started {
            stats.place_s = started.elapsed().as_secs_f64();
        }
        stats.instances = out.len().saturating_sub(instances_before);
        self.stats = stats;
    }

    pub fn take_fx_events(&mut self) -> impl Iterator<Item = (u32, String)> + '_ {
        self.pending_fx.drain(..)
    }

    pub fn take_bone_fx_events(&mut self) -> impl Iterator<Item = (u32, BoneFxFire)> + '_ {
        self.pending_bone_fx.drain(..)
    }

    #[must_use]
    pub fn state_slot(&self, id: u32) -> Option<(usize, f32, u32)> {
        let drawable = self.drawable_by_id(id)?;
        let (frame, frames) = self.clip_frame(id)?;
        Some((drawable.state?, frame, frames))
    }

    pub fn take_sound_events(&mut self) -> impl Iterator<Item = (u32, String, Option<f32>)> + '_ {
        self.pending_sound.drain(..)
    }

    #[must_use]
    pub fn report_line(&self) -> String {
        let r = &self.report;
        let (under_construction, construction_clips) = self.construction_counts();
        format!(
            "units: {} template(s), {} drew a model ({} of them particles only), {} ExtraMesh bod(ies) loaded \
             ({} would not), {} drawable(s) sharing {} mesh(es) / {} vertices, \
             skinned in the vertex shader ({} skinned part(s) refused); \
             {} clip(s) bound, {} failed, {} AnimationSound cue(s) matched; \
             {} animation state(s) compiled, {} unreachable (unmodelled flag), \
             {} transition(s), {} with a script, {} with no clip; \
             {} script(s) parsed, {} failed to parse; \
             {} host call name(s) refused, {} refusal(s) total; \
             {} model state(s), {} reachable, {} texture swap(s) built, \
             {} model swap bod(ies) loaded, {} would not load, \
             {} clip-only bod(ies) rescued; \
             {} floor module(s) loaded ({} would not), {} drawn last frame; \
             {} model module(s) beside a body loaded ({} would not, {} attach bone(s) missing), \
             {} drawn last frame; \
             {} drawable(s) showing a construction state now, {} of them playing its clip",
            r.templates,
            r.templates_drawn,
            r.particle_only,
            r.bodies,
            r.bodies_unloadable,
            r.drawables,
            r.meshes,
            r.vertices,
            r.skins_refused,
            r.clips_bound,
            r.clips_failed,
            r.animation_sound_cues,
            r.machines.states,
            r.machines.unreachable,
            r.machines.transitions,
            r.machines.scripted,
            r.machines.clipless,
            r.scripts_parsed,
            r.scripts_failed,
            r.host_calls_refused.len(),
            r.host_calls_refused
                .iter()
                .map(|(_, count)| count)
                .sum::<u64>(),
            r.model_states,
            r.model_states_reachable,
            r.skins,
            r.model_swaps,
            r.model_swaps_unbuilt,
            r.clip_bodies_rescued,
            r.floors,
            r.floors_unloadable,
            self.stats.floors,
            r.model_modules,
            r.model_modules_unloadable,
            r.model_modules_bone_missing,
            self.stats.model_modules,
            under_construction,
            construction_clips,
        )
    }

    fn construction_counts(&self) -> (usize, usize) {
        let mut showing = 0usize;
        let mut with_clip = 0usize;
        for drawable in &self.drawables {
            let Some(art) = self.templates.get(drawable.template) else {
                continue;
            };
            let Some(state) = drawable.state else {
                continue;
            };
            let names_construction = art.states.get(state).is_some_and(|def| {
                def.conditions.iter().any(|token| {
                    Conditions::from_token(token)
                        .is_some_and(|bit| bit.intersects(Conditions::UNDER_CONSTRUCTION))
                })
            });
            if !names_construction {
                continue;
            }
            showing = showing.saturating_add(1);
            if art.clip(drawable.body, state, drawable.clip).is_some() {
                with_clip = with_clip.saturating_add(1);
            }
        }
        (showing, with_clip)
    }

    #[must_use]
    pub fn house_color_line(&self) -> String {
        let mut with_mesh = 0usize;
        let mut meshes = 0usize;
        let mut coloured = 0usize;
        let mut gated_off = 0usize;
        for drawable in &self.drawables {
            let count = drawable
                .model
                .parts
                .iter()
                .filter(|part| part.house_mask.is_some())
                .count();
            if count == 0 {
                continue;
            }
            with_mesh = with_mesh.saturating_add(1);
            meshes = meshes.saturating_add(count);
            if !drawable.ok_to_change_model_color {
                gated_off = gated_off.saturating_add(1);
                continue;
            }
            if drawable.house_color[0] > 0.0
                || drawable.house_color[1] > 0.0
                || drawable.house_color[2] > 0.0
            {
                coloured = coloured.saturating_add(1);
            }
        }
        format!(
            "house colour: {} of {} drawable(s) have a house-colour mesh ({meshes} mesh(es)), \
             {coloured} of them carry a player colour, {gated_off} gated off by \
             OkToChangeModelColor; {} base texture(s) mapped",
            with_mesh,
            self.drawables.len(),
            self.cache.house_color_entries()
        )
    }

    #[must_use]
    pub fn body_lines(&self) -> Vec<String> {
        let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for drawable in &self.drawables {
            let slot = counts
                .entry((drawable.template, drawable.body))
                .or_insert(0);
            *slot = slot.saturating_add(1);
        }
        let mut lines = Vec::new();
        for (template, art) in self.templates.iter().enumerate() {
            if art.pool < 2 {
                continue;
            }
            let worn: Vec<String> = art
                .bodies
                .iter()
                .take(art.pool)
                .enumerate()
                .map(|(body, name)| {
                    let count = counts.get(&(template, body)).copied().unwrap_or(0);
                    format!("{count} x {name}")
                })
                .collect();
            lines.push(format!("  {}: {}", art.name, worn.join(", ")));
        }
        lines
    }

    #[must_use]
    pub fn part_lines(&self) -> Vec<String> {
        let mut standing: BTreeMap<(usize, bool, &str, bool), usize> = BTreeMap::new();
        for drawable in &self.drawables {
            let plain = drawable.plain_parts.iter().map(|write| (false, write));
            let permanent = drawable.permanent_parts.iter().map(|write| (true, write));
            for (is_permanent, (part, shown)) in plain.chain(permanent) {
                let slot = standing
                    .entry((drawable.template, is_permanent, part.as_str(), *shown))
                    .or_insert(0);
                *slot = slot.saturating_add(1);
            }
        }
        standing
            .into_iter()
            .map(|((template, is_permanent, part, shown), count)| {
                let name = self
                    .templates
                    .get(template)
                    .map_or("?", |art| art.name.as_str());
                let verb = if shown { "shown" } else { "hidden" };
                let how = if is_permanent {
                    "permanently"
                } else {
                    "by a script"
                };
                format!("  {count} x {name}: {part} {verb} {how}")
            })
            .collect()
    }

    #[must_use]
    pub fn state_lines(&self) -> Vec<String> {
        let mut counts: BTreeMap<(usize, Option<usize>), usize> = BTreeMap::new();
        for drawable in &self.drawables {
            let slot = counts
                .entry((drawable.template, drawable.state))
                .or_insert(0);
            *slot = slot.saturating_add(1);
        }
        counts
            .into_iter()
            .map(|((template, state), count)| {
                let art = self.templates.get(template);
                let name = art.map_or("?", |art| art.name.as_str());
                let label = match (art, state) {
                    (Some(art), Some(state)) => art.states.get(state).map_or_else(
                        || "?".to_owned(),
                        |state| {
                            if state.conditions.is_empty() {
                                "IdleAnimationState".to_owned()
                            } else {
                                state.conditions.join(" ")
                            }
                        },
                    ),
                    _ => "no state selected".to_owned(),
                };
                format!("  {count} x {name}: {label}")
            })
            .chain(self.part_lines())
            .collect()
    }

    #[must_use]
    pub fn clip_frame(&self, id: u32) -> Option<(f32, u32)> {
        let drawable = self.drawable_by_id(id)?;
        let art = self.templates.get(drawable.template)?;
        let animation = art.clip(drawable.body, drawable.state?, drawable.clip)?;
        Some((drawable.fx_frame?, animation.frames))
    }

    #[must_use]
    pub fn playing_clip_names(&self, id: u32) -> Vec<String> {
        self.drawables
            .iter()
            .filter(|drawable| drawable.id == id)
            .filter_map(|drawable| {
                let art = self.templates.get(drawable.template)?;
                let clip = art.states.get(drawable.state?)?.clips.get(drawable.clip)?;
                Some(clip.name_for(art.animation_prefix(drawable.body)))
            })
            .collect()
    }

    #[must_use]
    pub fn clip_frames_named(&self, id: u32, name: &str) -> Option<u32> {
        self.drawables
            .iter()
            .filter(|drawable| drawable.id == id)
            .find_map(|drawable| {
                let art = self.templates.get(drawable.template)?;
                let prefix = art.animation_prefix(drawable.body);
                let wanted = match name.split_once(mint_sage_ini::anim_state::MODEL_TOKEN) {
                    Some((before, after)) => format!("{before}{prefix}{after}"),
                    None => name.to_owned(),
                };
                art.states.iter().enumerate().find_map(|(state, def)| {
                    def.clips.iter().enumerate().find_map(|(index, clip)| {
                        clip.name_for(prefix)
                            .eq_ignore_ascii_case(&wanted)
                            .then(|| art.clip(drawable.body, state, index))
                            .flatten()
                            .map(|animation| animation.frames)
                    })
                })
            })
    }

    #[must_use]
    pub fn state_line(&self, id: u32) -> Option<String> {
        let drawable = self.drawable_by_id(id)?;
        let mut line = self.drawable_state_line(drawable)?;
        for module in &self.drawables {
            let Some(art) = self.templates.get(module.template) else {
                continue;
            };
            if module.id != id || art.module_of != Some(drawable.template) {
                continue;
            }
            if let Some(text) = self.drawable_state_line(module) {
                line.push_str(&format!(" | {}: {text}", art.name));
            }
        }
        Some(line)
    }

    fn drawable_state_line(&self, drawable: &UnitDrawable) -> Option<String> {
        let art = self.templates.get(drawable.template)?;
        let header = |conditions: &[String]| {
            if conditions.is_empty() {
                "default".to_owned()
            } else {
                conditions.join(" ")
            }
        };
        let model_state = drawable
            .model_state
            .and_then(|state| art.model_states.get(state))
            .map_or_else(|| "none".to_owned(), |state| header(&state.conditions));
        let model = if drawable.hidden {
            "(nothing drawn)"
        } else {
            art.bodies.get(drawable.body).map_or("?", String::as_str)
        };
        let animation = drawable.state.and_then(|state| art.states.get(state));
        let clip = animation
            .and_then(|state| state.clips.get(drawable.clip))
            .map_or("none", |clip| clip.name.as_str());
        let bound = drawable
            .state
            .and_then(|state| art.clip(drawable.body, state, drawable.clip))
            .is_some();
        let cut = if drawable.cut_swings == 0 {
            String::new()
        } else {
            format!(
                "; {} swing(s) cut, the last at {:.0}% by [{}]",
                drawable.cut_swings,
                drawable.last_cut_at * 100.0,
                drawable
                    .last_cut
                    .and_then(|state| art.states.get(state))
                    .map_or_else(|| "none".to_owned(), |state| header(&state.conditions))
            )
        };
        let blend = drawable.blend.map_or(String::new(), |blend| {
            let from = art
                .states
                .get(blend.from.state)
                .and_then(|state| state.clips.get(blend.from.clip))
                .map_or("?", |clip| clip.name.as_str());
            format!(
                "; blending in from {from} over {:.0} ms{}",
                blend.duration * 1000.0,
                if blend.into.is_some() { ", queued" } else { "" }
            )
        });
        Some(format!(
            "model state [{model_state}] model {model}; animation state [{}] clip {clip}{}{cut}{blend}",
            animation.map_or_else(|| "none".to_owned(), |state| header(&state.conditions)),
            if bound || clip == "none" {
                ""
            } else {
                " (unbound)"
            }
        ))
    }
}

#[must_use]
pub fn resolve_state_bodies(
    model_states: &[ModelStateDef],
    machine: &ModelStateMachine,
    bodies: &mut Vec<String>,
) -> Vec<StateBody> {
    let pool = bodies.len();
    model_states
        .iter()
        .zip(machine.states())
        .map(|(state, compiled)| match state.model.as_deref() {
            _ if !compiled.selectable => StateBody::Unloaded,
            None => StateBody::Nothing,
            Some(name)
                if bodies
                    .iter()
                    .take(pool)
                    .any(|body| body.eq_ignore_ascii_case(name)) =>
            {
                StateBody::Pool
            }
            Some(name) => {
                let at = bodies
                    .iter()
                    .position(|body| body.eq_ignore_ascii_case(name))
                    .unwrap_or_else(|| {
                        bodies.push(name.to_owned());
                        bodies.len().saturating_sub(1)
                    });
                StateBody::Body(at)
            }
        })
        .collect()
}

fn conditions_of(tokens: &[String], extras: &mut ExtraTokens) -> Conditions {
    tokens.iter().fold(Conditions::NONE, |acc, token| {
        acc.union(
            Conditions::from_token(token)
                .or_else(|| extras.intern(token))
                .unwrap_or(Conditions::NONE),
        )
    })
}

fn draw_module_has_attachments(module: &DrawModuleView<'_>) -> bool {
    module
        .animation_states
        .iter()
        .any(|state| !state.particles.is_empty())
        || module
            .model_states
            .iter()
            .any(|state| !state.particles.is_empty() || state.decal.is_some())
}

#[must_use]
fn states_needing_a_rescue_body(
    states: &[AnimationStateDef],
    compiled: &[CompiledState],
    is_bound: impl Fn(usize) -> bool,
) -> Vec<(usize, String)> {
    compiled
        .iter()
        .filter(|state| state.selectable)
        .filter_map(|state| {
            let index = state.source;
            let clips = &states.get(index)?.clips;
            if is_bound(index) {
                return None;
            }
            clips
                .iter()
                .find_map(AnimationClip::hierarchy)
                .map(|hierarchy| (index, hierarchy.to_owned()))
        })
        .collect()
}

#[must_use]
pub fn ghost_tint(tint: [f32; 4], ghost: GhostLook) -> [f32; 4] {
    let unit = |v: f32| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            1.0
        }
    };
    [
        tint[0] * unit(ghost.tint[0]),
        tint[1] * unit(ghost.tint[1]),
        tint[2] * unit(ghost.tint[2]),
        tint[3] * unit(ghost.opacity),
    ]
}

fn whole_look(
    drawable: &UnitDrawable,
    blend: mint_render::Blend,
    mut tint: [f32; 4],
) -> (mint_render::Blend, [f32; 4], [f32; 3]) {
    let whole = match drawable.ghost {
        Some(ghost) => Some(GhostLook {
            opacity: ghost.opacity * drawable.opacity,
            ..ghost
        }),
        None if drawable.opacity < 1.0 => Some(GhostLook {
            opacity: drawable.opacity,
            tint: [1.0; 3],
            light_add: [0.0; 3],
            state_particles: true,
        }),
        None => None,
    };
    let blend = match whole {
        Some(ghost) => {
            tint = ghost_tint(tint, ghost);
            match blend {
                mint_render::Blend::Opaque => mint_render::Blend::AlphaBlendWritesDepth,
                mint_render::Blend::Additive | mint_render::Blend::AlphaAdditive => {
                    let dim = tint[3];
                    tint = [tint[0] * dim, tint[1] * dim, tint[2] * dim, tint[3]];
                    blend
                }
                other => other,
            }
        }
        None => blend,
    };
    let light_add = whole.map_or([0.0; 3], |ghost| ghost.light_add);
    (blend, tint, light_add)
}

fn emit_group(drawables: &[UnitDrawable], group: &[usize], now: f32, out: &mut Vec<Instance>) {
    let Some(first) = group.first().and_then(|index| drawables.get(*index)) else {
        return;
    };
    let parts = first.model.parts.len();
    out.reserve(parts.saturating_mul(group.len()));
    let light_adds: Vec<Option<[f32; 3]>> = group
        .iter()
        .filter_map(|at| drawables.get(*at))
        .map(|drawable| drawable.tints.at(now))
        .collect();
    for index in 0..parts {
        for (drawable, light_add) in group
            .iter()
            .filter_map(|at| drawables.get(*at))
            .zip(&light_adds)
        {
            let (Some(part), Some(pose)) = (
                drawable.model.parts.get(index),
                drawable
                    .poses
                    .get(index)
                    .copied()
                    .or(Some(PartPose::default())),
            ) else {
                continue;
            };
            let house_layer = drawable.house_layer();
            let skinned = drawable
                .art
                .gpu
                .parts
                .get(index)
                .and_then(Option::as_ref)
                .filter(|_| drawable.bone_base.is_some());
            let local = pose.transform.unwrap_or(part.instance.transform);
            let base = &part.instance;
            let (faded_blend, tint) = mint_sage_model::faded(base.blend, base.tint, pose.opacity);
            let (blend, tint, ghost_light_add) = whole_look(drawable, faded_blend, tint);
            let light_add = light_add.unwrap_or(base.light_add);
            let light_add = [
                light_add[0] + ghost_light_add[0],
                light_add[1] + ghost_light_add[1],
                light_add[2] + ghost_light_add[2],
            ];
            let look = drawable.look.as_deref();
            let texture = drawable
                .skin
                .as_ref()
                .and_then(|skin| skin.get(index).copied().flatten())
                .or_else(|| {
                    look.and_then(|look| look.skin.as_ref())
                        .and_then(|skin| skin.get(index).copied().flatten())
                })
                .or(base.texture);
            let hidden_by_override = drawable
                .state_parts
                .as_deref()
                .and_then(|parts| parts.get(index).copied().flatten())
                == Some(false);
            let uv = uv_override_for_part(part.uv_mapper.as_ref(), now);
            let mut instance = Instance {
                blend,
                texture,
                mesh: skinned.map_or(base.mesh, |gpu| gpu.mesh),
                bone_base: skinned.and(drawable.bone_base),
                transform: mint_sage_model::cache::placed_transform(drawable.placement, local),
                bounds: pose.bounds.or(base.bounds),
                tint,
                light_add,
                light_set: mint_sage_model::cache::part_light_set(
                    drawable.light_set,
                    mint_sage_model::cache::instance_effect_lit(base),
                ),
                visibility_mask: drawable.shroud_masked,
                visible: base.visible && pose.visible && !hidden_by_override,
                casts_shadow: base.casts_shadow && drawable.casts_shadow,
                ..*base
            };
            if let (Some(mask), Some((colour, extra))) = (part.house_mask, house_layer) {
                instance.texture2 = Some(mask);
                instance.detail_combine = DetailCombine::HouseColor;
                instance.detail_alpha_combine = DetailAlphaCombine::Disable;
                instance.house_color = colour;
                instance.house_color_extra = extra;
            }
            if let Some(((uv_linear, uv_offset), (uv2_linear, uv2_offset), bump)) = uv {
                instance.uv_linear = uv_linear;
                instance.uv_offset = uv_offset;
                instance.uv2_linear = uv2_linear;
                instance.uv2_offset = uv2_offset;
                if instance.bump_offset.is_some() {
                    instance.bump_offset = Some(bump);
                }
            }
            out.push(instance);
        }
    }
}

struct FloorSpec {
    model: String,
    hide_if: Conditions,
    weather: Vec<(NuggetWeather, String)>,
    force_to_back: bool,
    tag: Option<String>,
    start_hidden: bool,
}

fn hide_modules_by_name(
    drawables: &mut [UnitDrawable],
    templates: &[UnitTemplate],
    keys: &index::DrawableKeys,
) {
    let mut hidden = Vec::new();
    for drawable in drawables.iter() {
        if !drawable.module_hidden.contains(&true) {
            continue;
        }
        let Some(template) = templates.get(drawable.template) else {
            continue;
        };
        for (module, _) in template
            .modules
            .iter()
            .zip(&drawable.module_hidden)
            .filter(|(_, hidden)| **hidden)
        {
            hidden.extend(keys.by_key(drawable.id, drawable.generation, *module));
        }
    }
    for at in hidden {
        if let Some(module) = drawables.get_mut(at) {
            module.drawn = false;
        }
    }
}

fn expand_poses(templates: &[UnitTemplate], poses: &[UnitPose], out: &mut Vec<UnitPose>) {
    out.clear();
    for pose in poses {
        out.push(*pose);
        let Some(template) = templates.get(pose.template) else {
            continue;
        };
        for &module in &template.modules {
            let attach = templates.get(module).and_then(|module| module.attach);
            out.push(UnitPose {
                template: module,
                placement: attach.map_or(pose.placement, |offset| {
                    pose.placement.mul_mat4(Mat4::from_translation(offset))
                }),
                ..*pose
            });
        }
    }
}

fn house_colour_layer(list: &[[f32; 4]]) -> Option<([f32; 4], [[f32; 4]; 2])> {
    const NONE: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    match *list {
        [one] if one[0] > 0.0 || one[1] > 0.0 || one[2] > 0.0 => Some((one, [NONE; 2])),
        [hair, skin, paint] => Some((hair, [skin, paint])),
        _ => None,
    }
}

struct ModuleSource {
    model: Option<String>,
    alternates: Vec<String>,
    states: Vec<AnimationStateDef>,
    model_states: Vec<ModelStateDef>,
    hide_if: Vec<String>,
    ok_to_change_model_color: bool,
    truck_tires: Option<mint_sage_ini::TruckTires>,
}

pub fn weather_skin(
    cache: &mut ModelCache,
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    model: &CachedModel,
    texture: &str,
) -> Option<Skin> {
    let swaps: Vec<(String, String)> = model
        .parts
        .iter()
        .filter_map(|part| part.texture_name.clone())
        .map(|own| (own, texture.to_owned()))
        .collect();
    let skin = cache.substitute(renderer, vfs, resolver, model, &swaps);
    skin.iter().any(Option::is_some).then(|| skin.into())
}

fn weather_of(conditions: Conditions) -> NuggetWeather {
    if conditions.intersects(Conditions::SNOW) {
        NuggetWeather::Snowy
    } else {
        NuggetWeather::Normal
    }
}

const FLOOR_DEPTH_TEST: DepthTest = DepthTest::NearerOrEqual;

const FLOOR_BLEND: mint_render::Blend = mint_render::Blend::AlphaBlend;

fn floors_wanted(
    drawables: &[UnitDrawable],
    templates: &[UnitTemplate],
) -> Vec<(bool, usize, usize, usize)> {
    let mut wanted: Vec<(bool, usize, usize, usize)> = Vec::new();
    for (at, drawable) in drawables.iter().enumerate() {
        if !drawable.present {
            continue;
        }
        let Some(art) = templates.get(drawable.template) else {
            continue;
        };
        for (floor, module) in art.floors.iter().enumerate() {
            let flags = drawable.floor_flags.get(floor).copied().unwrap_or_default();
            let drawn = !flags.hidden
                && !flags.lock
                && (flags.changed || !module.start_hidden)
                && !drawable.last_conditions.intersects(module.hide_if);
            if drawn {
                wanted.push((module.force_to_back, drawable.template, floor, at));
            }
        }
    }
    wanted.sort_unstable();
    wanted
}

fn emit_floors(
    drawables: &[UnitDrawable],
    templates: &[UnitTemplate],
    out: &mut Vec<Instance>,
) -> usize {
    let wanted = floors_wanted(drawables, templates);
    for (_, template, floor, at) in &wanted {
        let (Some(module), Some(drawable)) = (
            templates
                .get(*template)
                .and_then(|art| art.floors.get(*floor)),
            drawables.get(*at),
        ) else {
            continue;
        };
        let weather = weather_of(drawable.last_conditions);
        let skin = module
            .weather
            .iter()
            .find(|(of, _)| *of == weather)
            .map(|(_, skin)| skin);
        for (index, part) in module.model.parts.iter().enumerate() {
            let base = &part.instance;
            let (blend, tint, _ghost_light_add) = whole_look(drawable, FLOOR_BLEND, base.tint);
            out.push(Instance {
                blend,
                tint,
                texture: skin
                    .and_then(|skin| skin.get(index).copied().flatten())
                    .or(base.texture),
                transform: mint_sage_model::cache::placed_transform(
                    drawable.placement,
                    base.transform,
                ),
                light_set: mint_sage_model::cache::part_light_set(
                    drawable.light_set,
                    mint_sage_model::cache::instance_effect_lit(base),
                ),
                visibility_mask: drawable.shroud_masked,
                surface_overlay: true,
                depth_test: FLOOR_DEPTH_TEST,
                ..*base
            });
        }
    }
    wanted.len()
}

fn uv_override_for_part(
    mapper: Option<&mint_sage_model::convert::UvMapperState>,
    now: f32,
) -> Option<mint_sage_model::convert::SampledUvTransforms> {
    mapper.map(|state| mint_sage_model::convert::sample_uv_transforms_for(state, now))
}

#[must_use]
pub fn template_draws_a_model(index: &ObjectIndex, name: &str) -> bool {
    matches!(index.lookup(name), Template::Model(_))
}

#[must_use]
pub const fn is_drawable_for(id: u32, generation: u32, template: usize, pose: &UnitPose) -> bool {
    id == pose.id && generation == pose.generation && template == pose.template
}

#[must_use]
pub const fn drawable_id(entity_index: u32) -> u32 {
    entity_index
}

#[must_use]
pub fn drawable_variant(id: u32) -> u64 {
    variant(id, 0, 0)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests;
