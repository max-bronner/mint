use std::sync::Arc;

use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{
    parameter_int, parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState,
    ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};
use mint_movement::Vec2;
use mint_sage_map::{NamedCamera, Parameter};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CameraMove {
    pub(crate) seq: u32,
    pub(crate) target: Vec2,
    pub(crate) camera: Option<NamedCamera>,
    pub(crate) ms: i32,
    pub(crate) ease_in_ms: i32,
    pub(crate) ease_out_ms: i32,
    pub(crate) zoom: Option<f32>,
    pub(crate) pitch: Option<f32>,
    pub(crate) look_toward: Option<Vec2>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ScriptView {
    pub(crate) camera_move: Option<CameraMove>,
    pub(crate) fade: Option<ScriptFade>,
    pub(crate) clip_depth_multiplier: Option<f32>,
    pub(crate) ui_hidden: bool,
    pub(crate) terrain_render_disabled: bool,
    pub(crate) effects: super::view_effects::ViewEffects,
    pub(crate) camera_ops: super::camera_ops::CameraOps,
    pub(crate) cine: super::cinematic::CineState,
    pub(crate) mission_end: Option<mint_sage_campaign::MissionEnd>,
    pub(crate) victory_screen_seq: u32,
    pub(crate) hud: super::hud::HudView,
    pub(crate) ui: super::ui_facts::UiView,
}

impl MapScriptState {
    pub(crate) fn view(&self) -> ScriptView {
        ScriptView {
            camera_move: self.camera_move.clone(),
            fade: self.fade.map(|fade| ScriptFade {
                kind: fade.kind,
                previous: fade.previous(),
                value: fade.value,
            }),
            clip_depth_multiplier: self.clip_depth_multiplier,
            ui_hidden: self.ui_hidden,
            terrain_render_disabled: self.terrain_render_disabled,
            effects: self.effects,
            camera_ops: self.camera_ops.clone(),
            cine: self.cine.clone(),
            mission_end: self.mission_end,
            victory_screen_seq: self.victory_screen_seq,
            hud: self.hud.view(&self.counters),
            ui: self.ui.view(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FadeKind {
    Add,
    Subtract,
    Saturate,
    Multiply,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScriptFade {
    pub(crate) kind: FadeKind,
    previous: f32,
    pub(crate) value: f32,
}

impl ScriptFade {
    pub(crate) fn drawn(&self, progress: f32) -> f32 {
        if self.previous < 0.0 {
            return self.value;
        }
        let progress = progress.clamp(0.0, 1.0);
        self.previous * (1.0 - progress) + self.value * progress
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fade {
    pub(crate) kind: FadeKind,
    min: f32,
    max: f32,
    up: i32,
    hold: i32,
    down: i32,
    frame: i32,
    pub(crate) value: f32,
}

impl Fade {
    fn begin(kind: FadeKind, min: f32, max: f32, up: i32, hold: i32, down: i32) -> Option<Self> {
        let mut fade = Self {
            kind,
            min,
            max,
            up,
            hold,
            down,
            frame: 0,
            value: min,
        };
        if up == 0 {
            return fade.step().then_some(fade);
        }
        Some(fade)
    }

    pub(crate) fn step(&mut self) -> bool {
        self.frame = self.frame.saturating_add(1);
        match self.value_at(self.frame) {
            Some(value) => {
                self.value = value;
                true
            }
            None => false,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn value_at(&self, frame: i32) -> Option<f32> {
        if frame <= self.up {
            let factor = frame as f32 / self.up as f32;
            return Some(self.min + factor * (self.max - self.min));
        }
        let after_up = frame.saturating_sub(self.up);
        if after_up <= self.hold {
            return Some(self.max);
        }
        let into_down = after_up.saturating_sub(self.hold);
        if into_down <= self.down {
            let factor = into_down as f32 / self.down.saturating_add(1) as f32;
            return Some(self.max + factor * (self.min - self.max));
        }
        None
    }

    fn previous(&self) -> f32 {
        let just_set = self.frame == i32::from(self.up == 0);
        if just_set {
            return self.value;
        }
        self.value_at(self.frame.saturating_sub(1))
            .unwrap_or(self.value)
    }
}

struct ActionMoveCameraTo {
    target: Option<(Vec2, Option<NamedCamera>)>,
    ms: i32,
    ease_in_ms: i32,
    ease_out_ms: i32,
}
impl ActionVerb for ActionMoveCameraTo {
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
        let Some((target, camera)) = &self.target else {
            diag!("script: MOVE_CAMERA_TO names neither a waypoint nor a camera - no move");
            return Ok(());
        };
        let seq = state
            .camera_move
            .as_ref()
            .map_or(1, |m| m.seq.wrapping_add(1));
        state.camera_move = Some(CameraMove {
            seq,
            target: *target,
            camera: camera.clone(),
            ms: self.ms,
            ease_in_ms: self.ease_in_ms,
            ease_out_ms: self.ease_out_ms,
            zoom: None,
            pitch: None,
            look_toward: None,
        });
        Ok(())
    }
}

#[allow(clippy::cast_possible_truncation)]
fn seconds_to_ms(seconds: f32) -> i32 {
    (seconds * 1000.0) as i32
}

pub(super) fn parse_move_camera_to(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let name = parameter_string(params, 0)?;
    let camera = ctx.named_cameras.get(&name);
    let waypoint = ctx.waypoints.get(&name).map(|(at, _)| *at);
    let target = match (camera, waypoint) {
        (Some(camera), _) => Some((
            Vec2::new(camera.look_at[0], camera.look_at[1]),
            Some(camera.clone()),
        )),
        (None, Some(at)) => Some((at, None)),
        (None, None) => None,
    };
    Some(Arc::new(ActionMoveCameraTo {
        target,
        ms: seconds_to_ms(parameter_real(params, 1)?).max(1),
        ease_in_ms: seconds_to_ms(parameter_real(params, 3)?),
        ease_out_ms: seconds_to_ms(parameter_real(params, 4)?),
    }))
}

struct ActionFade {
    kind: FadeKind,
    min: f32,
    max: f32,
    up: i32,
    hold: i32,
    down: i32,
}
impl ActionVerb for ActionFade {
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
        state.fade = Fade::begin(self.kind, self.min, self.max, self.up, self.hold, self.down);
        Ok(())
    }
}

fn parse_fade(kind: FadeKind, params: &[Parameter]) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionFade {
        kind,
        min: parameter_real(params, 0)?,
        max: parameter_real(params, 1)?,
        up: parameter_int(params, 2)?,
        hold: parameter_int(params, 3)?,
        down: parameter_int(params, 4)?,
    }))
}

pub(super) fn parse_fade_add(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_fade(FadeKind::Add, params)
}

pub(super) fn parse_fade_subtract(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_fade(FadeKind::Subtract, params)
}

pub(super) fn parse_fade_saturate(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_fade(FadeKind::Saturate, params)
}

pub(super) fn parse_fade_multiply(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_fade(FadeKind::Multiply, params)
}

struct ActionClipDepth(f32);
impl ActionVerb for ActionClipDepth {
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
        state.clip_depth_multiplier = Some(self.0);
        Ok(())
    }
}

pub(super) fn parse_clip_depth(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionClipDepth(parameter_real(params, 0)?)))
}

struct ActionSetUiHidden(bool);
impl ActionVerb for ActionSetUiHidden {
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
        state.ui_hidden = self.0;
        Ok(())
    }
}

pub(super) fn parse_hide_ui(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetUiHidden(true)))
}

pub(super) fn parse_show_ui(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetUiHidden(false)))
}

struct ActionTerrainRenderDisable(bool);
impl ActionVerb for ActionTerrainRenderDisable {
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
        state.terrain_render_disabled = self.0;
        Ok(())
    }
}

pub(super) fn parse_terrain_render_disable(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTerrainRenderDisable(
        parameter_int(params, 0)? != 0,
    )))
}

struct ActionSetBloom(bool);
impl ActionVerb for ActionSetBloom {
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
        state.bloom = Some(self.0);
        Ok(())
    }
}

pub(super) fn parse_bloom_begin(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetBloom(true)))
}

pub(super) fn parse_bloom_end(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetBloom(false)))
}
