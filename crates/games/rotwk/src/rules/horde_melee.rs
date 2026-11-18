use super::melee_engage::Engage;
use super::*;
use crate::goal_cells::{self, Footprint};
use crate::melee::{
    AmoebaTuning, HordeMelee, IdleChange, MeleeKind, MeleePhase, MemberAttack, MemberMelee, Step,
    SwarmState, Weigh, RIM_SCORE,
};
use mint_movement::MoverId;
use mint_sage_modules::Effect;

const READY_MS: u32 = 3000;

const BASE_READY_GAP: f32 = 100.0;

const WAIT_PATH_MS: u32 = 1400;

const WAIT_PATH_CHECKS: u8 = 6;

const WAIT_START_GAP: f32 = 40.0;

const WAIT_START_GAP_SIEGE: f32 = 60.0;

const READY_HEIGHT: f32 = 20.0;

const ADJACENT_HEIGHT: f32 = 10.0;

const FLEE_LEAD: f32 = 60.0;

const APPROACH_EXIT_SNAP_SQ: f32 = 12.5;

const REFORM_TURN: f32 = core::f32::consts::FRAC_PI_6;

const REPLAN_TICKS: u64 = mint_sim::SIMULATION_HZ as u64;

const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

pub(super) struct Snapshot<'a> {
    pub(super) grid: &'a TargetGrid,
    pub(super) living: &'a [(Foe, PlayerId)],
    pub(super) global_max_radius: f32,
}

pub(super) fn approach_exit_position(at: Vec2, goal: Vec2) -> Vec2 {
    if at.minus(goal).length_squared() < APPROACH_EXIT_SNAP_SQ {
        goal
    } else {
        at
    }
}

impl RotwkRules {
    pub(super) fn melee_rangefinder(&self, horde: &Horde) -> Option<f32> {
        self.melee_hordes
            .get(usize::from(horde.formation))
            .copied()
            .flatten()
    }

    pub(crate) fn is_melee_horde(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.horde, entity)
            .is_some_and(|horde| self.melee_rangefinder(horde).is_some())
    }

    pub(super) fn behaviour_of(
        &self,
        world: &World,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
    ) -> (MeleeKind, mint_sage_ini::AmoebaDef) {
        let own = self
            .formations
            .get(usize::from(horde.formation))
            .and_then(|table| table.settings().melee_behavior.as_ref());
        match self.stance_melee_behavior(world, ids, horde_entity).or(own) {
            Some(def) => (MeleeKind::of(def), def.amoeba.clone().unwrap_or_default()),
            None => (MeleeKind::Swarm, mint_sage_ini::AmoebaDef::default()),
        }
    }

    fn amoeba_tuning(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
    ) -> (AmoebaTuning, Vec<String>) {
        let (_, def) = self.behaviour_of(tick.state(), ids, horde_entity, horde);
        (AmoebaTuning::of(&def), def.idle_model_conditions)
    }

    pub(super) fn after(now: Frame, ms: u32) -> Frame {
        now.checked_add(LogicFrames::from_milliseconds(ms))
            .unwrap_or(now)
    }

    pub(super) fn goal_members(&self, tick: &Tick<'_>, ids: Ids, goal: Entity) -> Vec<Foe> {
        match tick.state().get(ids.horde, goal) {
            Some(theirs) => theirs
                .members
                .iter()
                .filter_map(|member| self.foe_of(tick, ids, *member))
                .collect(),
            None => self.foe_of(tick, ids, goal).into_iter().collect(),
        }
    }

    fn nearest_goal_member(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        goal: Entity,
        at: Vec2,
    ) -> Option<Foe> {
        let Some(theirs) = tick.state().get(ids.horde, goal) else {
            return self.foe_of(tick, ids, goal);
        };
        let mut best: Option<(f32, Entity)> = None;
        for member in &theirs.members {
            let alive = tick
                .state()
                .get(ids.health, *member)
                .is_some_and(|health| !health.0.is_dead())
                && !tick.state().contains(ids.concealed, *member);
            let Some(position) = tick.state().get(ids.body, *member).map(|b| b.0.position) else {
                continue;
            };
            let d = position.minus(at).length_squared();
            if alive && best.is_none_or(|(best_d, _)| d < best_d) {
                best = Some((d, *member));
            }
        }
        self.foe_of(tick, ids, best?.1)
    }

    pub(super) fn nearest_to(foes: &[Foe], at: Vec2) -> Option<Foe> {
        foes.iter()
            .copied()
            .fold(None, |best: Option<(f32, Foe)>, foe| {
                let d = foe.position.minus(at).length_squared();
                match best {
                    Some((best_d, _)) if best_d <= d => best,
                    _ => Some((d, foe)),
                }
            })
            .map(|(_, foe)| foe)
    }

    fn horde_gap(&self, tick: &Tick<'_>, ids: Ids, horde_entity: Entity, foe: &Foe) -> Option<f32> {
        let horde = tick.state().get(ids.horde, horde_entity)?;
        let body = tick.state().get(ids.body, horde_entity)?.0;
        let along = body.facing.to_vector();
        let local = |at: Vec2| {
            let d = at.minus(body.position);
            Vec2::new(d.dot(along), along.x * d.y - along.y * d.x)
        };
        let mut bounds: Option<(Vec2, Vec2)> = None;
        let mut radius = 0.0f32;
        for member in &horde.members {
            let Some(at) = tick.state().get(ids.body, *member).map(|b| b.0.position) else {
                continue;
            };
            radius = radius.max(
                tick.state()
                    .get(ids.shape, *member)
                    .map_or(0.0, |shape| shape.radius),
            );
            let p = local(at);
            bounds = Some(bounds.map_or((p, p), |(lo, hi)| {
                (
                    Vec2::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Vec2::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            }));
        }
        let (lo, hi) = bounds?;
        let p = local(foe.position);
        let dx = (lo.x - radius - p.x).max(p.x - hi.x - radius).max(0.0);
        let dy = (lo.y - radius - p.y).max(p.y - hi.y - radius).max(0.0);
        Some(((dx * dx + dy * dy).sqrt() - foe.radius).max(0.0))
    }

    pub(super) fn object_radius(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> f32 {
        let world = tick.state();
        if let Some(shape) = world.get(ids.shape, entity) {
            return shape.radius;
        }
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.geometry.major_radius)
            .unwrap_or_else(|| Self::mover_radius(tick, ids, entity))
    }

    fn border_gap(&self, tick: &Tick<'_>, ids: Ids, from: Entity, target: Entity) -> Option<f32> {
        let world = tick.state();
        let mine = world.get(ids.body, from)?.0.position;
        let theirs = world.get(ids.body, target)?.0;
        let own = self.object_radius(tick, ids, from);
        let offset = mine.minus(theirs.position);
        let box_radii = Self::geometry_kind(world, ids, target)
            .and_then(|kind| self.kinds.get(kind))
            .map(|entry| &entry.record.geometry)
            .filter(|geometry| {
                geometry
                    .shape
                    .as_deref()
                    .is_some_and(|shape| shape.eq_ignore_ascii_case("BOX"))
            })
            .and_then(|geometry| geometry.major_radius.zip(geometry.minor_radius));
        let Some((major, minor)) = box_radii else {
            return Some(offset.length() - own - self.object_radius(tick, ids, target));
        };
        let along = theirs.facing.to_vector();
        let local = Vec2::new(offset.dot(along), along.x * offset.y - along.y * offset.x);
        let outside = Vec2::new(
            (local.x.abs() - major.abs()).max(0.0),
            (local.y.abs() - minor.abs()).max(0.0),
        );
        Some(outside.length() - own)
    }

    fn member_contact(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        goal: Entity,
        now: Frame,
    ) -> bool {
        let world = tick.state();
        let body_of = |entity: Entity| -> Option<(Vec2, f32)> {
            if world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
            {
                return None;
            }
            let at = world.get(ids.body, entity)?.0.position;
            Some((
                at,
                world
                    .get(ids.shape, entity)
                    .map_or(0.0, |shape| shape.radius),
            ))
        };
        let members_of = |of: Entity| -> Vec<Entity> {
            world
                .get(ids.horde, of)
                .map_or_else(|| vec![of], |held| held.members.clone())
        };
        let target_horde = world
            .get(ids.member, goal)
            .map_or(goal, |member| member.horde);
        let mut touched: Vec<(Vec2, f32)> = members_of(target_horde)
            .into_iter()
            .chain(std::iter::once(goal))
            .filter_map(body_of)
            .collect();
        let owner = horde.members.iter().find_map(|member| {
            world
                .get(ids.owner, *member)
                .map(|owner| (*member, owner.0))
        });
        if let (Some((first, _)), Some(store)) = (owner, world.store(ids.horde_melee)) {
            let relation = self.relation_from(world, first);
            for (other, theirs) in store.iter() {
                if theirs.goal != goal || theirs.ready_until.is_none_or(|until| now >= until) {
                    continue;
                }
                let allied = other != horde_entity
                    && world.get(ids.horde, other).is_some_and(|held| {
                        held.members.iter().any(|member| {
                            world.get(ids.owner, *member).is_some()
                                && relation.to(*member) != super::map_scripts::Relation::Enemies
                        })
                    });
                let fighting = members_of(other)
                    .iter()
                    .any(|member| world.contains(ids.member_attack, *member));
                if allied && fighting {
                    touched.extend(members_of(other).into_iter().filter_map(body_of));
                }
            }
        }
        horde
            .members
            .iter()
            .filter_map(|member| body_of(*member))
            .any(|(at, radius)| {
                touched
                    .iter()
                    .any(|(them, theirs)| at.minus(*them).length() <= radius + theirs)
            })
    }

    pub(super) fn fleeing(&self, tick: &Tick<'_>, ids: Ids, from: Vec2, goal: Entity) -> bool {
        let Some(body) = tick.state().get(ids.body, goal).map(|b| b.0) else {
            return false;
        };
        let away = body.facing.to_vector().dot(body.position.minus(from)) >= 0.0;
        away && body.speed > 0.25 * self.mover_limits(tick, ids, goal).max_speed
    }

    fn pace_to_fleeing(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let (Some(mine), Some(theirs)) = (
            world.get(ids.body, horde_entity).map(|b| b.0),
            world.get(ids.body, goal).map(|b| b.0),
        ) else {
            return Ok(());
        };
        let gap = mine.position.minus(theirs.position).length()
            - self.object_radius(tick, ids, horde_entity)
            - self.object_radius(tick, ids, goal);
        #[allow(clippy::cast_precision_loss)]
        let per_second = crate::game_data::RETAIL_LOGIC_HZ as f32;
        let desired = (gap * per_second + theirs.speed).max(0.0);
        crate::diag::diag!(
            "melee: horde {} paces to fleeing {} at {desired:.1} (gap {gap:.1})",
            horde_entity.index(),
            goal.index()
        );
        tick.state_mut().insert(
            ids.desired_speed,
            horde_entity,
            crate::unit::DesiredSpeed(desired),
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn melee_ready(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        melee: &mut HordeMelee,
        victim: &Foe,
        range: f32,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<bool> {
        if melee.ready_until.is_some_and(|until| now < until) {
            return Ok(true);
        }
        if self.member_contact(tick, ids, horde_entity, horde, melee.goal, now) {
            melee.ready_until = Some(Self::after(now, READY_MS));
            return Ok(true);
        }
        let amoeba = melee.kind == MeleeKind::Amoeba;
        if amoeba {
            let z = Self::feet(tick.state(), ids, horde_entity);
            if !victim.is_structure && (z - victim.z).abs() > READY_HEIGHT {
                return Ok(false);
            }
        }
        let reach = if amoeba {
            (range - 10.0).max(0.1)
        } else {
            BASE_READY_GAP
        };
        let close = self
            .border_gap(tick, ids, horde_entity, melee.goal)
            .is_some_and(|gap| gap < reach);
        let mut ready = false;
        for member in &horde.members {
            if let Some(attack) = tick.state().get(ids.member_attack, *member) {
                let theirs = tick
                    .state()
                    .get(ids.member, attack.victim)
                    .map(|entry| entry.horde);
                if close || attack.victim == melee.goal || theirs == Some(melee.goal) {
                    ready = true;
                }
            } else if close {
                if tick
                    .state()
                    .get(ids.path, *member)
                    .is_some_and(|path| path.kind != PathKind::Burning)
                {
                    if amoeba {
                        return Ok(false);
                    }
                    continue;
                }
                if amoeba {
                    ready = true;
                } else if let Some(foe) = self.foe_of(tick, ids, *member) {
                    ready |= self.touching_attack(tick, ids, &foe, melee.goal, snapshot)?;
                }
            }
        }
        if ready {
            melee.ready_until = Some(Self::after(now, READY_MS));
        }
        Ok(ready)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_horde_melee(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
        holds: bool,
        budgets: &mut PathBudgets,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<()> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("horde_melee"));
        let existing = tick.state().get(ids.horde_melee, horde_entity).copied();
        if existing.is_some_and(|melee| melee.goal != goal) {
            self.end_melee(tick, ids, horde_entity)?;
        }
        let fresh = existing.is_none_or(|melee| melee.goal != goal);
        let stands = self.stands_ground(tick.state(), ids, horde_entity);
        let due = crate::horde::decision_stagger(Self::tick_seconds())
            .is_due(MoverId(horde_entity.index()), now.get());
        if !due && !fresh {
            return Ok(());
        }
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let Some(range) = self.melee_rangefinder(&horde) else {
            return Ok(());
        };
        let Some(body) = tick.state().get(ids.body, horde_entity).map(|b| b.0) else {
            return Ok(());
        };
        let (kind, _) = self.behaviour_of(tick.state(), ids, horde_entity, &horde);
        let mut melee = match existing {
            Some(melee) if melee.goal == goal => melee,
            _ => HordeMelee::new(goal, kind),
        };
        if melee.kind != kind {
            self.switch_behaviour(tick, ids, horde_entity, &horde, &mut melee, kind)?;
        }
        let Some(victim) = self.nearest_goal_member(tick, ids, goal, body.position) else {
            tick.state_mut()
                .insert(ids.horde_melee, horde_entity, melee)?;
            return Ok(());
        };
        let fleeing = self.fleeing(tick, ids, body.position, goal);
        if melee.phase == MeleePhase::Approach && fleeing {
            self.pace_to_fleeing(tick, ids, horde_entity, goal)?;
        }
        if melee.phase == MeleePhase::Approach {
            if !fleeing
                && self.melee_ready(
                    tick,
                    ids,
                    horde_entity,
                    &horde,
                    &mut melee,
                    &victim,
                    range,
                    snapshot,
                    now,
                )?
            {
                self.leave_approach(tick, ids, horde_entity)?;
                self.enter_wait(
                    tick,
                    ids,
                    horde_entity,
                    &horde,
                    &mut melee,
                    &victim,
                    range,
                    snapshot,
                    now,
                    stands,
                )?;
            } else if holds || stands {
                self.leave_approach(tick, ids, horde_entity)?;
            } else if self.approach_replan_due(tick, ids, horde_entity, goal, &mut melee, now) {
                let (to, reach) = match tick.state().get(ids.body, goal).map(|b| b.0) {
                    Some(theirs) if fleeing => {
                        let lead = 2.0 * victim.radius + FLEE_LEAD;
                        (
                            theirs.position.plus(theirs.facing.to_vector().scale(lead)),
                            0.0,
                        )
                    }
                    Some(theirs) => self
                        .melee_destination(
                            tick,
                            ids,
                            horde_entity,
                            body.position,
                            goal,
                            theirs.position,
                        )
                        .map_or((theirs.position, victim.radius), |spot| (spot, 0.0)),
                    None => (victim.position, victim.radius),
                };
                self.close_on_target(
                    tick,
                    ids,
                    horde_entity,
                    body.position,
                    to,
                    reach,
                    PathKind::Closing,
                    budgets,
                )?;
            }
        } else if let MeleePhase::Wait { .. } = melee.phase {
            if fleeing {
                self.leave_wait(tick, ids, horde_entity, &mut melee, now, stands)?;
            } else if self.melee_ready(
                tick,
                ids,
                horde_entity,
                &horde,
                &mut melee,
                &victim,
                range,
                snapshot,
                now,
            )? {
                self.start_melee_attack(tick, ids, horde_entity, &mut melee)?;
                melee.phase = MeleePhase::Wait {
                    until: Self::after(now, READY_MS),
                };
            } else if let MeleePhase::Wait { until } = melee.phase {
                if now >= until {
                    self.leave_wait(tick, ids, horde_entity, &mut melee, now, stands)?;
                }
            }
        } else if let MeleePhase::WaitPath { until, .. } = melee.phase {
            if now >= until {
                let route = self.route_exists(tick, ids, horde_entity, goal);
                match Self::wait_path_step(melee.phase, now, route) {
                    Some(phase) => melee.phase = phase,
                    None => return self.abandon_attack(tick, ids, horde_entity, goal),
                }
            }
        }
        if melee.victim.is_some() {
            self.run_behaviour(tick, ids, horde_entity, &mut melee, &victim, snapshot, now)?;
        }
        if melee.phase != MeleePhase::Approach {
            tick.state_mut().remove(ids.desired_speed, horde_entity);
        }
        if crate::diag::wants("melee") {
            self.diag_melee(tick, ids, horde_entity, &melee);
        }
        tick.state_mut()
            .insert(ids.horde_melee, horde_entity, melee)?;
        Ok(())
    }

    fn leave_approach(&self, tick: &mut Tick<'_>, ids: Ids, horde_entity: Entity) -> Result<()> {
        let goal = tick
            .state()
            .get(ids.path, horde_entity)
            .filter(|path| path.kind == PathKind::Closing)
            .map(|path| path.goal);
        if let (Some(goal), Some(mut body)) =
            (goal, tick.state().get(ids.body, horde_entity).copied())
        {
            let placed = approach_exit_position(body.0.position, goal);
            if placed != body.0.position {
                body.0.position = placed;
                tick.state_mut().insert(ids.body, horde_entity, body)?;
            }
        }
        self.end_group_closing(tick, ids, horde_entity);
        Ok(())
    }

    fn approach_replan_due(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
        melee: &mut HordeMelee,
        now: Frame,
    ) -> bool {
        let world = tick.state();
        let (Some(from), Some(victim_at)) = (
            world.get(ids.body, horde_entity).map(|b| b.0.position),
            world.get(ids.body, goal).map(|b| b.0.position),
        ) else {
            return true;
        };
        let routed = world.contains(ids.path, horde_entity);
        let due = match melee.planned {
            Some((seen, when)) if routed => {
                now.get().saturating_sub(when.get()) >= REPLAN_TICKS
                    && super::movement::goal_moved(from, seen, victim_at)
            }
            _ => true,
        };
        if due {
            melee.planned = Some((victim_at, now));
        }
        due
    }

    fn diag_melee(&self, tick: &Tick<'_>, ids: Ids, horde_entity: Entity, melee: &HordeMelee) {
        let (Some(horde), Some(centre)) = (
            tick.state().get(ids.horde, horde_entity),
            tick.state()
                .get(ids.body, horde_entity)
                .map(|b| b.0.position),
        ) else {
            return;
        };
        let fighting = horde
            .members
            .iter()
            .filter(|member| tick.state().contains(ids.member_attack, **member))
            .count();
        let farthest = horde
            .members
            .iter()
            .filter_map(|member| tick.state().get(ids.body, *member))
            .map(|body| body.0.position.minus(centre).length())
            .fold(0.0f32, f32::max);
        let states = |wanted: SwarmState| {
            horde
                .members
                .iter()
                .filter_map(|member| tick.state().get(ids.member_melee, *member))
                .filter(|record| record.state == wanted)
                .count()
        };
        crate::diag::diag!(
            "melee: frame {} horde {} goal {} {:?} {:?} moving {} rotating {} arrived {} members {} fighting {} farthest {:.1}",
            tick.frame().get(),
            horde_entity.index(),
            melee.goal.index(),
            melee.kind,
            melee.phase,
            states(SwarmState::Moving),
            states(SwarmState::Rotating),
            states(SwarmState::Arrived),
            horde.members.len(),
            fighting,
            farthest,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn enter_wait(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        melee: &mut HordeMelee,
        victim: &Foe,
        range: f32,
        snapshot: &Snapshot<'_>,
        now: Frame,
        stands: bool,
    ) -> Result<()> {
        if !self.melee_ready(
            tick,
            ids,
            horde_entity,
            horde,
            melee,
            victim,
            range,
            snapshot,
            now,
        )? {
            let siege = self.is_siege_tower(tick, ids, victim.entity);
            let start = if siege {
                WAIT_START_GAP_SIEGE
            } else {
                WAIT_START_GAP
            };
            if self
                .horde_gap(tick, ids, horde_entity, victim)
                .is_some_and(|gap| gap < start)
            {
                melee.ready_until = Some(Self::after(now, READY_MS));
            } else {
                melee.phase = Self::wait_path(now, stands);
                return Ok(());
            }
        }
        melee.phase = MeleePhase::Wait {
            until: Self::after(now, READY_MS),
        };
        self.start_melee_attack(tick, ids, horde_entity, melee)
    }

    pub(super) fn wait_path(now: Frame, stands: bool) -> MeleePhase {
        MeleePhase::WaitPath {
            until: if stands {
                Self::after(now, WAIT_PATH_MS)
            } else {
                now
            },
            checks: 0,
        }
    }

    pub(super) fn wait_path_step(phase: MeleePhase, now: Frame, route: bool) -> Option<MeleePhase> {
        let MeleePhase::WaitPath { checks, .. } = phase else {
            return Some(phase);
        };
        if route {
            return Some(MeleePhase::Approach);
        }
        let checks = checks.saturating_add(1);
        (checks < WAIT_PATH_CHECKS).then(|| MeleePhase::WaitPath {
            until: Self::after(now, WAIT_PATH_MS),
            checks,
        })
    }

    fn route_exists(&self, tick: &Tick<'_>, ids: Ids, horde_entity: Entity, goal: Entity) -> bool {
        let world = tick.state();
        let (Some(from), Some(to)) = (
            world.get(ids.body, horde_entity).map(|b| b.0.position),
            world.get(ids.body, goal).map(|b| b.0.position),
        ) else {
            return false;
        };
        let mover = self.mover_class(tick, ids, horde_entity);
        let reach = if self.has_kind_bit(world, ids, goal, "STRUCTURE") {
            world.get(ids.shape, goal).map_or(0.0, |shape| shape.radius)
        } else {
            0.0
        };
        let (Some(start), Some(end)) = (
            self.passable_cell_for(tick, ids, from, mover),
            self.passable_cell_near(tick, ids, to, reach, mover),
        ) else {
            return false;
        };
        self.reachable(mover, start, end)
    }

    fn abandon_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
    ) -> Result<()> {
        crate::diag::diag!(
            "melee: horde {} finds no route to {} after {WAIT_PATH_CHECKS} checks; attack ends",
            horde_entity.index(),
            goal.index()
        );
        tick.state_mut().remove(ids.attack_order, horde_entity);
        tick.state_mut().remove(ids.horde_target, horde_entity);
        self.end_group_closing(tick, ids, horde_entity);
        self.end_melee(tick, ids, horde_entity)
    }

    fn start_melee_attack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
    ) -> Result<()> {
        if melee.victim == Some(melee.goal) {
            return Ok(());
        }
        melee.victim = Some(melee.goal);
        melee.force = true;
        if self.face_goal(tick, ids, horde_entity, melee.goal)? > REFORM_TURN {
            self.reform_horde(tick, ids, horde_entity)?;
        }
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let (tuning, _) = self.amoeba_tuning(tick, ids, horde_entity, &horde);
        for member in &horde.members {
            if tick.state().contains(ids.path, *member) {
                continue;
            }
            let Some(at) = tick.state().get(ids.body, *member).map(|b| b.0.position) else {
                continue;
            };
            let face = tick
                .state()
                .get(ids.member_melee, *member)
                .map_or(at, |record| record.face);
            let mut record = MemberMelee::at(at, tuning.delay_until_idle);
            record.face = face;
            tick.state_mut().insert(ids.member_melee, *member, record)?;
        }
        Ok(())
    }

    fn face_goal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
    ) -> Result<f32> {
        let Some(towards) = tick.state().get(ids.body, goal).map(|b| b.0.position) else {
            return Ok(0.0);
        };
        let Some(mut body) = tick.state().get(ids.body, horde_entity).copied() else {
            return Ok(0.0);
        };
        let Some(facing) = Facing::from_vector(towards.minus(body.0.position)) else {
            return Ok(0.0);
        };
        let turn = body.0.facing.delta_to(facing).abs();
        if turn > 0.0 {
            body.0.facing = facing;
            tick.state_mut().insert(ids.body, horde_entity, body)?;
        }
        Ok(turn)
    }

    pub(super) fn end_melee(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> Result<()> {
        tick.state_mut().remove(ids.desired_speed, horde_entity);
        if tick
            .state_mut()
            .remove(ids.horde_melee, horde_entity)
            .is_none()
        {
            return Ok(());
        }
        let Some(mut horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let (tuning, conditions) = self.amoeba_tuning(tick, ids, horde_entity, &horde);
        let mut effects = Vec::new();
        for member in &horde.members {
            if let Some(mut record) = tick.state().get(ids.member_melee, *member).copied() {
                if record.stop_idle(tuning.delay_until_idle) {
                    effects.push(idle_effect(*member, &conditions, false));
                }
                tick.state_mut().remove(ids.member_melee, *member);
            }
        }
        horde.dirty = true;
        tick.state_mut().insert(ids.horde, horde_entity, horde)?;
        self.apply_effects(tick, ids, effects)
    }

    fn leave_wait(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
        now: Frame,
        stands: bool,
    ) -> Result<()> {
        melee.victim = None;
        melee.ready_until = None;
        melee.force = false;
        melee.phase = Self::wait_path(now, stands);
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let (tuning, conditions) = self.amoeba_tuning(tick, ids, horde_entity, &horde);
        let mut effects = Vec::new();
        for member in &horde.members {
            if let Some(mut record) = tick.state().get(ids.member_melee, *member).copied() {
                if record.stop_idle(tuning.delay_until_idle) {
                    effects.push(idle_effect(*member, &conditions, false));
                }
                tick.state_mut().remove(ids.member_melee, *member);
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn footprint_of(&self, tick: &Tick<'_>, ids: Ids, member: Entity) -> Footprint {
        goal_cells::footprint_of(
            tick.state(),
            Self::horde_ids(ids),
            &self.kinds,
            member,
            self.passability.geometry,
        )
    }

    fn adjacent_enemy(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        member: &Foe,
        only: Option<&[Foe]>,
        snapshot: &Snapshot<'_>,
    ) -> Option<Foe> {
        let armed = tick.state().get(ids.armed, member.entity)?;
        let template = self.combat.weapons.get(armed.weapon).filter(|t| t.melee)?;
        let range = self.attack_range_of(tick.state(), ids, member.entity, template);
        let reaches = |foe: &Foe| {
            (foe.z - member.z).abs() <= ADJACENT_HEIGHT
                && !foe.unattackable
                && self.melee_in_reach(tick, ids, member, foe, range)
        };
        let gap = |foe: &Foe| foe.position.minus(member.position).length() - foe.radius;
        let best = |foes: &mut dyn Iterator<Item = Foe>| {
            foes.filter(reaches)
                .fold(None, |best: Option<(f32, Foe)>, foe| {
                    let d = gap(&foe);
                    match best {
                        Some((best_d, _)) if best_d <= d => best,
                        _ => Some((d, foe)),
                    }
                })
        };
        if let Some(only) = only {
            return best(&mut only.iter().copied()).map(|(_, foe)| foe);
        }
        tick.state().get(ids.owner, member.entity)?;
        let mut found: Vec<u32> = Vec::new();
        snapshot.grid.query_into(
            grid_cell(member.position.x),
            grid_cell(member.position.y),
            grid_radius_cells(TargetGrid::search_radius(
                range + 2.0 * self.passability.geometry.cell_size(),
                member.radius,
                snapshot.global_max_radius,
            )),
            &mut found,
        );
        found.sort_unstable();
        let mut candidates = found.into_iter().filter_map(|at| {
            #[allow(clippy::cast_possible_truncation)]
            let (foe, _) = snapshot.living.get(at as usize).copied()?;
            (self.object_hostile(tick.state(), member.entity, foe.entity)
                && tick.state().get(ids.horde, foe.entity).is_none())
            .then_some(foe)
        });
        best(&mut candidates).map(|(_, foe)| foe)
    }

    pub(super) fn attack_unit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        victim: Entity,
    ) -> Result<()> {
        tick.state_mut()
            .insert(ids.member_attack, member.entity, MemberAttack::on(victim))?;
        Ok(())
    }

    pub(super) fn claim_spot(&self, tick: &mut Tick<'_>, ids: Ids, member: Entity, spot: Vec2) {
        let footprint = self.footprint_of(tick, ids, member);
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let cell = goal_cells::anchor(geometry, spot, footprint);
        if let Some(rect) = goal_cells::rect(extent, cell, footprint) {
            if let Some(layer) = goal_cells::layer_mut(tick, ids.goal_cells) {
                layer.claim(member, rect);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_behaviour(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
        victim: &Foe,
        snapshot: &Snapshot<'_>,
        now: Frame,
    ) -> Result<()> {
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        self.face_goal(tick, ids, horde_entity, melee.goal)?;
        match melee.kind {
            MeleeKind::Amoeba => {
                self.recentre(tick, ids, horde_entity, &horde)?;
                let mut effects = Vec::new();
                self.amoeba(
                    tick,
                    ids,
                    horde_entity,
                    melee,
                    victim,
                    snapshot,
                    &mut effects,
                )?;
                self.apply_effects(tick, ids, effects)
            }
            MeleeKind::Swarm => self.swarm(tick, ids, &horde.members.clone(), melee, snapshot, now),
            MeleeKind::HoldGround => {
                self.hold_ground(tick, ids, horde_entity, &horde, melee, snapshot)
            }
            MeleeKind::Formation => self.formation(tick, ids, &horde, melee, snapshot, now),
        }
    }

    fn switch_behaviour(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        melee: &mut HordeMelee,
        kind: MeleeKind,
    ) -> Result<()> {
        let (tuning, conditions) = self.amoeba_tuning(tick, ids, horde_entity, horde);
        let mut effects = Vec::new();
        for member in &horde.members {
            if let Some(mut record) = tick.state().get(ids.member_melee, *member).copied() {
                if record.stop_idle(tuning.delay_until_idle) {
                    effects.push(idle_effect(*member, &conditions, false));
                }
                tick.state_mut().remove(ids.member_melee, *member);
            }
        }
        melee.kind = kind;
        melee.victim = None;
        self.apply_effects(tick, ids, effects)
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn amoeba(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
        victim: &Foe,
        snapshot: &Snapshot<'_>,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("amoeba"));
        let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
            return Ok(());
        };
        let Some(centre) = tick
            .state()
            .get(ids.body, horde_entity)
            .map(|b| b.0.position)
        else {
            return Ok(());
        };
        let (tuning, conditions) = self.amoeba_tuning(tick, ids, horde_entity, &horde);
        let enemies = self.goal_members(tick, ids, melee.goal);
        let mut candidates: Vec<(f32, Foe)> = Vec::new();
        for member in &horde.members {
            if tick.state().contains(ids.member_attack, *member) {
                continue;
            }
            let Some(foe) = self.foe_of(tick, ids, *member) else {
                continue;
            };
            if tick.state().contains(ids.path, *member) {
                if let Some(mut record) = tick.state().get(ids.member_melee, *member).copied() {
                    record.forget();
                    tick.state_mut().insert(ids.member_melee, *member, record)?;
                }
                continue;
            }
            let gap = (foe.position.minus(victim.position).length() - foe.radius - victim.radius)
                .max(0.0);
            candidates.push((gap, foe));
        }
        candidates.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then(a.1.entity.index().cmp(&b.1.entity.index()))
        });
        let force = melee.force;
        melee.force = false;
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        for (_, member) in candidates {
            let mut record = tick
                .state()
                .get(ids.member_melee, member.entity)
                .copied()
                .unwrap_or_else(|| MemberMelee::at(member.position, tuning.delay_until_idle));
            if let Some(nearest) = Self::nearest_to(&enemies, member.position) {
                record.face = nearest.position;
            }
            if let Some(enemy) = self.adjacent_enemy(tick, ids, &member, Some(&enemies), snapshot) {
                if record.stop_idle(tuning.delay_until_idle) {
                    effects.push(idle_effect(member.entity, &conditions, false));
                }
                record.forget();
                record.goal = member.position;
                tick.state_mut()
                    .insert(ids.member_melee, member.entity, record)?;
                self.attack_unit(tick, ids, &member, enemy.entity)?;
                continue;
            }
            if force && record.stop_idle(tuning.delay_until_idle) {
                effects.push(idle_effect(member.entity, &conditions, false));
            }
            let activate = if !record.idle && record.timer == 0 {
                let spread = tuning.activate_max.saturating_sub(tuning.activate_min);
                tuning
                    .activate_min
                    .saturating_add(tick.rng().below(spread.saturating_add(1)))
            } else {
                0
            };
            match record.tick_idle(tuning.delay_until_idle, activate) {
                IdleChange::Start => effects.push(idle_effect(member.entity, &conditions, true)),
                IdleChange::Stop => effects.push(idle_effect(member.entity, &conditions, false)),
                IdleChange::None => {}
            }
            let footprint = self.footprint_of(tick, ids, member.entity);
            let here = goal_cells::anchor(geometry, member.position, footprint);
            let index_of = |cell: (i32, i32)| {
                let x = u16::try_from(cell.0).ok()?;
                let y = u16::try_from(cell.1).ok()?;
                extent.index(mint_movement::Cell::new(x, y))
            };
            let mut chosen: Option<(Vec2, goal_cells::CellRect, u32, Vec2)> = None;
            if !record.idle {
                let mover = self.mover_class(tick, ids, member.entity);
                let view = self.dynamic_view(tick, ids);
                let layer = goal_cells::layer(tick.state(), ids.goal_cells);
                let facing = tick
                    .state()
                    .get(ids.body, member.entity)
                    .map_or(Facing::EAST, |b| b.0.facing);
                let crowding = layer
                    .zip(goal_cells::rect(extent, here, footprint))
                    .map_or(0, |(layer, rect)| layer.crowding(member.entity, rect));
                let mut free: Vec<(Vec2, goal_cells::CellRect, u32)> = Vec::new();
                let mut best = (0.0f32, None);
                let mut outside = false;
                for (dx, dy) in NEIGHBOURS {
                    let cell = (here.0.saturating_add(dx), here.1.saturating_add(dy));
                    let to = goal_cells::anchor_point(geometry, cell, footprint);
                    let (Some(rect), Some(index)) =
                        (goal_cells::rect(extent, cell, footprint), index_of(cell))
                    else {
                        continue;
                    };
                    let passable = geometry
                        .cell_at(to, 0, extent)
                        .is_some_and(|at| view.is_passable(at, mover));
                    let roomier = layer.is_none_or(|layer| {
                        let there = layer.crowding(member.entity, rect);
                        (there == 0 || there < crowding)
                            && !layer
                                .claimants_of(member.entity, rect)
                                .into_iter()
                                .any(|other| self.move_blocked(tick, ids, other))
                    });
                    if !passable || !roomier || record.remembers(index) {
                        continue;
                    }
                    free.push((to, rect, index));
                    let Some(enemy) = Self::nearest_to(&enemies, to) else {
                        continue;
                    };
                    let step = Step {
                        from: member.position,
                        facing,
                        radius: member.radius,
                        to,
                        enemy: enemy.position,
                        enemy_radius: enemy.radius,
                        enemy_is_structure: enemy.is_structure,
                        centre,
                    };
                    match crate::melee::weigh(&step, &tuning) {
                        Weigh::Score(score) if score > best.0 => {
                            best = (score, Some((to, rect, index, enemy.position)));
                        }
                        Weigh::BeyondOuter => outside = true,
                        Weigh::Score(_) | Weigh::OutsideCone => {}
                    }
                }
                if best.0 < 1.0 && outside {
                    for (to, rect, index) in &free {
                        let score = RIM_SCORE - to.minus(centre).length_squared();
                        if score > best.0 {
                            best = (score, Some((*to, *rect, *index, record.face)));
                        }
                    }
                }
                if best.0 > 0.0 {
                    chosen = best.1;
                }
            }
            if let Some((to, rect, index, face)) = chosen {
                record.goal = to;
                record.face = face;
                record.remember(index);
                tick.state_mut()
                    .insert(ids.member_melee, member.entity, record)?;
                if let Some(layer) = goal_cells::layer_mut(tick, ids.goal_cells) {
                    layer.claim(member.entity, rect);
                }
                continue;
            }
            if let Some(enemy) = self.adjacent_enemy(tick, ids, &member, None, snapshot) {
                tick.state_mut()
                    .insert(ids.member_melee, member.entity, record)?;
                self.attack_unit(tick, ids, &member, enemy.entity)?;
                continue;
            }
            if let Some(index) = index_of(here) {
                record.remember(index);
            }
            tick.state_mut()
                .insert(ids.member_melee, member.entity, record)?;
            if let Some(rect) = goal_cells::rect(extent, here, footprint) {
                if let Some(layer) = goal_cells::layer_mut(tick, ids.goal_cells) {
                    if layer.held(member.entity).is_none() {
                        layer.claim(member.entity, rect);
                    }
                }
            }
        }
        Ok(())
    }

    fn recentre(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
    ) -> Result<()> {
        let positions: Vec<Vec2> = horde
            .members
            .iter()
            .filter(|member| {
                tick.state()
                    .get(ids.health, **member)
                    .is_some_and(|health| !health.0.is_dead())
            })
            .filter_map(|member| tick.state().get(ids.body, *member).map(|b| b.0.position))
            .collect();
        if positions.is_empty() {
            return Ok(());
        }
        #[allow(clippy::cast_precision_loss)]
        let count = positions.len() as f32;
        let sum = positions.iter().fold(Vec2::ZERO, |sum, at| sum.plus(*at));
        let centroid = sum.scale(1.0 / count);
        let Some(nearest) =
            positions
                .iter()
                .copied()
                .fold(None, |best: Option<(f32, Vec2)>, at| {
                    let d = at.minus(centroid).length_squared();
                    match best {
                        Some((best_d, _)) if best_d <= d => best,
                        _ => Some((d, at)),
                    }
                })
        else {
            return Ok(());
        };
        let Some(mut body) = tick.state().get(ids.body, horde_entity).copied() else {
            return Ok(());
        };
        if body.0.position != nearest.1 {
            body.0.position = nearest.1;
            tick.state_mut().insert(ids.body, horde_entity, body)?;
        }
        Ok(())
    }

    pub(super) fn member_attack_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        living: &[(Foe, PlayerId)],
        range: f32,
        now: Frame,
    ) -> Result<Option<Foe>> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("member_attack_victim"));
        let Some(mut attack) = tick.state().get(ids.member_attack, member.entity).copied() else {
            return Ok(None);
        };
        let victim = living
            .binary_search_by_key(&attack.victim, |(foe, _)| foe.entity)
            .ok()
            .and_then(|at| living.get(at))
            .map(|(foe, _)| *foe);
        let Some(victim) = victim else {
            tick.state_mut().remove(ids.member_attack, member.entity);
            return Ok(None);
        };
        let reached = self.melee_in_reach(tick, ids, member, &victim, range);
        let since = if reached {
            None
        } else {
            Some(attack.out_of_reach_since.unwrap_or(now))
        };
        if since.is_some_and(|since| Self::after(since, crate::melee::GIVE_UP_MS) <= now) {
            tick.state_mut().remove(ids.member_attack, member.entity);
            return Ok(None);
        }
        let before = attack;
        attack.out_of_reach_since = since;
        let standing = attack
            .spot
            .is_none_or(|spot| spot.minus(member.position).length() < 1.0);
        let waiting = attack.waits_until.is_some_and(|until| now < until);
        let due = !waiting
            && (standing
                || attack.planned.is_none_or(|(seen, when)| {
                    now.get().saturating_sub(when.get()) >= REPLAN_TICKS
                        && super::movement::goal_moved(member.position, seen, victim.position)
                }));
        if due {
            let keep = !standing
                && attack.spot.is_some_and(|spot| {
                    let there = Foe {
                        position: spot,
                        ..*member
                    };
                    self.melee_in_reach(tick, ids, &there, &victim, range)
                });
            if !keep {
                match self.melee_engage_spot(tick, ids, member.entity, victim.entity, range) {
                    Engage::At(spot) => {
                        attack.spot = Some(spot);
                        attack.waits_until = None;
                    }
                    Engage::Wait if reached => {
                        attack.spot = Some(member.position);
                        attack.waits_until =
                            Some(Self::after(now, super::melee_engage::ENGAGE_WAIT_MS));
                    }
                    Engage::Wait => {
                        tick.state_mut().remove(ids.member_attack, member.entity);
                        return Ok(None);
                    }
                }
            }
            attack.planned = Some((victim.position, now));
            if let Some(spot) = attack.spot {
                self.claim_spot(tick, ids, member.entity, spot);
            }
        }
        if attack != before {
            tick.state_mut()
                .insert(ids.member_attack, member.entity, attack)?;
        }
        Ok(Some(victim))
    }
}

fn idle_effect(member: Entity, conditions: &[String], starting: bool) -> (Entity, Effect) {
    let names = || mint_sage_modules::effects::names(conditions.to_vec());
    let none = || mint_sage_modules::effects::names(Vec::new());
    let (add, remove) = if starting {
        (names(), none())
    } else {
        (none(), names())
    };
    (member, Effect::ModelConditions { add, remove })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_approach_exit_snaps_only_within_the_square_root_of_twelve_and_a_half() {
        let goal = Vec2::new(100.0, 100.0);
        let near = Vec2::new(103.5, 100.0);
        let far = Vec2::new(103.6, 100.0);
        assert_eq!(
            (
                approach_exit_position(near, goal),
                approach_exit_position(far, goal)
            ),
            (goal, far)
        );
    }
}
