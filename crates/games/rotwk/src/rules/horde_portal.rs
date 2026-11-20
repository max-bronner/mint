use mint_movement::{Facing, MotionLimits, MotionPhase, MotionStatus, MoverState, Route, Vec2};
use mint_sim::{Entity, Frame, Tick};

use super::{Ids, RotwkRules};
use crate::horde::PortalWalk;
use crate::rules::portal_links::{KIND_CLIMB, KIND_WALK};
use mint_movement::Motion;

const TELEPORT_RANGE: f32 = 10.0;
const WALK_PLACEMENT_SPEEDS: f32 = 3.0;
const CLIMB_TIMEOUT_FRAMES: u64 = 60;

pub(super) struct Staged {
    pub(super) motion: Motion,
    start: Vec<(Entity, PortalWalk)>,
    clear: Vec<Entity>,
}

impl Staged {
    pub(super) fn apply(self, tick: &mut Tick<'_>, ids: Ids) -> mint_sim::Result<()> {
        for member in self.clear {
            tick.state_mut().remove(ids.portal_walk, member);
        }
        for (member, walk) in self.start {
            tick.state_mut().insert(ids.portal_walk, member, walk)?;
        }
        Ok(())
    }
}

fn climb_points(
    here: Vec2,
    ahead: Vec2,
    (end_node, body): (Vec2, Vec2),
    off_ground: bool,
    slot: Option<Vec2>,
) -> Vec<Vec2> {
    let fourth = if off_ground {
        end_node.plus(body.minus(end_node).scale(0.5))
    } else {
        body
    };
    let mut points = vec![here, ahead, end_node];
    if slot != Some(fourth) {
        points.push(fourth);
    }
    points.extend(slot);
    points
}

impl RotwkRules {
    pub(super) fn stage_horde_portal(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        body: Entity,
        route: &Route,
        start: MoverState,
        limits: &MotionLimits,
    ) -> Option<Staged> {
        let horde = tick.state().get(ids.horde, body).cloned()?;
        let waypoint = start.waypoint;
        let tag = usize::try_from(waypoint)
            .ok()
            .and_then(|at| route.portals.get(at))
            .copied()
            .flatten()
            .filter(|tag| tag.leg != mint_movement::Leg::Through);
        let now = tick.frame();
        let walking: Vec<Entity> = horde
            .members
            .iter()
            .copied()
            .filter(|member| tick.state().contains(ids.portal_walk, *member))
            .collect();
        let staged = |state: MoverState,
                      phase: MotionPhase,
                      start: Vec<(Entity, PortalWalk)>,
                      clear: Vec<Entity>| {
            Some(Staged {
                motion: Motion {
                    state,
                    velocity: Vec2::ZERO,
                    status: MotionStatus::Moving,
                    phase,
                },
                start,
                clear,
            })
        };
        let Some(tag) = tag else {
            let walk_wait = walking.iter().any(|member| {
                tick.state()
                    .get(ids.portal_walk, *member)
                    .is_some_and(|walk| walk.until.is_none())
            });
            return if walk_wait {
                staged(start, MotionPhase::NONE, Vec::new(), Vec::new())
            } else {
                None
            };
        };
        if !walking.is_empty() {
            let waiting = walking.iter().any(|member| {
                tick.state()
                    .get(ids.portal_walk, *member)
                    .is_some_and(|walk| walk.until.is_none_or(|until| now < until))
            });
            if waiting {
                return staged(start, MotionPhase::NONE, Vec::new(), Vec::new());
            }
            let state = MoverState {
                waypoint: route.past_crossing(waypoint),
                ..start
            };
            return staged(state, MotionPhase::WALKING, Vec::new(), walking);
        }
        let at = |i: u32| {
            usize::try_from(i)
                .ok()
                .and_then(|i| route.points.get(i))
                .copied()
        };
        match tag.kind {
            KIND_WALK => {
                let past = route.past_crossing(waypoint);
                let collected: Vec<Vec2> = (waypoint..past).filter_map(at).collect();
                let started: Vec<(Entity, PortalWalk)> = if collected.is_empty() {
                    Vec::new()
                } else {
                    horde
                        .members
                        .iter()
                        .map(|member| {
                            (
                                *member,
                                PortalWalk {
                                    points: collected.clone(),
                                    next: 0,
                                    until: None,
                                },
                            )
                        })
                        .collect()
                };
                let (position, reached) = route.place_along(
                    start.position,
                    waypoint,
                    WALK_PLACEMENT_SPEEDS * limits.max_speed,
                );
                let state = MoverState {
                    position,
                    waypoint: reached.max(past),
                    ..start
                };
                staged(state, MotionPhase::WALKING, started, Vec::new())
            }
            KIND_CLIMB => {
                let target_index = if route
                    .portals
                    .get(
                        usize::try_from(waypoint)
                            .unwrap_or(usize::MAX)
                            .saturating_add(1),
                    )
                    .is_some_and(Option::is_some)
                {
                    waypoint.saturating_add(1)
                } else {
                    waypoint
                };
                let target = at(target_index)?;
                if start.position.minus(target).length() <= TELEPORT_RANGE {
                    let state = MoverState {
                        waypoint: route.past_crossing(waypoint),
                        ..start
                    };
                    return staged(state, MotionPhase::WALKING, Vec::new(), Vec::new());
                }
                let (ahead, _) = route.place_along(
                    start.position,
                    waypoint,
                    limits.max_speed * limits.frame.max(0.0),
                );
                let state = MoverState {
                    position: target,
                    waypoint: target_index,
                    ..start
                };
                let slots = crate::horde::slot_poses(&horde, &state);
                let end_node = at(waypoint)?;
                let off_ground = self.off_the_ground(tick.state(), ids, end_node);
                let until = Frame::new(now.get().saturating_add(CLIMB_TIMEOUT_FRAMES));
                let mut started = Vec::new();
                for member in &horde.members {
                    let Some(here) = tick.state().get(ids.body, *member).map(|b| b.0.position)
                    else {
                        continue;
                    };
                    let slot = tick
                        .state()
                        .get(ids.member, *member)
                        .and_then(|entry| entry.slot)
                        .and_then(|slot| slots.get(usize::from(slot)).copied());
                    let points = climb_points(here, ahead, (end_node, target), off_ground, slot);
                    started.push((
                        *member,
                        PortalWalk {
                            points,
                            next: 1,
                            until: Some(until),
                        },
                    ));
                }
                staged(state, MotionPhase::WALKING, started, Vec::new())
            }
            _ => None,
        }
    }

    fn off_the_ground(&self, world: &mint_sim::World, ids: Ids, point: Vec2) -> bool {
        let extent = self.passability.grid.extent();
        self.passability
            .geometry
            .cell_at(point, 0, extent)
            .zip(crate::wall_layer::layer_of(world, ids.wall_layer))
            .is_some_and(|(cell, layer)| layer.tag_at(extent, cell).is_wall_top())
    }

    pub(super) fn advance_portal_walkers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
    ) -> mint_sim::Result<()> {
        let walkers: Vec<Entity> = tick
            .state()
            .store(ids.portal_walk)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let dt = Self::tick_seconds();
        for member in walkers {
            let (Some(mut walk), Some(mut body)) = (
                tick.state().get(ids.portal_walk, member).cloned(),
                tick.state().get(ids.body, member).copied(),
            ) else {
                continue;
            };
            let speed = crate::unit::current_limits(
                tick.state(),
                &self.kinds,
                (ids.kind, ids.locomotion, ids.health),
                member,
                None,
                1.0,
            )
            .unwrap_or(crate::unit::PLACEHOLDER_LIMITS)
            .max_speed;
            let mut left = speed.max(0.0) * dt;
            while left > 0.0 {
                let Some(target) = walk.points.get(walk.next).copied() else {
                    break;
                };
                let to = target.minus(body.0.position);
                let length = to.length();
                if length <= left {
                    body.0.position = target;
                    left -= length;
                    walk.next = walk.next.saturating_add(1);
                } else {
                    body.0.position = body.0.position.plus(to.scale(left / length));
                    if let Some(facing) = Facing::from_vector(to) {
                        body.0.facing = facing;
                    }
                    left = 0.0;
                }
            }
            tick.state_mut().insert(ids.body, member, body)?;
            if walk.next >= walk.points.len() {
                tick.state_mut().remove(ids.portal_walk, member);
            } else {
                tick.state_mut().insert(ids.portal_walk, member, walk)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_climb_path_is_here_ahead_end_node_fourth_slot() {
        let (here, ahead) = (Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0));
        let (node, body) = (Vec2::new(10.0, 0.0), Vec2::new(30.0, 0.0));
        let slot = Some(Vec2::new(32.0, 1.0));
        assert_eq!(
            climb_points(here, ahead, (node, body), false, slot),
            [here, ahead, node, body, Vec2::new(32.0, 1.0)]
        );
        assert_eq!(
            climb_points(here, ahead, (node, body), true, slot),
            [
                here,
                ahead,
                node,
                Vec2::new(20.0, 0.0),
                Vec2::new(32.0, 1.0)
            ]
        );
        assert_eq!(
            climb_points(here, ahead, (node, body), false, Some(body)),
            [here, ahead, node, body]
        );
    }
}
