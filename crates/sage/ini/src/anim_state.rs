use crate::object::{
    parse_particle_sys_bone, AnimationFlags, AnimationMode, ParticleSysBone, NONE_VALUE,
    PARTICLE_SYS_BONE_KEYWORD,
};
use crate::tree::{Block, Field};

pub const STATE_NAME_KEYWORD: &str = "StateName";

pub const ENTERING_STATE_FX_KEYWORD: &str = "EnteringStateFX";

pub const SPEED_FACTOR_KEYWORD: &str = "AnimationSpeedFactorRange";

pub const DISTANCE_KEYWORD: &str = "Distance";

pub const PRIORITY_KEYWORD: &str = "AnimationPriority";

pub const MAX_CLIPS_PER_STATE: usize = 64;

pub const MAX_STATES_PER_DRAW: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnimationStateKind {
    Conditioned,
    Idle,
    Transition,
}

pub const MODEL_TOKEN: &str = "#(MODEL)";

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationClip {
    pub label: Option<String>,
    pub name: String,
    pub mode: Option<AnimationMode>,
    pub blend_time: Option<f32>,
    pub must_complete_blend: bool,
    pub speed_range: Option<(f32, f32)>,
    pub distance: Option<f32>,
    pub use_weapon_timing: bool,
    pub priority: Option<i32>,
}

impl AnimationClip {
    #[must_use]
    pub fn asset(&self) -> &str {
        match self.name.rsplit_once('.') {
            Some((_, asset)) => asset,
            None => &self.name,
        }
    }

    #[must_use]
    pub fn name_for(&self, prefix: &str) -> String {
        match self.name.split_once(MODEL_TOKEN) {
            Some((before, after)) => format!("{before}{prefix}{after}"),
            None => self.name.clone(),
        }
    }

    #[must_use]
    pub fn asset_for(&self, prefix: &str) -> String {
        let name = self.name_for(prefix);
        match name.rsplit_once('.') {
            Some((_, asset)) => asset.to_owned(),
            None => name,
        }
    }

    #[must_use]
    pub fn hierarchy(&self) -> Option<&str> {
        self.name.rsplit_once('.').map(|(hierarchy, _)| hierarchy)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationStateDef {
    pub kind: AnimationStateKind,
    pub conditions: Vec<String>,
    pub state_name: Option<String>,
    pub entering_state_fx: Option<String>,
    pub flags: AnimationFlags,
    pub clips: Vec<AnimationClip>,
    pub script: Option<crate::tree::Script>,
    pub fx_events: Vec<AnimationFxEvent>,
    pub particles: Vec<ParticleSysBone>,
    pub frame_for_pristine_bone_positions: Option<u32>,
    pub lua_frame_events: Vec<AnimationLuaEvent>,
    pub allow_repeat_in_random_pick: bool,
    pub file: std::sync::Arc<str>,
    pub line: u32,
}

impl AnimationStateDef {
    #[must_use]
    pub fn is_unconditional(&self) -> bool {
        self.kind == AnimationStateKind::Idle || self.conditions.is_empty()
    }

    #[must_use]
    pub fn transition_name(&self) -> Option<&str> {
        (self.kind == AnimationStateKind::Transition)
            .then(|| self.conditions.first().map(String::as_str))
            .flatten()
    }
}

fn kind_of(keyword: &str) -> Option<AnimationStateKind> {
    if keyword.eq("AnimationState") {
        Some(AnimationStateKind::Conditioned)
    } else if keyword.eq(crate::object::IDLE_ANIMATION_STATE_KEYWORD) {
        Some(AnimationStateKind::Idle)
    } else if keyword.eq("TransitionState") {
        Some(AnimationStateKind::Transition)
    } else {
        None
    }
}

fn number(field: Option<&Field>) -> Option<f32> {
    crate::value::parse_float(field?.value()?)
}

fn read_clip(block: &Block) -> Option<AnimationClip> {
    let value = |keyword: &str| block.field(keyword).and_then(Field::value);
    let name = value("AnimationName")?.trim();
    if name.is_empty() {
        return None;
    }
    let speed_range = block.field(SPEED_FACTOR_KEYWORD).and_then(|field| {
        let min = crate::value::parse_float(field.args.first()?)?;
        let max = crate::value::parse_float(field.args.get(1)?)?;
        Some((min, max))
    });
    Some(AnimationClip {
        label: block.args.first().map(|label| label.trim().to_owned()),
        name: name.to_owned(),
        mode: value("AnimationMode").and_then(AnimationMode::parse),
        blend_time: number(block.field("AnimationBlendTime")),
        speed_range,
        distance: number(block.field(DISTANCE_KEYWORD)).filter(|d| *d > 0.0),
        must_complete_blend: value("AnimationMustCompleteBlend")
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        use_weapon_timing: value("UseWeaponTiming")
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        priority: block
            .field(PRIORITY_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_int)
            .and_then(|priority| i32::try_from(priority).ok()),
    })
}

fn read_state(block: &Block, kind: AnimationStateKind, module_flag: bool) -> AnimationStateDef {
    let mut clips = Vec::new();
    for child in block.blocks() {
        if clips.len() >= MAX_CLIPS_PER_STATE {
            break;
        }
        if !child.keyword.eq(crate::object::ANIMATION_KEYWORD) {
            continue;
        }
        if let Some(clip) = read_clip(child) {
            clips.push(clip);
        }
    }
    let conditions = match kind {
        AnimationStateKind::Transition => block.args.clone(),
        _ => block
            .args
            .iter()
            .filter(|token| !token.eq_ignore_ascii_case(NONE_VALUE))
            .map(|token| token.to_ascii_uppercase())
            .collect(),
    };
    AnimationStateDef {
        kind,
        conditions,
        state_name: block
            .field(STATE_NAME_KEYWORD)
            .and_then(Field::value)
            .map(|name| name.trim().to_owned()),
        entering_state_fx: block
            .field(ENTERING_STATE_FX_KEYWORD)
            .and_then(Field::value)
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(NONE_VALUE))
            .map(str::to_owned),
        flags: block
            .field(crate::object::ANIMATION_FLAGS_KEYWORD)
            .and_then(Field::value)
            .map(AnimationFlags::parse)
            .unwrap_or_default(),
        clips,
        script: block.scripts().next().cloned(),
        fx_events: block
            .fields_named(FX_EVENT_KEYWORD)
            .filter_map(read_fx_event)
            .take(MAX_FX_EVENTS_PER_STATE)
            .collect(),
        particles: block
            .fields_named(PARTICLE_SYS_BONE_KEYWORD)
            .filter_map(|field| parse_particle_sys_bone(&field.args))
            .map(|mut attachment| {
                attachment.module_flag = module_flag;
                attachment
            })
            .collect(),
        frame_for_pristine_bone_positions: block
            .field(FRAME_FOR_PRISTINE_BONE_POSITIONS_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_int)
            .and_then(|frame| u32::try_from(frame).ok()),
        lua_frame_events: block
            .fields_named(LUA_EVENT_KEYWORD)
            .filter_map(read_lua_frame_event)
            .take(MAX_FX_EVENTS_PER_STATE)
            .collect(),
        allow_repeat_in_random_pick: block
            .field(ALLOW_REPEAT_IN_RANDOM_PICK_KEYWORD)
            .and_then(Field::value)
            .and_then(crate::value::parse_bool)
            .unwrap_or(false),
        file: block.file.clone(),
        line: block.line,
    }
}

pub const ALLOW_REPEAT_IN_RANDOM_PICK_KEYWORD: &str = "AllowRepeatInRandomPick";

pub const FRAME_FOR_PRISTINE_BONE_POSITIONS_KEYWORD: &str = "FrameForPristineBonePositions";

pub const FX_EVENT_KEYWORD: &str = "FXEvent";

const FIRE_WHEN_SKIPPED_KEYWORD: &str = "FireWhenSkipped";

pub const MAX_FX_EVENTS_PER_STATE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationFxEvent {
    pub frame: u32,
    pub fire_when_skipped: bool,
    pub fx_list: String,
    pub frame_stop: Option<u32>,
    pub bone: Option<String>,
}

fn strip_key<'a>(token: &'a str, key: &str) -> Option<&'a str> {
    let candidate = token.get(..key.len())?;
    let rest = token.get(key.len()..)?;
    if candidate.eq_ignore_ascii_case(key) && rest.starts_with(':') {
        rest.get(1..)
    } else {
        None
    }
}

fn fx_value<'a>(rest: &'a str, args: &'a [String], next: usize) -> (Option<&'a str>, usize) {
    if rest.trim().is_empty() {
        (args.get(next).map(String::as_str), next.saturating_add(1))
    } else {
        (Some(rest), next)
    }
}

fn read_fx_event(field: &Field) -> Option<AnimationFxEvent> {
    let args = &field.args;
    let (mut frame, mut frame_stop, mut name, mut bone, mut fire_when_skipped) =
        (None, None, None, None, false);
    let mut index = 0usize;
    while let Some(token) = args.get(index) {
        index = index.saturating_add(1);
        if token.eq_ignore_ascii_case(FIRE_WHEN_SKIPPED_KEYWORD) {
            fire_when_skipped = true;
        } else if let Some(rest) = strip_key(token, "Frame") {
            let (value, next) = fx_value(rest, args, index);
            frame = value.and_then(|v| v.trim().parse().ok());
            index = next;
        } else if let Some(rest) = strip_key(token, "Name") {
            let (value, next) = fx_value(rest, args, index);
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                name = Some(value.to_owned());
            }
            index = next;
        } else if let Some(rest) = strip_key(token, "FrameStop") {
            let (value, next) = fx_value(rest, args, index);
            frame_stop = value.and_then(|v| v.trim().parse().ok());
            index = next;
        } else if let Some(rest) = strip_key(token, "Bone") {
            let (value, next) = fx_value(rest, args, index);
            if let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) {
                bone = Some(value.to_owned());
            }
            index = next;
        }
    }
    Some(AnimationFxEvent {
        frame: frame.unwrap_or(0),
        fire_when_skipped,
        fx_list: name?,
        frame_stop,
        bone,
    })
}

pub const LUA_EVENT_KEYWORD: &str = "LuaEvent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationLuaEvent {
    pub frame: u32,
    pub data: Option<String>,
}

fn read_lua_frame_event(field: &Field) -> Option<AnimationLuaEvent> {
    let args = &field.args;
    let (mut frame, mut data) = (None, None);
    let mut index = 0usize;
    while let Some(token) = args.get(index) {
        index = index.saturating_add(1);
        if let Some(rest) = strip_key(token, "Frame") {
            let (value, next) = fx_value(rest, args, index);
            frame = value.and_then(|v| v.trim().parse().ok());
            index = next;
        } else if let Some(rest) = strip_key(token, "Data") {
            let (value, next) = fx_value(rest, args, index);
            data = value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned);
            index = next;
        }
    }
    Some(AnimationLuaEvent {
        frame: frame?,
        data,
    })
}

#[must_use]
pub fn draw_animation_states(draw: &Block) -> Vec<AnimationStateDef> {
    let module_flag = draw
        .field(crate::object::PARTICLES_ATTACHED_TO_ANIMATED_BONES_KEYWORD)
        .and_then(Field::value)
        .and_then(crate::value::parse_bool)
        == Some(true);
    let mut states = Vec::new();
    for child in draw.blocks() {
        if states.len() >= MAX_STATES_PER_DRAW {
            break;
        }
        let Some(kind) = kind_of(&child.keyword) else {
            continue;
        };
        states.push(read_state(child, kind, module_flag));
    }
    states
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests;
