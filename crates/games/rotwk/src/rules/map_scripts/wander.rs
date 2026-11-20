use std::collections::{BTreeMap, BTreeSet};

use mint_movement::Vec2;
use mint_sim::{Entity, Result, StateHasher, Tick};

use super::orders::ScriptOrder;
use super::MapScriptState;
use crate::rules::{Ids, RotwkRules};

const PATHFIND_CELL_SIZE: f32 = 10.0;

const WANDER_CONDITION: &str = "WANDER";

const AIR_SURFACE: u16 = 1 << 3;
const PANIC_CONDITION: &str = "PANICKING";

const SUCCESS_NODE_RADIUS: f32 = 40.0;

const EXTRA_DISTANCE_HOPS: usize = 5;

const DEFAULT_WANDER_CELLS: i32 = 3;

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Wanderers(
    BTreeMap<Entity, WanderPlace>,
    BTreeMap<Entity, PathWander>,
    BTreeSet<Entity>,
);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WanderPlace {
    origin: [u32; 2],
    timer: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct PathWander {
    label: String,
    current: usize,
    prior: Option<usize>,
    started: bool,
    timer: Option<i32>,
    panic: bool,
    goal: Vec2,
    extra_distance: f32,
    route_end: Option<Vec2>,
    stale_goal: Option<Vec2>,
}

impl Wanderers {
    pub(super) fn begin(&mut self, entity: Entity, origin: Vec2) {
        self.0.insert(
            entity,
            WanderPlace {
                origin: [origin.x.to_bits(), origin.y.to_bits()],
                timer: None,
            },
        );
    }

    pub(super) fn begin_path(&mut self, entity: Entity, label: &str, start: usize, panic: bool) {
        self.2.remove(&entity);
        self.1.insert(
            entity,
            PathWander {
                label: label.to_owned(),
                current: start,
                prior: None,
                started: false,
                timer: None,
                panic,
                goal: Vec2::ZERO,
                extra_distance: 0.0,
                route_end: None,
                stale_goal: None,
            },
        );
    }

    pub(in crate::rules) fn contains(&self, entity: Entity) -> bool {
        self.0.contains_key(&entity) || self.1.contains_key(&entity)
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.0.remove(&entity);
        if self.1.remove(&entity).is_some_and(|walk| walk.panic) {
            self.2.insert(entity);
        }
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.0.retain(|&entity, _| is_live(entity));
        self.1.retain(|&entity, _| is_live(entity));
        self.2.retain(|&entity| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() && self.1.is_empty() && self.2.is_empty() {
            return;
        }
        hasher.write_usize(self.1.len());
        for (entity, walk) in &self.1 {
            entity.hash_into(hasher);
            hasher.write_bytes(walk.label.as_bytes());
            hasher.write_usize(walk.current);
            hasher.write_usize(walk.prior.map_or(0, |prior| prior.saturating_add(1)));
            hasher.write_bool(walk.started);
            if let Some(timer) = walk.timer {
                hasher.write_u32(u32::from_ne_bytes(timer.to_ne_bytes()));
            }
            if walk.panic {
                hasher.write_f32(walk.goal.x);
                hasher.write_f32(walk.goal.y);
                hasher.write_f32(walk.extra_distance);
                for point in [walk.route_end, walk.stale_goal].into_iter().flatten() {
                    hasher.write_f32(point.x);
                    hasher.write_f32(point.y);
                }
            }
        }
        hasher.write_usize(self.0.len());
        for (entity, place) in &self.0 {
            entity.hash_into(hasher);
            hasher.write_u32(place.origin[0]);
            hasher.write_u32(place.origin[1]);
            if let Some(timer) = place.timer {
                hasher.write_u32(u32::from_ne_bytes(timer.to_ne_bytes()));
            }
        }
        if !self.2.is_empty() {
            hasher.write_usize(self.2.len());
            for entity in &self.2 {
                entity.hash_into(hasher);
            }
        }
    }
}

fn chain_extra_distance(points: &[super::orders::PathPoint], from: usize) -> f32 {
    let mut total = 1.0;
    let mut at = from;
    for _ in 0..EXTRA_DISTANCE_HOPS {
        let Some(here) = points.get(at) else { break };
        let Some(next) = here.links.first().copied() else {
            break;
        };
        let Some(there) = points.get(next) else { break };
        total += there.position.minus(here.position).length();
        at = next;
    }
    total
}

fn ground_movement(air_only: bool, current: Option<bool>, held: bool) -> bool {
    !air_only && current == Some(false) && !held
}

impl RotwkRules {
    fn panic_move_succeeded(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        walk: &mut PathWander,
    ) -> bool {
        let world = tick.state();
        let Some(body) = world.get(ids.body, entity).map(|body| body.0) else {
            return false;
        };
        let sent = world.get(ids.path, entity).filter(|path| {
            walk.stale_goal
                .is_none_or(|stale| path.goal.minus(stale).length() >= 1.0)
        });
        let (left, end) = match sent {
            Some(path) => {
                let mut at = body.position;
                let mut left = 0.0_f32;
                let first = usize::try_from(body.waypoint).unwrap_or(usize::MAX);
                for point in path.route.points.iter().skip(first) {
                    left += point.minus(at).length();
                    at = *point;
                }
                let end = path.route.points.last().copied().unwrap_or(walk.goal);
                walk.route_end = Some(end);
                (left, end)
            }
            None => match walk.route_end {
                Some(end) => (0.0, end),
                None => (body.position.minus(walk.goal).length(), walk.goal),
            },
        };
        let limits = self.mover_limits(tick, ids, entity);
        let speed = body.speed / crate::game_data::RETAIL_LOGIC_HZ as f32;
        let close =
            left < limits.arrival_radius || (walk.extra_distance > speed && left < 2.0 * speed);
        close
            && (!self.doing_ground_movement(tick, ids, entity)
                || body.position.minus(end).length() <= SUCCESS_NODE_RADIUS)
    }

    fn doing_ground_movement(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        let air_only = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.surfaces)
            == Some(AIR_SURFACE);
        let current = world
            .get(ids.locomotion, entity)
            .map(|locomotion| locomotion.0.crosses_air);
        ground_movement(air_only, current, world.contains(ids.held, entity))
    }

    pub(in crate::rules) fn wander_cells(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> i32 {
        let world = tick.state();
        let radius = world
            .get(ids.locomotion, entity)
            .map(|locomotion| locomotion.0.wander_radius)
            .or_else(|| {
                let horde = world.get(ids.horde, entity)?;
                let member = horde.members.first()?;
                world
                    .get(ids.locomotion, *member)
                    .map(|locomotion| locomotion.0.wander_radius)
            });
        radius.map_or(DEFAULT_WANDER_CELLS, |radius| {
            #[allow(clippy::cast_possible_truncation)]
            let cells = (radius / PATHFIND_CELL_SIZE + 0.5).floor() as i32;
            cells
        })
    }

    pub(super) fn step_path_wanderers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &super::MapScriptEngine,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let points = engine.path_points();
        for entity in std::mem::take(&mut state.rt.wanderers.2) {
            let world = tick.state();
            let panicking = state
                .rt
                .wanderers
                .1
                .get(&entity)
                .is_some_and(|walk| walk.panic);
            if world.is_live(entity) && !world.contains(ids.repulsed, entity) && !panicking {
                self.set_model_condition(tick, ids, entity, PANIC_CONDITION, false)?;
            }
        }
        let entities: Vec<Entity> = state.rt.wanderers.1.keys().copied().collect();
        for entity in entities {
            let world = tick.state();
            if !world.is_live(entity) || world.contains(ids.doomed, entity) {
                continue;
            }
            if world.contains(ids.repulsed, entity) {
                state.rt.wanderers.1.remove(&entity);
                continue;
            }
            let mut timer = state
                .rt
                .wanderers
                .1
                .get(&entity)
                .and_then(|walk| walk.timer);
            let hit = self.repulsor_check(world, ids, entity, &mut timer);
            let panic = state.rt.wanderers.1.get_mut(&entity).map(|walk| {
                walk.timer = timer;
                walk.panic
            });
            if hit {
                state.rt.wanderers.1.remove(&entity);
                if panic == Some(false) {
                    self.set_model_condition(tick, ids, entity, WANDER_CONDITION, false)?;
                }
                self.begin_flee(tick, ids, entity)?;
                continue;
            }
            let world = tick.state();
            if self.mood_asleep(world, ids, entity) {
                continue;
            }
            let width = world
                .get(ids.locomotion, entity)
                .map_or(0.0, |locomotion| locomotion.0.wander_width);
            let started_panic = state
                .rt
                .wanderers
                .1
                .get(&entity)
                .is_some_and(|walk| walk.started && walk.panic);
            if started_panic {
                let Some(walk) = state.rt.wanderers.1.get_mut(&entity) else {
                    continue;
                };
                if !self.panic_move_succeeded(tick, ids, entity, walk) {
                    continue;
                }
            } else if world.contains(ids.path, entity) {
                continue;
            }
            let Some(walk) = state.rt.wanderers.1.get_mut(&entity) else {
                continue;
            };
            if walk.started {
                walk.prior = Some(walk.current);
                let links = points.get(walk.current).map_or(&[][..], |p| &p.links[..]);
                let next = match links.len() {
                    0 => None,
                    n => {
                        let pick = tick.rng().below(u32::try_from(n).unwrap_or(1));
                        links.get(usize::try_from(pick).unwrap_or(0)).copied()
                    }
                };
                match next {
                    Some(next) => walk.current = next,
                    None => {
                        let panic = walk.panic;
                        state.rt.wanderers.1.remove(&entity);
                        state.rt.walks.complete(entity);
                        if panic {
                            self.set_model_condition(tick, ids, entity, PANIC_CONDITION, false)?;
                        } else {
                            self.set_model_condition(tick, ids, entity, WANDER_CONDITION, false)?;
                            self.apply_locomotor_override(tick, ids, entity, None)?;
                        }
                        continue;
                    }
                }
            } else {
                walk.started = true;
                if !walk.panic {
                    self.set_model_condition(tick, ids, entity, WANDER_CONDITION, true)?;
                }
            }
            let Some(point) = points.get(walk.current).map(|point| point.position) else {
                state.rt.wanderers.1.remove(&entity);
                continue;
            };
            let mut goal = point;
            let has_next = points
                .get(walk.current)
                .is_some_and(|p| !p.links.is_empty());
            if width > 0.0 {
                #[allow(clippy::cast_possible_truncation)]
                let delta = ((width + 0.5).floor() as i32).max(1);
                let span = u32::try_from(delta.saturating_mul(2).saturating_add(1)).unwrap_or(1);
                let mut offset = [0.0_f32; 2];
                for axis in &mut offset {
                    let drawn = i32::try_from(tick.rng().below(span)).unwrap_or(0);
                    let steps = i16::try_from(drawn.saturating_sub(delta)).unwrap_or(0);
                    *axis = f32::from(steps) * PATHFIND_CELL_SIZE;
                }
                goal = Vec2::new(point.x + offset[0], point.y + offset[1]);
            }
            let world_path_goal = tick.state().get(ids.path, entity).map(|path| path.goal);
            walk.goal = goal;
            walk.route_end = None;
            walk.stale_goal = world_path_goal;
            if walk.panic {
                walk.extra_distance = chain_extra_distance(points, walk.current);
            }
            let order = if has_next {
                ScriptOrder::MoveLeg(goal, false)
            } else {
                ScriptOrder::Move(goal, false)
            };
            self.issue_order_to(tick, ids, &[entity], order)?;
        }
        Ok(())
    }

    pub(super) fn step_wanderers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let places: Vec<Entity> = state.rt.wanderers.0.keys().copied().collect();
        for entity in places {
            let world = tick.state();
            if world.contains(ids.repulsed, entity) {
                state.rt.wanderers.forget(entity);
                continue;
            }
            let Some(place) = state.rt.wanderers.0.get_mut(&entity) else {
                continue;
            };
            let bits = place.origin;
            if world.is_live(entity) && !world.contains(ids.doomed, entity) {
                let hit = self.repulsor_check(world, ids, entity, &mut place.timer);
                if hit {
                    state.rt.wanderers.forget(entity);
                    self.begin_flee(tick, ids, entity)?;
                    continue;
                }
            }
            let world = tick.state();
            if !world.is_live(entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.path, entity)
                || world.contains(ids.attack_order, entity)
                || self.mood_asleep(world, ids, entity)
            {
                continue;
            }
            let cells = self.wander_cells(tick, ids, entity);
            if cells < 1 {
                continue;
            }
            let span = u32::try_from(cells.saturating_mul(2).saturating_add(1)).unwrap_or(1);
            let mut offset = [0.0_f32; 2];
            for axis in &mut offset {
                let drawn = i32::try_from(tick.rng().below(span)).unwrap_or(0);
                let steps = i16::try_from(drawn.saturating_sub(cells)).unwrap_or(0);
                *axis = f32::from(steps) * PATHFIND_CELL_SIZE;
            }
            let goal = Vec2::new(
                f32::from_bits(bits[0]) + offset[0],
                f32::from_bits(bits[1]) + offset[1],
            );
            self.issue_order_to(tick, ids, &[entity], ScriptOrder::Move(goal, false))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ground_movement;

    #[test]
    fn the_four_not_ground_answers_skip_the_panic_walks_node_sanity() {
        let rows = [
            (false, Some(false), false, true),
            (true, Some(false), false, false),
            (false, None, false, false),
            (false, Some(true), false, false),
            (false, Some(false), true, false),
        ];
        for (air_only, current, held, expected) in rows {
            assert_eq!(ground_movement(air_only, current, held), expected);
        }
    }
}
