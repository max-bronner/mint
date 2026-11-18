use std::collections::{BTreeMap, BTreeSet};

use crate::tree::{Block, Document, Field};

pub const OBJECT_KEYWORDS: [&str; 3] = ["Object", "ChildObject", "ObjectReskin"];

pub const DRAW_KEYWORD: &str = "Draw";
const ADD_MODULE_KEYWORD: &str = "AddModule";
const REPLACE_MODULE_KEYWORD: &str = "ReplaceModule";
const REMOVE_MODULE_KEYWORD: &str = "RemoveModule";

pub const ROTATION_ANCHOR_KEYWORD: &str = "GeometryRotationAnchorOffset";

pub const TREE_DRAW_MODULE: &str = "W3DTreeDraw";

pub const TAINTED_TREE_KEYWORD: &str = "TaintedTree";

#[derive(Debug, Clone, PartialEq)]
pub struct TreeTopple {
    pub topple_fx: Option<String>,
    pub bounce_fx: Option<String>,
    pub kill_when_finished: bool,
    pub initial_velocity: f32,
    pub initial_accel: f32,
    pub bounce_velocity: f32,
    pub minimum_speed: f32,
    pub sink_distance: f32,
    pub sink_time_ms: f32,
}

impl Default for TreeTopple {
    fn default() -> Self {
        Self {
            topple_fx: None,
            bounce_fx: None,
            kill_when_finished: true,
            initial_velocity: 0.2,
            initial_accel: 0.01,
            bounce_velocity: 0.3,
            minimum_speed: 0.5,
            sink_distance: 20.0,
            sink_time_ms: 10_000.0,
        }
    }
}

fn draw_tree_topple(draw: &Block, macros: &crate::value::MacroTable) -> TreeTopple {
    let defaults = TreeTopple::default();
    let fx = |keyword| {
        draw.field(keyword)
            .and_then(Field::value)
            .filter(|name| !crate::value::is_none(name))
            .map(str::to_owned)
    };
    let real = |keyword, default| {
        draw.field(keyword)
            .and_then(|field| crate::value::field_expression(field, macros))
            .unwrap_or(default)
    };
    let percent = |keyword, default| {
        draw.field(keyword)
            .and_then(|field| crate::value::resolve_percent(&field.args, macros))
            .unwrap_or(default)
    };
    TreeTopple {
        topple_fx: fx("ToppleFX"),
        bounce_fx: fx("BounceFX"),
        kill_when_finished: draw
            .field("KillWhenFinishedToppling")
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            .unwrap_or(defaults.kill_when_finished),
        initial_velocity: percent("InitialVelocityPercent", defaults.initial_velocity),
        initial_accel: percent("InitialAccelPercent", defaults.initial_accel),
        bounce_velocity: percent("BounceVelocityPercent", defaults.bounce_velocity),
        minimum_speed: real("MinimumToppleSpeed", defaults.minimum_speed),
        sink_distance: real("SinkDistance", defaults.sink_distance),
        sink_time_ms: real("SinkTime", defaults.sink_time_ms),
    }
}

pub const STREAK_DRAW_MODULE: &str = "W3DStreakDraw";

pub const FLOOR_DRAW_MODULE: &str = "W3DFloorDraw";

pub const WEATHER_TEXTURE_KEYWORD: &str = "WeatherTexture";

pub const START_HIDDEN_KEYWORD: &str = "StartHidden";

pub const FORCE_TO_BACK_KEYWORD: &str = "ForceToBack";

pub const ATTACH_TO_BONE_KEYWORD: &str = "AttachToBoneInAnotherModule";

pub const OK_TO_CHANGE_MODEL_COLOR_KEYWORD: &str = "OkToChangeModelColor";

pub const TRUCK_DRAW_MODULE: &str = "w3dtruckdraw";

pub const TIRE_ROTATION_MULTIPLIER_KEYWORD: &str = "TireRotationMultiplier";

pub const TIRE_BONE_KEYWORDS: &[&str] = &[
    "LeftFrontTireBone",
    "RightFrontTireBone",
    "LeftRearTireBone",
    "RightRearTireBone",
    "MidLeftFrontTireBone",
    "MidRightFrontTireBone",
    "MidLeftRearTireBone",
    "MidRightRearTireBone",
    "MidLeftMidTireBone",
    "MidRightMidTireBone",
];

pub const MODEL_NAME_KEYWORD: &str = "ModelName";

pub const WALL_BOUNDS_MESH_KEYWORD: &str = "WallBoundsMesh";

pub const RAMP_MESH_1_KEYWORD: &str = "RampMesh1";

pub const RAMP_MESH_2_KEYWORD: &str = "RampMesh2";

pub const RAISED_WALL_MESH_KEYWORD: &str = "RaisedWallMesh";

pub const MODEL_KEYWORD: &str = "Model";

pub const HIDE_IF_MODEL_CONDITIONS_KEYWORD: &str = "HideIfModelConditions";

pub const EXTRA_MESH_FLAG: &str = "ExtraMesh";

pub const MODEL_STATE_KEYWORDS: [&str; 2] = ["DefaultModelConditionState", "ModelConditionState"];

pub const IDLE_ANIMATION_STATE_KEYWORD: &str = "IdleAnimationState";

pub const DEFAULT_MODEL_STATE_KEYWORD: &str = "DefaultModelConditionState";

pub const PARTICLE_SYS_BONE_KEYWORD: &str = "ParticleSysBone";

pub const ANIMATION_STATE_KEYWORDS: [&str; 2] = ["AnimationState", "TransitionState"];

const FOLLOW_BONE_MODIFIER: &str = "FollowBone";

pub const PARTICLES_ATTACHED_TO_ANIMATED_BONES_KEYWORD: &str = "ParticlesAttachedToAnimatedBones";

pub const ANIMATION_KEYWORD: &str = "Animation";

pub const ANIMATION_FIELDS: [&str; 3] = ["AnimationName", "AnimationMode", "AnimationBlendTime"];

pub const ANIMATION_FLAGS_KEYWORD: &str = "Flags";

pub const NONE_VALUE: &str = "NONE";

pub const ROAD_FIELDS: [&str; 3] = ["Texture", "RoadWidth", "RoadWidthInTexture"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadDefinition {
    pub texture: Option<String>,
    pub width: Option<f32>,
    pub width_in_texture: Option<f32>,
}

pub const MAX_INHERITANCE_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelNaming {
    ModelName,
    ConditionState,
    Neither,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NoModelReason {
    ModelIsNone,
    NonDrawingModule,
    NoDrawModule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnimationMode {
    Manual,
    Once,
    OnceBackwards,
    Loop,
    LoopBackwards,
    PingPong,
}

impl AnimationMode {
    pub const NAMES: [(&'static str, AnimationMode); 7] = [
        ("MANUAL", AnimationMode::Manual),
        ("ONCE", AnimationMode::Once),
        ("ONCE_BACKWARDS", AnimationMode::OnceBackwards),
        ("LOOP", AnimationMode::Loop),
        ("LOOP_BACKWARDS", AnimationMode::LoopBackwards),
        ("PING_PONG", AnimationMode::PingPong),
        ("LOOP_PINGPONG", AnimationMode::PingPong),
    ];

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        Self::NAMES
            .iter()
            .find(|(name, _)| value.eq_ignore_ascii_case(name))
            .map(|(_, mode)| *mode)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AnimationMode::Manual => "MANUAL",
            AnimationMode::Once => "ONCE",
            AnimationMode::OnceBackwards => "ONCE_BACKWARDS",
            AnimationMode::Loop => "LOOP",
            AnimationMode::LoopBackwards => "LOOP_BACKWARDS",
            AnimationMode::PingPong => "PING_PONG",
        }
    }

    #[must_use]
    pub fn repeats(self) -> bool {
        matches!(
            self,
            AnimationMode::Loop | AnimationMode::LoopBackwards | AnimationMode::PingPong
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnimationFlags {
    pub random_start: bool,
    pub start_frame_first: bool,
    pub start_frame_last: bool,
    pub maintain_frame: u8,
    pub restart_when_complete: bool,
}

impl AnimationFlags {
    pub const NAMES: [&'static str; 8] = [
        "RANDOMSTART",
        "START_FRAME_FIRST",
        "START_FRAME_LAST",
        "MAINTAIN_FRAME_ACROSS_STATES",
        "RESTART_ANIM_WHEN_COMPLETE",
        "MAINTAIN_FRAME_ACROSS_STATES2",
        "MAINTAIN_FRAME_ACROSS_STATES3",
        "MAINTAIN_FRAME_ACROSS_STATES4",
    ];

    const MAINTAIN_FRAME_GROUPS: [(&'static str, u8); 4] = [
        ("MAINTAIN_FRAME_ACROSS_STATES", 1),
        ("MAINTAIN_FRAME_ACROSS_STATES2", 2),
        ("MAINTAIN_FRAME_ACROSS_STATES3", 4),
        ("MAINTAIN_FRAME_ACROSS_STATES4", 8),
    ];

    #[must_use]
    pub fn parse(value: &str) -> Self {
        let mut flags = Self::default();
        let mut zeroed = false;
        for token in value.split_whitespace() {
            if token.eq_ignore_ascii_case("NONE") {
                return Self::default();
            }
            let (name, set) = match token.split_at_checked(1) {
                Some(("+", rest)) => (rest, true),
                Some(("-", rest)) => (rest, false),
                _ => {
                    if !zeroed {
                        flags = Self::default();
                        zeroed = true;
                    }
                    (token, true)
                }
            };
            if name.eq_ignore_ascii_case("RANDOMSTART") {
                flags.random_start = set;
            } else if name.eq_ignore_ascii_case("START_FRAME_FIRST") {
                flags.start_frame_first = set;
            } else if name.eq_ignore_ascii_case("START_FRAME_LAST") {
                flags.start_frame_last = set;
            } else if name.eq_ignore_ascii_case("RESTART_ANIM_WHEN_COMPLETE") {
                flags.restart_when_complete = set;
            } else if let Some(&(_, bit)) = Self::MAINTAIN_FRAME_GROUPS
                .iter()
                .find(|(group, _)| name.eq_ignore_ascii_case(group))
            {
                if set {
                    flags.maintain_frame |= bit;
                } else {
                    flags.maintain_frame &= !bit;
                }
            }
        }
        flags
    }

    #[must_use]
    pub fn any(self) -> bool {
        self.random_start
            || self.start_frame_first
            || self.start_frame_last
            || self.maintain_frame != 0
            || self.restart_when_complete
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnimationSource {
    IdleAnimationState,
    ModelConditionState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultAnimation {
    pub name: String,
    pub mode: Option<AnimationMode>,
    pub blend_frames: Option<u32>,
    pub flags: AnimationFlags,
    pub source: AnimationSource,
}

impl DefaultAnimation {
    #[must_use]
    pub fn asset(&self) -> &str {
        match self.name.rsplit_once('.') {
            Some((_, asset)) => asset,
            None => &self.name,
        }
    }

    #[must_use]
    pub fn hierarchy(&self) -> Option<&str> {
        self.name.rsplit_once('.').map(|(hierarchy, _)| hierarchy)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParticleSysBone {
    pub bone: Option<String>,
    pub system: String,
    pub follow_bone: bool,
    pub module_flag: bool,
    pub persist: Persist,
    pub persist_id: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Persist {
    #[default]
    None,
    Hold,
    Kill,
    Spawn,
}

impl Persist {
    fn from_token(value: &str) -> Option<Self> {
        [
            ("NONE", Self::None),
            ("HOLD", Self::Hold),
            ("KILL", Self::Kill),
            ("SPAWN", Self::Spawn),
        ]
        .into_iter()
        .find_map(|(name, mode)| value.eq_ignore_ascii_case(name).then_some(mode))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParticleBones {
    pub default: Vec<ParticleSysBone>,
    pub idle: Vec<ParticleSysBone>,
    pub conditioned: Vec<ParticleSysBone>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TruckTires {
    pub bones: Vec<String>,
    pub rotation_multiplier: f32,
}

impl ParticleBones {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.default.is_empty() && self.idle.is_empty() && self.conditioned.is_empty()
    }

    pub fn standing(&self) -> impl Iterator<Item = &ParticleSysBone> {
        self.default.iter()
    }

    pub fn all(&self) -> impl Iterator<Item = &ParticleSysBone> {
        self.default
            .iter()
            .chain(self.idle.iter())
            .chain(self.conditioned.iter())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template<'a> {
    Model(&'a str),
    NoModel(NoModelReason),
    Road,
    Terrain,
    Undeclared,
}

impl Template<'_> {
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        match *self {
            Template::Model(name) => Some(name),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_object(&self) -> bool {
        matches!(*self, Template::Model(_) | Template::NoModel(_))
    }
}

#[must_use]
pub fn module_model_name(draw: &Block) -> Option<&str> {
    draw.field(MODEL_NAME_KEYWORD)
        .and_then(|f| f.value())
        .filter(|name| !name.eq_ignore_ascii_case(NONE_VALUE))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StreakDraw {
    pub length: Option<f32>,
    pub width: Option<f32>,
    pub segments: Option<u32>,
    pub color: Option<crate::multiplayer_color::Rgb8>,
    pub texture: Option<String>,
    pub additive: Option<bool>,
    pub weather_textures: Vec<(crate::NuggetWeather, String)>,
}

impl StreakDraw {
    fn patch_under(&mut self, ancestor: &StreakDraw) {
        self.length = self.length.or(ancestor.length);
        self.width = self.width.or(ancestor.width);
        self.segments = self.segments.or(ancestor.segments);
        self.color = self.color.or(ancestor.color);
        if self.texture.is_none() {
            self.texture.clone_from(&ancestor.texture);
        }
        self.additive = self.additive.or(ancestor.additive);
        if self.weather_textures.is_empty() {
            self.weather_textures.clone_from(&ancestor.weather_textures);
        }
    }
}

#[must_use]
pub fn draw_streak(draw: &Block) -> Option<StreakDraw> {
    if !draw
        .args
        .first()
        .is_some_and(|module| module.eq(STREAK_DRAW_MODULE))
    {
        return None;
    }
    let number =
        |keyword: &str| -> Option<f32> { crate::value::parse_float(draw.field(keyword)?.value()?) };
    Some(StreakDraw {
        length: number("Length"),
        width: number("Width"),
        segments: number("NumSegments").and_then(|n| {
            if n.is_finite() && n >= 1.0 && n <= f32::from(u16::MAX) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Some(n as u32)
            } else {
                None
            }
        }),
        color: crate::multiplayer_color::rgb_field(draw, "Color"),
        texture: draw
            .field("Texture")
            .and_then(Field::value)
            .filter(|name| !name.eq_ignore_ascii_case(NONE_VALUE))
            .map(str::to_owned),
        additive: draw
            .field("Additive")
            .and_then(Field::value)
            .map(|value| value.eq_ignore_ascii_case("Yes")),
        weather_textures: draw_weather_textures(draw),
    })
}

#[must_use]
pub fn first_model_state(draw: &Block) -> Option<&Block> {
    draw.blocks()
        .find(|b| MODEL_STATE_KEYWORDS.iter().any(|k| b.keyword.eq(k)))
}

pub(crate) fn state_models(state: &Block) -> impl Iterator<Item = (&str, bool)> {
    state
        .fields_named(MODEL_KEYWORD)
        .filter_map(|field| {
            let name = field.value()?;
            let extra = field.args.iter().skip(1).any(|argument| {
                argument.split_once(':').is_some_and(|(flag, value)| {
                    flag.eq_ignore_ascii_case(EXTRA_MESH_FLAG) && value.eq_ignore_ascii_case("Yes")
                })
            });
            Some((name, extra))
        })
        .filter(|(name, _)| !name.eq_ignore_ascii_case(NONE_VALUE))
}

#[must_use]
pub fn draw_model(draw: &Block) -> Option<&str> {
    if let Some(name) = module_model_name(draw) {
        return Some(name);
    }
    let state = first_model_state(draw)?;
    state_models(state)
        .find(|(_, extra)| !extra)
        .or_else(|| state_models(state).next())
        .map(|(name, _)| name)
}

#[must_use]
pub fn draw_model_alternates(draw: &Block) -> Vec<&str> {
    let Some(state) = first_model_state(draw) else {
        return Vec::new();
    };
    state_models(state)
        .filter(|(_, extra)| *extra)
        .map(|(name, _)| name)
        .collect()
}

pub const MAX_HIDE_IF_CONDITIONS: usize = 64;

#[must_use]
pub fn draw_hide_if_model_conditions(draw: &Block) -> Vec<String> {
    draw.fields_named(HIDE_IF_MODEL_CONDITIONS_KEYWORD)
        .filter_map(crate::tree::Field::value)
        .take(MAX_HIDE_IF_CONDITIONS)
        .map(str::to_ascii_uppercase)
        .collect()
}

#[must_use]
pub fn draw_weather_textures(draw: &Block) -> Vec<(crate::NuggetWeather, String)> {
    draw.fields_named(WEATHER_TEXTURE_KEYWORD)
        .take(MAX_HIDE_IF_CONDITIONS)
        .filter_map(|field| {
            let weather = crate::NuggetWeather::parse(field.args.first()?)?;
            Some((weather, field.args.get(1)?.clone()))
        })
        .collect()
}

#[must_use]
pub fn first_idle_state(draw: &Block) -> Option<&Block> {
    draw.blocks()
        .find(|b| b.keyword.eq(IDLE_ANIMATION_STATE_KEYWORD))
}

fn state_animation(state: &Block, source: AnimationSource) -> Option<DefaultAnimation> {
    let [name_field, mode_field, blend_field] = ANIMATION_FIELDS;
    let animation = state.blocks().find(|b| b.keyword.eq(ANIMATION_KEYWORD))?;
    let value = |keyword: &str| animation.field(keyword).and_then(crate::tree::Field::value);
    let name = value(name_field)?.trim();
    if name.is_empty() {
        return None;
    }
    Some(DefaultAnimation {
        name: name.to_owned(),
        mode: value(mode_field).and_then(AnimationMode::parse),
        blend_frames: value(blend_field).and_then(|text| text.trim().parse().ok()),
        flags: state
            .field(ANIMATION_FLAGS_KEYWORD)
            .and_then(crate::tree::Field::value)
            .map(AnimationFlags::parse)
            .unwrap_or_default(),
        source,
    })
}

#[must_use]
pub fn draw_animation(draw: &Block) -> Option<DefaultAnimation> {
    if let Some(idle) = first_idle_state(draw) {
        if let Some(animation) = state_animation(idle, AnimationSource::IdleAnimationState) {
            return Some(animation);
        }
    }
    state_animation(
        first_model_state(draw)?,
        AnimationSource::ModelConditionState,
    )
}

#[must_use]
pub fn draw_particle_bones(draw: &Block) -> ParticleBones {
    let module_flag = draw
        .field(PARTICLES_ATTACHED_TO_ANIMATED_BONES_KEYWORD)
        .and_then(Field::value)
        .and_then(crate::value::parse_bool)
        == Some(true);
    let mut bones = ParticleBones::default();
    for state in draw.blocks() {
        if state_kind(state) == StateKind::Other {
            continue;
        }
        let unconditional = state.keyword.eq(DEFAULT_MODEL_STATE_KEYWORD)
            || state.args.is_empty()
            || state
                .args
                .iter()
                .all(|token| token.eq_ignore_ascii_case(NONE_VALUE));
        let into = match (state_kind(state), unconditional) {
            (StateKind::Model, true) => &mut bones.default,
            (StateKind::Idle, _) => &mut bones.idle,
            _ => &mut bones.conditioned,
        };
        for field in state.fields() {
            if !field.keyword.eq(PARTICLE_SYS_BONE_KEYWORD) {
                continue;
            }
            if let Some(mut attachment) = parse_particle_sys_bone(&field.args) {
                attachment.module_flag = module_flag;
                into.push(attachment);
            }
        }
    }
    bones
}

#[must_use]
pub fn draw_truck_tires(draw: &Block, module: Option<&str>) -> Option<TruckTires> {
    if module != Some(TRUCK_DRAW_MODULE) {
        return None;
    }
    let bones: Vec<String> = TIRE_BONE_KEYWORDS
        .iter()
        .filter_map(|keyword| {
            draw.field(keyword)
                .and_then(Field::value)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
        })
        .collect();
    if bones.is_empty() {
        return None;
    }
    let rotation_multiplier = draw
        .field(TIRE_ROTATION_MULTIPLIER_KEYWORD)
        .and_then(Field::value)
        .and_then(crate::value::parse_float)
        .unwrap_or(0.0);
    Some(TruckTires {
        bones,
        rotation_multiplier,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Model,
    Idle,
    Animation,
    Other,
}

fn state_kind(state: &Block) -> StateKind {
    if MODEL_STATE_KEYWORDS
        .iter()
        .any(|keyword| state.keyword.eq(keyword))
    {
        StateKind::Model
    } else if state.keyword.eq(IDLE_ANIMATION_STATE_KEYWORD) {
        StateKind::Idle
    } else if ANIMATION_STATE_KEYWORDS
        .iter()
        .any(|keyword| state.keyword.eq(keyword))
    {
        StateKind::Animation
    } else {
        StateKind::Other
    }
}

pub fn parse_particle_sys_bone(args: &[String]) -> Option<ParticleSysBone> {
    let bone = args.first()?;
    let system = args.get(1)?;
    if system.is_empty() {
        return None;
    }
    Some(ParticleSysBone {
        bone: if bone.eq_ignore_ascii_case(NONE_VALUE) {
            None
        } else {
            Some(bone.clone())
        },
        system: system.clone(),
        follow_bone: args.iter().skip(2).any(|token| {
            token
                .split(':')
                .next()
                .is_some_and(|word| word.eq_ignore_ascii_case(FOLLOW_BONE_MODIFIER))
        }),
        module_flag: false,
        persist: modifier(args, "Persist")
            .and_then(Persist::from_token)
            .unwrap_or_default(),
        persist_id: modifier(args, "PersistID")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
    })
}

fn modifier<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().skip(2).find_map(|token| {
        let (key, value) = token.split_once(':')?;
        key.eq_ignore_ascii_case(name).then_some(value)
    })
}

#[must_use]
pub fn model_naming(draw: &Block) -> ModelNaming {
    let has_name = draw.field(MODEL_NAME_KEYWORD).is_some();
    let has_state = first_model_state(draw).is_some();
    match (has_name, has_state) {
        (true, true) => ModelNaming::Both,
        (true, false) => ModelNaming::ModelName,
        (false, true) => ModelNaming::ConditionState,
        (false, false) => ModelNaming::Neither,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DrawFacet {
    module: Option<String>,
    model: Option<String>,
    offered_a_model: bool,
    alternates: Vec<String>,
    animation: Option<DefaultAnimation>,
    animation_states: Vec<crate::anim_state::AnimationStateDef>,
    model_states: Vec<crate::model_state::ModelStateDef>,
    particle_bones: ParticleBones,
    vegetation: bool,
    tainted: bool,
    topple: Option<TreeTopple>,
    streak: Option<StreakDraw>,
    hide_if: Vec<String>,
    weather_textures: Vec<(crate::NuggetWeather, String)>,
    start_hidden: bool,
    force_to_back: bool,
    ok_to_change_model_color: bool,
    attach_to_bone: Option<String>,
    wall_bounds_mesh: Option<String>,
    ramp_mesh_1: Option<String>,
    ramp_mesh_2: Option<String>,
    raised_wall_mesh: Option<String>,
    truck_tires: Option<TruckTires>,
}

impl DrawFacet {
    fn names_state_model(&self) -> bool {
        self.model_states.iter().any(|state| state.model.is_some())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ObjectEntry {
    draws: Vec<(Option<String>, DrawFacet)>,
    reason: NoModelReason,
    parent: Option<String>,
    evicted: Vec<String>,
    sounds: BTreeMap<String, String>,
    unit_specific_sounds: Option<BTreeMap<String, String>>,
    unit_specific_fx: Option<BTreeMap<String, String>>,
    rotation_anchor: Option<[f32; 2]>,
    crowd_response_key: Option<String>,
    animation_sound: Option<crate::client_behavior::AnimationSoundBehavior>,
    animation_sound_errors: Vec<crate::client_behavior::AnimationSoundError>,
}

impl ObjectEntry {
    fn patch(&mut self, over: ObjectEntry) {
        self.draws
            .retain(|(tag, _)| tag.as_ref().is_none_or(|tag| !over.evicted.contains(tag)));
        self.draws.extend(over.draws);
        self.evicted.extend(over.evicted);
        self.reason = no_model_reason(&self.draws);
        self.sounds.extend(over.sounds);
        if over.unit_specific_sounds.is_some() {
            self.unit_specific_sounds = over.unit_specific_sounds;
        }
        if over.unit_specific_fx.is_some() {
            self.unit_specific_fx = over.unit_specific_fx;
        }
        if over.rotation_anchor.is_some() {
            self.rotation_anchor = over.rotation_anchor;
        }
        if over.crowd_response_key.is_some() {
            self.crowd_response_key = over.crowd_response_key;
        }
        if over.animation_sound.is_some() {
            self.animation_sound = over.animation_sound;
        }
    }
}

fn reduce_draws<'a>(
    facets: impl IntoIterator<Item = &'a DrawFacet>,
) -> (
    Option<&'a str>,
    &'a [String],
    Option<&'a DefaultAnimation>,
    &'a [crate::anim_state::AnimationStateDef],
    &'a [crate::model_state::ModelStateDef],
    ParticleBones,
    bool,
    Option<&'a StreakDraw>,
    &'a [String],
) {
    let mut model = None;
    let mut alternates: &[String] = &[];
    let mut animation = None;
    let mut animation_states: &[crate::anim_state::AnimationStateDef] = &[];
    let mut model_states: &[crate::model_state::ModelStateDef] = &[];
    let mut particle_bones = ParticleBones::default();
    let mut vegetation = false;
    let mut streak = None;
    let mut hide_if: &[String] = &[];
    for facet in facets {
        if model.is_none() && facet.model.is_some() {
            model = facet.model.as_deref();
            alternates = &facet.alternates;
            model_states = &facet.model_states;
            hide_if = &facet.hide_if;
        }
        if animation.is_none() {
            animation = facet.animation.as_ref();
        }
        if animation_states.is_empty() {
            animation_states = &facet.animation_states;
        }
        particle_bones
            .default
            .extend(facet.particle_bones.default.iter().cloned());
        particle_bones
            .idle
            .extend(facet.particle_bones.idle.iter().cloned());
        particle_bones
            .conditioned
            .extend(facet.particle_bones.conditioned.iter().cloned());
        vegetation |= facet.vegetation;
        if streak.is_none() {
            streak = facet.streak.as_ref();
        }
    }
    (
        model,
        alternates,
        animation,
        animation_states,
        model_states,
        particle_bones,
        vegetation,
        streak,
        hide_if,
    )
}

#[derive(Debug, Clone, Default)]
pub struct ObjectIndex {
    entries: BTreeMap<String, ObjectEntry>,
    road_names: BTreeMap<String, RoadDefinition>,
    terrain_names: BTreeSet<String>,
    objects: usize,
    roads: usize,
    terrains: usize,
    redefinitions: usize,
    animation_sound_failures: Vec<crate::client_behavior::AnimationSoundError>,
}

const MAX_ANIMATION_SOUND_FAILURES: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct DrawModuleView<'a> {
    pub tag: Option<&'a str>,
    pub module: Option<&'a str>,
    pub model: Option<&'a str>,
    pub is_model_module: bool,
    pub alternates: &'a [String],
    pub model_states: &'a [crate::model_state::ModelStateDef],
    pub animation_states: &'a [crate::anim_state::AnimationStateDef],
    pub hide_if: &'a [String],
    pub weather_textures: &'a [(crate::NuggetWeather, String)],
    pub start_hidden: bool,
    pub force_to_back: bool,
    pub ok_to_change_model_color: bool,
    pub attach_to_bone: Option<&'a str>,
    pub wall_bounds_mesh: Option<&'a str>,
    pub ramp_mesh_1: Option<&'a str>,
    pub ramp_mesh_2: Option<&'a str>,
    pub raised_wall_mesh: Option<&'a str>,
}

impl DrawModuleView<'_> {
    #[must_use]
    pub fn is_floor(&self) -> bool {
        self.module
            .is_some_and(|kind| kind.eq_ignore_ascii_case(FLOOR_DRAW_MODULE))
    }
}

impl ObjectIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_document(&mut self, document: &Document) {
        self.insert_document_with(document, &crate::value::MacroTable::new());
    }

    pub fn insert_document_with(&mut self, document: &Document, macros: &crate::value::MacroTable) {
        for block in document.roots() {
            self.insert_block(block, macros);
        }
    }

    pub fn insert_block(&mut self, block: &Block, macros: &crate::value::MacroTable) {
        let Some(name) = block.name() else {
            return;
        };
        let key = name.to_ascii_lowercase();

        if block.keyword.eq("Road") {
            self.roads = self.roads.saturating_add(1);
            self.road_names.insert(key, road_definition(block));
        } else if block.keyword.eq("Terrain") {
            self.terrains = self.terrains.saturating_add(1);
            self.terrain_names.insert(key);
        } else if OBJECT_KEYWORDS.iter().any(|k| block.keyword.eq(k)) {
            self.objects = self.objects.saturating_add(1);
            let mut entry = analyse(block, macros);
            for error in entry.animation_sound_errors.drain(..) {
                if self.animation_sound_failures.len() < MAX_ANIMATION_SOUND_FAILURES {
                    self.animation_sound_failures.push(error);
                }
            }
            if self.entries.insert(key, entry).is_some() {
                self.redefinitions = self.redefinitions.saturating_add(1);
            }
        }
    }

    pub fn insert_override_document(
        &mut self,
        document: &Document,
        macros: &crate::value::MacroTable,
    ) -> usize {
        let mut patched = 0usize;
        for block in document.roots() {
            let key = block.name().map(str::to_ascii_lowercase);
            let Some(entry) = key
                .filter(|_| block.keyword.eq("Object"))
                .and_then(|key| self.entries.get_mut(&key))
            else {
                self.insert_block(block, macros);
                continue;
            };
            let mut patch = analyse(block, macros);
            for error in patch.animation_sound_errors.drain(..) {
                if self.animation_sound_failures.len() < MAX_ANIMATION_SOUND_FAILURES {
                    self.animation_sound_failures.push(error);
                }
            }
            entry.patch(patch);
            patched = patched.saturating_add(1);
        }
        patched
    }

    fn chain(&self, key: &str) -> Vec<&ObjectEntry> {
        let mut out = Vec::new();
        let mut cur = key.to_owned();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let Some(entry) = self.entries.get(&cur) else {
                break;
            };
            out.push(entry);
            match &entry.parent {
                Some(parent) => cur.clone_from(parent),
                None => break,
            }
        }
        out
    }

    fn effective_draws<'a>(chain: &[&'a ObjectEntry]) -> Vec<(Option<&'a str>, &'a DrawFacet)> {
        let mut merged: Vec<(Option<&'a str>, &'a DrawFacet)> = Vec::new();
        let mut shadowed: Vec<(Option<&'a str>, &'a DrawFacet)> = Vec::new();
        for entry in chain.iter().rev() {
            for evicted in &entry.evicted {
                let kept = |(tag, _): &(Option<&str>, &DrawFacet)| *tag != Some(evicted.as_str());
                merged.retain(kept);
                shadowed.retain(kept);
            }
            for (tag, facet) in &entry.draws {
                let tag = tag.as_deref();
                if let Some(t) = tag {
                    if let Some(slot) = merged.iter_mut().find(|(mt, _)| *mt == Some(t)) {
                        if slot.1.module == facet.module {
                            shadowed.push((tag, slot.1));
                        }
                        slot.1 = facet;
                        continue;
                    }
                }
                merged.push((tag, facet));
            }
        }
        shadowed.reverse();
        merged.into_iter().chain(shadowed).collect()
    }

    fn body_at(facets: &[(Option<&str>, &DrawFacet)]) -> Option<usize> {
        facets
            .iter()
            .position(|(_, facet)| facet.model.is_some())
            .or_else(|| {
                facets
                    .iter()
                    .position(|(_, facet)| facet.names_state_model())
            })
    }

    fn body_facet<'a>(facets: &[(Option<&'a str>, &'a DrawFacet)]) -> Option<&'a DrawFacet> {
        facets.get(Self::body_at(facets)?).map(|(_, facet)| *facet)
    }

    fn model_module<'a>(facets: &[(Option<&'a str>, &'a DrawFacet)]) -> Vec<&'a DrawFacet> {
        let Some(at) = Self::body_at(facets) else {
            return Vec::new();
        };
        let tag = facets.get(at).and_then(|(tag, _)| *tag);
        facets
            .iter()
            .filter(|(slot, _)| *slot == tag)
            .map(|(_, facet)| *facet)
            .collect()
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Template<'_> {
        let key = name.to_ascii_lowercase();
        let chain = self.chain(&key);
        let Some(leaf) = chain.first() else {
            return self.other_namespace(&key);
        };
        let facets = Self::effective_draws(&chain);
        match Self::body_facet(&facets).and_then(|facet| facet.model.as_deref()) {
            Some(model) => Template::Model(model),
            None => Template::NoModel(leaf.reason),
        }
    }

    #[must_use]
    pub fn lookup_local(&self, name: &str) -> Template<'_> {
        let key = name.to_ascii_lowercase();
        match self.entries.get(&key) {
            None => self.other_namespace(&key),
            Some(object) => match reduce_draws(object.draws.iter().map(|(_, facet)| facet)).0 {
                Some(model) => Template::Model(model),
                None => Template::NoModel(object.reason),
            },
        }
    }

    #[must_use]
    pub fn sound(&self, name: &str, keyword: &str) -> Option<&str> {
        let field = keyword.to_ascii_lowercase();
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let object = self.entries.get(&key)?;
            if let Some(value) = object.sounds.get(&field) {
                return Some(value.as_str());
            }
            key = object.parent.clone()?;
        }
        None
    }

    #[must_use]
    pub fn rotation_anchor(&self, name: &str) -> Option<[f32; 2]> {
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let object = self.entries.get(&key)?;
            if let Some(anchor) = object.rotation_anchor {
                return Some(anchor);
            }
            key = object.parent.clone()?;
        }
        None
    }

    #[must_use]
    pub fn crowd_response_key(&self, name: &str) -> Option<&str> {
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let object = self.entries.get(&key)?;
            if let Some(crowd) = object.crowd_response_key.as_deref() {
                return Some(crowd);
            }
            key = object.parent.clone()?;
        }
        None
    }

    #[must_use]
    pub fn animation_sound_behavior(
        &self,
        name: &str,
    ) -> Option<&crate::client_behavior::AnimationSoundBehavior> {
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let object = self.entries.get(&key)?;
            if let Some(behavior) = object.animation_sound.as_ref() {
                return Some(behavior);
            }
            key = object.parent.clone()?;
        }
        None
    }

    #[must_use]
    pub fn animation_sound_failures(&self) -> &[crate::client_behavior::AnimationSoundError] {
        &self.animation_sound_failures
    }

    #[must_use]
    pub fn definitions_with_sounds(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| !entry.sounds.is_empty())
            .count()
    }

    #[must_use]
    pub fn unit_specific_sounds(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        self.per_unit_map(name, |entry| entry.unit_specific_sounds.as_ref())
    }

    #[must_use]
    pub fn unit_specific_sound(&self, name: &str, key: &str) -> Option<&str> {
        self.unit_specific_sounds(name)?
            .get(key)
            .map(String::as_str)
    }

    #[must_use]
    pub fn unit_specific_fx(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        self.per_unit_map(name, |entry| entry.unit_specific_fx.as_ref())
    }

    fn per_unit_map<'a>(
        &'a self,
        name: &str,
        select: impl Fn(&'a ObjectEntry) -> Option<&'a BTreeMap<String, String>>,
    ) -> Option<&'a BTreeMap<String, String>> {
        let mut key = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let entry = self.entries.get(&key)?;
            if let Some(map) = select(entry) {
                return Some(map);
            }
            key = entry.parent.clone()?;
        }
        None
    }

    #[must_use]
    pub fn animation(&self, name: &str) -> Option<&DefaultAnimation> {
        let chain = self.chain(&name.to_ascii_lowercase());
        let facets = Self::effective_draws(&chain);
        if let Some(own) = Self::model_module(&facets)
            .into_iter()
            .find_map(|facet| facet.animation.as_ref())
        {
            return Some(own);
        }
        facets
            .into_iter()
            .find_map(|(_, facet)| facet.animation.as_ref())
    }

    #[must_use]
    pub fn animation_states(&self, name: &str) -> &[crate::anim_state::AnimationStateDef] {
        let chain = self.chain(&name.to_ascii_lowercase());
        let facets = Self::effective_draws(&chain);
        if let Some(own) = Self::model_module(&facets)
            .into_iter()
            .find(|facet| !facet.animation_states.is_empty())
        {
            return own.animation_states.as_slice();
        }
        facets
            .into_iter()
            .find(|(_, facet)| !facet.animation_states.is_empty())
            .map_or(&[], |(_, facet)| facet.animation_states.as_slice())
    }

    #[must_use]
    pub fn model_alternates(&self, name: &str) -> &[String] {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::body_facet(&Self::effective_draws(&chain))
            .map_or(&[], |facet| facet.alternates.as_slice())
    }

    #[must_use]
    pub fn model_states(&self, name: &str) -> &[crate::model_state::ModelStateDef] {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::body_facet(&Self::effective_draws(&chain))
            .map_or(&[], |facet| facet.model_states.as_slice())
    }

    #[must_use]
    pub fn truck_tires(&self, name: &str) -> Option<&TruckTires> {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::body_facet(&Self::effective_draws(&chain))?
            .truck_tires
            .as_ref()
    }

    #[must_use]
    pub fn hide_if_model_conditions(&self, name: &str) -> &[String] {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::body_facet(&Self::effective_draws(&chain))
            .map_or(&[], |facet| facet.hide_if.as_slice())
    }

    #[must_use]
    pub fn ok_to_change_model_color(&self, name: &str) -> bool {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::body_facet(&Self::effective_draws(&chain))
            .is_some_and(|facet| facet.ok_to_change_model_color)
    }

    #[must_use]
    pub fn draw_modules(&self, name: &str) -> Vec<DrawModuleView<'_>> {
        let chain = self.chain(&name.to_ascii_lowercase());
        let facets = Self::effective_draws(&chain);
        let mut slots: Vec<(Option<&str>, Vec<&DrawFacet>)> = Vec::new();
        for (tag, facet) in &facets {
            if let Some(t) = tag {
                if let Some(slot) = slots.iter_mut().find(|(slot_tag, _)| *slot_tag == Some(*t)) {
                    slot.1.push(facet);
                    continue;
                }
            }
            slots.push((*tag, vec![*facet]));
        }
        slots
            .into_iter()
            .map(|(tag, facets)| {
                let model = facets.iter().find_map(|facet| facet.model.as_deref());
                let own = facets
                    .iter()
                    .find(|facet| facet.model.is_some())
                    .or_else(|| facets.iter().find(|facet| facet.names_state_model()));
                DrawModuleView {
                    tag,
                    module: facets.first().and_then(|facet| facet.module.as_deref()),
                    model,
                    is_model_module: own.is_some(),
                    model_states: facets
                        .iter()
                        .find(|facet| facet.model.is_some() || !facet.model_states.is_empty())
                        .map_or(&[], |facet| facet.model_states.as_slice()),
                    animation_states: facets
                        .iter()
                        .find(|facet| !facet.animation_states.is_empty())
                        .map_or(&[], |facet| facet.animation_states.as_slice()),
                    hide_if: own.map_or(&[], |facet| facet.hide_if.as_slice()),
                    weather_textures: facets
                        .iter()
                        .find(|facet| !facet.weather_textures.is_empty())
                        .map_or(&[], |facet| facet.weather_textures.as_slice()),
                    start_hidden: own.is_some_and(|facet| facet.start_hidden),
                    force_to_back: own.is_some_and(|facet| facet.force_to_back),
                    ok_to_change_model_color: own
                        .is_some_and(|facet| facet.ok_to_change_model_color),
                    alternates: own.map_or(&[], |facet| facet.alternates.as_slice()),
                    attach_to_bone: facets
                        .iter()
                        .find_map(|facet| facet.attach_to_bone.as_deref()),
                    wall_bounds_mesh: facets
                        .iter()
                        .find_map(|facet| facet.wall_bounds_mesh.as_deref()),
                    ramp_mesh_1: facets.iter().find_map(|facet| facet.ramp_mesh_1.as_deref()),
                    ramp_mesh_2: facets.iter().find_map(|facet| facet.ramp_mesh_2.as_deref()),
                    raised_wall_mesh: facets
                        .iter()
                        .find_map(|facet| facet.raised_wall_mesh.as_deref()),
                }
            })
            .collect()
    }

    #[must_use]
    pub fn model_modules(&self, name: &str) -> Vec<DrawModuleView<'_>> {
        let modules = self.draw_modules(name);
        let body = modules
            .iter()
            .position(|module| module.model.is_some())
            .or_else(|| modules.iter().position(|module| module.is_model_module));
        if body
            .and_then(|at| modules.get(at))
            .and_then(|module| module.module)
            .is_some_and(|kind| kind.eq_ignore_ascii_case(TREE_DRAW_MODULE))
        {
            return Vec::new();
        }
        modules
            .into_iter()
            .enumerate()
            .filter(|(at, module)| {
                Some(*at) != body && module.is_model_module && !module.start_hidden
            })
            .map(|(_, module)| module)
            .collect()
    }

    #[must_use]
    pub fn floor_modules(&self, name: &str) -> Vec<DrawModuleView<'_>> {
        self.model_modules(name)
            .into_iter()
            .filter(DrawModuleView::is_floor)
            .collect()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    #[must_use]
    pub fn lineage(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = name.to_ascii_lowercase();
        for _ in 0..MAX_INHERITANCE_DEPTH {
            let Some(entry) = self.entries.get(&cur) else {
                break;
            };
            out.push(cur.clone());
            match &entry.parent {
                Some(parent) => cur.clone_from(parent),
                None => break,
            }
        }
        out
    }

    pub fn model_state_declarations(
        &self,
    ) -> impl Iterator<Item = (&str, &[crate::model_state::ModelStateDef])> {
        self.entries.iter().filter_map(|(name, entry)| {
            entry
                .draws
                .iter()
                .find(|(_, facet)| facet.model.is_some())
                .filter(|(_, facet)| !facet.model_states.is_empty())
                .map(|(_, facet)| (name.as_str(), facet.model_states.as_slice()))
        })
    }

    pub fn animation_state_declarations(
        &self,
    ) -> impl Iterator<Item = (&str, &[crate::anim_state::AnimationStateDef])> {
        self.entries.iter().filter_map(|(name, entry)| {
            entry
                .draws
                .iter()
                .find(|(_, facet)| !facet.animation_states.is_empty())
                .map(|(_, facet)| (name.as_str(), facet.animation_states.as_slice()))
        })
    }

    #[must_use]
    pub fn particle_bones(&self, name: &str) -> Option<ParticleBones> {
        let chain = self.chain(&name.to_ascii_lowercase());
        if chain.is_empty() {
            return None;
        }
        let facets = Self::effective_draws(&chain);
        let bones = reduce_draws(facets.iter().map(|(_, facet)| *facet)).5;
        (!bones.is_empty()).then_some(bones)
    }

    #[must_use]
    pub fn streak_draws(&self, name: &str) -> Vec<StreakDraw> {
        let chain = self.chain(&name.to_ascii_lowercase());
        let mut slots: Vec<(Option<&str>, StreakDraw)> = Vec::new();
        for (tag, facet) in Self::effective_draws(&chain) {
            let Some(streak) = facet.streak.as_ref() else {
                continue;
            };
            match tag.and_then(|tag| slots.iter_mut().find(|(slot, _)| *slot == Some(tag))) {
                Some((_, merged)) => merged.patch_under(streak),
                None => slots.push((tag, streak.clone())),
            }
        }
        slots.into_iter().map(|(_, streak)| streak).collect()
    }

    #[must_use]
    pub fn streaks(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.draws.iter().any(|(_, facet)| facet.streak.is_some()))
            .count()
    }

    #[must_use]
    pub fn sways(&self, name: &str) -> bool {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::effective_draws(&chain)
            .into_iter()
            .any(|(_, facet)| facet.vegetation)
    }

    #[must_use]
    pub fn tainted_tree(&self, name: &str) -> bool {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::effective_draws(&chain)
            .into_iter()
            .any(|(_, facet)| facet.tainted)
    }

    #[must_use]
    pub fn tree_topple(&self, name: &str) -> Option<TreeTopple> {
        let chain = self.chain(&name.to_ascii_lowercase());
        Self::effective_draws(&chain)
            .into_iter()
            .find_map(|(_, facet)| facet.topple.clone())
    }

    pub fn particle_bone_declarations(&self) -> impl Iterator<Item = (&str, ParticleBones)> + '_ {
        self.entries.iter().filter_map(|(name, entry)| {
            let bones = reduce_draws(entry.draws.iter().map(|(_, facet)| facet)).5;
            (!bones.is_empty()).then_some((name.as_str(), bones))
        })
    }

    #[must_use]
    pub fn animation_local(&self, name: &str) -> Option<&DefaultAnimation> {
        let entry = self.entries.get(&name.to_ascii_lowercase())?;
        entry
            .draws
            .iter()
            .find_map(|(_, facet)| facet.animation.as_ref())
    }

    pub fn animations(&self) -> impl Iterator<Item = (&str, &DefaultAnimation)> {
        self.entries.iter().filter_map(|(name, entry)| {
            entry
                .draws
                .iter()
                .find_map(|(_, facet)| facet.animation.as_ref())
                .map(|animation| (name.as_str(), animation))
        })
    }

    fn other_namespace(&self, key: &str) -> Template<'_> {
        if self.road_names.contains_key(key) {
            Template::Road
        } else if self.terrain_names.contains(key) {
            Template::Terrain
        } else {
            Template::Undeclared
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn objects(&self) -> usize {
        self.objects
    }

    #[must_use]
    pub fn declared_as_road(&self, name: &str) -> bool {
        self.road_names.contains_key(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn road(&self, name: &str) -> Option<&RoadDefinition> {
        self.road_names.get(&name.to_ascii_lowercase())
    }

    pub fn road_definitions(&self) -> impl Iterator<Item = (&str, &RoadDefinition)> {
        self.road_names
            .iter()
            .map(|(name, road)| (name.as_str(), road))
    }

    #[must_use]
    pub fn declared_as_terrain(&self, name: &str) -> bool {
        self.terrain_names.contains(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn roads(&self) -> usize {
        self.roads
    }

    #[must_use]
    pub fn terrains(&self) -> usize {
        self.terrains
    }

    #[must_use]
    pub fn redefinitions(&self) -> usize {
        self.redefinitions
    }
}

fn road_definition(block: &Block) -> RoadDefinition {
    let [texture, width, width_in_texture] = ROAD_FIELDS;
    let value = |keyword: &str| block.field(keyword).and_then(crate::tree::Field::value);
    RoadDefinition {
        texture: value(texture).map(str::to_owned),
        width: value(width).and_then(|text| text.trim().parse().ok()),
        width_in_texture: value(width_in_texture).and_then(|text| text.trim().parse().ok()),
    }
}

pub const OBJECT_SOUND_KEYWORDS: [&str; 28] = [
    "VoiceSelect",
    "VoiceSelectBattle",
    "VoiceMove",
    "VoiceMoveToCamp",
    "VoiceAttack",
    "VoiceAttackCharge",
    "VoiceAttackStructure",
    "VoiceGuard",
    "VoiceFear",
    "VoiceCreated",
    "VoiceFullyCreated",
    "VoiceRetreatToCastle",
    "SoundImpact",
    "SoundImpactCyclonic",
    "SoundAmbient",
    "SoundAmbientDamaged",
    "SoundAmbientReallyDamaged",
    "SoundAmbientRubble",
    "SoundMoveStart",
    "SoundMoveStartDamaged",
    "SoundMoveLoop",
    "SoundMoveLoopDamaged",
    "SoundOnDamaged",
    "SoundOnReallyDamaged",
    "VoiceAttackMachine",
    "VoiceAttackAir",
    "SoundStealthOn",
    "SoundStealthOff",
];

pub const OBJECT_EVA_KEYWORDS: [&str; 9] = [
    "EvaEventDieOwner",
    "EvaEventDieAlly",
    "EvaEventDieEnemy",
    "EvaEventDamagedOwner",
    "EvaEventDamagedFromShroudedSourceOwner",
    "EvaEventDamagedByFireOwner",
    "EvaEventSecondDamageFarFromFirstOwner",
    "EvaEventSecondDamageFarFromFirstScanRange",
    "EvaEventSecondDamageFarFromFirstTimeoutMS",
];

fn analyse(block: &Block, macros: &crate::value::MacroTable) -> ObjectEntry {
    let parent = if block.keyword.eq("Object") {
        None
    } else {
        block.args.get(1).map(|p| p.to_ascii_lowercase())
    };

    let mut draws: Vec<(Option<String>, DrawFacet)> = Vec::new();

    for draw in module_blocks(block, DRAW_KEYWORD) {
        let tag = draw.args.get(1).map(|t| t.to_ascii_lowercase());
        let module = draw.args.first().map(|m| m.to_ascii_lowercase());
        let vegetation = draw
            .args
            .first()
            .is_some_and(|module| module.eq(TREE_DRAW_MODULE));
        let tainted = vegetation
            && draw
                .field(TAINTED_TREE_KEYWORD)
                .and_then(Field::value)
                .and_then(crate::value::parse_bool)
                == Some(true);
        let topple = vegetation.then(|| draw_tree_topple(draw, macros));
        let streak = draw_streak(draw);
        let offered_a_model = !matches!(model_naming(draw), ModelNaming::Neither);
        let model = draw_model(draw).map(str::to_owned);
        let alternates = if model.is_some() {
            draw_model_alternates(draw)
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        let model_states = crate::model_state::draw_model_states(draw, macros);
        let animation = draw_animation(draw);
        let particle_bones = draw_particle_bones(draw);
        let animation_states = crate::anim_state::draw_animation_states(draw);
        let hide_if = draw_hide_if_model_conditions(draw);
        let weather_textures = draw_weather_textures(draw);
        let start_hidden = draw
            .field(START_HIDDEN_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            == Some(true);
        let force_to_back = draw
            .field(FORCE_TO_BACK_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            == Some(true);
        let ok_to_change_model_color = draw
            .field(OK_TO_CHANGE_MODEL_COLOR_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            == Some(true);
        let attach_to_bone = draw
            .field(ATTACH_TO_BONE_KEYWORD)
            .and_then(Field::value)
            .filter(|bone| !bone.is_empty())
            .map(str::to_owned);
        let named_mesh = |keyword| {
            draw.field(keyword)
                .and_then(Field::value)
                .filter(|mesh| !mesh.is_empty())
                .map(str::to_owned)
        };
        let wall_bounds_mesh = named_mesh(WALL_BOUNDS_MESH_KEYWORD);
        let ramp_mesh_1 = named_mesh(RAMP_MESH_1_KEYWORD);
        let ramp_mesh_2 = named_mesh(RAMP_MESH_2_KEYWORD);
        let raised_wall_mesh = named_mesh(RAISED_WALL_MESH_KEYWORD);
        let truck_tires = draw_truck_tires(draw, module.as_deref());
        draws.push((
            tag,
            DrawFacet {
                module,
                model,
                offered_a_model,
                alternates,
                animation,
                animation_states,
                model_states,
                particle_bones,
                vegetation,
                tainted,
                topple,
                streak,
                hide_if,
                weather_textures,
                start_hidden,
                force_to_back,
                ok_to_change_model_color,
                attach_to_bone,
                wall_bounds_mesh,
                ramp_mesh_1,
                ramp_mesh_2,
                raised_wall_mesh,
                truck_tires,
            },
        ));
    }

    let reason = no_model_reason(&draws);

    let mut sounds = BTreeMap::new();
    for keyword in OBJECT_SOUND_KEYWORDS.iter().chain(&OBJECT_EVA_KEYWORDS) {
        if let Some(value) = block.field(keyword).and_then(crate::tree::Field::value) {
            if !crate::value::is_none(value) {
                sounds.insert(keyword.to_ascii_lowercase(), value.to_string());
            }
        }
    }

    let (animation_sound, animation_sound_errors) = client_behavior_sounds(block);

    ObjectEntry {
        draws,
        reason,
        parent,
        evicted: evicted_tags(block),
        sounds,
        unit_specific_sounds: wildcard_map(block, "UnitSpecificSounds"),
        unit_specific_fx: wildcard_map(block, "UnitSpecificFX"),
        rotation_anchor: tagged_pair(block, ROTATION_ANCHOR_KEYWORD),
        crowd_response_key: block
            .field("CrowdResponseKey")
            .and_then(crate::tree::Field::value)
            .filter(|value| !crate::value::is_none(value))
            .map(ToOwned::to_owned),
        animation_sound,
        animation_sound_errors,
    }
}

fn no_model_reason(draws: &[(Option<String>, DrawFacet)]) -> NoModelReason {
    if draws
        .iter()
        .any(|(_, f)| f.model.is_some() || f.offered_a_model)
    {
        NoModelReason::ModelIsNone
    } else if draws.is_empty() {
        NoModelReason::NoDrawModule
    } else {
        NoModelReason::NonDrawingModule
    }
}

fn module_blocks<'a>(block: &'a Block, keyword: &'a str) -> impl Iterator<Item = &'a Block> {
    block.blocks_named(keyword).chain(
        block
            .blocks()
            .filter(|wrapper| {
                wrapper.keyword.eq(ADD_MODULE_KEYWORD) || wrapper.keyword.eq(REPLACE_MODULE_KEYWORD)
            })
            .flat_map(move |wrapper| wrapper.blocks_named(keyword)),
    )
}

fn evicted_tags(block: &Block) -> Vec<String> {
    let replaced = block
        .blocks_named(REPLACE_MODULE_KEYWORD)
        .filter_map(|wrapper| wrapper.args.first());
    let removed = block
        .fields_named(REMOVE_MODULE_KEYWORD)
        .filter_map(Field::value);
    replaced
        .map(String::as_str)
        .chain(removed)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn client_behavior_sounds(
    block: &Block,
) -> (
    Option<crate::client_behavior::AnimationSoundBehavior>,
    Vec<crate::client_behavior::AnimationSoundError>,
) {
    let mut merged: Option<crate::client_behavior::AnimationSoundBehavior> = None;
    let mut errors = Vec::new();
    for module in module_blocks(block, "ClientBehavior").filter(|module| {
        module.selector().is_some_and(|kind| {
            kind.eq_ignore_ascii_case(crate::client_behavior::ANIMATION_SOUND_CLIENT_BEHAVIOR)
        })
    }) {
        let (behavior, mut found_errors) = crate::client_behavior::animation_sound_behavior(module);
        errors.append(&mut found_errors);
        let target = merged.get_or_insert_with(Default::default);
        target.sounds.extend(behavior.sounds);
        if behavior.max_update_range_cap.is_some() {
            target.max_update_range_cap = behavior.max_update_range_cap;
        }
    }
    (merged, errors)
}

fn tagged_pair(block: &Block, keyword: &str) -> Option<[f32; 2]> {
    let field = block.fields_named(keyword).last()?;
    let axis = |tag: &str| {
        crate::value::tagged_value(&field.args, tag)
            .and_then(crate::value::parse_float)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    };
    Some([axis("X"), axis("Y")])
}

fn wildcard_map(block: &Block, keyword: &str) -> Option<BTreeMap<String, String>> {
    let inner = block.blocks_named(keyword).last()?;
    let mut map = BTreeMap::new();
    for field in inner.fields() {
        if let Some(value) = field.value() {
            map.entry(field.keyword.clone())
                .or_insert_with(|| value.to_string());
        }
    }
    Some(map)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectFields {
    pub kind_of: Vec<String>,
    pub side: Option<String>,
    pub display_name: Option<String>,
    pub editor_sorting: Option<String>,
    pub vision_range: Option<f32>,
    pub threat_level: Option<f32>,
    pub build_cost_raw: Option<String>,
    pub build_time: Option<crate::value::Duration>,
    pub command_set: Option<String>,
    pub radar_priority: Option<String>,
    pub shadow: Option<String>,
    pub transport_slot_count: Option<i64>,
    pub geometry: Option<String>,
    pub geometry_is_small: Option<bool>,
    pub geometry_major_radius: Option<f32>,
    pub geometry_minor_radius: Option<f32>,
    pub geometry_height: Option<f32>,
    pub geometry_offset: Vec<[f32; 3]>,
    pub geometry_contact_point: Vec<[f32; 3]>,
    pub shockwave_resistance_raw: Option<String>,
}

fn object_value<'a>(block: &'a Block, keyword: &str) -> Option<&'a str> {
    block.field(keyword).and_then(crate::tree::Field::value)
}

fn object_reference(block: &Block, keyword: &str) -> Option<String> {
    let value = object_value(block, keyword)?;
    (!value.eq_ignore_ascii_case(NONE_VALUE)).then(|| value.to_owned())
}

fn tagged_triples(block: &Block, keyword: &str) -> Vec<[f32; 3]> {
    block
        .fields_named(keyword)
        .map(|f| {
            let axis = |tag: &str| {
                crate::value::tagged_value(&f.args, tag).and_then(crate::value::parse_float)
            };
            [
                axis("X").unwrap_or(0.0),
                axis("Y").unwrap_or(0.0),
                axis("Z").unwrap_or(0.0),
            ]
        })
        .collect()
}

#[must_use]
pub fn read_object_fields(block: &Block) -> ObjectFields {
    let build_time_raw = object_value(block, "BuildTime");
    let build_time = build_time_raw
        .and_then(crate::value::parse_float)
        .map(crate::value::Duration::Seconds);
    ObjectFields {
        kind_of: block
            .field("KindOf")
            .map(|f| f.args.clone())
            .unwrap_or_default(),
        side: object_value(block, "Side").map(str::to_owned),
        display_name: object_value(block, "DisplayName").map(str::to_owned),
        editor_sorting: object_reference(block, "EditorSorting"),
        vision_range: object_value(block, "VisionRange").and_then(crate::value::parse_float),
        threat_level: object_value(block, "ThreatLevel").and_then(crate::value::parse_float),
        build_cost_raw: object_value(block, "BuildCost").map(str::to_owned),
        build_time,
        command_set: object_value(block, "CommandSet").map(str::to_owned),
        radar_priority: object_value(block, "RadarPriority").map(str::to_owned),
        shadow: object_reference(block, "Shadow"),
        transport_slot_count: object_value(block, "TransportSlotCount")
            .and_then(crate::value::parse_int),
        geometry: object_value(block, "Geometry").map(str::to_owned),
        geometry_is_small: object_value(block, "GeometryIsSmall")
            .and_then(crate::value::parse_bool),
        geometry_major_radius: object_value(block, "GeometryMajorRadius")
            .and_then(crate::value::parse_float),
        geometry_minor_radius: object_value(block, "GeometryMinorRadius")
            .and_then(crate::value::parse_float),
        geometry_height: object_value(block, "GeometryHeight").and_then(crate::value::parse_float),
        geometry_offset: tagged_triples(block, "GeometryOffset"),
        geometry_contact_point: tagged_triples(block, "GeometryContactPoint"),
        shockwave_resistance_raw: object_value(block, "ShockwaveResistance").map(str::to_owned),
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::table::BlockTable;

    #[test]
    fn a_particle_bone_is_bucketed_by_the_state_that_holds_it() {
        let index = index(
            "Object Hut\n  Draw = W3DScriptedModelDraw Tag\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = AHut\n\
             \x20     ParticleSysBone = Bone01 ForgeSmoke\n\
             \x20   End\n\
             \x20   ModelConditionState = DAMAGED\n\
             \x20     ParticleSysBone = Bone01 FireBuildingSmall\n\
             \x20   End\n\
             \x20   IdleAnimationState\n\
             \x20     ParticleSysBone = NONE FireTorch\n\
             \x20   End\n\
             \x20   AnimationState = DYING\n\
             \x20     ParticleSysBone = NONE DeathDust\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        let bones = index.particle_bones("hut").expect("declared");
        assert_eq!(bones.default.len(), 1);
        assert_eq!(bones.default[0].system, "ForgeSmoke");
        assert_eq!(bones.default[0].bone.as_deref(), Some("Bone01"));
        assert_eq!(bones.idle.len(), 1);
        assert_eq!(bones.idle[0].system, "FireTorch");
        assert_eq!(bones.idle[0].bone, None, "NONE is the object's origin");
        let standing: Vec<&str> = bones
            .standing()
            .map(|attachment| attachment.system.as_str())
            .collect();
        assert!(
            !standing.contains(&"FireTorch"),
            "an idle animation state's attachments are not played: {standing:?}"
        );
        assert_eq!(
            standing.len(),
            bones.default.len(),
            "standing() is the unconditional state and nothing else"
        );
        assert_eq!(bones.conditioned.len(), 2);
        assert_eq!(bones.all().count(), 4);
    }

    #[test]
    fn model_modules_are_the_inherited_visible_modules_beside_the_body() {
        let index = index(
            "Object Base\n\
             \x20 Draw = W3DScriptedModelDraw Body\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = ABody\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DFloorDraw Floor\n\
             \x20   ModelName = ABib\n\
             \x20   WeatherTexture = SNOWY ABib_Snow.tga\n\
             \x20   HideIfModelConditions = AWAITING_CONSTRUCTION\n\
             \x20   ForceToBack = Yes\n\
             \x20 End\n\
             \x20 Draw = W3DFloorDraw Later\n\
             \x20   ModelName = AnUpgradeBib\n\
             \x20   StartHidden = Yes\n\
             \x20 End\n\
             End\n\
             ChildObject Child Base\n\
             \x20 Draw = W3DScriptedModelDraw Banner\n\
             \x20   AttachToBoneInAnotherModule = ABone\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = ABanner\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DScriptedModelDraw Oil\n\
             \x20   OkToChangeModelColor = Yes\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = None\n\
             \x20   End\n\
             \x20   ModelConditionState = UPGRADE_OIL\n\
             \x20     Model = AnOil\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DScriptedModelDraw Nothing\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = None\n\
             \x20   End\n\
             \x20   ModelConditionState = DAMAGED\n\
             \x20   End\n\
             \x20 End\n\
             End\n\
             Object Pad\n\
             \x20 Draw = W3DFloorDraw Floor\n\
             \x20   ModelName = APad\n\
             \x20 End\n\
             End\n",
        );
        let modules: Vec<_> = index
            .model_modules("child")
            .iter()
            .map(|module| {
                (
                    module.tag,
                    module.model,
                    module.attach_to_bone,
                    module.ok_to_change_model_color,
                )
            })
            .collect();
        assert_eq!(
            modules,
            [
                (Some("floor"), Some("ABib"), None, false),
                (Some("banner"), Some("ABanner"), Some("ABone"), false),
                (Some("oil"), None, None, true),
            ],
            "a module whose model only a state names is handed out; one naming none is not"
        );
        let floors = index.floor_modules("child");
        assert_eq!(floors.len(), 1);
        assert_eq!(floors[0].model, Some("ABib"));
        assert_eq!(floors[0].hide_if, ["AWAITING_CONSTRUCTION".to_owned()]);
        assert_eq!(
            floors[0].weather_textures,
            [(crate::NuggetWeather::Snowy, "ABib_Snow.tga".to_owned())]
        );
        assert!(floors[0].force_to_back, "ForceToBack = Yes was declared");
        assert!(
            index.floor_modules("pad").is_empty(),
            "a floor-only object's floor is its body"
        );
    }

    #[test]
    fn the_body_is_the_default_model_module_else_the_first_naming_one_on_a_state() {
        let index = index(
            "Object Site
               Draw = W3DScriptedModelDraw Body
                 DefaultModelConditionState
                   Model = None
                 End
                 ModelConditionState = ACTIVELY_BEING_CONSTRUCTED
                   Model = AScaffold
                 End
               End
               Draw = W3DScriptedModelDraw Door
                 DefaultModelConditionState
                   Model = None
                 End
                 ModelConditionState = ACTIVELY_BEING_CONSTRUCTED
                   Model = ADoor
                 End
               End
             End
             Object Hero
               Draw = W3DScriptedModelDraw EditorOnly
                 DefaultModelConditionState
                   Model = None
                 End
                 ModelConditionState = WORLD_BUILDER
                   Model = AnEditorHero
                 End
               End
               Draw = W3DScriptedModelDraw Body
                 DefaultModelConditionState
                   Model = AHero
                 End
               End
             End
             Object Marker
               Draw = W3DScriptedModelDraw Body
                 DefaultModelConditionState
                   Model = None
                 End
               End
             End
",
        );
        let body_models = |name: &str| -> Vec<Option<String>> {
            index
                .model_states(name)
                .iter()
                .map(|state| state.model.clone())
                .collect()
        };
        let beside = |name: &str| -> Vec<Option<&str>> {
            index
                .model_modules(name)
                .iter()
                .map(|module| module.tag)
                .collect()
        };
        assert_eq!(
            (body_models("site"), beside("site")),
            (vec![None, Some("AScaffold".to_owned())], vec![Some("door")])
        );
        assert_eq!(index.lookup("hero").model(), Some("AHero"));
        assert_eq!(beside("hero"), [Some("editoronly")]);
        assert_eq!(
            (body_models("marker"), beside("marker")),
            (Vec::new(), Vec::new())
        );
    }

    #[test]
    fn ok_to_change_model_color_defaults_false_and_reads_yes() {
        let index = index(
            "Object Unset\n\
             \x20 Draw = W3DScriptedModelDraw Body\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = AModel\n\
             \x20   End\n\
             \x20 End\n\
             End\n\
             Object Opted\n\
             \x20 Draw = W3DScriptedModelDraw Body\n\
             \x20   OkToChangeModelColor = Yes\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = AModel\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        assert!(!index.ok_to_change_model_color("unset"));
        assert!(index.ok_to_change_model_color("opted"));
    }

    #[test]
    fn draw_modules_lists_a_model_less_module_beside_the_model_one() {
        let index = index(
            "Object Horde\n\
             \x20 Draw = W3DHordeModelDraw ModuleTag_01\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = AHorde\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DScriptedModelDraw DustEffects\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = None\n\
             \x20   End\n\
             \x20   AnimationState = MOVING ACCELERATE\n\
             \x20     ParticleSysBone = NONE GenericSiegeTrailDust\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        let modules = index.draw_modules("horde");
        assert_eq!(modules.len(), 2);
        assert!(modules[0].is_model_module);
        assert_eq!(modules[0].model, Some("AHorde"));
        assert!(!modules[1].is_model_module);
        assert_eq!(modules[1].model, None);
        assert_eq!(
            modules[1].model_states.len(),
            1,
            "a model-less module's own DefaultModelConditionState is real"
        );
        assert_eq!(modules[1].animation_states.len(), 1);
        assert_eq!(
            modules[1].animation_states[0].particles[0].system,
            "GenericSiegeTrailDust"
        );
    }

    #[test]
    fn a_condition_of_none_is_the_unconditional_state() {
        let index = index(
            "Object Post\n  Draw = W3DScriptedModelDraw Tag\n\
             \x20   ModelConditionState = NONE\n\
             \x20     Model = APost\n\
             \x20     ParticleSysBone = NONE TorchGlow\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        let bones = index.particle_bones("post").expect("declared");
        assert_eq!(bones.default.len(), 1, "{bones:?}");
        assert!(bones.conditioned.is_empty());
    }

    #[test]
    fn follow_bone_is_read_in_every_spelling_the_data_uses() {
        let index = index(
            "Object Camp\n  Draw = W3DScriptedModelDraw Tag\n\
             \x20   DefaultModelConditionState\n\
             \x20     ParticleSysBone = B01 A FollowBone:Yes\n\
             \x20     ParticleSysBone = B02 B FOLLOWBONE\n\
             \x20     ParticleSysBone B03 C Followbone:YES\n\
             \x20     ParticleSysBone = B04 D Persist PersistID:3\n\
             \x20     ParticleSysBone = B05 E\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        let bones = index.particle_bones("camp").expect("declared");
        let following: Vec<bool> = bones.default.iter().map(|b| b.follow_bone).collect();
        assert_eq!(following, vec![true, true, true, false, false]);
        let persist: Vec<(Persist, u32)> = bones
            .default
            .iter()
            .map(|b| (b.persist, b.persist_id))
            .collect();
        assert_eq!(persist[3], (Persist::None, 3));
        assert_eq!(persist[4], (Persist::None, 0));
        let systems: Vec<&str> = bones
            .default
            .iter()
            .map(|bone| bone.system.as_str())
            .collect();
        assert_eq!(
            systems,
            vec!["A", "B", "C", "D", "E"],
            "the `=` is optional"
        );
    }

    #[test]
    fn a_particle_bone_line_with_no_system_is_not_an_attachment() {
        let index = index(
            "Object Stub\n  Draw = W3DScriptedModelDraw Tag\n\
             \x20   DefaultModelConditionState\n\
             \x20     ParticleSysBone = NONE\n\
             \x20     ParticleSysBone = Bone01 Real\n\
             \x20   End\n\
             \x20 End\n\
             End\n",
        );
        let bones = index.particle_bones("stub").expect("declared");
        assert_eq!(bones.default.len(), 1);
        assert_eq!(bones.default[0].system, "Real");
    }

    #[test]
    fn attachments_are_inherited_and_gathered_from_every_draw_module() {
        let index = index(
            "Object Parent\n  Draw = W3DScriptedModelDraw One\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = AParent\n\
             \x20     ParticleSysBone = NONE FromOne\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DScriptedModelDraw Two\n\
             \x20   DefaultModelConditionState\n\
             \x20     ParticleSysBone = NONE FromTwo\n\
             \x20   End\n\
             \x20 End\n\
             End\n\
             ChildObject Child Parent\n\
             \x20 Scale = 2\n\
             End\n",
        );
        let parent = index.particle_bones("parent").expect("declared");
        let systems: Vec<&str> = parent
            .default
            .iter()
            .map(|bone| bone.system.as_str())
            .collect();
        assert_eq!(systems, vec!["FromOne", "FromTwo"]);
        assert_eq!(
            index
                .particle_bones("child")
                .map(|bones| bones.default.len()),
            Some(2),
            "a child that overrides nothing inherits the parent's"
        );
        assert!(index.particle_bones("nothing").is_none());
    }

    #[test]
    fn extra_mesh_lines_are_alternates_and_not_the_model() {
        let index = index(
            "Object MordorFighter
  Draw = W3DHordeModelDraw ModuleTag_01
    DefaultModelConditionState
      Model     = MUOrcWar_SKN
      Skeleton  = MUOrcWarr_SKL
      Model     = MUOrcWar_D_SKN  ExtraMesh:Yes
      Model     = MUOrcWar_E_SKN  ExtraMesh:Yes
      Model     = MUOrcWar_F_SKN  ExtraMesh:Yes
      Model     = MUOrcBrute_SKN  ExtraMesh:Yes
    End
  End
End
",
        );
        assert_eq!(index.lookup("mordorfighter").model(), Some("MUOrcWar_SKN"));
        assert_eq!(
            index.model_alternates("MordorFighter"),
            [
                "MUOrcWar_D_SKN",
                "MUOrcWar_E_SKN",
                "MUOrcWar_F_SKN",
                "MUOrcBrute_SKN"
            ]
        );
    }

    #[test]
    fn a_state_with_one_model_line_offers_no_alternates() {
        let index = index(
            "Object GondorArcher
  Draw = W3DScriptedModelDraw ModuleTag_01
    DefaultModelConditionState
      Model = GUArcher_SKN
    End
  End
End
",
        );
        assert_eq!(index.lookup("gondorarcher").model(), Some("GUArcher_SKN"));
        assert!(index.model_alternates("GondorArcher").is_empty());
    }

    #[test]
    fn the_base_model_is_the_unflagged_line_whatever_its_position() {
        let index = index(
            "Object Backwards
  Draw = W3DHordeModelDraw ModuleTag_01
    DefaultModelConditionState
      Model = AltA_SKN  ExtraMesh:Yes
      Model = Base_SKN
      Model = AltB_SKN  ExtraMesh:Yes
    End
  End
End
",
        );
        assert_eq!(index.lookup("backwards").model(), Some("Base_SKN"));
        assert_eq!(
            index.model_alternates("backwards"),
            ["AltA_SKN", "AltB_SKN"]
        );
    }

    #[test]
    fn a_base_repeated_as_an_extra_stays_in_the_pool_twice() {
        let index = index(
            "Object MordorCorsairsOfUmbar
  Draw = W3DHordeModelDraw ModuleTag_01
    DefaultModelConditionState
      Model = MUCorsar_SKN
      Model = MUCorsar2_SKN  ExtraMesh:Yes
      Model = MUCorsar_SKN ExtraMesh:Yes
    End
  End
End
",
        );
        assert_eq!(
            index.lookup("mordorcorsairsofumbar").model(),
            Some("MUCorsar_SKN")
        );
        assert_eq!(
            index.model_alternates("mordorcorsairsofumbar"),
            ["MUCorsar2_SKN", "MUCorsar_SKN"]
        );
    }

    fn index(text: &str) -> ObjectIndex {
        let doc = Document::parse("t.ini", text, &BlockTable::object_model_path())
            .expect("synthetic fixture parses");
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let mut index = ObjectIndex::new();
        index.insert_document(&doc);
        index
    }

    #[test]
    fn every_streak_module_is_handed_out_and_a_patch_inherits_what_it_omits() {
        let index = index(
            "Object FireArrow\n\
             \x20 Draw = W3DScriptedModelDraw Tag_Draw\n\
             \x20   DefaultModelConditionState\n\
             \x20     Model = NONE\n\
             \x20   End\n\
             \x20 End\n\
             \x20 Draw = W3DStreakDraw Tag_Streak\n\
             \x20   Length = 15\n\
             \x20   Width = 3\n\
             \x20   Texture = AStreakFire.tga\n\
             \x20   WeatherTexture = SNOWY AStreakFire_Snow.tga\n\
             \x20 End\n\
             \x20 Draw = W3DStreakDraw Tag_Glow\n\
             \x20   Length = 50\n\
             \x20   Width = 1\n\
             \x20   Texture = AGlow.tga\n\
             \x20 End\n\
             End\n\
             ChildObject WideFireArrow FireArrow\n\
             \x20 Draw = W3DStreakDraw Tag_Streak\n\
             \x20   Width = 6\n\
             \x20 End\n\
             End\n",
        );
        let streaks = index.streak_draws("FireArrow");
        let textures: Vec<Option<&str>> = streaks.iter().map(|s| s.texture.as_deref()).collect();
        assert_eq!(textures, [Some("AStreakFire.tga"), Some("AGlow.tga")]);
        let child = index.streak_draws("WideFireArrow");
        assert_eq!(child.len(), 2, "{child:?}");
        assert_eq!(child[0].width, Some(6.0));
        assert_eq!(child[0].length, Some(15.0));
        assert_eq!(child[0].texture.as_deref(), Some("AStreakFire.tga"));
        let snowy = vec![(
            crate::NuggetWeather::Snowy,
            "AStreakFire_Snow.tga".to_owned(),
        )];
        assert_eq!(streaks[0].weather_textures, snowy);
        assert_eq!(child[0].weather_textures, snowy);
        assert!(child[1].weather_textures.is_empty());
    }

    #[test]
    fn a_rotation_anchor_is_read_and_inherited_like_any_plain_field() {
        let index = index(concat!(
            "Object WallPiece\n",
            "  GeometryRotationAnchorOffset = X:1076.832 Y:4.0\n",
            "End\n",
            "ChildObject WallUpgrade WallPiece\n",
            "End\n",
            "ChildObject WallInner WallPiece\n",
            "  GeometryRotationAnchorOffset = X:401.132\n",
            "End\n",
            "Object Plain\n",
            "End\n",
        ));
        assert_eq!(index.rotation_anchor("wallpiece"), Some([1076.832, 4.0]));
        assert_eq!(index.rotation_anchor("WallUpgrade"), Some([1076.832, 4.0]));
        assert_eq!(index.rotation_anchor("WallInner"), Some([401.132, 0.0]));
        assert_eq!(index.rotation_anchor("Plain"), None);
        assert_eq!(index.rotation_anchor("NoSuchThing"), None);
    }

    #[test]
    fn a_road_declaration_yields_its_three_fields() {
        let index = index(
            "Road Sidewalk15
  Texture = TRSidewalk15.tga
  RoadWidth = 52
               RoadWidthInTexture = .95
End
",
        );
        assert_eq!(index.roads(), 1);
        assert_eq!(index.len(), 0, "a road is not an object");
        let road = index
            .road("sIdEwAlK15")
            .expect("declared, case-insensitively");
        assert_eq!(road.texture.as_deref(), Some("TRSidewalk15.tga"));
        assert_eq!(road.width, Some(52.0));
        assert_eq!(road.width_in_texture, Some(0.95));
        assert!(matches!(index.lookup("Sidewalk15"), Template::Road));
    }

    #[test]
    fn a_road_field_that_is_absent_or_not_a_number_stays_absent() {
        let index = index(
            "Road Broken
  RoadWidth = wide
End
",
        );
        let road = index.road("broken").expect("declared");
        assert_eq!(road.width, None);
        assert_eq!(road.width_in_texture, None);
        assert_eq!(road.texture, None);
    }

    #[test]
    fn a_tree_draw_module_is_what_makes_a_template_sway() {
        let objects = index(
            "Object Oak\n  Draw = W3DTreeDraw Tag\n    ModelName = AnOak\n                 DoTopple = Yes\n  End\n  KindOf = TREE SHRUBBERY\nEnd\n\
             Object Bramble\n  Draw = W3DTreeDraw Tag\n    ModelName = ABramble\n                 MoveOutwardTime = 333\n    DarkeningFactor = 0.3\n  End\n  KindOf = SHRUB\nEnd\n\
             Object Hut\n  Draw = W3DScriptedModelDraw Tag\n    DefaultModelConditionState\n               \x20    Model = AHut\n    End\n  End\nEnd\n",
        );
        assert!(objects.sways("oak"), "a TREE with a tree draw sways");
        assert!(objects.sways("BRAMBLE"), "and so does a SHRUB with one");
        assert!(!objects.sways("hut"), "a scripted model draw does not");
        assert!(
            !objects.sways("nothing declared"),
            "nor does an unknown name"
        );

        let odd = index(
            "Object Willow\n  Draw = w3dtreedraw Tag\n    ModelName = AWillow\n  \
                         End\nEnd\n",
        );
        assert!(
            !odd.sways("willow"),
            "wrong-case module type is not W3DTreeDraw"
        );
    }

    #[test]
    fn a_tainted_tree_draw_is_read_over_the_composed_lineage() {
        let index = index(
            "Object DeadOak
  Draw = W3DTreeDraw Tag
    ModelName = ADeadOak
                 TaintedTree	     = Yes
  End
End
             ChildObject DeadOakCopy DeadOak
  KindOf = TREE
End
             Object LiveOak
  Draw = W3DTreeDraw Tag
    ModelName = AnOak
  End
End
",
        );
        assert!(index.tainted_tree("deadoak"));
        assert!(
            index.tainted_tree("DeadOakCopy"),
            "a child inherits its parent's tree draw whole"
        );
        assert!(!index.tainted_tree("liveoak"));
        assert!(!index.tainted_tree("nothing declared"));
    }

    #[test]
    fn a_tree_draw_topple_is_read_over_the_composed_lineage() {
        let index = index(
            "Object FallingTest
  Draw = W3DScriptedModelDraw First
    DefaultModelConditionState
      Model = AFallingTest
    End
  End
  Draw = W3DTreeDraw Tag
    ModelName = AFallingTest
    ToppleFX = FX_TestTopple
    KillWhenFinishedToppling = No ; stays
    InitialVelocityPercent = 40%
    SinkDistance = 3;
    SinkTime = 4000
  End
End
ChildObject FallingTestCopy FallingTest
  KindOf = TREE
End
",
        );
        let topple = index.tree_topple("FallingTestCopy").expect("a tree draw");
        assert_eq!(topple.topple_fx.as_deref(), Some("FX_TestTopple"));
        assert_eq!(topple.bounce_fx, None);
        assert!(!topple.kill_when_finished);
        assert!((topple.initial_velocity - 0.4).abs() < 1e-6);
        assert!((topple.initial_accel - 0.01).abs() < 1e-6);
        assert!((topple.sink_distance - 3.0).abs() < 1e-6);
        assert!((topple.sink_time_ms - 4000.0).abs() < 1e-3);
        assert!(index.tree_topple("nothing declared").is_none());
    }

    #[test]
    fn a_second_draw_module_written_first_does_not_hide_the_tree_draw() {
        let index = index(
            "Object Elm\n  Draw = W3DScriptedModelDraw Burnt\n                 DefaultModelConditionState\n      Model = AnElmBurning\n    End\n  End\n               Draw = W3DTreeDraw Tag\n    ModelName = AnElm\n  End\nEnd\n",
        );
        assert!(index.sways("elm"));
    }

    #[test]
    fn a_child_inherits_its_parents_tree_draw_but_an_override_replaces_it() {
        let index = index(
            "Object BaseTree\n  Draw = W3DTreeDraw T\n    ModelName = ABase\n  End\nEnd\n\
             ChildObject Recoloured BaseTree\n  KindOf = TREE\nEnd\n\
             ChildObject Rebuilt BaseTree\n  Draw = W3DScriptedModelDraw T\n                 DefaultModelConditionState\n      Model = ARebuild\n    End\n  End\nEnd\n",
        );
        assert!(index.sways("basetree"));
        assert!(
            index.sways("recoloured"),
            "a child that declares no draw module inherits its parent's"
        );
        assert!(
            !index.sways("rebuilt"),
            "a child that declares its own draw module replaces its parent's"
        );
    }

    #[test]
    fn a_child_that_only_adds_a_decorative_draw_module_still_shows_its_parents_body() {
        let index = index(
            "Object FarmInterface\n\
               Draw = W3DScriptedModelDraw ModuleTag_Draw\n\
                 DefaultModelConditionState\n\
                   Model = GBFarm_SKN\n\
                 End\n\
                 ModelConditionState = ACTIVELY_BEING_CONSTRUCTED\n\
                   Model = GBFarm_A\n\
                   ParticleSysBone = DUSTBONE BuildingContructDust\n\
                 End\n\
               End\n\
             End\n\
             ChildObject GondorFarm FarmInterface\n\
               Draw = W3DScriptedModelDraw ModuleTag_Draw_HCBanner\n\
                 DefaultModelConditionState\n\
                   Model = GBHCFarm\n\
                 End\n\
               End\n\
               Draw = W3DScriptedModelDraw Draw_BonusEffects\n\
                 DefaultModelConditionState\n\
                   Model = None\n\
                 End\n\
               End\n\
             End\n",
        );
        assert_eq!(
            index.lookup("gondorfarm").model(),
            Some("GBFarm_SKN"),
            "the inherited body must win over the child's own decorative module"
        );
        let construction = index
            .particle_bones("gondorfarm")
            .expect("the parent's construction dust must still reach the child");
        assert!(construction
            .conditioned
            .iter()
            .any(|bone| bone.system == "BuildingContructDust"));
    }

    #[test]
    fn a_model_name_in_the_module_is_found_as_well_as_one_in_a_state() {
        let index = index(
            "Object Tree\n  Draw = W3DTreeDraw Tag\n    ModelName = ATree\n  End\nEnd\n\
             Object Hut\n  Draw = W3DScriptedModelDraw Tag\n    DefaultModelConditionState\n\
             \x20     Model = AHut\n    End\n  End\nEnd\n",
        );
        assert_eq!(index.lookup("tree").model(), Some("ATree"));
        assert_eq!(index.lookup("HUT").model(), Some("AHut"));
        assert_eq!(index.objects(), 2);
    }

    #[test]
    fn the_first_condition_state_wins_and_later_ones_are_ignored() {
        let index = index(
            "Object Wall\n  Draw = W3DScriptedModelDraw Tag\n    DefaultModelConditionState\n\
             \x20     Model = AWall\n    End\n    ModelConditionState = DAMAGED\n\
             \x20     Model = AWallDamaged\n    End\n  End\nEnd\n",
        );
        assert_eq!(index.lookup("wall").model(), Some("AWall"));
    }

    #[test]
    fn none_is_an_answer_and_the_three_reasons_are_distinguished() {
        let index = index(
            "Object Proxy\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n\
             \x20     Model = NONE\n    End\n  End\nEnd\n\
             Object Sound\n  Draw = W3DDefaultDraw T\n  End\nEnd\n\
             Object Marker\n  KindOf = SPAWN\nEnd\n",
        );
        assert_eq!(
            index.lookup("proxy"),
            Template::NoModel(NoModelReason::ModelIsNone)
        );
        assert_eq!(
            index.lookup("sound"),
            Template::NoModel(NoModelReason::NonDrawingModule)
        );
        assert_eq!(
            index.lookup("marker"),
            Template::NoModel(NoModelReason::NoDrawModule)
        );
    }

    #[test]
    fn a_child_inherits_its_parent_model_and_a_cycle_terminates() {
        let index = index(
            "Object Base\n  Draw = W3DTreeDraw T\n    ModelName = ABase\n  End\nEnd\n\
             ChildObject Mid Base\n  KindOf = STRUCTURE\nEnd\n\
             ChildObject Leaf Mid\n  KindOf = STRUCTURE\nEnd\n\
             ChildObject Own Base\n  Draw = W3DTreeDraw T\n    ModelName = AOwn\n  End\nEnd\n\
             ChildObject LoopA LoopB\nEnd\n\
             ChildObject LoopB LoopA\nEnd\n",
        );
        assert_eq!(index.lookup("leaf").model(), Some("ABase"), "depth 2");
        assert_eq!(index.lookup_local("leaf").model(), None, "not its own");
        assert_eq!(index.lookup("own").model(), Some("AOwn"), "child overrides");
        assert!(index.lookup("loopa").model().is_none(), "cycle terminates");
    }

    #[test]
    fn roads_and_terrain_are_their_own_namespaces_not_missing_objects() {
        let index = index(
            "Road Sidewalk\n  Texture = TRSidewalk.tga\nEnd\n\
             Terrain GrassRhun01\n  Texture = TGras.tga\nEnd\n",
        );
        assert_eq!(index.lookup("sidewalk"), Template::Road);
        assert_eq!(index.lookup("grassrhun01"), Template::Terrain);
        assert_eq!(index.lookup("never-declared"), Template::Undeclared);
        assert!(!index.lookup("sidewalk").is_object());
        assert_eq!(index.roads(), 1);
        assert_eq!(index.terrains(), 1);
    }

    #[test]
    fn animation_is_a_block_and_its_three_leaves_are_read() {
        let index = index(
            "Object Rock
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
                   Model = ARock
    End
    IdleAnimationState
                   Animation = Label
        AnimationName = ARock_SKL.ARock_IDLA
                     AnimationMode = LOOP
        AnimationBlendTime = 15
      End
                 End
  End
End
             Object Bare
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
                   Animation
        AnimationName = ABare_IDLA
      End
    End
               End
End
",
        );
        let rock = index.animation("rock").expect("declared");
        assert_eq!(rock.name, "ARock_SKL.ARock_IDLA");
        assert_eq!(rock.mode, Some(AnimationMode::Loop));
        assert_eq!(rock.blend_frames, Some(15));
        assert_eq!(rock.source, AnimationSource::IdleAnimationState);
        assert_eq!(rock.asset(), "ARock_IDLA");
        assert_eq!(rock.hierarchy(), Some("ARock_SKL"));

        let bare = index.animation("bare").expect("declared");
        assert_eq!(bare.asset(), "ABare_IDLA");
        assert_eq!(bare.hierarchy(), None);
        assert_eq!(bare.mode, None, "an absent mode is not a default one");
    }

    #[test]
    fn a_conditioned_animation_state_is_not_a_default_and_the_idle_state_wins() {
        let index = index(
            "Object Gate
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
                   Model = AGate
    End
    AnimationState = DYING
      Animation
                     AnimationName = AGate_DIEA
      End
    End
                 IdleAnimationState
      Animation
                     AnimationName = AGate_IDLA
      End
    End
  End
End
",
        );
        let gate = index.animation("gate").expect("declared");
        assert_eq!(gate.name, "AGate_IDLA");
        assert_eq!(gate.source, AnimationSource::IdleAnimationState);
        assert_eq!(index.lookup("gate").model(), Some("AGate"));
    }

    #[test]
    fn an_animation_inherits_independently_of_the_model_and_a_cycle_terminates() {
        let index = index(
            "Object Base
  Draw = W3DScriptedModelDraw T
    DefaultModelConditionState
                   Model = ABase
    End
    IdleAnimationState
      Animation
                     AnimationName = ABase_IDLA
        AnimationMode = MANUAL
                   End
    End
  End
End
             ChildObject Reskin Base
  Draw = W3DScriptedModelDraw T
                 DefaultModelConditionState
      Model = AReskin
    End
  End
End
             ChildObject Own Base
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
                   Animation
        AnimationName = AOwn_IDLA
      End
    End
               End
End
             ChildObject LoopA LoopB
End
             ChildObject LoopB LoopA
End
",
        );
        assert_eq!(index.lookup("reskin").model(), Some("AReskin"));
        let inherited = index.animation("reskin").expect("through the parent");
        assert_eq!(inherited.name, "ABase_IDLA");
        assert_eq!(inherited.mode, Some(AnimationMode::Manual));
        assert!(index.animation_local("reskin").is_none(), "not its own");

        assert_eq!(
            index.animation("own").map(|a| a.name.as_str()),
            Some("AOwn_IDLA")
        );
        assert!(index.animation("loopa").is_none(), "cycle terminates");
        assert_eq!(
            index.animations().count(),
            2,
            "declarations, not resolutions"
        );
    }

    #[test]
    fn the_animation_may_come_from_a_later_draw_module_than_the_model() {
        let index = index(
            "Object Tree
  Draw = W3DTreeDraw Opt
    ModelName = ATree
  End
               Draw = W3DScriptedModelDraw T
    IdleAnimationState
      Animation
                     AnimationName = ATree_IDLA
      End
    End
  End
End
",
        );
        assert_eq!(index.lookup("tree").model(), Some("ATree"));
        assert_eq!(
            index.animation("tree").map(|a| a.name.as_str()),
            Some("ATree_IDLA")
        );
    }

    #[test]
    fn the_states_come_from_the_module_that_carries_the_model() {
        let index = index(
            "Object Orc
  Draw = W3DScriptedModelDraw Stun
    DefaultModelConditionState
      Model = None
    End
               AnimationState = EMOTION_TERROR
      ParticleSysBone NONE StunSmall
    End
  End
  Draw = W3DHordeModelDraw Body
    DefaultModelConditionState
                   Model = AOrc
    End
    AnimationState = MOVING
      Animation
                     AnimationName = AOrc_RUNA
      End
    End
  End
End
",
        );
        assert_eq!(index.lookup("orc").model(), Some("AOrc"));
        let states = index.animation_states("orc");
        assert_eq!(states.len(), 1, "one module's machine, not both");
        assert_eq!(states[0].conditions, ["MOVING"]);
    }

    #[test]
    fn the_naming_mechanism_is_structural() {
        let doc = Document::parse(
            "t.ini",
            "Object X\n  Draw = ModAddedDraw T\n    ModelName = AThing\n  End\n\
             \x20 Draw = W3DDefaultDraw T2\n  End\n\
             \x20 Draw = Odd T3\n    ModelName = A\n    DefaultModelConditionState\n\
             \x20     Model = B\n    End\n  End\nEnd\n",
            &BlockTable::object_model_path(),
        )
        .unwrap();
        let draws: Vec<_> = doc.roots()[0].blocks_named("Draw").collect();
        assert_eq!(model_naming(draws[0]), ModelNaming::ModelName);
        assert_eq!(model_naming(draws[1]), ModelNaming::Neither);
        assert_eq!(model_naming(draws[2]), ModelNaming::Both);
    }

    #[test]
    fn the_state_flags_are_read_from_the_state_and_not_from_the_animation() {
        let index = index(
            "Object Bird
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
                   Animation = IdleA
        AnimationName = A.B
                     AnimationMode = ONCE
      End
      Flags = RANDOMSTART
                 End
  End
End
             Object Still
  Draw = W3DScriptedModelDraw T
    IdleAnimationState
                   Animation = IdleA
        AnimationName = A.B
      End
                 End
  End
End
",
        );
        let bird = index.animation("bird").expect("Bird declares one");
        assert!(bird.flags.random_start, "RANDOMSTART is on the state");
        assert!(!bird.flags.start_frame_first);
        assert_eq!(bird.mode, Some(AnimationMode::Once));
        let still = index.animation("still").expect("Still declares one");
        assert_eq!(
            still.flags,
            AnimationFlags::default(),
            "a state with no Flags leaf carries none"
        );
    }

    #[test]
    fn animation_flags_follow_the_bit_string_operators() {
        let bare = AnimationFlags::parse("START_FRAME_LAST");
        assert!(bare.start_frame_last && !bare.random_start && !bare.start_frame_first);
        let two = AnimationFlags::parse("RANDOMSTART START_FRAME_FIRST");
        assert!(two.random_start && two.start_frame_first);
        let cleared = AnimationFlags::parse("RANDOMSTART -RANDOMSTART +START_FRAME_LAST");
        assert!(!cleared.random_start && cleared.start_frame_last);
        assert_eq!(
            AnimationFlags::parse("RANDOMSTART NONE"),
            AnimationFlags::default()
        );
        let unknown = AnimationFlags::parse("PRISTINE_BONE_POS_IN_FINAL_FRAME RANDOMSTART");
        assert!(unknown.random_start);
        assert_eq!(AnimationFlags::parse("   "), AnimationFlags::default());
        assert_eq!(
            AnimationFlags::parse("MAINTAIN_FRAME_ACROSS_STATES MAINTAIN_FRAME_ACROSS_STATES4")
                .maintain_frame,
            0b1001
        );
        assert!(AnimationFlags::parse("RESTART_ANIM_WHEN_COMPLETE").restart_when_complete);
    }

    #[test]
    fn both_spellings_of_the_ping_pong_mode_parse() {
        assert_eq!(
            AnimationMode::parse("PING_PONG"),
            Some(AnimationMode::PingPong)
        );
        assert_eq!(
            AnimationMode::parse("loop_pingpong"),
            Some(AnimationMode::PingPong)
        );
        assert_eq!(AnimationMode::PingPong.as_str(), "PING_PONG");
        assert_eq!(AnimationMode::parse("SOMETHING_ELSE"), None);
    }

    #[test]
    fn object_fields_reads_its_top_direct_fields() {
        let doc = crate::tree::Document::parse(
            "object.ini",
            "Object GondorArcher\n  \
                KindOf = SELECTABLE PROJECTILE\n  \
                Side = Gondor\n  \
                EditorSorting = INFANTRY\n  \
                VisionRange = 150.0\n  \
                BuildCost = 300\n  \
                BuildTime = 15.0\n  \
                CommandSet = GondorArcherCommandSet\n  \
                GeometryIsSmall = Yes\n  \
                GeometryMajorRadius = 12.0\n  \
                GeometryContactPoint = X:-33 Y:10 Z:0\n  \
                GeometryContactPoint = X:33 Y:10 Z:0\n  \
                ShockwaveResistance = SHOCKWAVE_RESISTANCE_STRONG\n\
            End\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let fields = read_object_fields(&doc.roots()[0]);
        assert_eq!(fields.kind_of, vec!["SELECTABLE", "PROJECTILE"]);
        assert_eq!(fields.side.as_deref(), Some("Gondor"));
        assert_eq!(fields.vision_range, Some(150.0));
        assert_eq!(
            fields.build_time,
            Some(crate::value::Duration::Seconds(15.0))
        );
        assert_eq!(fields.geometry_is_small, Some(true));
        assert_eq!(fields.geometry_contact_point.len(), 2);
        assert_eq!(fields.geometry_contact_point[0], [-33.0, 10.0, 0.0]);
        assert_eq!(fields.geometry_contact_point[1], [33.0, 10.0, 0.0]);
        assert_eq!(
            fields.shockwave_resistance_raw.as_deref(),
            Some("SHOCKWAVE_RESISTANCE_STRONG")
        );
    }

    #[test]
    fn none_clears_object_reference_fields_too() {
        let doc = crate::tree::Document::parse(
            "object.ini",
            "Object Bare\n  Shadow = NONE\n  EditorSorting = NONE\nEnd\n",
            &BlockTable::priority_blocks_path(),
        )
        .unwrap();
        let fields = read_object_fields(&doc.roots()[0]);
        assert_eq!(fields.shadow, None);
        assert_eq!(fields.editor_sorting, None);
    }
}

#[cfg(test)]
mod sound_tests {
    use super::*;
    use crate::table::BlockTable;

    fn index(text: &str) -> ObjectIndex {
        let document = Document::parse("o.ini", text, &BlockTable::object_model_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        let mut index = ObjectIndex::new();
        index.insert_document(&document);
        index
    }

    #[test]
    fn an_object_names_its_own_voices() {
        let index = index(
            "Object GondorArcher\n\
             \x20 VoiceSelect = GondorArcherVoiceSelect\n\
             \x20 VoiceMove = GondorArcherVoiceMove\n\
             \x20 VoicePriority = 84\n\
             \x20 SoundImpact = ImpactHorse\n\
             End\n",
        );
        assert_eq!(
            index.sound("gondorarcher", "VoiceSelect"),
            Some("GondorArcherVoiceSelect")
        );
        assert_eq!(
            index.sound("GONDORARCHER", "voicemove"),
            Some("GondorArcherVoiceMove")
        );
        assert_eq!(
            index.sound("GondorArcher", "SoundImpact"),
            Some("ImpactHorse")
        );
        assert_eq!(index.sound("GondorArcher", "VoicePriority"), None);
        assert_eq!(index.definitions_with_sounds(), 1);
    }

    #[test]
    fn a_child_object_inherits_the_voices_it_does_not_name() {
        let index = index(
            "Object GondorArcher\n  VoiceSelect = Base\n  VoiceMove = BaseMove\nEnd\n\
             ChildObject GondorArcherSummoned GondorArcher\n  VoiceSelect = Summoned\nEnd\n",
        );
        assert_eq!(
            index.sound("GondorArcherSummoned", "VoiceSelect"),
            Some("Summoned")
        );
        assert_eq!(
            index.sound("GondorArcherSummoned", "VoiceMove"),
            Some("BaseMove")
        );
    }

    #[test]
    fn none_clears_a_sound_rather_than_naming_one() {
        let index = index("Object Silent\n  VoiceSelect = None\nEnd\n");
        assert_eq!(index.sound("Silent", "VoiceSelect"), None);
        assert_eq!(index.definitions_with_sounds(), 0);
    }

    #[test]
    fn an_undeclared_template_answers_nothing() {
        let index = index("Object Real\n  VoiceSelect = X\nEnd\n");
        assert_eq!(index.sound("NotDeclared", "VoiceSelect"), None);
    }

    #[test]
    fn a_parent_cycle_terminates() {
        let index =
            index("ChildObject A B\n  VoiceMove = M\nEnd\nChildObject B A\n  VoiceMove = N\nEnd\n");
        assert_eq!(index.sound("A", "VoiceSelect"), None);
        assert_eq!(index.sound("A", "VoiceMove"), Some("M"));
    }

    #[test]
    fn unit_specific_sounds_reads_an_arbitrary_key() {
        let index = index(
            "Object Troll\n\
             \x20 UnitSpecificSounds\n\
             \x20   UnderConstruction = TrollBuilding\n\
             \x20   AKeyNoShippedFileWrites = TrollObscure\n\
             \x20 End\n\
             End\n",
        );
        assert_eq!(
            index.unit_specific_sound("Troll", "UnderConstruction"),
            Some("TrollBuilding")
        );
        assert_eq!(
            index.unit_specific_sound("Troll", "AKeyNoShippedFileWrites"),
            Some("TrollObscure")
        );
        assert_eq!(index.unit_specific_sound("Troll", "NeverWritten"), None);
    }

    #[test]
    fn unit_specific_fx_is_the_same_construct_with_no_corpus_evidence() {
        let index = index("Object Ent\n  UnitSpecificFX\n    RootStomp = FXEntStomp\n  End\nEnd\n");
        assert_eq!(
            index.unit_specific_fx("Ent"),
            Some(&BTreeMap::from([(
                "RootStomp".to_string(),
                "FXEntStomp".to_string()
            )]))
        );
    }

    #[test]
    fn unit_specific_sound_keys_are_case_sensitive() {
        let index = index("Object T\n  UnitSpecificSounds\n    TankTurretMove = M\n  End\nEnd\n");
        assert_eq!(index.unit_specific_sound("t", "TankTurretMove"), Some("M"));
        assert_eq!(index.unit_specific_sound("t", "tankturretmove"), None);
    }

    #[test]
    fn none_is_stored_literally_in_a_per_unit_sound() {
        let index = index("Object T\n  UnitSpecificSounds\n    VoiceGarrison = None\n  End\nEnd\n");
        assert_eq!(
            index.unit_specific_sound("t", "VoiceGarrison"),
            Some("None")
        );
    }

    #[test]
    fn a_child_that_redeclares_unit_specific_sounds_replaces_the_whole_map() {
        let index = index(
            "Object Base\n\
             \x20 UnitSpecificSounds\n\
             \x20   UnderConstruction = BaseBuild\n\
             \x20   VoiceGarrison = BaseGarrison\n\
             \x20 End\n\
             End\n\
             ChildObject Child Base\n\
             \x20 UnitSpecificSounds\n\
             \x20   VoiceGarrison = ChildGarrison\n\
             \x20 End\n\
             End\n",
        );
        assert_eq!(
            index.unit_specific_sound("Child", "VoiceGarrison"),
            Some("ChildGarrison")
        );
        assert_eq!(
            index.unit_specific_sound("Child", "UnderConstruction"),
            None,
            "the child's own block replaced the parent's wholesale"
        );
        assert_eq!(
            index.unit_specific_sound("Base", "UnderConstruction"),
            Some("BaseBuild")
        );
    }

    #[test]
    fn a_child_that_never_writes_the_block_inherits_it_whole() {
        let index = index(
            "Object Base\n  UnitSpecificSounds\n    UnderConstruction = BaseBuild\n  End\nEnd\n\
             ChildObject Reskin Base\n  VoiceSelect = ReskinSelect\nEnd\n",
        );
        assert_eq!(
            index.unit_specific_sound("Reskin", "UnderConstruction"),
            Some("BaseBuild")
        );
    }

    #[test]
    fn the_first_of_a_repeated_key_in_one_block_wins() {
        let index = index(
            "Object T\n\
             \x20 UnitSpecificSounds\n\
             \x20   VoiceGarrison = First\n\
             \x20   VoiceGarrison = Second\n\
             \x20 End\n\
             End\n",
        );
        assert_eq!(
            index.unit_specific_sound("t", "VoiceGarrison"),
            Some("First")
        );
    }

    #[test]
    fn a_second_occurrence_of_the_block_replaces_the_first() {
        let index = index(
            "Object T\n\
             \x20 UnitSpecificSounds\n\
             \x20   UnderConstruction = FirstBlock\n\
             \x20 End\n\
             \x20 UnitSpecificSounds\n\
             \x20   VoiceGarrison = SecondBlock\n\
             \x20 End\n\
             End\n",
        );
        assert_eq!(
            index.unit_specific_sound("t", "UnderConstruction"),
            None,
            "wiped by the second block's own clear()"
        );
        assert_eq!(
            index.unit_specific_sound("t", "VoiceGarrison"),
            Some("SecondBlock")
        );
    }

    #[test]
    fn an_object_with_no_wildcard_block_answers_none() {
        let index = index("Object Plain\n  VoiceSelect = X\nEnd\n");
        assert_eq!(index.unit_specific_sounds("Plain"), None);
        assert_eq!(index.unit_specific_fx("Plain"), None);
    }
}

#[cfg(test)]
mod override_tests {
    use super::*;
    use crate::table::BlockTable;

    fn document(text: &str) -> Document {
        let document = Document::parse("o.ini", text, &BlockTable::object_model_path()).unwrap();
        assert!(document.is_balanced(), "{:?}", document.diagnostics());
        document
    }

    fn standing(index: &ObjectIndex, name: &str) -> Vec<String> {
        index.particle_bones(name).map_or_else(Vec::new, |bones| {
            bones.standing().map(|a| a.system.clone()).collect()
        })
    }

    const BASE: &str = "Object Lantern\n\
        \x20 VoiceSelect = LanternHum\n\
        \x20 Draw = W3DScriptedModelDraw Glow\n\
        \x20   DefaultModelConditionState\n\
        \x20     Model = NONE\n\
        \x20     ParticleSysBone = None DustPuffTest\n\
        \x20   End\n\
        \x20 End\n\
        End\n";

    #[test]
    fn a_replace_module_swaps_the_inherited_draw_its_header_names() {
        let mut index = ObjectIndex::new();
        index.insert_document(&document(&format!(
            "{BASE}ChildObject LanternChild Lantern\n\
             \x20 ReplaceModule Glow\n\
             \x20   Draw = W3DScriptedModelDraw GlowSwapped\n\
             \x20     DefaultModelConditionState\n\
             \x20       Model = NONE\n\
             \x20       ParticleSysBone = None SparkTest\n\
             \x20     End\n\
             \x20   End\n\
             \x20 End\n\
             End\n"
        )));
        assert_eq!(standing(&index, "LanternChild"), ["SparkTest"]);
        assert_eq!(standing(&index, "Lantern"), ["DustPuffTest"]);
    }

    #[test]
    fn a_map_ini_object_block_patches_the_template_it_names() {
        let mut index = ObjectIndex::new();
        index.insert_document(&document(BASE));
        let patched = index.insert_override_document(
            &document(
                "Object Lantern\n\
                 \x20 ReplaceModule Glow\n\
                 \x20   Draw = W3DScriptedModelDraw GlowOverride\n\
                 \x20     DefaultModelConditionState\n\
                 \x20       Model = NONE\n\
                 \x20       ParticleSysBone = None FlockTest\n\
                 \x20       ParticleSysBone = None MistTest\n\
                 \x20     End\n\
                 \x20   End\n\
                 \x20 End\n\
                 End\n\
                 Object Signpost\n  Draw = W3DScriptedModelDraw Body\n\
                 \x20   DefaultModelConditionState\n      Model = SignpostModel\n    End\n  End\nEnd\n",
            ),
            &crate::value::MacroTable::new(),
        );
        assert_eq!(patched, 1, "Signpost is new, not an override");
        assert_eq!(standing(&index, "Lantern"), ["FlockTest", "MistTest"]);
        assert_eq!(index.sound("Lantern", "VoiceSelect"), Some("LanternHum"));
        assert_eq!(index.lookup("Signpost"), Template::Model("SignpostModel"));
    }

    #[test]
    fn a_remove_module_in_an_override_drops_the_draw() {
        let mut index = ObjectIndex::new();
        index.insert_document(&document(BASE));
        index.insert_override_document(
            &document("Object Lantern\n  RemoveModule = Glow\nEnd\n"),
            &crate::value::MacroTable::new(),
        );
        assert!(standing(&index, "Lantern").is_empty());
    }
}
