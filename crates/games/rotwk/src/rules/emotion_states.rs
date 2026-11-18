use std::collections::BTreeMap;

use mint_movement::{Facing, Route, Vec2};
use mint_sage_ini::{EmotionAiState, EmotionType};
use mint_sage_modules::Effect;
use mint_sim::{Entity, LogicFrames, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::unit::{Path, PathKind};

const FACE_TOLERANCE: f32 = 0.035;

const BACK_UP_CELL: f32 = 10.0;

const COWER_FLEE_DISTANCE: f32 = 20.0;

const COWER_STEP_SHARE: f32 = 0.9;

const COWER_RADIUS_STEP: f32 = 5.0;

const QUARREL_CENTRE_LIMIT_SQUARED: f32 = 10_000.0;

const QUARREL_RING_MIN: f32 = 50.0;
const QUARREL_RING_MAX: f32 = 60.0;

const QUARREL_SEPARATION: f32 = 20.0;

const QUARREL_CLOSE_SQUARED: f32 = 410.0;

const QUARREL_CROWDED_SQUARED: f32 = 55.0;

const QUARREL_SHIFT: f32 = 4.0;

const QUARRELSOME_FIGHTING: &str = "QUARRELSOME_FIGHTING";
const EMOTION_QUARRELSOME: &str = "EMOTION_QUARRELSOME";

const BACK_AWAY_MOVEMENT_CONDITIONS: [&str; 8] = [
    "MOVING",
    "TURN_LEFT",
    "TURN_RIGHT",
    "TURN_LEFT_HIGH_SPEED",
    "TURN_RIGHT_HIGH_SPEED",
    "ACCELERATE",
    "DECELERATE",
    "ENGAGED",
];

const RECENT_DAMAGE: LogicFrames = LogicFrames::from_engine_milliseconds(4000);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    BackAway,
    Idle,
    Panic { animal: bool, fled: bool },
    Cower { until: Option<mint_sim::Frame> },
    Face { turns: u32 },
    Quarrel { pair: Option<(Entity, Entity)> },
}

impl Phase {
    const fn code(self) -> u8 {
        match self {
            Self::BackAway => 0,
            Self::Idle => 1,
            Self::Panic { .. } => 2,
            Self::Cower { .. } => 5,
            Self::Face { .. } => 3,
            Self::Quarrel { .. } => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Backing {
    member: Entity,
    to: Vec2,
    delay: u32,
    sent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmotionState {
    phase: Phase,
    goal: Entity,
    locked: bool,
    interrupted: bool,
    stash: Option<Path>,
    backing: Vec<Backing>,
    ring: Vec<(Entity, f32)>,
}

impl EmotionState {
    pub(super) fn holds_route(&self) -> bool {
        self.stash.is_some()
    }

    pub(super) fn mark_animal_flee(&mut self) {
        if let Phase::Panic { fled, .. } = self.phase {
            self.phase = Phase::Panic { animal: true, fled };
        }
    }

    pub(super) fn unlock(&mut self) {
        self.locked = false;
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.phase.code());
        match self.phase {
            Phase::Face { turns } => hasher.write_u32(turns),
            Phase::Panic { animal, fled } => {
                hasher.write_bool(animal);
                hasher.write_bool(fled);
            }
            Phase::Cower { until } => {
                hasher.write_bool(until.is_some());
                if let Some(until) = until {
                    hasher.write_u64(until.get());
                }
            }
            Phase::Quarrel { pair } => {
                hasher.write_bool(pair.is_some());
                if let Some((a, b)) = pair {
                    hasher.write_u32(a.index());
                    hasher.write_u32(b.index());
                }
            }
            _ => {}
        }
        hasher.write_u32(self.goal.index());
        hasher.write_bool(self.locked);
        hasher.write_bool(self.interrupted);
        hasher.write_bool(self.stash.is_some());
        if let Some(path) = &self.stash {
            mint_sim::Component::hash_into(path, hasher);
        }
        hasher.write_usize(self.backing.len());
        for entry in &self.backing {
            hasher.write_u32(entry.member.index());
            hasher.write_f32(entry.to.x);
            hasher.write_f32(entry.to.y);
            hasher.write_u32(entry.delay);
            hasher.write_bool(entry.sent);
        }
        hasher.write_usize(self.ring.len());
        for (member, radius) in &self.ring {
            hasher.write_u32(member.index());
            hasher.write_f32(*radius);
        }
    }
}

fn holders(world: &World, ids: Ids, entity: Entity) -> [Option<Entity>; 2] {
    let horde = world
        .get(ids.member, entity)
        .map(|member| member.horde)
        .filter(|horde| world.contains(ids.horde, *horde));
    [Some(entity), horde]
}

fn running_state(world: &World, ids: Ids, holder: Entity) -> Option<&EmotionState> {
    world
        .get(ids.emotion, holder)?
        .state
        .as_ref()
        .filter(|state| !state.interrupted)
}

impl RotwkRules {
    pub(super) fn emotion_refuses_order(world: &World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.rampaging, entity)
            || holders(world, ids, entity)
                .into_iter()
                .flatten()
                .any(|holder| {
                    world
                        .get(ids.emotion, holder)
                        .is_some_and(|emotion| emotion.prevents_commands)
                        || running_state(world, ids, holder).is_some_and(|state| state.locked)
                })
    }

    pub(super) fn emotion_locked(world: &World, ids: Ids, entity: Entity) -> bool {
        holders(world, ids, entity)
            .into_iter()
            .flatten()
            .any(|holder| running_state(world, ids, holder).is_some_and(|state| state.locked))
    }

    pub(super) fn emotion_holds(world: &World, ids: Ids, entity: Entity) -> bool {
        holders(world, ids, entity)
            .into_iter()
            .flatten()
            .any(|holder| running_state(world, ids, holder).is_some())
    }

    pub(super) fn interrupt_emotion_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        for holder in holders(tick.state(), ids, entity).into_iter().flatten() {
            let interrupted = tick
                .state_mut()
                .get_mut(ids.emotion, holder)
                .and_then(|emotion| emotion.state.as_mut())
                .is_some_and(|state| {
                    if state.locked {
                        false
                    } else {
                        state.interrupted = true;
                        state.stash = None;
                        true
                    }
                });
            if interrupted {
                self.apply_locomotor_override(tick, ids, holder, None)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter_emotion_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: EmotionAiState,
        goal: Entity,
        flee_bonus: f32,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let (phase, locked) = match state {
            EmotionAiState::BackAway => (Phase::BackAway, true),
            EmotionAiState::Idle => (Phase::Idle, true),
            EmotionAiState::RunAwayPanic => (
                Phase::Panic {
                    animal: false,
                    fled: false,
                },
                true,
            ),
            EmotionAiState::FaceObject => (Phase::Face { turns: 0 }, false),
            EmotionAiState::Quarrel => (Phase::Quarrel { pair: None }, false),
            EmotionAiState::AvoidScarer => return Ok(()),
        };
        let Some(emotion) = tick.state().get(ids.emotion, entity).cloned() else {
            return Ok(());
        };
        if emotion
            .state
            .as_ref()
            .is_some_and(|held| held.phase.code() == phase.code() && !held.interrupted)
        {
            return Ok(());
        }
        if let Some(horde) = tick.state().get(ids.horde, entity).cloned() {
            self.end_melee(tick, ids, entity)?;
            for member in &horde.members {
                drop_route(tick, ids, *member);
                tick.state_mut().remove(ids.attack_order, *member);
            }
        }
        let stash = match emotion.state.and_then(|held| held.stash) {
            Some(held) => Some(held),
            None => take_route(tick, ids, entity),
        };
        let mut held = EmotionState {
            phase,
            goal,
            locked,
            interrupted: false,
            stash,
            backing: Vec::new(),
            ring: Vec::new(),
        };
        self.enter_phase(tick, ids, entity, &mut held, flee_bonus, effects)?;
        if let Some(emotion) = tick.state_mut().get_mut(ids.emotion, entity) {
            emotion.state = Some(held);
        }
        Ok(())
    }

    fn enter_phase(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: &mut EmotionState,
        flee_bonus: f32,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let goal_at = tick
            .state()
            .get(ids.body, state.goal)
            .map(|body| body.0.position);
        match state.phase {
            Phase::BackAway => {
                self.push_conditions_for_horde(
                    tick.state(),
                    ids,
                    entity,
                    Vec::new(),
                    BACK_AWAY_MOVEMENT_CONDITIONS
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    effects,
                );
                let Some(scarer) = goal_at else {
                    return Ok(());
                };
                if tick.state().contains(ids.horde, entity) {
                    state.backing = self.back_away_members(tick, ids, entity, state.goal, scarer);
                    mark_dirty(tick, ids, entity);
                } else {
                    snap_face(tick, ids, entity, scarer)?;
                }
            }
            Phase::Panic { .. } => {
                self.apply_locomotor_override(tick, ids, entity, Some("PANIC"))?;
                self.push_conditions_for_horde(
                    tick.state(),
                    ids,
                    entity,
                    vec!["PANICKING".to_owned()],
                    Vec::new(),
                    effects,
                );
                if goal_at.is_some() {
                    if let Some(to) =
                        self.request_safe_goal(tick, ids, entity, state.goal, flee_bonus)?
                    {
                        walk(tick, ids, entity, to)?;
                    }
                }
            }
            Phase::Quarrel { .. } => {
                let (pair, ring) = self.instigate_quarrel(tick, ids, entity);
                state.phase = Phase::Quarrel { pair };
                state.ring = ring;
            }
            Phase::Idle | Phase::Face { .. } | Phase::Cower { .. } => {}
        }
        Ok(())
    }

    pub(super) fn advance_emotion_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        frame_due: bool,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(mut state) = tick
            .state()
            .get(ids.emotion, entity)
            .and_then(|emotion| emotion.state.clone())
        else {
            return Ok(());
        };
        if state.interrupted {
            return self.deactivate(tick, ids, entity, effects);
        }
        let now = tick.frame();
        let goal_dead = tick
            .state()
            .get(ids.health, state.goal)
            .is_some_and(|health| health.0.is_dead());
        let goal_spell_book = tick
            .state()
            .get(ids.shape, state.goal)
            .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::SPELL_BOOK));
        let goal_at = tick
            .state()
            .get(ids.body, state.goal)
            .filter(|_| !goal_dead || goal_spell_book)
            .map(|body| body.0.position);
        let hurt = tick
            .state()
            .get(ids.emotion_table, entity)
            .is_some_and(|table| table.hurt_within(RECENT_DAMAGE, now));
        let finished = match state.phase {
            Phase::BackAway => {
                if goal_at.is_none() || (hurt && !state.locked) {
                    true
                } else {
                    self.advance_backing(tick, ids, entity, &mut state, goal_at);
                    false
                }
            }
            Phase::Idle => false,
            Phase::Panic { animal, fled } => {
                let walking = has_own_walk(tick.state(), ids, entity);
                if walking {
                    state.phase = Phase::Panic {
                        animal,
                        fled: fled || animal,
                    };
                    false
                } else if animal && fled {
                    self.enter_cower(tick, ids, entity, &mut state, goal_at, effects)?
                } else {
                    true
                }
            }
            Phase::Cower { until } => {
                self.advance_cower(tick, ids, entity, &mut state, until, goal_at, now)?
            }
            Phase::Face { turns } => match goal_at {
                None => true,
                Some(target) => {
                    let off = tick.state().get(ids.body, entity).and_then(|body| {
                        let wanted = Facing::from_vector(target.minus(body.0.position))?;
                        Some(body.0.facing.delta_to(wanted).abs())
                    });
                    match off {
                        Some(off) if off < FACE_TOLERANCE => true,
                        _ => {
                            if off.is_some() {
                                if self.cannot_turn_in_place(tick.state(), ids, entity) {
                                    if frame_due || !has_own_walk(tick.state(), ids, entity) {
                                        walk(tick, ids, entity, target)?;
                                    }
                                } else if !tick.state().contains(ids.horde, entity) {
                                    self.turn_towards(tick, ids, entity, target)?;
                                }
                                mark_dirty(tick, ids, entity);
                            }
                            if self.holds_a_stance(tick.state(), ids, entity) && frame_due {
                                let next = turns.saturating_add(1);
                                state.phase = Phase::Face { turns: next };
                                next >= 5
                            } else {
                                false
                            }
                        }
                    }
                }
            },
            Phase::Quarrel { pair } => {
                hurt || self.advance_quarrel(tick, ids, entity, &state, pair, effects)
            }
        };
        if finished {
            if let Some(emotion) = tick.state_mut().get_mut(ids.emotion, entity) {
                emotion.state = Some(state);
            }
            return self.leave_emotion_state(tick, ids, entity, effects);
        }
        if let Some(emotion) = tick.state_mut().get_mut(ids.emotion, entity) {
            emotion.state = Some(state);
        }
        Ok(())
    }

    fn enter_cower(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: &mut EmotionState,
        enemy_at: Option<Vec2>,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<bool> {
        let (Some(enemy_at), Some(at)) = (
            enemy_at,
            tick.state()
                .get(ids.body, entity)
                .map(|body| body.0.position),
        ) else {
            return Ok(true);
        };
        let z = |who: Entity| {
            tick.state()
                .get(ids.elevation, who)
                .map_or(0.0, |elevation| elevation.0)
        };
        let enemy_height = tick
            .state()
            .get(ids.shape, state.goal)
            .map_or(0.0, |shape| shape.height);
        if z(entity) - z(state.goal) > enemy_height {
            return Ok(true);
        }
        state.phase = Phase::Cower { until: None };
        self.push_conditions_for_horde(
            tick.state(),
            ids,
            entity,
            vec!["EMOTION_AFRAID".to_owned()],
            Vec::new(),
            effects,
        );
        let away = at.minus(enemy_at);
        let length = away.length();
        if length > 0.0 {
            let to = at.plus(away.scale(COWER_FLEE_DISTANCE / length));
            walk(tick, ids, entity, to)?;
        }
        Ok(false)
    }

    #[allow(clippy::too_many_arguments)]
    fn advance_cower(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: &mut EmotionState,
        until: Option<mint_sim::Frame>,
        enemy_at: Option<Vec2>,
        now: mint_sim::Frame,
    ) -> Result<bool> {
        let Some(enemy_at) = enemy_at else {
            return Ok(true);
        };
        if until.is_none() {
            if has_own_walk(tick.state(), ids, entity) {
                return Ok(false);
            }
            let (min, max) = self.cower_window(tick.state(), ids, entity);
            let span = max.saturating_sub(min).saturating_add(1);
            let drawn = min.saturating_add(tick.rng().below(span));
            let end = now
                .checked_add(LogicFrames::from_frames(drawn))
                .unwrap_or(mint_sim::Frame::FOREVER);
            state.phase = Phase::Cower { until: Some(end) };
            return Ok(false);
        }
        snap_face(tick, ids, entity, enemy_at)?;
        Ok(until.is_some_and(|end| now > end))
    }

    fn cower_window(&self, world: &World, ids: Ids, entity: Entity) -> (u32, u32) {
        let ai = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record.ai);
        let ticks = |ms: Option<f32>| super::death::frames_of_ms(ms.unwrap_or(0.0)).get();
        ai.map_or((0, 0), |ai| {
            (ticks(ai.min_cower_time_ms), ticks(ai.max_cower_time_ms))
        })
    }

    fn advance_backing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: &mut EmotionState,
        scarer: Option<Vec2>,
    ) {
        let cower_radius = self
            .back_away_tuning(tick.state(), ids, entity)
            .cower_radius;
        if scarer.is_some() && cower_radius > 0.0 {
            return;
        }
        for entry in &mut state.backing {
            if entry.sent || self.is_gone_member(tick.state(), ids, entry.member) {
                continue;
            }
            if entry.delay > 0 {
                entry.delay = entry.delay.saturating_sub(1);
                continue;
            }
            entry.sent = true;
        }
    }

    fn backing_orders(
        &self,
        world: &World,
        ids: Ids,
        holder: Entity,
        state: &EmotionState,
        scarer: Option<Vec2>,
        order: &mut crate::horde::FormationOrder,
    ) {
        let cower_radius = self.back_away_tuning(world, ids, holder).cower_radius;
        for entry in &state.backing {
            if self.is_gone_member(world, ids, entry.member) {
                continue;
            }
            let slot = if let (Some(scarer), true) = (scarer, cower_radius > 0.0) {
                let Some(at) = world
                    .get(ids.body, entry.member)
                    .map(|body| body.0.position)
                else {
                    continue;
                };
                let (away, distance) =
                    self.separation(world, ids, entry.member, at, state.goal, scarer);
                if distance < COWER_STEP_SHARE * cower_radius && distance > 0.0 {
                    let step = COWER_RADIUS_STEP / distance;
                    Some(Vec2::new(at.x + away.x * step, at.y + away.y * step))
                } else if distance < cower_radius {
                    Some(at)
                } else {
                    None
                }
            } else if entry.sent {
                Some(entry.to)
            } else {
                continue;
            };
            order.members.insert(
                entry.member,
                crate::horde::MemberOrder { slot, face: scarer },
            );
        }
    }

    pub(super) fn backing_members(&self, world: &World, ids: Ids) -> Vec<Entity> {
        let Some(store) = world.store(ids.emotion) else {
            return Vec::new();
        };
        let mut held = Vec::new();
        for (horde, emotion) in store.iter() {
            let Some(state) = emotion.state.as_ref().filter(|state| !state.interrupted) else {
                continue;
            };
            if self.is_porcupine(world, ids, horde) {
                continue;
            }
            let waits = self.back_away_tuning(world, ids, horde).cower_radius == 0.0;
            held.extend(
                state
                    .backing
                    .iter()
                    .filter(|entry| waits && !entry.sent)
                    .map(|entry| entry.member),
            );
        }
        held
    }

    fn cannot_turn_in_place(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        crate::unit::current_limits(
            world,
            &self.kinds,
            (ids.kind, ids.locomotion, ids.health),
            entity,
            None,
            1.0,
        )
        .is_some_and(|limits| limits.min_speed > 0.0)
    }

    pub(super) fn formation_orders(
        &self,
        world: &World,
        ids: Ids,
    ) -> BTreeMap<Entity, crate::horde::FormationOrder> {
        let mut orders = BTreeMap::new();
        let Some(store) = world.store(ids.emotion) else {
            return orders;
        };
        for (holder, emotion) in store.iter() {
            let Some(state) = emotion.state.as_ref().filter(|state| !state.interrupted) else {
                continue;
            };
            if !world.contains(ids.horde, holder) || self.is_porcupine(world, ids, holder) {
                continue;
            }
            let goal_at = world
                .get(ids.body, state.goal)
                .filter(|_| {
                    !world
                        .get(ids.health, state.goal)
                        .is_some_and(|health| health.0.is_dead())
                })
                .map(|body| body.0.position);
            let mut order = crate::horde::FormationOrder::default();
            match state.phase {
                Phase::BackAway if !state.backing.is_empty() => {
                    order.face_point = goal_at;
                    self.backing_orders(world, ids, holder, state, goal_at, &mut order);
                }
                Phase::Face { .. } if !self.cannot_turn_in_place(world, ids, holder) => {
                    order.face_point = goal_at;
                }
                Phase::Quarrel { pair: Some(pair) } if !state.ring.is_empty() => {
                    quarrel_orders(world, ids, holder, pair, &state.ring, &mut order);
                }
                _ => {}
            }
            if order.face_point.is_some() || !order.members.is_empty() {
                orders.insert(holder, order);
            }
        }
        orders
    }

    fn is_porcupine(&self, world: &World, ids: Ids, horde: Entity) -> bool {
        world
            .get(ids.horde, horde)
            .and_then(|contain| self.formations.get(usize::from(contain.formation)))
            .is_some_and(|table| table.settings().porcupine)
    }

    pub(super) fn backing_hordes(
        &self,
        world: &World,
        ids: Ids,
    ) -> std::collections::BTreeSet<Entity> {
        world
            .store(ids.emotion)
            .map(|store| {
                store
                    .iter()
                    .filter(|(horde, emotion)| {
                        !self.is_porcupine(world, ids, *horde)
                            && emotion
                                .state
                                .as_ref()
                                .is_some_and(|state| !state.backing.is_empty())
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_gone_member(&self, world: &World, ids: Ids, member: Entity) -> bool {
        !world.contains(ids.body, member)
            || world
                .get(ids.health, member)
                .is_some_and(|health| health.0.is_dead())
    }

    pub(super) fn separation(
        &self,
        world: &World,
        ids: Ids,
        member: Entity,
        at: Vec2,
        scarer_object: Entity,
        scarer: Vec2,
    ) -> (Vec2, f32) {
        let away = at.minus(scarer);
        let rise = self.object_z(world, ids, member) - self.object_z(world, ids, scarer_object);
        let length = (away.length_squared() + rise * rise).sqrt();
        (away, length)
    }

    fn back_away_tuning(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> mint_sage_modules::BackAway {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.back_away)
            .unwrap_or(mint_sage_modules::BackAway {
                min_delay_ms: 400.0,
                max_delay_ms: 3000.0,
                min_distance: 3.0,
                max_distance: 5.0,
                percentage: 0.5,
                cower_radius: 0.0,
            })
    }

    fn back_away_members(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        scarer_object: Entity,
        scarer: Vec2,
    ) -> Vec<Backing> {
        let tuning = self.back_away_tuning(tick.state(), ids, entity);
        let members = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        let frames = |ms: f32| -> u32 {
            if !ms.is_finite() || ms <= 0.0 {
                return 0;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = (ms.min(3_600_000.0) / 200.0).ceil() as u32;
            whole
        };
        let (low, high) = (frames(tuning.min_delay_ms), frames(tuning.max_delay_ms));
        let per_frame = super::emotions::ONE_FRAME.get();
        let mut backing = Vec::new();
        for member in members {
            if self.is_gone_member(tick.state(), ids, member) {
                continue;
            }
            if tuning.cower_radius == 0.0 && tick.rng().next_f32() > tuning.percentage {
                continue;
            }
            let span = tuning.max_distance - tuning.min_distance;
            let distance = tuning.min_distance + span * tick.rng().next_f32();
            let spread = high.saturating_sub(low).saturating_add(1);
            let delay = low.saturating_add(tick.rng().below(spread));
            let Some(at) = tick
                .state()
                .get(ids.body, member)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let (away, length) =
                self.separation(tick.state(), ids, member, at, scarer_object, scarer);
            if !length.is_finite() || length <= 0.0 {
                continue;
            }
            let step = BACK_UP_CELL * distance / length;
            let to = self.on_map(at, Vec2::new(at.x + away.x * step, at.y + away.y * step));
            backing.push(Backing {
                member,
                to,
                delay: delay.saturating_mul(per_frame),
                sent: false,
            });
        }
        backing
    }

    #[allow(clippy::type_complexity)]
    fn instigate_quarrel(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> (Option<(Entity, Entity)>, Vec<(Entity, f32)>) {
        let none = (None, Vec::new());
        let Some(horde) = tick.state().get(ids.horde, entity) else {
            return none;
        };
        let members: Vec<(Entity, Vec2)> = horde
            .members
            .iter()
            .filter_map(|member| Some((*member, tick.state().get(ids.body, *member)?.0.position)))
            .collect();
        let Ok(count) = u32::try_from(members.len()) else {
            return none;
        };
        if count < 2 {
            return none;
        }
        let first = quarrel_first(&members.iter().map(|(_, at)| *at).collect::<Vec<_>>());
        let offset = 1usize.saturating_add(
            usize::try_from(tick.rng().below(count.saturating_sub(1))).unwrap_or(0),
        );
        let second = first.saturating_add(offset) % members.len();
        let mut ring = Vec::new();
        for (index, (member, _)) in members.iter().enumerate() {
            if index != first && index != second {
                let radius = (QUARREL_RING_MIN
                    + (QUARREL_RING_MAX - QUARREL_RING_MIN) * tick.rng().next_f32())
                .floor();
                ring.push((*member, radius));
            }
        }
        let pair = members.get(first).zip(members.get(second));
        (pair.map(|(a, b)| (a.0, b.0)), ring)
    }

    fn advance_quarrel(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        holder: Entity,
        state: &EmotionState,
        pair: Option<(Entity, Entity)>,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> bool {
        let Some((a, b)) = pair.filter(|_| !state.ring.is_empty()) else {
            return false;
        };
        let world = tick.state();
        let selected = |who: Entity| controller_selected(world, ids, who);
        let members = world
            .get(ids.horde, holder)
            .map(|horde| horde.members.as_slice())
            .unwrap_or_default();
        if self.is_gone_member(world, ids, a)
            || self.is_gone_member(world, ids, b)
            || selected(holder)
            || members.iter().any(|member| selected(*member))
        {
            return true;
        }
        if quarrel_close(world, ids, a, b) {
            let mut show = |who: Entity, name: &str| {
                if !self.holds_model_condition(tick, who, name) {
                    effects.push((
                        who,
                        Effect::ModelConditions {
                            add: mint_sage_modules::effects::names(vec![name.to_owned()]),
                            remove: mint_sage_modules::effects::names(Vec::new()),
                        },
                    ));
                }
            };
            show(a, QUARRELSOME_FIGHTING);
            show(b, QUARRELSOME_FIGHTING);
            for (member, _) in &state.ring {
                show(*member, EMOTION_QUARRELSOME);
            }
        }
        false
    }

    fn on_map(&self, from: Vec2, goal: Vec2) -> Vec2 {
        let mut goal = goal;
        for _ in 0..4 {
            if self.passability.cell_at(goal).is_some() {
                return goal;
            }
            goal = Vec2::new((goal.x + from.x) * 0.5, (goal.y + from.y) * 0.5);
        }
        from
    }

    pub(super) fn leave_emotion_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(state) = tick
            .state_mut()
            .get_mut(ids.emotion, entity)
            .and_then(|emotion| emotion.state.take())
        else {
            return Ok(());
        };
        match state.phase {
            Phase::BackAway => {
                if let Some(table) = tick.state_mut().get_mut(ids.emotion_table, entity) {
                    table.clear(EmotionType::Fear);
                }
                for entry in &state.backing {
                    if has_own_walk(tick.state(), ids, entry.member) {
                        drop_route(tick, ids, entry.member);
                    }
                }
                mark_dirty(tick, ids, entity);
            }
            Phase::Panic { .. } => self.push_conditions_for_horde(
                tick.state(),
                ids,
                entity,
                Vec::new(),
                vec!["PANICKING".to_owned()],
                effects,
            ),
            Phase::Cower { .. } => self.push_conditions_for_horde(
                tick.state(),
                ids,
                entity,
                Vec::new(),
                vec!["EMOTION_AFRAID".to_owned()],
                effects,
            ),
            Phase::Face { .. } => mark_dirty(tick, ids, entity),
            Phase::Quarrel { .. } => {
                self.push_conditions_for_horde(
                    tick.state(),
                    ids,
                    entity,
                    Vec::new(),
                    vec![
                        QUARRELSOME_FIGHTING.to_owned(),
                        EMOTION_QUARRELSOME.to_owned(),
                    ],
                    effects,
                );
                mark_dirty(tick, ids, entity);
            }
            Phase::Idle => {}
        }
        if has_own_walk(tick.state(), ids, entity) {
            drop_route(tick, ids, entity);
        }
        if let Some(stash) = state.stash.filter(|_| !state.interrupted) {
            if !tick.state().contains(ids.path, entity) {
                tick.state_mut().insert(ids.path, entity, stash)?;
            }
        }
        Ok(())
    }
}

pub(super) fn controller_selected(world: &World, ids: Ids, who: Entity) -> bool {
    world
        .get(ids.owner, who)
        .zip(world.get(ids.selected, who))
        .is_some_and(|(owner, mask)| mask.holds(owner.0))
}

pub(super) fn quarrel_first(spots: &[Vec2]) -> usize {
    let count = f32::from(u16::try_from(spots.len()).unwrap_or(u16::MAX));
    let sum = spots
        .iter()
        .fold(Vec2::ZERO, |sum, at| Vec2::new(sum.x + at.x, sum.y + at.y));
    let centre = Vec2::new(sum.x / count, sum.y / count);
    let mut first = 0;
    let mut best = QUARREL_CENTRE_LIMIT_SQUARED;
    for (index, at) in spots.iter().enumerate() {
        let squared = at.minus(centre).length_squared();
        if squared < best {
            best = squared;
            first = index;
        }
    }
    first
}

fn quarrel_close(world: &World, ids: Ids, a: Entity, b: Entity) -> bool {
    let at = |who: Entity| world.get(ids.body, who).map(|body| body.0.position);
    at(a).zip(at(b)).is_some_and(|(a, b)| {
        let gap = b.minus(a);
        gap.length_squared() <= QUARREL_CLOSE_SQUARED
    })
}

fn quarrel_orders(
    world: &World,
    ids: Ids,
    horde: Entity,
    (a, b): (Entity, Entity),
    ring: &[(Entity, f32)],
    order: &mut crate::horde::FormationOrder,
) {
    let at = |who: Entity| world.get(ids.body, who).map(|body| body.0.position);
    let (Some(from), Some(to)) = (at(a), at(b)) else {
        return;
    };
    if let Some(direction) = to.minus(from).normalised() {
        order.members.insert(
            b,
            crate::horde::MemberOrder {
                slot: Some(from.plus(direction.scale(QUARREL_SEPARATION))),
                face: Some(from),
            },
        );
    }
    if !quarrel_close(world, ids, a, b) {
        return;
    }
    order.members.entry(a).or_default().face = Some(to);
    let everyone: Vec<(Entity, Vec2)> = world
        .get(ids.horde, horde)
        .map(|horde| horde.members.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|member| Some((*member, at(*member)?)))
        .collect();
    for (member, radius) in ring {
        let Some(here) = at(*member) else {
            continue;
        };
        let Some(outward) = here.minus(from).normalised() else {
            continue;
        };
        let mut point = from.plus(outward.scale(*radius));
        let crowder = everyone
            .iter()
            .filter(|(other, _)| other != member)
            .map(|(_, there)| (*there, there.minus(here).length_squared()))
            .min_by(|(_, x), (_, y)| x.total_cmp(y))
            .filter(|(_, squared)| *squared <= QUARREL_CROWDED_SQUARED)
            .map(|(there, _)| there);
        if let Some(there) = crowder {
            let tangent = Vec2::new(-outward.y, outward.x);
            let away = if tangent.dot(there.minus(here)) > 0.0 {
                -QUARREL_SHIFT
            } else {
                QUARREL_SHIFT
            };
            point = point.plus(tangent.scale(away));
        }
        order.members.insert(
            *member,
            crate::horde::MemberOrder {
                slot: Some(point),
                face: Some(from),
            },
        );
    }
}

fn has_own_walk(world: &World, ids: Ids, entity: Entity) -> bool {
    world
        .get(ids.path, entity)
        .is_some_and(|path| path.kind == PathKind::Emotion)
}

pub(super) fn take_route(tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Option<Path> {
    let kind = tick.state().get(ids.path, entity)?.kind;
    if matches!(kind, PathKind::Burning | PathKind::Emotion) {
        return None;
    }
    tick.state_mut().remove(ids.path, entity)
}

fn drop_route(tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
    if tick
        .state()
        .get(ids.path, entity)
        .is_some_and(|path| path.kind != PathKind::Burning)
    {
        tick.state_mut().remove(ids.path, entity);
    }
}

pub(super) fn walk(tick: &mut Tick<'_>, ids: Ids, entity: Entity, to: Vec2) -> Result<()> {
    if tick
        .state()
        .get(ids.path, entity)
        .is_some_and(|path| path.kind == PathKind::Burning)
    {
        return Ok(());
    }
    crate::unit::give_path(
        tick.state_mut(),
        (ids.body, ids.path),
        entity,
        Path {
            route: Route::to_point(to),
            goal: to,
            kind: PathKind::Emotion,
            facing: None,
        },
    )?;
    Ok(())
}

fn snap_face(tick: &mut Tick<'_>, ids: Ids, entity: Entity, towards: Vec2) -> Result<()> {
    let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
        return Ok(());
    };
    let Some(facing) = Facing::from_vector(towards.minus(body.0.position)) else {
        return Ok(());
    };
    if body.0.facing.delta_to(facing).abs() > 0.0 {
        body.0.facing = facing;
        tick.state_mut().insert(ids.body, entity, body)?;
        mark_dirty(tick, ids, entity);
    }
    Ok(())
}

fn mark_dirty(tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
    if let Some(horde) = tick.state_mut().get_mut(ids.horde, entity) {
        horde.dirty = true;
    }
}
