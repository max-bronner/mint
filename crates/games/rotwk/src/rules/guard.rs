use super::map_scripts::{TeamId, TeamWard, ATTITUDE_AGGRESSIVE, ATTITUDE_ALERT, ATTITUDE_SLEEP};
use mint_sage_ini::Stance;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardState {
    Inner,
    Idle,
    Outer,
    Return,
    AttackAggressor,
}

impl GuardState {
    const fn index(self) -> u8 {
        match self {
            Self::Inner => 0,
            Self::Idle => 1,
            Self::Outer => 2,
            Self::Return => 3,
            Self::AttackAggressor => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardMode {
    Normal,
    HoldGround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardTarget {
    Point,
    Object(Entity),
    Team(TeamId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Guard {
    pub(crate) state: GuardState,
    pub(crate) mode: GuardMode,
    pub(crate) point: Vec2,
    pub(crate) nemesis: Option<Entity>,
    pub(crate) next_scan: Frame,
    pub(crate) give_up_at: Frame,
    pub(crate) recheck_at: Frame,
    pub(crate) moving: bool,
    pub(crate) scripted: bool,
    pub(crate) area: Option<GuardArea>,
    pub(crate) target: GuardTarget,
    pub(crate) goal: Vec2,
    pub(crate) last_seen: Vec2,
    pub(crate) centre: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GuardArea {
    pub(crate) centre: Vec2,
    pub(crate) radius: f32,
}

impl Guard {
    pub(crate) fn returning(&self) -> bool {
        self.state == GuardState::Return
    }
}

impl Component for Guard {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.state.index());
        hasher.write_u8(u8::from(self.mode == GuardMode::HoldGround));
        hasher.write_f32(self.point.x);
        hasher.write_f32(self.point.y);
        match self.nemesis {
            Some(nemesis) => {
                hasher.write_u8(1);
                nemesis.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        self.next_scan.hash_into(hasher);
        self.give_up_at.hash_into(hasher);
        self.recheck_at.hash_into(hasher);
        hasher.write_bool(self.moving);
        match self.target {
            GuardTarget::Point => {}
            GuardTarget::Object(entity) => {
                hasher.write_u8(1);
                entity.hash_into(hasher);
            }
            GuardTarget::Team(team) => {
                hasher.write_u8(2);
                hasher.write_u32(team.get());
            }
        }
        if self.target != GuardTarget::Point {
            for point in [self.goal, self.last_seen, self.centre] {
                hasher.write_f32(point.x);
                hasher.write_f32(point.y);
            }
        }
        if self.scripted {
            hasher.write_u8(0xFE);
            if let Some(area) = self.area {
                hasher.write_f32(area.radius);
                hasher.write_f32(area.centre.x);
                hasher.write_f32(area.centre.y);
            } else {
                hasher.write_f32(0.0);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LastAttacker {
    pub(crate) source: Entity,
    pub(crate) at: Frame,
    pub(crate) fresh: bool,
}

impl Component for LastAttacker {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.source.hash_into(hasher);
        self.at.hash_into(hasher);
        hasher.write_bool(self.fresh);
    }
}

const RETURN_ARRIVED: f32 = 5.0;
const STRUCTURE_FIRST_RECHECK: LogicFrames = LogicFrames::from_engine_milliseconds(3_000);
const STRUCTURE_RECHECK: LogicFrames = LogicFrames::from_engine_milliseconds(1_000);
const WARD_MOVED_SQUARED: f32 = 400.0;
const MAX_TRANSITIONS: usize = 4;

fn staggered(entity: Entity, now: Frame, rate: LogicFrames) -> Frame {
    let span = rate.get().saturating_add(1);
    now.checked_add(LogicFrames::from_frames(entity.index() % span))
        .unwrap_or(Frame::FOREVER)
}

fn later(now: Frame, by: LogicFrames) -> Frame {
    now.checked_add(by).unwrap_or(Frame::FOREVER)
}

fn distance(a: Vec2, b: Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    (dx * dx + dy * dy).sqrt()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardDecision {
    Pass,
    Leave,
    Step,
}

impl RotwkRules {
    pub(super) fn commanded_owner(world: &World, ids: Ids, entity: Entity) -> Option<PlayerId> {
        if let Some(owner) = world.get(ids.owner, entity) {
            return Some(owner.0);
        }
        let horde = world.get(ids.horde, entity)?;
        horde
            .members
            .iter()
            .find_map(|member| world.get(ids.owner, *member).map(|owner| owner.0))
    }

    pub(super) fn note_attackers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &[DamageEvent],
    ) -> Result<()> {
        let now = tick.frame();
        let frame_ticks = u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        for event in events {
            if event.attacker == event.victim || !tick.state().is_live(event.attacker) {
                continue;
            }
            let struck = Self::commanded_unit(tick.state(), ids, event.victim);
            if let Some(held) = tick.state().get(ids.last_attacker, struck) {
                let recent = now.get() / frame_ticks <= held.at.get() / frame_ticks + 1;
                let overrides = tick
                    .state()
                    .get(ids.kind, event.attacker)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| {
                        entry.record.has_kind("INFANTRY") || entry.record.has_kind("CAVALRY")
                    });
                if recent && !overrides {
                    continue;
                }
            }
            tick.state_mut().insert(
                ids.last_attacker,
                struck,
                LastAttacker {
                    source: event.attacker,
                    at: now,
                    fresh: true,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn guard_displaced(world: &World, ids: Ids, unit: Entity) -> bool {
        world
            .get(ids.attack_order, unit)
            .is_some_and(|order| !order.guard)
            || world.contains(ids.attack_move_order, unit)
            || world.contains(ids.ground_order, unit)
            || world.contains(ids.using_ability, unit)
            || world.contains(ids.contained, unit)
            || world.contains(ids.entering, unit)
            || world
                .get(ids.path, unit)
                .is_some_and(|path| matches!(path.kind, PathKind::Ordered | PathKind::Ability))
            || Self::emotion_holds(world, ids, unit)
            || world.contains(ids.ai_busy, unit)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_guards(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
        grid: &TargetGrid,
        living: &[(Foe, PlayerId)],
        global_max_radius: f32,
        now: Frame,
    ) -> Result<()> {
        let mut units: Vec<Entity> = Vec::new();
        for store in [
            tick.state().store(ids.horde).map(|store| store.entities()),
            tick.state().store(ids.armed).map(|store| store.entities()),
            tick.state().store(ids.guard).map(|store| store.entities()),
        ]
        .into_iter()
        .flatten()
        {
            units.extend_from_slice(store);
        }
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        units.retain(|unit| !self.ai_disabled(tick.state(), ids, *unit, now));
        let scan = GuardScan {
            grid,
            living,
            global_max_radius,
        };
        let decide_perf = crate::perf::sub(crate::perf::sub_row!("decide_guards"));
        let decided: Vec<GuardDecision> = units
            .iter()
            .map(|&unit| self.decide_guard(tick.state(), ids, unit))
            .collect();
        drop(decide_perf);
        for (unit, decision) in units.into_iter().zip(decided) {
            debug_assert_eq!(
                decision,
                self.decide_guard(tick.state(), ids, unit),
                "an earlier guard step changed entity {}'s decision",
                unit.index()
            );
            match decision {
                GuardDecision::Pass => continue,
                GuardDecision::Leave => {
                    self.leave_guard(tick, ids, unit);
                    continue;
                }
                GuardDecision::Step => {}
            }
            let held = tick.state().get(ids.guard, unit).copied();
            let fresh = held.is_none();
            let guard = match held {
                Some(guard) => Some(guard),
                None => self.going_idle(tick, ids, unit, now)?,
            };
            let Some(mut guard) = guard else { continue };
            self.step_guard(tick, ids, unit, &mut guard, fresh, &scan, budgets, now)?;
            tick.state_mut().insert(ids.guard, unit, guard)?;
        }
        Ok(())
    }

    pub(super) fn decide_guard(&self, world: &World, ids: Ids, unit: Entity) -> GuardDecision {
        let guarded = world.contains(ids.guard, unit);
        let eligible = world.is_live(unit)
            && !world.contains(ids.member, unit)
            && !world.contains(ids.doomed, unit)
            && world.contains(ids.body, unit)
            && (self.holds_a_stance(world, ids, unit)
                || world.get(ids.guard, unit).is_some_and(|g| g.scripted));
        if !eligible {
            return if guarded {
                GuardDecision::Leave
            } else {
                GuardDecision::Pass
            };
        }
        if Self::emotion_holds(world, ids, unit) || world.contains(ids.ai_busy, unit) {
            return GuardDecision::Pass;
        }
        if Self::guard_displaced(world, ids, unit) {
            return if guarded {
                GuardDecision::Leave
            } else {
                GuardDecision::Pass
            };
        }
        GuardDecision::Step
    }

    fn going_idle(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        now: Frame,
    ) -> Result<Option<Guard>> {
        let mode = match self.stance_of(tick.state(), ids, unit) {
            Some(Stance::Battle) => GuardMode::Normal,
            Some(Stance::HoldGround | Stance::Porcupine) => GuardMode::HoldGround,
            Some(Stance::HoldGroundMoving) => {
                self.change_stance(tick, ids, unit, Stance::HoldGround)?;
                GuardMode::HoldGround
            }
            Some(Stance::Aggressive | Stance::Uninitialized) | None => return Ok(None),
        };
        Ok(self.guard_here(tick, ids, unit, mode, now))
    }

    pub(super) fn guard_here(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        mode: GuardMode,
        now: Frame,
    ) -> Option<Guard> {
        let point = tick.state().get(ids.body, unit)?.0.position;
        tick.state_mut().remove(ids.horde_target, unit);
        if tick
            .state()
            .get(ids.attack_order, unit)
            .is_some_and(|order| order.guard)
        {
            tick.state_mut().remove(ids.attack_order, unit);
        }
        diag!(
            "guard: entity {} guards ({:.0}, {:.0}) in mode {}",
            unit.index(),
            point.x,
            point.y,
            u8::from(mode == GuardMode::HoldGround),
        );
        Some(Guard {
            state: GuardState::Return,
            mode,
            point,
            nemesis: None,
            next_scan: now,
            give_up_at: Frame::FOREVER,
            recheck_at: Frame::FOREVER,
            moving: false,
            scripted: false,
            area: None,
            target: GuardTarget::Point,
            goal: point,
            last_seen: Vec2::new(0.0, 0.0),
            centre: point,
        })
    }

    pub(super) fn leave_guard(&self, tick: &mut Tick<'_>, ids: Ids, unit: Entity) {
        tick.state_mut().remove(ids.guard, unit);
        if tick
            .state()
            .get(ids.attack_order, unit)
            .is_some_and(|order| order.guard)
        {
            tick.state_mut().remove(ids.attack_order, unit);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn step_guard(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &mut Guard,
        fresh: bool,
        scan: &GuardScan<'_>,
        budgets: &mut PathBudgets,
        now: Frame,
    ) -> Result<()> {
        let mut entering = fresh.then_some(GuardState::Return);
        let frame_ticks = u64::from(SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        if entering.is_none()
            && matches!(guard.state, GuardState::Return | GuardState::Idle)
            && now.get() % frame_ticks == 0
            && self.attacked_me(tick, ids, unit)?
        {
            entering = Some(GuardState::AttackAggressor);
        }
        if entering.is_none() {
            entering = self.update_guard_state(tick, ids, unit, guard, scan, budgets, now)?;
        }
        for _ in 0..MAX_TRANSITIONS {
            let Some(next) = entering else { break };
            if guard.state != next {
                diag!(
                    "guard: entity {} {:?} -> {next:?} at frame {}",
                    unit.index(),
                    guard.state,
                    now.get()
                );
            }
            guard.state = next;
            entering = self.enter_guard_state(tick, ids, unit, guard, now)?;
        }
        Ok(())
    }

    fn enter_guard_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &mut Guard,
        now: Frame,
    ) -> Result<Option<GuardState>> {
        let ranges = self.guard_ranges;
        match guard.state {
            GuardState::Return => {
                self.drop_guard_attack(tick, ids, unit);
                guard.next_scan = staggered(unit, now, ranges.return_scan);
                guard.moving = false;
                let here = self.guard_position(tick, ids, unit);
                let Some(goal) = self.position_to_guard(tick, ids, unit, guard, false) else {
                    return Ok(Some(GuardState::Idle));
                };
                guard.goal = goal;
                if here.is_none_or(|here| distance(here, goal) < RETURN_ARRIVED) {
                    return Ok(Some(GuardState::Idle));
                }
                Ok(None)
            }
            GuardState::Idle => {
                self.drop_guard_attack(tick, ids, unit);
                guard.next_scan = staggered(unit, now, ranges.enemy_scan);
                Ok(None)
            }
            GuardState::Inner => {
                guard.centre = self.entry_centre(tick, ids, guard);
                let Some(nemesis) = guard.nemesis.filter(|n| self.target_is_live(tick, ids, *n))
                else {
                    return Ok(Some(GuardState::Outer));
                };
                guard.recheck_at = later(now, STRUCTURE_FIRST_RECHECK);
                self.guard_attack(tick, ids, unit, nemesis)?;
                Ok(None)
            }
            GuardState::Outer => {
                guard.centre = self.entry_centre(tick, ids, guard);
                let Some(nemesis) = guard.nemesis.filter(|n| self.target_is_live(tick, ids, *n))
                else {
                    return Ok(Some(GuardState::Return));
                };
                if guard.mode == GuardMode::HoldGround {
                    return Ok(Some(GuardState::Return));
                }
                guard.give_up_at = later(now, ranges.chase);
                self.guard_attack(tick, ids, unit, nemesis)?;
                Ok(None)
            }
            GuardState::AttackAggressor => {
                let source = tick
                    .state()
                    .get(ids.last_attacker, unit)
                    .map(|last| last.source);
                if source.is_some() {
                    guard.nemesis = source;
                }
                let Some(nemesis) = guard.nemesis.filter(|n| self.target_is_live(tick, ids, *n))
                else {
                    return Ok(Some(GuardState::Return));
                };
                guard.give_up_at = later(now, ranges.chase);
                self.guard_attack(tick, ids, unit, nemesis)?;
                Ok(None)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_guard_state(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &mut Guard,
        scan: &GuardScan<'_>,
        budgets: &mut PathBudgets,
        now: Frame,
    ) -> Result<Option<GuardState>> {
        let ranges = self.guard_ranges;
        let radius = self
            .guard_radius(tick, ids, unit)
            .max(guard.area.map_or(0.0, |area| area.radius));
        match guard.state {
            GuardState::Return => {
                let Some(here) = self.guard_position(tick, ids, unit) else {
                    return Ok(Some(GuardState::Idle));
                };
                let to_go = distance(here, guard.goal);
                if now >= guard.next_scan && to_go < radius {
                    guard.next_scan = later(now, ranges.return_scan);
                    if let Some(found) = self.look_for_inner_target(tick, ids, unit, guard, scan) {
                        guard.nemesis = Some(found);
                        return Ok(Some(GuardState::Inner));
                    }
                }
                let route_ended = guard.moving && !tick.state().contains(ids.path, unit);
                if to_go < RETURN_ARRIVED || route_ended {
                    if tick.state().contains(ids.horde, unit) {
                        self.end_group_closing(tick, ids, unit);
                    }
                    return Ok(Some(GuardState::Idle));
                }
                self.close_on_target(
                    tick,
                    ids,
                    unit,
                    here,
                    guard.goal,
                    0.0,
                    PathKind::Closing,
                    budgets,
                )?;
                guard.moving = true;
                Ok(None)
            }
            GuardState::Idle => {
                if now < guard.next_scan {
                    return Ok(None);
                }
                guard.next_scan = later(now, ranges.enemy_scan);
                if let Some(found) = self.look_for_inner_target(tick, ids, unit, guard, scan) {
                    guard.nemesis = Some(found);
                    return Ok(Some(GuardState::Inner));
                }
                let Some(ward) = self.live_ward_point(tick, ids, guard) else {
                    return Ok(None);
                };
                let (dx, dy) = (ward.x - guard.last_seen.x, ward.y - guard.last_seen.y);
                if dx * dx > WARD_MOVED_SQUARED || dy * dy > WARD_MOVED_SQUARED {
                    guard.last_seen = ward;
                    return Ok(Some(GuardState::Return));
                }
                Ok(None)
            }
            GuardState::Inner | GuardState::Outer | GuardState::AttackAggressor => {
                if guard.state != GuardState::AttackAggressor {
                    if let Some(ward) = self.live_ward_point(tick, ids, guard) {
                        guard.centre = ward;
                    }
                }
                let victim = tick
                    .state()
                    .get(ids.attack_order, unit)
                    .filter(|order| order.guard)
                    .map(|order| order.target)
                    .filter(|victim| self.target_is_live(tick, ids, *victim));
                if victim.is_some() {
                    guard.nemesis = victim;
                }
                let Some(victim) = victim else {
                    if guard.state == GuardState::AttackAggressor {
                        return Ok(Some(GuardState::Return));
                    }
                    guard.nemesis = self.look_for_inner_target(tick, ids, unit, guard, scan);
                    return Ok(Some(match (guard.state, guard.nemesis) {
                        (GuardState::Inner, _) => GuardState::Inner,
                        (_, Some(_)) => GuardState::Outer,
                        (_, None) => GuardState::Return,
                    }));
                };
                let (victim_at, victim_radius) = self.guard_victim_extent(tick, ids, victim);
                let exit_centre = Self::exit_centre(guard);
                let border = (distance(exit_centre, victim_at) - victim_radius).max(0.0);
                match guard.state {
                    GuardState::Inner => {
                        if border > radius {
                            return Ok(Some(GuardState::Outer));
                        }
                        let structure = tick
                            .state()
                            .get(ids.shape, victim)
                            .is_some_and(|shape| shape.kind_of.contains(KindOf::STRUCTURE));
                        if structure && now >= guard.recheck_at {
                            guard.recheck_at = later(now, STRUCTURE_RECHECK);
                            if let Some(found) =
                                self.look_for_inner_target(tick, ids, unit, guard, scan)
                            {
                                if found != victim {
                                    guard.nemesis = Some(found);
                                    return Ok(Some(GuardState::Inner));
                                }
                            }
                        }
                        Ok(None)
                    }
                    GuardState::Outer => {
                        if distance(exit_centre, victim_at) <= radius {
                            guard.give_up_at = later(now, ranges.chase);
                        }
                        Ok((border > radius || now >= guard.give_up_at)
                            .then_some(GuardState::Return))
                    }
                    _ => Ok((now >= guard.give_up_at).then_some(GuardState::Return)),
                }
            }
        }
    }

    fn attacked_me(&self, tick: &mut Tick<'_>, ids: Ids, unit: Entity) -> Result<bool> {
        let Some(mut last) = tick
            .state()
            .get(ids.last_attacker, unit)
            .copied()
            .filter(|last| last.fresh)
        else {
            return Ok(false);
        };
        last.fresh = false;
        tick.state_mut().insert(ids.last_attacker, unit, last)?;
        let source = last.source;
        if !self.target_is_live(tick, ids, source) {
            return Ok(false);
        }
        let (Some(_), Some(_)) = (
            Self::commanded_owner(tick.state(), ids, unit),
            tick.state().get(ids.owner, source).map(|owner| owner.0),
        ) else {
            return Ok(false);
        };
        if !self.object_hostile(tick.state(), unit, source) {
            return Ok(false);
        }
        let (Some(mask), Some(foe)) = (
            self.guard_attack_mask(tick, ids, unit),
            self.foe_of(tick, ids, source),
        ) else {
            return Ok(false);
        };
        if !target_allowed(mask, &foe) {
            return Ok(false);
        }
        if self.move_refused(tick, ids, unit).is_some() {
            let reach = self.guard_weapon_reach(tick, ids, unit).unwrap_or(0.0);
            let Some(here) = self.guard_position(tick, ids, unit) else {
                return Ok(false);
            };
            return Ok(crate::combat::in_range_volumes(
                (here, Self::mover_radius(tick, ids, unit), None),
                foe.reach_volume(),
                0.0,
                0.0,
                reach,
            ));
        }
        Ok(true)
    }

    fn look_for_inner_target(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &Guard,
        scan: &GuardScan<'_>,
    ) -> Option<Entity> {
        let (found, scanned) = self.inner_target(tick, ids, unit, guard, scan)?;
        Some(if scanned {
            self.horde_member_pick(tick, ids, unit, found)
        } else {
            found
        })
    }

    fn inner_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &Guard,
        scan: &GuardScan<'_>,
    ) -> Option<(Entity, bool)> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("guard_scan"));
        if Self::holds_fire(tick.state(), ids, unit) {
            return None;
        }
        let mask = self.guard_attack_mask(tick, ids, unit)?;
        if let Some(common) = self.common_target(tick.state(), ids, unit, scan.living, false) {
            return Some((common.entity, false));
        }
        let owner = Self::commanded_owner(tick.state(), ids, unit)?;
        let radius = guard
            .area
            .map_or_else(|| self.guard_radius(tick, ids, unit), |area| area.radius);
        let centre = self.position_to_guard(tick, ids, unit, guard, true);
        let stands = self.stands_ground(tick.state(), ids, unit);
        let muster = self.guard_position(tick, ids, unit);
        let unit_radius = Self::mover_radius(tick, ids, unit);
        let structures = tick
            .state()
            .get(ids.kind, unit)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry.record.auto_acquire != mint_sage_modules::AutoAcquire::default()
            });
        let here_reach = muster
            .filter(|_| stands)
            .map(|here| (here, self.reach_from_here(tick, ids, unit)));
        let relation = self.relation_from(tick.state(), unit);
        let high = self.active_high(tick.state(), ids);
        let nearest = |from: Vec2, reach: f32, ranked: bool| {
            let search = TargetGrid::search_radius(reach, 0.0, scan.global_max_radius);
            let mut indices: Vec<u32> = Vec::new();
            scan.grid.query_into(
                grid_cell(from.x),
                grid_cell(from.y),
                grid_radius_cells(search),
                &mut indices,
            );
            let candidates: Vec<Foe> = indices
                .into_iter()
                .filter_map(|at| {
                    #[allow(clippy::cast_possible_truncation)]
                    let (foe, _) = scan.living.get(at as usize)?;
                    if super::targeting::outside(high, foe.position) {
                        return None;
                    }
                    if let Some((here, reach)) = &here_reach {
                        if !reach.attackable(*here, unit_radius, foe, true) {
                            return None;
                        }
                    }
                    let relation = relation.to(foe.entity).victim();
                    if !crate::combat::auto_acquirable(relation) {
                        return None;
                    }
                    mint_sage_combat::can_target(
                        mint_sage_combat::Attacker {
                            relation,
                            class_allowed: target_allowed(mask, foe),
                            scans_structures: structures,
                        },
                        self.scanned_victim(tick.state(), ids, owner, foe),
                        mint_sage_combat::Mode::AutoAcquire,
                    )
                    .then_some(*foe)
                })
                .collect();
            if ranked {
                self.acquire_for(tick.state(), ids, unit, from, 0.0, &candidates, 0.0, reach)
            } else {
                crate::combat::acquire_target(from, 0.0, &candidates, 0.0, reach)
            }
        };
        if tick.state().contains(ids.giant_bird, unit) {
            return if self.computer_controlled(owner) {
                let here = self.guard_position(tick, ids, unit)?;
                nearest(here, radius, true).map(|found| (found, true))
            } else {
                nearest(centre?, radius, false).map(|found| (found, true))
            };
        }
        let centre = centre?;
        if let Some(found) = nearest(centre, radius, true) {
            return Some((found, true));
        }
        let here = self.guard_position(tick, ids, unit)?;
        let half = radius * 0.5;
        (distance(here, centre) > half)
            .then(|| nearest(here, half, true))
            .flatten()
            .map(|found| (found, true))
    }

    pub(super) fn guard_radius(&self, tick: &Tick<'_>, ids: Ids, unit: Entity) -> f32 {
        let vision = tick
            .state()
            .get(ids.kind, unit)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .filter(|vision| vision.is_finite() && *vision > 0.0)
            .unwrap_or(0.0);
        let bonus = tick
            .state()
            .get(ids.bonuses, unit)
            .map_or(0.0, |bonuses| bonuses.0.vision);
        let range = vision * (1.0 + bonus).max(0.0) + Self::mover_radius(tick, ids, unit);
        self.attitude_scaled(tick.state(), ids, unit, range)
    }

    pub(super) fn mood_scan_radius(&self, world: &World, ids: Ids, unit: Entity) -> f32 {
        if !self.has_ai(world, ids, unit) {
            return 0.0;
        }
        let Some(record) = world
            .get(ids.kind, unit)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record)
        else {
            return 0.0;
        };
        let vision = record
            .vision_range
            .filter(|vision| vision.is_finite() && *vision > 0.0)
            .unwrap_or(0.0);
        let bonus = world
            .get(ids.bonuses, unit)
            .map_or(0.0, |bonuses| bonuses.0.vision);
        let height = record
            .vision_bonus
            .as_ref()
            .zip(world.get(ids.body, unit))
            .map_or(0.0, |(bonus, body)| {
                let ground = |x, y| self.passability.heights.height_at(x, y);
                let at = body.0.position;
                crate::combat::ground_rise(bonus, &ground, at.x, at.y)
                    .map_or(0.0, |rise| crate::combat::height_bonus(bonus, rise))
            });
        let radius = world.get(ids.shape, unit).map_or(5.0, |shape| shape.radius);
        let range = crate::combat::vision_reach(vision * (1.0 + bonus).max(0.0), height) + radius;
        self.attitude_scaled(world, ids, unit, range)
    }

    fn attitude_scaled(&self, world: &World, ids: Ids, unit: Entity, range: f32) -> f32 {
        let ranges = self.attack_priorities.ranges;
        match self.mood_attitude(world, ids, unit) {
            ATTITUDE_SLEEP => 0.0,
            ATTITUDE_ALERT => range * ranges.alert,
            ATTITUDE_AGGRESSIVE => range * ranges.aggressive,
            _ => range,
        }
    }

    fn guard_attack_mask(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
    ) -> Option<mint_sage_combat::AntiMask> {
        if tick.state().contains(ids.no_attack, unit) {
            return None;
        }
        let mask_of = |entity: Entity| {
            let armed = tick.state().get(ids.armed, entity)?;
            Some(self.combat.weapons.get(armed.weapon)?.anti_mask)
        };
        let members = tick
            .state()
            .get(ids.horde, unit)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        std::iter::once(unit)
            .chain(members)
            .filter_map(mask_of)
            .reduce(mint_sage_combat::AntiMask::union)
    }

    fn guard_weapon_reach(&self, tick: &Tick<'_>, ids: Ids, unit: Entity) -> Option<f32> {
        if let Some(horde) = tick.state().get(ids.horde, unit).cloned() {
            return self.horde_max_range(tick, ids, &horde);
        }
        let armed = tick.state().get(ids.armed, unit)?;
        let template = self.combat.weapons.get(armed.weapon)?;
        Some(self.attack_range_of(tick.state(), ids, unit, template))
    }

    fn live_ward_point(&self, tick: &Tick<'_>, ids: Ids, guard: &Guard) -> Option<Vec2> {
        match guard.target {
            GuardTarget::Point => None,
            GuardTarget::Object(ward) => self.object_ward(tick, ids, ward),
            GuardTarget::Team(team) => match self.team_ward(tick.state(), ids, team) {
                TeamWard::At(at) => Some(at),
                TeamWard::Gone | TeamWard::Empty => None,
            },
        }
    }

    fn object_ward(&self, tick: &Tick<'_>, ids: Ids, ward: Entity) -> Option<Vec2> {
        if !tick.state().is_live(ward) {
            return None;
        }
        tick.state().get(ids.body, ward).map(|body| body.0.position)
    }

    pub(super) fn position_to_guard(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        unit: Entity,
        guard: &Guard,
        with_area: bool,
    ) -> Option<Vec2> {
        let stored = match guard.target {
            GuardTarget::Point => Some(guard.point),
            GuardTarget::Object(ward) => self.object_ward(tick, ids, ward).or(Some(guard.point)),
            GuardTarget::Team(team) => match self.team_ward(tick.state(), ids, team) {
                TeamWard::At(at) => Some(at),
                TeamWard::Empty => None,
                TeamWard::Gone => Some(guard.point),
            },
        };
        let point = match (with_area.then_some(guard.area).flatten(), stored) {
            (Some(area), Some(_)) => area.centre,
            (_, stored) => stored?,
        };
        if point.x == 0.0 && point.y == 0.0 {
            return self.guard_position(tick, ids, unit);
        }
        Some(point)
    }

    fn entry_centre(&self, tick: &Tick<'_>, ids: Ids, guard: &Guard) -> Vec2 {
        match guard.target {
            GuardTarget::Point => guard.point,
            GuardTarget::Object(ward) => self.object_ward(tick, ids, ward).unwrap_or(guard.point),
            GuardTarget::Team(team) => match self.team_ward(tick.state(), ids, team) {
                TeamWard::At(at) => at,
                TeamWard::Empty => Vec2::new(f32::NAN, f32::NAN),
                TeamWard::Gone => guard.point,
            },
        }
    }

    fn exit_centre(guard: &Guard) -> Vec2 {
        if guard.target == GuardTarget::Point {
            guard.point
        } else {
            guard.centre
        }
    }

    fn guard_position(&self, tick: &Tick<'_>, ids: Ids, unit: Entity) -> Option<Vec2> {
        tick.state().get(ids.body, unit).map(|body| body.0.position)
    }

    fn guard_victim_extent(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> (Vec2, f32) {
        let measured = tick
            .state()
            .get(ids.member, victim)
            .map_or(victim, |member| member.horde);
        let at = tick
            .state()
            .get(ids.body, measured)
            .or_else(|| tick.state().get(ids.body, victim))
            .map_or(Vec2::new(0.0, 0.0), |body| body.0.position);
        let radius = if measured == victim {
            tick.state()
                .get(ids.shape, victim)
                .map_or(0.0, |shape| shape.radius)
        } else {
            Self::mover_radius(tick, ids, measured)
        };
        (at, radius)
    }

    fn guard_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        victim: Entity,
    ) -> Result<()> {
        let horde = if tick.state().contains(ids.horde, unit) {
            None
        } else {
            tick.state()
                .get(ids.member, victim)
                .map(|member| member.horde)
        };
        tick.state_mut().insert(
            ids.attack_order,
            unit,
            AttackOrder {
                target: victim,
                planned: false,
                horde,
                guard: true,
                relation_free: false,
                order_owner: None,
            },
        )?;
        diag!(
            "guard: entity {} attacks entity {} at frame {}",
            unit.index(),
            victim.index(),
            tick.frame().get()
        );
        Ok(())
    }

    fn drop_guard_attack(&self, tick: &mut Tick<'_>, ids: Ids, unit: Entity) {
        if tick
            .state()
            .get(ids.attack_order, unit)
            .is_some_and(|order| order.guard)
        {
            tick.state_mut().remove(ids.attack_order, unit);
            tick.state_mut().remove(ids.horde_target, unit);
            if tick.state().contains(ids.horde, unit) {
                self.end_group_closing(tick, ids, unit);
            }
        }
    }
}

struct GuardScan<'a> {
    grid: &'a TargetGrid,
    living: &'a [(Foe, PlayerId)],
    global_max_radius: f32,
}
