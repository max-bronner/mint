use mint_movement::{Facing, Route, Vec2};
use mint_sage_ini::{EmotionAiState, EmotionType};
use mint_sage_modules::Effect;
use mint_sim::{Entity, LogicFrames, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::unit::{Path, PathKind};

const FACE_TOLERANCE: f32 = 0.035;

const BACK_UP_CELL: f32 = 10.0;

const COWER_STEP_SHARE: f32 = 0.9;

const COWER_RADIUS_STEP: f32 = 5.0;

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

const RECENT_DAMAGE: LogicFrames = LogicFrames::from_milliseconds(4000);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    BackAway,
    Idle,
    Panic,
    Face { turns: u32 },
    Quarrel { pair: Option<(Entity, Entity)> },
}

impl Phase {
    const fn code(self) -> u8 {
        match self {
            Self::BackAway => 0,
            Self::Idle => 1,
            Self::Panic => 2,
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
}

impl EmotionState {
    pub(super) fn holds_route(&self) -> bool {
        self.stash.is_some()
    }

    pub(super) fn unlock(&mut self) {
        self.locked = false;
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.phase.code());
        match self.phase {
            Phase::Face { turns } => hasher.write_u32(turns),
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
            EmotionAiState::RunAwayPanic => (Phase::Panic, true),
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
                    state.backing = self.back_away_members(tick, ids, entity, scarer);
                    mark_dirty(tick, ids, entity);
                } else {
                    snap_face(tick, ids, entity, scarer)?;
                }
            }
            Phase::Panic => {
                self.apply_locomotor_override(tick, ids, entity, Some("PANIC"))?;
                self.push_conditions_for_horde(
                    tick.state(),
                    ids,
                    entity,
                    vec!["PANICKING".to_owned()],
                    Vec::new(),
                    effects,
                );
                if let Some(scarer) = goal_at {
                    let vision = tick
                        .state()
                        .get(ids.kind, entity)
                        .and_then(|kind| self.kinds.get(*kind))
                        .and_then(|entry| entry.record.vision_range)
                        .unwrap_or(0.0);
                    let reach = vision + self.guard_ranges.repulsed_distance + flee_bonus;
                    if let Some(to) = self.away_from(tick, ids, entity, scarer, reach, true) {
                        walk(tick, ids, entity, to)?;
                    }
                }
            }
            Phase::Quarrel { .. } => {
                state.phase = Phase::Quarrel {
                    pair: self.instigate_quarrel(tick, ids, entity),
                };
            }
            Phase::Idle | Phase::Face { .. } => {}
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
                    self.advance_backing(tick, ids, entity, &mut state, goal_at)?;
                    false
                }
            }
            Phase::Idle => false,
            Phase::Panic => !has_own_walk(tick.state(), ids, entity),
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
                                self.turn_towards(tick, ids, entity, target)?;
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
            Phase::Quarrel { .. } => hurt,
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

    fn advance_backing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: &mut EmotionState,
        scarer: Option<Vec2>,
    ) -> Result<()> {
        let cower_radius = self
            .back_away_tuning(tick.state(), ids, entity)
            .cower_radius;
        for entry in &mut state.backing {
            if self.is_gone_member(tick.state(), ids, entry.member) {
                continue;
            }
            if let (Some(scarer), true) = (scarer, cower_radius > 0.0) {
                let Some(at) = tick
                    .state()
                    .get(ids.body, entry.member)
                    .map(|body| body.0.position)
                else {
                    continue;
                };
                let away = at.minus(scarer);
                let distance = away.length();
                if distance < COWER_STEP_SHARE * cower_radius && distance > 0.0 {
                    let step = COWER_RADIUS_STEP / distance;
                    walk(
                        tick,
                        ids,
                        entry.member,
                        Vec2::new(at.x + away.x * step, at.y + away.y * step),
                    )?;
                } else if distance < cower_radius {
                    drop_route(tick, ids, entry.member);
                } else if has_own_walk(tick.state(), ids, entry.member) {
                    drop_route(tick, ids, entry.member);
                    mark_dirty(tick, ids, entity);
                }
                continue;
            }
            if entry.sent {
                if let (Some(scarer), false) =
                    (scarer, has_own_walk(tick.state(), ids, entry.member))
                {
                    snap_face(tick, ids, entry.member, scarer)?;
                }
                continue;
            }
            if entry.delay > 0 {
                entry.delay = entry.delay.saturating_sub(1);
                continue;
            }
            entry.sent = true;
            walk(tick, ids, entry.member, entry.to)?;
        }
        Ok(())
    }

    pub(super) fn backing_members(world: &World, ids: Ids) -> Vec<Entity> {
        world
            .store(ids.emotion)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(_, emotion)| emotion.state.as_ref())
                    .filter(|state| !state.interrupted)
                    .flat_map(|state| {
                        state
                            .backing
                            .iter()
                            .filter(|entry| entry.sent)
                            .map(|entry| entry.member)
                    })
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
            if tuning.cower_radius <= 0.0 && tick.rng().next_f32() > tuning.percentage {
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
            let away = at.minus(scarer);
            let length = away.length();
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

    fn instigate_quarrel(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<(Entity, Entity)> {
        let members: Vec<(Entity, Vec2)> = tick
            .state()
            .get(ids.horde, entity)?
            .members
            .iter()
            .filter_map(|member| Some((*member, tick.state().get(ids.body, *member)?.0.position)))
            .collect();
        let count = u32::try_from(members.len()).ok()?;
        if count < 2 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let n = count as f32;
        let centre = members.iter().fold(Vec2::ZERO, |sum, (_, at)| {
            Vec2::new(sum.x + at.x, sum.y + at.y)
        });
        let centre = Vec2::new(centre.x / n, centre.y / n);
        let nearest = members
            .iter()
            .enumerate()
            .min_by(|(_, (_, a)), (_, (_, b))| {
                a.minus(centre)
                    .length()
                    .total_cmp(&b.minus(centre).length())
            })
            .map(|(index, _)| index)?;
        let offset =
            1usize.saturating_add(usize::try_from(tick.rng().below(count.saturating_sub(1))).ok()?);
        let other = nearest.saturating_add(offset) % members.len();
        Some((members.get(other)?.0, members.get(nearest)?.0))
    }

    fn away_from(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        scarer: Vec2,
        distance: f32,
        from_scarer: bool,
    ) -> Option<Vec2> {
        let from = tick.state().get(ids.body, entity)?.0.position;
        let away = from.minus(scarer);
        let length = away.length();
        if !length.is_finite() || length <= 0.0 || !distance.is_finite() || distance <= 0.0 {
            return None;
        }
        let origin = if from_scarer { scarer } else { from };
        let step = distance / length;
        Some(self.on_map(
            from,
            Vec2::new(origin.x + away.x * step, origin.y + away.y * step),
        ))
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
        self.apply_locomotor_override(tick, ids, entity, None)?;
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
            Phase::Panic => self.push_conditions_for_horde(
                tick.state(),
                ids,
                entity,
                Vec::new(),
                vec!["PANICKING".to_owned()],
                effects,
            ),
            Phase::Idle | Phase::Face { .. } | Phase::Quarrel { .. } => {}
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

fn has_own_walk(world: &World, ids: Ids, entity: Entity) -> bool {
    world
        .get(ids.path, entity)
        .is_some_and(|path| path.kind == PathKind::Emotion)
}

fn take_route(tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Option<Path> {
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

fn walk(tick: &mut Tick<'_>, ids: Ids, entity: Entity, to: Vec2) -> Result<()> {
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
