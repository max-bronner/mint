use mint_movement::Vec2;
use mint_sage_formation::FormationBasis;
use mint_sim::{Entity, PlayerId, Tick};

use super::unit_index::{kept_by, BodyRecord};
use super::{Ids, RotwkRules};
use crate::goal_cells::{self, CellRect};

pub(super) const HEAVY_MELEE_HITTER: &str = "HEAVY_MELEE_HITTER";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DestinationAsker {
    pub(super) owner: Option<PlayerId>,
    pub(super) crusher: u8,
    pub(super) heavy: bool,
    pub(super) through_infantry: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GoalHolder {
    pub(super) owner: Option<PlayerId>,
    pub(super) standing: bool,
    pub(super) crushable: i16,
    pub(super) infantry: bool,
    pub(super) heavy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GoalVerdict {
    Clear,
    Ally,
    Refused,
}

#[derive(Debug, Clone, Copy, Default)]
struct Held {
    ally_goals: u32,
    ally_positions: u32,
    other_positions: u32,
    standing_other: bool,
    destination_allies: u32,
    destination_refused: bool,
    goals: u32,
    heavy_goals: u32,
    enemy_positions: u32,
    settled: u32,
    heavy_settled: u32,
}

pub(super) struct UnitCells {
    x0: i32,
    y0: i32,
    side: i32,
    cells: Vec<Held>,
    enemies: Vec<(Vec2, f32)>,
}

thread_local! {
    static SPARE: std::cell::RefCell<Vec<Vec<Held>>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn spare_cells(len: usize) -> Vec<Held> {
    let mut cells = SPARE
        .with(|spare| {
            spare
                .try_borrow_mut()
                .ok()
                .and_then(|mut spare| spare.pop())
        })
        .unwrap_or_default();
    cells.clear();
    cells.resize(len, Held::default());
    cells
}

impl Drop for UnitCells {
    fn drop(&mut self) {
        let cells = core::mem::take(&mut self.cells);
        SPARE.with(|spare| {
            if let Ok(mut spare) = spare.try_borrow_mut() {
                if spare.len() < 4 {
                    spare.push(cells);
                }
            }
        });
    }
}

impl UnitCells {
    fn at(&self, x: i32, y: i32) -> Option<&Held> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0 || dy < 0 || dx >= self.side || dy >= self.side {
            return None;
        }
        self.cells.get(usize::try_from(dy * self.side + dx).ok()?)
    }

    fn mark(&mut self, rect: CellRect, mut f: impl FnMut(&mut Held)) {
        let (x0, y0, x1, y1) = rect.span();
        for y in y0.max(self.y0)..y1.min(self.y0 + self.side) {
            for x in x0.max(self.x0)..x1.min(self.x0 + self.side) {
                let index = usize::try_from((y - self.y0) * self.side + (x - self.x0)).ok();
                if let Some(held) = index.and_then(|i| self.cells.get_mut(i)) {
                    f(held);
                }
            }
        }
    }

    pub(super) fn melee_count(&self, cells: impl Iterator<Item = (i32, i32)>) -> Option<u32> {
        let mut count = 0u32;
        for (x, y) in cells {
            let Some(held) = self.at(x, y) else {
                continue;
            };
            if held.standing_other {
                return None;
            }
            count = count
                .saturating_add(held.ally_goals)
                .saturating_add(held.ally_positions.saturating_mul(3))
                .saturating_add(held.other_positions);
        }
        Some(count)
    }

    pub(super) fn melee_spot_free(
        &self,
        mut cells: impl Iterator<Item = (i32, i32)>,
        heavy: bool,
    ) -> bool {
        cells.all(|(x, y)| {
            self.at(x, y).is_none_or(|held| {
                let goals = if heavy { held.heavy_goals } else { held.goals };
                goals == 0 && held.enemy_positions == 0
            })
        })
    }

    pub(super) fn walk_clear_of_enemies(&self, from: Vec2, to: Vec2, radius: f32) -> bool {
        let line = to.minus(from);
        let length_sq = line.length_squared();
        self.enemies.iter().all(|(them, theirs)| {
            let reach = radius + theirs;
            if them.minus(from).length() < reach {
                return true;
            }
            let t = if length_sq > 0.0 {
                (them.minus(from).dot(line) / length_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            from.plus(line.scale(t)).minus(*them).length() >= reach
        })
    }

    pub(super) fn nobody_settled(
        &self,
        mut cells: impl Iterator<Item = (i32, i32)>,
        heavy: bool,
    ) -> bool {
        cells.all(|(x, y)| {
            self.at(x, y).is_none_or(|held| {
                (if heavy {
                    held.heavy_settled
                } else {
                    held.settled
                }) == 0
            })
        })
    }

    pub(super) fn destination_clear(&self, cells: impl Iterator<Item = (i32, i32)>) -> bool {
        self.allied_goals(cells) == Some(0)
    }

    pub(super) fn allied_goals(&self, cells: impl Iterator<Item = (i32, i32)>) -> Option<u32> {
        let mut count = 0u32;
        for (x, y) in cells {
            let Some(held) = self.at(x, y) else {
                continue;
            };
            if held.destination_refused {
                return None;
            }
            count = count.saturating_add(held.destination_allies);
        }
        Some(count)
    }
}

impl RotwkRules {
    pub(super) fn unit_cells(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        asker: Entity,
        ignored: Option<Entity>,
        centre: Vec2,
        reach: f32,
    ) -> UnitCells {
        let _perf = crate::perf::sub(crate::perf::sub_row!("unit_cells"));
        let world = tick.state();
        let geometry = self.passability.geometry;
        let size = geometry.cell_size();
        #[allow(clippy::cast_possible_truncation)]
        let cell_of = |v: f32, o: f32| ((v - o) / size).floor().clamp(-1.0e6, 1.0e6) as i32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let half = (reach / size).ceil().clamp(1.0, 512.0) as i32;
        let x0 = cell_of(centre.x, geometry.origin().x) - half;
        let y0 = cell_of(centre.y, geometry.origin().y) - half;
        let side = 2 * half + 1;
        let mut map = UnitCells {
            x0,
            y0,
            side,
            cells: spare_cells(usize::try_from(side * side).unwrap_or(0)),
            enemies: Vec::new(),
        };
        let me = self.destination_asker(tick, ids, asker);
        let Some(_) = me.owner else {
            return map;
        };
        let reach_sq = {
            let r = reach + 2.0 * size * 5.0;
            r * r
        };
        let mut index = self.unit_index.try_borrow_mut().ok();
        let synced = index.as_mut().is_some_and(|index| {
            let _perf = crate::perf::sub(crate::perf::sub_row!("unit_index_sync"));
            index.sync(self, world, ids)
        });
        let indexed = index.as_mut().filter(|_| synced).and_then(|index| {
            let _perf = crate::perf::sub(crate::perf::sub_row!("unit_index_query"));
            index.query(centre, reach_sq)
        });
        let walked;
        let near = match indexed {
            Some(near) => {
                debug_assert_eq!(
                    format!("{near:?}"),
                    format!(
                        "{:?}",
                        self.bodies_near(world, ids, centre, reach_sq)
                            .iter()
                            .collect::<Vec<_>>()
                    ),
                    "the unit-cells index differs from the full walk round {centre:?}"
                );
                near
            }
            None => {
                walked = self.bodies_near(world, ids, centre, reach_sq);
                walked.iter().collect()
            }
        };
        self.fill_unit_cells(tick, ids, &mut map, me, asker, ignored, &near);
        map
    }

    pub(super) fn bodies_near(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        centre: Vec2,
        reach_sq: f32,
    ) -> Vec<BodyRecord> {
        let (Some(store), Some(bodies)) = (world.store(ids.shape), world.store(ids.body)) else {
            return Vec::new();
        };
        mint_sim::join(store, bodies)
            .filter(|(_, _, body)| kept_by(body.0.position, centre, reach_sq))
            .map(|(entity, shape, body)| BodyRecord::read(self, world, ids, entity, shape, body))
            .filter(|record| !record.left_out)
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_unit_cells(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        map: &mut UnitCells,
        me: DestinationAsker,
        asker: Entity,
        ignored: Option<Entity>,
        near: &[&BodyRecord],
    ) {
        let world = tick.state();
        let geometry = self.passability.geometry;
        let size = geometry.cell_size();
        let layer = goal_cells::layer(world, ids.goal_cells);
        let relation = self.relation_from(world, asker);
        let mut claim_poses: Vec<(Entity, Option<(Vec2, FormationBasis)>)> = Vec::new();
        for record in near {
            let entity = record.entity;
            if entity == asker
                || Some(entity) == ignored
                || record.member.is_some_and(|(horde, _)| horde == asker)
            {
                continue;
            }
            let position = record.position;
            let goal = layer
                .and_then(|layer| layer.held(entity))
                .map(|rect| rect.world(geometry).0)
                .or_else(|| world.get(ids.path, entity).map(|path| path.goal))
                .or_else(|| {
                    record.member.and_then(|(horde_entity, slot)| {
                        let at = match claim_poses.binary_search_by_key(&horde_entity, |(h, _)| *h)
                        {
                            Ok(at) => at,
                            Err(at) => {
                                let pose = self
                                    .claim_pose(tick, ids, horde_entity)
                                    .map(|pose| (pose.position, FormationBasis::of(pose.facing)));
                                claim_poses.insert(at, (horde_entity, pose));
                                at
                            }
                        };
                        let (centre, basis) = claim_poses.get(at).and_then(|(_, pose)| *pose)?;
                        let horde = world.get(ids.horde, horde_entity)?;
                        crate::horde::slot_pose_from(horde, centre, basis, usize::from(slot?))
                    })
                })
                .unwrap_or(position);
            let relation = relation.to_owned_by(entity, record.holder.owner);
            let ally = relation == super::map_scripts::Relation::Allies;
            let at_goal = goal.minus(position).length() <= size;
            let holder = GoalHolder {
                standing: at_goal,
                ..record.holder
            };
            let verdict = self.goal_verdict(me, holder);
            let heavy = u32::from(holder.heavy);
            let enemy = relation == super::map_scripts::Relation::Enemies;
            if enemy {
                map.enemies.push((position, record.radius));
            }
            let goal_rect = if goal == position {
                record.body_rect
            } else {
                super::unit_index::square(self, goal, record.footprint)
            };
            if let Some(rect) = goal_rect {
                map.mark(rect, |held| {
                    if ally {
                        held.ally_goals = held.ally_goals.saturating_add(1);
                    } else if at_goal {
                        held.standing_other = true;
                    }
                    match verdict {
                        GoalVerdict::Clear => {}
                        GoalVerdict::Ally => {
                            held.destination_allies = held.destination_allies.saturating_add(1);
                        }
                        GoalVerdict::Refused => held.destination_refused = true,
                    }
                    held.goals = held.goals.saturating_add(1);
                    held.heavy_goals = held.heavy_goals.saturating_add(heavy);
                    if at_goal {
                        held.settled = held.settled.saturating_add(1);
                        held.heavy_settled = held.heavy_settled.saturating_add(heavy);
                    }
                });
            }
            if let Some(rect) = record.body_rect {
                map.mark(rect, |held| {
                    if ally {
                        held.ally_positions = held.ally_positions.saturating_add(1);
                    } else {
                        held.other_positions = held.other_positions.saturating_add(1);
                    }
                    held.enemy_positions = held.enemy_positions.saturating_add(u32::from(enemy));
                });
            }
        }
    }

    pub(super) fn destination_asker(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> DestinationAsker {
        let facts = self.kind_facts(tick.state(), ids, entity);
        DestinationAsker {
            owner: self.owner_of(tick, ids, entity),
            crusher: self
                .record_of(tick, ids, entity)
                .map_or(0, |record| record.crush.crusher_level),
            heavy: facts.heavy_melee_hitter,
            through_infantry: facts.path_through_infantry,
        }
    }

    pub(super) fn goal_holder(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        standing: bool,
    ) -> GoalHolder {
        let world = tick.state();
        let entry = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind));
        Self::holder_from(world, ids, entity, entry, standing)
    }

    pub(super) fn holder_from(
        world: &mint_sim::World,
        ids: Ids,
        entity: Entity,
        entry: Option<&crate::templates::KindEntry>,
        standing: bool,
    ) -> GoalHolder {
        let facts = entry.map(|entry| entry.facts).unwrap_or_default();
        GoalHolder {
            owner: world.get(ids.owner, entity).map(|owner| owner.0),
            standing,
            crushable: entry.map_or(i16::MAX, |entry| {
                i16::from(entry.record.crush.crushable_level)
            }),
            infantry: facts.infantry,
            heavy: facts.heavy_melee_hitter,
        }
    }

    pub(super) fn goal_verdict(&self, asker: DestinationAsker, holder: GoalHolder) -> GoalVerdict {
        let ally = asker
            .owner
            .zip(holder.owner)
            .is_some_and(|(mine, theirs)| self.allied(mine, theirs));
        if ally {
            return if asker.heavy && !holder.heavy {
                GoalVerdict::Clear
            } else {
                GoalVerdict::Ally
            };
        }
        let crushed = i16::from(asker.crusher) > holder.crushable;
        let passed = asker.through_infantry && holder.infantry;
        if holder.standing && !crushed && !passed {
            GoalVerdict::Refused
        } else {
            GoalVerdict::Clear
        }
    }

    fn claim_pose(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde: Entity,
    ) -> Option<mint_movement::MoverState> {
        let world = tick.state();
        world.get(ids.horde, horde)?;
        let mut body = world.get(ids.body, horde)?.0;
        if let Some(path) = world.get(ids.path, horde) {
            let from = path.route.points.first().copied().unwrap_or(body.position);
            body.facing = claim_facing(from, path.goal, body.facing);
            body.position = path.goal;
        }
        Some(body)
    }
}

fn claim_facing(from: Vec2, goal: Vec2, facing: mint_movement::Facing) -> mint_movement::Facing {
    mint_movement::Facing::from_vector(goal.minus(from)).unwrap_or(facing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_claims_turn_to_the_bearing_of_the_destination_not_the_horde_facing() {
        let east = mint_movement::Facing::from_radians(0.0);
        let facing = claim_facing(Vec2::new(0.0, 0.0), Vec2::new(0.0, 100.0), east);
        assert!((facing.radians() - core::f32::consts::FRAC_PI_2).abs() < 1e-4);
    }
}
