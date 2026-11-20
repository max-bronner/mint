use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_movement::{Route, Vec2};
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick, World};

use super::team::TeamId;
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
    ATTITUDE_AGGRESSIVE, ATTITUDE_ALERT,
};
use crate::combat::{acquire_target, Foe};
use crate::diag::diag;
use crate::rules::busy::{HeldOrder, MovePath};
use crate::rules::{Ids, RotwkRules};
use crate::unit::{Owner, Selected};

const PROXY_PLAYER: u8 = 7;

const MAX_PATH_STEPS: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PathPoint {
    pub(crate) position: Vec2,
    pub(crate) labels: [Option<String>; 3],
    pub(crate) links: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Hunters {
    set: BTreeSet<Entity>,
    scans: BTreeMap<Entity, u64>,
    team_targets: BTreeMap<TeamId, Entity>,
}

impl Hunters {
    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.set.retain(|&entity| is_live(entity));
        self.scans.retain(|&entity, _| is_live(entity));
        self.team_targets.retain(|_, &mut target| is_live(target));
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.set.remove(&entity);
    }

    pub(super) fn insert(&mut self, entity: Entity) {
        self.set.insert(entity);
    }

    #[cfg(test)]
    pub(super) fn next_scan(&self, entity: Entity) -> Option<u64> {
        self.scans.get(&entity).copied()
    }

    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.set.contains(&entity)
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.set.len());
        for entity in &self.set {
            entity.hash_into(hasher);
        }
        if self.scans.is_empty() && self.team_targets.is_empty() {
            return;
        }
        hasher.write_usize(self.scans.len());
        for (entity, at) in &self.scans {
            entity.hash_into(hasher);
            hasher.write_u64(*at);
        }
        hasher.write_usize(self.team_targets.len());
        for (team, target) in &self.team_targets {
            hasher.write_u32(team.get());
            target.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::rules) enum ScriptOrder {
    Move(Vec2, bool),
    MoveLeg(Vec2, bool),
    MoveFollow(Vec2, bool),
    AttackMove(Vec2, bool),
    AttackFollow(Vec2, bool),
    AttackFollowLeg(Vec2, bool),
    AttackMoveObject(Entity),
    Attack(Entity),
    Enter(Entity),
    Stop,
}

#[derive(Debug, Clone)]
enum Subject {
    Named(String),
    Team { name: String, side: String },
}

#[derive(Debug, Clone)]
enum What {
    MoveTo(Option<Vec2>),
    AttackMoveTo(Option<Vec2>),
    FollowPath {
        path: String,
        attack: bool,
        as_team: bool,
        in_formation: bool,
        exact: bool,
    },
    Stop,
    Hunt,
    AttackNamed(String),
    AttackMoveToNamed(String),
    AttackTeam {
        name: String,
        side: String,
    },
}

struct ActionOrder {
    subject: Subject,
    what: What,
    verb: &'static str,
}

impl RotwkRules {
    pub(super) fn subject_members(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        name: &str,
        side: Option<&str>,
    ) -> Vec<Entity> {
        let Some(side) = side else {
            return self
                .object_named(tick, ids, &state.rt, name, "")
                .into_iter()
                .collect();
        };
        let Some(team) = self.peek_team(state, name, side) else {
            return Vec::new();
        };
        state
            .rt
            .teams
            .get(team)
            .map(|instance| {
                instance
                    .members()
                    .iter()
                    .copied()
                    .filter(|&m| tick.state().is_live(m) && !tick.state().contains(ids.doomed, m))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn issue_script_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        members: &[Entity],
        order: ScriptOrder,
    ) -> Result<()> {
        let awake: Vec<Entity> = members
            .iter()
            .copied()
            .filter(|&member| !self.mood_asleep(tick.state(), ids, member))
            .collect();
        if let ScriptOrder::Move(to, queue)
        | ScriptOrder::MoveLeg(to, queue)
        | ScriptOrder::MoveFollow(to, queue) = order
        {
            let attack = match order {
                ScriptOrder::MoveLeg(..) => ScriptOrder::AttackFollowLeg(to, queue),
                ScriptOrder::MoveFollow(..) => ScriptOrder::AttackFollow(to, queue),
                _ => ScriptOrder::AttackMove(to, queue),
            };
            let (hot, calm): (Vec<Entity>, Vec<Entity>) = awake.iter().copied().partition(|&m| {
                matches!(
                    self.mood_attitude(tick.state(), ids, m),
                    ATTITUDE_ALERT | ATTITUDE_AGGRESSIVE
                )
            });
            if !hot.is_empty() {
                self.issue_order_to(tick, ids, &hot, attack)?;
                return self.issue_order_to(tick, ids, &calm, order);
            }
        }
        self.issue_order_to(tick, ids, &awake, order)
    }

    pub(in crate::rules) fn issue_order_to(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        members: &[Entity],
        order: ScriptOrder,
    ) -> Result<()> {
        if let ScriptOrder::AttackMoveObject(object) = order {
            let Some(at) = self.position_of_entity(tick, ids, object) else {
                return Ok(());
            };
            self.issue_order_to(tick, ids, members, ScriptOrder::AttackMove(at, false))?;
            let height = self.object_z(tick.state(), ids, object);
            for &member in members {
                let mover = self.approach_mover(tick, ids, member);
                if let Some(held) = tick.state_mut().get_mut(ids.attack_move_order, mover) {
                    held.object = Some((object, at, height));
                }
            }
            return Ok(());
        }
        self.as_owner_selection(tick, ids, members, |rules, tick, actor| match order {
            ScriptOrder::Move(to, queue) | ScriptOrder::MoveFollow(to, queue) => rules.issue_order(
                tick,
                ids,
                actor,
                HeldOrder::MoveTo(to, queue, MovePath::Group),
            ),
            ScriptOrder::AttackFollow(to, queue) => rules.issue_order(
                tick,
                ids,
                actor,
                HeldOrder::AttackMove(to, queue, MovePath::AttackFollow),
            ),
            ScriptOrder::AttackFollowLeg(to, queue) => rules.issue_order(
                tick,
                ids,
                actor,
                HeldOrder::AttackMove(to, queue, MovePath::AttackFollowLeg),
            ),
            ScriptOrder::MoveLeg(to, queue) => rules.issue_order(
                tick,
                ids,
                actor,
                HeldOrder::MoveTo(to, queue, MovePath::GroupLeg),
            ),
            ScriptOrder::AttackMove(to, queue) => rules.issue_order(
                tick,
                ids,
                actor,
                HeldOrder::AttackMove(to, queue, MovePath::Group),
            ),
            ScriptOrder::AttackMoveObject(_) => Ok(()),
            ScriptOrder::Attack(target) => {
                rules.issue_order(tick, ids, actor, HeldOrder::ScriptAttackObject(target))
            }
            ScriptOrder::Enter(container) => {
                rules.issue_order(tick, ids, actor, HeldOrder::Enter(container))
            }
            ScriptOrder::Stop => rules.issue_order(tick, ids, actor, HeldOrder::Stop),
        })
    }

    pub(super) fn as_owner_selection(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        members: &[Entity],
        mut act: impl FnMut(&Self, &mut Tick<'_>, PlayerId) -> Result<()>,
    ) -> Result<()> {
        let mut by_owner: BTreeMap<u8, Vec<Entity>> = BTreeMap::new();
        for &member in members {
            if let Some(owner) = tick.state().get(ids.owner, member) {
                by_owner.entry(owner.0.get()).or_default().push(member);
            }
        }
        for (owner, group) in by_owner {
            let player = PlayerId::new(owner);
            let actor = if Selected::bit(player).is_some() {
                player
            } else {
                PlayerId::new(PROXY_PLAYER)
            };
            if actor != player {
                for &member in &group {
                    tick.state_mut().insert(ids.owner, member, Owner(actor))?;
                }
            }
            let held = self.selection(tick, ids, actor);
            let saved = Self::deselect(tick, ids, actor, &held)?;
            self.select_for_script(tick, ids, actor, &group)?;
            let result = act(self, tick, actor);
            self.clear_selection(tick, ids, actor)?;
            Self::restore_selection(tick, ids, &saved)?;
            if actor != player {
                for &member in &group {
                    tick.state_mut().insert(ids.owner, member, Owner(player))?;
                }
            }
            result?;
        }
        Ok(())
    }

    pub(super) fn position_of_entity(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<Vec2> {
        tick.state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
    }

    pub(super) fn follow_exactly(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
        walk: &[Vec2],
        offset: Vec2,
    ) -> Result<()> {
        let Some(&last) = walk.last() else {
            return Ok(());
        };
        self.issue_script_order(
            tick,
            ids,
            &[member],
            ScriptOrder::Move(last.plus(offset), false),
        )?;
        let carrier = Self::pather(tick, ids, member);
        let Some(mut path) = tick.state().get(ids.path, carrier).cloned() else {
            return Ok(());
        };
        let Some(start) = self.position_of_entity(tick, ids, carrier) else {
            return Ok(());
        };
        let mut points: Vec<Vec2> = std::iter::once(start)
            .chain(walk.iter().map(|point| point.plus(offset)))
            .collect();
        let geometry = self.passability.geometry;
        if let Some(end) = points.last_mut() {
            let extent = self.passability.grid.extent();
            if let Some(cell) = geometry.cell_at(*end, 0, extent) {
                *end = geometry.centre_of(cell);
            }
        }
        path.goal = last.plus(offset);
        path.route = Route {
            portals: vec![None; points.len()],
            points,
            extra_distance: 0.0,
            unsearched: true,
        };
        crate::unit::give_path(tick.state_mut(), (ids.body, ids.path), carrier, path)
    }

    pub(super) fn nearest_on_path(
        engine: &MapScriptEngine,
        label: &str,
        from: Vec2,
    ) -> Option<usize> {
        engine
            .path_points()
            .iter()
            .enumerate()
            .filter(|(_, point)| {
                point
                    .labels
                    .iter()
                    .flatten()
                    .any(|found| found.eq_ignore_ascii_case(label))
            })
            .min_by(|a, b| {
                let da = distance_squared(a.1.position, from);
                let db = distance_squared(b.1.position, from);
                da.total_cmp(&db)
            })
            .map(|(index, _)| index)
    }

    pub(super) fn path_from(
        &self,
        tick: &mut Tick<'_>,
        engine: &MapScriptEngine,
        label: &str,
        from: Vec2,
    ) -> Vec<Vec2> {
        let points = engine.path_points();
        let nearest = Self::nearest_on_path(engine, label, from);
        let mut walk = Vec::new();
        let mut seen = BTreeSet::new();
        let mut at = nearest;
        while let Some(index) = at {
            if walk.len() >= MAX_PATH_STEPS || !seen.insert(index) {
                break;
            }
            let Some(point) = points.get(index) else {
                break;
            };
            walk.push(point.position);
            at = match point.links.len() {
                0 => None,
                1 => point.links.first().copied(),
                n => {
                    let pick = tick.rng().below(u32::try_from(n).unwrap_or(1));
                    point.links.get(usize::try_from(pick).unwrap_or(0)).copied()
                }
            };
        }
        walk
    }

    pub(super) fn hostile_foes(&self, tick: &Tick<'_>, ids: Ids, scanner: Entity) -> Vec<Foe> {
        let world = tick.state();
        world
            .store(ids.owner)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        self.object_hostile(world, scanner, *entity)
                            && world.is_live(*entity)
                            && !world.contains(ids.doomed, *entity)
                    })
                    .filter_map(|(entity, _)| self.foe_of(tick, ids, entity))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn step_hunters(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let now = Self::script_frame(tick);
        let mut hunters: BTreeSet<Entity> = state.rt.hunters.set.clone();
        if state.rt.standing.has_hunters() {
            let hunting: Vec<Entity> = tick
                .state()
                .store(ids.owner)
                .map(|store| {
                    store
                        .iter()
                        .filter(|(_, owner)| state.rt.standing.is_hunting(owner.0))
                        .map(|(entity, _)| entity)
                        .collect()
                })
                .unwrap_or_default();
            hunters.extend(
                hunting
                    .into_iter()
                    .filter(|&entity| self.hunts_when_idle(tick, ids, entity)),
            );
        }
        let mut scans = BTreeMap::new();
        for hunter in hunters {
            if !tick.state().is_live(hunter) {
                continue;
            }
            let due_at = match state.rt.hunters.scans.get(&hunter) {
                Some(&at) => at,
                None => now + u64::from(tick.rng().below(HUNT_SCAN_FRAMES_SPREAD)),
            };
            if now < due_at {
                scans.insert(hunter, due_at);
                continue;
            }
            if self.out_of_ammo(tick, ids, hunter) {
                diag!(
                    "script: hunter {} is out of ammo - its hunt fails",
                    hunter.index()
                );
                state.rt.hunters.forget(hunter);
                continue;
            }
            scans.insert(hunter, now + HUNT_SCAN_FRAMES);
            let world = tick.state();
            if world.contains(ids.path, hunter) || world.contains(ids.attack_order, hunter) {
                continue;
            }
            let (Some(owner), Some(at)) = (
                world.get(ids.owner, hunter).map(|o| o.0),
                self.position_of_entity(tick, ids, hunter),
            ) else {
                continue;
            };
            let mut foes = self.hostile_foes(tick, ids, hunter);
            foes.retain(|foe| !self.off_map(tick.state(), ids, foe.position));
            let radius = Self::mover_radius(tick, ids, hunter);
            let hunting_all = state.rt.standing.is_hunting(owner);
            let common_team = state
                .rt
                .teams
                .team_of(hunter)
                .zip(state.rt.teams.proto_of(hunter))
                .filter(|&(_, proto)| {
                    engine
                        .team_prototypes()
                        .get(proto)
                        .is_some_and(|proto| proto.template.attack_common_target)
                })
                .map(|(team, _)| team);
            let team_victim = common_team
                .and_then(|team| state.rt.hunters.team_targets.get(&team).copied())
                .filter(|&victim| self.common_target_holds(tick, ids, victim));
            let has_set = self.priority_set_of(world, ids, hunter).is_some();
            let target = if team_victim.is_some() && !has_set {
                team_victim
            } else {
                let ranked =
                    self.acquire_for(world, ids, hunter, at, radius, &foes, 0.0, HUNT_RANGE);
                let mut found = ranked.or_else(|| {
                    (hunting_all && !foes.is_empty())
                        .then(|| acquire_target(at, radius, &foes, 0.0, HUNT_RANGE))
                        .flatten()
                });
                if let Some(team) = common_team {
                    if let (Some(shared), true) = (team_victim, has_set) {
                        let shared_priority = self.base_priority(world, ids, hunter, shared);
                        let own_priority =
                            found.map_or(0, |found| self.base_priority(world, ids, hunter, found));
                        if found.is_none() || shared_priority >= own_priority {
                            found = Some(shared);
                        }
                    }
                    match found {
                        None => {
                            state.rt.hunters.team_targets.remove(&team);
                        }
                        Some(victim) => {
                            if self.computer_controlled(owner) && !self.easy_computer(owner) {
                                state.rt.hunters.team_targets.insert(team, victim);
                            }
                        }
                    }
                }
                found
            };
            if let Some(target) = target {
                let target = if Some(target) == team_victim {
                    target
                } else {
                    self.horde_member_pick(tick, ids, hunter, target)
                };
                self.issue_script_order(tick, ids, &[hunter], ScriptOrder::Attack(target))?;
            }
        }
        state.rt.hunters.scans = scans;
        Ok(())
    }

    pub(super) fn out_of_ammo(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        use mint_sage_combat::WeaponStatus;
        let world = tick.state();
        if let Some(horde) = world.get(ids.horde, entity) {
            return self.horde_rangefinder(horde).is_none()
                && self.melee_rangefinder(horde).is_none();
        }
        let Some(armed) = world.get(ids.armed, entity).copied() else {
            return true;
        };
        let now = tick.frame();
        let mut current = armed.state;
        if current.status(now) != WeaponStatus::OutOfAmmo {
            return false;
        }
        let Some((_, slots)) = self.selected_set(tick, ids, entity) else {
            return true;
        };
        let held = world.get(ids.weapon_slots, entity).copied();
        slots.arms.iter().enumerate().all(|(index, arm)| {
            if arm.is_none() || held.is_none_or(|held| usize::from(held.current) == index) {
                return true;
            }
            held.and_then(|held| held.states.get(index).copied().flatten())
                .is_some_and(|mut state| state.status(now) == WeaponStatus::OutOfAmmo)
        })
    }

    pub(in crate::rules) fn common_target(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        living: &[(Foe, PlayerId)],
        mood: bool,
    ) -> Option<Foe> {
        let engine = self.map_scripts.as_ref()?;
        let state = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .map(|(_, state)| state)?;
        if state.rt.hunters.team_targets.is_empty() {
            return None;
        }
        let unit = Self::commanded_unit(world, ids, entity);
        let team = state.rt.teams.team_of(unit)?;
        let proto = state.rt.teams.get(team)?.proto;
        if !engine
            .team_prototypes()
            .get(proto)
            .is_some_and(|proto| proto.template.attack_common_target)
        {
            return None;
        }
        let victim = *state.rt.hunters.team_targets.get(&team)?;
        if (mood && self.mood_attitude(world, ids, entity) < 0)
            || world.contains(ids.contained, victim)
        {
            return None;
        }
        if let Some(controller) = self.team_owner(engine, state, team) {
            if !self.not_hidden_from(world, ids, controller, victim) {
                return None;
            }
        }
        let (foe, _) = living
            .binary_search_by_key(&victim, |(foe, _)| foe.entity)
            .ok()
            .and_then(|at| living.get(at))?;
        self.object_hostile(world, entity, victim).then_some(*foe)
    }

    pub(in crate::rules) fn note_common_target(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) {
        let Some(engine) = self.map_scripts.as_ref() else {
            return;
        };
        let world = tick.state();
        let Some((anchor, state)) = world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
        else {
            return;
        };
        let unit = Self::commanded_unit(world, ids, attacker);
        let Some(team) = state.rt.teams.team_of(unit) else {
            return;
        };
        let Some(proto) = state.rt.teams.get(team).map(|team| team.proto) else {
            return;
        };
        if !engine
            .team_prototypes()
            .get(proto)
            .is_some_and(|proto| proto.template.attack_common_target)
        {
            return;
        }
        let controller = self.team_owner(engine, state, team);
        if let Some(&held) = state.rt.hunters.team_targets.get(&team) {
            if world.is_live(held)
                && !world.contains(ids.contained, held)
                && controller.is_none_or(|owner| self.not_hidden_from(world, ids, owner, held))
            {
                return;
            }
        }
        let keeps = controller
            .is_some_and(|owner| self.computer_controlled(owner) && !self.easy_computer(owner));
        if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
            if keeps {
                state.rt.hunters.team_targets.insert(team, victim);
            } else {
                state.rt.hunters.team_targets.remove(&team);
            }
        }
    }

    fn common_target_holds(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> bool {
        self.target_is_live(tick, ids, victim)
            && self.foe_of(tick, ids, victim).is_some()
            && !tick.state().contains(ids.contained, victim)
            && !self.has_kind_bit(tick.state(), ids, victim, "AIRCRAFT")
    }
}

pub(super) const HUNT_RANGE: f32 = 9999.9;

const HUNT_SCAN_FRAMES: u64 = 15;
const HUNT_SCAN_FRAMES_SPREAD: u32 = 16;

const TEAM_OFFSET_MAX: f32 = 150.0;

fn team_offset(at: Vec2, center: Vec2) -> Vec2 {
    let offset = at.minus(center);
    let length = offset.length();
    if length > TEAM_OFFSET_MAX {
        offset.scale(TEAM_OFFSET_MAX / length)
    } else {
        offset
    }
}

fn distance_squared(a: Vec2, b: Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}

impl ActionVerb for ActionOrder {
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
        let (name, side) = match &self.subject {
            Subject::Named(name) => (name.as_str(), None),
            Subject::Team { name, side } => (name.as_str(), Some(side.as_str())),
        };
        let members = rules.subject_members(tick, ids, state, name, side);
        if members.is_empty() {
            return Ok(());
        }
        let order = match &self.what {
            What::MoveTo(to) => to.map(|to| ScriptOrder::Move(to, false)),
            What::AttackMoveTo(to) => to.map(|to| ScriptOrder::AttackMove(to, false)),
            What::Stop => Some(ScriptOrder::Stop),
            What::AttackNamed(unit) => rules
                .object_named(tick, ids, &state.rt, unit, "")
                .map(ScriptOrder::Attack),
            What::AttackMoveToNamed(unit) => rules
                .object_named(tick, ids, &state.rt, unit, "")
                .map(ScriptOrder::AttackMoveObject),
            What::AttackTeam { name, side } => rules
                .subject_members(tick, ids, state, name, Some(side))
                .first()
                .copied()
                .map(ScriptOrder::Attack),
            What::Hunt => {
                state.rt.hunters.set.extend(members.iter().copied());
                for &member in &members {
                    state.rt.attack_areas.forget(member);
                    state.rt.wanderers.forget(member);
                }
                diag!(
                    "script: {} sends {} unit(s) hunting",
                    self.verb,
                    members.len()
                );
                return Ok(());
            }
            What::FollowPath {
                path,
                attack,
                as_team,
                in_formation,
                exact,
            } => {
                let positions: Vec<Vec2> = members
                    .iter()
                    .filter_map(|&m| rules.position_of_entity(tick, ids, m))
                    .collect();
                let Some(center) = super::cinematic::mean_position(&positions) else {
                    return Ok(());
                };
                let walk = rules.path_from(tick, engine, path, center);
                let offsets: Vec<(Entity, Vec2)> = if *in_formation {
                    let first = walk.first().copied().unwrap_or(center);
                    rules.column_slots(tick, ids, &members, center, first)
                } else {
                    members
                        .iter()
                        .map(|&m| {
                            let offset = if *as_team {
                                rules
                                    .position_of_entity(tick, ids, m)
                                    .map_or(Vec2::ZERO, |at| team_offset(at, center))
                            } else {
                                Vec2::ZERO
                            };
                            (m, offset)
                        })
                        .collect()
                };
                for &member in &members {
                    state.rt.wanderers.forget(member);
                    tick.state_mut().remove(ids.formation_slot, member);
                }
                if (*as_team || *in_formation) && !*exact {
                    state
                        .rt
                        .walks
                        .begin_team(&offsets, path, walk.clone(), *attack);
                    if let Some(&(key, _)) = offsets.first() {
                        for &(member, _) in &offsets {
                            rules.retarget_team_member(tick, ids, state, key, member)?;
                        }
                    }
                } else {
                    for &(member, offset) in &offsets {
                        if *exact {
                            rules.follow_exactly(tick, ids, member, &walk, offset)?;
                        } else {
                            for (step, &to) in walk.iter().enumerate() {
                                let to = to.plus(offset);
                                let more = step + 1 < walk.len();
                                let order = match (*attack, more) {
                                    (true, true) => ScriptOrder::AttackFollowLeg(to, step > 0),
                                    (true, false) => ScriptOrder::AttackFollow(to, step > 0),
                                    (false, true) => ScriptOrder::MoveLeg(to, step > 0),
                                    (false, false) => ScriptOrder::MoveFollow(to, step > 0),
                                };
                                rules.issue_script_order(tick, ids, &[member], order)?;
                            }
                        }
                        state.rt.walks.begin(member, path);
                    }
                }
                if *in_formation {
                    for &(member, offset) in &offsets {
                        tick.state_mut().insert(
                            ids.formation_slot,
                            member,
                            crate::unit::FormationSlot(offset),
                        )?;
                    }
                }
                diag!(
                    "script: {} sends {} unit(s) along {path:?} ({} waypoint(s))",
                    self.verb,
                    members.len(),
                    walk.len()
                );
                return Ok(());
            }
        };
        if let Some(order) = order {
            for &member in &members {
                state.rt.walks.forget(member);
                state.rt.attack_areas.forget(member);
                state.rt.wanderers.forget(member);
            }
            rules.issue_script_order(tick, ids, &members, order)?;
            diag!("script: {} orders {} unit(s)", self.verb, members.len());
        }
        Ok(())
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        match &self.subject {
            Subject::Named(name) => Some(name),
            Subject::Team { .. } => None,
        }
    }
}

fn subject(params: &[Parameter], ctx: &ParseCtx<'_>, named: bool) -> Option<Subject> {
    let name = parameter_string(params, 0)?;
    Some(if named {
        Subject::Named(name)
    } else {
        Subject::Team {
            name,
            side: ctx.side.to_owned(),
        }
    })
}

fn waypoint(params: &[Parameter], ctx: &ParseCtx<'_>, index: usize) -> Option<Vec2> {
    let name = parameter_string(params, index)?;
    ctx.waypoints.get(&name).map(|(position, _)| *position)
}

fn build(subject: Subject, what: What, verb: &'static str) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionOrder {
        subject,
        what,
        verb,
    }))
}

pub(super) fn parse_move_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, true)?,
        What::MoveTo(waypoint(params, ctx, 1)),
        "MOVE_NAMED_UNIT_TO",
    )
}

pub(super) fn parse_attack_move_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, true)?,
        What::AttackMoveTo(waypoint(params, ctx, 1)),
        "ATTACK_MOVE_NAMED_UNIT_TO",
    )
}

pub(super) fn parse_move_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, false)?,
        What::MoveTo(waypoint(params, ctx, 1)),
        "MOVE_TEAM_TO",
    )
}

pub(super) fn parse_attack_move_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, false)?,
        What::AttackMoveTo(waypoint(params, ctx, 1)),
        "ATTACK_MOVE_TEAM_TO",
    )
}

fn follow(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    named: bool,
    attack: bool,
    exact: bool,
    verb: &'static str,
) -> Option<Arc<dyn ActionVerb>> {
    let flag = |index| !named && parameter_int(params, index).is_some_and(|flag| flag != 0);
    let as_team = flag(2);
    let in_formation = flag(3) && !exact;
    build(
        subject(params, ctx, named)?,
        What::FollowPath {
            path: parameter_string(params, 1)?,
            attack,
            as_team,
            in_formation,
            exact,
        },
        verb,
    )
}

pub(super) fn parse_follow_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(params, ctx, true, false, false, "NAMED_FOLLOW_WAYPOINTS")
}

pub(super) fn parse_follow_named_exact(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(
        params,
        ctx,
        true,
        false,
        true,
        "NAMED_FOLLOW_WAYPOINTS_EXACT",
    )
}

pub(super) fn parse_attack_follow_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(
        params,
        ctx,
        true,
        true,
        false,
        "NAMED_ATTACK_FOLLOW_WAYPOINTS",
    )
}

pub(super) fn parse_follow_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(params, ctx, false, false, false, "TEAM_FOLLOW_WAYPOINTS")
}

pub(super) fn parse_follow_team_exact(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(
        params,
        ctx,
        false,
        false,
        true,
        "TEAM_FOLLOW_WAYPOINTS_EXACT",
    )
}

pub(super) fn parse_attack_follow_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    follow(
        params,
        ctx,
        false,
        true,
        false,
        "TEAM_ATTACK_MOVE_FOLLOW_WAYPOINTS",
    )
}

pub(super) fn parse_stop_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(subject(params, ctx, true)?, What::Stop, "NAMED_STOP")
}

pub(super) fn parse_stop_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(subject(params, ctx, false)?, What::Stop, "TEAM_STOP")
}

pub(super) fn parse_hunt_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(subject(params, ctx, false)?, What::Hunt, "TEAM_HUNT")
}

pub(super) fn parse_hunt_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(subject(params, ctx, true)?, What::Hunt, "NAMED_HUNT")
}

pub(super) fn parse_team_attack_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, false)?,
        What::AttackNamed(parameter_string(params, 1)?),
        "TEAM_ATTACK_NAMED",
    )
}

pub(super) fn parse_named_attack_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, true)?,
        What::AttackTeam {
            name: parameter_string(params, 1)?,
            side: ctx.side.to_owned(),
        },
        "NAMED_ATTACK_TEAM",
    )
}

pub(super) fn parse_team_attack_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, false)?,
        What::AttackTeam {
            name: parameter_string(params, 1)?,
            side: ctx.side.to_owned(),
        },
        "TEAM_ATTACK_TEAM",
    )
}

pub(super) fn parse_named_attack_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, true)?,
        What::AttackNamed(parameter_string(params, 1)?),
        "NAMED_ATTACK_NAMED",
    )
}

pub(super) fn parse_attack_move_team_to_named(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    build(
        subject(params, ctx, false)?,
        What::AttackMoveToNamed(parameter_string(params, 1)?),
        "ATTACK_MOVE_TEAM_TO_NAMED_OBJECT",
    )
}
