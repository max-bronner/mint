use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_audio::MusicRequest;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::counters::{SideKey, TICKS_PER_SCRIPT_FRAME};
use super::{
    parameter_int, parameter_real, parameter_string, ActionVerb, ConditionVerb, MapScriptEngine,
    MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::music_script::MusicCommand;
use crate::rules::{Ids, RotwkRules};

const PENDING_CAP: usize = 32;

const FRAME_MS: u64 = 1000 / RETAIL_LOGIC_HZ as u64;

const MUSIC_INIT_FLAG: &str = "/___MusicScript_Init";

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AudioOp {
    Sound {
        name: String,
        at: Option<Vec2>,
        owner: Option<Entity>,
        allow_overlap: bool,
    },
    ObjectSound {
        entity: Entity,
        enabled: bool,
    },
    FadeImmunity {
        name: String,
        immune: bool,
    },
    AllSubjectToFade,
    FadeVolume {
        from: f32,
        to: f32,
        ramp: f32,
        hold: f32,
        back: f32,
    },
    MusicVolume(f32),
    Background {
        suspended: bool,
    },
    Eva(bool),
    DisableEvent(String),
    EnableEvent(String),
    EnableAllEvents,
    Music(MusicCommand),
    MusicTurnOff {
        fade: bool,
    },
    MusicReturn {
        fade: bool,
    },
    MusicReset {
        fade: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AudioCmd {
    pub(crate) serial: u32,
    pub(crate) op: AudioOp,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ScriptAudio {
    serial: u32,
    pub(crate) pending: Vec<AudioCmd>,
    waiting: BTreeMap<String, u64>,
    disabled: BTreeSet<String>,
}

impl ScriptAudio {
    pub(crate) fn push(&mut self, op: AudioOp) {
        match &op {
            AudioOp::DisableEvent(name) => {
                self.disabled.insert(name.to_ascii_lowercase());
            }
            AudioOp::EnableEvent(name) => {
                self.disabled.remove(&name.to_ascii_lowercase());
            }
            AudioOp::EnableAllEvents => self.disabled.clear(),
            _ => {}
        }
        self.serial = self.serial.wrapping_add(1);
        if self.pending.len() >= PENDING_CAP {
            self.pending.remove(0);
        }
        self.pending.push(AudioCmd {
            serial: self.serial,
            op,
        });
    }

    pub(crate) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.serial == 0 && self.waiting.is_empty() {
            return;
        }
        hasher.write_u32(self.serial);
        hasher.write_usize(self.waiting.len());
        for (key, deadline) in &self.waiting {
            hasher.write_bytes(key.as_bytes());
            hasher.write_u8(0);
            hasher.write_u64(*deadline);
        }
        if !self.disabled.is_empty() {
            hasher.write_usize(self.disabled.len());
            for name in &self.disabled {
                hasher.write_bytes(name.as_bytes());
                hasher.write_u8(0);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn event_disabled(&self, name: &str) -> bool {
        self.disabled.contains(&name.to_ascii_lowercase())
    }

    pub(super) fn finished(&mut self, key: &str, length_ms: u64, frame: u64) -> bool {
        let deadline = *self
            .waiting
            .entry(key.to_string())
            .or_insert_with(|| frame.saturating_add(length_ms / FRAME_MS));
        let done = frame >= deadline;
        if done {
            self.waiting.remove(key);
        }
        done
    }
}

fn script_frame(tick: &Tick<'_>) -> u64 {
    tick.frame().get() / TICKS_PER_SCRIPT_FRAME
}

struct ConditionHasFinishedAudio {
    name: String,
}

impl ConditionVerb for ConditionHasFinishedAudio {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        match rules.sound_length_ms(&self.name) {
            None => true,
            Some(_) => state
                .audio
                .waiting
                .get(&self.name.to_ascii_lowercase())
                .is_some_and(|deadline| script_frame(tick) >= *deadline),
        }
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        _: Ids,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let Some(length_ms) = rules.sound_length_ms(&self.name) else {
            return true;
        };
        let done = state.audio.finished(
            &self.name.to_ascii_lowercase(),
            length_ms.into(),
            script_frame(tick),
        );
        if done {
            diag!("script: HAS_FINISHED_AUDIO {} fires", self.name);
        }
        done
    }

    fn finished_audio_name(&self) -> Option<&str> {
        Some(&self.name)
    }
}

pub(super) fn parse_has_finished_audio(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionHasFinishedAudio {
        name: parameter_string(params, 0)?,
    }))
}

type Parsed = Option<Arc<dyn ActionVerb>>;

struct Push<F> {
    unit: Option<String>,
    build: F,
}

impl<F> ActionVerb for Push<F>
where
    F: Fn(&RotwkRules, &Tick<'_>, Ids, &MapScriptEngine, &MapScriptState) -> Option<AudioOp>
        + Send
        + Sync,
{
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if let Some(op) = (self.build)(rules, tick, ids, engine, state) {
            diag!("script: audio {op:?}");
            state.audio.push(op);
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.unit.as_deref()
    }
}

fn push<F>(build: F) -> Parsed
where
    F: Fn(&RotwkRules, &Tick<'_>, Ids, &MapScriptEngine, &MapScriptState) -> Option<AudioOp>
        + Send
        + Sync
        + 'static,
{
    Some(Arc::new(Push { unit: None, build }))
}

fn fixed(op: AudioOp) -> Parsed {
    push(move |_, _, _, _, _| Some(op.clone()))
}

fn position_of(tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Vec2> {
    tick.state()
        .get(ids.body, entity)
        .map(|body| body.0.position)
}

pub(super) fn parse_play_sound_effect(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    fixed(AudioOp::Sound {
        name,
        at: None,
        owner: None,
        allow_overlap: true,
    })
}

pub(super) fn parse_speech_play(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let allow_overlap = parameter_int(params, 1)? != 0;
    fixed(AudioOp::Sound {
        name,
        at: None,
        owner: None,
        allow_overlap,
    })
}

pub(super) fn parse_play_sound_effect_at(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let waypoint = parameter_string(params, 1)?;
    let Some((at, _)) = ctx.waypoints.get(&waypoint).copied() else {
        return no_op();
    };
    fixed(AudioOp::Sound {
        name,
        at: Some(at),
        owner: None,
        allow_overlap: true,
    })
}

fn no_op() -> Parsed {
    push(|_, _, _, _, _| None)
}

pub(super) fn parse_sound_play_named(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let unit = parameter_string(params, 1)?;
    let lookup = unit.clone();
    Some(Arc::new(Push {
        unit: Some(unit),
        build: move |rules: &RotwkRules,
                     tick: &Tick<'_>,
                     ids: Ids,
                     _: &MapScriptEngine,
                     state: &MapScriptState| {
            let entity = rules.object_named(tick, ids, &state.rt, &lookup, "")?;
            Some(AudioOp::Sound {
                name: name.clone(),
                at: position_of(tick, ids, entity),
                owner: Some(entity),
                allow_overlap: true,
            })
        },
    }))
}

pub(super) fn parse_play_sound_effect_at_team(params: &[Parameter], ctx: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let team = SideKey::qualify(&parameter_string(params, 1)?, ctx.side);
    push(
        move |_: &RotwkRules,
              tick: &Tick<'_>,
              ids: Ids,
              engine: &MapScriptEngine,
              state: &MapScriptState| {
            let proto = engine.team_protos.find(&team.side, &team.name)?;
            let member = state
                .rt
                .teams
                .instances_of(proto)
                .into_iter()
                .filter_map(|id| state.rt.teams.get(id))
                .flat_map(|instance| instance.members().iter().copied())
                .find(|member| tick.state().is_live(*member))?;
            Some(AudioOp::Sound {
                name: name.clone(),
                at: position_of(tick, ids, member),
                owner: None,
                allow_overlap: true,
            })
        },
    )
}

fn object_sound(params: &[Parameter], enabled: bool) -> Parsed {
    let unit = parameter_string(params, 0)?;
    let lookup = unit.clone();
    Some(Arc::new(Push {
        unit: Some(unit),
        build: move |rules: &RotwkRules,
                     tick: &Tick<'_>,
                     ids: Ids,
                     _: &MapScriptEngine,
                     state: &MapScriptState| {
            let entity = rules.object_named(tick, ids, &state.rt, &lookup, "")?;
            Some(AudioOp::ObjectSound { entity, enabled })
        },
    }))
}

pub(super) fn parse_enable_object_sound(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    object_sound(params, true)
}

pub(super) fn parse_disable_object_sound(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    object_sound(params, false)
}

pub(super) fn parse_sound_disable_type(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::DisableEvent(parameter_string(params, 0)?))
}

pub(super) fn parse_sound_enable_type(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::EnableEvent(parameter_string(params, 0)?))
}

pub(super) fn parse_sound_enable_all(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::EnableAllEvents)
}

pub(super) fn parse_immune_to_fade(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::FadeImmunity {
        name: parameter_string(params, 0)?,
        immune: true,
    })
}

pub(super) fn parse_subject_to_fade(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::FadeImmunity {
        name: parameter_string(params, 0)?,
        immune: false,
    })
}

pub(super) fn parse_all_subject_to_fade(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::AllSubjectToFade)
}

pub(super) fn parse_fade_volume(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let real = |index| parameter_real(params, index);
    fixed(AudioOp::FadeVolume {
        from: real(0)?.clamp(0.0, 1.0),
        to: real(1)?.clamp(0.0, 1.0),
        ramp: real(2)?.max(0.0),
        hold: real(3)?.max(0.0),
        back: real(4)?.max(0.0),
    })
}

pub(super) fn parse_music_set_volume(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::MusicVolume(
        (parameter_real(params, 0)? / 100.0).clamp(0.0, 1.0),
    ))
}

pub(super) fn parse_suspend_background(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Background { suspended: true })
}

pub(super) fn parse_resume_background(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Background { suspended: false })
}

pub(super) fn parse_eva_enabled(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Eva(parameter_int(params, 0)? != 0))
}

pub(super) fn parse_reverb_none(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    no_op()
}

fn finite_request(params: &[Parameter], with_notify: bool) -> Option<MusicRequest> {
    let notify = if with_notify {
        parameter_string(params, 4)?
    } else {
        String::new()
    };
    Some(MusicRequest {
        times: Some(u32::try_from(parameter_int(params, 1)?).unwrap_or(1).max(1)),
        fade_out: parameter_int(params, 2)? != 0,
        fade_in: parameter_int(params, 3)? != 0,
        notify: (!notify.is_empty()).then_some(notify),
        ..MusicRequest::forever(&parameter_string(params, 0)?)
    })
}

pub(super) fn parse_music_play_finite(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Music(MusicCommand::Replace(finite_request(
        params, false,
    )?)))
}

pub(super) fn parse_music_play_finite_notify(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Music(MusicCommand::Replace(finite_request(
        params, true,
    )?)))
}

pub(super) fn parse_push_music(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::Music(MusicCommand::Push(MusicRequest {
        fade_out: parameter_int(params, 1)? != 0,
        fade_in: parameter_int(params, 2)? != 0,
        ..MusicRequest::forever(&parameter_string(params, 0)?)
    })))
}

pub(super) fn parse_music_turn_off(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::MusicTurnOff {
        fade: parameter_int(params, 0)? != 0,
    })
}

pub(super) fn parse_music_return(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    fixed(AudioOp::MusicReturn {
        fade: parameter_int(params, 0)? != 0,
    })
}

pub(super) fn parse_music_reset(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let fade = parameter_int(params, 0)? != 0;
    Some(Arc::new(MusicReset { fade }))
}

struct MusicReset {
    fade: bool,
}

impl ActionVerb for MusicReset {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        state.audio.push(AudioOp::MusicReset { fade: self.fade });
        if let Some(flag) = state.flags.get_mut(&SideKey::qualify(MUSIC_INIT_FLAG, "")) {
            *flag = true;
        }
        Ok(())
    }
}
