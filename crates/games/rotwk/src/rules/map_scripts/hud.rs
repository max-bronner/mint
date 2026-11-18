use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::cinematic::{mean_position, simple, Parsed};
use super::counters::SideKey;
use super::{
    parameter_int, parameter_string, ActionVerb, ConditionVerb, CounterValue, MapScriptEngine,
    MapScriptState, ParseCtx,
};
use crate::rules::{Ids, RotwkRules};

const PENDING_CAP: usize = 16;

const CAMEO_FLASHES_PER_SECOND: i32 = 3;

const OBJECT_FLASH_NUMERATOR: i32 = 5;
const OBJECT_FLASH_DENOMINATOR: i32 = 15;

const BUTTON_FLASH_FRAMES_PER_SECOND: i32 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiFlashTarget {
    Cameo(String),
    HeroSelect(String),
    SelectBuilder,
    SpellStore,
    Objectives,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiFlash {
    pub(crate) seq: u32,
    pub(crate) target: UiFlashTarget,
    pub(crate) amount: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectFlash {
    pub(crate) seq: u32,
    pub(crate) entity: Entity,
    pub(crate) white: bool,
    pub(crate) count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MovieCmd {
    pub(crate) seq: u32,
    pub(crate) name: String,
    pub(crate) skippable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NamedTimer {
    key: SideKey,
    label: String,
    countdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TimerView {
    pub(crate) label: String,
    pub(crate) countdown: bool,
    pub(crate) value: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HudState {
    ui_flashes: Vec<UiFlash>,
    object_flashes: Vec<ObjectFlash>,
    flash_serial: u32,
    house_color_off: bool,
    timers: Vec<NamedTimer>,
    timers_hidden: bool,
    removed_buttons: Vec<(String, String)>,
    added_buttons: Vec<AddedButton>,
    movie: Option<MovieCmd>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddedButton {
    pub(crate) set: String,
    pub(crate) slot: u32,
    pub(crate) button: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct HudView {
    pub(crate) ui_flashes: Vec<UiFlash>,
    pub(crate) object_flashes: Vec<ObjectFlash>,
    pub(crate) house_color_off: bool,
    pub(crate) timers: Vec<TimerView>,
    pub(crate) removed_buttons: Vec<(String, String)>,
    pub(crate) added_buttons: Vec<AddedButton>,
    pub(crate) movie: Option<MovieCmd>,
}

impl HudState {
    fn next_serial(&mut self) -> u32 {
        self.flash_serial = self.flash_serial.wrapping_add(1);
        self.flash_serial
    }

    fn flash_ui(&mut self, target: UiFlashTarget, amount: i32) {
        let seq = self.next_serial();
        if self.ui_flashes.len() >= PENDING_CAP {
            self.ui_flashes.remove(0);
        }
        self.ui_flashes.push(UiFlash {
            seq,
            target,
            amount,
        });
    }

    fn flash_object(&mut self, entity: Entity, white: bool, count: i32) {
        let seq = self.next_serial();
        if self.object_flashes.len() >= PENDING_CAP {
            self.object_flashes.remove(0);
        }
        self.object_flashes.push(ObjectFlash {
            seq,
            entity,
            white,
            count,
        });
    }

    fn show_timer(&mut self, timer: NamedTimer) {
        self.timers.retain(|existing| existing.key != timer.key);
        self.timers.push(timer);
    }

    fn hide_timer(&mut self, key: &SideKey) {
        self.timers.retain(|existing| &existing.key != key);
    }

    pub(crate) fn remove_button(&mut self, set: &str, button: &str) {
        let entry = (set.to_owned(), button.to_owned());
        if !self.removed_buttons.contains(&entry) {
            self.removed_buttons.push(entry);
        }
    }

    fn add_button(&mut self, entry: AddedButton) {
        self.added_buttons
            .retain(|held| !(held.set == entry.set && held.slot == entry.slot));
        self.added_buttons.push(entry);
    }

    pub(crate) fn view(&self, counters: &BTreeMap<SideKey, CounterValue>) -> HudView {
        HudView {
            ui_flashes: self.ui_flashes.clone(),
            object_flashes: self.object_flashes.clone(),
            house_color_off: self.house_color_off,
            timers: if self.timers_hidden {
                Vec::new()
            } else {
                self.timers
                    .iter()
                    .map(|timer| TimerView {
                        label: timer.label.clone(),
                        countdown: timer.countdown,
                        value: counters.get(&timer.key).map_or(0, |c| c.value),
                    })
                    .collect()
            },
            removed_buttons: self.removed_buttons.clone(),
            added_buttons: self.added_buttons.clone(),
            movie: self.movie.clone(),
        }
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if *self == Self::default() {
            return;
        }
        hasher.write_u8(1);
        hasher.write_u32(self.flash_serial);
        hasher.write_bool(self.house_color_off);
        hasher.write_bool(self.timers_hidden);
        hasher.write_usize(self.timers.len());
        for timer in &self.timers {
            hasher.write_bytes(timer.key.side.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(timer.key.name.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(timer.label.as_bytes());
            hasher.write_bool(timer.countdown);
        }
        hasher.write_usize(self.removed_buttons.len());
        for (set, button) in &self.removed_buttons {
            hasher.write_bytes(set.as_bytes());
            hasher.write_u8(0);
            hasher.write_bytes(button.as_bytes());
            hasher.write_u8(0);
        }
        hasher.write_usize(self.added_buttons.len());
        for added in &self.added_buttons {
            hasher.write_bytes(added.set.as_bytes());
            hasher.write_u8(0);
            hasher.write_u32(added.slot);
            hasher.write_bytes(added.button.as_bytes());
            hasher.write_u8(0);
        }
        hasher.write_u32(self.movie.as_ref().map_or(0, |movie| movie.seq));
    }
}

fn parse_flash_ui(
    params: &[Parameter],
    target: impl Fn(&[Parameter]) -> Option<UiFlashTarget> + Send + Sync + 'static,
    amount: impl Fn(i32) -> Option<i32> + Send + Sync + 'static,
    seconds_index: usize,
) -> Parsed {
    let target = target(params)?;
    let seconds = parameter_int(params, seconds_index)?;
    let amount = amount(seconds)?;
    simple(move |_, _, _, state| state.hud.flash_ui(target.clone(), amount))
}

pub(super) fn parse_cameo_flash(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    parse_flash_ui(
        params,
        |p| parameter_string(p, 0).map(UiFlashTarget::Cameo),
        |seconds| {
            let flashes = seconds.saturating_mul(CAMEO_FLASHES_PER_SECOND);
            Some(if flashes % 2 == 1 {
                flashes.saturating_add(1)
            } else {
                flashes
            })
        },
        1,
    )
}

pub(super) fn parse_hero_select_button_flash(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    parse_flash_ui(
        params,
        |p| parameter_string(p, 0).map(UiFlashTarget::HeroSelect),
        |seconds| Some(seconds.saturating_mul(BUTTON_FLASH_FRAMES_PER_SECOND)),
        1,
    )
}

pub(super) fn parse_select_builder_button_flash(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    parse_flash_ui(
        params,
        |_| Some(UiFlashTarget::SelectBuilder),
        |seconds| Some(seconds.saturating_mul(BUTTON_FLASH_FRAMES_PER_SECOND)),
        0,
    )
}

pub(super) fn parse_flash_spell_store_button(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    parse_flash_ui(
        params,
        |_| Some(UiFlashTarget::SpellStore),
        |seconds| (seconds >= 0).then_some(seconds),
        0,
    )
}

pub(super) fn parse_flash_objectives_button(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    parse_flash_ui(
        params,
        |_| Some(UiFlashTarget::Objectives),
        |seconds| (seconds >= 0).then_some(seconds),
        0,
    )
}

fn object_flash_count(seconds: i32) -> i32 {
    seconds.saturating_mul(OBJECT_FLASH_NUMERATOR) / OBJECT_FLASH_DENOMINATOR
}

struct ActionNamedFlash {
    unit: String,
    side: String,
    seconds: i32,
    white: bool,
}
impl ActionVerb for ActionNamedFlash {
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
        if self.seconds <= 0 {
            return Ok(());
        }
        if let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) {
            state
                .hud
                .flash_object(entity, self.white, object_flash_count(self.seconds));
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

fn named_flash(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    white: bool,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNamedFlash {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        seconds: parameter_int(params, 1)?,
        white,
    }))
}

pub(super) fn parse_named_flash(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    named_flash(params, ctx, false)
}

pub(super) fn parse_named_flash_white(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    named_flash(params, ctx, true)
}

struct ActionTeamFlash {
    team: String,
    side: String,
    seconds: i32,
    white: bool,
}
impl ActionVerb for ActionTeamFlash {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return Ok(());
        };
        let count = object_flash_count(self.seconds);
        for member in rules.live_members(tick, state, team) {
            state.hud.flash_object(member, self.white, count);
        }
        Ok(())
    }
}

fn team_flash(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    white: bool,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamFlash {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        seconds: parameter_int(params, 1)?,
        white,
    }))
}

pub(super) fn parse_team_flash(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_flash(params, ctx, false)
}

pub(super) fn parse_team_flash_white(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    team_flash(params, ctx, true)
}

struct ActionTeamRadarEvent {
    team: String,
    side: String,
    kind: i32,
}
impl ActionVerb for ActionTeamRadarEvent {
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
        let Some(team) = rules.peek_team(state, &self.team, &self.side) else {
            return Ok(());
        };
        let positions: Vec<_> = rules
            .live_members(tick, state, team)
            .into_iter()
            .filter_map(|member| tick.state().get(ids.body, member))
            .map(|body| body.0.position)
            .collect();
        if let Some(at) = mean_position(&positions) {
            state.cine.push_radar(at, self.kind);
        }
        Ok(())
    }
}

pub(super) fn parse_team_radar_event(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionTeamRadarEvent {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        kind: parameter_int(params, 1)?,
    }))
}

pub(super) fn parse_enable_house_color(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let enabled = parameter_int(params, 0)? != 0;
    simple(move |_, _, _, state| state.hud.house_color_off = !enabled)
}

fn parse_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    countdown: bool,
) -> Option<Arc<dyn ActionVerb>> {
    let timer = NamedTimer {
        key: SideKey::qualify(&parameter_string(params, 0)?, ctx.side),
        label: parameter_string(params, 1)?,
        countdown,
    };
    super::cinematic::simple(move |_, _, _, state| state.hud.show_timer(timer.clone()))
}

pub(super) fn parse_display_counter(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_timer(params, ctx, false)
}

pub(super) fn parse_display_countdown_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    parse_timer(params, ctx, true)
}

pub(super) fn parse_hide_timer(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let key = SideKey::qualify(&parameter_string(params, 0)?, ctx.side);
    super::cinematic::simple(move |_, _, _, state| state.hud.hide_timer(&key))
}

pub(super) fn parse_enable_timer_display(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| state.hud.timers_hidden = false)
}

pub(super) fn parse_disable_timer_display(_: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    simple(|_, _, _, state| state.hud.timers_hidden = true)
}

struct ActionRemoveButton {
    button: String,
    object_type: String,
}
impl ActionVerb for ActionRemoveButton {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        if let Some(set) = rules.command_set_of_type(&self.object_type) {
            state.hud.remove_button(&set, &self.button);
        }
        Ok(())
    }
}

struct ActionAddButtonSlot {
    button: String,
    object_type: String,
    slot: i32,
}
impl ActionVerb for ActionAddButtonSlot {
    fn run(
        &self,
        rules: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let slot = u32::try_from(self.slot)
            .ok()
            .filter(|slot| (1..=33).contains(slot));
        if let (Some(slot), Some(set)) = (slot, rules.command_set_of_type(&self.object_type)) {
            state.hud.add_button(AddedButton {
                set,
                slot,
                button: self.button.clone(),
            });
        }
        Ok(())
    }
}

pub(super) fn parse_add_button_slot(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionAddButtonSlot {
        button: parameter_string(params, 0)?,
        object_type: parameter_string(params, 1)?,
        slot: parameter_int(params, 2)?,
    }))
}

pub(super) fn parse_remove_button(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRemoveButton {
        button: parameter_string(params, 0)?,
        object_type: parameter_string(params, 1)?,
    }))
}

pub(super) fn parse_play_movie(params: &[Parameter], _: &ParseCtx<'_>) -> Parsed {
    let name = parameter_string(params, 0)?;
    let skippable = parameter_int(params, 1)? != 0;
    simple(move |_, _, _, state| {
        let seq = state
            .hud
            .movie
            .as_ref()
            .map_or(1, |m| m.seq.wrapping_add(1));
        state.hud.movie = Some(MovieCmd {
            seq,
            name: name.clone(),
            skippable,
        });
    })
}

impl RotwkRules {
    pub(super) fn command_set_of_type(&self, object_type: &str) -> Option<String> {
        let kind = self.kinds.kind(object_type)?;
        self.kinds.get(kind)?.record.command_set.clone()
    }
}

struct ActionDeselect;
impl ActionVerb for ActionDeselect {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        match rules.script_local_player {
            Some(local) => rules.clear_selection(tick, ids, local),
            None => Ok(()),
        }
    }
}

pub(super) fn parse_deselect(_: &[Parameter], _: &ParseCtx<'_>) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionDeselect))
}

struct ActionSelectObject {
    player_text: String,
    unit: String,
    side: String,
}
impl ActionVerb for ActionSelectObject {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, &self.side) else {
            return Ok(());
        };
        for target in rules
            .player_mask(&state.rt, &self.player_text, player)
            .players()
        {
            rules.set_selection(tick, ids, target, true, &[entity])?;
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

pub(super) fn parse_select_object(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSelectObject {
        player_text: parameter_string(params, 0)?,
        unit: parameter_string(params, 1)?,
        side: ctx.side.to_owned(),
    }))
}

enum SelectedTest {
    Named { unit: String, side: String },
    Type(String),
}

struct ConditionSelected(SelectedTest);
impl ConditionVerb for ConditionSelected {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let Some(local) = rules.script_local_player else {
            return false;
        };
        let mut selected = rules.selection(tick, ids, local);
        let hordes: Vec<Entity> = selected
            .iter()
            .filter_map(|&entity| tick.state().get(ids.member, entity).map(|m| m.horde))
            .collect();
        selected.extend(hordes);
        match &self.0 {
            SelectedTest::Named { unit, side } => rules
                .object_named(tick, ids, &state.rt, unit, side)
                .is_some_and(|entity| selected.contains(&entity)),
            SelectedTest::Type(types) => {
                let names = state.rt.lists.types_of(types);
                let kinds: Vec<_> = names.iter().filter_map(|n| rules.kinds.kind(n)).collect();
                selected.iter().any(|&entity| {
                    tick.state()
                        .get(ids.kind, entity)
                        .is_some_and(|kind| kinds.contains(kind))
                })
            }
        }
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.0 {
            SelectedTest::Named { unit, .. } => Some(unit),
            SelectedTest::Type(_) => None,
        }
    }
}

pub(super) fn parse_named_selected(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionSelected(SelectedTest::Named {
        unit: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
    })))
}

pub(super) fn parse_type_selected(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionSelected(SelectedTest::Type(
        parameter_string(params, 0)?,
    ))))
}
