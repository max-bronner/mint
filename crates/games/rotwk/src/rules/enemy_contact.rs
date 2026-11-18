use mint_movement::{Cell, CellFlags, Facing, GridExtent, Passability, PathRequest};
use mint_sage_passability::OBSTACLE;
use mint_sim::{Component, Entity, Result, StateHasher, Tick};

use super::movement::PathBudgets;
use super::{Ids, RotwkRules};
use crate::combat::{overlaps, Volume};
use crate::goal_cells;
use crate::unit::{Path, PathKind};
use mint_movement::{GoalId, PlanOutcome, Route, Vec2};

const FPS: u64 = crate::game_data::RETAIL_LOGIC_HZ as u64;

const NEAR_GOAL: f32 = 40.0;

const SAME_WAY: f32 = 0.9;

const SPEED_MARGIN: f32 = 0.95;

const WALK_STEP: f32 = 10.0;

const RECENT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Contact {
    blocked: bool,
    frames: u64,
    pending: Option<Entity>,
    recent: Vec<(Entity, u64)>,
    ignore_until: u64,
}

impl Contact {
    pub(crate) fn blocked(&self) -> bool {
        self.frames > 0
    }
}

impl Component for Contact {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(u16::from(self.blocked));
        hasher.write_u64(self.frames);
        hasher.write_u32(self.pending.map_or(u32::MAX, |e| e.index()));
        hasher.write_usize(self.recent.len());
        for (entity, frame) in &self.recent {
            hasher.write_u32(entity.index());
            hasher.write_u64(*frame);
        }
        hasher.write_u64(self.ignore_until);
    }
}

#[derive(Debug, Clone, Copy)]
struct Party {
    entity: Entity,
    at: Vec2,
    facing: Facing,
    volume: Volume,
    goal: Option<Vec2>,
    max_speed: f32,
    priority: u32,
    crusher: u8,
    crushable: i16,
}

fn ahead(a: &Party, b: &Party) -> bool {
    let shared = a.facing.to_vector().plus(b.facing.to_vector());
    shared.dot(b.at.minus(a.at)) < 0.0
}

struct ClearOf<'a> {
    inner: &'a dyn Passability,
    blocked: &'a [Cell],
}

impl Passability for ClearOf<'_> {
    fn extent(&self) -> GridExtent {
        self.inner.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        let flags = self.inner.flags(cell);
        if self.blocked.binary_search(&cell).is_ok() {
            CellFlags::from_bits(flags.bits() | OBSTACLE.bits())
        } else {
            flags
        }
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.inner.extra_cost(cell)
    }

    fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
        self.inner.goal_extra_cost(cell, goal)
    }

    fn tail_clear(&self, cell: Cell, mover: mint_movement::MoverClass) -> bool {
        self.inner.tail_clear(cell, mover)
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }
}

fn put(tick: &mut Tick<'_>, ids: Ids, entity: Entity, contact: Contact) -> Result<()> {
    tick.state_mut().insert(ids.contact, entity, contact)?;
    Ok(())
}

impl RotwkRules {
    pub(super) fn volume_of(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Volume> {
        let geometry = &self.record_of(tick, ids, entity)?.geometry;
        let major = geometry.major_radius.or(geometry.radius)?;
        if major <= 0.0 {
            return None;
        }
        let boxed = geometry
            .shape
            .as_deref()
            .is_some_and(|shape| shape.eq_ignore_ascii_case("BOX"));
        Some(if boxed {
            Volume::Box(major, geometry.minor_radius.unwrap_or(major))
        } else {
            Volume::Circle(major)
        })
    }

    fn parties(&self, tick: &Tick<'_>, ids: Ids) -> Vec<Party> {
        let world = tick.state();
        let Some(store) = world.store(ids.body) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (entity, body) in store.iter() {
            if world.contains(ids.member, entity)
                || world.contains(ids.contained, entity)
                || world.contains(ids.flight, entity)
                || world.contains(ids.doomed, entity)
                || world
                    .get(ids.horde, entity)
                    .is_some_and(|h| h.members.is_empty())
            {
                continue;
            }
            if !world.contains(ids.owner, entity) {
                continue;
            }
            let limits = self.mover_limits(tick, ids, entity);
            if limits.max_speed <= 0.0 || !self.moves_on_ground(tick, ids, entity) {
                continue;
            }
            let Some(volume) = self.volume_of(tick, ids, entity) else {
                continue;
            };
            let (crusher, crushable) = self.record_of(tick, ids, entity).map_or(
                (0, i16::from(mint_sage_modules::UNCRUSHABLE)),
                |record| {
                    (
                        record.crush.crusher_level,
                        i16::from(record.crush.crushable_level),
                    )
                },
            );
            out.push(Party {
                entity,
                at: body.0.position,
                facing: body.0.facing,
                volume,
                goal: world.get(ids.path, entity).map(|path| path.goal),
                max_speed: limits.max_speed,
                priority: self.path_priority(tick, ids, entity),
                crusher,
                crushable,
            });
        }
        out
    }

    pub(super) fn moves_on_ground(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        if self.current_locomotor(tick, ids, entity).is_none() {
            return false;
        }
        let world = tick.state();
        let air = mint_movement::MoverClass::new(mint_sage_passability::BOUNDARY);
        let flier = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.mover)
            == Some(air);
        !flier && !self.flies_itself(world, ids, entity)
    }

    pub(super) fn move_blocked(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        let mover = world
            .get(ids.member, entity)
            .map_or(entity, |member| member.horde);
        world.get(ids.contact, mover).is_some_and(Contact::blocked)
    }

    pub(super) fn run_enemy_contact(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let ticks = (u64::from(mint_sim::SIMULATION_HZ) / FPS).max(1);
        if tick.frame().get() % ticks != 0 {
            return Ok(());
        }
        let now = tick.frame().get() / ticks;
        let states: Vec<(Entity, Contact)> = tick
            .state()
            .store(ids.contact)
            .map(|store| store.iter().map(|(e, c)| (e, c.clone())).collect())
            .unwrap_or_default();
        for (entity, mut contact) in states {
            contact.frames = if contact.blocked {
                contact.frames.saturating_add(1)
            } else {
                0
            };
            contact.blocked = false;
            put(tick, ids, entity, contact)?;
        }

        let parties = self.parties(tick, ids);
        crate::diag::diag!(
            "contact: frame {} {} bodies take part, {} of them hordes",
            tick.frame().get(),
            parties.len(),
            parties
                .iter()
                .filter(|p| tick.state().contains(ids.horde, p.entity))
                .count()
        );
        let mut order: Vec<Party> = parties.clone();
        order.sort_by(|pa, pb| {
            (pa.at.x - pa.volume.reach())
                .total_cmp(&(pb.at.x - pb.volume.reach()))
                .then(pa.entity.index().cmp(&pb.entity.index()))
        });
        let mut pairs: Vec<(Party, Party)> = Vec::new();
        for (i, pa) in order.iter().enumerate() {
            let right = pa.at.x + pa.volume.reach();
            for pb in order.iter().skip(i + 1) {
                if pb.at.x - pb.volume.reach() > right {
                    break;
                }
                if !self.object_hostile(tick.state(), pa.entity, pb.entity) {
                    continue;
                }
                if overlaps((pa.at, pa.facing, pa.volume), (pb.at, pb.facing, pb.volume)) {
                    let (lo, hi) = if pa.entity.index() < pb.entity.index() {
                        (*pa, *pb)
                    } else {
                        (*pb, *pa)
                    };
                    pairs.push((lo, hi));
                }
            }
        }
        pairs.sort_unstable_by_key(|(a, b)| (a.entity.index(), b.entity.index()));
        let mut reseat: Vec<Entity> = Vec::new();
        for (a, b) in pairs {
            self.collide(tick, ids, &a, &b, now, &mut reseat)?;
            self.collide(tick, ids, &b, &a, now, &mut reseat)?;
        }
        reseat.sort_unstable_by_key(|entity| entity.index());
        reseat.dedup();
        for entity in reseat {
            self.reseat(tick, ids, entity, budgets)?;
        }

        let pending: Vec<(Entity, Entity)> = tick
            .state()
            .store(ids.contact)
            .map(|store| {
                store
                    .iter()
                    .filter_map(|(e, c)| c.pending.map(|p| (e, p)))
                    .collect()
            })
            .unwrap_or_default();
        for (entity, blocker) in pending {
            self.respond(tick, ids, &parties, entity, blocker, now, budgets)?;
        }
        Ok(())
    }

    fn collide(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        me: &Party,
        other: &Party,
        now: u64,
        reseat: &mut Vec<Entity>,
    ) -> Result<()> {
        if tick.state().contains(ids.horde, other.entity) && self.narrow_at(other.at) {
            return Ok(());
        }
        let mut contact = tick
            .state()
            .get(ids.contact, me.entity)
            .cloned()
            .unwrap_or_default();
        if contact.ignore_until > now {
            return Ok(());
        }
        if let Some(at) = contact.recent.iter().position(|(e, _)| *e == other.entity) {
            if let Some(entry) = contact.recent.get_mut(at) {
                if entry.1.saturating_add(1) >= now {
                    entry.1 = now;
                    return put(tick, ids, me.entity, contact);
                }
            }
            contact.recent.remove(at);
        }
        let done = |tick: &mut Tick<'_>, contact: Contact| {
            if contact == Contact::default() {
                tick.state_mut().remove(ids.contact, me.entity);
                Ok(())
            } else {
                put(tick, ids, me.entity, contact)
            }
        };
        if i16::from(me.crusher) > other.crushable {
            return done(tick, contact);
        }
        let near_end = |party: &Party| {
            party
                .goal
                .is_some_and(|goal| goal.minus(party.at).length() < NEAR_GOAL)
        };
        let Some(_) = me.goal else {
            if other.goal.is_none() && self.may_reseat(tick, ids, me.entity) {
                reseat.push(me.entity);
            }
            return done(tick, contact);
        };
        if near_end(me) || near_end(other) {
            return done(tick, contact);
        }
        let yields = match other.goal {
            None => me.priority <= other.priority,
            Some(_) if me.facing.to_vector().dot(other.facing.to_vector()) > SAME_WAY => {
                if ahead(me, other) {
                    other.max_speed * SPEED_MARGIN >= me.max_speed
                } else {
                    me.max_speed * SPEED_MARGIN > other.max_speed
                }
            }
            Some(_) => {
                me.priority < other.priority
                    || (me.priority == other.priority && me.entity.index() <= other.entity.index())
            }
        };
        crate::diag::diag!(
            "contact: frame {} entity {} touches {}; {}",
            tick.frame().get(),
            me.entity.index(),
            other.entity.index(),
            if yields { "yields" } else { "keeps its way" }
        );
        if !yields {
            return done(tick, contact);
        }
        contact.blocked = true;
        if contact.frames == 0 {
            contact.frames = 1;
        }
        if Self::moving_and_awaiting_path(tick, ids, other.entity) {
            return done(tick, contact);
        }
        let id = u64::from(me.entity.index());
        if contact.frames % FPS == id % FPS {
            if contact.pending.is_none() {
                contact.pending = Some(other.entity);
            }
            if contact.frames > 2 * FPS + 1 {
                contact.blocked = false;
                contact.frames = 0;
                contact.ignore_until = now + 2 * FPS;
            }
        }
        done(tick, contact)
    }

    fn may_reseat(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        !self.has_kind_bit(world, ids, entity, "LARGE_RECTANGLE_PATHFIND")
            && !self.using_ability(world, ids, entity)
            && self.regular_state_idle(world, ids, entity)
    }

    fn reseat(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let Some(from) = tick.state().get(ids.body, entity).map(|body| body.0) else {
            return Ok(());
        };
        let to = self.adjust_to_possible_destination(tick, ids, entity, from.position);
        if to == from.position
            || Self::emotion_locked(tick.state(), ids, entity)
            || self.move_refused(tick, ids, entity).is_some()
        {
            return Ok(());
        }
        let budget = budgets.budget_for(entity);
        if budget.remaining() == 0 {
            return Ok(());
        }
        let mover = self.mover_class(tick, ids, entity);
        let (Some(start), Some(goal)) = (
            self.passable_cell_for(tick, ids, from.position, mover),
            self.passable_cell_for(tick, ids, to, mover),
        ) else {
            return Ok(());
        };
        let ground = self.dynamic_view(tick, ids);
        let costs = self
            .unit_cost_view(tick, ids, entity, &ground)
            .exempting(&[goal]);
        let view = self.footprint_view(tick, ids, entity, mover, &costs, &ground);
        let request = PathRequest {
            start,
            goal,
            mover,
            goal_id: GoalId(u64::from(entity.index())),
            origin: Some(from.position),
            corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
            departure: self.departure(tick, ids, entity),
            approach: None,
            prune: false,
            final_goal: true,
        };
        let modes = self.modes(tick, ids);
        crate::perf::note_plan_call();
        let _site = crate::perf::sub(crate::perf::sub_row!("path_site_contact"));
        let outcome = self
            .plan_path(modes.config(), &view, &request, budget)
            .map_err(|e| mint_sim::Error::Rules {
                frame: tick.frame().get(),
                reason: format!("a re-seat for entity {}: {e}", entity.index()),
            })?;
        let PlanOutcome::Ready(plan) = outcome else {
            return Ok(());
        };
        let mut route: Route = plan.route;
        if let Some(last) = route.points.last_mut() {
            *last = to;
        }
        crate::diag::diag!(
            "contact: frame {} entity {} re-seats from ({:.0}, {:.0}) to ({:.0}, {:.0})",
            tick.frame().get(),
            entity.index(),
            from.position.x,
            from.position.y,
            to.x,
            to.y
        );
        self.interrupt_emotion_state(tick, ids, entity)?;
        let mut state = from;
        state.waypoint = 0;
        tick.state_mut()
            .insert(ids.body, entity, crate::unit::Body(state))?;
        tick.state_mut().insert(
            ids.path,
            entity,
            Path {
                route,
                goal: to,
                kind: PathKind::Ordered,
                facing: None,
            },
        )?;
        Ok(())
    }

    pub(super) fn adjust_to_possible_destination(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        dest: Vec2,
    ) -> Vec2 {
        let world = tick.state();
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let Some(from) = world.get(ids.body, entity).map(|body| body.0.position) else {
            return dest;
        };
        let Some(start) = geometry.cell_at(from, 0, extent) else {
            return dest;
        };
        let footprint = self.path_footprint(tick, ids, entity);
        let mover = self.mover_class(tick, ids, entity);
        let view = self.dynamic_view(tick, ids);
        let cells = u64::from(extent.width) * u64::from(extent.height);
        let limit = u64::from(self.max_cells_adjust_to_possible_destination).min(cells);
        #[allow(clippy::cast_precision_loss)]
        let reach = ((limit as f32).sqrt() * 0.5 + f32::from(footprint.side()) + 1.0)
            * geometry.cell_size();
        let units = self.unit_cells(tick, ids, entity, None, from, reach);
        let cell_of = |(x, y): (i32, i32)| {
            let cell = Cell::new(u16::try_from(x).ok()?, u16::try_from(y).ok()?);
            extent.contains(cell).then_some(cell)
        };
        let in_zone =
            |at: (i32, i32)| cell_of(at).is_some_and(|cell| self.reachable(mover, start, cell));
        let fits = |at: (i32, i32)| {
            let Some(rect) = goal_cells::rect(extent, at, footprint) else {
                return false;
            };
            let (x0, y0, x1, y1) = rect.span();
            let covered = || (y0..y1).flat_map(move |y| (x0..x1).map(move |x| (x, y)));
            covered().all(|at| cell_of(at).is_some_and(|cell| view.is_passable(cell, mover)))
                && units.destination_clear(covered())
        };
        let goal = goal_cells::anchor(geometry, dest, footprint);
        if in_zone(goal) && fits(goal) {
            return dest;
        }
        let mut out = dest;
        let (mut i, mut j) = goal;
        let mut left = limit;
        let mut delta = 1u64;
        while left > 0 {
            for (turn, (di, dj)) in [(1, 0), (0, 1), (-1, 0), (0, -1)].into_iter().enumerate() {
                for _ in 0..delta {
                    i += di;
                    j += dj;
                    left = left.saturating_sub(1);
                    let possible = cell_of((i, j))
                        .is_some_and(|cell| view.is_passable(cell, mover) && in_zone((i, j)));
                    if possible {
                        out = goal_cells::anchor_point(geometry, (i, j), footprint);
                        if fits((i, j)) {
                            return out;
                        }
                    }
                }
                if turn % 2 == 1 {
                    delta += 1;
                }
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn respond(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        parties: &[Party],
        entity: Entity,
        blocker: Entity,
        now: u64,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let Some(mut contact) = tick.state().get(ids.contact, entity).cloned() else {
            return Ok(());
        };
        let budget = budgets.budget_for(entity);
        if budget.remaining() == 0 {
            return Ok(());
        }
        contact.pending = None;
        let (Some(path), Some(body), Some(me)) = (
            tick.state().get(ids.path, entity).cloned(),
            tick.state().get(ids.body, entity).map(|b| b.0),
            parties.iter().find(|p| p.entity == entity).copied(),
        ) else {
            return put(tick, ids, entity, contact);
        };
        let obstacles: Vec<&Party> = parties
            .iter()
            .filter(|p| p.entity != entity && p.priority >= me.priority)
            .collect();
        let points = &path.route.points;
        let first = usize::try_from(body.waypoint)
            .unwrap_or(usize::MAX)
            .min(points.len());
        let mut from = body.position;
        let mut target: Option<(Vec2, usize)> = None;
        'walk: for (index, point) in points.iter().enumerate().skip(first) {
            let leg = point.minus(from);
            let length = leg.length();
            let dir = leg.normalised().unwrap_or(Vec2::ZERO);
            let mut travelled = WALK_STEP;
            while travelled < length {
                let at = from.plus(dir.scale(travelled));
                if self.collision_free(tick, ids, &me, at, &obstacles) {
                    target = Some((at, index));
                    break 'walk;
                }
                travelled += WALK_STEP;
            }
            from = *point;
        }
        let Some((at, index)) = target else {
            crate::diag::diag!(
                "contact: frame {} entity {} finds no free point on its route",
                tick.frame().get(),
                entity.index()
            );
            return put(tick, ids, entity, contact);
        };
        let mover = self.mover_class(tick, ids, entity);
        let (Some(start), Some(goal)) = (
            self.passable_cell_for(tick, ids, body.position, mover),
            self.passable_cell_for(tick, ids, at, mover),
        ) else {
            return put(tick, ids, entity, contact);
        };
        let widen = match me.volume {
            Volume::Circle(r) => r,
            Volume::Box(_, b) => b,
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let mut blocked: Vec<Cell> = Vec::new();
        for other in &obstacles {
            let reach = other.volume.reach() + widen;
            let lo = geometry.cell_at(other.at.minus(Vec2::new(reach, reach)), 0, extent);
            let hi = geometry.cell_at(other.at.plus(Vec2::new(reach, reach)), 0, extent);
            let (Some(lo), Some(hi)) = (lo, hi) else {
                continue;
            };
            for y in lo.y..=hi.y {
                for x in lo.x..=hi.x {
                    let cell = Cell::new(x, y);
                    let centre = geometry.centre_of(cell);
                    let grown = match other.volume {
                        Volume::Circle(r) => Volume::Circle(r + widen),
                        Volume::Box(a, b) => Volume::Box(a + widen, b + widen),
                    };
                    let point = (centre, Facing::EAST, Volume::Circle(0.0));
                    if overlaps((other.at, other.facing, grown), point)
                        && !overlaps((me.at, me.facing, me.volume), point)
                        && cell != goal
                    {
                        blocked.push(cell);
                    }
                }
            }
        }
        blocked.sort_unstable();
        blocked.dedup();
        let ground = self.dynamic_view(tick, ids);
        let costs = self
            .unit_cost_view(tick, ids, entity, &ground)
            .exempting(&[goal]);
        let blocked_view = ClearOf {
            inner: &costs,
            blocked: &blocked,
        };
        let clear = self.footprint_view(tick, ids, entity, mover, &blocked_view, &ground);
        let request = PathRequest {
            start,
            goal,
            mover,
            goal_id: GoalId(u64::from(entity.index())),
            origin: Some(me.at),
            corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
            departure: self.departure(tick, ids, entity),
            approach: None,
            prune: false,
            final_goal: true,
        };
        let modes = self.modes(tick, ids);
        crate::perf::note_plan_call();
        let _site = crate::perf::sub(crate::perf::sub_row!("path_site_contact"));
        let outcome = self
            .plan_path(modes.config(), &clear, &request, budget)
            .map_err(|e| mint_sim::Error::Rules {
                frame: tick.frame().get(),
                reason: format!("a collision detour for entity {}: {e}", entity.index()),
            })?;
        let PlanOutcome::Ready(plan) = outcome else {
            crate::diag::diag!(
                "contact: frame {} entity {} finds no detour to ({:.0}, {:.0}): {:?}",
                tick.frame().get(),
                entity.index(),
                at.x,
                at.y,
                outcome
            );
            return put(tick, ids, entity, contact);
        };
        let mut route = plan.route;
        if let Some(last) = route.points.last_mut() {
            *last = at;
        }
        route.points.extend(points.iter().skip(index).copied());
        crate::diag::diag!(
            "contact: frame {} entity {} detours round {} to ({:.0}, {:.0})",
            tick.frame().get(),
            entity.index(),
            blocker.index(),
            at.x,
            at.y
        );
        let mut updated = path;
        updated.route = route;
        tick.state_mut().insert(ids.path, entity, updated)?;
        let mut state = body;
        state.waypoint = 0;
        tick.state_mut()
            .insert(ids.body, entity, crate::unit::Body(state))?;
        contact.blocked = false;
        contact.frames = 0;
        if contact.recent.len() >= RECENT {
            if let Some(oldest) = contact
                .recent
                .iter()
                .enumerate()
                .min_by_key(|(_, (_, frame))| *frame)
                .map(|(i, _)| i)
            {
                contact.recent.remove(oldest);
            }
        }
        contact.recent.push((blocker, now));
        put(tick, ids, entity, contact)
    }

    fn collision_free(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        me: &Party,
        at: Vec2,
        obstacles: &[&Party],
    ) -> bool {
        let mover = self.mover_class(tick, ids, me.entity);
        let view = self.dynamic_view(tick, ids);
        let passable = self
            .passability
            .geometry
            .cell_at(at, 0, view.extent())
            .is_some_and(|cell| view.is_passable(cell, mover));
        if !passable {
            return false;
        }
        let mine = (at, me.facing, me.volume);
        let eta = |from: Vec2, speed: f32| at.minus(from).length() / speed.max(f32::EPSILON);
        obstacles.iter().all(|other| {
            if overlaps(mine, (other.at, other.facing, other.volume)) {
                return false;
            }
            match other.goal {
                None => true,
                Some(goal) => {
                    !overlaps(mine, (goal, other.facing, other.volume))
                        || eta(other.at, other.max_speed) >= eta(me.at, me.max_speed)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_touch_only_when_their_sides_meet() {
        let east = Facing::EAST;
        let a = (Vec2::new(0.0, 0.0), east, Volume::Box(30.0, 20.0));
        assert!(overlaps(
            a,
            (Vec2::new(55.0, 0.0), east, Volume::Box(30.0, 20.0))
        ));
        assert!(!overlaps(
            a,
            (Vec2::new(65.0, 0.0), east, Volume::Box(30.0, 20.0))
        ));
        assert!(overlaps(
            a,
            (Vec2::new(0.0, 35.0), east, Volume::Circle(20.0))
        ));
        assert!(!overlaps(
            a,
            (Vec2::new(0.0, 45.0), east, Volume::Circle(20.0))
        ));
    }
}
