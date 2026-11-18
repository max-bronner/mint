use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::{
    parameter_int, parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState,
    ParseCtx,
};
use crate::rules::{Ids, RotwkRules};

const PENDING_CAP: usize = 16;

const COMPLETED_SOUND_AFTER_FRAME: u64 = 3;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaptionCmd {
    pub(crate) seq: u32,
    pub(crate) label: String,
    pub(crate) ms: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NotificationCmd {
    pub(crate) seq: u32,
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) ms: i32,
    pub(crate) object_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Objective {
    pub(crate) tag: String,
    pub(crate) bonus: bool,
    pub(crate) shown: bool,
    pub(crate) completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectiveSoundKind {
    Added,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObjectiveSound {
    pub(crate) serial: u32,
    pub(crate) kind: ObjectiveSoundKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RadarCmd {
    pub(crate) serial: u32,
    pub(crate) at: Vec2,
    pub(crate) kind: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CineState {
    pub(crate) letterbox: bool,
    pub(crate) input_disabled: bool,
    pub(crate) input_seq: u32,
    pub(crate) time_frozen: bool,
    pub(crate) caption: Option<CaptionCmd>,
    pub(crate) notification: Option<NotificationCmd>,
    pub(crate) objectives: Vec<Objective>,
    pub(crate) objective_sounds: Vec<ObjectiveSound>,
    sound_serial: u32,
    pub(crate) radar: Vec<RadarCmd>,
    radar_serial: u32,
    pub(crate) radar_refresh_seq: u32,
}

impl CineState {
    pub(crate) fn with_objectives(tags: &[(String, bool)]) -> Self {
        Self {
            objectives: tags
                .iter()
                .map(|(tag, bonus)| Objective {
                    tag: tag.clone(),
                    bonus: *bonus,
                    shown: false,
                    completed: false,
                })
                .collect(),
            ..Self::default()
        }
    }

    fn push_sound(&mut self, kind: ObjectiveSoundKind) {
        self.sound_serial = self.sound_serial.wrapping_add(1);
        if self.objective_sounds.len() >= PENDING_CAP {
            self.objective_sounds.remove(0);
        }
        self.objective_sounds.push(ObjectiveSound {
            serial: self.sound_serial,
            kind,
        });
    }

    pub(crate) fn push_radar(&mut self, at: Vec2, kind: i32) {
        self.radar_serial = self.radar_serial.wrapping_add(1);
        if self.radar.len() >= PENDING_CAP {
            self.radar.remove(0);
        }
        self.radar.push(RadarCmd {
            serial: self.radar_serial,
            at,
            kind,
        });
    }

    pub(crate) fn show_objective(&mut self, number: i32) {
        let Some(objective) = Self::entry(&mut self.objectives, number) else {
            return;
        };
        let newly = !objective.shown;
        objective.shown = true;
        if newly {
            self.push_sound(ObjectiveSoundKind::Added);
        }
    }

    pub(crate) fn hide_objective(&mut self, number: i32) {
        if let Some(objective) = Self::entry(&mut self.objectives, number) {
            objective.shown = false;
        }
    }

    pub(crate) fn complete_objective(&mut self, number: i32, completed: bool, frame: u64) {
        let Some(objective) = Self::entry(&mut self.objectives, number) else {
            return;
        };
        let newly = completed && !objective.completed;
        objective.completed = completed;
        if newly && frame > COMPLETED_SOUND_AFTER_FRAME {
            self.push_sound(ObjectiveSoundKind::Completed);
        }
    }

    fn entry(objectives: &mut [Objective], number: i32) -> Option<&mut Objective> {
        let index = usize::try_from(number.checked_sub(1)?).ok()?;
        objectives.get_mut(index)
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.letterbox);
        hasher.write_bool(self.input_disabled);
        hasher.write_u32(self.input_seq);
        hasher.write_bool(self.time_frozen);
        hasher.write_u32(self.caption.as_ref().map_or(0, |c| c.seq));
        hasher.write_u32(self.notification.as_ref().map_or(0, |n| n.seq));
        hasher.write_usize(self.objectives.len());
        for objective in &self.objectives {
            hasher.write_bool(objective.shown);
            hasher.write_bool(objective.completed);
        }
        hasher.write_u32(self.sound_serial);
        hasher.write_u32(self.radar_serial);
        hasher.write_u32(self.radar_refresh_seq);
    }
}

pub(super) struct Simple<F> {
    pub(super) unit: Option<String>,
    pub(super) run: F,
}

impl<F> ActionVerb for Simple<F>
where
    F: Fn(&RotwkRules, &Tick<'_>, Ids, &mut MapScriptState) + Send + Sync,
{
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        (self.run)(rules, tick, ids, state);
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.unit.as_deref()
    }
}

pub(super) type Parsed = Option<Arc<dyn ActionVerb>>;

pub(super) fn simple<F>(run: F) -> Parsed
where
    F: Fn(&RotwkRules, &Tick<'_>, Ids, &mut MapScriptState) + Send + Sync + 'static,
{
    Some(Arc::new(Simple { unit: None, run }))
}

pub(super) fn parse_letterbox_begin(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.ui_hidden = true;
        state.cine.letterbox = true;
    })
}

pub(super) fn parse_letterbox_end(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.ui_hidden = false;
        state.cine.letterbox = false;
    })
}

pub(super) fn parse_disable_input(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.cine.input_disabled = true;
        state.cine.input_seq = state.cine.input_seq.wrapping_add(1);
    })
}

pub(super) fn parse_enable_input(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| state.cine.input_disabled = false)
}

pub(super) fn parse_freeze_time(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| state.cine.time_frozen = true)
}

pub(super) fn parse_unfreeze_time(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| state.cine.time_frozen = false)
}

#[allow(clippy::cast_possible_truncation)]
pub(super) fn seconds_to_ms(seconds: f32) -> i32 {
    (seconds * 1000.0) as i32
}

pub(super) fn bump(previous: Option<u32>) -> u32 {
    previous.map_or(1, |seq| seq.wrapping_add(1))
}

pub(super) fn parse_military_caption(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let label = parameter_string(params, 0)?;
    let ms = seconds_to_ms(parameter_real(params, 1)?);
    simple(move |_, _, _, state| {
        state.cine.caption = Some(CaptionCmd {
            seq: bump(state.cine.caption.as_ref().map(|c| c.seq)),
            label: label.clone(),
            ms,
        });
    })
}

fn notification(params: &[Parameter], object_type: String) -> Parsed {
    let kind = parameter_string(params, 0)?;
    let label = parameter_string(params, 1)?;
    let ms = parameter_int(params, 2)?.saturating_mul(1000);
    simple(move |_, _, _, state| {
        state.cine.notification = Some(NotificationCmd {
            seq: bump(state.cine.notification.as_ref().map(|n| n.seq)),
            kind: kind.clone(),
            label: label.clone(),
            ms,
            object_type: object_type.clone(),
        });
    })
}

pub(super) fn parse_notification_box(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    notification(params, String::new())
}

pub(super) fn parse_notification_box_with_image(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    notification(params, parameter_string(params, 3)?)
}

pub(super) fn parse_show_objective(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let number = parameter_int(params, 0)?;
    simple(move |_, _, _, state| {
        crate::diag::diag!(
            "script: SHOW_MISSION_OBJECTIVE {number} of {}",
            state.cine.objectives.len()
        );
        state.cine.show_objective(number);
    })
}

pub(super) fn parse_hide_objective(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let number = parameter_int(params, 0)?;
    simple(move |_, _, _, state| state.cine.hide_objective(number))
}

pub(super) fn parse_complete_objective(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let number = parameter_int(params, 0)?;
    complete(number, true)
}

pub(super) fn parse_uncomplete_objective(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let number = parameter_int(params, 0)?;
    complete(number, false)
}

fn complete(number: i32, completed: bool) -> Parsed {
    Some(Arc::new(Simple {
        unit: None,
        run: move |_: &RotwkRules, tick: &Tick<'_>, _: Ids, state: &mut MapScriptState| {
            state
                .cine
                .complete_objective(number, completed, tick.frame().get());
        },
    }))
}

pub(super) fn parse_radar_create_event(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let Some(Parameter::Coord3D { x, y, .. }) = params.first() else {
        return None;
    };
    let (at, kind) = (Vec2::new(*x, *y), parameter_int(params, 1)?);
    simple(move |_, _, _, state| state.cine.push_radar(at, kind))
}

pub(super) fn parse_object_radar_event(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let kind = parameter_int(params, 1)?;
    let lookup = name.clone();
    Some(Arc::new(Simple {
        unit: Some(name),
        run: move |rules: &RotwkRules, tick: &Tick<'_>, ids: Ids, state: &mut MapScriptState| {
            let at = rules
                .object_named(tick, ids, &state.rt, &lookup, "")
                .and_then(|entity| tick.state().get(ids.body, entity))
                .map(|body| body.0.position);
            if let Some(at) = at {
                state.cine.push_radar(at, kind);
            }
        },
    }))
}

pub(super) fn parse_refresh_radar(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| {
        state.cine.radar_refresh_seq = state.cine.radar_refresh_seq.wrapping_add(1);
    })
}

#[allow(clippy::cast_precision_loss, dead_code)]
pub(crate) fn mean_position(members: &[Vec2]) -> Option<Vec2> {
    if members.is_empty() {
        return None;
    }
    let (mut x, mut y) = (0.0_f32, 0.0_f32);
    for member in members {
        x += member.x;
        y += member.y;
    }
    let reciprocal = 1.0 / members.len() as f32;
    Some(Vec2::new(x * reciprocal, y * reciprocal))
}
