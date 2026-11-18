use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_movement::{Facing, Vec2};
use mint_sage_map::{
    Condition, NamedCamera, Parameter, Script, ScriptAction, ScriptNode, ScriptTree,
};
use mint_sim::{Component, Entity, Frame, PlayerId, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::diag::diag;

mod audio;
mod border_verbs;
mod camera;
mod camera_ops;
mod carryover;
mod cinematic;
#[allow(dead_code)]
mod count_cache;
mod counter_math;
mod counter_verbs;
mod counters;
#[allow(dead_code)]
mod lists;
#[allow(dead_code)]
mod masks;
pub(crate) use audio::{AudioCmd, AudioOp};
pub(crate) use camera::{CameraMove, FadeKind, ScriptView};
pub(crate) use camera_ops::{AnimationPlay, CameraOps, ChannelMove};
#[cfg(test)]
pub(crate) use camera_ops::{Follow, LookObject, LookWaypoint, ResetMove};
#[cfg(test)]
pub(crate) use cinematic::mean_position;
pub(crate) use cinematic::{CaptionCmd, CineState, NotificationCmd, Objective, ObjectiveSoundKind};
pub(crate) use hud::{MovieCmd, TimerView, UiFlashTarget};
pub(crate) use ui_facts::{UiFacts, UiView};
pub(crate) use view_effects::ViewEffects;
mod area;
mod area_events;
mod attack_area;
#[cfg(test)]
mod border_tests;
mod build_verbs;
mod buildability;
mod built_upgrades;
mod burn_rate;
mod buttons;
mod emotion_verbs;
mod endgame;
mod experience_verbs;
mod face_teleport;
#[cfg(test)]
mod follow_tests;
mod garrison_verbs;
mod gate_verbs;
mod generic;
mod guard_verbs;
mod hud;
mod mission_end;
mod named;
mod nearest;
mod object_status;
pub(super) mod orders;
#[cfg(test)]
mod p46_tests;
#[cfg(test)]
mod p67_team_tests;
#[cfg(test)]
mod p67_tests;
mod player;
mod player_counts;
mod player_ops;
mod player_verbs;
mod power_cost;
mod powers;
mod priority;
#[allow(dead_code)]
mod refs;
mod relation;
#[allow(dead_code)]
mod relations;
mod reveal;
mod runtime;
#[cfg(test)]
mod s1_tests;
#[cfg(test)]
mod s5b_tests;
mod sequential;
mod sight_attacked;
mod skirmish_ready;
mod spawn_verbs;
mod stand_ground;
mod standing;
#[allow(dead_code)]
mod status;
#[allow(dead_code)]
mod team;
mod team_build;
mod team_build_queue;
mod team_build_release;
mod team_build_step;
#[cfg(test)]
mod team_build_tests;
mod team_build_verbs;
mod team_conditions;
mod team_orders;
mod team_recruit;
mod team_verbs;
mod topple;
mod tutorial_verbs;
mod ui_facts;
mod view_effects;
#[cfg(test)]
mod w3a_tests;
#[cfg(test)]
mod w3b_tests;
mod walks;
mod wander;
pub(crate) use built_upgrades::note_upgrade_completed;
pub(crate) use nearest::TeamWard;
pub(crate) use object_status::entity_has_script_status;
pub(crate) use orders::PathPoint;
pub(crate) use powers::note_special_power_triggered;
pub(crate) use relations::{Relation, Relations};
pub(crate) use relations::{
    ATTITUDE_AGGRESSIVE, ATTITUDE_ALERT, ATTITUDE_NORMAL, ATTITUDE_PASSIVE, ATTITUDE_PEACEFUL,
    ATTITUDE_SLEEP,
};
pub(crate) use status::ScriptStatusBit;
pub(crate) use team::{
    hand_retired_hordes_to_soldiers, note_object_count_changed, TeamId, TeamProtos,
};

use counters::{seconds_to_script_frames, SideKey, TICKS_PER_SCRIPT_FRAME};

const COMPARE_LESS_THAN: i32 = 0;
const COMPARE_LESS_EQUAL: i32 = 1;
const COMPARE_EQUAL: i32 = 2;
const COMPARE_GREATER_EQUAL: i32 = 3;
const COMPARE_GREATER: i32 = 4;
const COMPARE_NOT_EQUAL: i32 = 5;

const DELAY_EVAL_FIRST_SPAN: u32 = 2 * crate::game_data::RETAIL_LOGIC_HZ + 1;

const RUN_MODE: u8 = 1;
const MODE_STANDARD: u8 = 1;
const MODE_ENGINE: u8 = 3;

const MAX_SUBROUTINE_DEPTH: u8 = 16;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ScriptObjectName(pub(crate) String);

impl Component for ScriptObjectName {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.0.as_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ForcedModelCondition {
    pub(crate) name: String,
    pub(crate) until: Option<Frame>,
}

impl Component for ForcedModelCondition {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.name.as_bytes());
        hasher.write_bool(self.until.is_some());
        hasher.write_u64(self.until.map_or(0, Frame::get));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CounterValue {
    value: i32,
    countdown: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct MapScriptState {
    active: Vec<bool>,
    group_active: Vec<bool>,
    next_eval: Vec<Option<u64>>,
    counters: BTreeMap<SideKey, CounterValue>,
    flags: BTreeMap<SideKey, bool>,
    pub(crate) bloom: Option<bool>,
    pub(crate) camera_move: Option<camera::CameraMove>,
    pub(crate) fade: Option<camera::Fade>,
    pub(crate) clip_depth_multiplier: Option<f32>,
    pub(crate) ui_hidden: bool,
    pub(crate) terrain_render_disabled: bool,
    pub(crate) effects: view_effects::ViewEffects,
    pub(crate) camera_ops: camera_ops::CameraOps,
    pub(crate) cine: cinematic::CineState,
    pub(crate) audio: audio::ScriptAudio,
    pub(crate) mission_end: Option<mint_sage_campaign::MissionEnd>,
    pub(crate) victory_screen_seq: u32,
    pub(crate) carryover: Option<super::carryover::CarryoverRun>,
    pub(crate) wotr: Option<super::wotr_battle::WotrRun>,
    pub(crate) ever_existed: BTreeSet<String>,
    pub(crate) rt: runtime::ScriptRuntime,
    reveals: reveal::NamedReveals,
    hud: hud::HudState,
    pub(crate) ui: ui_facts::UiFacts,
}

impl MapScriptState {
    #[cfg(test)]
    pub(crate) fn script_active(&self, index: usize) -> bool {
        self.active.get(index).copied().unwrap_or(false)
    }

    #[cfg(test)]
    pub(crate) fn counter_value(&self, side: &str, name: &str) -> i32 {
        self.counters
            .get(&SideKey::qualify(&format!("{side}/{name}"), side))
            .map_or(0, |c| c.value)
    }
}

impl Component for MapScriptState {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.active.len());
        for &value in &self.active {
            hasher.write_bool(value);
        }
        hasher.write_usize(self.group_active.len());
        for &value in &self.group_active {
            hasher.write_bool(value);
        }
        hasher.write_usize(self.next_eval.len());
        for value in &self.next_eval {
            hasher.write_bool(value.is_some());
            hasher.write_u64(value.unwrap_or(0));
        }
        hasher.write_usize(self.counters.len());
        for (key, value) in &self.counters {
            hasher.write_bytes(key.side.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(key.name.as_bytes());
            hasher.write_i32(value.value);
            hasher.write_bool(value.countdown);
        }
        hasher.write_usize(self.flags.len());
        for (key, value) in &self.flags {
            hasher.write_bytes(key.side.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(key.name.as_bytes());
            hasher.write_bool(*value);
        }
        hasher.write_bool(self.bloom.is_some());
        hasher.write_bool(self.bloom == Some(true));
        match &self.camera_move {
            Some(m) => {
                hasher.write_bool(true);
                hasher.write_u32(m.seq);
            }
            None => hasher.write_bool(false),
        }
        match &self.fade {
            Some(fade) => {
                hasher.write_bool(true);
                hasher.write_u8(fade.kind as u8);
                hasher.write_f32(fade.value);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_f32(self.clip_depth_multiplier.unwrap_or(1.0));
        hasher.write_bool(self.ui_hidden);
        hasher.write_bool(self.terrain_render_disabled);
        self.effects.hash_into(hasher);
        self.camera_ops.hash_into(hasher);
        self.cine.hash_into(hasher);
        self.audio.hash_into(hasher);
        hasher.write_u8(match self.mission_end {
            None => 0,
            Some(mint_sage_campaign::MissionEnd::Victory) => 1,
            Some(mint_sage_campaign::MissionEnd::Defeat) => 2,
        });
        hasher.write_u32(self.victory_screen_seq);
        hasher.write_bool(self.carryover.is_some());
        if let Some(run) = &self.carryover {
            run.hash_into(hasher);
        }
        if let Some(run) = &self.wotr {
            run.hash_into(hasher);
        }
        hasher.write_usize(self.ever_existed.len());
        for name in &self.ever_existed {
            hasher.write_bytes(name.as_bytes());
        }
        self.rt.hash_into(hasher);
        self.reveals.hash_into(hasher);
        self.hud.hash_into(hasher);
        self.ui.hash_into(hasher);
    }
}

struct ParseCtx<'a> {
    waypoints: &'a BTreeMap<String, (Vec2, f32)>,
    trigger_areas: &'a BTreeMap<String, Vec<(f32, f32)>>,
    allow_ring_heroes: bool,
    named_cameras: &'a BTreeMap<String, NamedCamera>,
    side: &'a str,
    run_mode: u8,
    record_seq: &'a std::cell::Cell<u32>,
}

impl ParseCtx<'_> {
    fn next_cache_key(&self) -> count_cache::CacheKey {
        let key = self.record_seq.get();
        self.record_seq.set(key.saturating_add(1));
        count_cache::CacheKey(key)
    }
}

trait ConditionVerb: Send + Sync {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool;

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        self.eval(rules, tick, ids, state, player)
    }

    fn finished_audio_name(&self) -> Option<&str> {
        None
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        None
    }

    fn watched_area(&self) -> Option<&area::Area> {
        None
    }

    fn team_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn reads_triggered_powers(&self) -> bool {
        false
    }

    fn reads_completed_upgrades(&self) -> bool {
        false
    }
}

trait ActionVerb: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()>;

    fn referenced_template(&self) -> Option<&str> {
        None
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        None
    }
}

struct ConditionTrue;
impl ConditionVerb for ConditionTrue {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        _: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        true
    }
}

struct ConditionLiteral(bool);
impl ConditionVerb for ConditionLiteral {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        _: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        self.0
    }
}

struct ConditionCounter {
    key: SideKey,
    compare: i32,
    value: i32,
}
impl ConditionVerb for ConditionCounter {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        compare(
            state.counters.get(&self.key).map_or(0, |c| c.value),
            self.compare,
            self.value,
        )
    }
}

struct ConditionNamedCreated(String);
impl ConditionVerb for ConditionNamedCreated {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        _: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules.named_entity_exists(tick, ids, &self.0)
    }
}

struct ConditionTimerExpired(SideKey);
impl ConditionVerb for ConditionTimerExpired {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        state
            .counters
            .get(&self.0)
            .is_some_and(|c| c.countdown && c.value < 1)
    }
}

struct ConditionFlag {
    key: SideKey,
    value: bool,
}
impl ConditionVerb for ConditionFlag {
    fn eval(
        &self,
        _: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        state.flags.get(&self.key).copied().unwrap_or(false) == self.value
    }
}

fn set_active(
    engine: &MapScriptEngine,
    state: &mut MapScriptState,
    name: &ScriptName,
    value: bool,
) {
    let script = engine.find_script(&name.name, &name.side);
    let group = engine.find_group(&name.name, &name.side);
    if let Some(index) = script.and_then(|index| state.active.get_mut(index)) {
        *index = value;
    }
    if let Some(index) = group.and_then(|index| state.group_active.get_mut(index)) {
        *index = value;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DebugScriptValue {
    Flag(bool),
    Counter(i32),
    Timer(i32),
}

#[derive(Debug, Clone)]
struct ScriptName {
    name: String,
    side: String,
}

impl ScriptName {
    fn parse(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Self> {
        Some(Self {
            name: parameter_string(params, 0)?,
            side: ctx.side.to_owned(),
        })
    }
}

struct ActionEnable(ScriptName);
impl ActionVerb for ActionEnable {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        set_active(engine, state, &self.0, true);
        Ok(())
    }
}

struct ActionDisable(ScriptName);
impl ActionVerb for ActionDisable {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        set_active(engine, state, &self.0, false);
        Ok(())
    }
}

struct ActionSetRandomCounter {
    key: SideKey,
    min: i32,
    max: i32,
}
impl ActionVerb for ActionSetRandomCounter {
    fn run(
        &self,
        _: &RotwkRules,
        tick: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let span = self.max.saturating_sub(self.min).saturating_add(1).max(1);
        let draw = i32::try_from(tick.rng().below(u32::try_from(span).unwrap_or(1))).unwrap_or(0);
        state.counters.entry(self.key.clone()).or_default().value = self.min.saturating_add(draw);
        Ok(())
    }
}

struct ActionCreateNamed {
    unit_name: String,
    template: String,
    team: String,
    side: String,
    at: Option<(Vec2, f32)>,
}
impl ActionVerb for ActionCreateNamed {
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
        if rules
            .run_create_named(tick, ids, engine, state, self)?
            .is_some()
        {
            state.ever_existed.insert(self.unit_name.clone());
        }
        Ok(())
    }

    fn referenced_template(&self) -> Option<&str> {
        Some(&self.template)
    }
}

struct ActionSetTimer {
    key: SideKey,
    frames: i32,
}
impl ActionVerb for ActionSetTimer {
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
        start_timer(state, &self.key, self.frames);
        Ok(())
    }
}

fn start_timer(state: &mut MapScriptState, key: &SideKey, frames: i32) {
    state.counters.insert(
        key.clone(),
        CounterValue {
            value: frames,
            countdown: true,
        },
    );
}

struct ActionSetRandomMsecTimer {
    key: SideKey,
    low: i32,
    high: i32,
}
impl ActionVerb for ActionSetRandomMsecTimer {
    fn run(
        &self,
        _: &RotwkRules,
        tick: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let span = self.high.saturating_sub(self.low).saturating_add(1).max(1);
        let draw = i32::try_from(tick.rng().below(u32::try_from(span).unwrap_or(1))).unwrap_or(0);
        let seconds = self.low.saturating_add(draw);
        #[allow(clippy::cast_precision_loss)]
        let frames = seconds_to_script_frames(seconds as f32);
        start_timer(state, &self.key, frames);
        Ok(())
    }
}

struct ActionSetFlag {
    key: SideKey,
    value: bool,
}
impl ActionVerb for ActionSetFlag {
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
        state.flags.insert(self.key.clone(), self.value);
        Ok(())
    }
}

struct ActionCallSubroutine(ScriptName);
impl ActionVerb for ActionCallSubroutine {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        rules.run_subroutine(tick, ids, engine, state, &self.0, player, depth, kills)
    }
}

fn parameter_string(params: &[Parameter], index: usize) -> Option<String> {
    match params.get(index)? {
        Parameter::Value { string_value, .. } => Some(string_value.clone()),
        Parameter::Coord3D { .. } => None,
    }
}

fn parameter_int(params: &[Parameter], index: usize) -> Option<i32> {
    match params.get(index)? {
        Parameter::Value { int_value, .. } => Some(*int_value),
        Parameter::Coord3D { .. } => None,
    }
}

fn parameter_real(params: &[Parameter], index: usize) -> Option<f32> {
    match params.get(index)? {
        Parameter::Value { real_value, .. } => Some(*real_value),
        Parameter::Coord3D { .. } => None,
    }
}

fn parse_true(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTrue))
}

fn parse_game_mode_active(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    let mode = parameter_string(params, 0)?;
    if mode.eq_ignore_ascii_case("ringheroes") {
        Some(Arc::new(ConditionLiteral(ctx.allow_ring_heroes)))
    } else {
        None
    }
}

fn side_key(params: &[Parameter], index: usize, ctx: &ParseCtx<'_>) -> Option<SideKey> {
    Some(SideKey::qualify(
        &parameter_string(params, index)?,
        ctx.side,
    ))
}

fn parse_counter(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionCounter {
        key: side_key(params, 0, ctx)?,
        compare: parameter_int(params, 1)?,
        value: parameter_int(params, 2)?,
    }))
}

fn parse_named_created(params: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionNamedCreated(parameter_string(
        params, 0,
    )?)))
}

fn parse_timer_expired(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTimerExpired(side_key(params, 0, ctx)?)))
}

fn parse_flag(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionFlag {
        key: side_key(params, 0, ctx)?,
        value: parameter_int(params, 1)? != 0,
    }))
}

fn parse_enable(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionEnable(ScriptName::parse(params, ctx)?)))
}

fn parse_disable(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionDisable(ScriptName::parse(params, ctx)?)))
}

fn parse_set_random_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetRandomCounter {
        key: side_key(params, 0, ctx)?,
        min: parameter_int(params, 1)?,
        max: parameter_int(params, 2)?,
    }))
}

fn parse_create_named(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 3)?;
    Some(Arc::new(ActionCreateNamed {
        unit_name: parameter_string(params, 0)?,
        template: parameter_string(params, 1)?,
        team: parameter_string(params, 2)?,
        side: ctx.side.to_owned(),
        at: ctx.waypoints.get(&waypoint).copied(),
    }))
}

fn parse_set_timer(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetTimer {
        key: side_key(params, 0, ctx)?,
        frames: parameter_int(params, 1)?,
    }))
}

fn parse_set_millisecond_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetTimer {
        key: side_key(params, 0, ctx)?,
        frames: seconds_to_script_frames(parameter_real(params, 1)?),
    }))
}

#[allow(clippy::cast_possible_truncation)]
fn parse_set_random_msec_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetRandomMsecTimer {
        key: side_key(params, 0, ctx)?,
        low: parameter_real(params, 1)? as i32,
        high: parameter_real(params, 2)? as i32,
    }))
}

fn parse_set_flag(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSetFlag {
        key: side_key(params, 0, ctx)?,
        value: parameter_int(params, 1)? != 0,
    }))
}

fn parse_call_subroutine(params: &[Parameter], ctx: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionCallSubroutine(ScriptName::parse(
        params, ctx,
    )?)))
}

type ConditionParse = fn(&[Parameter], &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>>;
type ActionParse = fn(&[Parameter], &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>>;

struct ConditionRow {
    name: &'static str,
    parse: ConditionParse,
}

struct ActionRow {
    name: &'static str,
    parse: ActionParse,
    mode_mask: u8,
    type_id: Option<i32>,
}

impl ActionRow {
    const fn new(name: &'static str, parse: ActionParse, mode_mask: u8) -> Self {
        Self {
            name,
            parse,
            mode_mask,
            type_id: None,
        }
    }

    const fn with_id(mut self, type_id: i32) -> Self {
        self.type_id = Some(type_id);
        self
    }
}

fn find_row<'a, T>(
    rows: &'a [T],
    name: &str,
    stored_id: i32,
    name_of: impl Fn(&T) -> &str,
    id_of: impl Fn(&T) -> Option<i32>,
) -> Option<&'a T> {
    rows.iter()
        .find(|row| name_of(row) == name && id_of(row) == Some(stored_id))
        .or_else(|| rows.iter().find(|row| name_of(row) == name))
}

const CONDITION_REGISTRY: &[ConditionRow] = &[
    ConditionRow {
        name: "NAMED_SELECTED",
        parse: hud::parse_named_selected,
    },
    ConditionRow {
        name: "TYPE_SELECTED",
        parse: hud::parse_type_selected,
    },
    ConditionRow {
        name: "SPELL_STORE_IS_OPEN",
        parse: tutorial_verbs::parse_spell_store_open,
    },
    ConditionRow {
        name: "OBJECTIVES_SCREEN_IS_OPEN",
        parse: tutorial_verbs::parse_objectives_open,
    },
    ConditionRow {
        name: "TOGGLE_STANCE_SUBMENU_IS_OPEN",
        parse: tutorial_verbs::parse_stance_submenu_open,
    },
    ConditionRow {
        name: "CAMERA_SCROLL_DISTANCE",
        parse: tutorial_verbs::parse_camera_scroll_distance,
    },
    ConditionRow {
        name: "PLAYER_ISSUED_FORMATION_ORDER",
        parse: tutorial_verbs::parse_formation_order_issued,
    },
    ConditionRow {
        name: "UNIT_USING_STANCE",
        parse: tutorial_verbs::parse_unit_using_stance,
    },
    ConditionRow {
        name: "CONDITION_TRUE",
        parse: parse_true,
    },
    ConditionRow {
        name: "IS_GAME_MODE_ACTIVE",
        parse: parse_game_mode_active,
    },
    ConditionRow {
        name: "UNIT_HAS_OBJECT_STATUS",
        parse: object_status::parse_unit_has_status,
    },
    ConditionRow {
        name: "TEAM_SOME_HAVE_OBJECT_STATUS",
        parse: object_status::parse_team_some_have_status,
    },
    ConditionRow {
        name: "NAMED_BUILDING_IS_EMPTY",
        parse: object_status::parse_building_is_empty,
    },
    ConditionRow {
        name: "NAMED_TOTALLY_DEAD",
        parse: object_status::parse_totally_dead,
    },
    ConditionRow {
        name: "UNIT_HEALTH",
        parse: team_conditions::parse_unit_health,
    },
    ConditionRow {
        name: "BUILT_BY_PLAYER",
        parse: player_counts::parse_built_by_player,
    },
    ConditionRow {
        name: "PLAYER_HAS_NUM_UNITS_WITH_UPGRADE",
        parse: player_counts::parse_num_units_with_upgrade,
    },
    ConditionRow {
        name: "PLAYER_HAS_SCIENCEPURCHASEPOINTS",
        parse: player_counts::parse_science_purchase_points,
    },
    ConditionRow {
        name: "PLAYER_HAS_NUMBER_OBJECTS_WITH_MODELCONDITION",
        parse: player_counts::parse_objects_with_model_condition,
    },
    ConditionRow {
        name: "SKIRMISH_PLAYER_HAS_COMPARISON_GARRISONED",
        parse: player_counts::parse_comparison_garrisoned,
    },
    ConditionRow {
        name: "PLAYER_HAS_KILLED_TYPE_UNITS",
        parse: player_counts::parse_has_killed_type_units,
    },
    ConditionRow {
        name: "SKIRMISH_SPECIAL_POWER_READY",
        parse: skirmish_ready::parse_power_ready,
    },
    ConditionRow {
        name: "SKIRMISH_COMMAND_BUTTON_READY_ALL",
        parse: skirmish_ready::parse_button_ready_all,
    },
    ConditionRow {
        name: "SKIRMISH_COMMAND_BUTTON_READY_PARTIAL",
        parse: skirmish_ready::parse_button_ready_partial,
    },
    ConditionRow {
        name: "UNIT_IS_AT_LEVEL",
        parse: experience_verbs::parse_unit_is_at_level,
    },
    ConditionRow {
        name: "UNIT_HAS_GAINED_LEVEL",
        parse: experience_verbs::parse_unit_has_gained_level,
    },
    ConditionRow {
        name: "NAMED_RANK_LEVEL",
        parse: experience_verbs::parse_rank_level,
    },
    ConditionRow {
        name: "COUNTER",
        parse: parse_counter,
    },
    ConditionRow {
        name: "NAMED_CREATED",
        parse: parse_named_created,
    },
    ConditionRow {
        name: "GATE_IS_OPEN",
        parse: gate_verbs::parse_is_open,
    },
    ConditionRow {
        name: "PLAYER_TRIGGERED_SPECIAL_POWER",
        parse: powers::parse_player_triggered,
    },
    ConditionRow {
        name: "TIMER_EXPIRED",
        parse: parse_timer_expired,
    },
    ConditionRow {
        name: "FLAG",
        parse: parse_flag,
    },
    ConditionRow {
        name: "NAMED_DESTROYED",
        parse: named::parse_named_destroyed,
    },
    ConditionRow {
        name: "COUNTER_SECONDS",
        parse: counter_verbs::parse_counter_seconds,
    },
    ConditionRow {
        name: "COUNTER_COUNTER",
        parse: counter_verbs::parse_counter_counter,
    },
    ConditionRow {
        name: "HAS_FINISHED_AUDIO",
        parse: audio::parse_has_finished_audio,
    },
    ConditionRow {
        name: "HAS_DELAYED_CARRYOVER_UNIT_OF_TYPE",
        parse: carryover::parse_has_delayed,
    },
    ConditionRow {
        name: "NAMED_OWNED_BY_PLAYER",
        parse: player_verbs::parse_named_owned_by_player,
    },
    ConditionRow {
        name: "PLAYER_HAS_CREDITS",
        parse: player_verbs::parse_player_has_credits,
    },
    ConditionRow {
        name: "PLAYER_ACQUIRED_SCIENCE",
        parse: player_verbs::parse_player_acquired_science,
    },
    ConditionRow {
        name: "PLAYER_BUILT_UPGRADE",
        parse: built_upgrades::parse_player_built_upgrade,
    },
    ConditionRow {
        name: "SKIRMISH_PLAYER_FACTION",
        parse: player_verbs::parse_skirmish_player_faction,
    },
    ConditionRow {
        name: "PLAYER_HAS_OBJECT_COMPARISON",
        parse: player_verbs::parse_player_has_object_comparison,
    },
    ConditionRow {
        name: "NAMED_DISCOVERED",
        parse: player_verbs::parse_named_discovered,
    },
    ConditionRow {
        name: "TEAM_DISCOVERED",
        parse: player_verbs::parse_team_discovered,
    },
    ConditionRow {
        name: "PLAYER_ALL_DESTROYED",
        parse: endgame::parse_player_all_destroyed,
    },
    ConditionRow {
        name: "MULTIPLAYER_PLAYER_DEFEAT",
        parse: endgame::parse_multiplayer_player_defeat,
    },
    ConditionRow {
        name: "PLAYER_LOST_OBJECT_TYPE",
        parse: player_ops::parse_player_lost_object_type,
    },
    ConditionRow {
        name: "SKIRMISH_PLAYER_HAS_BEEN_ATTACKED_BY_PLAYER",
        parse: player_ops::parse_attacked_by_player,
    },
    ConditionRow {
        name: "TEAM_HAS_UNITS",
        parse: team_verbs::parse_team_has_units,
    },
    ConditionRow {
        name: "TEAM_DESTROYED",
        parse: team_verbs::parse_team_destroyed,
    },
    ConditionRow {
        name: "TEAM_CREATED",
        parse: team_verbs::parse_team_created,
    },
    ConditionRow {
        name: "NAMED_NOT_DESTROYED",
        parse: team_verbs::parse_named_not_destroyed,
    },
    ConditionRow {
        name: "NAMED_INSIDE_AREA",
        parse: area::parse_named_inside,
    },
    ConditionRow {
        name: "NAMED_OUTSIDE_AREA",
        parse: area::parse_named_outside,
    },
    ConditionRow {
        name: "TEAM_INSIDE_AREA_ENTIRELY",
        parse: area::parse_team_inside_entirely,
    },
    ConditionRow {
        name: "TEAM_INSIDE_AREA_PARTIALLY",
        parse: area::parse_team_inside_partially,
    },
    ConditionRow {
        name: "TEAM_OUTSIDE_AREA_ENTIRELY",
        parse: area::parse_team_outside_entirely,
    },
    ConditionRow {
        name: "SKIRMISH_PLAYER_HAS_UNITS_IN_AREA",
        parse: area::parse_skirmish_has_units,
    },
    ConditionRow {
        name: "SKIRMISH_PLAYER_IS_OUTSIDE_AREA",
        parse: area::parse_skirmish_outside,
    },
    ConditionRow {
        name: "PLAYER_HAS_COMPARISON_UNIT_TYPE_IN_TRIGGER_AREA",
        parse: area::parse_type_in_area,
    },
    ConditionRow {
        name: "PLAYER_HAS_COMPARISON_UNIT_TYPE_IN_TRIGGER_AREA_COMPLETELY_BUILT",
        parse: area::parse_type_in_area_built,
    },
    ConditionRow {
        name: "NAMED_ENTERED_AREA",
        parse: area_events::parse_named_entered,
    },
    ConditionRow {
        name: "NAMED_EXITED_AREA",
        parse: area_events::parse_named_exited,
    },
    ConditionRow {
        name: "TEAM_ENTERED_AREA_ENTIRELY",
        parse: area_events::parse_team_entered_entirely,
    },
    ConditionRow {
        name: "TEAM_ENTERED_AREA_PARTIALLY",
        parse: area_events::parse_team_entered_partially,
    },
    ConditionRow {
        name: "TEAM_EXITED_AREA_ENTIRELY",
        parse: area_events::parse_team_exited_entirely,
    },
    ConditionRow {
        name: "TEAM_EXITED_AREA_PARTIALLY",
        parse: area_events::parse_team_exited_partially,
    },
    ConditionRow {
        name: "EVAL_TEAM_HEALTH",
        parse: team_conditions::parse_eval_team_health,
    },
    ConditionRow {
        name: "TEAM_HAS_FEWER_THAN_X_UNITS_WITH_KINDOF",
        parse: team_conditions::parse_team_more_than,
    },
    ConditionRow {
        name: "SKIRMISH_VALUE_IN_AREA",
        parse: team_conditions::parse_value_in_area,
    },
    ConditionRow {
        name: "TEAM_REACHED_WAYPOINTS_END",
        parse: team_conditions::parse_team_reached_path_end,
    },
    ConditionRow {
        name: "NAMED_ATTACKED_BY_PLAYER",
        parse: sight_attacked::parse_named_attacked_by_player,
    },
    ConditionRow {
        name: "TEAM_ATTACKED_BY_PLAYER",
        parse: sight_attacked::parse_team_attacked_by_player,
    },
    ConditionRow {
        name: "NAMED_ATTACKED_BY_OBJECTTYPE",
        parse: sight_attacked::parse_named_attacked_by_types,
    },
    ConditionRow {
        name: "TEAM_ATTACKED_BY_OBJECTTYPE",
        parse: sight_attacked::parse_team_attacked_by_types,
    },
    ConditionRow {
        name: "ENEMY_SIGHTED",
        parse: sight_attacked::parse_enemy_sighted,
    },
    ConditionRow {
        name: "ENEMY_SIGHTED_BY_TEAM",
        parse: sight_attacked::parse_enemy_sighted_by_team,
    },
    ConditionRow {
        name: "TYPE_SIGHTED",
        parse: sight_attacked::parse_type_sighted,
    },
];

const ACTION_REGISTRY: &[ActionRow] = &[
    ActionRow::new("ENABLE_SCRIPT", parse_enable, MODE_ENGINE),
    ActionRow::new("DISABLE_SCRIPT", parse_disable, MODE_ENGINE),
    ActionRow::new("SET_RANDOM_COUNTER", parse_set_random_counter, MODE_ENGINE),
    ActionRow::new(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        parse_create_named,
        MODE_STANDARD,
    ),
    ActionRow::new("SET_TIMER", parse_set_timer, MODE_STANDARD),
    ActionRow::new(
        "SET_MILLISECOND_TIMER",
        parse_set_millisecond_timer,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_RANDOM_MSEC_TIMER",
        parse_set_random_msec_timer,
        MODE_STANDARD,
    ),
    ActionRow::new("SET_FLAG", parse_set_flag, MODE_ENGINE),
    ActionRow::new("CALL_SUBROUTINE", parse_call_subroutine, MODE_ENGINE),
    ActionRow::new(
        "PLAYER_SET_MONEY",
        player::parse_player_set_money,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "OVERRIDE_PLAYER_COMMAND_POINTS",
        player::parse_override_command_points,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_SWITCH_BORDER",
        border_verbs::parse_map_switch_border,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "HUMAN_IMPASSABLE_AREA",
        border_verbs::parse_human_impassable_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ALLOW_DISALLOW_ONE_BUILDING",
        player::parse_allow_disallow_one_building,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ALLOW_DISALLOW_ALL_BUILDINGS",
        player::parse_allow_disallow_all_buildings,
        MODE_STANDARD,
    ),
    ActionRow::new("NAMED_KILL", named::parse_kill, MODE_STANDARD),
    ActionRow::new(
        "UNIT_SET_MODELCONDITION_FOR_DURATION",
        named::parse_set_model_condition,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_BLOOM_EFFECT_BEGIN",
        camera::parse_bloom_begin,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_BLOOM_EFFECT_END",
        camera::parse_bloom_end,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MOVE_CAMERA_TO",
        camera::parse_move_camera_to,
        MODE_STANDARD,
    ),
    ActionRow::new("CAMERA_FADE_ADD", camera::parse_fade_add, MODE_STANDARD),
    ActionRow::new(
        "CAMERA_FADE_SUBTRACT",
        camera::parse_fade_subtract,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_FADE_SATURATE",
        camera::parse_fade_saturate,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_FADE_MULTIPLY",
        camera::parse_fade_multiply,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_CAMERA_CLIP_DEPTH_MULTIPLIER",
        camera::parse_clip_depth,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_DELAYED_CARRYOVER_UNIT_AT_WAYPOINT",
        carryover::parse_create_delayed,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_UNIT_REVIVAL_ENTRY_FROM_DELAYED_CARRYOVER_HERO",
        carryover::parse_revival_entry,
        MODE_STANDARD,
    ),
    ActionRow::new("VICTORY", mission_end::parse_victory, MODE_STANDARD),
    ActionRow::new("QUICKVICTORY", mission_end::parse_victory, MODE_STANDARD),
    ActionRow::new("DEFEAT", mission_end::parse_defeat, MODE_STANDARD),
    ActionRow::new("LOCALDEFEAT", mission_end::parse_defeat, MODE_STANDARD),
    ActionRow::new(
        "VICTORY_SCREEN",
        mission_end::parse_victory_screen,
        MODE_STANDARD,
    ),
    ActionRow::new("HIDE_UI", camera::parse_hide_ui, MODE_STANDARD),
    ActionRow::new("SHOW_UI", camera::parse_show_ui, MODE_STANDARD),
    ActionRow::new(
        "DISABLE_BORDER_SHROUD",
        view_effects::parse_disable_border_shroud,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_BORDER_SHROUD",
        view_effects::parse_enable_border_shroud,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SCREEN_SHAKE",
        view_effects::parse_screen_shake,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TERRAIN_RENDER_DISABLE",
        camera::parse_terrain_render_disable,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_REVEAL_PERMANENTLY_IN_TRIGGER",
        reveal::parse_reveal_in_trigger,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_REVEAL_AT_WAYPOINT",
        reveal::parse_glance_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_REVEAL_IN_TRIGGER",
        reveal::parse_glance_in_trigger,
        MODE_STANDARD,
    )
    .with_id(552),
    ActionRow::new(
        "MAP_REVEAL_IN_TRIGGER",
        reveal::parse_nothing,
        MODE_STANDARD,
    )
    .with_id(553),
    ActionRow::new(
        "MAP_SHROUD_AT_WAYPOINT",
        reveal::parse_nothing,
        MODE_STANDARD,
    ),
    ActionRow::new("MAP_REVEAL_ALL", reveal::parse_reveal_all, MODE_STANDARD),
    ActionRow::new(
        "MAP_REVEAL_ALL_PERM",
        reveal::parse_reveal_all_permanently,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_REVEAL_ALL_UNDO_PERM",
        reveal::parse_undo_reveal_all_permanently,
        MODE_STANDARD,
    ),
    ActionRow::new("MAP_SHROUD_ALL", reveal::parse_shroud_all, MODE_STANDARD),
    ActionRow::new(
        "MAP_UNDO_REVEAL_PERMANENTLY_AT_WAYPOINT",
        reveal::parse_undo_named_reveal,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_UNDO_REVEAL_PERMANENTLY_IN_TRIGGER",
        reveal::parse_undo_named_reveal,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_LETTERBOX_BEGIN",
        cinematic::parse_letterbox_begin,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_LETTERBOX_END",
        cinematic::parse_letterbox_end,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_INPUT",
        cinematic::parse_disable_input,
        MODE_STANDARD,
    ),
    ActionRow::new("ENABLE_INPUT", cinematic::parse_enable_input, MODE_STANDARD),
    ActionRow::new("FREEZE_TIME", cinematic::parse_freeze_time, MODE_STANDARD),
    ActionRow::new(
        "UNFREEZE_TIME",
        cinematic::parse_unfreeze_time,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_MOD_FREEZE_TIME",
        camera_ops::parse_mod_freeze_time,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SHOW_MILITARY_CAPTION",
        cinematic::parse_military_caption,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISPLAY_NOTIFICATION_BOX",
        cinematic::parse_notification_box,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISPLAY_NOTIFICATION_BOX_WITH_OBJECT_TYPE_IMAGE_OVERRIDE",
        cinematic::parse_notification_box_with_image,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SHOW_MISSION_OBJECTIVE",
        cinematic::parse_show_objective,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "HIDE_MISSION_OBJECTIVE",
        cinematic::parse_hide_objective,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MARK_MISSION_OBJECTIVE_COMPLETED",
        cinematic::parse_complete_objective,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MARK_MISSION_OBJECTIVE_NOT_COMPLETED",
        cinematic::parse_uncomplete_objective,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "RADAR_CREATE_EVENT",
        cinematic::parse_radar_create_event,
        MODE_STANDARD,
    ),
    ActionRow::new("CAMEO_FLASH", hud::parse_cameo_flash, MODE_STANDARD),
    ActionRow::new(
        "HERO_SELECT_BUTTON_FLASH",
        hud::parse_hero_select_button_flash,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SELECT_BUILDER_BUTTON_FLASH",
        hud::parse_select_builder_button_flash,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FLASH_SPELL_STORE_BUTTON",
        hud::parse_flash_spell_store_button,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FLASH_OBJECTIVES_BUTTON",
        hud::parse_flash_objectives_button,
        MODE_STANDARD,
    ),
    ActionRow::new("NAMED_FLASH", hud::parse_named_flash, MODE_STANDARD),
    ActionRow::new(
        "NAMED_FLASH_WHITE",
        hud::parse_named_flash_white,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_FLASH", hud::parse_team_flash, MODE_STANDARD),
    ActionRow::new(
        "TEAM_FLASH_WHITE",
        hud::parse_team_flash_white,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_CREATE_RADAR_EVENT",
        hud::parse_team_radar_event,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_HOUSE_COLOR",
        hud::parse_enable_house_color,
        MODE_STANDARD,
    ),
    ActionRow::new("DISPLAY_COUNTER", hud::parse_display_counter, MODE_STANDARD),
    ActionRow::new(
        "DISPLAY_COUNTDOWN_TIMER",
        hud::parse_display_countdown_timer,
        MODE_STANDARD,
    ),
    ActionRow::new("HIDE_COUNTER", hud::parse_hide_timer, MODE_STANDARD),
    ActionRow::new("HIDE_COUNTDOWN_TIMER", hud::parse_hide_timer, MODE_STANDARD),
    ActionRow::new(
        "ENABLE_COUNTDOWN_TIMER_DISPLAY",
        hud::parse_enable_timer_display,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_COUNTDOWN_TIMER_DISPLAY",
        hud::parse_disable_timer_display,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "COMMANDBAR_ADD_BUTTON_OBJECTTYPE_SLOT",
        hud::parse_add_button_slot,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "COMMANDBAR_REMOVE_BUTTON_OBJECTTYPE",
        hud::parse_remove_button,
        MODE_STANDARD,
    ),
    ActionRow::new("PLAY_MOVIE_IN_GAME", hud::parse_play_movie, MODE_STANDARD),
    ActionRow::new("DESELECT", hud::parse_deselect, MODE_STANDARD),
    ActionRow::new("SELECT_OBJECT", hud::parse_select_object, MODE_STANDARD),
    ActionRow::new(
        "OBJECT_CREATE_RADAR_EVENT",
        cinematic::parse_object_radar_event,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "REFRESH_RADAR",
        cinematic::parse_refresh_radar,
        MODE_STANDARD,
    ),
    ActionRow::new("ZOOM_CAMERA", camera_ops::parse_zoom, MODE_STANDARD),
    ActionRow::new("PITCH_CAMERA", camera_ops::parse_pitch, MODE_STANDARD),
    ActionRow::new("ROTATE_CAMERA", camera_ops::parse_rotate, MODE_STANDARD),
    ActionRow::new("RESET_CAMERA", camera_ops::parse_reset, MODE_STANDARD),
    ActionRow::new(
        "ROTATE_CAMERA_LOCKED",
        camera_ops::parse_rotate_locked,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FOCAL_LENGTH_CAMERA",
        camera_ops::parse_focal_length,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_LOOK_TOWARD_OBJECT",
        camera_ops::parse_look_toward_object,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_LOOK_TOWARD_WAYPOINT",
        camera_ops::parse_look_toward_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_RESTRICT_TO_AREA",
        camera_ops::parse_restrict_to_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_REMOVE_AREA_RESTRICTION",
        camera_ops::parse_remove_area_restriction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MOVE_CAMERA_ALONG_SPLINE_PATH",
        camera_ops::parse_spline_path,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MOVE_CAMERA_LOCATOR_ALONG_SPLINE_PATH",
        camera_ops::parse_spline_path,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_MOD_LOOK_TOWARD",
        camera_ops::parse_look_toward,
        MODE_STANDARD,
    ),
    ActionRow::new("SETUP_CAMERA", camera_ops::parse_setup, MODE_STANDARD),
    ActionRow::new(
        "CAMERA_FOLLOW_NAMED",
        camera_ops::parse_follow,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_STOP_FOLLOW",
        camera_ops::parse_stop_follow,
        MODE_STANDARD,
    ),
    ActionRow::new("LOCK_CAMERA", camera_ops::parse_lock_camera, MODE_STANDARD),
    ActionRow::new(
        "OBJECTLIST_ADDOBJECTTYPE",
        priority::parse_objectlist_add,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "OBJECTLIST_REMOVEOBJECTTYPE",
        priority::parse_objectlist_remove,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_ATTACK_PRIORITY_THING",
        priority::parse_set_priority_thing,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_ATTACK_PRIORITY_KIND_OF",
        priority::parse_set_priority_kind_of,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_DEFAULT_ATTACK_PRIORITY",
        priority::parse_set_default_priority,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_APPLY_ATTACK_PRIORITY_SET",
        priority::parse_team_apply_priority,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_APPLY_ATTACK_PRIORITY_SET",
        priority::parse_named_apply_priority,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_RELATES_PLAYER",
        relation::parse_player_relates_player,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_OVERRIDE_RELATION_TO_TEAM",
        relation::parse_team_relation_to_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_OVERRIDE_RELATION_TO_PLAYER",
        relation::parse_team_relation_to_player,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_SET_OVERRIDE_RELATION_TO_TEAM",
        relation::parse_player_relation_to_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_REMOVE_ALL_OVERRIDE_RELATIONS",
        relation::parse_team_remove_all_relations,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_SET_ATTITUDE",
        relation::parse_named_set_attitude,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_ATTITUDE",
        relation::parse_team_set_attitude,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_GRANT_SCIENCE",
        player_verbs::parse_grant_science,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_SCIENCE_AVAILABILITY",
        player_verbs::parse_science_availability,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_GIVE_MONEY",
        player_verbs::parse_give_money,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "GIVE_PLAYER_UPGRADE",
        player_verbs::parse_give_upgrade,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_ENABLE_UNIT_CONSTRUCTION",
        player_verbs::parse_enable_unit_construction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_DISABLE_UNIT_CONSTRUCTION",
        player_verbs::parse_disable_unit_construction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_ENABLE_BASE_CONSTRUCTION",
        player_verbs::parse_enable_base_construction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_DISABLE_BASE_CONSTRUCTION",
        player_verbs::parse_disable_base_construction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "BUILD_TEAM",
        team_build_verbs::parse_build_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_BASE_CONSTRUCTION_SPEED",
        team_build_verbs::parse_set_base_construction_speed,
        MODE_STANDARD,
    ),
    ActionRow::new("PLAYER_KILL", endgame::parse_player_kill, MODE_STANDARD),
    ActionRow::new("PLAYER_HUNT", player_ops::parse_player_hunt, MODE_STANDARD),
    ActionRow::new(
        "IDLE_ALL_UNITS",
        player_ops::parse_idle_all_units,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_TRANSFER_OWNERSHIP_PLAYER",
        player_ops::parse_player_transfer,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_TRANSFER_OWNERSHIP_PLAYER",
        player_ops::parse_named_transfer,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_TRANSFER_TO_PLAYER",
        player_ops::parse_team_transfer,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_REF_TO_NEREST_TEAM_OF_TYPE_OWNED_BY_PLAYER",
        nearest::parse_of_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_REF_TO_NEREST_TEAM_OF_UNNAMED_TYPE_OWNED_BY_PLAYER",
        nearest::parse_of_unnamed_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "LOCK_CAMERA_ANGLE_AND_HEIGHT",
        camera_ops::parse_lock_angle_and_height,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CAMERA_MOVE_HOME",
        camera_ops::parse_move_home,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MOVE_CAMERA_BY_ANIMATION",
        camera_ops::parse_by_animation,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MAP_REVEAL_PERMANENTLY_AT_WAYPOINT",
        reveal::parse_reveal_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new("SET_COUNTER", counter_verbs::parse_set_counter, MODE_ENGINE),
    ActionRow::new(
        "INCREMENT_COUNTER",
        counter_verbs::parse_increment_counter,
        MODE_ENGINE,
    ),
    ActionRow::new(
        "DECREMENT_COUNTER",
        counter_verbs::parse_decrement_counter,
        MODE_ENGINE,
    ),
    ActionRow::new("STOP_TIMER", counter_verbs::parse_stop_timer, MODE_ENGINE),
    ActionRow::new(
        "RESTART_TIMER",
        counter_verbs::parse_restart_timer,
        MODE_ENGINE,
    ),
    ActionRow::new("NO_OP", counter_verbs::parse_no_op, MODE_ENGINE),
    ActionRow::new(
        "PLAY_SOUND_EFFECT",
        audio::parse_play_sound_effect,
        MODE_STANDARD,
    ),
    ActionRow::new("SPEECH_PLAY", audio::parse_speech_play, MODE_STANDARD),
    ActionRow::new(
        "PLAY_SOUND_EFFECT_AT",
        audio::parse_play_sound_effect_at,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SOUND_PLAY_NAMED",
        audio::parse_sound_play_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAY_SOUND_EFFECT_AT_TEAM",
        audio::parse_play_sound_effect_at_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_OBJECT_SOUND",
        audio::parse_enable_object_sound,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_OBJECT_SOUND",
        audio::parse_disable_object_sound,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "AUDIO_MAKE_SOUND_IMMUNE_TO_FADE",
        audio::parse_immune_to_fade,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "AUDIO_MAKE_SOUND_SUBJECT_TO_FADE",
        audio::parse_subject_to_fade,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "AUDIO_MAKE_ALL_SOUNDS_SUBJECT_TO_FADE",
        audio::parse_all_subject_to_fade,
        MODE_STANDARD,
    ),
    ActionRow::new("AUDIO_FADE_VOLUME", audio::parse_fade_volume, MODE_STANDARD),
    ActionRow::new(
        "MUSIC_SET_VOLUME",
        audio::parse_music_set_volume,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SUSPEND_BACKGROUND_SOUNDS",
        audio::parse_suspend_background,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "RESUME_BACKGROUND_SOUNDS",
        audio::parse_resume_background,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "EVA_SET_ENABLED_DISABLED",
        audio::parse_eva_enabled,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "AUDIO_SET_REVERB_ROOM_TYPE",
        audio::parse_reverb_none,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "AUDIO_SET_REVERB_SUPPRESSION_POLYGON",
        audio::parse_reverb_none,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MUSIC_PLAY_TRACK_FINITE_TIMES",
        audio::parse_music_play_finite,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MUSIC_PLAY_TRACK_FINITE_TIMES_AND_NOTIFY",
        audio::parse_music_play_finite_notify,
        MODE_STANDARD,
    ),
    ActionRow::new("AUDIO_PUSH_MUSIC", audio::parse_push_music, MODE_STANDARD),
    ActionRow::new(
        "MUSIC_TURN_OFF_MUSIC_SCRIPTING",
        audio::parse_music_turn_off,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MUSIC_RETURN_TO_MUSIC_SCRIPTING",
        audio::parse_music_return,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MUSIC_RESET_MUSIC_SCRIPTING_SYSTEM",
        audio::parse_music_reset,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_REINFORCEMENT_TEAM",
        team_verbs::parse_reinforce,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_UNNAMED_ON_TEAM_AT_WAYPOINT",
        team_verbs::parse_create_unnamed,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_DELETE",
        team_verbs::parse_named_delete,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_DELETE", team_verbs::parse_team_delete, MODE_STANDARD),
    ActionRow::new(
        "NAMED_RECEIVE_UPGRADE",
        team_verbs::parse_named_receive_upgrade,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_DELETE_LIVING",
        team_verbs::parse_team_delete_living,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_KILL", team_verbs::parse_team_kill, MODE_STANDARD),
    ActionRow::new(
        "UNIT_SET_TEAM",
        team_verbs::parse_unit_set_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_MERGE_INTO_TEAM",
        team_verbs::parse_merge_teams,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TECHTREE_MODIFY_BUILDABILITY_OBJECT",
        buildability::parse_modify_buildability,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_UPGRADE",
        team_verbs::parse_team_upgrade,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ABILITY",
        buttons::parse_named_ability,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT",
        buttons::parse_named_ability_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ABILITY_ON_NAMED",
        buttons::parse_named_ability_on_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_USE_COMMANDBUTTON_ABILITY",
        buttons::parse_team_ability,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_USE_COMMANDBUTTON_ABILITY_AT_WAYPOINT",
        buttons::parse_team_ability_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT",
        buttons::parse_named_nearest_enemy,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ON_NEAREST_KINDOF",
        buttons::parse_named_nearest_kindof,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_USE_COMMANDBUTTON_ON_NEAREST_OBJECTTYPE",
        buttons::parse_named_nearest_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_ALL_USE_COMMANDBUTTON_ON_NEAREST_ENEMY_UNIT",
        buttons::parse_team_all_nearest_enemy,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAME_SET_AUTO_ABILITY",
        buttons::parse_named_auto,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_AUTO_ABILITY",
        buttons::parse_team_auto,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_BUILD_STRUCTURE_AT_WAYPOINT",
        build_verbs::parse_build_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "BUILD_BUILDING_ON_FOUNDATION",
        build_verbs::parse_build_on_foundation,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_OBJECT",
        spawn_verbs::parse_create_object,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_SPAWN_NAMED_LOCATION_ORIENTATION",
        spawn_verbs::parse_spawn_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_NAMED_ON_TEAM_AT_OBJECTTYPE",
        spawn_verbs::parse_create_beside_object,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_PLAYERS_NEAREST_UNIT_OF_TYPE_TO_REFERENCE",
        spawn_verbs::parse_set_ref_nearest_of_teams_player,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_ATTACK_NAMED",
        orders::parse_named_attack_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ATTACK_MOVE_TEAM_TO_NAMED_OBJECT",
        orders::parse_attack_move_team_to_named,
        MODE_STANDARD,
    ),
    ActionRow::new("GATE_OPEN", gate_verbs::parse_open, MODE_STANDARD),
    ActionRow::new("GATE_CLOSE", gate_verbs::parse_close, MODE_STANDARD),
    ActionRow::new(
        "PLAYER_FIRE_SPECIAL_POWER_AT_WAYPOINT",
        powers::parse_fire_at_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST",
        power_cost::parse_fire_at_most_cost,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FIRE_SPECIAL_POWER_ON_TEAM",
        powers::parse_fire_on_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FIRE_SPECIAL_POWER_ON_NEAREST_OBJECTTYPE_BY_PLAYER",
        powers::parse_fire_on_nearest_by_player,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "FIRE_SPECIAL_POWER_ON_NEAREST_OBJECTTYPE",
        powers::parse_fire_on_nearest,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "COUNTER_MATH_COUNTER",
        counter_math::parse_counter_math,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_PLAYER_KILLS_OF_TYPE_TO_COUNTER",
        player_counts::parse_kills_of_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_PLAYER_COMMAND_POINTS_USED_TO_COUNTER",
        player_counts::parse_command_points_used,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_PLAYER_COMMAND_POINTS_TOTAL_TO_COUNTER",
        player_counts::parse_command_points_total,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_PLAYER_OWNERSHIP_OF_TYPE_COUNTER",
        player_counts::parse_ownership_counter,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SET_PLAYER_OWNERSHIP_OF_TYPE_COUNTER_INCLUDE_DEAD",
        player_counts::parse_ownership_counter_with_dead,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_CHANGE_OBJECT_STATUS",
        object_status::parse_unit_change_status,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_CHANGE_OBJECT_STATUS",
        object_status::parse_team_change_status,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_AFFECT_OBJECT_PANEL_FLAGS",
        object_status::parse_unit_panel_flags,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_AFFECT_OBJECT_PANEL_FLAGS",
        object_status::parse_team_panel_flags,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_SET_REPULSOR",
        object_status::parse_named_set_repulsor,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_REPULSOR",
        object_status::parse_team_set_repulsor,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_SET_STEALTH_ENABLED",
        object_status::parse_set_stealth_enabled,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GIVE_EXPERIENCE_POINTS",
        experience_verbs::parse_team_give_points,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GIVE_EXPERIENCE_POINTS",
        experience_verbs::parse_unit_give_points,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_SET_EXPERIENCE_POINTS",
        experience_verbs::parse_unit_set_points,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_SET_MAX_LEVEL",
        experience_verbs::parse_unit_set_max_level,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GAIN_LEVEL",
        experience_verbs::parse_unit_gain_level,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GIVE_EXPERIENCE_LEVEL",
        experience_verbs::parse_unit_give_level,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_SET_MAX_SPELLPOINTS",
        experience_verbs::parse_player_max_spellpoints,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_SET_RANKLEVEL",
        experience_verbs::parse_player_set_ranklevel,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_SPELL_STORE",
        tutorial_verbs::parse_enable_spell_store,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_SPELL_STORE",
        tutorial_verbs::parse_disable_spell_store,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_OBJECTIVES_SCREEN",
        tutorial_verbs::parse_enable_objectives,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_OBJECTIVES_SCREEN",
        tutorial_verbs::parse_disable_objectives,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CLOSE_OBJECTIVES_SCREEN",
        tutorial_verbs::parse_close_objectives,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ENABLE_PLANNING_MODE",
        tutorial_verbs::parse_enable_planning,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "DISABLE_PLANNING_MODE",
        tutorial_verbs::parse_disable_planning,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SOUND_DISABLE_TYPE",
        audio::parse_sound_disable_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SOUND_ENABLE_TYPE",
        audio::parse_sound_enable_type,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "SOUND_ENABLE_ALL",
        audio::parse_sound_enable_all,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CREATE_UNIT_REVIVAL_ENTRY_AT_LEVEL",
        experience_verbs::parse_revival_at_level,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_FORCE_EMOTION",
        emotion_verbs::parse_unit_force_emotion,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_FORCE_EMOTION",
        emotion_verbs::parse_team_force_emotion,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "PLAYER_FORCE_EMOTION",
        emotion_verbs::parse_player_force_emotion,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_SET_MODELCONDITION",
        emotion_verbs::parse_unit_set_model_condition,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_MODELCONDITION_FOR_DURATION",
        emotion_verbs::parse_team_model_condition_for_duration,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "KILL_HORDE_MEMBERS",
        emotion_verbs::parse_kill_horde_members,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_SHOCK",
        emotion_verbs::parse_named_shock,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_SET_TOPPLE_DIRECTION",
        topple::parse_set_topple_direction,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_SET_HEALTH",
        team_verbs::parse_team_set_health,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "MOVE_NAMED_UNIT_TO",
        orders::parse_move_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "ATTACK_MOVE_NAMED_UNIT_TO",
        orders::parse_attack_move_named,
        MODE_STANDARD,
    ),
    ActionRow::new("MOVE_TEAM_TO", orders::parse_move_team, MODE_STANDARD),
    ActionRow::new(
        "ATTACK_MOVE_TEAM_TO",
        orders::parse_attack_move_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_FOLLOW_WAYPOINTS",
        orders::parse_follow_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_FOLLOW_WAYPOINTS_EXACT",
        orders::parse_follow_named_exact,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_ATTACK_FOLLOW_WAYPOINTS",
        orders::parse_attack_follow_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_FOLLOW_WAYPOINTS",
        orders::parse_follow_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_FOLLOW_WAYPOINTS_EXACT",
        orders::parse_follow_team_exact,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_ATTACK_MOVE_FOLLOW_WAYPOINTS",
        orders::parse_attack_follow_team,
        MODE_STANDARD,
    ),
    ActionRow::new("NAMED_STOP", orders::parse_stop_named, MODE_STANDARD),
    ActionRow::new("NAMED_STOP_FLUSH", orders::parse_stop_named, MODE_STANDARD),
    ActionRow::new("TEAM_STOP", orders::parse_stop_team, MODE_STANDARD),
    ActionRow::new(
        "TEAM_STOP_AND_DISBAND",
        team_orders::parse_stop_and_disband,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_WANDER_IN_PLACE",
        team_orders::parse_wander_in_place,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_WANDER", team_orders::parse_wander, MODE_STANDARD),
    ActionRow::new("TEAM_PANIC", team_orders::parse_panic, MODE_STANDARD),
    ActionRow::new(
        "TEAM_MOVE_TO_NEAREST_OBJECT_OF_TYPE",
        team_orders::parse_move_to_nearest,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "RECRUIT_TEAM_AT_TEAM",
        team_recruit::parse_recruit_at_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_RECRUIT_UNITS_FROM_TEAM",
        team_recruit::parse_recruit_from_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "CHANGE_BURN_RATE_AT_WAYPOINT",
        burn_rate::parse_change_burn_rate,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_HUNT", orders::parse_hunt_team, MODE_STANDARD),
    ActionRow::new("NAMED_HUNT", orders::parse_hunt_named, MODE_STANDARD),
    ActionRow::new(
        "TEAM_ATTACK_NAMED",
        orders::parse_team_attack_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_ATTACK_TEAM",
        orders::parse_named_attack_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_ATTACK_TEAM",
        orders::parse_team_attack_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_FACE_WAYPOINT",
        face_teleport::parse_named_face_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_FACE_WAYPOINT",
        face_teleport::parse_team_face_waypoint,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_FACE_NAMED",
        face_teleport::parse_named_face_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_FACE_NAMED",
        face_teleport::parse_team_face_named,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_TELEPORT_TO_WAYPOINT",
        face_teleport::parse_named_teleport,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_TELEPORT_TO_WAYPOINT",
        face_teleport::parse_team_teleport,
        MODE_STANDARD,
    ),
    ActionRow::new("TEAM_GUARD", guard_verbs::parse_team_guard, MODE_STANDARD),
    ActionRow::new("NAMED_GUARD", guard_verbs::parse_named_guard, MODE_STANDARD),
    ActionRow::new(
        "TEAM_GUARD_POSITION",
        guard_verbs::parse_team_guard_position,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_AREA",
        guard_verbs::parse_team_guard_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_AREA_FROM_POSITION",
        guard_verbs::parse_team_guard_area_from,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GUARD_AREA",
        guard_verbs::parse_unit_guard_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GUARD_AREA_FROM_POSITION",
        guard_verbs::parse_unit_guard_area_from,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_OBJECT",
        guard_verbs::parse_team_guard_object,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_TEAM",
        guard_verbs::parse_team_guard_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_FOR_SECONDS",
        sequential::parse_team_hold_seconds,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_IDLE_FOR_SECONDS",
        sequential::parse_team_hold_seconds,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GUARD_FOR_FRAMECOUNT",
        sequential::parse_team_hold_frames,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_IDLE_FOR_FRAMECOUNT",
        sequential::parse_team_hold_frames,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_IDLE_FOR_FRAMECOUNT",
        sequential::parse_unit_hold_frames,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_GUARD_FOR_FRAMECOUNT",
        sequential::parse_unit_hold_frames,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_EXECUTE_SEQUENTIAL_SCRIPT",
        sequential::parse_team_execute,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING",
        sequential::parse_team_execute_looping,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_STOP_SEQUENTIAL_SCRIPT",
        sequential::parse_team_stop,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_EXECUTE_SEQUENTIAL_SCRIPT",
        sequential::parse_unit_execute,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING",
        sequential::parse_unit_execute_looping,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_STOP_SEQUENTIAL_SCRIPT",
        sequential::parse_unit_stop,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_STAND_GROUND",
        guard_verbs::parse_team_stand_ground,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "UNIT_STAND_GROUND",
        guard_verbs::parse_unit_stand_ground,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_ATTACK_AREA",
        guard_verbs::parse_team_attack_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_ATTACK_AREA",
        guard_verbs::parse_named_attack_area,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GARRISON_SPECIFIC_BUILDING_INSTANTLY",
        garrison_verbs::parse_team_garrison_instantly,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GARRISON_SPECIFIC_BUILDING",
        garrison_verbs::parse_team_garrison,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_GARRISON_SPECIFIC_BUILDING_INSTANTLY",
        garrison_verbs::parse_named_garrison_instantly,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "NAMED_GARRISON_SPECIFIC_BUILDING",
        garrison_verbs::parse_named_garrison,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "TEAM_GARRISON_TEAM_INSTANTLY",
        garrison_verbs::parse_team_garrison_team,
        MODE_STANDARD,
    ),
    ActionRow::new(
        "EXIT_SPECIFIC_BUILDING",
        garrison_verbs::parse_exit_building,
        MODE_STANDARD,
    ),
];

fn parse_condition(condition: &Condition, ctx: &ParseCtx<'_>) -> Option<Arc<dyn ConditionVerb>> {
    let row = CONDITION_REGISTRY
        .iter()
        .chain(test_condition_rows())
        .find(|row| row.name == condition.name)?;
    let verb = (row.parse)(&status::resolve_status_params(&condition.params), ctx)?;
    Some(if condition.negates() {
        Arc::new(NotCondition(verb))
    } else {
        verb
    })
}

struct NotCondition(Arc<dyn ConditionVerb>);

impl ConditionVerb for NotCondition {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        !self.0.eval(rules, tick, ids, state, player)
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        !self.0.eval_mut(rules, tick, ids, state, player)
    }

    fn finished_audio_name(&self) -> Option<&str> {
        self.0.finished_audio_name()
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        self.0.referenced_unit_name()
    }

    fn watched_area(&self) -> Option<&area::Area> {
        self.0.watched_area()
    }

    fn team_names(&self) -> Vec<String> {
        self.0.team_names()
    }

    fn reads_triggered_powers(&self) -> bool {
        self.0.reads_triggered_powers()
    }

    fn reads_completed_upgrades(&self) -> bool {
        self.0.reads_completed_upgrades()
    }
}

#[cfg(test)]
fn test_condition_rows() -> &'static [ConditionRow] {
    tests::TEST_CONDITIONS
}

#[cfg(not(test))]
fn test_condition_rows() -> &'static [ConditionRow] {
    &[]
}

#[cfg(test)]
fn test_action_rows() -> &'static [ActionRow] {
    tests::TEST_ACTIONS
}

#[cfg(not(test))]
fn test_action_rows() -> &'static [ActionRow] {
    &[]
}

enum ParsedAction {
    Verb(Arc<dyn ActionVerb>),
    OtherMode,
    Unsupported,
}

impl ParsedAction {
    fn or_else(self, other: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Unsupported => other(),
            parsed => parsed,
        }
    }
}

fn parse_action_in(
    registry: &[ActionRow],
    action: &ScriptAction,
    ctx: &ParseCtx<'_>,
) -> ParsedAction {
    let Some(row) = find_row(
        registry,
        &action.name,
        action.action_type,
        |row| row.name,
        |row| row.type_id,
    ) else {
        return ParsedAction::Unsupported;
    };
    if row.mode_mask & ctx.run_mode == 0 {
        return ParsedAction::OtherMode;
    }
    match (row.parse)(&status::resolve_status_params(&action.params), ctx) {
        Some(verb) => ParsedAction::Verb(verb),
        None => ParsedAction::Unsupported,
    }
}

#[derive(Clone)]
struct ScriptDef {
    name: String,
    one_shot: bool,
    initial_active: bool,
    subroutine: bool,
    delay_eval_seconds: i32,
    supported: bool,
    side: String,
    easy: bool,
    normal: bool,
    hard: bool,
    or_conditions: Vec<Vec<Option<Arc<dyn ConditionVerb>>>>,
    actions: Vec<Arc<dyn ActionVerb>>,
    actions_false: Vec<Arc<dyn ActionVerb>>,
    sequence: mint_sage_map::ScriptSequence,
    condition_team: Option<String>,
}

#[derive(Clone)]
struct GroupDef {
    name: String,
    side: String,
    is_subroutine: bool,
    initial_active: bool,
    children: Vec<NodeRef>,
}

#[derive(Debug, Clone, Copy)]
enum NodeRef {
    Script(usize),
    Group(usize),
}

#[derive(Clone, Default)]
pub(crate) struct MapScriptEngine {
    scripts: Vec<ScriptDef>,
    groups: Vec<GroupDef>,
    top: Vec<Vec<NodeRef>>,
    team_protos: team::TeamProtos,
    waypoints: BTreeMap<String, Vec2>,
    path_points: Vec<orders::PathPoint>,
    objectives: Vec<(String, bool)>,
    carryover: Option<Arc<super::carryover::CarryoverStart>>,
    wotr: Option<Arc<super::wotr_battle::WotrStart>>,
    sound_ms: BTreeMap<String, u32>,
    watched_areas: Vec<area::Area>,
    reads_triggered_powers: bool,
    reads_completed_upgrades: bool,
    generic: Vec<Vec<Option<usize>>>,
}

impl std::fmt::Debug for MapScriptEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapScriptEngine")
            .field("scripts", &self.scripts.len())
            .field("groups", &self.groups.len())
            .field("top", &self.top.len())
            .finish()
    }
}

fn build_script_def(
    script: &Script,
    ctx: &ParseCtx<'_>,
    side: &str,
    reported: &mut BTreeSet<String>,
) -> ScriptDef {
    let ctx = &ParseCtx { side, ..*ctx };
    let mut supported = true;
    let mut report = |kind: &str, verb: &str, script_name: &str| {
        diag!("script trace: {side}/{script_name} inert: {kind} {verb}");
        if reported.insert(format!("{kind}:{verb}")) {
            diag!(
                "script: {kind} {verb:?} is not implemented - {script_name:?} and every script \
                 using it stays inert"
            );
        }
    };
    let or_conditions: Vec<Vec<Option<Arc<dyn ConditionVerb>>>> = script
        .or_conditions
        .iter()
        .map(|or_condition| {
            or_condition
                .conditions
                .iter()
                .filter_map(|condition| {
                    if !condition.enabled {
                        return Some(None);
                    }
                    match parse_condition(condition, ctx) {
                        Some(def) => Some(Some(def)),
                        None => {
                            supported = false;
                            report("condition", &condition.name, &script.name);
                            None
                        }
                    }
                })
                .collect()
        })
        .collect();
    let mut parse_actions = |actions: &[ScriptAction]| -> Vec<Arc<dyn ActionVerb>> {
        actions
            .iter()
            .filter(|action| action.enabled)
            .filter_map(|action| {
                match parse_action_in(ACTION_REGISTRY, action, ctx)
                    .or_else(|| parse_action_in(test_action_rows(), action, ctx))
                {
                    ParsedAction::Verb(def) => Some(def),
                    ParsedAction::OtherMode => None,
                    ParsedAction::Unsupported => {
                        supported = false;
                        report("action", &action.name, &script.name);
                        None
                    }
                }
            })
            .collect()
    };
    let actions = parse_actions(&script.actions);
    let actions_false = parse_actions(&script.actions_false);
    ScriptDef {
        name: script.name.clone(),
        one_shot: script.one_shot,
        initial_active: script.active,
        subroutine: script.subroutine,
        delay_eval_seconds: script.delay_evaluation_seconds,
        supported,
        side: side.to_owned(),
        easy: script.easy,
        normal: script.normal,
        hard: script.hard,
        or_conditions,
        actions,
        actions_false,
        sequence: script.sequence.clone(),
        condition_team: None,
    }
}

fn build_nodes(
    nodes: &[ScriptNode],
    ctx: &ParseCtx<'_>,
    side: &str,
    scripts: &mut Vec<ScriptDef>,
    groups: &mut Vec<GroupDef>,
    reported: &mut BTreeSet<String>,
) -> Vec<NodeRef> {
    let mut refs = Vec::with_capacity(nodes.len());
    for node in nodes {
        match node {
            ScriptNode::Script(script) => {
                let index = scripts.len();
                scripts.push(build_script_def(script, ctx, side, reported));
                refs.push(NodeRef::Script(index));
            }
            ScriptNode::Group(group) => {
                let children = build_nodes(&group.children, ctx, side, scripts, groups, reported);
                let index = groups.len();
                groups.push(GroupDef {
                    name: group.name.clone(),
                    side: side.to_owned(),
                    is_subroutine: group.is_subroutine,
                    initial_active: group.active,
                    children,
                });
                refs.push(NodeRef::Group(index));
            }
        }
    }
    refs
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn script_frame_progress(frame: u64, blend: f32) -> f32 {
    let ticks_in = frame.saturating_sub(1) % TICKS_PER_SCRIPT_FRAME;
    (ticks_in as f32 + blend.clamp(0.0, 1.0)) / TICKS_PER_SCRIPT_FRAME as f32
}

fn step_fade(state: &mut MapScriptState) {
    if let Some(fade) = state.fade.as_mut() {
        if !fade.step() {
            state.fade = None;
        }
    }
}

impl MapScriptEngine {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn build(
        trees: &[ScriptTree],
        tree_sides: &[Vec<String>],
        waypoints: &BTreeMap<String, (Vec2, f32)>,
        trigger_areas: &BTreeMap<String, Vec<(f32, f32)>>,
        allow_ring_heroes: bool,
    ) -> Self {
        Self::build_with_cameras(
            trees,
            tree_sides,
            waypoints,
            trigger_areas,
            allow_ring_heroes,
            &BTreeMap::new(),
        )
    }

    #[must_use]
    pub(crate) fn build_with_cameras(
        trees: &[ScriptTree],
        tree_sides: &[Vec<String>],
        waypoints: &BTreeMap<String, (Vec2, f32)>,
        trigger_areas: &BTreeMap<String, Vec<(f32, f32)>>,
        allow_ring_heroes: bool,
        named_cameras: &BTreeMap<String, NamedCamera>,
    ) -> Self {
        let record_seq = std::cell::Cell::new(0);
        let ctx = ParseCtx {
            waypoints,
            trigger_areas,
            allow_ring_heroes,
            named_cameras,
            side: "",
            run_mode: RUN_MODE,
            record_seq: &record_seq,
        };
        let mut scripts = Vec::new();
        let mut groups = Vec::new();
        let mut top = Vec::new();
        let mut reported = BTreeSet::new();
        for (tree_index, tree) in trees.iter().enumerate() {
            let sides = tree_sides.get(tree_index);
            for (slot_index, slot) in tree.player_slots.iter().enumerate() {
                let side = sides
                    .and_then(|s| s.get(slot_index))
                    .cloned()
                    .unwrap_or_default();
                top.push(build_nodes(
                    slot,
                    &ctx,
                    &side,
                    &mut scripts,
                    &mut groups,
                    &mut reported,
                ));
            }
        }
        let mut watched_areas: Vec<area::Area> = Vec::new();
        for script in &scripts {
            for condition in script.or_conditions.iter().flatten().flatten() {
                if let Some(area) = condition.watched_area() {
                    if area.exists() && !watched_areas.iter().any(|seen| seen.name == area.name) {
                        watched_areas.push(area.clone());
                    }
                }
            }
        }
        let reads_triggered_powers = scripts
            .iter()
            .flat_map(|script| script.or_conditions.iter().flatten().flatten())
            .any(|condition| condition.reads_triggered_powers());
        let reads_completed_upgrades = scripts
            .iter()
            .flat_map(|script| script.or_conditions.iter().flatten().flatten())
            .any(|condition| condition.reads_completed_upgrades());
        Self {
            scripts,
            groups,
            top,
            watched_areas,
            reads_triggered_powers,
            reads_completed_upgrades,
            team_protos: team::TeamProtos::default(),
            waypoints: waypoints
                .iter()
                .map(|(name, (position, _))| (name.clone(), *position))
                .collect(),
            path_points: Vec::new(),
            objectives: Vec::new(),
            carryover: None,
            wotr: None,
            sound_ms: BTreeMap::new(),
            generic: Vec::new(),
        }
    }

    #[must_use]
    pub(crate) fn with_team_prototypes(mut self, protos: team::TeamProtos) -> Self {
        for script in &mut self.scripts {
            script.condition_team = team::condition_team_name(
                script
                    .or_conditions
                    .iter()
                    .flatten()
                    .flatten()
                    .flat_map(|condition| condition.team_names()),
                &script.side,
                &protos,
            );
        }
        self.generic = protos
            .iter()
            .map(|(_, proto)| {
                let hooks = &proto.template.generic_hooks;
                if hooks.iter().all(Option::is_none) {
                    return Vec::new();
                }
                hooks
                    .iter()
                    .map(|hook| {
                        hook.as_deref()
                            .and_then(|name| self.find_script(name, &proto.owner))
                    })
                    .collect()
            })
            .collect();
        self.team_protos = protos;
        self
    }

    fn generic_hooks(&self, proto: usize) -> &[Option<usize>] {
        self.generic.get(proto).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub(crate) fn with_path_points(mut self, points: Vec<orders::PathPoint>) -> Self {
        self.path_points = points;
        self
    }

    fn waypoint(&self, name: &str) -> Option<Vec2> {
        self.waypoints.get(name).copied()
    }

    fn path_points(&self) -> &[orders::PathPoint] {
        &self.path_points
    }

    pub(crate) fn team_prototypes(&self) -> &team::TeamProtos {
        &self.team_protos
    }

    #[must_use]
    pub(crate) fn with_objectives(mut self, objectives: Vec<(String, bool)>) -> Self {
        self.objectives = objectives;
        self
    }

    #[must_use]
    pub(crate) fn with_carryover(mut self, start: super::carryover::CarryoverStart) -> Self {
        self.carryover = Some(Arc::new(start));
        self
    }

    #[must_use]
    pub(crate) fn with_wotr(mut self, start: super::wotr_battle::WotrStart) -> Self {
        self.wotr = Some(Arc::new(start));
        self
    }

    pub(crate) fn wotr_start(&self) -> Option<&Arc<super::wotr_battle::WotrStart>> {
        self.wotr.as_ref()
    }

    pub(crate) fn finished_audio_names(&self) -> Vec<String> {
        let names: BTreeSet<String> = self
            .scripts
            .iter()
            .flat_map(|script| script.or_conditions.iter().flatten().flatten())
            .filter_map(|condition| condition.finished_audio_name().map(str::to_owned))
            .collect();
        names.into_iter().collect()
    }

    #[must_use]
    pub(crate) fn with_sound_lengths(mut self, lengths: BTreeMap<String, u32>) -> Self {
        self.sound_ms = lengths;
        self
    }

    pub(crate) fn reads_triggered_powers(&self) -> bool {
        self.reads_triggered_powers
    }

    pub(crate) fn reads_completed_upgrades(&self) -> bool {
        self.reads_completed_upgrades
    }

    fn watched_areas(&self) -> &[area::Area] {
        &self.watched_areas
    }

    pub(crate) fn carryover_start(&self) -> Option<&Arc<super::carryover::CarryoverStart>> {
        self.carryover.as_ref()
    }

    pub(crate) fn script_count(&self) -> usize {
        self.scripts.len()
    }

    #[must_use]
    pub(crate) fn initial_state(&self) -> MapScriptState {
        MapScriptState {
            active: self.scripts.iter().map(|s| s.initial_active).collect(),
            group_active: self.groups.iter().map(|g| g.initial_active).collect(),
            next_eval: vec![None; self.scripts.len()],
            counters: BTreeMap::new(),
            flags: BTreeMap::new(),
            bloom: None,
            camera_move: None,
            fade: None,
            clip_depth_multiplier: None,
            ui_hidden: false,
            terrain_render_disabled: false,
            effects: view_effects::ViewEffects::default(),
            camera_ops: camera_ops::CameraOps::default(),
            cine: cinematic::CineState::with_objectives(&self.objectives),
            audio: audio::ScriptAudio::default(),
            mission_end: None,
            victory_screen_seq: 0,
            carryover: self
                .carryover
                .as_ref()
                .map(|start| super::carryover::CarryoverRun::new(start.carry.clone())),
            wotr: self.wotr.as_ref().map(|start| super::wotr_battle::WotrRun {
                arrived: vec![false; start.seats.iter().map(|s| s.armies.len()).sum()],
                ..super::wotr_battle::WotrRun::default()
            }),
            ever_existed: BTreeSet::new(),
            rt: runtime::ScriptRuntime {
                teams: team::Teams::seeded(&self.team_protos),
                ..runtime::ScriptRuntime::default()
            },
            reveals: reveal::NamedReveals::default(),
            hud: hud::HudState::default(),
            ui: ui_facts::UiFacts::default(),
        }
    }

    fn find_script(&self, name: &str, side: &str) -> Option<usize> {
        let key = SideKey::qualify(name, side);
        self.scripts
            .iter()
            .position(|s| s.name == key.name && s.side == key.side)
    }

    fn find_group(&self, name: &str, side: &str) -> Option<usize> {
        let key = SideKey::qualify(name, side);
        self.groups
            .iter()
            .position(|g| g.name == key.name && g.side == key.side)
    }

    #[must_use]
    pub(crate) fn create_templates(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .scripts
            .iter()
            .flat_map(|script| script.actions.iter().chain(&script.actions_false))
            .filter_map(|action| action.referenced_template().map(str::to_owned))
            .collect();
        names.extend(
            self.team_protos
                .iter()
                .flat_map(|(_, proto)| proto.template.units.iter())
                .map(|slot| slot.template.clone()),
        );
        names.extend(self.carryover.iter().flat_map(|start| {
            start
                .carry
                .available()
                .iter()
                .map(|record| record.template_name.to_string_lossy())
        }));
        names.extend(self.wotr.iter().flat_map(|start| {
            start.seats.iter().flat_map(|seat| {
                seat.armies
                    .iter()
                    .flat_map(|army| army.units.iter().map(|(name, _, _)| name.clone()))
                    .chain(seat.structures.iter().map(|s| s.template.clone()))
            })
        }));
        names.sort();
        names.dedup();
        names
    }

    #[must_use]
    pub(crate) fn referenced_unit_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for script in &self.scripts {
            for and in &script.or_conditions {
                for condition in and.iter().flatten() {
                    if let Some(name) = condition.referenced_unit_name() {
                        names.insert(name.to_owned());
                    }
                }
            }
            for action in script.actions.iter().chain(&script.actions_false) {
                if let Some(name) = action.referenced_unit_name() {
                    names.insert(name.to_owned());
                }
            }
        }
        names
    }
}

fn compare(current: i32, op: i32, value: i32) -> bool {
    match op {
        COMPARE_LESS_THAN => current < value,
        COMPARE_LESS_EQUAL => current <= value,
        COMPARE_EQUAL => current == value,
        COMPARE_GREATER_EQUAL => current >= value,
        COMPARE_GREATER => current > value,
        COMPARE_NOT_EQUAL => current != value,
        _ => false,
    }
}

fn strip_team_prefix(team: &str) -> Option<&str> {
    let prefix = team.get(..4)?;
    if !prefix.eq_ignore_ascii_case("team") {
        return None;
    }
    team.get(4..)
}

impl RotwkRules {
    pub(crate) fn resolve_side_player(&self, side_name: &str) -> Option<PlayerId> {
        if let Some((_, player)) = self
            .map_script_named
            .iter()
            .find(|(side, _)| side.eq_ignore_ascii_case(side_name))
        {
            return Some(*player);
        }
        if let Some(start) = crate::map_players::start_named(side_name) {
            return self
                .map_script_starts
                .iter()
                .find(|(held, _)| *held == start)
                .map(|(_, player)| *player);
        }
        let offset = self
            .map_players
            .iter()
            .position(|name| name.eq_ignore_ascii_case(side_name))?;
        u8::try_from(offset)
            .ok()
            .and_then(|offset| self.players.checked_add(offset))
            .map(PlayerId::new)
    }

    pub(crate) fn seated_side(&self, side_name: &str) -> std::result::Result<PlayerId, String> {
        self.resolve_side_player(side_name).ok_or_else(|| {
            let mut seated: Vec<String> = self
                .map_script_starts
                .iter()
                .map(|(start, _)| format!("Player_{}", u16::from(*start).saturating_add(1)))
                .collect();
            seated.sort();
            seated.extend(self.map_players.iter().cloned());
            format!(
                "{side_name:?} names no player in this match (it has: {})",
                seated.join(", ")
            )
        })
    }

    fn resolve_team_player(&self, team: &str) -> Option<PlayerId> {
        let side_name = strip_team_prefix(team)?;
        self.resolve_side_player(side_name)
    }

    fn named_entity_exists(&self, tick: &Tick<'_>, ids: Ids, name: &str) -> bool {
        tick.state().store(ids.script_name).is_some_and(|store| {
            store
                .iter()
                .any(|(entity, tag)| tag.0 == name && tick.state().is_live(entity))
        })
    }

    fn sound_length_ms(&self, name: &str) -> Option<u32> {
        self.map_scripts
            .as_ref()?
            .sound_ms
            .get(&name.to_ascii_lowercase())
            .copied()
    }

    pub(super) fn named_entity(&self, tick: &Tick<'_>, ids: Ids, name: &str) -> Option<Entity> {
        let store = tick.state().store(ids.script_name)?;
        store
            .iter()
            .find(|(entity, tag)| {
                tag.0 == name
                    && tick.state().is_live(*entity)
                    && !tick.state().contains(ids.doomed, *entity)
            })
            .map(|(entity, _)| entity)
    }

    pub(super) fn named_entity_destroyed(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        name: &str,
    ) -> bool {
        state.ever_existed.contains(name) && self.named_entity(tick, ids, name).is_none()
    }

    fn run_create_named(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        action: &ActionCreateNamed,
    ) -> Result<Option<Entity>> {
        let ActionCreateNamed {
            unit_name,
            template,
            team,
            side,
            at,
        } = action;
        let Some((position, angle)) = *at else {
            diag!(
                "script: CREATE_NAMED_ON_TEAM_AT_WAYPOINT {unit_name:?} names a waypoint this map \
                 does not have - nothing spawned"
            );
            return Ok(None);
        };
        let Some(kind) = self.kinds.kind(template) else {
            diag!(
                "script: CREATE_NAMED_ON_TEAM_AT_WAYPOINT {unit_name:?} names template \
                 {template:?}, which the install has no `Kind` for - nothing spawned"
            );
            return Ok(None);
        };
        let team_id = self.get_team(state, team, side, true);
        let Some(owner) = team_id
            .and_then(|id| self.team_owner(engine, state, id))
            .or_else(|| self.resolve_team_player(team))
        else {
            diag!(
                "script: CREATE_NAMED_ON_TEAM_AT_WAYPOINT {unit_name:?} names team {team:?}, \
                 which has no controlling player - nothing spawned"
            );
            return Ok(None);
        };
        let Some(entity) = self.spawn_kind_placed(
            tick,
            ids,
            kind,
            super::spawning::Placement::Exact(position),
            Facing::from_radians(angle),
            owner,
        )?
        else {
            return Ok(None);
        };
        tick.state_mut().insert(
            ids.script_name,
            entity,
            ScriptObjectName(unit_name.to_owned()),
        )?;
        if let Some(id) = team_id {
            state.rt.teams.add_member(id, entity);
        }
        diag!("script: CREATE_NAMED_ON_TEAM_AT_WAYPOINT created {template:?} named {unit_name:?}");
        Ok(Some(entity))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_subroutine(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        name: &ScriptName,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if depth >= MAX_SUBROUTINE_DEPTH {
            return Ok(());
        }
        if let Some(group_index) = engine.find_group(&name.name, &name.side) {
            let callable = engine
                .groups
                .get(group_index)
                .is_some_and(|g| g.is_subroutine)
                && state
                    .group_active
                    .get(group_index)
                    .copied()
                    .unwrap_or(false);
            if let Some(group) = engine.groups.get(group_index).filter(|_| callable) {
                self.execute_scripts(
                    tick,
                    ids,
                    engine,
                    state,
                    &group.children,
                    Some(player),
                    depth + 1,
                    kills,
                )?;
            }
            return Ok(());
        }
        let script = engine
            .find_script(&name.name, &name.side)
            .filter(|&index| engine.scripts.get(index).is_some_and(|s| s.subroutine));
        if let Some(index) = script {
            self.eval_script(tick, ids, engine, state, index, player, depth + 1, kills)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_scripts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        nodes: &[NodeRef],
        player: Option<Option<PlayerId>>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        for node in nodes {
            let NodeRef::Script(index) = *node else {
                continue;
            };
            let Some(def) = engine.scripts.get(index).filter(|def| !def.subroutine) else {
                continue;
            };
            let current = player.unwrap_or_else(|| self.resolve_side_player(&def.side));
            self.eval_script(tick, ids, engine, state, index, current, depth, kills)?;
        }
        Ok(())
    }

    pub(super) fn debug_script_targets(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        name: &str,
        team: bool,
    ) -> Vec<Entity> {
        let Some(state) = tick
            .state()
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next().map(|(_, state)| state))
        else {
            return Vec::new();
        };
        self.subject_members(tick, ids, state, name, team.then_some(""))
    }

    pub(super) fn debug_hunt(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entities: &[Entity],
    ) -> Result<()> {
        let found = tick.state().store(ids.map_script_state).and_then(|store| {
            store
                .iter()
                .next()
                .map(|(anchor, state)| (anchor, state.clone()))
        });
        let Some((anchor, mut state)) = found else {
            return Ok(());
        };
        for entity in entities {
            state.rt.hunters.insert(*entity);
        }
        tick.state_mut()
            .insert(ids.map_script_state, anchor, state)?;
        Ok(())
    }

    pub(super) fn debug_set_script_value(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        name: &str,
        value: DebugScriptValue,
    ) -> Result<()> {
        let found = tick.state().store(ids.map_script_state).and_then(|store| {
            store
                .iter()
                .next()
                .map(|(anchor, state)| (anchor, state.clone()))
        });
        let Some((anchor, mut state)) = found else {
            return Ok(());
        };
        let key = SideKey::qualify(name, "");
        match value {
            DebugScriptValue::Flag(set) => {
                state.flags.insert(key.clone(), set);
            }
            DebugScriptValue::Counter(count) => {
                state.counters.entry(key.clone()).or_default().value = count;
            }
            DebugScriptValue::Timer(frames) => {
                let counter = state.counters.entry(key.clone()).or_default();
                counter.value = frames;
                counter.countdown = true;
            }
        }
        diag!(
            "script: debug sets {:?}/{:?} to {value:?}",
            key.side,
            key.name
        );
        tick.state_mut()
            .insert(ids.map_script_state, anchor, state)?;
        Ok(())
    }

    fn delay_eval_ready(
        tick: &mut Tick<'_>,
        state: &mut MapScriptState,
        index: usize,
        def: &ScriptDef,
    ) -> bool {
        if def.delay_eval_seconds <= 0 {
            return true;
        }
        let current = tick.frame().get() / TICKS_PER_SCRIPT_FRAME;
        let Some(slot) = state.next_eval.get_mut(index) else {
            return true;
        };
        let next = match *slot {
            Some(next) => next,
            None => {
                let offset = u64::from(tick.rng().below(DELAY_EVAL_FIRST_SPAN));
                *slot = Some(offset);
                offset
            }
        };
        if current < next {
            return false;
        }
        let period = u64::try_from(def.delay_eval_seconds).unwrap_or(0)
            * u64::from(crate::game_data::RETAIL_LOGIC_HZ);
        *slot = Some(current.saturating_add(period));
        true
    }

    fn difficulty_ok(&self, player: Option<PlayerId>, def: &ScriptDef) -> bool {
        let Some(player) = player else {
            return true;
        };
        let tier = self
            .ai_power_difficulty
            .iter()
            .find(|(p, _)| *p == player)
            .map_or(crate::lobby::Difficulty::Medium, |(_, d)| *d);
        match tier {
            crate::lobby::Difficulty::Easy => def.easy,
            crate::lobby::Difficulty::Medium => def.normal,
            crate::lobby::Difficulty::Hard | crate::lobby::Difficulty::Brutal => def.hard,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_script(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        index: usize,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(def) = engine.scripts.get(index).cloned() else {
            return Ok(());
        };
        if !def.supported || !state.active.get(index).copied().unwrap_or(false) {
            return Ok(());
        }
        if !self.difficulty_ok(player, &def) {
            return Ok(());
        }
        if !Self::delay_eval_ready(tick, state, index, &def) {
            return Ok(());
        }
        if def.sequence.run_sequentially || state.rt.seq.is_flagged(index) {
            state.rt.condition_team = None;
            return self.queue_sequential(tick, ids, state, index, &def, player);
        }
        let instances = def
            .condition_team
            .as_deref()
            .map(|name| self.condition_team_instances(engine, state, name, &def.side))
            .filter(|instances| !instances.is_empty());
        let Some(instances) = instances else {
            state.rt.condition_team = None;
            let fired =
                self.run_script_once(tick, ids, engine, state, &def, player, depth, kills)?;
            if def.one_shot && fired.is_some() {
                if let Some(slot) = state.active.get_mut(index) {
                    *slot = false;
                }
            }
            return Ok(());
        };
        for team in instances {
            state.rt.condition_team = Some(team);
            let fired = self.run_script_once(tick, ids, engine, state, &def, player, depth, kills);
            state.rt.condition_team = None;
            if def.one_shot && fired? == Some(true) {
                if let Some(slot) = state.active.get_mut(index) {
                    *slot = false;
                }
            }
        }
        Ok(())
    }

    fn condition_team_instances(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        name: &str,
        side: &str,
    ) -> Vec<TeamId> {
        let key = SideKey::qualify(name, side);
        engine
            .team_protos
            .find(&key.side, &key.name)
            .map(|proto| state.rt.teams.instances_of(proto))
            .unwrap_or_default()
    }

    fn conditions_hold(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        def: &ScriptDef,
        player: Option<PlayerId>,
    ) -> bool {
        def.or_conditions.iter().any(|and| {
            !and.is_empty()
                && and.iter().all(|c| {
                    c.as_ref()
                        .is_none_or(|c| c.eval_mut(self, tick, ids, state, player))
                })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_script_once(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        def: &ScriptDef,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<Option<bool>> {
        let outer = state.rt.objects.swap_running_side(def.side.clone());
        let ran = self.run_script_record(tick, ids, engine, state, def, player, depth, kills);
        state.rt.objects.swap_running_side(outer);
        ran
    }

    #[allow(clippy::too_many_arguments)]
    fn run_script_record(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        def: &ScriptDef,
        player: Option<PlayerId>,
        depth: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<Option<bool>> {
        let condition_true = self.conditions_hold(tick, ids, state, def, player);
        if condition_true {
            diag!(
                "script trace: {}/{} fires at tick {}",
                def.side,
                def.name,
                tick.frame().get()
            );
        }
        let actions = if condition_true {
            &def.actions
        } else if def.actions_false.is_empty() {
            return Ok(None);
        } else {
            &def.actions_false
        };
        for action in actions.clone() {
            action.run(self, tick, ids, engine, state, player, depth, kills)?;
        }
        Ok(Some(condition_true))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_script(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        name: &str,
        side: &str,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let player = state
            .rt
            .calling_team
            .and_then(|team| self.team_owner(engine, state, team));
        let name = ScriptName {
            name: name.to_owned(),
            side: side.to_owned(),
        };
        self.run_subroutine(tick, ids, engine, state, &name, player, 0, kills)
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_group(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        index: usize,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(group) = engine.groups.get(index).cloned() else {
            return Ok(());
        };
        if group.is_subroutine || !state.group_active.get(index).copied().unwrap_or(false) {
            return Ok(());
        }
        self.execute_scripts(tick, ids, engine, state, &group.children, None, 0, kills)?;
        for child in &group.children {
            if let NodeRef::Group(group_index) = *child {
                self.eval_group(tick, ids, engine, state, group_index, kills)?;
            }
        }
        Ok(())
    }

    pub(super) fn step_map_scripts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        anchor: Option<Entity>,
    ) -> Result<Vec<(Entity, f32)>> {
        let (Some(engine), Some(anchor)) = (self.map_scripts.as_ref(), anchor) else {
            return Ok(Vec::new());
        };
        if tick.frame().get() % TICKS_PER_SCRIPT_FRAME != 0 {
            return Ok(Vec::new());
        }
        let Some(mut state) = tick.state().get(ids.map_script_state, anchor).cloned() else {
            return Ok(Vec::new());
        };
        step_fade(&mut state);
        let frame = tick.frame().get();
        for (key, value) in &mut state.counters {
            if value.countdown && value.value >= 0 {
                value.value -= 1;
                if value.value == 0 {
                    diag!(
                        "script: timer {:?}/{:?} expired at tick {frame}",
                        key.side,
                        key.name
                    );
                }
            }
        }
        self.update_area_watch(tick, ids, engine, &mut state);
        self.adopt_into_default_teams(tick, ids, engine, &mut state);
        let mut kills = Vec::new();
        for slot in &engine.top {
            self.execute_scripts(tick, ids, engine, &mut state, slot, None, 0, &mut kills)?;
            for node in slot {
                if let NodeRef::Group(index) = *node {
                    self.eval_group(tick, ids, engine, &mut state, index, &mut kills)?;
                }
            }
        }
        self.update_team_states(tick, ids, engine, &mut state, &mut kills)?;
        self.step_sequential(tick, ids, engine, &mut state, &mut kills)?;
        self.step_hunters(tick, ids, engine, &mut state)?;
        self.step_attack_areas(tick, ids, &mut state)?;
        self.step_walks(tick, ids, &mut state);
        self.step_team_walks(tick, ids, &mut state)?;
        self.step_wanderers(tick, ids, &mut state)?;
        self.step_path_wanderers(tick, ids, engine, &mut state)?;
        self.step_facings(tick, ids, &mut state)?;
        self.step_generic_scripts(tick, ids, engine, &mut state, &mut kills)?;
        self.sync_count_stamp(tick.state(), ids, &mut state);
        tick.state_mut()
            .insert(ids.map_script_state, anchor, state)?;
        Ok(kills)
    }
}

#[cfg(test)]
mod tests;
