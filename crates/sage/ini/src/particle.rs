use std::collections::BTreeMap;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Document, Field};

pub const FX_PARTICLE_SYSTEM_KEYWORD: &str = "FXParticleSystem";

pub const PARTICLE_SYSTEM_KEYWORD: &str = "ParticleSystem";

pub const FX_LIST_KEYWORD: &str = "FXList";

pub const FX_LIST_AT_BONE_POS_KEYWORD: &str = "FXListAtBonePos";

pub const CAMERA_SHAKER_VOLUME_KEYWORD: &str = "CameraShakerVolume";

pub const EVA_EVENT_KEYWORD: &str = "EvaEvent";

pub const DYNAMIC_DECAL_KEYWORD: &str = "DynamicDecal";

pub const PARTICLE_DOCUMENTS: [&str; 5] = [
    "data/ini/fxparticlesystem.ini",
    "data/ini/fxparticlesystemcustom.ini",
    "data/ini/particlesystem.ini",
    "data/ini/fxlist.ini",
    "data/ini/default/fxlist.ini",
];

pub const DEFINE_DOCUMENTS: [&str; 1] = ["data/ini/gamedata.ini"];

pub const MODULE_SLOTS: [&str; 11] = [
    "System",
    "EmissionVelocity",
    "EmissionVolume",
    "Color",
    "Update",
    "Draw",
    "Physics",
    "Alpha",
    "Wind",
    "Event",
    "Emitter",
];

const IS_HOLLOW_KEYWORD: &str = "IsHollow";

pub const KEY_SLOTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RandomRange {
    pub low: f32,
    pub high: f32,
}

impl RandomRange {
    pub const UNIT: Self = Self::constant(1.0);

    #[must_use]
    pub const fn constant(value: f32) -> Self {
        Self {
            low: value,
            high: value,
        }
    }

    #[must_use]
    pub fn is_constant(self) -> bool {
        self.low == self.high
    }

    #[must_use]
    pub fn midpoint(self) -> f32 {
        (self.low + self.high) * 0.5
    }

    #[must_use]
    pub fn at(self, t: f32) -> f32 {
        self.low + (self.high - self.low) * t
    }

    #[must_use]
    pub fn parse(args: &[String]) -> Option<Self> {
        match args {
            [one] => parse_number(one).map(Self::constant),
            [low, high] => Some(Self {
                low: parse_number(low)?,
                high: parse_number(high)?,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorKey {
    pub rgb: [f32; 3],
    pub frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AlphaKey {
    pub value: RandomRange,
    pub frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleShader {
    Additive,
    Alpha,
    AlphaTest,
    Multiply,
    W3dEmissive,
    W3dAlpha,
    W3dDiffuse,
    None,
}

impl ParticleShader {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_uppercase().as_str() {
            "ADDITIVE" => Self::Additive,
            "ALPHA" => Self::Alpha,
            "ALPHA_TEST" => Self::AlphaTest,
            "MULTIPLY" => Self::Multiply,
            "W3D_EMISSIVE" => Self::W3dEmissive,
            "W3D_ALPHA" => Self::W3dAlpha,
            "W3D_DIFFUSE" => Self::W3dDiffuse,
            "NONE" => Self::None,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Additive => "ADDITIVE",
            Self::Alpha => "ALPHA",
            Self::AlphaTest => "ALPHA_TEST",
            Self::Multiply => "MULTIPLY",
            Self::W3dEmissive => "W3D_EMISSIVE",
            Self::W3dAlpha => "W3D_ALPHA",
            Self::W3dDiffuse => "W3D_DIFFUSE",
            Self::None => "NONE",
        }
    }

    #[must_use]
    pub fn reads_alpha(self) -> bool {
        !matches!(self, Self::Additive)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ParticlePriority {
    UltraHighOnly,
    HighOrAbove,
    MediumOrAbove,
    LowOrAbove,
    #[default]
    VeryLowOrAbove,
    Unrated,
    AlwaysRender,
}

impl ParticlePriority {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "ALWAYS_RENDER" => Self::AlwaysRender,
            "VERY_LOW_OR_ABOVE" => Self::VeryLowOrAbove,
            "LOW_OR_ABOVE" => Self::LowOrAbove,
            "MEDIUM_OR_ABOVE" => Self::MediumOrAbove,
            "HIGH_OR_ABOVE" => Self::HighOrAbove,
            "ULTRA_HIGH_ONLY" => Self::UltraHighOnly,
            _ => Self::Unrated,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlwaysRender => "ALWAYS_RENDER",
            Self::Unrated => "NONE",
            Self::VeryLowOrAbove => "VERY_LOW_OR_ABOVE",
            Self::LowOrAbove => "LOW_OR_ABOVE",
            Self::MediumOrAbove => "MEDIUM_OR_ABOVE",
            Self::HighOrAbove => "HIGH_OR_ABOVE",
            Self::UltraHighOnly => "ULTRA_HIGH_ONLY",
        }
    }

    #[must_use]
    pub fn is_exempt(self) -> bool {
        matches!(self, Self::AlwaysRender)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticleRotation {
    #[default]
    Off,
    X,
    Y,
    Z,
    Velocity,
}

impl ParticleRotation {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_uppercase().as_str() {
            "ROTATION_OFF" | "NONE" => Self::Off,
            "ROTATE_X" => Self::X,
            "ROTATE_Y" => Self::Y,
            "ROTATE_Z" => Self::Z,
            "ROTATE_V" => Self::Velocity,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticleKind {
    #[default]
    Particle,
    Drawable,
    Streak,
    VolumeParticle,
    Smudge,
    GpuParticle,
    GpuTerrainFire,
    None,
}

impl ParticleKind {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_uppercase().as_str() {
            "PARTICLE" => Self::Particle,
            "DRAWABLE" => Self::Drawable,
            "STREAK" => Self::Streak,
            "VOLUME_PARTICLE" => Self::VolumeParticle,
            "SMUDGE" => Self::Smudge,
            "GPU_PARTICLE" => Self::GpuParticle,
            "GPU_TERRAINFIRE" => Self::GpuTerrainFire,
            "NONE" => Self::None,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Particle => "PARTICLE",
            Self::Drawable => "DRAWABLE",
            Self::Streak => "STREAK",
            Self::VolumeParticle => "VOLUME_PARTICLE",
            Self::Smudge => "SMUDGE",
            Self::GpuParticle => "GPU_PARTICLE",
            Self::GpuTerrainFire => "GPU_TERRAINFIRE",
            Self::None => "NONE",
        }
    }

    #[must_use]
    pub fn is_sprite(self) -> bool {
        matches!(self, Self::Particle | Self::None)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum EmissionVolume {
    #[default]
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
    Unread {
        module: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LightningWave {
    pub amplitude: RandomRange,
    pub frequency: RandomRange,
    pub phase: RandomRange,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum EmissionVelocity {
    #[default]
    Ortho,
    Spherical {
        speed: RandomRange,
    },
    Hemispherical {
        speed: RandomRange,
    },
    Cylindrical {
        radial: RandomRange,
        normal: RandomRange,
    },
    Outward {
        speed: RandomRange,
        other_speed: RandomRange,
    },
    Unread {
        module: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionForm {
    Block,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flipbook {
    pub total_frames: u32,
    pub frames_per_row: u32,
    pub speed_multiplier: f32,
}

impl Flipbook {
    #[must_use]
    pub fn rows(self) -> u32 {
        let per_row = self.frames_per_row.max(1);
        self.total_frames
            .saturating_add(per_row.saturating_sub(1))
            .checked_div(per_row)
            .unwrap_or(1)
            .max(1)
    }

    #[must_use]
    pub fn animates(self) -> bool {
        self.total_frames > 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindMotion {
    #[default]
    None,
    PingPong,
    Circular,
}

impl WindMotion {
    #[must_use]
    pub fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("PingPong") {
            Self::PingPong
        } else if value.eq_ignore_ascii_case("Circular") {
            Self::Circular
        } else {
            Self::None
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::PingPong => "PingPong",
            Self::Circular => "Circular",
        }
    }

    #[must_use]
    pub fn moves(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Wind {
    pub motion: WindMotion,
    pub declared_strength: Option<f32>,
    pub declared_full_strength_dist: Option<f32>,
    pub declared_zero_strength_dist: Option<f32>,
    pub angle_change: RandomRange,
    pub ping_pong_start: RandomRange,
    pub ping_pong_end: RandomRange,
}

pub const DEFAULT_WIND_STRENGTH: f32 = 2.0;

pub const DEFAULT_FULL_STRENGTH_DIST: f32 = 75.0;

pub const DEFAULT_ZERO_STRENGTH_DIST: f32 = 200.0;

impl Wind {
    #[must_use]
    pub fn strength(&self) -> f32 {
        self.declared_strength.unwrap_or(DEFAULT_WIND_STRENGTH)
    }

    #[must_use]
    pub fn full_strength_dist(&self) -> f32 {
        self.declared_full_strength_dist
            .unwrap_or(DEFAULT_FULL_STRENGTH_DIST)
    }

    #[must_use]
    pub fn zero_strength_dist(&self) -> f32 {
        self.declared_zero_strength_dist
            .unwrap_or(DEFAULT_ZERO_STRENGTH_DIST)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TerrainCollisionEvent {
    pub fx: String,
    pub height_offset: RandomRange,
    pub kill_after_event: bool,
    pub orient_to_terrain: bool,
    pub per_particle: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticleSystemDef {
    pub name: String,
    pub form: DefinitionForm,
    pub particle_name: Option<String>,
    pub declared_shader: Option<ParticleShader>,
    pub kind: ParticleKind,
    pub draw_module: Option<String>,
    pub lifetime: RandomRange,
    pub system_lifetime: Option<f32>,
    pub initial_delay: RandomRange,
    pub burst_count: RandomRange,
    pub burst_delay: RandomRange,
    pub size: RandomRange,
    pub one_shot: bool,
    pub ground_aligned: bool,
    pub up_towards_emitter: bool,
    pub declared_priority: Option<ParticlePriority>,
    pub sort_level: Option<u8>,
    pub volume: EmissionVolume,
    pub velocity: EmissionVelocity,
    pub ortho_velocity: [RandomRange; 3],
    pub gravity: f32,
    pub drift_velocity: [f32; 3],
    pub velocity_damping: RandomRange,
    pub particles_attach_to_bone: bool,
    pub swirly: bool,
    pub size_rate: RandomRange,
    pub size_rate_damping: RandomRange,
    pub angle_z: RandomRange,
    pub angular_rate_z: RandomRange,
    pub angular_damping: RandomRange,
    pub rotation: ParticleRotation,
    pub start_size_rate: RandomRange,
    pub colors: [ColorKey; KEY_SLOTS],
    pub colors_authored: usize,
    pub alphas: [AlphaKey; KEY_SLOTS],
    pub alphas_authored: usize,
    pub has_alpha_block: bool,
    pub color_scale: RandomRange,
    pub flipbook: Option<Flipbook>,
    pub lightning_offset: Option<[RandomRange; 3]>,
    pub detail_texture: Option<String>,
    pub slave_system: Option<String>,
    pub slave_pos_offset: [f32; 3],
    pub per_particle_system: Option<String>,
    pub event: Option<TerrainCollisionEvent>,
    pub wind: Option<Wind>,
    pub render_size: Option<[RandomRange; 3]>,
    pub render_size_rate: [RandomRange; 3],
    pub render_size_damping: [RandomRange; 3],
}

impl Default for ParticleSystemDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            form: DefinitionForm::Block,
            particle_name: None,
            declared_shader: None,
            kind: ParticleKind::default(),
            draw_module: None,
            lifetime: RandomRange::default(),
            system_lifetime: None,
            initial_delay: RandomRange::default(),
            burst_count: RandomRange::default(),
            burst_delay: RandomRange::default(),
            size: RandomRange::default(),
            one_shot: false,
            ground_aligned: false,
            up_towards_emitter: false,
            declared_priority: None,
            sort_level: None,
            volume: EmissionVolume::default(),
            velocity: EmissionVelocity::default(),
            ortho_velocity: [RandomRange::default(); 3],
            gravity: 0.0,
            drift_velocity: [0.0; 3],
            velocity_damping: RandomRange::default(),
            particles_attach_to_bone: false,
            swirly: false,
            size_rate: RandomRange::default(),
            size_rate_damping: RandomRange::UNIT,
            angle_z: RandomRange::default(),
            angular_rate_z: RandomRange::default(),
            angular_damping: RandomRange::UNIT,
            rotation: ParticleRotation::Off,
            start_size_rate: RandomRange::default(),
            colors: [ColorKey::default(); KEY_SLOTS],
            colors_authored: 0,
            alphas: [AlphaKey::default(); KEY_SLOTS],
            alphas_authored: 0,
            has_alpha_block: false,
            color_scale: RandomRange::default(),
            flipbook: None,
            lightning_offset: None,
            detail_texture: None,
            slave_system: None,
            slave_pos_offset: [0.0; 3],
            per_particle_system: None,
            event: None,
            wind: None,
            render_size: None,
            render_size_rate: [RandomRange::default(); 3],
            render_size_damping: [RandomRange::UNIT; 3],
        }
    }
}

const COLOR_UNIT: f32 = 255.0;

impl ParticleSystemDef {
    #[must_use]
    pub fn shader(&self) -> ParticleShader {
        self.declared_shader.unwrap_or(ParticleShader::Additive)
    }

    #[must_use]
    pub fn priority(&self) -> ParticlePriority {
        self.declared_priority.unwrap_or(ParticlePriority::Unrated)
    }

    #[must_use]
    pub fn is_sprite_path(&self) -> bool {
        self.kind.is_sprite()
            && !matches!(self.volume, EmissionVolume::Unread { .. })
            && !matches!(self.velocity, EmissionVelocity::Unread { .. })
            && self
                .draw_module
                .as_deref()
                .is_none_or(|module| module.eq("DefaultDraw"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NuggetWeather {
    Normal,
    Snowy,
}

impl NuggetWeather {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_uppercase().as_str() {
            "NORMAL" => Self::Normal,
            "SNOWY" => Self::Snowy,
            _ => return None,
        })
    }
}

#[must_use]
pub fn water_gate_admits(on_land: bool, on_water: bool, is_on_water: bool) -> bool {
    (!on_water || is_on_water) && (!on_land || !is_on_water)
}

#[must_use]
pub fn weather_gate_admits(gate: Option<NuggetWeather>, current: NuggetWeather) -> bool {
    gate.is_none_or(|gate| gate == current)
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NuggetConditions {
    pub source_filter: Option<Vec<String>>,
    pub object_filter: Option<Vec<String>>,
    pub required_source: Vec<String>,
    pub excluded_source: Vec<String>,
    pub required_secondary: Vec<String>,
    pub excluded_secondary: Vec<String>,
    pub stop_if_played: bool,
    pub weather: Option<NuggetWeather>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FxParticleNugget {
    pub offset: [f32; 3],
    pub orient_to_object: bool,
    pub attach_to_object: bool,
    pub create_at_ground_height: bool,
    pub set_target_matrix: bool,
    pub initial_delay: RandomRange,
    pub create_bone: Option<String>,
    pub target_bone: Option<String>,
    pub on_land: bool,
    pub on_water: bool,
    pub use_target_offset: bool,
    pub target_offset: [f32; 3],
}

pub const FX_DELAY_FRAMES_PER_MS: f32 = 0.005;

#[derive(Debug, Clone, PartialEq)]
pub struct FxNugget {
    pub keyword: String,
    pub name: Option<String>,
    pub conditions: NuggetConditions,
    pub particle: Option<FxParticleNugget>,
    pub at_bone_pos: Option<FxAtBonePos>,
    pub camera_shaker: Option<CameraShakerVolume>,
    pub eva_event: Option<FxEvaEvent>,
    pub dynamic_decal: Option<FxDynamicDecal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecalShader {
    #[default]
    Alpha,
    Additive,
    Subtract,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FxDynamicDecal {
    pub texture: String,
    pub shader: DecalShader,
    pub size: f32,
    pub color: [f32; 3],
    pub offset: [f32; 2],
    pub orient_to_object: bool,
    pub opacity: [i64; 3],
    pub fade_ms: [f32; 3],
    pub starting_delay_ms: f32,
    pub lifetime_ms: f32,
}

fn read_dynamic_decal(
    block: &Block,
    refusals: Refusals<'_>,
    macros: &crate::value::MacroTable,
) -> FxDynamicDecal {
    let scalar = |keyword: &str| -> f32 {
        let Some(field) = block.field(keyword) else {
            return 0.0;
        };
        match field
            .value()
            .and_then(|token| crate::value::parse_float_or_macro(token, macros))
        {
            Some(value) => value,
            None => {
                refusals.field(field, "a number");
                0.0
            }
        }
    };
    let shader = match block.field("Shader") {
        None => DecalShader::Alpha,
        Some(field) => match field.value().map(str::to_ascii_uppercase).as_deref() {
            Some("ALPHA") => DecalShader::Alpha,
            Some("ADDITIVE") => DecalShader::Additive,
            Some("SUBTRACT") => DecalShader::Subtract,
            _ => {
                refusals.field(field, "ALPHA, ADDITIVE or SUBTRACT");
                DecalShader::Alpha
            }
        },
    };
    let int = |keyword: &str| -> i64 {
        let Some(field) = block.field(keyword) else {
            return 0;
        };
        match field.value().and_then(crate::value::parse_int) {
            Some(value) => value,
            None => {
                refusals.field(field, "an integer");
                0
            }
        }
    };
    let [r, g, b] = refused_coord(block, "Color", refusals);
    let [x, y, _] = refused_coord(block, "Offset", refusals);
    FxDynamicDecal {
        texture: name_field(block, "DecalName").unwrap_or_default(),
        shader,
        size: scalar("Size"),
        color: [r, g, b],
        offset: [x, y],
        orient_to_object: flag_field_default(block, "OrientToObject", true),
        opacity: [int("OpacityStart"), int("OpacityPeak"), int("OpacityEnd")],
        fade_ms: [
            scalar("OpacityFadeTimeOne"),
            scalar("OpacityPeakTime"),
            scalar("OpacityFadeTimeTwo"),
        ],
        starting_delay_ms: scalar("StartingDelay"),
        lifetime_ms: scalar("Lifetime"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FxEvaEvent {
    pub owner: Option<String>,
    pub ally: Option<String>,
    pub enemy: Option<String>,
}

fn read_eva_event(block: &Block) -> FxEvaEvent {
    FxEvaEvent {
        owner: name_field(block, "EvaEventOwner"),
        ally: name_field(block, "EvaEventAlly"),
        enemy: name_field(block, "EvaEventEnemy"),
    }
}

impl FxNugget {
    #[must_use]
    pub fn is_particle_system(&self) -> bool {
        self.keyword.eq(PARTICLE_SYSTEM_KEYWORD)
    }
}

fn read_nugget_conditions(block: &Block, refusals: Refusals<'_>) -> NuggetConditions {
    NuggetConditions {
        source_filter: optional_token_list(block, "SourceObjectFilter"),
        object_filter: optional_token_list(block, "ObjectFilter"),
        required_source: token_list(block, "RequiredSourceModelConditions"),
        excluded_source: token_list(block, "ExcludedSourceModelConditions"),
        required_secondary: token_list(block, "RequiredSecondaryModelConditions"),
        excluded_secondary: token_list(block, "ExcludedSecondaryModelConditions"),
        stop_if_played: refused_bool(block, "StopIfNuggetPlayed", refusals),
        weather: read_weather_field(block, refusals),
    }
}

fn token_list(block: &Block, keyword: &str) -> Vec<String> {
    block
        .field(keyword)
        .map(|field| field.args.clone())
        .unwrap_or_default()
}

fn optional_token_list(block: &Block, keyword: &str) -> Option<Vec<String>> {
    block.field(keyword).map(|field| field.args.clone())
}

fn refused_bool(block: &Block, keyword: &str, refusals: Refusals<'_>) -> bool {
    let Some(field) = block.field(keyword) else {
        return false;
    };
    match field.value().and_then(crate::value::parse_bool) {
        Some(value) => value,
        None => {
            refusals.field(field, "Yes or No");
            false
        }
    }
}

fn read_particle_nugget(block: &Block, refusals: Refusals<'_>) -> FxParticleNugget {
    FxParticleNugget {
        create_bone: name_field(block, "CreateBoneOverride"),
        target_bone: name_field(block, "TargetBoneOverride"),
        offset: refused_coord(block, "Offset", refusals),
        orient_to_object: flag_field(block, "OrientToObject"),
        attach_to_object: flag_field(block, "AttachToObject"),
        create_at_ground_height: flag_field(block, "CreateAtGroundHeight"),
        set_target_matrix: flag_field(block, "SetTargetMatrix"),
        initial_delay: delay_frames(nugget_delay_millis(block)),
        on_land: flag_field(block, "OnlyIfOnLand"),
        on_water: flag_field(block, "OnlyIfOnWater"),
        use_target_offset: flag_field(block, "UseTargetOffset"),
        target_offset: refused_coord(block, "TargetOffset", refusals),
    }
}

fn read_weather_field(block: &Block, refusals: Refusals<'_>) -> Option<NuggetWeather> {
    let field = block.field("Weather")?;
    let value = field.value().unwrap_or_default();
    match NuggetWeather::parse(value) {
        Some(weather) => Some(weather),
        None => {
            refusals.field(field, "NORMAL or SNOWY");
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FxAtBonePos {
    pub fx: Option<String>,
    pub bone_name: Option<String>,
    pub orient_to_bone: bool,
}

fn name_field(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(crate::tree::Field::value)
        .map(str::to_owned)
}

fn read_at_bone_pos(block: &Block) -> FxAtBonePos {
    FxAtBonePos {
        fx: name_field(block, "FX"),
        bone_name: name_field(block, "BoneName"),
        orient_to_bone: flag_field_default(block, "OrientToBone", true),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CameraShakerVolume {
    pub radius: f32,
    pub duration_seconds: f32,
    pub amplitude_degrees: f32,
}

#[derive(Clone, Copy)]
struct Refusals<'a> {
    errors: &'a ValueErrors,
    context: &'a str,
}

impl Refusals<'_> {
    fn field(self, field: &Field, expected: &'static str) {
        self.errors.field(self.context, field, expected);
    }
}

fn read_camera_shaker(block: &Block, refusals: Refusals<'_>) -> CameraShakerVolume {
    CameraShakerVolume {
        radius: refused_scalar(block, "Radius", refusals),
        duration_seconds: refused_scalar(block, "Duration_Seconds", refusals),
        amplitude_degrees: refused_scalar(block, "Amplitude_Degrees", refusals),
    }
}

fn refused_scalar(block: &Block, keyword: &str, refusals: Refusals<'_>) -> f32 {
    let Some(field) = block.field(keyword) else {
        return 0.0;
    };
    match field.value().and_then(parse_number) {
        Some(value) => value,
        None => {
            refusals.field(field, "a number");
            0.0
        }
    }
}

fn refused_coord(block: &Block, keyword: &str, refusals: Refusals<'_>) -> [f32; 3] {
    let Some(field) = block.field(keyword) else {
        return [0.0; 3];
    };
    let mut out = [0.0; 3];
    let mut slot = 0usize;
    let mut tokens = field.args.iter();
    while let Some(token) = tokens.next() {
        let Some(held) = out.get_mut(slot) else {
            break;
        };
        let text = match token.split_once(':') {
            Some((tag, "")) if tag.len() == 1 => tokens.next().map_or("", String::as_str),
            Some((tag, rest)) if tag.len() == 1 => rest,
            _ => token.as_str(),
        };
        match crate::value::parse_float(text) {
            Some(value) => *held = value,
            None => {
                refusals.field(field, "three numbers");
                return [0.0; 3];
            }
        }
        slot = slot.saturating_add(1);
    }
    out
}

fn nugget_delay_millis(block: &Block) -> RandomRange {
    let Some(field) = block.field("InitialDelay") else {
        return RandomRange::default();
    };
    let (Some(low), Some(high)) = (
        field.args.first().and_then(|token| parse_number(token)),
        field.args.get(1).and_then(|token| parse_number(token)),
    ) else {
        return field
            .args
            .first()
            .and_then(|token| parse_number(token))
            .map_or_else(RandomRange::default, RandomRange::constant);
    };
    RandomRange { low, high }
}

fn delay_frames(millis: RandomRange) -> RandomRange {
    let to_frames = |value: f32| {
        if !value.is_finite() {
            return 0.0;
        }
        (value * FX_DELAY_FRAMES_PER_MS).max(0.0).ceil()
    };
    RandomRange {
        low: to_frames(millis.low),
        high: to_frames(millis.high),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FxList {
    pub name: String,
    pub nuggets: Vec<FxNugget>,
    pub particle_sys_bones: Vec<Vec<String>>,
    pub play_even_if_shrouded: Option<bool>,
    pub culling_info: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParticleIndex {
    systems: BTreeMap<String, ParticleSystemDef>,
    errors: ValueErrors,
    block_definitions: usize,
    flat_definitions: usize,
    shadowed_flat: usize,
    redefinitions: usize,
}

impl ParticleIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_override_document(&mut self, globals: &[Document], map: &Document) {
        let is_system = |block: &Block| {
            block.keyword.eq(FX_PARTICLE_SYSTEM_KEYWORD)
                || block.keyword.eq(PARTICLE_SYSTEM_KEYWORD)
        };
        let mut bases: BTreeMap<String, &Block> = BTreeMap::new();
        for block in globals
            .iter()
            .flat_map(Document::roots)
            .filter(|block| is_system(block))
        {
            if let Some(name) = block.name() {
                bases.insert(name.to_ascii_lowercase(), block);
            }
        }
        let mut order: Vec<String> = Vec::new();
        let mut laid: BTreeMap<String, Block> = BTreeMap::new();
        for block in map.roots().iter().filter(|block| is_system(block)) {
            let Some(name) = block.name() else { continue };
            let key = name.to_ascii_lowercase();
            let next = match laid.get(&key).or_else(|| bases.get(&key).copied()) {
                Some(base) => base.overlaid(block),
                None => block.clone(),
            };
            if laid.insert(key.clone(), next).is_none() {
                order.push(key);
            }
        }
        for block in order.iter().filter_map(|key| laid.get(key)) {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        let Some(name) = block.name() else {
            return;
        };
        let form = if block.keyword.eq(FX_PARTICLE_SYSTEM_KEYWORD) {
            DefinitionForm::Block
        } else if block.keyword.eq(PARTICLE_SYSTEM_KEYWORD) {
            DefinitionForm::Flat
        } else {
            return;
        };

        match form {
            DefinitionForm::Block => {
                self.block_definitions = self.block_definitions.wrapping_add(1)
            }
            DefinitionForm::Flat => self.flat_definitions = self.flat_definitions.wrapping_add(1),
        }

        let key = name.to_ascii_lowercase();
        if form == DefinitionForm::Flat
            && self
                .systems
                .get(&key)
                .is_some_and(|held| held.form == DefinitionForm::Block)
        {
            self.shadowed_flat = self.shadowed_flat.wrapping_add(1);
            return;
        }
        let definition = read_definition(
            block,
            form,
            Refusals {
                errors: &self.errors,
                context: name,
            },
        );
        if let Some(held) = self.systems.insert(key, definition) {
            if held.form == form {
                self.redefinitions = self.redefinitions.wrapping_add(1);
            } else {
                self.shadowed_flat = self.shadowed_flat.wrapping_add(1);
            }
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ParticleSystemDef> {
        self.systems.get(&name.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParticleSystemDef)> {
        self.systems
            .iter()
            .map(|(key, definition)| (key.as_str(), definition))
    }

    #[must_use]
    pub const fn value_errors(&self) -> &ValueErrors {
        &self.errors
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    #[must_use]
    pub fn block_definitions(&self) -> usize {
        self.block_definitions
    }

    #[must_use]
    pub fn flat_definitions(&self) -> usize {
        self.flat_definitions
    }

    #[must_use]
    pub fn shadowed_flat(&self) -> usize {
        self.shadowed_flat
    }

    #[must_use]
    pub fn redefinitions(&self) -> usize {
        self.redefinitions
    }
}

#[derive(Debug, Clone, Default)]
pub struct FxListIndex {
    lists: BTreeMap<String, FxList>,
    errors: ValueErrors,
    macros: crate::value::MacroTable,
    definitions: usize,
    redefinitions: usize,
}

impl FxListIndex {
    pub fn insert_defines(&mut self, defines: &[crate::tree::Define]) {
        self.macros.insert_defines(defines);
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if !block.keyword.eq(FX_LIST_KEYWORD) {
            return;
        }
        let Some(name) = block.name() else {
            return;
        };
        self.definitions = self.definitions.wrapping_add(1);
        let list = FxList {
            name: name.to_owned(),
            nuggets: block
                .blocks()
                .map(|nugget| FxNugget {
                    keyword: nugget.keyword.clone(),
                    name: nugget
                        .field("Name")
                        .and_then(|field| field.value())
                        .map(str::to_owned),
                    conditions: read_nugget_conditions(
                        nugget,
                        Refusals {
                            errors: &self.errors,
                            context: name,
                        },
                    ),
                    particle: nugget.keyword.eq(PARTICLE_SYSTEM_KEYWORD).then(|| {
                        read_particle_nugget(
                            nugget,
                            Refusals {
                                errors: &self.errors,
                                context: name,
                            },
                        )
                    }),
                    at_bone_pos: nugget
                        .keyword
                        .eq(FX_LIST_AT_BONE_POS_KEYWORD)
                        .then(|| read_at_bone_pos(nugget)),
                    camera_shaker: nugget.keyword.eq(CAMERA_SHAKER_VOLUME_KEYWORD).then(|| {
                        read_camera_shaker(
                            nugget,
                            Refusals {
                                errors: &self.errors,
                                context: name,
                            },
                        )
                    }),
                    eva_event: nugget
                        .keyword
                        .eq(EVA_EVENT_KEYWORD)
                        .then(|| read_eva_event(nugget)),
                    dynamic_decal: nugget.keyword.eq(DYNAMIC_DECAL_KEYWORD).then(|| {
                        read_dynamic_decal(
                            nugget,
                            Refusals {
                                errors: &self.errors,
                                context: name,
                            },
                            &self.macros,
                        )
                    }),
                })
                .collect(),
            particle_sys_bones: block
                .fields_named("ParticleSysBone")
                .map(|field| field.args.clone())
                .collect(),
            play_even_if_shrouded: block
                .field("PlayEvenIfShrouded")
                .and_then(|field| field.value())
                .and_then(crate::value::parse_bool),
            culling_info: block
                .field("CullingInfo")
                .and_then(|field| field.value())
                .map(str::to_owned),
        };
        if self.lists.insert(name.to_ascii_lowercase(), list).is_some() {
            self.redefinitions = self.redefinitions.wrapping_add(1);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&FxList> {
        self.lists.get(&name.to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &FxList)> {
        self.lists.iter().map(|(key, list)| (key.as_str(), list))
    }

    pub fn particle_references(&self) -> impl Iterator<Item = (&str, &str)> {
        self.lists.values().flat_map(|list| {
            list.nuggets
                .iter()
                .filter(|nugget| nugget.is_particle_system())
                .filter_map(move |nugget| Some((list.name.as_str(), nugget.name.as_deref()?)))
        })
    }

    #[must_use]
    pub const fn value_errors(&self) -> &ValueErrors {
        &self.errors
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lists.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lists.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }

    #[must_use]
    pub fn redefinitions(&self) -> usize {
        self.redefinitions
    }
}

fn read_definition(
    block: &Block,
    form: DefinitionForm,
    refusals: Refusals<'_>,
) -> ParticleSystemDef {
    let mut definition = ParticleSystemDef {
        name: block.name().unwrap_or_default().to_owned(),
        form,
        ..ParticleSystemDef::default()
    };
    match form {
        DefinitionForm::Flat => {
            read_system_fields(block, &mut definition, refusals);
            read_motion_fields(block, &mut definition, refusals);
            read_flat_emitter(block, &mut definition, refusals);
            read_color_keys(block, &mut definition);
            read_alpha_keys(block, &mut definition);
            definition.has_alpha_block = true;
            definition.wind = block
                .field("WindMotion")
                .map(|_| read_wind(block, refusals));
        }
        DefinitionForm::Block => read_modules(block, &mut definition, refusals),
    }
    definition
}

fn read_modules(block: &Block, definition: &mut ParticleSystemDef, refusals: Refusals<'_>) {
    for module in block.blocks() {
        let selector = module.selector().unwrap_or_default().to_owned();
        let keyword = module.keyword.as_str();
        if keyword.eq("System") {
            read_system_fields(module, definition, refusals);
        } else if keyword.eq("Color") {
            read_color_keys(module, definition);
        } else if keyword.eq("Alpha") {
            definition.has_alpha_block = true;
            read_alpha_keys(module, definition);
        } else if keyword.eq("Physics") || keyword.eq("Update") {
            read_motion_fields(module, definition, refusals);
        } else if keyword.eq("Draw") {
            read_draw_fields(module, &selector, definition, refusals);
            definition.draw_module = Some(selector);
        } else if keyword.eq("Event") {
            definition.event = read_event(module, &selector, refusals);
        } else if keyword.eq("Wind") {
            definition.wind = Some(read_wind(module, refusals));
        } else if keyword.eq("EmissionVolume") {
            definition.volume = read_volume(module, &selector, refusals);
        } else if keyword.eq("EmissionVelocity") {
            read_velocity(module, &selector, definition, refusals);
        } else if keyword.eq("Emitter") {
            read_flat_emitter(module, definition, refusals);
        }
    }
}

fn read_system_fields(block: &Block, definition: &mut ParticleSystemDef, refusals: Refusals<'_>) {
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        let value = field.value().unwrap_or_default();
        if keyword.eq("ParticleName") {
            definition.particle_name = Some(value.to_owned());
        } else if keyword.eq("Shader") {
            definition.declared_shader = ParticleShader::parse(value);
        } else if keyword.eq("Type") {
            if let Some(kind) = ParticleKind::parse(value) {
                definition.kind = kind;
            }
        } else if keyword.eq("Lifetime") {
            set_range(&mut definition.lifetime, &field.args);
        } else if keyword.eq("SystemLifetime") {
            definition.system_lifetime = RandomRange::parse(&field.args).map(RandomRange::midpoint);
        } else if keyword.eq("InitialDelay") {
            set_range(&mut definition.initial_delay, &field.args);
        } else if keyword.eq("BurstCount") {
            set_range(&mut definition.burst_count, &field.args);
        } else if keyword.eq("BurstDelay") {
            set_range(&mut definition.burst_delay, &field.args);
        } else if keyword.eq("Size") {
            set_range(&mut definition.size, &field.args);
        } else if keyword.eq("IsOneShot") {
            set_flag(&mut definition.one_shot, value);
        } else if keyword.eq("IsGroundAligned") {
            set_flag(&mut definition.ground_aligned, value);
        } else if keyword.eq("IsParticleUpTowardsEmitter") {
            set_flag(&mut definition.up_towards_emitter, value);
        } else if keyword.eq("Priority") {
            definition.declared_priority = Some(ParticlePriority::parse(value));
        } else if keyword.eq("SlaveSystem") {
            definition.slave_system = Some(value.to_owned());
        } else if keyword.eq("SlavePosOffset") {
            definition.slave_pos_offset = refused_coord(block, "SlavePosOffset", refusals);
        } else if keyword.eq("PerParticleAttachedSystem") {
            definition.per_particle_system = Some(value.to_owned());
        } else if keyword.eq("SortLevel") {
            definition.sort_level =
                crate::value::parse_int(value).and_then(|level| u8::try_from(level).ok());
        } else if keyword.eq("StartSizeRate") {
            set_range(&mut definition.start_size_rate, &field.args);
        }
    }
}

fn read_color_keys(block: &Block, definition: &mut ParticleSystemDef) {
    for field in block.fields() {
        if field.keyword.eq("ColorScale") {
            set_range(&mut definition.color_scale, &field.args);
            continue;
        }
        let Some(slot) = key_slot(&field.keyword, "Color") else {
            continue;
        };
        let Some(key) = parse_color_key(&field.args) else {
            continue;
        };
        if let Some(held) = definition.colors.get_mut(slot) {
            *held = key;
            definition.colors_authored = definition.colors_authored.wrapping_add(1);
        }
    }
}

fn read_alpha_keys(block: &Block, definition: &mut ParticleSystemDef) {
    for field in block.fields() {
        let Some(slot) = key_slot(&field.keyword, "Alpha") else {
            continue;
        };
        let ([low, high, frame] | [low, high, frame, ..]) = field.args.as_slice() else {
            continue;
        };
        let (Some(low), Some(high)) = (parse_number(low), parse_number(high)) else {
            continue;
        };
        if let Some(held) = definition.alphas.get_mut(slot) {
            *held = AlphaKey {
                value: RandomRange { low, high },
                frame: parse_frame(frame),
            };
            definition.alphas_authored = definition.alphas_authored.wrapping_add(1);
        }
    }
}

fn read_draw_fields(
    block: &Block,
    module: &str,
    definition: &mut ParticleSystemDef,
    refusals: Refusals<'_>,
) {
    if module.eq("LightningDraw") {
        definition.lightning_offset =
            Some(["OffsetX", "OffsetY", "OffsetZ"].map(|axis| range_field(block, axis, refusals)));
    }
    definition.detail_texture = block
        .field("DetailTexture")
        .and_then(crate::tree::Field::value)
        .map(str::to_owned);
    let (Some(total), Some(per_row)) = (
        block
            .field("TotalFrames")
            .and_then(crate::tree::Field::value)
            .and_then(parse_count),
        block
            .field("FramesPerRow")
            .and_then(crate::tree::Field::value)
            .and_then(parse_count),
    ) else {
        return;
    };
    let Some(speed) = block
        .field("SpeedMultiplier")
        .and_then(crate::tree::Field::value)
        .and_then(parse_number)
    else {
        return;
    };
    definition.flipbook = Some(Flipbook {
        total_frames: total,
        frames_per_row: per_row,
        speed_multiplier: speed,
    });
}

fn read_event(
    block: &Block,
    module: &str,
    refusals: Refusals<'_>,
) -> Option<TerrainCollisionEvent> {
    if !module.eq("TerrainCollision") {
        return None;
    }
    let fx = block
        .field("EventFX")
        .and_then(crate::tree::Field::value)?
        .to_owned();
    Some(TerrainCollisionEvent {
        fx,
        height_offset: range_field(block, "HeightOffset", refusals),
        kill_after_event: flag_field(block, "KillAfterEvent"),
        orient_to_terrain: flag_field(block, "OrientFXToTerrain"),
        per_particle: flag_field(block, "PerParticle"),
    })
}

fn read_wind(block: &Block, refusals: Refusals<'_>) -> Wind {
    Wind {
        motion: block
            .field("WindMotion")
            .and_then(crate::tree::Field::value)
            .map_or(WindMotion::None, WindMotion::parse),
        declared_strength: optional_scalar(block, "WindStrength"),
        declared_full_strength_dist: optional_scalar(block, "WindFullStrengthDist"),
        declared_zero_strength_dist: optional_scalar(block, "WindZeroStrengthDist"),
        angle_change: min_max_pair(block, "WindAngleChange", refusals),
        ping_pong_start: min_max_pair(block, "WindPingPongStartAngle", refusals),
        ping_pong_end: min_max_pair(block, "WindPingPongEndAngle", refusals),
    }
}

fn min_max_pair(block: &Block, prefix: &str, refusals: Refusals<'_>) -> RandomRange {
    RandomRange {
        low: refused_scalar(block, &format!("{prefix}Min"), refusals),
        high: refused_scalar(block, &format!("{prefix}Max"), refusals),
    }
}

fn optional_scalar(block: &Block, keyword: &str) -> Option<f32> {
    block
        .field(keyword)
        .and_then(crate::tree::Field::value)
        .and_then(parse_number)
}

fn flag_field(block: &Block, keyword: &str) -> bool {
    flag_field_default(block, keyword, false)
}

fn flag_field_default(block: &Block, keyword: &str, default: bool) -> bool {
    block
        .field(keyword)
        .and_then(crate::tree::Field::value)
        .and_then(crate::value::parse_bool)
        .unwrap_or(default)
}

fn parse_count(token: &str) -> Option<u32> {
    token.trim().parse::<u32>().ok().filter(|count| *count > 0)
}

fn read_volume(block: &Block, module: &str, refusals: Refusals<'_>) -> EmissionVolume {
    let hollow = || flag_field(block, IS_HOLLOW_KEYWORD);
    if module.eq("PointEmissionVolume") {
        EmissionVolume::Point
    } else if module.eq("LineEmissionVolume") {
        EmissionVolume::Line {
            start: refused_coord(block, "StartPoint", refusals),
            end: refused_coord(block, "EndPoint", refusals),
        }
    } else if module.eq("BoxEmissionVolume") {
        EmissionVolume::Box {
            half_size: refused_coord(block, "HalfSize", refusals),
            hollow: hollow(),
        }
    } else if module.eq("SphereEmissionVolume") {
        EmissionVolume::Sphere {
            radius: refused_scalar(block, "Radius", refusals),
            hollow: hollow(),
        }
    } else if module.eq("CylinderEmissionVolume") {
        EmissionVolume::Cylinder {
            radius: refused_scalar(block, "Radius", refusals),
            length: refused_scalar(block, "Length", refusals),
            offset: refused_coord(block, "Offset", refusals),
            radius_rate: refused_scalar(block, "RadiusRate", refusals),
            hollow: hollow(),
        }
    } else if module.eq("LightningEmission") {
        let range = |keyword: String| range_field(block, &keyword, refusals);
        EmissionVolume::Lightning {
            start: refused_coord(block, "StartPoint", refusals),
            end: refused_coord(block, "EndPoint", refusals),
            waves: [1, 2, 3].map(|n| LightningWave {
                amplitude: range(format!("Amplitude{n}")),
                frequency: range(format!("Frequency{n}")),
                phase: range(format!("Phase{n}")),
            }),
        }
    } else {
        EmissionVolume::Unread {
            module: module.to_owned(),
        }
    }
}

fn read_velocity(
    block: &Block,
    module: &str,
    definition: &mut ParticleSystemDef,
    refusals: Refusals<'_>,
) {
    let range = |keyword: &str| range_field(block, keyword, refusals);
    definition.velocity = if module.eq("OrthoEmissionVelocity") {
        for (slot, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
            if let (Some(field), Some(held)) =
                (block.field(axis), definition.ortho_velocity.get_mut(slot))
            {
                set_range(held, &field.args);
            }
        }
        EmissionVelocity::Ortho
    } else if module.eq("SphericalEmissionVelocity") {
        EmissionVelocity::Spherical {
            speed: range("Speed"),
        }
    } else if module.eq("HemisphericalEmissionVelocity") {
        EmissionVelocity::Hemispherical {
            speed: range("Speed"),
        }
    } else if module.eq("CylindricalEmissionVelocity") {
        EmissionVelocity::Cylindrical {
            radial: range("Radial"),
            normal: range("Normal"),
        }
    } else if module.eq("OutwardEmissionVelocity") {
        EmissionVelocity::Outward {
            speed: range("Speed"),
            other_speed: range("OtherSpeed"),
        }
    } else {
        EmissionVelocity::Unread {
            module: module.to_owned(),
        }
    };
}

fn read_motion_fields(block: &Block, definition: &mut ParticleSystemDef, refusals: Refusals<'_>) {
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if keyword.eq("Gravity") {
            if let Some(value) = field.value().and_then(parse_number) {
                definition.gravity = value;
            }
        } else if keyword.eq("DriftVelocity") {
            definition.drift_velocity = refused_coord(block, "DriftVelocity", refusals);
        } else if keyword.eq("VelocityDamping") {
            set_range(&mut definition.velocity_damping, &field.args);
        } else if keyword.eq("Swirly") {
            match field.value().and_then(crate::value::parse_bool) {
                Some(flag) => definition.swirly = flag,
                None => refusals.field(field, "Yes or No"),
            }
        } else if keyword.eq("ParticlesAttachToBone") {
            match field.value().and_then(crate::value::parse_bool) {
                Some(flag) => definition.particles_attach_to_bone = flag,
                None => refusals.field(field, "Yes or No"),
            }
        } else if keyword.eq("SizeRate") {
            set_range(&mut definition.size_rate, &field.args);
        } else if keyword.eq("SizeRateDamping") {
            set_range(&mut definition.size_rate_damping, &field.args);
        } else if keyword.eq("AngleZ") {
            set_range(&mut definition.angle_z, &field.args);
        } else if keyword.eq("AngularRateZ") {
            set_range(&mut definition.angular_rate_z, &field.args);
        } else if keyword.eq("AngularDamping") {
            set_range(&mut definition.angular_damping, &field.args);
        } else if keyword.eq("Rotation") {
            match field.value().and_then(ParticleRotation::parse) {
                Some(rotation) => definition.rotation = rotation,
                None => refusals.field(
                    field,
                    "ROTATION_OFF, ROTATE_X, ROTATE_Y, ROTATE_Z or ROTATE_V",
                ),
            }
        } else if let Some(slot) = render_size_slot(definition, keyword) {
            set_range(slot, &field.args);
        }
    }
}

fn render_size_slot<'a>(
    definition: &'a mut ParticleSystemDef,
    keyword: &str,
) -> Option<&'a mut RandomRange> {
    let (family, axis) = ["StartSize", "SizeRate", "SizeDamping"]
        .into_iter()
        .find_map(|prefix| render_size_axis(keyword, prefix).map(|axis| (prefix, axis)))?;
    let start = definition
        .render_size
        .get_or_insert([RandomRange::default(); 3]);
    match family {
        "StartSize" => start.get_mut(axis),
        "SizeRate" => definition.render_size_rate.get_mut(axis),
        _ => definition.render_size_damping.get_mut(axis),
    }
}

fn render_size_axis(keyword: &str, prefix: &str) -> Option<usize> {
    let suffix = keyword.strip_prefix(prefix)?;
    match suffix {
        "X" => Some(0),
        "Y" => Some(1),
        "Z" => Some(2),
        _ => None,
    }
}

fn read_flat_emitter(block: &Block, definition: &mut ParticleSystemDef, refusals: Refusals<'_>) {
    let volume_type = block
        .field("VolumeType")
        .and_then(|field| field.value())
        .unwrap_or("NONE");
    let hollow = flag_field(block, IS_HOLLOW_KEYWORD);
    definition.volume = match volume_type.to_ascii_uppercase().as_str() {
        "NONE" | "POINT" => EmissionVolume::Point,
        "LINE" => EmissionVolume::Line {
            start: refused_coord(block, "VolLineStart", refusals),
            end: refused_coord(block, "VolLineEnd", refusals),
        },
        "BOX" => EmissionVolume::Box {
            half_size: refused_coord(block, "VolBoxHalfSize", refusals),
            hollow,
        },
        "SPHERE" => EmissionVolume::Sphere {
            radius: refused_scalar(block, "VolSphereRadius", refusals),
            hollow,
        },
        "CYLINDER" => EmissionVolume::Cylinder {
            radius: refused_scalar(block, "VolCylinderRadius", refusals),
            length: refused_scalar(block, "VolCylinderLength", refusals),
            offset: [0.0; 3],
            radius_rate: 0.0,
            hollow,
        },
        other => EmissionVolume::Unread {
            module: other.to_owned(),
        },
    };

    let velocity_type = block
        .field("VelocityType")
        .and_then(|field| field.value())
        .unwrap_or("NONE");
    match velocity_type.to_ascii_uppercase().as_str() {
        "NONE" | "ORTHO" => {
            definition.velocity = EmissionVelocity::Ortho;
            for (slot, axis) in ["VelOrthoX", "VelOrthoY", "VelOrthoZ"]
                .into_iter()
                .enumerate()
            {
                if let (Some(field), Some(held)) =
                    (block.field(axis), definition.ortho_velocity.get_mut(slot))
                {
                    set_range(held, &field.args);
                }
            }
        }
        "SPHERICAL" => {
            definition.velocity = EmissionVelocity::Spherical {
                speed: range_field(block, "VelSpherical", refusals),
            };
        }
        "HEMISPHERICAL" => {
            definition.velocity = EmissionVelocity::Hemispherical {
                speed: range_field(block, "VelHemispherical", refusals),
            };
        }
        "CYLINDRICAL" => {
            definition.velocity = EmissionVelocity::Cylindrical {
                radial: range_field(block, "VelCylindricalRadial", refusals),
                normal: range_field(block, "VelCylindricalNormal", refusals),
            };
        }
        "OUTWARD" => {
            definition.velocity = EmissionVelocity::Outward {
                speed: range_field(block, "VelOutward", refusals),
                other_speed: range_field(block, "VelOutwardOther", refusals),
            };
        }
        other => {
            definition.velocity = EmissionVelocity::Unread {
                module: other.to_owned(),
            };
        }
    }
}

fn range_field(block: &Block, keyword: &str, refusals: Refusals<'_>) -> RandomRange {
    let Some(field) = block.field(keyword) else {
        return RandomRange::default();
    };
    match RandomRange::parse(&field.args) {
        Some(range) => range,
        None => {
            refusals.field(field, "one or two numbers");
            RandomRange::default()
        }
    }
}

fn key_slot(keyword: &str, prefix: &str) -> Option<usize> {
    let digits = keyword.get(prefix.len()..)?;
    if !keyword.get(..prefix.len())?.eq(prefix) {
        return None;
    }
    let index: usize = digits.parse().ok()?;
    index.checked_sub(1).filter(|slot| *slot < KEY_SLOTS)
}

fn parse_color_key(args: &[String]) -> Option<ColorKey> {
    let ([r, g, b, frame] | [r, g, b, frame, ..]) = args else {
        return None;
    };
    Some(ColorKey {
        rgb: [
            parse_number(r)? / COLOR_UNIT,
            parse_number(g)? / COLOR_UNIT,
            parse_number(b)? / COLOR_UNIT,
        ],
        frame: parse_frame(frame),
    })
}

fn set_range(held: &mut RandomRange, args: &[String]) {
    if let Some(range) = RandomRange::parse(args) {
        *held = range;
    }
}

fn parse_number(token: &str) -> Option<f32> {
    let text = match token.split_once(':') {
        Some((prefix, rest)) if prefix.len() == 1 => rest,
        _ => token,
    };
    crate::value::parse_float(text)
}

fn parse_frame(token: &str) -> u32 {
    if let Ok(frame) = token.parse::<u32>() {
        return frame;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    token
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map_or(0, |value| value.clamp(0.0, f64::from(u32::MAX)) as u32)
}

fn set_flag(slot: &mut bool, value: &str) {
    if let Some(flag) = crate::value::parse_bool(value) {
        *slot = flag;
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]
mod tests;
