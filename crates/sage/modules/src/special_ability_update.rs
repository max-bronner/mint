use std::sync::Arc;

use mint_sage_combat::{surface_distance_squared_2d, within_attack_range, Point3};
use mint_sim::module::{
    Capability, CapabilityMask, CreateContext, ModuleKind, Sleep, UpdateContext,
};
use mint_sim::{
    ActivateContext, Activation, Component, Entity, FieldError, Frame, LogicFrames,
    ModuleFieldSource, StateHasher,
};

use crate::effects::{CastEdge, Effect, EffectLog, Name, Names};
use crate::world_reads::{
    Hordes, Immobile, ModelConditions, Movers, PlayerOrdered, Positions, StillMoving, Structures,
    ViewBlocked,
};

const IS_USING_ABILITY: &str = "IS_USING_ABILITY";

const NO_AUTO_ACQUIRE: &str = "NO_AUTO_ACQUIRE";

const APPROACH_GIVE_UP_COUNT: u32 = 2 * RETAIL_LOGIC_HZ;

const APPROACH_GIVE_UP_DISTANCE: f32 = 5.0;

const FIXED_POINT_RANGE: f32 = 50.0;

fn report_status_bit(
    effects: &mut mint_sim::module::Effects<'_>,
    entity: Entity,
    bit: &str,
    on: bool,
) {
    let Some(log) = effects.get::<EffectLog>() else {
        return;
    };
    let set: Names = Names::from(vec![Name::from(bit)]);
    let empty: Names = Names::from(Vec::new());
    log.push(
        entity,
        Effect::StatusBits {
            set: if on { set.clone() } else { empty.clone() },
            clear: if on { empty } else { set },
        },
    );
}

fn report_using_ability(ctx: &mut UpdateContext<'_>, entity: Entity, using: bool) {
    report_status_bit(&mut ctx.effects, entity, IS_USING_ABILITY, using);
}

const UNPACKING: &str = "UNPACKING";
const PREPARING: &str = "PREPARING";
const PACKING: &str = "PACKING";

fn push_conditions(ctx: &mut UpdateContext<'_>, entity: Entity, add: Vec<Name>, remove: Vec<Name>) {
    if add.is_empty() && remove.is_empty() {
        return;
    }
    let Some(log) = ctx.effects.get::<EffectLog>() else {
        return;
    };
    log.push(
        entity,
        Effect::ModelConditions {
            add: Names::from(add),
            remove: Names::from(remove),
        },
    );
}

fn push_lose_stealth(ctx: &mut UpdateContext<'_>, entity: Entity) {
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, Effect::LoseStealth);
    }
}

fn end_ability<T: Trigger>(data: &AbilityData<T>, entity: Entity, ctx: &mut UpdateContext<'_>) {
    report_using_ability(ctx, entity, false);
    report_status_bit(&mut ctx.effects, entity, NO_AUTO_ACQUIRE, false);
    let mut remove = vec![
        Name::from(UNPACKING),
        Name::from(PREPARING),
        Name::from(PACKING),
    ];
    if let Some(packing_type) = &data.packing_type {
        remove.push(Name::clone(packing_type));
    }
    if let Some(anim) = &data.custom_anim {
        if anim.lasts.is_none() {
            remove.push(Name::clone(&anim.state));
        }
    }
    push_conditions(ctx, entity, Vec::new(), remove);
    stop_active_loop_sound(data, entity, ctx);
    if data.kill_modifier_on_exit {
        clear_trigger_modifier(data, entity, ctx);
    }
    data.trigger.finish(entity, ctx);
}

fn clear_trigger_modifier<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    let Some(modifier) = &data.trigger_modifier else {
        return;
    };
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(
            entity,
            Effect::ClearSelfTimedModifier(Name::clone(modifier)),
        );
    }
}

fn finish_ability<T: Trigger>(
    data: &AbilityData<T>,
    active_slot: &mut Option<Active>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
    hold: LogicFrames,
    completed: bool,
) -> Sleep {
    if completed {
        if let Some(button) = &data.chained_button {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(entity, Effect::PressCommand(Name::clone(button)));
            }
        }
    }
    let hold = active_slot
        .as_ref()
        .map_or(hold, |active| hold.max(active.frozen));
    if hold.is_zero() {
        *active_slot = None;
        end_ability(data, entity, ctx);
        return Sleep::Forever;
    }
    if let Some(active) = active_slot {
        active.phase = Phase::Paralyzed;
        active.remaining = hold;
    }
    Sleep::NextFrame
}

fn conditions_reject<T: Trigger>(
    data: &AbilityData<T>,
    reads: mint_sim::module::Reads<'_>,
    entity: Entity,
) -> bool {
    if data.required_conditions.is_empty() && data.rejected_conditions.is_empty() {
        return false;
    }
    let Some(conditions) = reads.get::<ModelConditions>() else {
        return false;
    };
    data.required_conditions
        .iter()
        .any(|flag| !conditions.holds(entity, flag))
        || data
            .rejected_conditions
            .iter()
            .any(|flag| conditions.holds(entity, flag))
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn condition_flags(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Vec<Name>, FieldError> {
    fields
        .field_args(keyword)
        .into_iter()
        .map(|word| {
            let mapped = if same(word, "WEAPON_TOGGLE") {
                "WEAPONSET_TOGGLE_1"
            } else if same(word, "MOUNTED") || same(word, "MOVING") {
                return Ok(Name::from(word));
            } else {
                return Err(FieldError::new(format!(
                    "{keyword} does not name MOUNTED, WEAPON_TOGGLE or MOVING: {word}"
                )));
            };
            Ok(Name::from(mapped))
        })
        .collect()
}

fn resolve_mover(reads: mint_sim::module::Reads<'_>, entity: Entity) -> Entity {
    reads
        .get::<Movers>()
        .map_or(entity, |movers| movers.resolve(entity))
}

pub const RETAIL_LOGIC_HZ: u32 = 5;

const TICKS_PER_RETAIL_FRAME: u32 = mint_sim::SIMULATION_HZ / RETAIL_LOGIC_HZ;

#[must_use]
pub fn retail_duration_ticks(milliseconds: u32) -> LogicFrames {
    let ms_per_retail_frame = 1000 / RETAIL_LOGIC_HZ;
    let retail_frames = milliseconds.div_ceil(ms_per_retail_frame);
    LogicFrames::from_frames(retail_frames.saturating_mul(TICKS_PER_RETAIL_FRAME))
}

pub(crate) fn duration(fields: &dyn ModuleFieldSource, keyword: &str) -> (bool, LogicFrames) {
    match fields.field_numeric(keyword) {
        Some(ms) if ms.is_finite() && ms > 0.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            (true, retail_duration_ticks(whole))
        }
        _ => (false, LogicFrames::ZERO),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbilityTarget {
    Object(Entity),
    Position(f32, f32),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Approaching,
    Facing,
    Unpacking,
    Preparing,
    Packing,
    Paralyzed,
}

const MIN_FACING_TOLERANCE_RADIANS: f32 = 0.034_906_59;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Active {
    target: AbilityTarget,
    phase: Phase,
    remaining: LogicFrames,
    prepare_reported: bool,
    triggers: u32,
    frozen: LogicFrames,
    busy: LogicFrames,
    last_known_target: Option<(f32, f32)>,
    holding: bool,
    last_period: bool,
    approaches: u32,
    last_approach_aim: Option<(f32, f32)>,
    pending_custom_anim: Option<Frame>,
    capture_phase: f32,
}

fn ignores_orders<T: Trigger>(data: &AbilityData<T>, active: &Active) -> bool {
    data.ignore_ai_command_always || !active.busy.is_zero()
}

fn movement_held<T: Trigger>(data: &AbilityData<T>, active: &Active) -> bool {
    ignores_orders(data, active) || !active.frozen.is_zero()
}

pub trait Trigger: 'static + Clone + std::fmt::Debug + PartialEq {
    fn begin(&self, _entity: Entity, _ctx: &mut UpdateContext<'_>) {}

    fn prepare(&self, _entity: Entity, _target: AbilityTarget, _ctx: &mut UpdateContext<'_>) {}

    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>);

    fn fires_after_first_trigger(&self) -> bool {
        true
    }

    fn continue_preparation(&self, _triggers: u32) -> bool {
        true
    }

    fn abort(&self, _entity: Entity, _target: AbilityTarget, _ctx: &mut UpdateContext<'_>) {}

    fn finish(&self, _entity: Entity, _ctx: &mut UpdateContext<'_>) {}

    fn effect_timer(&self) -> Option<LogicFrames> {
        None
    }

    fn effect_expired(&self, _entity: Entity, _ctx: &mut UpdateContext<'_>) {}

    fn on_create(&self, _entity: Entity, _ctx: &mut CreateContext<'_>) {}

    fn facing_tolerance(&self) -> f32 {
        MIN_FACING_TOLERANCE_RADIANS
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerTrigger {
    power: Name,
}

impl PowerTrigger {
    fn report(
        &self,
        entity: Entity,
        target: AbilityTarget,
        edge: CastEdge,
        ctx: &mut UpdateContext<'_>,
    ) {
        let target = match target {
            AbilityTarget::Object(target) => Some(target),
            AbilityTarget::Position(..) | AbilityTarget::None => None,
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::CastEdge {
                    power: Name::clone(&self.power),
                    target,
                    edge,
                },
            );
        }
    }
}

impl Trigger for PowerTrigger {
    fn prepare(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        self.report(entity, target, CastEdge::Prepare, ctx);
    }

    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        self.report(entity, target, CastEdge::Trigger, ctx);
    }

    fn abort(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        self.report(entity, target, CastEdge::Abort, ctx);
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct AbilityData<T: Trigger> {
    power: Name,
    start_range: f32,
    unpack_needed: bool,
    unpack_frames: LogicFrames,
    pack_needed: bool,
    pack_frames: LogicFrames,
    prep_frames: LogicFrames,
    persistent_frames: Option<LogicFrames>,
    persistent_count: Option<u32>,
    silent_final_trigger: bool,
    halts_on_arrival: bool,
    needs_facing: bool,
    packing_type: Option<Name>,
    paralyze_completed: LogicFrames,
    paralyze_aborted: LogicFrames,
    survives_target_death: bool,
    abort_range: Option<f32>,
    requires_los: bool,
    ignore_ai_command_always: bool,
    freeze_frames: LogicFrames,
    busy_frames: LogicFrames,
    pub(crate) stops_before_activating: bool,
    instant: bool,
    chained_button: Option<Name>,
    trigger_sound: Option<Name>,
    active_loop: Option<Name>,
    do_capture_fx: bool,
    trigger_experience: u32,
    trigger_skill_points: u32,
    trigger_modifier: Option<Name>,
    trigger_modifier_duration: LogicFrames,
    kill_modifier_on_exit: bool,
    kill_modifier_on_rejected: bool,
    required_conditions: Vec<Name>,
    rejected_conditions: Vec<Name>,
    approach_until_members_in_range: bool,
    custom_anim: Option<CustomAnim>,
    trigger_condition: Option<(Name, LogicFrames)>,
    lose_stealth_on_trigger: bool,
    pre_trigger_unstealth_frames: LogicFrames,
    pub(crate) trigger: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbilityUpdate<T: Trigger> {
    pub(crate) data: Arc<AbilityData<T>>,
    pub(crate) active: Option<Active>,
    pub(crate) effect_until: Option<Frame>,
}

impl<T: Trigger> AbilityUpdate<T> {
    pub fn end_persistence(&mut self) {
        if let Some(active) = &mut self.active {
            active.last_period = true;
        }
    }

    pub(crate) fn trigger_nearness(&self) -> f32 {
        let Some(active) = &self.active else {
            return 0.0;
        };
        let fraction = |total: LogicFrames| {
            #[allow(clippy::cast_precision_loss)]
            let (left, total) = (active.remaining.get() as f32, total.get().max(1) as f32);
            (left / total).clamp(0.0, 1.0)
        };
        match active.phase {
            Phase::Unpacking => 1.0 - fraction(self.data.unpack_frames),
            Phase::Preparing => 1.0,
            Phase::Packing => fraction(self.data.pack_frames),
            Phase::Approaching | Phase::Facing | Phase::Paralyzed => 0.0,
        }
    }
}

pub type SpecialAbilityUpdate = AbilityUpdate<PowerTrigger>;

impl<T: Trigger> Component for AbilityUpdate<T> {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.active {
            None => hasher.write_u8(0),
            Some(active) => {
                hasher.write_u8(1);
                match active.target {
                    AbilityTarget::Object(entity) => {
                        hasher.write_u8(0);
                        entity.hash_into(hasher);
                    }
                    AbilityTarget::Position(x, y) => {
                        hasher.write_u8(1);
                        hasher.write_f32(x);
                        hasher.write_f32(y);
                    }
                    AbilityTarget::None => hasher.write_u8(2),
                }
                hasher.write_u8(active.phase as u8);
                hasher.write_u32(active.remaining.get());
                hasher.write_bool(active.prepare_reported);
                hasher.write_u32(active.triggers);
                hasher.write_u32(active.frozen.get());
                hasher.write_u32(active.busy.get());
                match active.last_known_target {
                    Some((x, y)) => {
                        hasher.write_bool(true);
                        hasher.write_f32(x);
                        hasher.write_f32(y);
                    }
                    None => hasher.write_bool(false),
                }
                hasher.write_bool(active.holding);
                hasher.write_bool(active.last_period);
                hasher.write_u32(active.approaches);
                match active.last_approach_aim {
                    Some((x, y)) => {
                        hasher.write_bool(true);
                        hasher.write_f32(x);
                        hasher.write_f32(y);
                    }
                    None => hasher.write_bool(false),
                }
                hasher.write_bool(active.pending_custom_anim.is_some());
                hasher.write_u64(active.pending_custom_anim.map_or(0, Frame::get));
            }
        }
        hasher.write_bool(self.effect_until.is_some());
        hasher.write_u64(self.effect_until.map_or(0, Frame::get));
    }
}

impl<T: Trigger> ModuleKind for AbilityUpdate<T> {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower)
        .union(CapabilityMask::single(Capability::Update))
        .union(CapabilityMask::single(Capability::Create));

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.data.power, power)
    }

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        self.data.trigger.on_create(ctx.entity, ctx);
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        if self.active.is_some() {
            return;
        }
        let target = match (activation.target, activation.at) {
            (Some(entity), _) => AbilityTarget::Object(entity),
            (None, Some((x, y))) => AbilityTarget::Position(x, y),
            (None, None) => AbilityTarget::None,
        };
        self.active = Some(Active {
            target,
            phase: Phase::Approaching,
            remaining: LogicFrames::ZERO,
            prepare_reported: false,
            triggers: 0,
            frozen: LogicFrames::ZERO,
            busy: LogicFrames::ZERO,
            last_known_target: None,
            holding: false,
            last_period: false,
            approaches: 0,
            last_approach_aim: None,
            pending_custom_anim: None,
            capture_phase: 0.0,
        });
        if !(self.data.instant && matches!(target, AbilityTarget::None)) {
            report_status_bit(&mut ctx.effects, ctx.entity, NO_AUTO_ACQUIRE, true);
        }
    }

    fn end_other_activation(&mut self, new_power: &str, ctx: &mut UpdateContext<'_>) {
        if same(&self.data.power, new_power) {
            return;
        }
        let Some(active) = &self.active else {
            return;
        };
        let entity = ctx.entity;
        let was_holding = active.holding;
        let target = active.target;
        let preparing = preparing(active);
        if preparing {
            self.data.trigger.abort(entity, target, ctx);
        }
        let hold = if preparing {
            self.data.paralyze_aborted
        } else {
            LogicFrames::ZERO
        };
        finish_ability(&self.data, &mut self.active, entity, ctx, hold, false);
        let held = self
            .active
            .as_ref()
            .is_some_and(|active| movement_held(&self.data, active));
        if held != was_holding {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(entity, Effect::HoldOrders(held));
            }
        }
    }

    fn pending_approach_check(&self) -> Option<Entity> {
        let active = self.active.as_ref()?;
        if !self.data.requires_los || active.phase != Phase::Approaching {
            return None;
        }
        match active.target {
            AbilityTarget::Object(target) => Some(target),
            AbilityTarget::Position(..) | AbilityTarget::None => None,
        }
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let timer_sleep = match self.effect_until {
            Some(until) if ctx.now >= until => {
                self.effect_until = None;
                self.data.trigger.effect_expired(ctx.entity, ctx);
                None
            }
            Some(until) => Some(Sleep::Frames(
                until.since(ctx.now).unwrap_or(LogicFrames::ONE),
            )),
            None => None,
        };
        let Some(active) = &mut self.active else {
            return timer_sleep.unwrap_or(Sleep::Forever);
        };
        active.frozen = active.frozen.saturating_sub(LogicFrames::ONE);
        active.busy = active.busy.saturating_sub(LogicFrames::ONE);
        let was_holding = active.holding;
        let sleep = self.step(ctx);
        let sleep = match timer_sleep {
            Some(timer_sleep) => crate::upgrade_mux::sooner(sleep, timer_sleep),
            None => sleep,
        };
        let held = self
            .active
            .as_ref()
            .is_some_and(|active| movement_held(&self.data, active));
        if held != was_holding {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(ctx.entity, Effect::HoldOrders(held));
            }
        }
        if let Some(active) = &mut self.active {
            active.holding = held;
        }
        sleep
    }
}

impl<T: Trigger> AbilityUpdate<T> {
    fn step(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let Some(active) = &mut self.active else {
            return Sleep::Forever;
        };
        let entity = ctx.entity;
        if active.pending_custom_anim.is_some_and(|at| ctx.now > at) {
            active.pending_custom_anim = None;
            raise_custom_anim(&self.data, entity, ctx);
        }
        if active.phase != Phase::Paralyzed
            && !ignores_orders(&self.data, active)
            && ctx
                .reads
                .get::<PlayerOrdered>()
                .is_some_and(|ordered| ordered.contains(entity))
        {
            let (target, preparing) = (active.target, preparing(active));
            if preparing {
                self.data.trigger.abort(entity, target, ctx);
            }
            let hold = if preparing {
                self.data.paralyze_aborted
            } else {
                LogicFrames::ZERO
            };
            return finish_ability(&self.data, &mut self.active, entity, ctx, hold, false);
        }
        if active.phase != Phase::Paralyzed && conditions_reject(&self.data, ctx.reads, entity) {
            let (target, preparing) = (active.target, preparing(active));
            if preparing {
                self.data.trigger.abort(entity, target, ctx);
            }
            if self.data.kill_modifier_on_rejected {
                clear_trigger_modifier(&self.data, entity, ctx);
            }
            let hold = if preparing {
                self.data.paralyze_aborted
            } else {
                LogicFrames::ZERO
            };
            return finish_ability(&self.data, &mut self.active, entity, ctx, hold, false);
        }
        let positions = ctx.reads.get::<Positions>();
        let placed = match active.target {
            AbilityTarget::None => None,
            AbilityTarget::Object(target) => {
                let Some(positions) = positions else {
                    return Sleep::NextFrame;
                };
                match (positions.get(entity), positions.get(target)) {
                    (Some(here), Some(there)) => {
                        active.last_known_target = Some((there.0, there.1));
                        Some((here, there))
                    }
                    (Some(here), None) if self.data.survives_target_death => {
                        let (x, y) = active.last_known_target.unwrap_or((here.0, here.1));
                        active.target = AbilityTarget::Position(x, y);
                        Some((here, (x, y, 0.0, 0.0)))
                    }
                    _ => {
                        let preparing = preparing(active);
                        if preparing {
                            self.data
                                .trigger
                                .abort(entity, AbilityTarget::Object(target), ctx);
                        }
                        let hold = if preparing {
                            self.data.paralyze_aborted
                        } else {
                            LogicFrames::ZERO
                        };
                        return finish_ability(
                            &self.data,
                            &mut self.active,
                            entity,
                            ctx,
                            hold,
                            false,
                        );
                    }
                }
            }
            AbilityTarget::Position(x, y) => {
                let Some(positions) = positions else {
                    return Sleep::NextFrame;
                };
                let Some(here) = positions.get(entity) else {
                    let preparing = preparing(active);
                    if preparing {
                        self.data
                            .trigger
                            .abort(entity, AbilityTarget::Position(x, y), ctx);
                    }
                    let hold = if preparing {
                        self.data.paralyze_aborted
                    } else {
                        LogicFrames::ZERO
                    };
                    return finish_ability(&self.data, &mut self.active, entity, ctx, hold, false);
                };
                Some((here, (x, y, 0.0, 0.0)))
            }
        };

        match active.phase {
            Phase::Approaching => {
                if self.data.instant && matches!(active.target, AbilityTarget::None) {
                    self.data.trigger.fire(entity, active.target, ctx);
                    apply_trigger_modifier(&self.data, entity, ctx);
                    report_triggered(&self.data, entity, ctx);
                    play_trigger_sound(&self.data, entity, ctx);
                    reward_trigger(&self.data, entity, ctx);
                    self.active = None;
                    return Sleep::Forever;
                }
                if let Some((here, there)) = placed {
                    let is_horde_approach = self.data.approach_until_members_in_range
                        && ctx
                            .reads
                            .get::<Hordes>()
                            .is_some_and(|hordes| hordes.contains(entity));
                    let distance_squared = if is_horde_approach {
                        Point3::new(here.0, here.1, 0.0)
                            .distance_squared_2d(Point3::new(there.0, there.1, 0.0))
                            + here.2
                    } else {
                        surface_distance_squared_2d(
                            Point3::new(here.0, here.1, 0.0),
                            here.2,
                            Point3::new(there.0, there.1, 0.0),
                            there.2,
                        )
                    };
                    let los_blocked = self.data.requires_los
                        && matches!(active.target, AbilityTarget::Object(_))
                        && ctx
                            .reads
                            .get::<ViewBlocked>()
                            .is_some_and(|blocked| blocked.contains(entity));
                    if !within_attack_range(distance_squared, 0.0, self.data.start_range)
                        || los_blocked
                    {
                        let mover = resolve_mover(ctx.reads, entity);
                        let idle = !ctx
                            .reads
                            .get::<StillMoving>()
                            .is_some_and(|moving| moving.contains(mover));
                        if idle {
                            if active.approaches > APPROACH_GIVE_UP_COUNT {
                                let stuck = active.last_approach_aim.is_some_and(|(ax, ay)| {
                                    let (dx, dy) = (there.0 - ax, there.1 - ay);
                                    dx * dx + dy * dy
                                        < APPROACH_GIVE_UP_DISTANCE * APPROACH_GIVE_UP_DISTANCE
                                });
                                if stuck {
                                    return finish_ability(
                                        &self.data,
                                        &mut self.active,
                                        entity,
                                        ctx,
                                        LogicFrames::ZERO,
                                        false,
                                    );
                                }
                            }
                            active.approaches = active.approaches.saturating_add(1);
                            active.last_approach_aim = Some((there.0, there.1));
                            let approach = match active.target {
                                AbilityTarget::Object(target)
                                    if self.data.start_range > FIXED_POINT_RANGE
                                        && !ctx
                                            .reads
                                            .get::<Structures>()
                                            .is_some_and(|s| s.contains(target)) =>
                                {
                                    Effect::MoveToApproachPoint {
                                        target,
                                        max_range: self.data.start_range,
                                    }
                                }
                                AbilityTarget::Object(target) => Effect::MoveToObject { target },
                                _ => Effect::MoveToPosition {
                                    at: (there.0, there.1),
                                },
                            };
                            if let Some(log) = ctx.effects.get::<EffectLog>() {
                                log.push(entity, approach);
                            }
                        }
                        return Sleep::NextFrame;
                    }
                }
                if self.data.halts_on_arrival
                    && (placed.is_some() || self.data.stops_before_activating)
                {
                    if let Some(log) = ctx.effects.get::<EffectLog>() {
                        log.push(entity, Effect::Halt);
                    }
                }
                if self.data.needs_facing && placed.is_some() {
                    active.phase = Phase::Facing;
                } else {
                    begin_unpack_or_prepare(
                        &self.data,
                        active,
                        &mut self.effect_until,
                        entity,
                        ctx,
                    );
                }
                Sleep::NextFrame
            }
            Phase::Facing => {
                let Some((_, there)) = placed else {
                    return Sleep::NextFrame;
                };
                if ctx
                    .reads
                    .get::<Immobile>()
                    .is_some_and(|set| set.contains(entity))
                {
                    begin_unpack_or_prepare(
                        &self.data,
                        active,
                        &mut self.effect_until,
                        entity,
                        ctx,
                    );
                    return Sleep::NextFrame;
                }
                let mover = resolve_mover(ctx.reads, entity);
                if ctx
                    .reads
                    .get::<StillMoving>()
                    .is_some_and(|moving| moving.contains(mover))
                {
                    return Sleep::NextFrame;
                }
                let Some(mover_here) = positions.and_then(|positions| positions.get(mover)) else {
                    return Sleep::NextFrame;
                };
                let (dx, dy) = (there.0 - mover_here.0, there.1 - mover_here.1);
                let aligned = dx * dx + dy * dy <= f32::MIN_POSITIVE || {
                    let desired = mint_math::f32::atan2(dy, dx);
                    mint_math::f32::angle_delta(mover_here.3, desired).abs()
                        <= self.data.trigger.facing_tolerance()
                };
                if aligned {
                    begin_unpack_or_prepare(
                        &self.data,
                        active,
                        &mut self.effect_until,
                        entity,
                        ctx,
                    );
                } else {
                    if self.data.halts_on_arrival {
                        if let Some(log) = ctx.effects.get::<EffectLog>() {
                            log.push(entity, Effect::Halt);
                        }
                    }
                    let face = match active.target {
                        AbilityTarget::Object(target) => Effect::FaceObject { target },
                        _ => Effect::FacePosition {
                            at: (there.0, there.1),
                        },
                    };
                    if let Some(log) = ctx.effects.get::<EffectLog>() {
                        log.push(entity, face);
                    }
                }
                Sleep::NextFrame
            }
            Phase::Unpacking => {
                if let Some(remaining) = active.remaining.checked_sub(LogicFrames::ONE) {
                    active.remaining = remaining;
                }
                if self.data.lose_stealth_on_trigger
                    && !active.remaining.is_zero()
                    && active.remaining < self.data.pre_trigger_unstealth_frames
                {
                    push_lose_stealth(ctx, entity);
                }
                if active.remaining.is_zero() {
                    start_preparation(&self.data, active, entity, ctx);
                }
                Sleep::NextFrame
            }
            Phase::Preparing => {
                if let Some(remaining) = active.remaining.checked_sub(LogicFrames::ONE) {
                    active.remaining = remaining;
                }
                if self.data.do_capture_fx {
                    if let AbilityTarget::Object(target) = active.target {
                        advance_capture_fx(&self.data, active, target, entity, ctx);
                    }
                }
                if active.remaining.is_zero() {
                    let target = active.target;
                    if !active.last_period {
                        let is_final_trigger = self
                            .data
                            .persistent_count
                            .is_some_and(|most| active.triggers.saturating_add(1) >= most);
                        if !(self.data.silent_final_trigger && is_final_trigger) {
                            if active.triggers == 0 || self.data.trigger.fires_after_first_trigger()
                            {
                                self.data.trigger.fire(entity, target, ctx);
                            }
                            apply_trigger_modifier(&self.data, entity, ctx);
                            report_triggered(&self.data, entity, ctx);
                            if self.data.lose_stealth_on_trigger {
                                push_lose_stealth(ctx, entity);
                            }
                            play_trigger_sound(&self.data, entity, ctx);
                            if active.triggers == 0 {
                                play_active_loop_sound(&self.data, entity, ctx);
                            }
                            reward_trigger(&self.data, entity, ctx);
                        }
                        active.triggers = active.triggers.saturating_add(1);
                        active.frozen = self.data.freeze_frames;
                        active.busy = self.data.busy_frames;
                    }
                    match self.data.persistent_frames {
                        Some(persistent)
                            if !active.last_period
                                && self
                                    .data
                                    .persistent_count
                                    .is_none_or(|most| active.triggers < most) =>
                        {
                            active.remaining = persistent;
                        }
                        _ if self.data.pack_needed => {
                            active.phase = Phase::Packing;
                            active.remaining = self.data.pack_frames;
                            stop_active_loop_sound(&self.data, entity, ctx);
                            push_conditions(
                                ctx,
                                entity,
                                vec![Name::from(PACKING)],
                                vec![Name::from(PREPARING)],
                            );
                        }
                        _ => {
                            return finish_ability(
                                &self.data,
                                &mut self.active,
                                entity,
                                ctx,
                                self.data.paralyze_completed,
                                true,
                            );
                        }
                    }
                } else {
                    let continue_preparation =
                        self.data.trigger.continue_preparation(active.triggers);
                    let out_of_range = continue_preparation
                        && self.data.abort_range.is_some_and(|abort_range| {
                            placed.is_some_and(|(here, there)| {
                                let distance_squared = surface_distance_squared_2d(
                                    Point3::new(here.0, here.1, 0.0),
                                    here.2,
                                    Point3::new(there.0, there.1, 0.0),
                                    there.2,
                                );
                                !within_attack_range(distance_squared, 0.0, abort_range)
                            })
                        });
                    if !continue_preparation || out_of_range {
                        if self.data.pack_needed {
                            active.phase = Phase::Packing;
                            active.remaining = self.data.pack_frames;
                            stop_active_loop_sound(&self.data, entity, ctx);
                            push_conditions(
                                ctx,
                                entity,
                                vec![Name::from(PACKING)],
                                vec![Name::from(PREPARING)],
                            );
                        } else {
                            return finish_ability(
                                &self.data,
                                &mut self.active,
                                entity,
                                ctx,
                                self.data.paralyze_completed,
                                true,
                            );
                        }
                    }
                }
                Sleep::NextFrame
            }
            Phase::Packing => {
                if let Some(remaining) = active.remaining.checked_sub(LogicFrames::ONE) {
                    active.remaining = remaining;
                }
                if self.data.lose_stealth_on_trigger
                    && !active.remaining.is_zero()
                    && active.remaining < self.data.pre_trigger_unstealth_frames
                {
                    push_lose_stealth(ctx, entity);
                }
                if active.remaining.is_zero() {
                    return finish_ability(
                        &self.data,
                        &mut self.active,
                        entity,
                        ctx,
                        self.data.paralyze_completed,
                        true,
                    );
                }
                Sleep::NextFrame
            }
            Phase::Paralyzed => {
                if let Some(remaining) = active.remaining.checked_sub(LogicFrames::ONE) {
                    active.remaining = remaining;
                }
                if active.remaining.is_zero() {
                    self.active = None;
                    end_ability(&self.data, entity, ctx);
                    return Sleep::Forever;
                }
                Sleep::NextFrame
            }
        }
    }
}

const fn preparing(active: &Active) -> bool {
    active.prepare_reported && active.triggers == 0
}

fn begin_unpack_or_prepare<T: Trigger>(
    data: &AbilityData<T>,
    active: &mut Active,
    effect_until: &mut Option<Frame>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    report_using_ability(ctx, entity, true);
    let whole_cast: Vec<Name> = data.packing_type.iter().map(Name::clone).collect();
    data.trigger.begin(entity, ctx);
    *effect_until = data
        .trigger
        .effect_timer()
        .and_then(|lasts| ctx.now.checked_add(lasts));
    if data.unpack_needed {
        let mut add = whole_cast;
        add.push(Name::from(UNPACKING));
        push_conditions(ctx, entity, add, Vec::new());
        active.phase = Phase::Unpacking;
        active.remaining = data.unpack_frames;
        if let Some(anim) = &data.custom_anim {
            if anim.delay.is_zero() {
                raise_custom_anim(data, entity, ctx);
            } else {
                active.pending_custom_anim = ctx.now.checked_add(anim.delay);
            }
        }
        if let Some((condition, lasts)) = &data.trigger_condition {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    entity,
                    Effect::TimedModelCondition {
                        flag: Name::clone(condition),
                        lasts: *lasts,
                        only_if_absent: false,
                    },
                );
            }
        }
    } else {
        push_conditions(ctx, entity, whole_cast, Vec::new());
        start_preparation(data, active, entity, ctx);
    }
}

fn raise_custom_anim<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    let Some(anim) = &data.custom_anim else {
        return;
    };
    match anim.lasts {
        None => push_conditions(ctx, entity, vec![Name::clone(&anim.state)], Vec::new()),
        Some(lasts) => {
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    entity,
                    Effect::TimedModelCondition {
                        flag: Name::clone(&anim.state),
                        lasts,
                        only_if_absent: false,
                    },
                );
            }
        }
    }
}

fn start_preparation<T: Trigger>(
    data: &AbilityData<T>,
    active: &mut Active,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    push_conditions(
        ctx,
        entity,
        vec![Name::from(PREPARING)],
        vec![Name::from(UNPACKING)],
    );
    active.phase = Phase::Preparing;
    active.remaining = data.prep_frames;
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(
            entity,
            Effect::MarkSpecialPowerTriggered(Name::clone(&data.power)),
        );
    }
    if !active.prepare_reported {
        active.prepare_reported = true;
        data.trigger.prepare(entity, active.target, ctx);
    }
}

fn apply_trigger_modifier<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    let Some(modifier) = &data.trigger_modifier else {
        return;
    };
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(
            entity,
            Effect::SelfTimedModifier {
                list: Name::clone(modifier),
                lasts: Some(data.trigger_modifier_duration),
            },
        );
    }
}

fn report_triggered<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    if data.chained_button.is_some() {
        return;
    }
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, Effect::StartRecharge(Name::clone(&data.power)));
    }
}

fn play_trigger_sound<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    let Some(sound) = &data.trigger_sound else {
        return;
    };
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, Effect::PlayObjectSound(Name::clone(sound)));
    }
}

fn play_active_loop_sound<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    let Some(sound) = &data.active_loop else {
        return;
    };
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, Effect::StartLoopSound(Name::clone(sound)));
    }
}

fn stop_active_loop_sound<T: Trigger>(
    data: &AbilityData<T>,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    if data.active_loop.is_none() {
        return;
    }
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, Effect::StopLoopSound);
    }
}

fn advance_capture_fx<T: Trigger>(
    data: &AbilityData<T>,
    active: &mut Active,
    target: Entity,
    entity: Entity,
    ctx: &mut UpdateContext<'_>,
) {
    #[allow(clippy::cast_precision_loss)]
    let total = data.prep_frames.get().max(1) as f32;
    #[allow(clippy::cast_precision_loss)]
    let fraction_left = active.remaining.get() as f32 / total;
    let before = active.capture_phase;
    active.capture_phase += (1.0 - fraction_left) / 3.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (before_floor, after_floor) = (before.floor() as u32, active.capture_phase.floor() as u32);
    if after_floor > before_floor && after_floor % 2 == 0 {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(entity, Effect::CaptureFlash { target });
        }
    }
}

fn reward_trigger<T: Trigger>(data: &AbilityData<T>, entity: Entity, ctx: &mut UpdateContext<'_>) {
    if data.trigger_experience == 0 && data.trigger_skill_points == 0 {
        return;
    }
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(
            entity,
            Effect::TriggerReward {
                experience: data.trigger_experience,
                skill_points: data.trigger_skill_points,
            },
        );
    }
}

pub(crate) fn cap_persistent_count<T: Trigger>(data: &mut AbilityData<T>, most: u32) {
    data.persistent_count = Some(data.persistent_count.map_or(most, |count| count.min(most)));
}

impl<T: Trigger> AbilityData<T> {
    #[cfg(test)]
    pub(crate) const fn persistent_count(&self) -> Option<u32> {
        self.persistent_count
    }
}

pub(crate) fn hold_at_least_after_trigger<T: Trigger>(
    mut data: AbilityData<T>,
    at_least: LogicFrames,
) -> AbilityData<T> {
    if at_least > data.freeze_frames {
        data.freeze_frames = at_least;
    }
    data
}

pub(crate) fn ability_data_from_fields<T: Trigger>(
    fields: &dyn ModuleFieldSource,
    trigger: T,
) -> Result<AbilityData<T>, FieldError> {
    ability_data_from_fields_with_persistence(fields, trigger, true)
}

pub(crate) fn ability_data_from_fields_with_persistence<T: Trigger>(
    fields: &dyn ModuleFieldSource,
    trigger: T,
    persists: bool,
) -> Result<AbilityData<T>, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let start_range = fields
        .field_numeric("StartAbilityRange")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(1e7);
    let (unpack_needed, unpack_frames) = duration(fields, "UnpackTime");
    let (pack_needed, pack_frames) = duration(fields, "PackTime");
    let (prep_needed, prep_frames) = duration(fields, "PreparationTime");
    let (persistent_needed, persistent_frames) = duration(fields, "PersistentPrepTime");
    let skip_continue = crate::fire_weapon_update::flag(fields, "SkipContinue") == Some(true);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let persistent_count = fields
        .field_numeric("PersistentCount")
        .filter(|count| count.is_finite() && *count >= 1.0)
        .map(|count| count.min(1_000_000.0) as u32);
    let charges = crate::fire_weapon_update::flag(fields, "ChargeAttackSpeedBoost") == Some(true);
    let ignore_facing_check =
        crate::fire_weapon_update::flag(fields, "IgnoreFacingCheck") == Some(true);
    let packing_type = packing_type_condition(fields.field_numeric("UnpackingVariation"));
    let (_, paralyze_completed) = duration(fields, "ParalyzeDurationWhenCompleted");
    let (_, paralyze_aborted) = duration(fields, "ParalyzeDurationWhenAborted");
    let (_, freeze_frames) = duration(fields, "FreezeAfterTriggerDuration");
    let (_, busy_frames) = duration(fields, "BusyForDuration");
    let survives_target_death =
        crate::fire_weapon_update::flag(fields, "MustFinishAbility") == Some(true);
    let abort_range = fields
        .field_numeric("AbilityAbortRange")
        .filter(|value| value.is_finite() && *value >= 0.0 && *value < 10_000_000.0);
    let requires_los = crate::fire_weapon_update::flag(fields, "ApproachRequiresLOS") == Some(true);
    let instant = crate::fire_weapon_update::flag(fields, "Instant") == Some(true);
    let chained_button = fields.field("ChainedButton").map(Name::from);
    let trigger_sound = fields
        .field("TriggerSound")
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(Name::from);
    let active_loop = fields
        .field("ActiveLoopSound")
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(Name::from);
    let do_capture_fx = crate::fire_weapon_update::flag(fields, "DoCaptureFX") == Some(true);
    let trigger_experience = whole_points(fields.field_numeric("AwardXPForTriggering"));
    let trigger_skill_points = match fields.field_numeric("SkillPointsForTriggering") {
        Some(value) if value.is_finite() && value != -1.0 => whole_points(Some(value)),
        _ => trigger_experience,
    };
    let trigger_modifier = fields.field("TriggerAttributeModifier").map(Name::from);
    let (_, trigger_modifier_duration) = duration(fields, "AttributeModifierDuration");
    let kill_modifier_on_exit =
        crate::fire_weapon_update::flag(fields, "KillAttributeModifierOnExit") == Some(true);
    let kill_modifier_on_rejected =
        crate::fire_weapon_update::flag(fields, "KillAttributeModifierOnRejected") == Some(true);
    let required_conditions = condition_flags(fields, "RequiredConditions")?;
    let rejected_conditions = condition_flags(fields, "RejectedConditions")?;
    let approach_until_members_in_range =
        crate::fire_weapon_update::flag(fields, "ApproachUntilMembersInRange") == Some(true);
    let custom_anim = custom_anim_and_duration(fields)?;
    let trigger_condition = trigger_model_condition(fields)?;
    let lose_stealth_on_trigger =
        crate::fire_weapon_update::flag(fields, "LoseStealthOnTrigger") == Some(true);
    let (_, pre_trigger_unstealth_frames) = duration(fields, "PreTriggerUnstealthTime");
    let persists = persists && prep_needed;
    let skip_continue_persists = persists && persistent_needed && skip_continue;
    Ok(AbilityData {
        power,
        start_range,
        unpack_needed,
        unpack_frames,
        pack_needed,
        pack_frames,
        prep_frames,
        persistent_frames: (persists && persistent_needed).then_some(persistent_frames),
        persistent_count: if skip_continue_persists {
            Some(2)
        } else {
            persistent_count
        },
        silent_final_trigger: skip_continue_persists,
        halts_on_arrival: !charges,
        needs_facing: !ignore_facing_check,
        packing_type,
        paralyze_completed,
        paralyze_aborted,
        survives_target_death,
        abort_range,
        requires_los,
        ignore_ai_command_always: false,
        freeze_frames,
        busy_frames,
        stops_before_activating: true,
        instant,
        chained_button,
        trigger_sound,
        active_loop,
        do_capture_fx,
        trigger_experience,
        trigger_skill_points,
        trigger_modifier,
        trigger_modifier_duration,
        kill_modifier_on_exit,
        kill_modifier_on_rejected,
        required_conditions,
        rejected_conditions,
        approach_until_members_in_range,
        custom_anim,
        trigger_condition,
        lose_stealth_on_trigger,
        pre_trigger_unstealth_frames,
        trigger,
    })
}

pub(crate) fn set_ignore_ai_command_always<T: Trigger>(data: &mut AbilityData<T>) {
    data.ignore_ai_command_always = true;
}

fn whole_points(value: Option<f32>) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    value
        .filter(|value| value.is_finite() && *value > 0.0)
        .map_or(0, |value| value.min(1_000_000.0) as u32)
}

fn packing_type_condition(value: Option<f32>) -> Option<Name> {
    let value = value?;
    if !value.is_finite() || value < 1.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let variation = value.round() as u32;
    Some(Name::from(format!("PACKING_TYPE_{variation}")))
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrabPassengerEat {
    pub state: Name,
    pub after: LogicFrames,
    pub lasts: LogicFrames,
    pub trigger: LogicFrames,
    pub heal: Option<f32>,
}

pub(crate) fn millisecond_token(token: &str, key: &str) -> Option<LogicFrames> {
    let (found, value) = token.split_once(':')?;
    if !found.eq_ignore_ascii_case(key) {
        return None;
    }
    let ms: u32 = value.trim().parse().ok()?;
    Some(LogicFrames::from_milliseconds(ms.min(86_400_000)))
}

fn retail_ms_token(token: &str, key: &str) -> Option<u32> {
    let (found, value) = token.split_once(':')?;
    if !found.eq_ignore_ascii_case(key) {
        return None;
    }
    value.trim().parse().ok().map(|ms: u32| ms.min(86_400_000))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CustomAnim {
    pub(crate) state: Name,
    pub(crate) lasts: Option<LogicFrames>,
    pub(crate) delay: LogicFrames,
}

pub(crate) fn custom_anim_and_duration(
    fields: &dyn ModuleFieldSource,
) -> Result<Option<CustomAnim>, FieldError> {
    let args = fields.field_args("CustomAnimAndDuration");
    if args.is_empty() {
        return Ok(None);
    }
    let bad = || {
        FieldError::new(format!(
            "CustomAnimAndDuration '{}' is not AnimState:<flag> AnimTime:<ms> \
             [TriggerTime:<ms>]",
            args.join(" ")
        ))
    };
    let (state, time, trigger) = match args.as_slice() {
        [state, time] => (state, time, None),
        [state, time, trigger] => (state, time, Some(trigger)),
        _ => return Err(bad()),
    };
    let state = state
        .split_once(':')
        .filter(|(key, flag)| key.eq_ignore_ascii_case("AnimState") && !flag.is_empty())
        .map(|(_, flag)| Name::from(flag))
        .ok_or_else(bad)?;
    let anim_ms = retail_ms_token(time, "AnimTime").ok_or_else(bad)?;
    let delay_ms = match trigger {
        Some(trigger) => retail_ms_token(trigger, "TriggerTime").ok_or_else(bad)?,
        None => 0,
    };
    Ok(Some(CustomAnim {
        state,
        lasts: (anim_ms != 0).then(|| retail_duration_ticks(anim_ms).max(LogicFrames::ONE)),
        delay: retail_duration_ticks(delay_ms),
    }))
}

pub(crate) fn trigger_model_condition(
    fields: &dyn ModuleFieldSource,
) -> Result<Option<(Name, LogicFrames)>, FieldError> {
    let args = fields.field_args("TriggerModelCondition");
    if args.is_empty() {
        return Ok(None);
    }
    let [state] = args.as_slice() else {
        return Err(FieldError::new(format!(
            "TriggerModelCondition '{}' is not ModelConditionState:<condition>",
            args.join(" ")
        )));
    };
    let condition = state
        .split_once(':')
        .filter(|(key, flag)| key.eq_ignore_ascii_case("ModelConditionState") && !flag.is_empty())
        .map(|(_, flag)| Name::from(flag))
        .ok_or_else(|| {
            FieldError::new(format!(
                "TriggerModelCondition '{state}' is not ModelConditionState:<condition>"
            ))
        })?;
    let seconds = fields
        .field_numeric("TriggerModelConditionDuration")
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let retail_frames = seconds.min(1_000_000.0).trunc() as u32;
    Ok(Some((
        condition,
        LogicFrames::from_frames(retail_frames.saturating_mul(TICKS_PER_RETAIL_FRAME)),
    )))
}

pub fn grab_passenger_eat(
    fields: &dyn ModuleFieldSource,
) -> Result<Option<GrabPassengerEat>, FieldError> {
    let args = fields.field_args("GrabPassengerAnimAndDuration");
    if args.is_empty() {
        return Ok(None);
    }
    let bad = || {
        FieldError::new(format!(
            "GrabPassengerAnimAndDuration '{}' is not AnimState:<name> AnimTime:<ms> \
             TriggerTime:<ms>",
            args.join(" ")
        ))
    };
    let [state, lasts, trigger] = args.as_slice() else {
        return Err(bad());
    };
    let state = state
        .split_once(':')
        .filter(|(key, name)| key.eq_ignore_ascii_case("AnimState") && !name.is_empty())
        .map(|(_, name)| Name::from(name))
        .ok_or_else(bad)?;
    let lasts = millisecond_token(lasts, "AnimTime").ok_or_else(bad)?;
    let trigger = millisecond_token(trigger, "TriggerTime")
        .filter(|trigger| *trigger <= lasts)
        .ok_or_else(bad)?;
    let heal = match fields.field("GrabPassengerHealGainPercent") {
        None => None,
        Some(_) => Some(
            fields
                .field_percent("GrabPassengerHealGainPercent")
                .filter(|share| *share >= 0.0)
                .ok_or_else(|| FieldError::new("GrabPassengerHealGainPercent is not a percent"))?,
        ),
    };
    Ok(Some(GrabPassengerEat {
        state,
        after: duration(fields, "PersistentPrepTime")
            .1
            .checked_add(duration(fields, "PackTime").1)
            .ok_or_else(|| FieldError::new("PersistentPrepTime + PackTime overflows"))?,
        lasts,
        trigger,
        heal,
    }))
}

pub fn special_ability_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpecialAbilityUpdate, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    grab_passenger_eat(fields)?;
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, PowerTrigger { power })?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
