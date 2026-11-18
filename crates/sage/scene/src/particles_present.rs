use std::collections::{BTreeMap, HashMap, HashSet};

use mint_render::{Camera, Frustum, Instance, Mat4, Renderer, TextureHandle, Vec3};
use mint_sage_ini::{
    water_gate_admits, weather_gate_admits, CameraShakerVolume, FxAtBonePos, FxListIndex,
    FxParticleNugget, NuggetWeather, ParticleIndex, ParticleSystemDef, Persist,
};
use mint_sage_map::{HeightMapData, Object, MAP_XY_FACTOR};
use mint_sage_model::{load_texture, missing_texture, ModelCache, ObjectIndex, Resolver};
use mint_sage_particle::{
    Collision, CollisionEvent, DrawShape, FxFrame, FxSpawn, Ground, ParticleWorld, SystemId,
    TemplateHandle, MAX_COMPOUND_DEPTH,
};
use mint_vfs::Vfs;

use crate::objects_present::ObjectScene;
use crate::particles_models::{ParticleModel, PoseScratch};
use crate::units_present::{UnitDrawable, UnitScene};

pub struct MapGround<'a> {
    height_map: &'a HeightMapData,
    vertical_scale: f32,
}

impl<'a> MapGround<'a> {
    #[must_use]
    pub const fn new(height_map: &'a HeightMapData, vertical_scale: f32) -> Self {
        Self {
            height_map,
            vertical_scale,
        }
    }

    fn sample(&self, x: i64, y: i64) -> Option<f32> {
        let x = u32::try_from(x).ok()?;
        let y = u32::try_from(y).ok()?;
        Some(f32::from(self.height_map.sample(x, y)?) * self.vertical_scale)
    }
}

impl Ground for MapGround<'_> {
    fn height_at(&self, x: f32, y: f32) -> Option<f32> {
        let border = f32::from(u16::try_from(self.height_map.border_size).unwrap_or(u16::MAX));
        let cx = x / MAP_XY_FACTOR + border;
        let cy = y / MAP_XY_FACTOR + border;
        if !cx.is_finite() || !cy.is_finite() {
            return None;
        }
        let (fx, fy) = (cx.floor(), cy.floor());
        #[allow(clippy::cast_possible_truncation)]
        let (ix, iy) = (fx as i64, fy as i64);
        let (tx, ty) = (cx - fx, cy - fy);
        let z00 = self.sample(ix, iy)?;
        let z10 = self.sample(ix.saturating_add(1), iy)?;
        let z01 = self.sample(ix, iy.saturating_add(1))?;
        let z11 = self.sample(ix.saturating_add(1), iy.saturating_add(1))?;
        let top = z00 + (z10 - z00) * tx;
        let bottom = z01 + (z11 - z01) * tx;
        Some(top + (bottom - top) * ty)
    }
}

const MAX_FX_LIST_DEPTH: u32 = 4;

#[derive(Debug, Clone, Default)]
pub struct FireReport {
    pub systems: usize,
    pub at_bone_pos: usize,
    pub bone_found: usize,
    pub bone_missing: usize,
    pub nested_list_missing: usize,
    pub camera_shakes: Vec<CameraShakerVolume>,
    pub dynamic_decals: Vec<crate::dynamic_decal::FiredDecal>,
    pub create_bone: usize,
    pub create_bone_found: usize,
    pub target_bone: usize,
    pub target_aimed: usize,
    pub target_bone_found: usize,
    pub bone_leaf_systems: Vec<SystemId>,
}

struct NuggetSpawn {
    spawn: FxSpawn,
    system: String,
    create_bone: Option<String>,
    target_bone: Option<String>,
    on_land: bool,
    on_water: bool,
    weather: Option<NuggetWeather>,
    use_target_offset: bool,
    target_offset: Vec3,
}

fn nugget_point(list_point: Vec3, bone: Option<Mat4>) -> Vec3 {
    bone.map_or(list_point, |bone| bone.transform_point(Vec3::ZERO))
}

fn target_offset_point(
    target_bone_frame: Option<Mat4>,
    caller_frame: Option<Mat4>,
    offset: Vec3,
) -> Vec3 {
    target_bone_frame
        .or(caller_frame)
        .unwrap_or(Mat4::IDENTITY)
        .transform_point(offset)
}

fn target_bone_point(victim_point: Vec3, bone: Option<Mat4>) -> Vec3 {
    bone.map_or(victim_point, |bone| bone.transform_point(Vec3::ZERO))
}

fn nugget_aims(
    target_bone: Option<&str>,
    set_target_matrix: bool,
    secondary: Option<Vec3>,
) -> bool {
    (set_target_matrix || target_bone.is_some()) && secondary.is_some()
}

fn diag_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("MINT_ROTWK_DIAG").is_some())
}

fn report_nugget_anchor(
    list: &str,
    nugget: &NuggetSpawn,
    list_frame: FxFrame,
    fired: FxFrame,
    caller: Option<&UnitDrawable>,
    found: bool,
) {
    let point = |at: Vec3| format!("({:.0},{:.0},{:.0})", at.x, at.y, at.z);
    let drawable = caller.map_or_else(|| "none".to_owned(), |drawable| drawable.id.to_string());
    let target = fired.secondary.map_or_else(
        || "no secondary point".to_owned(),
        |secondary| {
            format!(
                "target {} -> {}",
                nugget.target_bone.as_deref().unwrap_or("none"),
                point(secondary)
            )
        },
    );
    println!(
        "fx bone: {list} {} create {} on drawable {drawable} {} -> {}, list point {}; {target}",
        nugget.system,
        nugget.create_bone.as_deref().unwrap_or("none"),
        if found { "found" } else { "missing" },
        point(fired.at),
        point(list_frame.at),
    );
}

enum AttachTarget {
    MapBone {
        group: usize,
        placement: usize,
        pivot: Option<usize>,
    },
    MapOrigin {
        placement: usize,
    },
    UnitBone {
        drawable: u32,
        generation: u32,
        template: usize,
        bone: Option<String>,
        owner: usize,
        search: BoneSearch,
    },
    UnitOffset {
        drawable: u32,
        generation: u32,
        template: usize,
        offset: Vec3,
    },
}

const IN_VIEW_MARGIN: f32 = 25.0;

const SHROUD_CLEAR_GRACE_SECONDS: f32 = 2.0;

struct Attachment {
    system: SystemId,
    target: AttachTarget,
    local: Mat4,
    follow_bone: bool,
    module_flag: bool,
    last_clear: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BoneReport {
    pub placements: usize,
    pub spawned: usize,
    pub dangling: usize,
}

pub struct ParticleScene {
    world: ParticleWorld,
    index: ParticleIndex,
    lists: FxListIndex,
    models: ModelCache,
    textures: BTreeMap<String, Option<TextureHandle>>,
    templates: BTreeMap<String, TemplateHandle>,
    attachments: Vec<Attachment>,
    bone_frame: BoneFrameMode,
    bone_lookup: BoneLookup,
    hidden_systems: HashSet<String>,
    particle_models: BTreeMap<TemplateHandle, ParticleModel>,
    pose_scratch: PoseScratch,
    bones: Vec<Mat4>,
    clock: f32,
    attachments_start_held: bool,
    follow_ups: Vec<String>,
    collisions: Vec<Collision>,
    collision_decals: Vec<crate::dynamic_decal::FiredDecal>,
}

const MAX_PENDING_COLLISION_DECALS: usize = 1024;

fn heading_matrix([x, y]: [f32; 2]) -> Mat4 {
    let length = (x * x + y * y).sqrt();
    if !(length.is_finite() && length > 0.0) {
        return Mat4::IDENTITY;
    }
    let (c, s) = (x / length, y / length);
    Mat4::from_cols([
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoneLookup {
    #[default]
    DrawableWide,
    ModuleLocal,
}

impl BoneLookup {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::DrawableWide => Self::ModuleLocal,
            Self::ModuleLocal => Self::DrawableWide,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DrawableWide => "drawable-wide (first module declaring the bone)",
            Self::ModuleLocal => {
                "module-local (own module's skeleton unless ParticleBonesCheckDrawable)"
            }
        }
    }

    #[must_use]
    pub const fn searches_drawable(self, check_drawable: bool) -> bool {
        check_drawable || matches!(self, Self::DrawableWide)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoneSearch {
    OwnModule,
    Drawable { check_drawable: bool },
}

pub(crate) fn bone_source<'a>(
    units: &'a UnitScene,
    drawable: &'a UnitDrawable,
    bone: Option<&str>,
    wide: bool,
) -> &'a UnitDrawable {
    let Some(bone) = bone.filter(|_| wide) else {
        return drawable;
    };
    units
        .module_drawables(drawable)
        .find(|module| module.model.pivot_named(bone).is_some())
        .unwrap_or(drawable)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoneFrameMode {
    Bone,
    #[default]
    Object,
    World,
}

impl BoneFrameMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Bone => Self::Object,
            Self::Object => Self::World,
            Self::World => Self::Bone,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bone => "bone",
            Self::Object => "object",
            Self::World => "world",
        }
    }
}

impl ParticleScene {
    #[must_use]
    pub fn new(index: ParticleIndex, lists: FxListIndex) -> Self {
        Self {
            world: ParticleWorld::default(),
            index,
            lists,
            models: ModelCache::new(),
            textures: BTreeMap::new(),
            templates: BTreeMap::new(),
            attachments: Vec::new(),
            bone_frame: BoneFrameMode::default(),
            bone_lookup: BoneLookup::default(),
            hidden_systems: HashSet::new(),
            particle_models: BTreeMap::new(),
            pose_scratch: PoseScratch::default(),
            bones: Vec::new(),
            clock: 0.0,
            attachments_start_held: false,
            follow_ups: Vec::new(),
            collisions: Vec::new(),
            collision_decals: Vec::new(),
        }
    }

    pub fn take_collision_decals(&mut self) -> Vec<crate::dynamic_decal::FiredDecal> {
        std::mem::take(&mut self.collision_decals)
    }

    pub fn hold_attachments_from_birth(&mut self) {
        self.attachments_start_held = true;
    }

    pub fn set_system_hidden(&mut self, name: &str, hidden: bool) {
        let key = name.to_ascii_lowercase();
        if hidden {
            self.hidden_systems.insert(key);
        } else {
            self.hidden_systems.remove(&key);
        }
        self.sync_hidden();
    }

    pub fn clear_hidden(&mut self) {
        self.hidden_systems.clear();
        self.sync_hidden();
    }

    #[must_use]
    pub fn is_system_hidden(&self, name: &str) -> bool {
        self.hidden_systems.contains(&name.to_ascii_lowercase())
    }

    pub fn set_solo_system(&mut self, name: Option<&str>) -> bool {
        let Some(name) = name else {
            self.world.set_solo(None);
            return true;
        };
        let Some(handle) = self.templates.get(&name.to_ascii_lowercase()).copied() else {
            return false;
        };
        self.world.set_solo(Some(handle));
        true
    }

    fn sync_hidden(&mut self) {
        let handles: Vec<TemplateHandle> = self
            .templates
            .iter()
            .filter(|(name, _)| self.hidden_systems.contains(*name))
            .map(|(_, handle)| *handle)
            .collect();
        self.world.set_hidden(handles);
    }

    pub fn set_budget_numbers(&mut self, max_particles: Option<usize>, skip_mask: u32) {
        let mut budget = self.world.budget();
        budget.max_particles = max_particles;
        budget.skip_mask = skip_mask;
        self.world.set_budget(budget);
    }

    fn attach(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        frame: Mat4,
    ) -> Option<SystemId> {
        let handle = self.template(renderer, vfs, resolver, name)?;
        let origin = frame.transform_point(Vec3::new(0.0, 0.0, 0.0));
        self.sync_hidden();
        let id = self.world.spawn(handle, origin)?;
        self.world.set_frame(id, frame);
        Some(id)
    }

    fn track(
        &mut self,
        system: SystemId,
        target: AttachTarget,
        local: Mat4,
        follow_bone: bool,
        module_flag: bool,
    ) {
        if self.attachments_start_held {
            self.world.set_suppressed(system, true);
        }
        self.attachments.push(Attachment {
            system,
            target,
            local,
            follow_bone,
            module_flag,
            last_clear: None,
        });
    }

    pub fn hold_unseen_attachments(
        &mut self,
        view: &Frustum,
        units: &UnitScene,
        proxies: &[mint_sage_terrain::ObjectProxy],
        obscured: &dyn Fn(Vec3) -> bool,
        now: f32,
    ) {
        let origin = Vec3::new(0.0, 0.0, 0.0);
        for attachment in &mut self.attachments {
            let centre = match &attachment.target {
                AttachTarget::MapBone { placement, .. } | AttachTarget::MapOrigin { placement } => {
                    proxies
                        .get(*placement)
                        .map(|proxy| proxy.placement.transform_point(origin))
                }
                AttachTarget::UnitBone {
                    drawable,
                    generation,
                    template,
                    ..
                }
                | AttachTarget::UnitOffset {
                    drawable,
                    generation,
                    template,
                    ..
                } => units
                    .drawable_by_key(*drawable, *generation, *template)
                    .map(|drawable| drawable.placement().transform_point(origin)),
            };
            if let Some(centre) = centre {
                let shrouded = if obscured(centre) {
                    attachment
                        .last_clear
                        .is_none_or(|clear| now - clear >= SHROUD_CLEAR_GRACE_SECONDS)
                } else {
                    attachment.last_clear = Some(now);
                    false
                };
                let seen = view.sphere_within_sides(centre, IN_VIEW_MARGIN);
                self.world
                    .set_suppressed(attachment.system, !seen || shrouded);
            }
        }
    }

    fn update_unit_attachments(&mut self, units: &UnitScene) {
        let lookup = self.bone_lookup;
        let mut attachments = std::mem::take(&mut self.attachments);
        attachments.retain(|attachment| {
            let frame = match &attachment.target {
                AttachTarget::MapBone { .. } | AttachTarget::MapOrigin { .. } => return true,
                AttachTarget::UnitBone {
                    drawable,
                    generation,
                    template,
                    bone,
                    owner,
                    search,
                } => {
                    let Some(drawable) = units.drawable_by_key(*drawable, *generation, *template)
                    else {
                        return false;
                    };
                    self.world
                        .set_swirl_bones(attachment.system, &unit_swirl_bones(units, drawable));
                    let reposes =
                        reposes_every_frame(attachment.module_flag, attachment.follow_bone);
                    let source = match search {
                        BoneSearch::Drawable { check_drawable } if reposes => bone_source(
                            units,
                            drawable,
                            bone.as_deref(),
                            lookup.searches_drawable(*check_drawable),
                        ),
                        BoneSearch::Drawable { .. } => units
                            .drawable_by_key(drawable.id, drawable.generation, *owner)
                            .unwrap_or(drawable),
                        BoneSearch::OwnModule => drawable,
                    };
                    let local = if reposes {
                        unit_bone_follow(source, bone.as_deref())
                    } else {
                        attachment.local
                    };
                    source.placement().mul_mat4(local)
                }
                AttachTarget::UnitOffset {
                    drawable,
                    generation,
                    template,
                    offset,
                } => {
                    let Some(drawable) = units.drawable_by_key(*drawable, *generation, *template)
                    else {
                        return false;
                    };
                    self.world
                        .set_swirl_bones(attachment.system, &unit_swirl_bones(units, drawable));
                    drawable
                        .placement()
                        .mul_mat4(Mat4::from_translation(*offset))
                }
            };
            self.world.set_frame(attachment.system, frame)
        });
        self.attachments = attachments;
    }

    #[must_use]
    pub fn flipbook(&self) -> mint_sage_particle::FlipbookMode {
        self.world.flipbook()
    }

    pub fn set_flipbook(&mut self, flipbook: mint_sage_particle::FlipbookMode) {
        self.world.set_flipbook(flipbook);
    }

    pub fn cycle_solo(&mut self) -> Option<(String, usize, usize, usize)> {
        let live = self.world.live_templates();
        let next = match self.world.solo() {
            None => live.first(),
            Some(current) => live
                .iter()
                .position(|(template, _)| *template == current)
                .and_then(|index| live.get(index.saturating_add(1)))
                .or_else(|| live.iter().find(|(template, _)| *template > current)),
        };
        self.world.set_solo(next.map(|(template, _)| *template));
        let (template, count) = next?;
        let rung = live
            .iter()
            .position(|(t, _)| t == template)
            .map_or(0, |i| i.saturating_add(1));
        let name = self
            .world
            .definition(*template)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        Some((name, *count, rung, live.len()))
    }

    #[must_use]
    pub fn live_systems_by_name(&self) -> Vec<(String, usize)> {
        self.world
            .live_templates()
            .into_iter()
            .filter_map(|(template, count)| {
                Some((self.world.definition(template)?.name.clone(), count))
            })
            .collect()
    }

    #[must_use]
    pub fn live_centroids_by_name(&self) -> Vec<(String, usize, Vec3)> {
        self.world
            .live_centroids()
            .into_iter()
            .filter_map(|(template, count, mean)| {
                Some((self.world.definition(template)?.name.clone(), count, mean))
            })
            .collect()
    }

    #[must_use]
    pub fn live_particles(&self) -> usize {
        self.world.live_particles()
    }

    #[must_use]
    pub fn live_systems(&self) -> usize {
        self.world.live_systems()
    }

    #[must_use]
    pub fn flat_size(&self) -> mint_sage_particle::FlatSize {
        self.world.flat_size()
    }

    pub fn set_flat_size(&mut self, size: mint_sage_particle::FlatSize) {
        self.world.set_flat_size(size);
    }

    #[must_use]
    pub fn smudge_jitter(&self) -> mint_sage_particle::SmudgeJitter {
        self.world.smudge_jitter()
    }

    pub fn set_smudge_jitter(&mut self, jitter: mint_sage_particle::SmudgeJitter) {
        self.world.set_smudge_jitter(jitter);
    }

    #[must_use]
    pub fn wind(&self) -> mint_sage_particle::WindMode {
        self.world.wind()
    }

    pub fn set_wind(&mut self, wind: mint_sage_particle::WindMode) -> usize {
        self.world.set_wind(wind);
        self.world.live_with_wind()
    }

    #[must_use]
    pub fn attached_frame(&self) -> mint_sage_particle::AttachedFrame {
        self.world.attached_frame()
    }

    #[must_use]
    pub const fn bone_lookup(&self) -> BoneLookup {
        self.bone_lookup
    }

    pub fn set_bone_lookup(&mut self, lookup: BoneLookup) {
        self.bone_lookup = lookup;
    }

    pub fn set_attached_frame(&mut self, frame: mint_sage_particle::AttachedFrame) {
        self.world.set_attached_frame(frame);
    }

    #[must_use]
    pub const fn bone_frame(&self) -> BoneFrameMode {
        self.bone_frame
    }

    pub fn set_bone_frame(&mut self, mode: BoneFrameMode) {
        self.bone_frame = mode;
    }

    #[must_use]
    pub fn beam_length(&self) -> mint_sage_particle::BeamLengthMode {
        self.world.beam_length()
    }

    pub fn set_beam_length(&mut self, mode: mint_sage_particle::BeamLengthMode) {
        self.world.set_beam_length(mode);
    }

    #[must_use]
    pub fn additive_alpha(&self) -> mint_sage_particle::AdditiveAlpha {
        self.world.additive_alpha()
    }

    pub fn set_additive_alpha(&mut self, mode: mint_sage_particle::AdditiveAlpha) {
        self.world.set_additive_alpha(mode);
    }

    #[must_use]
    pub fn budget_report(&self) -> mint_sage_particle::BudgetReport {
        self.world.refusals()
    }

    pub fn advance(&mut self, seconds: f32, ground: &dyn Ground) {
        if seconds.is_finite() && seconds > 0.0 {
            self.clock += seconds;
        }
        self.world.advance(seconds, ground);
        self.world.take_collisions(&mut self.collisions);
        for collision in std::mem::take(&mut self.collisions) {
            let Some(list) = usize::try_from(collision.follow_up)
                .ok()
                .and_then(|token| self.follow_ups.get(token))
                .and_then(|name| self.lists.get(name))
            else {
                continue;
            };
            let matrix = collision.heading.map(heading_matrix);
            for decal in list
                .nuggets
                .iter()
                .filter_map(|nugget| nugget.dynamic_decal.as_ref())
            {
                if self.collision_decals.len() < MAX_PENDING_COLLISION_DECALS {
                    self.collision_decals
                        .push(crate::dynamic_decal::FiredDecal::place(
                            decal,
                            collision.at,
                            matrix,
                        ));
                }
            }
        }
    }

    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera,
        out: &mut Vec<Instance>,
        palette_base: usize,
    ) {
        self.bones.clear();
        if let Err(error) = self.world.draw(renderer, camera, out) {
            crate::once::report_once(&format!("  particles: {error}"));
        }
        for draw in self.world.model_draws() {
            if let Some(model) = self.particle_models.get(&draw.template) {
                if model.needs_frame() {
                    model.animate(
                        draw,
                        out,
                        self.clock,
                        &mut self.pose_scratch,
                        &mut self.bones,
                        palette_base,
                    );
                }
            }
        }
    }

    #[must_use]
    pub fn bones(&self) -> &[Mat4] {
        &self.bones
    }

    fn template(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
    ) -> Option<TemplateHandle> {
        self.template_at(renderer, vfs, resolver, name, 0)
    }

    fn template_at(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        depth: u32,
    ) -> Option<TemplateHandle> {
        let key = name.to_ascii_lowercase();
        if let Some(handle) = self.templates.get(&key) {
            return Some(*handle);
        }
        let definition = self.index.get(name)?.clone();
        let texture = self.texture_for(renderer, vfs, resolver, &definition);
        let (parts, bound) = self.model_parts(renderer, vfs, resolver, &definition);
        let handle = self.world.register_model(&definition, texture, parts);
        if let Some(bound) = bound {
            self.particle_models.insert(handle, bound);
        }
        self.templates.insert(key, handle);
        if depth >= MAX_COMPOUND_DEPTH {
            return Some(handle);
        }
        let slave = definition.slave_system.as_deref().and_then(|slave| {
            self.template_at(renderer, vfs, resolver, slave, depth.saturating_add(1))
        });
        let attached = definition
            .per_particle_system
            .as_deref()
            .and_then(|attached| {
                self.template_at(renderer, vfs, resolver, attached, depth.saturating_add(1))
            });
        let event = definition.event.as_ref().map(|event| CollisionEvent {
            fires: self.list_spawns(renderer, vfs, resolver, &event.fx, depth.saturating_add(1)),
            height_offset: event.height_offset.midpoint(),
            kill_after: event.kill_after_event,
            follow_up: self.follow_up(&event.fx),
            orient: event.orient_to_terrain,
        });
        self.world.link(handle, slave, attached, event);
        Some(handle)
    }

    fn follow_up(&mut self, list: &str) -> Option<u32> {
        let has_decal = self
            .lists
            .get(list)?
            .nuggets
            .iter()
            .any(|nugget| nugget.dynamic_decal.is_some());
        if !has_decal {
            return None;
        }
        let index = match self.follow_ups.iter().position(|known| known == list) {
            Some(index) => index,
            None => {
                self.follow_ups.push(list.to_owned());
                self.follow_ups.len().saturating_sub(1)
            }
        };
        u32::try_from(index).ok()
    }

    fn list_spawns(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        list: &str,
        depth: u32,
    ) -> Vec<FxSpawn> {
        self.list_nuggets(renderer, vfs, resolver, list, depth)
            .into_iter()
            .map(|nugget| nugget.spawn)
            .collect()
    }

    fn list_nuggets(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        list: &str,
        depth: u32,
    ) -> Vec<NuggetSpawn> {
        let fired: Vec<(String, FxParticleNugget, Option<NuggetWeather>)> = self
            .lists
            .get(list)
            .into_iter()
            .flat_map(|list| list.nuggets.iter())
            .filter_map(|nugget| {
                Some((
                    nugget.name.clone()?,
                    nugget.particle.clone()?,
                    nugget.conditions.weather,
                ))
            })
            .collect();
        fired
            .iter()
            .filter_map(|(name, nugget, weather)| {
                let template = self.template_at(renderer, vfs, resolver, name, depth)?;
                let [x, y, z] = nugget.offset;
                Some(NuggetSpawn {
                    spawn: FxSpawn {
                        template,
                        offset: Vec3::new(x, y, z),
                        delay: nugget.initial_delay,
                        at_ground_height: nugget.create_at_ground_height,
                        orient_to_object: nugget.orient_to_object,
                        attach_to_object: nugget.attach_to_object,
                        set_target_matrix: nugget.set_target_matrix,
                    },
                    system: name.clone(),
                    create_bone: nugget.create_bone.clone(),
                    target_bone: nugget.target_bone.clone(),
                    on_land: nugget.on_land,
                    on_water: nugget.on_water,
                    weather: *weather,
                    use_target_offset: nugget.use_target_offset,
                    target_offset: {
                        let [x, y, z] = nugget.target_offset;
                        Vec3::new(x, y, z)
                    },
                })
            })
            .collect()
    }

    fn list_bone_pos(&self, list: &str) -> Vec<(FxAtBonePos, Option<NuggetWeather>)> {
        self.lists
            .get(list)
            .into_iter()
            .flat_map(|list| list.nuggets.iter())
            .filter_map(|nugget| Some((nugget.at_bone_pos.clone()?, nugget.conditions.weather)))
            .collect()
    }

    fn list_particle_bones(&self, list: &str) -> Vec<mint_sage_ini::ParticleSysBone> {
        self.lists
            .get(list)
            .into_iter()
            .flat_map(|list| list.particle_sys_bones.iter())
            .filter_map(|raw| mint_sage_ini::parse_particle_sys_bone(raw))
            .collect()
    }

    fn list_camera_shakes(&self, list: &str) -> Vec<CameraShakerVolume> {
        self.lists
            .get(list)
            .into_iter()
            .flat_map(|list| list.nuggets.iter())
            .filter_map(|nugget| nugget.camera_shaker)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fire_list(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        at: Vec3,
        secondary: Option<Vec3>,
        ground: &dyn Ground,
        caller: Option<&UnitDrawable>,
        victim: Option<&UnitDrawable>,
        is_on_water: &dyn Fn(f32, f32) -> bool,
        weather: NuggetWeather,
    ) -> Option<FireReport> {
        self.lists.get(name)?;
        let mut report = FireReport::default();
        let frame = FxFrame {
            at,
            matrix: caller.map(UnitDrawable::placement),
            secondary,
        };
        self.fire_list_at(
            renderer,
            vfs,
            resolver,
            name,
            frame,
            ground,
            caller,
            victim,
            is_on_water,
            weather,
            0,
            &mut report,
        );
        self.sync_hidden();
        Some(report)
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_list_at(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        frame: FxFrame,
        ground: &dyn Ground,
        caller: Option<&UnitDrawable>,
        victim: Option<&UnitDrawable>,
        is_on_water: &dyn Fn(f32, f32) -> bool,
        weather: NuggetWeather,
        depth: u32,
        report: &mut FireReport,
    ) {
        for nugget in self.list_nuggets(renderer, vfs, resolver, name, depth) {
            let bone = nugget.create_bone.as_deref().and_then(|bone| {
                caller.and_then(|drawable| unit_bone_frames(drawable, Some(bone)).first().copied())
            });
            if nugget.create_bone.is_some() {
                report.create_bone = report.create_bone.saturating_add(1);
                if bone.is_some() {
                    report.create_bone_found = report.create_bone_found.saturating_add(1);
                }
            }
            let secondary = match frame.secondary {
                Some(victim_point) => {
                    let target_bone_frame = nugget.target_bone.as_deref().and_then(|target| {
                        victim.and_then(|drawable| {
                            unit_bone_frames(drawable, Some(target)).first().copied()
                        })
                    });
                    if nugget.target_bone.is_some() && target_bone_frame.is_some() {
                        report.target_bone_found = report.target_bone_found.saturating_add(1);
                    }
                    Some(target_bone_point(victim_point, target_bone_frame))
                }
                None => nugget.use_target_offset.then(|| {
                    let target_bone_frame = nugget.target_bone.as_deref().and_then(|target| {
                        caller.and_then(|drawable| {
                            unit_bone_frames(drawable, Some(target)).first().copied()
                        })
                    });
                    target_offset_point(target_bone_frame, frame.matrix, nugget.target_offset)
                }),
            };
            let mut spawn = nugget.spawn;
            spawn.set_target_matrix = nugget_aims(
                nugget.target_bone.as_deref(),
                spawn.set_target_matrix,
                secondary,
            );
            let nugget_frame = FxFrame {
                at: nugget_point(frame.at, bone),
                secondary,
                ..frame
            };
            if nugget.target_bone.is_some() {
                report.target_bone = report.target_bone.saturating_add(1);
                if spawn.set_target_matrix {
                    report.target_aimed = report.target_aimed.saturating_add(1);
                }
            }
            if diag_enabled() && (nugget.create_bone.is_some() || nugget.target_bone.is_some()) {
                report_nugget_anchor(name, &nugget, frame, nugget_frame, caller, bone.is_some());
            }
            let admitted = water_gate_admits(
                nugget.on_land,
                nugget.on_water,
                is_on_water(nugget_frame.at.x, nugget_frame.at.y),
            ) && weather_gate_admits(nugget.weather, weather);
            if !admitted {
                if diag_enabled() {
                    println!(
                        "fx gate: {name} {} skipped at ({:.0},{:.0}) [land {} water {} weather \
                         {:?}]",
                        nugget.system,
                        nugget_frame.at.x,
                        nugget_frame.at.y,
                        nugget.on_land,
                        nugget.on_water,
                        nugget.weather
                    );
                }
                continue;
            }
            let (started, attached) = self.world.fire(&[spawn], nugget_frame, ground);
            report.systems = report.systems.saturating_add(started);
            if let Some(drawable) = caller {
                for (system, offset) in attached {
                    self.track(
                        system,
                        AttachTarget::UnitOffset {
                            drawable: drawable.id,
                            generation: drawable.generation,
                            template: drawable.template,
                            offset,
                        },
                        Mat4::IDENTITY,
                        false,
                        false,
                    );
                }
            }
        }
        for leaf in self.list_particle_bones(name) {
            let (leaf_frame, local) = match caller {
                Some(drawable) => {
                    let local = unit_bone_entry(drawable, leaf.bone.as_deref(), leaf.follow_bone);
                    (drawable.placement().mul_mat4(local), local)
                }
                None => (frame.matrix.unwrap_or(Mat4::IDENTITY), Mat4::IDENTITY),
            };
            let Some(system) = self.attach(renderer, vfs, resolver, &leaf.system, leaf_frame)
            else {
                continue;
            };
            if diag_enabled() {
                println!(
                    "fx leaf: {name} {} on bone {} follow {}",
                    leaf.system,
                    leaf.bone.as_deref().unwrap_or("none"),
                    leaf.follow_bone
                );
            }
            report.bone_leaf_systems.push(system);
            if let Some(drawable) = caller {
                self.track(
                    system,
                    AttachTarget::UnitBone {
                        drawable: drawable.id,
                        generation: drawable.generation,
                        template: drawable.template,
                        bone: leaf.bone.clone(),
                        owner: drawable.template,
                        search: BoneSearch::OwnModule,
                    },
                    local,
                    leaf.follow_bone,
                    false,
                );
            }
        }
        report.camera_shakes.extend(self.list_camera_shakes(name));
        if let Some(list) = self.lists.get(name) {
            report.dynamic_decals.extend(
                list.nuggets
                    .iter()
                    .filter_map(|nugget| nugget.dynamic_decal.as_ref())
                    .map(|decal| {
                        crate::dynamic_decal::FiredDecal::place(decal, frame.at, frame.matrix)
                    }),
            );
        }
        if depth >= MAX_FX_LIST_DEPTH {
            return;
        }
        for (nugget, nugget_weather) in self.list_bone_pos(name) {
            if !weather_gate_admits(nugget_weather, weather) {
                if diag_enabled() {
                    println!(
                        "fx gate: {name} FXListAtBonePos skipped [weather {nugget_weather:?}]"
                    );
                }
                continue;
            }
            report.at_bone_pos = report.at_bone_pos.saturating_add(1);
            let Some(nested) = nugget.fx.as_deref() else {
                continue;
            };
            if self.lists.get(nested).is_none() {
                report.nested_list_missing = report.nested_list_missing.saturating_add(1);
                continue;
            }
            let frames = caller.map_or_else(Vec::new, |drawable| {
                unit_bone_frames(drawable, nugget.bone_name.as_deref())
            });
            if frames.is_empty() {
                report.bone_missing = report.bone_missing.saturating_add(1);
            } else {
                report.bone_found = report.bone_found.saturating_add(1);
            }
            let fallback = match caller {
                Some(drawable) => vec![drawable.placement()],
                None => Vec::new(),
            };
            let frames = if frames.is_empty() { fallback } else { frames };
            if frames.is_empty() {
                self.fire_list_at(
                    renderer,
                    vfs,
                    resolver,
                    nested,
                    FxFrame {
                        at: frame.at,
                        matrix: Some(Mat4::IDENTITY),
                        secondary: frame.secondary,
                    },
                    ground,
                    caller,
                    victim,
                    is_on_water,
                    weather,
                    depth.saturating_add(1),
                    report,
                );
            }
            for bone_frame in frames {
                let bone_frame = match (self.bone_frame, caller) {
                    (BoneFrameMode::Bone, _) | (_, None) => bone_frame,
                    (BoneFrameMode::Object, Some(drawable)) => {
                        at_point_of(drawable.placement(), bone_frame)
                    }
                    (BoneFrameMode::World, Some(_)) => at_point_of(Mat4::IDENTITY, bone_frame),
                };
                self.fire_list_at(
                    renderer,
                    vfs,
                    resolver,
                    nested,
                    FxFrame {
                        at: bone_frame.transform_point(Vec3::new(0.0, 0.0, 0.0)),
                        matrix: Some(bone_frame),
                        secondary: frame.secondary,
                    },
                    ground,
                    caller,
                    victim,
                    is_on_water,
                    weather,
                    depth.saturating_add(1),
                    report,
                );
            }
        }
    }

    pub fn attach_named(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        frame: Mat4,
    ) -> Option<SystemId> {
        self.attach(renderer, vfs, resolver, name, frame)
    }

    pub fn attach_named_to_unit(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        name: &str,
        drawable: &UnitDrawable,
        offset: Vec3,
    ) -> Option<SystemId> {
        let frame = drawable
            .placement()
            .mul_mat4(Mat4::from_translation(offset));
        let system = self.attach(renderer, vfs, resolver, name, frame)?;
        self.track(
            system,
            AttachTarget::UnitOffset {
                drawable: drawable.id,
                generation: drawable.generation,
                template: drawable.template,
                offset,
            },
            Mat4::IDENTITY,
            false,
            false,
        );
        Some(system)
    }

    pub fn clear(&mut self) {
        self.world.clear();
        self.attachments.clear();
        self.collision_decals.clear();
    }

    pub fn stop(&mut self, id: SystemId) {
        self.world.stop(id);
    }

    pub fn set_origin(&mut self, id: SystemId, at: Vec3) -> bool {
        self.world.set_origin(id, at)
    }

    #[must_use]
    pub fn fx_list(&self, name: &str) -> Option<&mint_sage_ini::FxList> {
        self.lists.get(name)
    }

    #[must_use]
    pub fn list_sounds(&self, name: &str) -> Vec<&str> {
        self.lists
            .get(name)
            .into_iter()
            .flat_map(|list| list.nuggets.iter())
            .filter(|nugget| nugget.keyword.eq_ignore_ascii_case("Sound"))
            .filter_map(|nugget| nugget.name.as_deref())
            .collect()
    }

    #[must_use]
    pub fn list_census<'a>(&self, names: impl Iterator<Item = &'a str>) -> (usize, usize, usize) {
        names.filter_map(|name| self.lists.get(name)).fold(
            (0, 0, 0),
            |(found, particles, sounds), list| {
                let has = |keyword: &str| {
                    list.nuggets
                        .iter()
                        .any(|nugget| nugget.keyword.eq_ignore_ascii_case(keyword))
                };
                (
                    found.saturating_add(1),
                    particles.saturating_add(usize::from(has("ParticleSystem"))),
                    sounds.saturating_add(usize::from(has("Sound"))),
                )
            },
        )
    }

    fn texture_for(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        definition: &ParticleSystemDef,
    ) -> Option<TextureHandle> {
        let texture = definition.particle_name.as_deref()?;
        let key = texture.to_ascii_lowercase();
        if let Some(held) = self.textures.get(&key) {
            return *held;
        }
        let handle = if key.ends_with(".w3d") {
            None
        } else {
            let data = load_texture(
                vfs,
                resolver,
                texture,
                renderer.supports_block_compression(),
            )
            .map_or_else(|_| missing_texture(), |loaded| loaded.data);
            renderer.upload_texture(&data).ok()
        };
        self.textures.insert(key, handle);
        handle
    }

    fn model_parts(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        definition: &ParticleSystemDef,
    ) -> (Vec<Instance>, Option<ParticleModel>) {
        if DrawShape::of(
            definition.kind,
            definition.draw_module.as_deref(),
            definition.particle_name.as_deref(),
        ) != DrawShape::Model
        {
            return (Vec::new(), None);
        }
        let Some(name) = definition.particle_name.as_deref() else {
            return (Vec::new(), None);
        };
        let Some(model) = self.models.get_or_load(renderer, vfs, resolver, name) else {
            return (Vec::new(), None);
        };
        let instances = model.parts.iter().map(|part| part.instance).collect();
        let bound = ParticleModel::bind(renderer, vfs, resolver, name, model);
        (instances, Some(bound))
    }
}

fn reposes_every_frame(module_flag: bool, follow_bone: bool) -> bool {
    follow_bone || module_flag
}

fn unit_bone_entry(drawable: &UnitDrawable, bone: Option<&str>, follow_bone: bool) -> Mat4 {
    if follow_bone {
        unit_bone_follow(drawable, bone)
    } else {
        unit_bone_local(drawable, bone)
    }
}

fn bone_yaw_frame(local: Mat4) -> Mat4 {
    let origin = local.transform_point(Vec3::new(0.0, 0.0, 0.0));
    let forward = local.transform_vector(Vec3::new(1.0, 0.0, 0.0));
    let length = (forward.x * forward.x + forward.y * forward.y).sqrt();
    let (x, y) = if length > f32::EPSILON {
        (forward.x / length, forward.y / length)
    } else {
        (1.0, 0.0)
    };
    Mat4::from_cols([
        [x, y, 0.0, 0.0],
        [-y, x, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [origin.x, origin.y, origin.z, 1.0],
    ])
}

pub fn attach_particle_bones(
    particles: &mut ParticleScene,
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    index: &ObjectIndex,
    scene: &ObjectScene,
    objects: &[Object],
    proxies: &[mint_sage_terrain::ObjectProxy],
) -> BoneReport {
    let mut report = BoneReport::default();
    for (group_slot, group) in scene
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| !group.beside)
    {
        for slot in &group.placements {
            let (Some(object), Some(proxy)) = (objects.get(*slot), proxies.get(*slot)) else {
                continue;
            };
            let Some(bones) = index.particle_bones(&object.template_name) else {
                continue;
            };
            let wanted: Vec<&mint_sage_ini::ParticleSysBone> = bones.standing().collect();
            if wanted.is_empty() {
                continue;
            }
            report.placements = report.placements.saturating_add(1);
            for attachment in wanted {
                let pivot = attachment
                    .bone
                    .as_ref()
                    .and_then(|bone| group.model.pivot_named(bone));
                let local = pivot
                    .and_then(|pivot| group.model.bind_bone(pivot))
                    .unwrap_or(Mat4::IDENTITY);
                let local = if attachment.follow_bone {
                    local
                } else {
                    bone_yaw_frame(local)
                };
                let frame = proxy.placement.mul_mat4(local);
                let attached = particles.attach(renderer, vfs, resolver, &attachment.system, frame);
                let Some(system) = attached else {
                    report.dangling = report.dangling.saturating_add(1);
                    continue;
                };
                report.spawned = report.spawned.saturating_add(1);
                particles.track(
                    system,
                    AttachTarget::MapBone {
                        group: group_slot,
                        placement: *slot,
                        pivot,
                    },
                    local,
                    attachment.follow_bone,
                    attachment.module_flag,
                );
            }
        }
    }
    for (placement, (object, proxy)) in objects.iter().zip(proxies).enumerate() {
        if !matches!(
            index.lookup(&object.template_name),
            mint_sage_model::Template::NoModel(_)
        ) {
            continue;
        }
        let Some(bones) = index.particle_bones(&object.template_name) else {
            continue;
        };
        let mut wanted = bones.standing().peekable();
        if wanted.peek().is_none() {
            continue;
        }
        report.placements = report.placements.saturating_add(1);
        for attachment in wanted {
            match particles.attach(renderer, vfs, resolver, &attachment.system, proxy.placement) {
                Some(system) => {
                    report.spawned = report.spawned.saturating_add(1);
                    particles.track(
                        system,
                        AttachTarget::MapOrigin { placement },
                        Mat4::IDENTITY,
                        attachment.follow_bone,
                        attachment.module_flag,
                    );
                }
                None => report.dangling = report.dangling.saturating_add(1),
            }
        }
    }
    println!(
        "particle bones: {} system(s) attached on {} placement(s), {} dangling",
        report.spawned, report.placements, report.dangling
    );
    report
}

pub fn follow_particle_bones(
    particles: &mut ParticleScene,
    scene: &ObjectScene,
    proxies: &[mint_sage_terrain::ObjectProxy],
    seconds: f32,
) {
    let mut attachments = std::mem::take(&mut particles.attachments);
    if attachments.is_empty() {
        particles.attachments = attachments;
        return;
    }
    let mut bones: Vec<Mat4> = Vec::new();
    let mut current = usize::MAX;
    attachments.retain(|attachment| {
        let AttachTarget::MapBone {
            group,
            placement,
            pivot,
        } = attachment.target
        else {
            return true;
        };
        let (Some(group_ref), Some(proxy)) = (scene.groups.get(group), proxies.get(placement))
        else {
            return true;
        };
        let local = match (&group_ref.animation, pivot) {
            (Some(animation), Some(pivot))
                if reposes_every_frame(attachment.module_flag, attachment.follow_bone) =>
            {
                if group != current {
                    animation.bones(
                        animation.frame_at(mint_sage_model::Completion::Restart, seconds),
                        &mut bones,
                    );
                    current = group;
                }
                bones.get(pivot).copied()
            }
            _ => None,
        }
        .unwrap_or(attachment.local);
        let frame = proxy.placement.mul_mat4(local);
        particles.world.set_frame(attachment.system, frame)
    });
    particles.attachments = attachments;
}

fn bone_local(drawable: &UnitDrawable, pivot: usize) -> Mat4 {
    drawable
        .bones()
        .get(pivot)
        .copied()
        .or_else(|| drawable.model.bind_bone(pivot))
        .unwrap_or(Mat4::IDENTITY)
}

pub(crate) fn unit_bone_local(drawable: &UnitDrawable, bone: Option<&str>) -> Mat4 {
    bone.and_then(|name| drawable.model.pivot_named(name))
        .map_or(Mat4::IDENTITY, |pivot| {
            bone_yaw_frame(bone_local(drawable, pivot))
        })
}

pub(crate) fn unit_bone_follow(drawable: &UnitDrawable, bone: Option<&str>) -> Mat4 {
    bone.and_then(|name| drawable.model.pivot_named(name))
        .map_or(Mat4::IDENTITY, |pivot| bone_local(drawable, pivot))
}

fn at_point_of(orientation: Mat4, place: Mat4) -> Mat4 {
    let mut cols = orientation.to_cols();
    if let (Some(origin), Some(from)) = (cols.get_mut(3), place.to_cols().get(3)) {
        *origin = *from;
    }
    Mat4::from_cols(cols)
}

fn unit_swirl_bones(units: &UnitScene, drawable: &UnitDrawable) -> Vec<Vec3> {
    const MAX_SWIRL_BONES: usize = 16;
    if !units.is_swirl_host(drawable.template) {
        return Vec::new();
    }
    let mut bones = Vec::new();
    for number in 1..=MAX_SWIRL_BONES {
        let Some(pivot) = drawable.model.pivot_named(&format!("B_TORNADO{number:02}")) else {
            break;
        };
        let frame = drawable.placement().mul_mat4(bone_local(drawable, pivot));
        bones.push(frame.transform_point(Vec3::new(0.0, 0.0, 0.0)));
    }
    bones
}

fn unit_bone_frames(drawable: &UnitDrawable, bone: Option<&str>) -> Vec<Mat4> {
    const MAX_BONE_POINTS: usize = 40;
    let Some(bone) = bone else {
        return Vec::new();
    };
    let frame = |pivot: usize| drawable.placement().mul_mat4(bone_local(drawable, pivot));
    let mut frames: Vec<Mat4> = drawable
        .model
        .pivot_named(bone)
        .map(frame)
        .into_iter()
        .collect();
    for number in 1..=99 {
        let Some(pivot) = drawable.model.pivot_named(&format!("{bone}{number:02}")) else {
            break;
        };
        if frames.len() >= MAX_BONE_POINTS {
            break;
        }
        frames.push(frame(pivot));
    }
    frames
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum HeldModule {
    Model,
    Companion(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum HeldKind {
    Model,
    Animation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct HeldKey {
    drawable: (u32, u32, usize),
    module: HeldModule,
    kind: HeldKind,
}

impl HeldKey {
    const fn new(drawable: &UnitDrawable, module: HeldModule, kind: HeldKind) -> Self {
        Self::of(
            (drawable.id, drawable.generation, drawable.template),
            module,
            kind,
        )
    }

    const fn of(drawable: (u32, u32, usize), module: HeldModule, kind: HeldKind) -> Self {
        Self {
            drawable,
            module,
            kind,
        }
    }
}

#[derive(Debug)]
struct HeldParticles {
    state: usize,
    systems: Vec<SystemId>,
}

#[derive(Debug, Default)]
pub struct ConditionParticles {
    held: HashMap<HeldKey, HeldParticles>,
    persisted: HashMap<ModuleKey, Vec<Persisted>>,
}

type ModuleKey = ((u32, u32, usize), HeldModule);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Persisted {
    id: u32,
    system: SystemId,
    killed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistAction {
    Start,
    Keep,
    Stop { entry: usize, forget: bool },
}

fn persist_action(
    persist: Persist,
    id: u32,
    tracker: impl IntoIterator<Item = (u32, bool)>,
    tracked: usize,
) -> PersistAction {
    let matching = tracker
        .into_iter()
        .enumerate()
        .filter(|(_, (held, _))| *held == id);
    for (entry, (_, killed)) in matching {
        if persist == Persist::Spawn || killed {
            return PersistAction::Keep;
        }
        match persist {
            Persist::Hold => {
                return PersistAction::Stop {
                    entry,
                    forget: true,
                }
            }
            Persist::Kill => {
                return PersistAction::Stop {
                    entry,
                    forget: false,
                }
            }
            Persist::None | Persist::Spawn => {}
        }
    }
    if tracked == 0 && matches!(persist, Persist::Hold | Persist::Kill) {
        PersistAction::Keep
    } else {
        PersistAction::Start
    }
}

impl ConditionParticles {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        particles: &mut ParticleScene,
        units: &UnitScene,
    ) {
        let mut seen: Vec<HeldKey> = Vec::new();
        let prune_persisted = !self.persisted.is_empty();
        let mut seen_modules: Vec<ModuleKey> = Vec::new();
        for drawable in units
            .drawables()
            .iter()
            .filter(|d| d.starts_state_particles())
        {
            let Some(template) = units.templates.get(drawable.template) else {
                continue;
            };
            self.sync_module(
                renderer,
                vfs,
                resolver,
                particles,
                units,
                drawable,
                HeldModule::Model,
                drawable
                    .model_state()
                    .and_then(|state| Some((state, template.model_states.get(state)?))),
                drawable
                    .state()
                    .and_then(|state| Some((state, template.states.get(state)?))),
                true,
                &mut seen,
                prune_persisted.then_some(&mut seen_modules),
            );
            let companion_states = drawable.companion_states();
            for (index, companion) in template.companions.iter().enumerate() {
                let (animation, model) =
                    companion_states.get(index).copied().unwrap_or((None, None));
                self.sync_module(
                    renderer,
                    vfs,
                    resolver,
                    particles,
                    units,
                    drawable,
                    HeldModule::Companion(index),
                    model.and_then(|state| Some((state, companion.model_states.get(state)?))),
                    animation.and_then(|state| Some((state, companion.states.get(state)?))),
                    false,
                    &mut seen,
                    prune_persisted.then_some(&mut seen_modules),
                );
            }
        }
        seen.sort_unstable();
        let gone: Vec<HeldKey> = self
            .held
            .keys()
            .copied()
            .filter(|key| seen.binary_search(key).is_err())
            .collect();
        for key in gone {
            self.stop(particles, key);
        }
        if prune_persisted {
            seen_modules.sort_unstable();
            let gone: Vec<ModuleKey> = self
                .persisted
                .keys()
                .copied()
                .filter(|key| seen_modules.binary_search(key).is_err())
                .collect();
            for key in gone {
                for entry in self.persisted.remove(&key).unwrap_or_default() {
                    particles.stop(entry.system);
                }
            }
        }
        particles.update_unit_attachments(units);
    }

    #[allow(clippy::too_many_arguments)]
    fn sync_module(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        particles: &mut ParticleScene,
        units: &UnitScene,
        drawable: &UnitDrawable,
        module: HeldModule,
        model: Option<(usize, &impl HasParticles)>,
        animation: Option<(usize, &impl HasParticles)>,
        real_bone: bool,
        seen: &mut Vec<HeldKey>,
        seen_modules: Option<&mut Vec<ModuleKey>>,
    ) {
        if let Some(seen_modules) = seen_modules {
            seen_modules.push((
                (drawable.id, drawable.generation, drawable.template),
                module,
            ));
        }
        let model_attaches = model.is_some_and(|(_, def)| !def.particles().is_empty());
        let animation_attaches = animation.is_some_and(|(_, def)| !def.particles().is_empty());
        if !model_attaches && !animation_attaches {
            return;
        }
        let model_key = HeldKey::new(drawable, module, HeldKind::Model);
        let animation_key = HeldKey::new(drawable, module, HeldKind::Animation);
        let held_state = |key: &HeldKey| self.held.get(key).map(|held| held.state);
        let unchanged = held_state(&model_key) == model.map(|(state, _)| state)
            && held_state(&animation_key) == animation.map(|(state, _)| state);
        for (key, wanted) in [
            (model_key, model.map(|(state, _)| state)),
            (animation_key, animation.map(|(state, _)| state)),
        ] {
            if wanted.is_some() {
                seen.push(key);
            }
        }
        if unchanged {
            return;
        }
        self.stop(particles, model_key);
        self.stop(particles, animation_key);
        let mut started = 0;
        self.attach_slot(
            renderer,
            vfs,
            resolver,
            particles,
            units,
            drawable,
            animation_key,
            animation,
            real_bone,
            &mut started,
        );
        self.attach_slot(
            renderer,
            vfs,
            resolver,
            particles,
            units,
            drawable,
            model_key,
            model,
            real_bone,
            &mut started,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn attach_slot(
        &mut self,
        renderer: &mut Renderer,
        vfs: &Vfs,
        resolver: &Resolver,
        particles: &mut ParticleScene,
        units: &UnitScene,
        drawable: &UnitDrawable,
        key: HeldKey,
        wanted: Option<(usize, &impl HasParticles)>,
        real_bone: bool,
        started: &mut usize,
    ) {
        let Some((state, def)) = wanted else {
            return;
        };
        let (id, generation, template) = (drawable.id, drawable.generation, drawable.template);
        let module_key = (key.drawable, key.module);
        let mut systems = Vec::new();
        for attachment in def.particles() {
            if attachment.persist_id != 0 {
                let tracker = self.persisted.entry(module_key).or_default();
                let tracked = tracker.len().saturating_add(*started);
                let kept = tracker.iter().map(|held| (held.id, held.killed));
                match persist_action(attachment.persist, attachment.persist_id, kept, tracked) {
                    PersistAction::Start => {}
                    PersistAction::Keep => continue,
                    PersistAction::Stop { entry, forget } => {
                        if let Some(held) = tracker.get_mut(entry) {
                            particles.stop(held.system);
                            held.killed = true;
                        }
                        if forget && entry < tracker.len() {
                            tracker.remove(entry);
                        }
                        continue;
                    }
                }
            }
            let bone = if real_bone {
                attachment.bone.clone()
            } else {
                None
            };
            let source = bone_source(
                units,
                drawable,
                bone.as_deref(),
                particles
                    .bone_lookup()
                    .searches_drawable(attachment.check_drawable),
            );
            let local = unit_bone_entry(source, bone.as_deref(), attachment.follow_bone);
            let Some(system) = particles.attach_named(
                renderer,
                vfs,
                resolver,
                &attachment.system,
                source.placement().mul_mat4(local),
            ) else {
                continue;
            };
            particles.track(
                system,
                AttachTarget::UnitBone {
                    drawable: id,
                    generation,
                    template,
                    bone,
                    owner: source.template,
                    search: BoneSearch::Drawable {
                        check_drawable: attachment.check_drawable,
                    },
                },
                local,
                attachment.follow_bone,
                attachment.module_flag,
            );
            *started = started.saturating_add(1);
            if attachment.persist_id == 0 {
                systems.push(system);
            } else {
                self.persisted
                    .entry(module_key)
                    .or_default()
                    .push(Persisted {
                        id: attachment.persist_id,
                        system,
                        killed: false,
                    });
            }
        }
        self.held.insert(key, HeldParticles { state, systems });
    }

    #[must_use]
    pub fn attached(&self) -> usize {
        self.held
            .keys()
            .map(|key| key.drawable)
            .collect::<HashSet<_>>()
            .len()
    }

    fn stop(&mut self, particles: &mut ParticleScene, key: HeldKey) {
        if let Some(held) = self.held.remove(&key) {
            for system in held.systems {
                particles.stop(system);
            }
        }
    }
}

trait HasParticles {
    fn particles(&self) -> &[mint_sage_ini::ParticleSysBone];
}

impl HasParticles for mint_sage_ini::ModelStateDef {
    fn particles(&self) -> &[mint_sage_ini::ParticleSysBone] {
        &self.particles
    }
}

impl HasParticles for mint_sage_ini::AnimationStateDef {
    fn particles(&self) -> &[mint_sage_ini::ParticleSysBone] {
        &self.particles
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    #[test]
    fn an_attached_system_emits_nothing_before_the_first_hold_pass() {
        let text = "FXParticleSystem Torch
  System
    Lifetime = 100 100
                        BurstCount = 2 2
    BurstDelay = 0 0
  End
End
";
        let document = Document::parse("t.ini", text, &BlockTable::particle_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        let mut index = ParticleIndex::new();
        index.insert_document(&document);
        let definition = index.iter().next().unwrap().1.clone();
        let mut particles = ParticleScene::new(ParticleIndex::new(), FxListIndex::new());
        particles.hold_attachments_from_birth();
        let handle = particles.world.register(&definition, None);
        let system = particles.world.spawn(handle, Vec3::ZERO).unwrap();
        particles.track(
            system,
            AttachTarget::MapOrigin { placement: 0 },
            Mat4::IDENTITY,
            false,
            false,
        );
        particles.advance(1.0 / 30.0, &mint_sage_particle::NoGround);
        assert_eq!(particles.live_particles(), 0);
    }

    fn scene() -> ParticleScene {
        let text = "FXList FX_Cast\n  ParticleSystem\n    Name = CastGlow\n  End\n  \
                    Sound\n    Name = CastSound\n  End\n  Sound\n    Name = CastEcho\n  End\nEnd\n\
                    FXList FX_Shake\n  ViewShake\n    Type = SUBTLE\n  End\nEnd\n";
        let document = Document::parse("fxlist.ini", text, &BlockTable::particle_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        let mut lists = FxListIndex::new();
        lists.insert_document(&document);
        ParticleScene::new(ParticleIndex::new(), lists)
    }

    fn scene_with_bone_leaf() -> ParticleScene {
        let text =
            "FXList FX_Glow\n  ParticleSysBone = SWORD LightningCharge FollowBone:Yes\nEnd\n";
        let document = Document::parse("fxlist.ini", text, &BlockTable::particle_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        let mut lists = FxListIndex::new();
        lists.insert_document(&document);
        ParticleScene::new(ParticleIndex::new(), lists)
    }

    #[test]
    fn a_lists_direct_particle_sys_bone_leaf_is_read_as_an_attachment() {
        let scene = scene_with_bone_leaf();
        let leaves = scene.list_particle_bones("fx_glow");
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].bone.as_deref(), Some("SWORD"));
        assert_eq!(leaves[0].system, "LightningCharge");
        assert!(leaves[0].follow_bone);
    }

    #[test]
    fn use_target_offset_aims_a_victimless_nugget_above_the_bone() {
        let bone = Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [100.0, 200.0, 10.0, 1.0],
        ]);
        let offset = Vec3::new(0.0, 0.0, 300.0);
        assert_eq!(
            target_offset_point(Some(bone), None, offset),
            Vec3::new(100.0, 200.0, 310.0)
        );
        let caller = Mat4::IDENTITY;
        assert_eq!(target_offset_point(None, Some(caller), offset), offset);
    }

    #[test]
    fn a_lists_sounds_are_its_sound_nuggets_in_order() {
        let scene = scene();
        assert_eq!(scene.list_sounds("fx_cast"), ["CastSound", "CastEcho"]);
        assert!(scene.list_sounds("FX_Shake").is_empty());
        assert!(scene.list_sounds("FX_Nothing").is_empty());
    }

    #[test]
    fn the_census_counts_declared_lists_and_what_they_carry() {
        let scene = scene();
        let names = ["FX_Cast", "FX_Shake", "FX_Nothing"];
        assert_eq!(scene.list_census(names.into_iter()), (2, 1, 1));
    }

    #[test]
    fn follow_bone_alone_reposes_and_the_module_flag_extends_it_to_every_line() {
        assert!(reposes_every_frame(false, true));
        assert!(reposes_every_frame(true, false));
        assert!(reposes_every_frame(true, true));
        assert!(!reposes_every_frame(false, false));
    }

    #[test]
    fn at_point_of_keeps_the_orientations_rotation_and_the_places_origin() {
        let quarter_turn = Mat4::from_cols([
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        let bone = Mat4::from_cols([
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [9.0, 26.0, 0.0, 1.0],
        ]);
        let result = at_point_of(quarter_turn, bone);
        assert_eq!(
            result.transform_point(Vec3::ZERO),
            Vec3::new(9.0, 26.0, 0.0),
            "the bone's origin"
        );
        assert_eq!(
            result.transform_vector(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(0.0, 1.0, 0.0),
            "the caller's rotation, not the bone's"
        );
    }

    #[test]
    fn a_creation_bone_moves_the_system_to_that_bones_world_point() {
        let list_point = Vec3::new(483.0, 1820.0, 12.0);
        let bone = Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [491.0, 1824.0, 46.0, 1.0],
        ]);
        assert_eq!(
            nugget_point(list_point, Some(bone)),
            Vec3::new(491.0, 1824.0, 46.0)
        );
        assert_eq!(
            nugget_point(list_point, None),
            list_point,
            "no bone of that name: the list's own point"
        );
    }

    #[test]
    fn a_target_bone_aims_at_the_bone_on_the_victims_model() {
        let victim_point = Vec3::new(631.0, 1716.0, 10.0);
        let bat_pelvis = Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 10.0, 1.0],
        ]);
        assert_eq!(
            target_bone_point(victim_point, Some(bat_pelvis)),
            Vec3::new(0.0, 0.0, 10.0),
            "the bone's own world point"
        );
        assert_eq!(
            target_bone_point(victim_point, None),
            victim_point,
            "no bone of that name, or no victim drawable: the victim's own position"
        );
    }

    #[test]
    fn a_target_bone_aims_the_system_at_the_lists_secondary_point() {
        let victim = Some(Vec3::new(631.0, 1716.0, 10.0));
        assert!(nugget_aims(Some("BAT_PELVIS"), false, victim));
        assert!(
            !nugget_aims(Some("BAT_PELVIS"), false, None),
            "nothing to aim at"
        );
        assert!(!nugget_aims(None, false, victim), "no target named");
        assert!(
            nugget_aims(None, true, victim),
            "SetTargetMatrix still aims"
        );
    }

    #[test]
    fn a_builder_and_the_plan_it_carries_hold_separate_slots() {
        let mut particles = ParticleScene::new(ParticleIndex::new(), FxListIndex::new());
        let mut held = ConditionParticles::new();
        let slot =
            |template| HeldKey::of((24, 1, template), HeldModule::Companion(0), HeldKind::Model);
        held.held.insert(
            slot(128),
            HeldParticles {
                state: 0,
                systems: Vec::new(),
            },
        );
        held.stop(&mut particles, slot(3));
        assert_eq!(held.attached(), 1);
    }

    #[test]
    fn a_persist_line_keeps_stops_or_starts_by_its_mode() {
        use PersistAction::{Keep, Start, Stop};
        let kept = [(7, false), (9, false)];
        assert_eq!(persist_action(Persist::Spawn, 9, kept, 2), Keep);
        assert_eq!(persist_action(Persist::Spawn, 5, kept, 2), Start);
        assert_eq!(
            persist_action(Persist::Hold, 9, kept, 2),
            Stop {
                entry: 1,
                forget: true
            }
        );
        assert_eq!(
            persist_action(Persist::Kill, 7, kept, 2),
            Stop {
                entry: 0,
                forget: false
            }
        );
        assert_eq!(persist_action(Persist::Hold, 7, [(7, true)], 1), Keep);
        assert_eq!(persist_action(Persist::None, 7, kept, 2), Start);
        assert_eq!(persist_action(Persist::Hold, 5, [], 0), Keep);
        assert_eq!(persist_action(Persist::Hold, 5, [], 1), Start);
    }

    #[test]
    fn the_bone_frame_default_is_the_callers_matrix() {
        assert_eq!(BoneFrameMode::default(), BoneFrameMode::Object);
    }

    #[test]
    fn bone_yaw_frame_keeps_the_bones_origin_and_only_its_ground_plane_rotation() {
        let yawed = Mat4::from_cols([
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [9.0, 26.0, 3.0, 1.0],
        ]);
        let frame = bone_yaw_frame(yawed);
        assert_eq!(frame.transform_point(Vec3::ZERO), Vec3::new(9.0, 26.0, 3.0));
        assert_eq!(
            frame.transform_vector(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(0.0, 1.0, 0.0),
            "the bone's own ground-plane rotation"
        );

        let vertical = Mat4::from_cols([
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);
        assert_eq!(
            bone_yaw_frame(vertical).transform_vector(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(1.0, 0.0, 0.0)
        );
    }
}
