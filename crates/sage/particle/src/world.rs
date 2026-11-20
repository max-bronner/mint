use std::collections::{BTreeMap, BTreeSet};

use mint_render::{
    Blend, Camera, CustomMaterialSource, CustomShader, Instance, Mat4, MeshData, MeshHandle,
    Renderer, TextureHandle, Vec3,
};
use mint_sage_ini::particle::KEY_SLOTS;
use mint_sage_ini::{
    AlphaKey, ColorKey, EmissionVelocity, EmissionVolume, Flipbook, LightningWave,
    ParticlePriority, ParticleSystemDef, RandomRange, WindMotion,
};

use crate::budget::{Budget, BudgetReport};
use crate::draw::{
    self, BatchShape, BillboardMode, DrawShape, FlatSize, FlipbookMode, ParticleShaders, QuadFill,
    RibbonJitter, ShapeMode, SmudgeJitter, SortMode, FLIPBOOK_SHADER_WGSL, SMUDGE_SHADER_WGSL,
};
use crate::ground::Ground;
use crate::rng::{ClientRng, MIN_LENGTH_SQUARED};

pub const PARTICLE_HZ: u32 = 30;

const STEP_SECONDS: f32 = 1.0 / 30.0;

const MAX_STEPS_PER_CALL: u32 = 4;

const MAX_WARM_UP_STEPS: u32 = 3_600;

pub const MAX_PARTICLES_PER_SYSTEM: usize = 512;

const MIN_PARTICLES_PER_SYSTEM: usize = 4;

const MIN_WIND_ANGLE_CHANGE: f32 = 0.005;

const WIND_PARTICLE_JITTER: RandomRange = RandomRange {
    low: 0.7,
    high: 1.3,
};

const TWO_PI: f32 = std::f32::consts::TAU;

const WIND_ANGLE_CHANGE_DEFAULT: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindMode {
    #[default]
    Retail,
    Off,
    Doubled,
}

impl WindMode {
    fn base(self) -> Option<f32> {
        match self {
            Self::Retail => Some(1.0),
            Self::Off => None,
            Self::Doubled => Some(2.0),
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Retail => Self::Off,
            Self::Off => Self::Doubled,
            Self::Doubled => Self::Retail,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Retail => "retail",
            Self::Off => "off",
            Self::Doubled => "doubled",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "retail" => Self::Retail,
            "off" => Self::Off,
            "doubled" => Self::Doubled,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AttachedFrame {
    #[default]
    World,
    Carrier,
}

impl AttachedFrame {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::World => Self::Carrier,
            Self::Carrier => Self::World,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Carrier => "carrier",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BeamLengthMode {
    #[default]
    Authored,
    Stretched,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdditiveAlpha {
    #[default]
    Intensity,
    Ignored,
}

impl AdditiveAlpha {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Intensity => Self::Ignored,
            Self::Ignored => Self::Intensity,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Intensity => "intensity",
            Self::Ignored => "ignored",
        }
    }
}

impl BeamLengthMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Authored => Self::Stretched,
            Self::Stretched => Self::Authored,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Authored => "authored",
            Self::Stretched => "stretched",
        }
    }
}

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub(crate) fn scale(v: Vec3, k: f32) -> Vec3 {
    Vec3::new(v.x * k, v.y * k, v.z * k)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TemplateHandle(usize);

#[derive(Debug, Clone)]
pub struct ModelDraw {
    pub template: TemplateHandle,
    pub start: usize,
    pub placements: Vec<Mat4>,
    pub ages: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SystemId(u64);

#[derive(Debug, Clone, Copy)]
struct Emission {
    volume: Volume,
    velocity: Velocity,
    size: RandomRange,
    lifetime: RandomRange,
    color_scale: RandomRange,
    burst_count: RandomRange,
    burst_delay: RandomRange,
    initial_delay: RandomRange,
    system_lifetime: Option<f32>,
    one_shot: bool,
    integrate_alpha: bool,
    colors: [ColorKey; KEY_SLOTS],
    alphas: [AlphaKey; KEY_SLOTS],
    motion: Motion,
    wind: Option<WindSpec>,
    start_size_rate: RandomRange,
    up_towards_emitter: bool,
    render_size: Option<RenderSize>,
}

#[derive(Debug, Clone, Copy)]
struct RenderSize {
    start: [RandomRange; 3],
    rate: [RandomRange; 3],
    damping: [RandomRange; 3],
}

impl RenderSize {
    fn compile(definition: &ParticleSystemDef) -> Option<Self> {
        Some(Self {
            start: definition.render_size?,
            rate: definition.render_size_rate,
            damping: definition.render_size_damping,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct WindSpec {
    motion: WindMotion,
    strength: f32,
    full: f32,
    zero: f32,
    angle_change: RandomRange,
    start: RandomRange,
    end: RandomRange,
}

#[derive(Debug, Clone, Copy, Default)]
struct WindState {
    angle: f32,
    change: f32,
    start: f32,
    end: f32,
    angle_change: f32,
}

#[derive(Debug, Clone, Copy)]
struct Motion {
    gravity: f32,
    drift: Vec3,
    velocity_damping: RandomRange,
    attach_to_bone: bool,
    swirly: bool,
    size_rate: RandomRange,
    size_rate_damping: RandomRange,
    angle_z: RandomRange,
    angular_rate_z: RandomRange,
    angular_damping: RandomRange,
}

impl Motion {
    fn compile(definition: &ParticleSystemDef) -> Self {
        let [x, y, z] = definition.drift_velocity;
        Self {
            gravity: definition.gravity,
            drift: Vec3::new(x, y, z),
            velocity_damping: definition.velocity_damping,
            attach_to_bone: definition.particles_attach_to_bone,
            swirly: definition.swirly,
            size_rate: definition.size_rate,
            size_rate_damping: definition.size_rate_damping,
            angle_z: definition.angle_z,
            angular_rate_z: definition.angular_rate_z,
            angular_damping: definition.angular_damping,
        }
    }
}

impl WindSpec {
    fn compile(definition: &ParticleSystemDef) -> Option<Self> {
        let wind = definition.wind?;
        if !wind.motion.moves() {
            return None;
        }
        Some(Self {
            motion: wind.motion,
            strength: wind.strength(),
            full: wind.full_strength_dist(),
            zero: wind.zero_strength_dist(),
            angle_change: wind.angle_change,
            start: wind.ping_pong_start,
            end: wind.ping_pong_end,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Volume {
    Point,
    Line {
        start: [f32; 3],
        end: [f32; 3],
    },
    Box {
        half_size: [f32; 3],
        hollow: bool,
    },
    Sphere {
        radius: f32,
        hollow: bool,
    },
    Cylinder {
        radius: f32,
        length: f32,
        offset: [f32; 3],
        radius_rate: f32,
        hollow: bool,
    },
    Lightning {
        start: [f32; 3],
        end: [f32; 3],
        waves: [LightningWave; 3],
    },
}

impl Volume {
    fn compile(volume: &EmissionVolume) -> Self {
        match *volume {
            EmissionVolume::Point | EmissionVolume::Unread { .. } => Self::Point,
            EmissionVolume::Line { start, end } => Self::Line { start, end },
            EmissionVolume::Box { half_size, hollow } => Self::Box { half_size, hollow },
            EmissionVolume::Sphere { radius, hollow } => Self::Sphere { radius, hollow },
            EmissionVolume::Cylinder {
                radius,
                length,
                offset,
                radius_rate,
                hollow,
            } => Self::Cylinder {
                radius,
                length,
                offset,
                radius_rate,
                hollow,
            },
            EmissionVolume::Lightning { start, end, waves } => {
                Self::Lightning { start, end, waves }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Velocity {
    Ortho([RandomRange; 3]),
    Spherical(RandomRange),
    Hemispherical(RandomRange),
    Cylindrical {
        radial: RandomRange,
        normal: RandomRange,
    },
    Outward {
        speed: RandomRange,
        other_speed: RandomRange,
    },
}

impl Velocity {
    fn compile(definition: &ParticleSystemDef) -> Self {
        match definition.velocity {
            EmissionVelocity::Ortho | EmissionVelocity::Unread { .. } => {
                Self::Ortho(definition.ortho_velocity)
            }
            EmissionVelocity::Spherical { speed } => Self::Spherical(speed),
            EmissionVelocity::Hemispherical { speed } => Self::Hemispherical(speed),
            EmissionVelocity::Cylindrical { radial, normal } => {
                Self::Cylindrical { radial, normal }
            }
            EmissionVelocity::Outward { speed, other_speed } => {
                Self::Outward { speed, other_speed }
            }
        }
    }
}

impl Emission {
    fn compile(definition: &ParticleSystemDef) -> Self {
        Self {
            volume: Volume::compile(&definition.volume),
            velocity: Velocity::compile(definition),
            size: definition.size,
            lifetime: definition.lifetime,
            color_scale: definition.color_scale,
            burst_count: definition.burst_count,
            burst_delay: definition.burst_delay,
            initial_delay: definition.initial_delay,
            system_lifetime: definition.system_lifetime.filter(|frames| *frames > 0.0),
            one_shot: definition.one_shot,
            integrate_alpha: definition.shader().reads_alpha(),
            colors: definition.colors,
            alphas: definition.alphas,
            motion: Motion::compile(definition),
            wind: WindSpec::compile(definition),
            start_size_rate: definition.start_size_rate,
            up_towards_emitter: definition.up_towards_emitter,
            render_size: RenderSize::compile(definition),
        }
    }

    fn color_frames(&self) -> [u32; KEY_SLOTS] {
        let mut frames = [0; KEY_SLOTS];
        for (slot, key) in self.colors.iter().enumerate() {
            if let Some(held) = frames.get_mut(slot) {
                *held = key.frame;
            }
        }
        frames
    }

    fn alpha_frames(&self) -> [u32; KEY_SLOTS] {
        let mut frames = [0; KEY_SLOTS];
        for (slot, key) in self.alphas.iter().enumerate() {
            if let Some(held) = frames.get_mut(slot) {
                *held = key.frame;
            }
        }
        frames
    }
}

#[derive(Debug, Clone)]
struct Template {
    definition: ParticleSystemDef,
    emission: Emission,
    texture: Option<TextureHandle>,
    blend: Blend,
    alpha_cutoff: f32,
    ground_aligned: bool,
    sort_level: Option<u8>,
    priority: ParticlePriority,
    capacity: usize,
    shape: DrawShape,
    flipbook: Option<Flipbook>,
    parts: Vec<Instance>,
    slave: Option<TemplateHandle>,
    masters: Vec<usize>,
    slave_offset: Vec3,
    per_particle: Option<TemplateHandle>,
    event: Option<CollisionEvent>,
}

#[derive(Debug, Clone, Copy)]
pub struct FxFrame {
    pub at: Vec3,
    pub matrix: Option<Mat4>,
    pub secondary: Option<Vec3>,
}

impl FxFrame {
    #[must_use]
    pub const fn at(at: Vec3) -> Self {
        Self {
            at,
            matrix: None,
            secondary: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FxSpawn {
    pub template: TemplateHandle,
    pub offset: Vec3,
    pub delay: RandomRange,
    pub at_ground_height: bool,
    pub orient_to_object: bool,
    pub attach_to_object: bool,
    pub set_target_matrix: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CollisionEvent {
    pub fires: Vec<FxSpawn>,
    pub height_offset: f32,
    pub kill_after: bool,
    pub follow_up: Option<u32>,
    pub orient: bool,
}

pub const MAX_COMPOUND_DEPTH: u32 = 4;

#[derive(Debug, Clone)]
pub(crate) struct Particle {
    pub(crate) position: Vec3,
    pub(crate) previous: Vec3,
    pub(crate) previous_size: f32,
    pub(crate) previous_angle: f32,
    pub(crate) velocity: Vec3,
    pub(crate) size: f32,
    pub(crate) age: u32,
    pub(crate) lifetime: u32,
    pub(crate) color: [f32; 3],
    pub(crate) previous_color: [f32; 3],
    pub(crate) color_rate: [f32; 3],
    pub(crate) color_slot: usize,
    pub(crate) alpha: f32,
    pub(crate) previous_alpha: f32,
    pub(crate) alpha_rate: f32,
    pub(crate) alpha_slot: usize,
    pub(crate) alpha_values: [f32; KEY_SLOTS],
    pub(crate) color_values: [[f32; 3]; KEY_SLOTS],
    pub(crate) velocity_damping: f32,
    pub(crate) size_rate: f32,
    pub(crate) size_rate_damping: f32,
    pub(crate) angle: f32,
    pub(crate) angular_rate: f32,
    pub(crate) angular_damping: f32,
    pub(crate) wind_jitter: f32,
    pub(crate) sheet_start: u32,
    pub(crate) attached: Option<SystemId>,
    pub(crate) collided: bool,
    pub(crate) emitter: Vec3,
    pub(crate) render_size: Option<[f32; 3]>,
    pub(crate) previous_render_size: Option<[f32; 3]>,
    pub(crate) render_size_rate: [f32; 3],
    pub(crate) render_size_damping: [f32; 3],
}

#[derive(Debug)]
struct LiveSystem {
    id: SystemId,
    template: usize,
    priority: ParticlePriority,
    origin: Vec3,
    previous_origin: Vec3,
    swirl: Vec<Vec3>,
    orientation: Option<Mat4>,
    target: Option<Vec3>,
    buffer: Option<Buffer>,
    filled: usize,
    particles: Vec<Particle>,
    radius: f32,
    delay_left: f32,
    burst_left: f32,
    age: u32,
    emitting: bool,
    suppressed: bool,
    dropped: u64,
    slave: Option<SystemId>,
    driven: bool,
    depth: u32,
    wind: WindState,
    size_bonus: f32,
}

const MAX_SIZE_BONUS: f32 = 50.0;

const MIN_CYLINDER_RADIUS: f32 = 0.1;

#[derive(Debug, Clone, Copy)]
struct Buffer {
    shape: BatchShape,
    capacity: usize,
    mesh: MeshHandle,
}

#[derive(Debug)]
pub struct ParticleWorld {
    templates: Vec<Template>,
    systems: Vec<LiveSystem>,
    free_meshes: Vec<Buffer>,
    rng: ClientRng,
    accumulator: f32,
    next_id: u64,
    steps: u64,
    smudge_jitter: SmudgeJitter,
    flat_size: FlatSize,
    solo: Option<usize>,
    hidden: BTreeSet<usize>,
    keys: bool,
    wind: WindMode,
    motion: bool,
    shaders: ParticleShaders,
    billboard: BillboardMode,
    sort: SortMode,
    flipbook: FlipbookMode,
    shape: ShapeMode,
    compound: bool,
    attached_frame: AttachedFrame,
    beam_length: BeamLengthMode,
    additive_alpha: AdditiveAlpha,
    smooth: bool,
    budget: Budget,
    refusals: BudgetReport,
    counted_live: usize,
    created: u64,
    scratch: MeshData,
    slots: BTreeMap<u64, usize>,
    scratch_follows: Vec<(SystemId, Vec3, bool)>,
    scratch_events: Vec<(Vec3, [f32; 2])>,
    scratch_placements: Vec<Mat4>,
    model_draws: Vec<ModelDraw>,
    collisions: Vec<Collision>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collision {
    pub follow_up: u32,
    pub at: Vec3,
    pub heading: Option<[f32; 2]>,
}

const MAX_PENDING_COLLISIONS: usize = 1024;

impl Default for ParticleWorld {
    fn default() -> Self {
        Self::new(0x5eed_0f19_2b3c_4d5e)
    }
}

impl ParticleWorld {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            templates: Vec::new(),
            systems: Vec::new(),
            free_meshes: Vec::new(),
            shaders: ParticleShaders::default(),
            rng: ClientRng::new(seed),
            accumulator: 0.0,
            next_id: 0,
            steps: 0,
            smudge_jitter: SmudgeJitter::default(),
            flat_size: FlatSize::default(),
            solo: None,
            hidden: BTreeSet::new(),
            keys: true,
            motion: true,
            wind: WindMode::default(),
            billboard: BillboardMode::default(),
            sort: SortMode::default(),
            flipbook: FlipbookMode::default(),
            shape: ShapeMode::default(),
            compound: true,
            attached_frame: AttachedFrame::default(),
            beam_length: BeamLengthMode::default(),
            additive_alpha: AdditiveAlpha::default(),
            smooth: true,
            budget: Budget::default(),
            refusals: BudgetReport::default(),
            counted_live: 0,
            created: 0,
            scratch: MeshData {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            slots: BTreeMap::new(),
            scratch_follows: Vec::new(),
            scratch_events: Vec::new(),
            scratch_placements: Vec::new(),
            model_draws: Vec::new(),
            collisions: Vec::new(),
        }
    }

    pub fn take_collisions(&mut self, out: &mut Vec<Collision>) {
        out.append(&mut self.collisions);
    }

    pub fn register(
        &mut self,
        definition: &ParticleSystemDef,
        texture: Option<TextureHandle>,
    ) -> TemplateHandle {
        self.register_model(definition, texture, Vec::new())
    }

    pub fn register_model(
        &mut self,
        definition: &ParticleSystemDef,
        texture: Option<TextureHandle>,
        parts: Vec<Instance>,
    ) -> TemplateHandle {
        let (blend, alpha_cutoff) = draw::shader_draw_state(definition.shader());
        let [x, y, z] = definition.slave_pos_offset;
        self.templates.push(Template {
            emission: Emission::compile(definition),
            texture,
            blend,
            alpha_cutoff,
            ground_aligned: definition.ground_aligned,
            sort_level: definition.sort_level,
            priority: definition.priority(),
            capacity: batch_capacity(definition),
            shape: DrawShape::of(
                definition.kind,
                definition.draw_module.as_deref(),
                definition.particle_name.as_deref(),
            ),
            flipbook: definition.flipbook,
            parts,
            slave: None,
            masters: Vec::new(),
            slave_offset: Vec3::new(x, y, z),
            per_particle: None,
            event: None,
            definition: definition.clone(),
        });
        TemplateHandle(self.templates.len().saturating_sub(1))
    }

    pub fn link(
        &mut self,
        template: TemplateHandle,
        slave: Option<TemplateHandle>,
        per_particle: Option<TemplateHandle>,
        event: Option<CollisionEvent>,
    ) -> bool {
        let Some(held) = self.templates.get_mut(template.0) else {
            return false;
        };
        held.slave = slave;
        held.per_particle = per_particle;
        held.event = event;
        if let Some(slave) = slave {
            if let Some(held) = self.templates.get_mut(slave.0) {
                if !held.masters.contains(&template.0) {
                    held.masters.push(template.0);
                }
            }
            self.resize_driven(slave.0, 0);
        }
        true
    }

    fn resize_driven(&mut self, template: usize, depth: u32) {
        if depth > MAX_COMPOUND_DEPTH {
            return;
        }
        let Some(held) = self.templates.get(template) else {
            return;
        };
        let lifetime = held.definition.lifetime;
        let mut rates = Vec::new();
        for master in &held.masters {
            self.arrival_rates(*master, 0, &mut rates);
        }
        let capacity = rates
            .iter()
            .map(|(count, delay)| capacity_from(lifetime, *count, *delay))
            .fold(batch_capacity(&held.definition), usize::max);
        let next = held.slave;
        if let Some(held) = self.templates.get_mut(template) {
            held.capacity = capacity;
        }
        if let Some(next) = next {
            self.resize_driven(next.0, depth.saturating_add(1));
        }
    }

    fn arrival_rates(
        &self,
        template: usize,
        depth: u32,
        out: &mut Vec<(RandomRange, RandomRange)>,
    ) {
        let Some(held) = self.templates.get(template) else {
            return;
        };
        out.push((held.definition.burst_count, held.definition.burst_delay));
        if depth >= MAX_COMPOUND_DEPTH {
            return;
        }
        for master in &held.masters {
            self.arrival_rates(*master, depth.saturating_add(1), out);
        }
    }

    #[must_use]
    pub fn shape_of(&self, template: TemplateHandle) -> Option<DrawShape> {
        self.templates.get(template.0).map(|held| held.shape)
    }

    #[must_use]
    pub fn model_parts(&self, template: TemplateHandle) -> usize {
        self.templates
            .get(template.0)
            .map_or(0, |held| held.parts.len())
    }

    #[must_use]
    pub fn templates(&self) -> usize {
        self.templates.len()
    }

    #[must_use]
    pub fn definition(&self, template: TemplateHandle) -> Option<&ParticleSystemDef> {
        self.templates.get(template.0).map(|held| &held.definition)
    }

    #[must_use]
    pub fn capacity(&self, template: TemplateHandle) -> Option<usize> {
        self.templates.get(template.0).map(|held| held.capacity)
    }

    pub fn spawn(&mut self, template: TemplateHandle, origin: Vec3) -> Option<SystemId> {
        self.spawn_at_depth(template, origin, 0, false, 0.0)
    }

    pub fn fire(
        &mut self,
        nuggets: &[FxSpawn],
        frame: FxFrame,
        ground: &dyn Ground,
    ) -> (usize, Vec<(SystemId, Vec3)>) {
        self.fire_at_depth(nuggets, frame, 0, ground)
    }

    fn fire_at_depth(
        &mut self,
        nuggets: &[FxSpawn],
        frame: FxFrame,
        depth: u32,
        ground: &dyn Ground,
    ) -> (usize, Vec<(SystemId, Vec3)>) {
        let mut started = 0usize;
        let mut attached = Vec::new();
        for index in 0..nuggets.len() {
            let Some(nugget) = nuggets.get(index).copied() else {
                continue;
            };
            let offset = frame.matrix.map_or(nugget.offset, |matrix| {
                matrix.transform_vector(nugget.offset)
            });
            let mut origin = add(frame.at, offset);
            if nugget.at_ground_height {
                if let Some(height) = ground.height_at(origin.x, origin.y) {
                    origin = Vec3::new(origin.x, origin.y, height);
                }
            }
            let delay = self.rng.range(nugget.delay).max(0.0);
            let target = frame.secondary.filter(|_| nugget.set_target_matrix);
            let caller = frame.matrix.filter(|_| nugget.orient_to_object);
            let orientation = match (caller, target) {
                (Some(caller), Some(secondary)) => {
                    Some(aim_frame(caller, origin, secondary, self.beam_length))
                }
                _ => caller,
            };
            if let Some(id) =
                self.spawn_oriented(nugget.template, origin, orientation, depth, false, delay)
            {
                started = started.saturating_add(1);
                if target.is_some() {
                    if let Some(system) = self.system_mut(id) {
                        system.target = frame.secondary;
                    }
                }
                if nugget.attach_to_object {
                    attached.push((id, nugget.offset));
                }
            }
        }
        (started, attached)
    }

    fn spawn_at_depth(
        &mut self,
        template: TemplateHandle,
        origin: Vec3,
        depth: u32,
        driven: bool,
        extra_delay: f32,
    ) -> Option<SystemId> {
        self.spawn_oriented(template, origin, None, depth, driven, extra_delay)
    }

    fn spawn_oriented(
        &mut self,
        template: TemplateHandle,
        origin: Vec3,
        orientation: Option<Mat4>,
        depth: u32,
        driven: bool,
        extra_delay: f32,
    ) -> Option<SystemId> {
        if depth > MAX_COMPOUND_DEPTH {
            self.refusals.depth_refused = self.refusals.depth_refused.saturating_add(1);
            return None;
        }
        let held = self.templates.get(template.0)?;
        let capacity = held.capacity;
        let emission = held.emission;
        let priority = held.priority;
        let slave_link = held.slave;
        let slave_offset = held.slave_offset;
        if !self.budget.detail.admits(priority) {
            self.refusals.lod_refused = self.refusals.lod_refused.saturating_add(1);
            return None;
        }

        let id = SystemId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.slots.insert(id.0, self.systems.len());
        let wind = match &emission.wind {
            None => WindState::default(),
            Some(spec) => {
                let start = self.rng.range(spec.start);
                let end = self.rng.range(spec.end);
                let angle = self.rng.range(RandomRange {
                    low: start,
                    high: end,
                });
                WindState {
                    angle,
                    change: 1.0,
                    start,
                    end,
                    angle_change: WIND_ANGLE_CHANGE_DEFAULT,
                }
            }
        };
        self.systems.push(LiveSystem {
            id,
            template: template.0,
            priority,
            origin,
            previous_origin: origin,
            swirl: Vec::new(),
            orientation,
            target: None,
            buffer: None,
            filled: 0,
            particles: Vec::with_capacity(capacity),
            radius: match emission.volume {
                Volume::Cylinder { radius, .. } => radius,
                _ => 0.0,
            },
            delay_left: (self.rng.range(emission.initial_delay) + extra_delay).max(0.0),
            burst_left: 0.0,
            age: 0,
            emitting: true,
            suppressed: false,
            dropped: 0,
            slave: None,
            driven,
            depth,
            wind,
            size_bonus: 0.0,
        });
        if self.compound {
            if let Some(slave) = slave_link {
                let started = self.spawn_at_depth(
                    slave,
                    add(origin, slave_offset),
                    depth.saturating_add(1),
                    true,
                    0.0,
                );
                if let Some(system) = self.system_mut(id) {
                    system.slave = started;
                }
            }
        }
        Some(id)
    }

    fn system_mut(&mut self, id: SystemId) -> Option<&mut LiveSystem> {
        let slot = *self.slots.get(&id.0)?;
        self.systems.get_mut(slot)
    }

    fn apply_size_bonus(&mut self, slot: usize, emission: &Emission, particle: &mut Particle) {
        if emission.start_size_rate == RandomRange::default() {
            return;
        }
        let bonus = self
            .systems
            .get(slot)
            .map_or(0.0, |system| system.size_bonus);
        particle.size += bonus;
        particle.previous_size = particle.size;
        let roll = self.rng.range(emission.start_size_rate);
        if let Some(system) = self.systems.get_mut(slot) {
            system.size_bonus = (system.size_bonus + roll).min(MAX_SIZE_BONUS);
        }
    }

    fn take_system(&mut self, slot: usize) -> Option<LiveSystem> {
        if slot >= self.systems.len() {
            return None;
        }
        let system = self.systems.swap_remove(slot);
        self.slots.remove(&system.id.0);
        if let Some(moved) = self.systems.get(slot) {
            self.slots.insert(moved.id.0, slot);
        }
        Some(system)
    }

    pub fn clear(&mut self) {
        self.free_meshes
            .extend(self.systems.iter().filter_map(|system| system.buffer));
        self.systems.clear();
        self.slots.clear();
        self.collisions.clear();
    }

    pub fn remove(&mut self, id: SystemId) -> bool {
        let Some(slot) = self.slots.get(&id.0).copied() else {
            return false;
        };
        let Some(system) = self.take_system(slot) else {
            return false;
        };
        let slave = system.slave;
        self.free_meshes.extend(system.buffer);
        if let Some(slave) = slave {
            self.remove(slave);
        }
        true
    }

    pub fn stop(&mut self, id: SystemId) {
        let slave = match self.system_mut(id) {
            Some(system) => {
                system.emitting = false;
                system.slave
            }
            None => return,
        };
        if let Some(slave) = slave {
            self.stop(slave);
        }
    }

    pub fn set_suppressed(&mut self, id: SystemId, suppressed: bool) -> bool {
        let Some(system) = self.system_mut(id) else {
            return false;
        };
        system.suppressed = suppressed;
        true
    }

    #[must_use]
    pub fn keys(&self) -> bool {
        self.keys
    }

    pub fn set_keys(&mut self, keys: bool) {
        self.keys = keys;
    }

    #[must_use]
    pub fn motion(&self) -> bool {
        self.motion
    }

    pub fn set_motion(&mut self, motion: bool) {
        self.motion = motion;
    }

    #[must_use]
    pub fn wind(&self) -> WindMode {
        self.wind
    }

    pub fn set_wind(&mut self, wind: WindMode) {
        self.wind = wind;
    }

    #[must_use]
    pub fn live_with_wind(&self) -> usize {
        self.systems
            .iter()
            .filter(|system| {
                self.templates
                    .get(system.template)
                    .is_some_and(|held| held.emission.wind.is_some())
            })
            .count()
    }

    #[must_use]
    pub fn billboard(&self) -> BillboardMode {
        self.billboard
    }

    pub fn set_billboard(&mut self, billboard: BillboardMode) {
        self.billboard = billboard;
    }

    #[must_use]
    pub fn sort(&self) -> SortMode {
        self.sort
    }

    pub fn set_sort(&mut self, sort: SortMode) {
        self.sort = sort;
    }

    #[must_use]
    pub fn flipbook(&self) -> FlipbookMode {
        self.flipbook
    }

    #[must_use]
    pub fn flat_size(&self) -> FlatSize {
        self.flat_size
    }

    pub fn set_flat_size(&mut self, size: FlatSize) {
        self.flat_size = size;
    }

    #[must_use]
    pub fn smudge_jitter(&self) -> SmudgeJitter {
        self.smudge_jitter
    }

    pub fn set_smudge_jitter(&mut self, jitter: SmudgeJitter) {
        self.smudge_jitter = jitter;
    }

    pub fn set_flipbook(&mut self, flipbook: FlipbookMode) {
        self.flipbook = flipbook;
    }

    #[must_use]
    pub fn shape(&self) -> ShapeMode {
        self.shape
    }

    pub fn set_shape(&mut self, shape: ShapeMode) {
        if self.shape == shape {
            return;
        }
        for system in &mut self.systems {
            self.free_meshes.extend(system.buffer.take());
        }
        self.shape = shape;
    }

    #[must_use]
    pub fn compound(&self) -> bool {
        self.compound
    }

    #[must_use]
    pub const fn attached_frame(&self) -> AttachedFrame {
        self.attached_frame
    }

    pub fn set_attached_frame(&mut self, frame: AttachedFrame) {
        self.attached_frame = frame;
    }

    #[must_use]
    pub const fn beam_length(&self) -> BeamLengthMode {
        self.beam_length
    }

    #[must_use]
    pub const fn additive_alpha(&self) -> AdditiveAlpha {
        self.additive_alpha
    }

    pub fn set_additive_alpha(&mut self, mode: AdditiveAlpha) {
        self.additive_alpha = mode;
    }

    pub fn set_beam_length(&mut self, mode: BeamLengthMode) {
        self.beam_length = mode;
    }

    pub fn set_compound(&mut self, compound: bool) {
        self.compound = compound;
    }

    #[must_use]
    pub fn live_with_flipbook(&self) -> usize {
        self.systems
            .iter()
            .filter(|system| {
                self.templates
                    .get(system.template)
                    .is_some_and(|template| template.flipbook.is_some())
            })
            .count()
    }

    #[must_use]
    pub fn live_by_shape(&self) -> [(DrawShape, usize); 5] {
        let mut counts: [(DrawShape, usize); 5] = [
            (DrawShape::Sprite, 0),
            (DrawShape::Model, 0),
            (DrawShape::Streak, 0),
            (DrawShape::Volume, 0),
            (DrawShape::Smudge, 0),
        ];
        for system in &self.systems {
            let Some(template) = self.templates.get(system.template) else {
                continue;
            };
            for entry in &mut counts {
                if entry.0 == template.shape {
                    entry.1 = entry.1.saturating_add(1);
                }
            }
        }
        counts
    }

    #[must_use]
    pub fn budget(&self) -> Budget {
        self.budget
    }

    pub fn set_budget(&mut self, budget: Budget) {
        self.budget = budget;
    }

    #[must_use]
    pub fn refusals(&self) -> BudgetReport {
        let mut report = self.refusals;
        let (counted, exempt) = self.live_split();
        report.counted = counted;
        report.exempt = exempt;
        report
    }

    fn live_split(&self) -> (usize, usize) {
        let mut counted = 0usize;
        let mut exempt = 0usize;
        for system in &self.systems {
            let held = system.particles.len();
            if system.priority.is_exempt() {
                exempt = exempt.saturating_add(held);
            } else {
                counted = counted.saturating_add(held);
            }
        }
        (counted, exempt)
    }

    pub fn set_origin(&mut self, id: SystemId, origin: Vec3) -> bool {
        let Some(slot) = self.slots.get(&id.0).copied() else {
            return false;
        };
        let Some(system) = self.systems.get_mut(slot) else {
            return false;
        };
        system.origin = origin;
        let (slave, template) = (system.slave, system.template);
        if let Some(slave) = slave {
            let offset = self
                .templates
                .get(template)
                .map_or(Vec3::ZERO, |held| held.slave_offset);
            self.set_origin(slave, add(origin, offset));
        }
        true
    }

    pub fn set_swirl_bones(&mut self, id: SystemId, bones: &[Vec3]) -> bool {
        let Some(system) = self.system_mut(id) else {
            return false;
        };
        system.swirl.clear();
        system.swirl.extend(bones.iter().take(16));
        true
    }

    pub fn set_frame(&mut self, id: SystemId, frame: Mat4) -> bool {
        let origin = frame.transform_point(Vec3::new(0.0, 0.0, 0.0));
        if !self.set_origin(id, origin) {
            return false;
        }
        if let Some(system) = self.system_mut(id) {
            system.orientation = Some(frame);
        }
        true
    }

    #[must_use]
    pub fn live_systems(&self) -> usize {
        self.systems.len()
    }

    #[must_use]
    pub fn live_particles(&self) -> usize {
        self.systems
            .iter()
            .map(|system| system.particles.len())
            .fold(0, usize::saturating_add)
    }

    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.systems
            .iter()
            .map(|system| system.dropped)
            .fold(0, u64::saturating_add)
    }

    pub fn advance(&mut self, seconds: f32, ground: &dyn Ground) -> u32 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        self.accumulator = (self.accumulator + seconds).min(1.0);
        let mut steps = 0;
        while self.accumulator >= STEP_SECONDS && steps < MAX_STEPS_PER_CALL {
            self.accumulator -= STEP_SECONDS;
            self.step(ground);
            steps = steps.saturating_add(1);
        }
        if steps == MAX_STEPS_PER_CALL {
            self.accumulator = 0.0;
        }
        steps
    }

    #[must_use]
    pub const fn smooth(&self) -> bool {
        self.smooth
    }

    pub const fn set_smooth(&mut self, on: bool) {
        self.smooth = on;
    }

    #[must_use]
    pub(crate) fn draw_blend(&self) -> f32 {
        if !self.smooth {
            return 1.0;
        }
        let blend = self.accumulator / STEP_SECONDS;
        if blend.is_finite() {
            blend.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    pub fn warm_up(&mut self, seconds: f32, ground: &dyn Ground) -> u32 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let wanted = (seconds / STEP_SECONDS)
            .round()
            .clamp(0.0, f32::from(u16::MAX)) as u32;
        let steps = wanted.min(MAX_WARM_UP_STEPS);
        for _ in 0..steps {
            self.step(ground);
        }
        self.accumulator = 0.0;
        steps
    }

    fn step(&mut self, ground: &dyn Ground) {
        self.steps = self.steps.wrapping_add(1);
        self.counted_live = self.live_split().0;
        let mut index = 0;
        while index < self.systems.len() {
            if self.step_system(index, ground) {
                index = index.saturating_add(1);
                continue;
            }
            let Some(system) = self.take_system(index) else {
                break;
            };
            if let Some(slave) = system.slave {
                self.stop(slave);
            }
            self.free_meshes.extend(system.buffer);
        }
    }

    fn step_system(&mut self, index: usize, ground: &dyn Ground) -> bool {
        let Some(template) = self
            .systems
            .get(index)
            .and_then(|system| self.templates.get(system.template))
        else {
            return false;
        };
        let emission = template.emission;
        let capacity = template.capacity;
        let per_particle = template.per_particle;
        let event = if self.compound {
            template.event.clone()
        } else {
            None
        };

        let Some(system) = self.systems.get_mut(index) else {
            return false;
        };

        let moved = sub(system.origin, system.previous_origin);
        system.previous_origin = system.origin;

        if system.delay_left > 0.0 {
            system.delay_left -= 1.0;
            return true;
        }

        if let Some(limit) = emission.system_lifetime {
            #[allow(clippy::cast_precision_loss)]
            if system.age as f32 >= limit {
                system.emitting = false;
            }
        }

        if system.emitting && !system.driven && !system.suppressed {
            if system.burst_left > 0.0 {
                if !emission.one_shot {
                    if let Some(system) = self.systems.get_mut(index) {
                        system.burst_left -= 1.0;
                    }
                }
            } else {
                let count = burst_size(self.rng.range(emission.burst_count), capacity);
                let bolt = match (emission.volume, self.systems.get(index)) {
                    (Volume::Lightning { start, end, waves }, Some(system)) => {
                        Some(Bolt::roll([start, end], &waves, system, &mut self.rng))
                    }
                    _ => None,
                };
                for number in 0..count {
                    let Some(system) = self.systems.get(index) else {
                        break;
                    };
                    let priority = system.priority;
                    if system.particles.len() >= capacity {
                        if let Some(system) = self.systems.get_mut(index) {
                            system.dropped = system.dropped.saturating_add(1);
                        }
                        continue;
                    }
                    if self.skipped_by_mask(priority) {
                        continue;
                    }
                    if !self.make_room_for(priority) {
                        self.refusals.refused = self.refusals.refused.saturating_add(1);
                        continue;
                    }
                    let Some(system) = self.systems.get(index) else {
                        break;
                    };
                    #[allow(clippy::cast_precision_loss)]
                    let behind = 1.0 - number as f32 / count as f32;
                    let place = Placement {
                        origin: system.origin,
                        radius: Some(system.radius),
                        orientation: system.orientation,
                        lag: scale(moved, behind),
                        bolt_point: bolt.map(|bolt| bolt.point(number, count)),
                    };
                    let mut particle = birth(&emission, &place, &mut self.rng);
                    self.apply_size_bonus(index, &emission, &mut particle);
                    if self.compound {
                        if let Some(attached) = per_particle {
                            let (depth, carrier) =
                                self.systems.get(index).map_or((0, None), |system| {
                                    (system.depth.saturating_add(1), system.orientation)
                                });
                            let orientation = match self.attached_frame {
                                AttachedFrame::World => None,
                                AttachedFrame::Carrier => carrier,
                            };
                            particle.attached = self.spawn_oriented(
                                attached,
                                particle.position,
                                orientation,
                                depth,
                                false,
                                0.0,
                            );
                        }
                    }
                    let slave_born = particle.position;
                    if let Some(system) = self.systems.get_mut(index) {
                        system.particles.push(particle);
                        if !priority.is_exempt() {
                            self.counted_live = self.counted_live.saturating_add(1);
                        }
                    }
                    self.push_slave_particle(index, slave_born);
                }
                let delay = self.rng.range(emission.burst_delay).max(0.0).trunc();
                if let Some(system) = self.systems.get_mut(index) {
                    system.burst_left = delay;
                }
            }
        }

        if let (Volume::Cylinder { radius_rate, .. }, Some(system)) =
            (emission.volume, self.systems.get_mut(index))
        {
            system.radius = advance_radius(system.radius, radius_rate);
        }

        let wind = match &emission.wind {
            None => None,
            Some(spec) => {
                let rng = &mut self.rng;
                let Some(system) = self.systems.get_mut(index) else {
                    return false;
                };
                advance_wind(&mut system.wind, spec, rng);
                wind_push(Some(spec), &system.wind, self.wind, system.origin)
            }
        };

        let motion = self.motion;
        let mut follows = std::mem::take(&mut self.scratch_follows);
        let mut events = std::mem::take(&mut self.scratch_events);
        follows.clear();
        events.clear();
        let Some(system) = self.systems.get_mut(index) else {
            self.scratch_follows = follows;
            self.scratch_events = events;
            return false;
        };
        let before = system.particles.len();
        let pinned_to = emission.motion.attach_to_bone.then_some(system.origin);
        let swirl: &[Vec3] = if emission.motion.swirly {
            &system.swirl
        } else {
            &[]
        };
        system.particles.retain_mut(|particle| {
            let finite = update(particle, &emission, motion, pinned_to, swirl, wind);
            let mut alive = finite && particle.age < particle.lifetime;
            if alive {
                if let Some(event) = &event {
                    if !particle.collided && reached_ground(particle, event, ground) {
                        particle.collided = true;
                        events.push((
                            particle.position,
                            [particle.velocity.x, particle.velocity.y],
                        ));
                        if event.kill_after {
                            alive = false;
                        }
                    }
                }
            }
            match particle.attached {
                Some(attached) if alive => follows.push((attached, particle.position, true)),
                Some(attached) => follows.push((attached, particle.position, false)),
                None => {}
            }
            alive
        });
        let died = before.saturating_sub(system.particles.len());
        let exempt = system.priority.is_exempt();
        system.age = system.age.saturating_add(1);
        let alive = system.emitting || !system.particles.is_empty();
        if !exempt {
            self.counted_live = self.counted_live.saturating_sub(died);
        }
        for index in 0..follows.len() {
            let Some((attached, at, living)) = follows.get(index).copied() else {
                continue;
            };
            if living {
                self.set_origin(attached, at);
            } else {
                self.stop(attached);
            }
        }
        if let Some(event) = &event {
            let depth = self
                .systems
                .get(index)
                .map_or(0, |system| system.depth)
                .saturating_add(1);
            for index in 0..events.len() {
                let Some((at, velocity)) = events.get(index).copied() else {
                    continue;
                };
                self.refusals.collisions = self.refusals.collisions.saturating_add(1);
                self.fire_at_depth(&event.fires, FxFrame::at(at), depth, ground);
                if let Some(follow_up) = event.follow_up {
                    if self.collisions.len() < MAX_PENDING_COLLISIONS {
                        self.collisions.push(Collision {
                            follow_up,
                            at,
                            heading: event.orient.then_some(velocity),
                        });
                    }
                }
            }
        }
        self.scratch_follows = follows;
        self.scratch_events = events;
        alive
    }

    fn push_slave_particle(&mut self, master: usize, at: Vec3) {
        let Some(slave) = self.systems.get(master).and_then(|system| system.slave) else {
            return;
        };
        let Some(slot) = self.slots.get(&slave.0).copied() else {
            return;
        };
        let Some((emission, capacity, priority, offset)) = self
            .systems
            .get(slot)
            .and_then(|system| Some((self.templates.get(system.template)?, system.priority)))
            .map(|(template, priority)| {
                (
                    template.emission,
                    template.capacity,
                    priority,
                    template.slave_offset,
                )
            })
        else {
            return;
        };
        if self
            .systems
            .get(slot)
            .is_none_or(|system| system.particles.len() >= capacity)
        {
            if let Some(system) = self.systems.get_mut(slot) {
                system.dropped = system.dropped.saturating_add(1);
            }
            return;
        }
        if self.skipped_by_mask(priority) {
            return;
        }
        if !self.make_room_for(priority) {
            self.refusals.refused = self.refusals.refused.saturating_add(1);
            return;
        }
        let place = Placement {
            radius: self.systems.get(slot).map(|system| system.radius),
            ..Placement::at(add(at, offset))
        };
        let mut particle = birth(&emission, &place, &mut self.rng);
        let Some(slot) = self.slots.get(&slave.0).copied() else {
            return;
        };
        self.apply_size_bonus(slot, &emission, &mut particle);
        if let Some(system) = self.systems.get_mut(slot) {
            system.particles.push(particle);
            if !priority.is_exempt() {
                self.counted_live = self.counted_live.saturating_add(1);
            }
        }
    }

    fn skipped_by_mask(&mut self, priority: ParticlePriority) -> bool {
        if self.budget.skip_mask == 0 || priority.is_exempt() {
            return false;
        }
        let counter = self.created;
        self.created = self.created.wrapping_add(1);
        #[allow(clippy::cast_possible_truncation)]
        let slot = counter as u32;
        if slot & self.budget.skip_mask == 0 {
            return false;
        }
        self.refusals.skipped = self.refusals.skipped.saturating_add(1);
        true
    }

    fn make_room_for(&mut self, priority: ParticlePriority) -> bool {
        if priority.is_exempt() {
            return true;
        }
        let Some(ceiling) = self.budget.max_particles else {
            return true;
        };
        while self.counted_live >= ceiling {
            let Some(slot) = self.oldest_evictable(priority) else {
                return false;
            };
            let Some(system) = self.systems.get_mut(slot) else {
                return false;
            };
            if system.particles.is_empty() {
                return false;
            }
            system.particles.remove(0);
            self.refusals.evicted = self.refusals.evicted.saturating_add(1);
            self.counted_live = self.counted_live.saturating_sub(1);
        }
        true
    }

    fn oldest_evictable(&self, priority: ParticlePriority) -> Option<usize> {
        let mut best: Option<(usize, u32)> = None;
        for (slot, system) in self.systems.iter().enumerate() {
            if system.priority.is_exempt() || system.priority > priority {
                continue;
            }
            let Some(oldest) = system.particles.first() else {
                continue;
            };
            if best.is_none_or(|(_, age)| oldest.age > age) {
                best = Some((slot, oldest.age));
            }
        }
        best.map(|(slot, _)| slot)
    }

    pub fn draw(
        &mut self,
        renderer: &mut Renderer,
        camera: &Camera,
        out: &mut Vec<Instance>,
    ) -> mint_render::Result<()> {
        let (right, up) = draw::billboard_axes(camera);
        let view = sub(camera.target, camera.eye);
        let toward_camera = if view.length_squared() > MIN_LENGTH_SQUARED {
            scale(view.normalize(), -1.0)
        } else {
            Vec3::Z
        };
        self.ensure_shaders(renderer);
        self.ensure_buffers(renderer)?;
        self.model_draws.clear();
        let mut refused = None;
        for index in 0..self.systems.len() {
            let Some(template) = self.systems.get(index).map(|system| system.template) else {
                continue;
            };
            if self.solo.is_some_and(|solo| template != solo) || self.hidden.contains(&template) {
                continue;
            }
            if let Err(error) = self.draw_system(renderer, index, right, up, toward_camera, out) {
                let subject = self.describe_system(index);
                refused.get_or_insert(error.about(subject));
            }
        }
        refused.map_or(Ok(()), Err)
    }

    #[must_use]
    pub fn model_draws(&self) -> &[ModelDraw] {
        &self.model_draws
    }

    fn describe_system(&self, index: usize) -> String {
        let system = self.systems.get(index);
        let template = system.and_then(|system| self.templates.get(system.template));
        format!(
            "particle system {:?} (slot {index}, {} live particle(s), capacity {})",
            template.map_or("", |template| template.definition.name.as_str()),
            system.map_or(0, |system| system.particles.len()),
            template.map_or(0, |template| template.capacity),
        )
    }

    #[must_use]
    pub fn solo(&self) -> Option<TemplateHandle> {
        self.solo.map(TemplateHandle)
    }

    pub fn set_solo(&mut self, template: Option<TemplateHandle>) {
        self.solo = template.map(|handle| handle.0);
    }

    pub fn set_hidden<I: IntoIterator<Item = TemplateHandle>>(&mut self, templates: I) {
        self.hidden = templates.into_iter().map(|handle| handle.0).collect();
    }

    #[must_use]
    pub fn is_hidden(&self, template: TemplateHandle) -> bool {
        self.hidden.contains(&template.0)
    }

    #[must_use]
    pub fn hidden(&self) -> usize {
        self.hidden.len()
    }

    #[must_use]
    pub fn live_templates(&self) -> Vec<(TemplateHandle, usize)> {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for system in &self.systems {
            let count = counts.entry(system.template).or_insert(0);
            *count = count.saturating_add(1);
        }
        counts
            .into_iter()
            .map(|(template, count)| (TemplateHandle(template), count))
            .collect()
    }

    #[must_use]
    pub fn live_centroids(&self) -> Vec<(TemplateHandle, usize, Vec3)> {
        let mut sums: BTreeMap<usize, (usize, Vec3)> = BTreeMap::new();
        for system in &self.systems {
            let entry = sums.entry(system.template).or_insert((0, Vec3::ZERO));
            for particle in &system.particles {
                entry.0 = entry.0.saturating_add(1);
                entry.1 = Vec3::new(
                    entry.1.x + particle.position.x,
                    entry.1.y + particle.position.y,
                    entry.1.z + particle.position.z,
                );
            }
        }
        sums.into_iter()
            .filter(|(_, (count, _))| *count > 0)
            .map(|(template, (count, sum))| {
                #[allow(clippy::cast_precision_loss)]
                let scale = 1.0 / count as f32;
                let mean = Vec3::new(sum.x * scale, sum.y * scale, sum.z * scale);
                (TemplateHandle(template), count, mean)
            })
            .collect()
    }

    fn ensure_shaders(&mut self, renderer: &mut Renderer) {
        let mut wants_flipbook = false;
        let mut wants_smudge = false;
        for system in &self.systems {
            let Some(template) = self.templates.get(system.template) else {
                continue;
            };
            if system.particles.is_empty() {
                continue;
            }
            match self.shape.shape(template.shape) {
                DrawShape::Smudge => wants_smudge = true,
                _ if template.flipbook.is_some() && self.flipbook.needs_shader() => {
                    wants_flipbook = true;
                }
                _ => {}
            }
        }
        if wants_flipbook && self.shaders.flipbook.is_none() {
            self.shaders.flipbook = renderer
                .register_custom_shader(&CustomShader {
                    label: "particle flipbook",
                    source: FLIPBOOK_SHADER_WGSL,
                    material: CustomMaterialSource::Builtin,
                    ..CustomShader::default()
                })
                .ok();
        }
        if wants_smudge && self.shaders.smudge.is_none() {
            self.shaders.smudge = renderer
                .register_custom_shader(&CustomShader {
                    label: "particle smudge",
                    source: SMUDGE_SHADER_WGSL,
                    material: CustomMaterialSource::Builtin,
                    scene_color: true,
                    ..CustomShader::default()
                })
                .ok();
        }
    }

    fn ensure_buffers(&mut self, renderer: &mut Renderer) -> mint_render::Result<()> {
        for index in 0..self.systems.len() {
            let Some((slot, held, empty)) = self
                .systems
                .get(index)
                .map(|system| (system.template, system.buffer, system.particles.is_empty()))
            else {
                continue;
            };
            if empty {
                continue;
            }
            let Some((shape, capacity)) = self
                .templates
                .get(slot)
                .and_then(|held| Some((self.shape.shape(held.shape).batch()?, held.capacity)))
            else {
                continue;
            };
            if held.is_some_and(|buffer| buffer.shape == shape && buffer.capacity >= capacity) {
                continue;
            }
            let fresh = match self
                .free_meshes
                .iter()
                .position(|pooled| pooled.shape == shape && pooled.capacity == capacity)
            {
                Some(pooled) => self.free_meshes.swap_remove(pooled),
                None => Buffer {
                    shape,
                    capacity,
                    mesh: renderer.upload_mesh(&draw::batch_mesh(shape, capacity))?,
                },
            };
            self.free_meshes.extend(held);
            if let Some(system) = self.systems.get_mut(index) {
                system.buffer = Some(fresh);
                system.filled = capacity;
            }
        }
        Ok(())
    }

    fn draw_system(
        &mut self,
        renderer: &mut Renderer,
        index: usize,
        right: Vec3,
        up: Vec3,
        toward_camera: Vec3,
        out: &mut Vec<Instance>,
    ) -> mint_render::Result<()> {
        let Some(system) = self.systems.get(index) else {
            return Ok(());
        };
        let Some(template) = self.templates.get(system.template) else {
            return Ok(());
        };
        if system.particles.is_empty() {
            return Ok(());
        }
        let shape = self.shape.shape(template.shape);
        if shape == DrawShape::Model {
            self.push_model_instances(index, out);
            return Ok(());
        }
        let Some(Buffer { capacity, mesh, .. }) = system.buffer else {
            return Ok(());
        };
        let blend = self.draw_blend();
        let live = system.particles.len().min(capacity);
        let slots = live.max(system.filled).min(capacity);
        let bounds = match shape {
            DrawShape::Streak => draw::fill_ribbon(
                &mut self.scratch,
                &system.particles,
                system.origin,
                slots,
                toward_camera,
                self.keys,
                blend,
                template
                    .definition
                    .lightning_offset
                    .map(|ranges| RibbonJitter {
                        ranges,
                        frame: system.orientation,
                        seed: system
                            .id
                            .0
                            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                            .wrapping_add(self.steps),
                    })
                    .as_ref(),
            ),
            _ => {
                let ground_aligned = template.ground_aligned && shape != DrawShape::Smudge;
                let (right, up) = draw::quad_axes(self.billboard, ground_aligned, right, up);
                let (right, up) =
                    if self.flat_size == FlatSize::Single && self.billboard.flat(ground_aligned) {
                        (scale(right, 0.5), scale(up, 0.5))
                    } else {
                        (right, up)
                    };
                let smudge_seed = (shape == DrawShape::Smudge).then(|| {
                    let seed = match self.smudge_jitter {
                        SmudgeJitter::Step => system
                            .id
                            .0
                            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                            .wrapping_add(self.steps),
                        SmudgeJitter::Frame => u64::from(self.rng.next_u32()),
                    };
                    seed | 1
                });
                draw::fill_quads(
                    &mut self.scratch,
                    &system.particles,
                    system.origin,
                    slots,
                    &QuadFill {
                        right_axis: right,
                        up_axis: up,
                        keys: self.keys,
                        flipbook: template.flipbook.map(|book| (book, self.flipbook)),
                        smudge_seed,
                        blend,
                    },
                )
            }
        };
        renderer.update_mesh_vertex_prefix(mesh, &self.scratch)?;
        if let Some(system) = self.systems.get_mut(index) {
            system.filled = live;
        }
        let blend = if shape == DrawShape::Smudge {
            Blend::AlphaBlend
        } else {
            template.blend
        };
        let custom_shader = match shape {
            DrawShape::Smudge => self.shaders.smudge,
            _ if template.flipbook.is_some() && self.flipbook.needs_shader() => {
                self.shaders.flipbook
            }
            _ => None,
        };
        let instance = Instance {
            texture: template.texture,
            blend,
            alpha_cutoff: template.alpha_cutoff,
            sort_level: self.sort.level_for(template.sort_level),
            dcg_lighting_disabled: true,
            diffuse_from_vertex_color: true,
            cull: false,
            bounds: Some(bounds),
            custom_shader,
            ..Instance::new(mesh)
        };
        if shape == DrawShape::Volume {
            draw::push_volume_copies(
                out,
                &instance,
                toward_camera,
                template.emission.size.high.max(template.emission.size.low),
            );
        } else {
            out.push(instance);
        }
        Ok(())
    }

    fn push_model_instances(&mut self, index: usize, out: &mut Vec<Instance>) {
        let Some(system) = self.systems.get(index) else {
            return;
        };
        let Some(template) = self.templates.get(system.template) else {
            return;
        };
        if template.parts.is_empty() {
            return;
        }
        let start = out.len();
        let blend = self.draw_blend();
        let mut placements = std::mem::take(&mut self.scratch_placements);
        placements.clear();
        let keys = self.keys;
        let frame = system.orientation;
        let mut keyed: Vec<([f32; 3], f32)> = Vec::with_capacity(system.particles.len());
        let mut ages: Vec<f32> = Vec::with_capacity(system.particles.len());
        for particle in system.particles.iter().take(template.capacity) {
            placements.push(draw::model_transform(
                particle,
                self.draw_blend(),
                frame,
                template.definition.rotation,
            ));
            keyed.push((particle.color, particle.alpha));
            ages.push((particle.age as f32 + blend) * STEP_SECONDS);
        }
        let shader = template.definition.declared_shader;
        for part in &template.parts {
            let alpha_in_colour =
                part.blend != Blend::Additive || self.additive_alpha == AdditiveAlpha::Intensity;
            let built = draw::PartKeys {
                tint: part.tint,
                diffuse: part.material_diffuse,
                emissive: part.material_emissive,
            };
            for (placement, &(colour, alpha)) in placements.iter().zip(keyed.iter()) {
                let material = if keys {
                    draw::part_keys(shader, built, colour, alpha, alpha_in_colour)
                } else {
                    built
                };
                out.push(Instance {
                    transform: placement.mul_mat4(part.transform),
                    tint: material.tint,
                    material_diffuse: material.diffuse,
                    material_emissive: material.emissive,
                    sort_level: self.sort.level_for(template.sort_level),
                    ..*part
                });
            }
        }
        let record = ModelDraw {
            template: TemplateHandle(system.template),
            start,
            placements: placements.clone(),
            ages,
        };
        self.scratch_placements = placements;
        self.model_draws.push(record);
    }
}

fn burst_size(rolled: f32, capacity: usize) -> usize {
    if !rolled.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = rolled.trunc().clamp(0.0, MAX_PARTICLES_PER_SYSTEM as f32) as usize;
    count.min(capacity)
}

fn batch_capacity(definition: &ParticleSystemDef) -> usize {
    capacity_from(
        definition.lifetime,
        definition.burst_count,
        definition.burst_delay,
    )
}

fn capacity_from(
    lifetime: RandomRange,
    burst_count: RandomRange,
    burst_delay: RandomRange,
) -> usize {
    let lifetime = lifetime.high.max(lifetime.low);
    let burst = burst_count.high.max(burst_count.low);
    let delay = burst_delay.low.min(burst_delay.high);
    if !lifetime.is_finite() || !burst.is_finite() {
        return MIN_PARTICLES_PER_SYSTEM;
    }
    let delay = if delay.is_finite() {
        delay.max(1.0)
    } else {
        1.0
    };
    let estimate = lifetime.max(0.0) / delay * burst.max(0.0) + burst.max(0.0);
    if !estimate.is_finite() {
        return MAX_PARTICLES_PER_SYSTEM;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let clamped = estimate.ceil().clamp(
        MIN_PARTICLES_PER_SYSTEM as f32,
        MAX_PARTICLES_PER_SYSTEM as f32,
    ) as usize;
    clamped
}

fn advance_radius(radius: f32, rate: f32) -> f32 {
    let grown = radius + rate;
    if MIN_CYLINDER_RADIUS > grown {
        MIN_CYLINDER_RADIUS
    } else {
        grown
    }
}

fn volume_offset(volume: &Volume, running_radius: Option<f32>, rng: &mut ClientRng) -> Vec3 {
    match *volume {
        Volume::Point => Vec3::ZERO,
        Volume::Line { start, end } | Volume::Lightning { start, end, .. } => {
            let t = rng.unit();
            let axis = |slot: usize| {
                let (a, b) = (
                    start.get(slot).copied().unwrap_or(0.0),
                    end.get(slot).copied().unwrap_or(0.0),
                );
                a + (b - a) * t
            };
            Vec3::new(axis(0), axis(1), axis(2))
        }
        Volume::Box { half_size, hollow } => {
            let half = |slot: usize| half_size.get(slot).copied().unwrap_or(0.0);
            let inside = Vec3::new(
                half(0) * rng.signed(),
                half(1) * rng.signed(),
                half(2) * rng.signed(),
            );
            if !hollow {
                return inside;
            }
            let face = if rng.unit() < 0.5 { -1.0 } else { 1.0 };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let axis_index = (rng.unit() * 3.0) as u32;
            match axis_index {
                0 => Vec3::new(half(0) * face, inside.y, inside.z),
                1 => Vec3::new(inside.x, half(1) * face, inside.z),
                _ => Vec3::new(inside.x, inside.y, half(2) * face),
            }
        }
        Volume::Sphere { radius, hollow } => {
            let [x, y, z] = rng.direction();
            let reach = if hollow { radius } else { radius * rng.unit() };
            Vec3::new(x * reach, y * reach, z * reach)
        }
        Volume::Cylinder {
            radius: authored,
            length,
            offset,
            hollow,
            ..
        } => {
            let radius = running_radius.unwrap_or(authored);
            let [x, y] = rng.direction_xy();
            let reach = if hollow { radius } else { radius * rng.unit() };
            let axis = |slot: usize| offset.get(slot).copied().unwrap_or(0.0);
            Vec3::new(
                x * reach + axis(0),
                y * reach + axis(1),
                length * 0.5 * rng.signed() + axis(2),
            )
        }
    }
}

fn birth_velocity(emission: &Emission, offset: Vec3, rng: &mut ClientRng) -> Vec3 {
    match emission.velocity {
        Velocity::Ortho(axes) => Vec3::new(
            rng.range(axes.first().copied().unwrap_or_default()),
            rng.range(axes.get(1).copied().unwrap_or_default()),
            rng.range(axes.get(2).copied().unwrap_or_default()),
        ),
        Velocity::Spherical(speed) => {
            let [x, y, z] = rng.direction();
            scale(Vec3::new(x, y, z), rng.range(speed))
        }
        Velocity::Hemispherical(speed) => {
            let [x, y, z] = rng.direction();
            scale(Vec3::new(x, y, z.abs()), rng.range(speed))
        }
        Velocity::Cylindrical { radial, normal } => {
            let [x, y] = rng.direction_xy();
            let out = rng.range(radial);
            Vec3::new(x * out, y * out, rng.range(normal))
        }
        Velocity::Outward { speed, other_speed } => {
            let speed = rng.range(speed);
            let other = rng.range(other_speed);
            match emission.volume {
                Volume::Cylinder { .. } => {
                    let flat = normalize_or(Vec3::new(offset.x, offset.y, 0.0), Vec3::ZERO);
                    Vec3::new(flat.x * speed, flat.y * speed, other)
                }
                Volume::Sphere { .. } | Volume::Box { .. } => {
                    scale(normalize_or(offset, Vec3::ZERO), speed)
                }
                Volume::Line { start, end } | Volume::Lightning { start, end, .. } => {
                    let axis = |slot: usize| {
                        end.get(slot).copied().unwrap_or(0.0)
                            - start.get(slot).copied().unwrap_or(0.0)
                    };
                    let along = normalize_or(Vec3::new(axis(0), axis(1), axis(2)), Vec3::X);
                    let side = normalize_or(along.cross(Vec3::Z), Vec3::Y);
                    let up = normalize_or(side.cross(along), Vec3::Z);
                    add(scale(side, speed), scale(up, other))
                }
                Volume::Point => {
                    let [x, y, z] = rng.direction();
                    scale(Vec3::new(x, y, z), speed)
                }
            }
        }
    }
}

fn normalize_or(v: Vec3, fallback: Vec3) -> Vec3 {
    let length_squared = v.length_squared();
    if !length_squared.is_finite() || length_squared < MIN_LENGTH_SQUARED {
        return fallback;
    }
    scale(v, 1.0 / length_squared.sqrt())
}

#[derive(Debug, Clone, Copy)]
struct Bolt {
    start: Vec3,
    end: Vec3,
    side: Vec3,
    waves: [[f32; 3]; 3],
}

impl Bolt {
    fn roll(
        points: [[f32; 3]; 2],
        waves: &[LightningWave; 3],
        system: &LiveSystem,
        rng: &mut ClientRng,
    ) -> Self {
        let turn = |point: [f32; 3]| {
            let local = Vec3::from_array(point);
            system
                .orientation
                .map_or(local, |frame| frame.transform_vector(local))
        };
        let [start, end] = points;
        let start = turn(start);
        let end = system
            .target
            .map_or_else(|| turn(end), |target| sub(target, system.origin));
        let side = Vec3::Z.cross(normalize_or(sub(end, start), Vec3::ZERO));
        Self {
            start,
            end,
            side,
            waves: waves.map(|wave| {
                [
                    rng.range(wave.amplitude),
                    rng.range(wave.frequency),
                    rng.range(wave.phase),
                ]
            }),
        }
    }

    fn point(&self, number: usize, count: usize) -> Vec3 {
        if !(2..=29).contains(&count) {
            return Vec3::ZERO;
        }
        #[allow(clippy::cast_precision_loss)]
        let t = number as f32 / count.saturating_sub(1) as f32;
        let chord = add(self.start, scale(sub(self.end, self.start), t));
        add(chord, scale(self.side, bolt_bend(&self.waves, t)))
    }
}

fn bolt_bend(waves: &[[f32; 3]; 3], t: f32) -> f32 {
    let sum: f32 = waves
        .iter()
        .map(|[amplitude, frequency, phase]| amplitude * mint_math::f32::sin(frequency * t + phase))
        .sum();
    (0.5 - (0.5 - t).abs()) * sum
}

struct Placement {
    origin: Vec3,
    radius: Option<f32>,
    orientation: Option<Mat4>,
    lag: Vec3,
    bolt_point: Option<Vec3>,
}

impl Placement {
    const fn at(origin: Vec3) -> Self {
        Self {
            origin,
            radius: None,
            orientation: None,
            lag: Vec3::ZERO,
            bolt_point: None,
        }
    }
}

fn birth(emission: &Emission, place: &Placement, rng: &mut ClientRng) -> Particle {
    let local = match place.bolt_point {
        Some(_) => Vec3::ZERO,
        None => volume_offset(&emission.volume, place.radius, rng),
    };
    let local_velocity = birth_velocity(emission, local, rng);
    let (offset, velocity) = match place.orientation {
        Some(frame) => (
            frame.transform_vector(local),
            frame.transform_vector(local_velocity),
        ),
        None => (local, local_velocity),
    };
    let offset = place.bolt_point.unwrap_or(offset);
    let origin = place.origin;

    let rolled = rng.range(emission.lifetime);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lifetime = if rolled.is_finite() {
        rolled.round().clamp(0.0, f32::from(u16::MAX)) as u32
    } else {
        0
    };

    let mut alpha_values = [0.0; KEY_SLOTS];
    for (slot, key) in emission.alphas.iter().enumerate() {
        if let Some(held) = alpha_values.get_mut(slot) {
            *held = rng.range(key.value);
        }
    }

    let born = sub(add(origin, offset), place.lag);
    let size = rng.range(emission.size);
    let angle = rng.range(emission.motion.angle_z);
    let (render_size, render_size_rate, render_size_damping) = match &emission.render_size {
        Some(render) => {
            let mut start = [0.0; 3];
            let mut rate = [0.0; 3];
            let mut damping = [0.0; 3];
            for axis in 0..3 {
                if let (Some(s), Some(rt), Some(d)) = (
                    start.get_mut(axis),
                    rate.get_mut(axis),
                    damping.get_mut(axis),
                ) {
                    *s = rng.range(render.start.get(axis).copied().unwrap_or_default());
                    *rt = rng.range(render.rate.get(axis).copied().unwrap_or_default());
                    *d = rng.range(
                        render
                            .damping
                            .get(axis)
                            .copied()
                            .unwrap_or(RandomRange::UNIT),
                    );
                }
            }
            (Some(start), rate, damping)
        }
        None => (None, [0.0; 3], [0.0; 3]),
    };
    let mut particle = Particle {
        position: born,
        previous: born,
        previous_size: size,
        previous_angle: angle,
        velocity,
        size,
        age: 0,
        lifetime,
        color: emission.colors.first().map_or([0.0; 3], |key| key.rgb),
        previous_color: emission.colors.first().map_or([0.0; 3], |key| key.rgb),
        color_rate: [0.0; 3],
        color_slot: 0,
        alpha: alpha_values.first().copied().unwrap_or(0.0),
        previous_alpha: alpha_values.first().copied().unwrap_or(0.0),
        alpha_rate: 0.0,
        alpha_slot: 0,
        alpha_values,
        color_values: birth_colors(emission, rng),
        velocity_damping: rng.range(emission.motion.velocity_damping),
        size_rate: rng.range(emission.motion.size_rate),
        size_rate_damping: rng.range(emission.motion.size_rate_damping),
        angle,
        angular_rate: rng.range(emission.motion.angular_rate_z),
        angular_damping: rng.range(emission.motion.angular_damping),
        wind_jitter: if emission.wind.is_some() {
            rng.range(WIND_PARTICLE_JITTER)
        } else {
            0.0
        },
        sheet_start: sheet_start(born, size, angle, lifetime),
        attached: None,
        collided: false,
        emitter: origin,
        render_size,
        previous_render_size: render_size,
        render_size_rate,
        render_size_damping,
    };
    particle.color_rate = color_rate(emission, &particle.color_values, 0, particle.color);
    particle.alpha_rate = alpha_rate(emission, &alpha_values, 0, particle.alpha);
    particle
}

fn aim_frame(caller: Mat4, primary: Vec3, secondary: Vec3, length: BeamLengthMode) -> Mat4 {
    const TARGET_RAISE: f32 = 10.0;
    let to_target = sub(secondary, primary);
    if !to_target.is_finite() {
        return caller;
    }
    let across = (to_target.x * to_target.x + to_target.y * to_target.y).sqrt();
    let pitch = mint_math::f32::atan2(to_target.z + TARGET_RAISE, across);
    let (sin, cos) = mint_math::f32::sin_cos(std::f32::consts::PI - pitch);
    let reach = match length {
        BeamLengthMode::Authored => 1.0,
        BeamLengthMode::Stretched => to_target.length(),
    };
    caller.mul_mat4(Mat4::from_cols([
        [cos * reach, 0.0, -sin * reach, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [sin, 0.0, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]))
}

fn up_away_from(position: Vec3, emitter: Vec3) -> f32 {
    use std::f32::consts::{PI, TAU};
    let (x, y) = (position.x - emitter.x, position.y - emitter.y);
    if y.abs() < f32::EPSILON {
        return if x > 0.0 { TAU } else { PI };
    }
    let length = (x * x + y * y).sqrt();
    if length == 0.0 || !length.is_finite() {
        return PI;
    }
    let theta = mint_math::f32::acos((y / length).clamp(-1.0, 1.0));
    if x > 0.0 {
        theta + PI
    } else {
        PI - theta
    }
}

fn sheet_start(born: Vec3, size: f32, angle: f32, lifetime: u32) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for word in [
        born.x.to_bits(),
        born.y.to_bits(),
        born.z.to_bits(),
        size.to_bits(),
        angle.to_bits(),
        lifetime,
    ] {
        for byte in word.to_le_bytes() {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    hash
}

impl Particle {
    #[must_use]
    pub(crate) fn drawn(&self, blend: f32) -> (Vec3, f32, f32) {
        let t = if blend.is_finite() {
            blend.clamp(0.0, 1.0)
        } else {
            1.0
        };
        (
            Vec3::new(
                self.previous.x + (self.position.x - self.previous.x) * t,
                self.previous.y + (self.position.y - self.previous.y) * t,
                self.previous.z + (self.position.z - self.previous.z) * t,
            ),
            self.previous_size + (self.size - self.previous_size) * t,
            self.previous_angle + (self.angle - self.previous_angle) * t,
        )
    }

    #[must_use]
    pub(crate) fn drawn_color(&self, blend: f32) -> ([f32; 3], f32) {
        let t = if blend.is_finite() {
            blend.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let mut color = self.previous_color;
        for slot in 0..3 {
            if let (Some(c), Some(p)) = (self.color.get(slot), color.get_mut(slot)) {
                *p += (c - *p) * t;
            }
        }
        (
            color,
            self.previous_alpha + (self.alpha - self.previous_alpha) * t,
        )
    }

    #[must_use]
    pub(crate) fn drawn_render_size(&self, blend: f32) -> Option<[f32; 3]> {
        let t = if blend.is_finite() {
            blend.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let current = self.render_size?;
        let previous = self.previous_render_size.unwrap_or(current);
        let mut out = [0.0; 3];
        for axis in 0..3 {
            if let (Some(o), Some(c), Some(p)) =
                (out.get_mut(axis), current.get(axis), previous.get(axis))
            {
                *o = p + (c - p) * t;
            }
        }
        Some(out)
    }
}

#[derive(Debug, Clone, Copy)]
struct WindPush {
    cos: f32,
    sin: f32,
    strength: f32,
    full: f32,
    zero: f32,
    origin: Vec3,
}

fn advance_wind(state: &mut WindState, spec: &WindSpec, rng: &mut ClientRng) {
    if spec.motion == WindMotion::Circular {
        if state.angle_change == 0.0 {
            state.angle_change = rng.range(spec.angle_change).abs();
        }
        state.angle = (state.angle + state.angle_change.max(MIN_WIND_ANGLE_CHANGE)) % TWO_PI;
        return;
    }
    let (low, high) = ordered(state.start, state.end);
    let half = (high - low) * 0.5;
    let from_edge = (state.angle - low).min(high - state.angle).max(0.0);
    let ease = if half > 0.0 {
        (from_edge / half).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let step = (state.angle_change * ease).max(MIN_WIND_ANGLE_CHANGE);
    state.angle += if state.change >= 0.0 { step } else { -step };
    if state.angle < high && state.angle > low {
        return;
    }
    state.change = -state.change;
    state.angle_change = rng.range(spec.angle_change).abs();
    state.start = rng.range(spec.start);
    state.end = rng.range(spec.end);
}

fn ordered(a: f32, b: f32) -> (f32, f32) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn wind_push(
    spec: Option<&WindSpec>,
    state: &WindState,
    mode: WindMode,
    origin: Vec3,
) -> Option<WindPush> {
    let spec = spec?;
    let base = mode.base()?;
    let (sin, cos) = mint_math::f32::sin_cos(state.angle);
    Some(WindPush {
        cos,
        sin,
        strength: base * spec.strength,
        full: spec.full,
        zero: spec.zero,
        origin,
    })
}

fn wind_falloff(distance: f32, full: f32, zero: f32) -> f32 {
    if distance <= full {
        return 1.0;
    }
    if distance >= zero || zero <= full {
        return 0.0;
    }
    ((zero - distance) / (zero - full)).clamp(0.0, 1.0)
}

fn reached_ground(particle: &Particle, event: &CollisionEvent, ground: &dyn Ground) -> bool {
    if event.fires.is_empty() && !event.kill_after && event.follow_up.is_none() {
        return false;
    }
    let Some(height) = ground.height_at(particle.position.x, particle.position.y) else {
        return false;
    };
    particle.position.z <= height + event.height_offset
}

const SWIRL_GAIN: f32 = 4.0;
const SWIRL_RANGE: f32 = 2000.0;
const SWIRL_SCAN_SLACK: f32 = 100.0;

fn approximate_distance(from: Vec3, to: Vec3) -> f32 {
    let mut gaps = [
        (to.x - from.x).abs(),
        (to.y - from.y).abs(),
        (to.z - from.z).abs(),
    ];
    gaps.sort_by(f32::total_cmp);
    let [min, mid, max] = gaps;
    max + 0.34375 * mid + 0.25 * min
}

fn swirl_tangent(position: Vec3, bones: &[Vec3]) -> Option<Vec3> {
    let mut best: Option<(f32, Vec3)> = None;
    for bone in bones.iter().take(16) {
        let distance = approximate_distance(position, *bone);
        if let Some((nearest, _)) = best {
            if distance > nearest + SWIRL_SCAN_SLACK {
                break;
            }
            if distance >= nearest {
                continue;
            }
        } else if distance >= SWIRL_RANGE {
            continue;
        }
        best = Some((distance, *bone));
    }
    let (_, bone) = best?;
    let (dx, dy) = (bone.x - position.x, bone.y - position.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= 0.0 {
        return None;
    }
    Some(Vec3::new(dy / length, -dx / length, 0.0))
}

fn update(
    particle: &mut Particle,
    emission: &Emission,
    motion: bool,
    pinned_to: Option<Vec3>,
    swirl: &[Vec3],
    wind: Option<WindPush>,
) -> bool {
    particle.previous = particle.position;
    particle.previous_size = particle.size;
    particle.previous_angle = particle.angle;
    particle.previous_color = particle.color;
    particle.previous_alpha = particle.alpha;
    particle.previous_render_size = particle.render_size;
    if motion {
        if let Some(origin) = pinned_to {
            particle.position = origin;
        } else {
            particle.velocity = Vec3::new(
                particle.velocity.x,
                particle.velocity.y,
                particle.velocity.z + emission.motion.gravity,
            );
            particle.velocity = scale(particle.velocity, particle.velocity_damping);
            particle.position = add(
                particle.position,
                add(particle.velocity, emission.motion.drift),
            );
            if let Some(tangent) = swirl_tangent(particle.position, swirl) {
                let v = particle.velocity;
                let speed = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
                particle.position = add(particle.position, scale(tangent, speed * SWIRL_GAIN));
            }
        }
        particle.angle += particle.angular_rate;
        particle.angular_rate *= particle.angular_damping;
        particle.size += particle.size_rate;
        particle.size_rate *= particle.size_rate_damping;
        if let Some(render_size) = particle.render_size.as_mut() {
            for axis in 0..3 {
                if let (Some(s), Some(rt), Some(d)) = (
                    render_size.get_mut(axis),
                    particle.render_size_rate.get_mut(axis),
                    particle.render_size_damping.get(axis),
                ) {
                    *s += *rt;
                    *rt *= *d;
                }
            }
        }
    } else {
        particle.position = pinned_to.unwrap_or_else(|| add(particle.position, particle.velocity));
    }
    if let Some(wind) = wind {
        let offset = sub(particle.position, wind.origin);
        let distance = (offset.x * offset.x + offset.y * offset.y + offset.z * offset.z).sqrt();
        let push =
            wind.strength * particle.wind_jitter * wind_falloff(distance, wind.full, wind.zero);
        particle.position = Vec3::new(
            particle.position.x + wind.cos * push,
            particle.position.y + wind.sin * push,
            particle.position.z,
        );
    }
    if emission.up_towards_emitter {
        particle.angle = up_away_from(particle.position, particle.emitter);
        particle.previous_angle = particle.angle;
    }
    particle.age = particle.age.saturating_add(1);

    if !particle.position.is_finite() || !particle.size.is_finite() {
        return false;
    }

    for slot in 0..3 {
        let (Some(channel), Some(rate)) =
            (particle.color.get_mut(slot), particle.color_rate.get(slot))
        else {
            continue;
        };
        *channel = (*channel + rate).clamp(0.0, 1.0);
    }
    if let Some(next) = reached(emission.color_frames(), particle.color_slot, particle.age) {
        particle.color_slot = next;
        particle.color_rate = color_rate(emission, &particle.color_values, next, particle.color);
    }

    if emission.integrate_alpha {
        particle.alpha = (particle.alpha + particle.alpha_rate).clamp(0.0, 1.0);
        if let Some(next) = reached(emission.alpha_frames(), particle.alpha_slot, particle.age) {
            particle.alpha_slot = next;
            particle.alpha = particle
                .alpha_values
                .get(next)
                .copied()
                .unwrap_or(particle.alpha)
                .clamp(0.0, 1.0);
            particle.alpha_rate =
                alpha_rate(emission, &particle.alpha_values, next, particle.alpha);
        }
    }
    true
}

fn reached(frames: [u32; KEY_SLOTS], current: usize, age: u32) -> Option<usize> {
    let next = current.checked_add(1)?;
    let frame = *frames.get(next)?;
    if frame == 0 || age < frame {
        return None;
    }
    Some(next)
}

fn birth_colors(emission: &Emission, rng: &mut ClientRng) -> [[f32; 3]; KEY_SLOTS] {
    let mut values = [[0.0; 3]; KEY_SLOTS];
    for (held, key) in values.iter_mut().zip(emission.colors.iter()) {
        *held = key.rgb;
    }
    let authored = emission.color_scale;
    if authored.low == 0.0 && authored.high == 0.0 {
        return values;
    }
    let range = RandomRange {
        low: authored.low / 255.0,
        high: authored.high / 255.0,
    };
    for key in values.iter_mut().take(KEY_SLOTS.saturating_sub(1)).skip(1) {
        for channel in key.iter_mut() {
            if *channel != 0.0 {
                *channel = (*channel + rng.range(range)).clamp(0.0, 1.0);
            }
        }
    }
    values
}

fn color_rate(
    emission: &Emission,
    values: &[[f32; 3]; KEY_SLOTS],
    slot: usize,
    from: [f32; 3],
) -> [f32; 3] {
    let frames = emission.color_frames();
    let Some(next) = slot.checked_add(1) else {
        return [0.0; 3];
    };
    let (Some(here), Some(there), Some(target)) =
        (frames.get(slot), frames.get(next), values.get(next))
    else {
        return [0.0; 3];
    };
    if *there == 0 {
        return [0.0; 3];
    }
    let Some(span) = there.checked_sub(*here).filter(|span| *span > 0) else {
        return [0.0; 3];
    };
    #[allow(clippy::cast_precision_loss)]
    let span = span as f32;
    let mut rate = [0.0; 3];
    for slot in 0..3 {
        let (Some(out), Some(to), Some(from)) =
            (rate.get_mut(slot), target.get(slot), from.get(slot))
        else {
            continue;
        };
        *out = (to - from) / span;
    }
    rate
}

fn alpha_rate(emission: &Emission, values: &[f32; KEY_SLOTS], slot: usize, from: f32) -> f32 {
    let frames = emission.alpha_frames();
    let Some(next) = slot.checked_add(1) else {
        return 0.0;
    };
    let (Some(here), Some(there), Some(target)) =
        (frames.get(slot), frames.get(next), values.get(next))
    else {
        return 0.0;
    };
    if *there == 0 {
        return 0.0;
    }
    let Some(span) = there.checked_sub(*here).filter(|span| *span > 0) else {
        return 0.0;
    };
    #[allow(clippy::cast_precision_loss)]
    {
        (target - from) / span as f32
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp
)]
mod tests;
