use std::collections::BTreeSet;

use super::*;

use mint_movement::{CellFlags, GridExtent, Passability};
use mint_sage_formation::{GroupMember, GroupSpacing, NO_FORMATION};
use mint_sage_passability::{CLIFF, DEEP_WATER, OBSTACLE as OBSTACLE_CELL, PINCHED, RUBBLE, WATER};

use crate::goal_cells::{self, Footprint};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroupFormation {
    pub(crate) spacing: GroupSpacing,
    pub(crate) use_formations: bool,
    pub(crate) attack_move_uses_formations: bool,
    pub(crate) hordes_wait_for_hordes: bool,
    pub(crate) click_to_gather_area_factor: f32,
    pub(crate) narrow_passage_scale: f32,
    pub(crate) min_distance_for_group: f32,
    pub(crate) skirmish_group_fudge: f32,
    pub(crate) force_skirmish_ai: bool,
    pub(crate) formation_enemy_distance: f32,
    pub(crate) planning_mode_enabled: bool,
}

impl Default for GroupFormation {
    fn default() -> Self {
        Self {
            spacing: GroupSpacing {
                column_width: 60.0,
                row_depth: 55.0,
                squad_spacing: 30.0,
                columns: 2,
            },
            use_formations: true,
            attack_move_uses_formations: true,
            hordes_wait_for_hordes: true,
            click_to_gather_area_factor: 0.5,
            narrow_passage_scale: 0.4,
            min_distance_for_group: 200.0,
            skirmish_group_fudge: 0.0,
            force_skirmish_ai: false,
            formation_enemy_distance: 500.0,
            planning_mode_enabled: true,
        }
    }
}

const RECENT_DAMAGE_TICKS: u64 = 4 * SIMULATION_HZ as u64;

pub(super) struct FormationOrder {
    pub(super) points: BTreeMap<Entity, Vec<Vec2>>,
    pub(super) facing: Facing,
    pub(super) slots: BTreeMap<Entity, Vec2>,
}

impl FormationOrder {
    pub(super) fn destinations(&self) -> BTreeMap<Entity, Vec2> {
        self.points
            .iter()
            .filter_map(|(entity, points)| Some((*entity, *points.last()?)))
            .collect()
    }

    pub(super) fn via(&self, entity: Entity) -> &[Vec2] {
        self.points
            .get(&entity)
            .and_then(|points| points.split_last())
            .map_or(&[], |(_, via)| via)
    }
}

struct Scanned {
    position: Vec2,
    aircraft: bool,
    held: bool,
}

fn move_reaches_min_distance(min_distance: f32, members: &[Scanned], goal: Vec2) -> bool {
    let cap = 4.0 * min_distance;
    let mut squared = members
        .iter()
        .filter(|m| !m.held && !m.aircraft)
        .map(|m| m.position.minus(goal).length_squared())
        .fold(cap * cap, f32::min);
    let mut bounds: Option<(Vec2, Vec2)> = None;
    for m in members.iter().filter(|m| !m.held) {
        let p = m.position;
        bounds = Some(match bounds {
            None => (p, p),
            Some((lo, hi)) => (
                Vec2::new(lo.x.min(p.x), lo.y.min(p.y)),
                Vec2::new(hi.x.max(p.x), hi.y.max(p.y)),
            ),
        });
    }
    if let Some((lo, hi)) = bounds {
        let diagonal = hi.minus(lo).length_squared();
        if diagonal > cap * cap {
            squared = diagonal;
        }
    }
    squared >= min_distance * min_distance
}

pub(super) fn refuses_by_height(
    height: f32,
    rise: f32,
    to_container: Vec2,
    to_target: Vec2,
) -> bool {
    rise <= height && to_container.dot(to_target) >= 0.0
}

const SPAWN_DEPTH: u8 = 4;

impl RotwkRules {
    pub(super) fn planning_mode_enabled(&self, world: &World, ids: Ids) -> bool {
        world
            .store(ids.map_script_state)
            .and_then(|store| store.iter().next())
            .and_then(|(_, state)| state.ui.planning_mode())
            .unwrap_or(self.group_formation.planning_mode_enabled)
    }

    pub(super) fn command_move_path(
        &self,
        world: &World,
        ids: Ids,
        player: PlayerId,
    ) -> busy::MovePath {
        if !self.is_computer(player) && self.planning_mode_enabled(world, ids) {
            busy::MovePath::PerMember
        } else if self.is_computer(player) {
            busy::MovePath::Group
        } else {
            busy::MovePath::PlayerGroup
        }
    }

    pub(super) fn group_move_tightens(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        goal: Vec2,
    ) -> bool {
        let factor = self.group_formation.click_to_gather_area_factor;
        if factor <= 0.0 || movers.len() < 2 {
            return false;
        }
        let mut positions = movers
            .iter()
            .filter_map(|entity| tick.state().get(ids.body, *entity).map(|b| b.0.position));
        let Some(first) = positions.next() else {
            return false;
        };
        let (mut min, mut max) = (first, first);
        for p in positions {
            min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
            max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
        }
        let centre = min.plus(max).scale(0.5);
        let half = max.minus(min).scale(0.5 * factor);
        let (scaled_min, scaled_max) = (centre.minus(half), centre.plus(half));
        let inside = goal.x >= scaled_min.x
            && goal.x <= scaled_max.x
            && goal.y >= scaled_min.y
            && goal.y <= scaled_max.y;
        if !inside {
            return false;
        }
        if movers
            .iter()
            .any(|entity| self.flies_itself(tick.state(), ids, *entity))
        {
            return false;
        }
        let cell_size = self.passability.geometry.cell_size();
        let width_cells = (max.x - min.x) / cell_size;
        let height_cells = (max.y - min.y) / cell_size;
        width_cells * height_cells < 2000.0
    }

    pub(super) fn formation_order(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        goal: Vec2,
        pipeline: &mut Pipeline,
    ) -> Result<Option<FormationOrder>> {
        if movers.len() < 2 || !self.group_formation.use_formations {
            return Ok(None);
        }
        let members: Vec<(Entity, GroupMember)> = movers
            .iter()
            .filter_map(|entity| Some((*entity, self.group_member(tick, ids, *entity)?)))
            .collect();
        if !members
            .iter()
            .any(|(_, m)| m.priority.is_some_and(|p| p != NO_FORMATION))
        {
            return Ok(None);
        }
        if !self.move_is_long_enough_to_form(tick, ids, &members, goal) {
            return Ok(None);
        }
        let Some(representative) = self.representative(tick, ids, &members) else {
            return Ok(None);
        };
        if self.enemy_near_representative(tick, ids, representative) {
            return Ok(None);
        }
        let Some(nodes) = self.group_path(tick, ids, &members, goal, pipeline)? else {
            return Ok(None);
        };
        if nodes.iter().any(|node| self.narrow_at(*node)) {
            return Ok(None);
        }
        let (Some(&from), Some(&to), Some(&end)) = (nodes.first(), nodes.get(1), nodes.last())
        else {
            return Ok(None);
        };
        let group: Vec<GroupMember> = members.iter().map(|(_, m)| *m).collect();
        let spacing = self.group_formation.spacing;
        let slots = mint_sage_formation::lay_out(&group, from, to, spacing);

        let row_depth = spacing.row_depth;
        #[allow(clippy::cast_precision_loss)]
        let reach = members
            .iter()
            .zip(&slots)
            .map(|((_, m), slot)| m.depth as f32 * row_depth - slot.x)
            .fold(0.0, f32::max);
        let now = tick.frame().get();
        let recently_damaged = members.iter().any(|(entity, _)| {
            tick.state()
                .get(
                    ids.last_attacker,
                    Self::commanded_unit(tick.state(), ids, *entity),
                )
                .is_some_and(|last| now.saturating_sub(last.at.get()) < RECENT_DAMAGE_TICKS)
        });
        let form_up = form_up_distance(row_depth, reach, recently_damaged);
        let walk = Walk::along(&nodes, form_up, 2.0 * row_depth);
        let own: Vec<Entity> = members.iter().map(|(entity, _)| *entity).collect();
        let idle = self.idle_unit_cells(tick, ids, &own);
        let ground_view = self.dynamic_view(tick, ids);
        let adjust = |anchor: Vec2, p: Vec2| {
            self.adjust_ground_path_position(&ground_view, &idle, anchor, p)
        };
        let start = walk.start;
        let facing_from = walk
            .end_node
            .and_then(|i| nodes.get(i))
            .copied()
            .unwrap_or(from);
        let facing = Facing::from_vector(end.minus(facing_from))
            .or_else(|| Facing::from_vector(end.minus(from)))
            .unwrap_or(Facing::from_radians(0.0));

        let mut points = BTreeMap::new();
        let mut offsets = BTreeMap::new();
        for ((entity, member), slot) in members.iter().zip(&slots) {
            let Some(position) = tick.state().get(ids.body, *entity).map(|b| b.0.position) else {
                continue;
            };
            let mover = self.mover_class(tick, ids, *entity);
            let use_form_point = walk.form_point.is_some_and(|f| {
                self.line_clear(tick, ids, mover, position, start)
                    || self.line_clear(tick, ids, mover, position, f)
            });
            let narrow_scale = |point: Vec2| {
                if self.narrow_at(point) {
                    self.group_formation.narrow_passage_scale
                } else {
                    1.0
                }
            };
            let (mut list, previous) =
                walk.member_points(&nodes, *slot, use_form_point, &narrow_scale, &adjust);
            let destination = if member.priority == Some(NO_FORMATION) {
                end
            } else {
                offsets.insert(*entity, *slot);
                let slot = slot.scale(narrow_scale(end));
                let rotated = mint_sage_formation::rotate_offset(previous, end, slot);
                adjust(end, end.plus(rotated))
            };
            list.push(if self.passability.cell_at(destination).is_some() {
                destination
            } else {
                end
            });
            points.insert(*entity, list);
        }
        crate::diag::diag!(
            "formation: {} movers, path {nodes:?}, final facing {:.1} deg",
            members.len(),
            facing.radians().to_degrees()
        );
        for ((entity, member), slot) in members.iter().zip(&slots) {
            crate::diag::diag!(
                "formation: entity {} class {:?} width {} depth {} slot ({:.2}, {:.2}) points {:?}",
                entity.index(),
                member.priority,
                member.width,
                member.depth,
                slot.x,
                slot.y,
                points.get(entity)
            );
        }
        Ok(Some(FormationOrder {
            points,
            facing,
            slots: offsets,
        }))
    }

    pub(in crate::rules) fn column_slots(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        from: Vec2,
        to: Vec2,
    ) -> Vec<(Entity, Vec2)> {
        let members: Vec<(Entity, GroupMember)> = movers
            .iter()
            .filter(|&&entity| self.move_refused(tick, ids, entity).is_none())
            .filter_map(|&entity| Some((entity, self.group_member(tick, ids, entity)?)))
            .collect();
        let group: Vec<GroupMember> = members.iter().map(|(_, m)| *m).collect();
        let slots = mint_sage_formation::lay_out(&group, from, to, self.group_formation.spacing);
        members
            .iter()
            .zip(slots)
            .map(|((entity, member), slot)| {
                let offset = if member.priority == Some(NO_FORMATION) {
                    Vec2::ZERO
                } else {
                    slot
                };
                (*entity, offset)
            })
            .collect()
    }

    pub(super) fn record_formation_slots(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        formation: Option<&FormationOrder>,
    ) -> Result<()> {
        for &mover in movers {
            match formation.and_then(|order| order.slots.get(&mover)) {
                Some(&offset) => {
                    tick.state_mut().insert(
                        ids.formation_slot,
                        mover,
                        crate::unit::FormationSlot(offset),
                    )?;
                }
                None => {
                    tick.state_mut().remove(ids.formation_slot, mover);
                }
            }
        }
        Ok(())
    }

    fn move_is_long_enough_to_form(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        members: &[(Entity, GroupMember)],
        goal: Vec2,
    ) -> bool {
        let members: Vec<Scanned> = members
            .iter()
            .map(|(entity, m)| Scanned {
                position: m.position,
                aircraft: self.has_kind_bit(tick.state(), ids, *entity, "AIRCRAFT"),
                held: tick.state().contains(ids.held, *entity),
            })
            .collect();
        move_reaches_min_distance(self.group_formation.min_distance_for_group, &members, goal)
    }

    fn representative(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        members: &[(Entity, GroupMember)],
    ) -> Option<Entity> {
        let world = tick.state();
        let has = |entity: Entity, bit: &str| self.has_kind_bit(world, ids, entity, bit);
        let counted = || {
            members
                .iter()
                .map(|(entity, _)| *entity)
                .filter(|entity| !world.contains(ids.held, *entity) && !has(*entity, "AIRCRAFT"))
        };
        counted()
            .filter(|e| has(*e, "MONSTER"))
            .next_back()
            .or_else(|| {
                counted()
                    .filter(|e| has(*e, "CAVALRY") || has(*e, "MACHINE"))
                    .next_back()
            })
            .or_else(|| counted().filter(|e| has(*e, "INFANTRY")).next_back())
    }

    fn enemy_near_representative(&self, tick: &Tick<'_>, ids: Ids, representative: Entity) -> bool {
        let world = tick.state();
        let horde = Self::horde_ids(ids);
        let me = world
            .get(horde.horde, representative)
            .and_then(|horde| horde.members.first().copied())
            .unwrap_or(representative);
        if world.contains(ids.no_auto_acquire, representative)
            || world.contains(ids.no_auto_acquire, me)
        {
            return false;
        }
        let Some(owner) = world.get(ids.owner, me).map(|owner| owner.0) else {
            return false;
        };
        let Some(body) = world.get(ids.body, representative) else {
            return false;
        };
        let position = body.0.position;
        let reach = self.group_formation.formation_enemy_distance;
        let relation = self.relation_from(world, me);
        let computer = self.computer_controlled(owner);
        let arc = self
            .template_firing_arc(world, ids, me)
            .filter(|arc| *arc < 360.0)
            .map(|arc| (body.0.facing.radians(), arc * core::f32::consts::PI / 360.0));
        self.living(tick, ids).iter().any(|(foe, _)| {
            let range = reach + foe.radius;
            let offset = foe.position.minus(position);
            if offset.length_squared() > range * range {
                return false;
            }
            if let Some((facing, half)) = arc {
                let bearing = mint_math::f32::atan2(offset.y, offset.x);
                if mint_math::f32::angle_delta(facing, bearing).abs() > half {
                    return false;
                }
            }
            let has = |bit: &str| self.has_kind_bit(world, ids, foe.entity, bit);
            if has("ROCK_VENDOR") || self.carries_object_status(world, foe.entity, "FLEE_OFF_MAP") {
                return false;
            }
            if foe.is_structure && self.insignificant_building(world, ids, foe.entity) {
                return false;
            }
            let mut admitted = *foe;
            admitted.not_autoacquirable &= !(computer && has("REBUILD_HOLE"));
            let foe = &admitted;
            let relation = relation.to(foe.entity).victim();
            crate::combat::auto_acquirable(relation)
                && mint_sage_combat::can_target(
                    mint_sage_combat::Attacker {
                        relation,
                        class_allowed: true,
                        scans_structures: computer
                            || has("MINE")
                            || has("FS_BASE_DEFENSE")
                            || (self.has_contain(world, foe.entity)
                                && self.object_able_to_attack(world, ids, foe.entity)),
                    },
                    self.scanned_victim(world, ids, owner, foe),
                    mint_sage_combat::Mode::AutoAcquire,
                )
        })
    }

    fn template_firing_arc(&self, world: &World, ids: Ids, entity: Entity) -> Option<f32> {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.firing_arc)
    }

    pub(super) fn carries_object_status(
        &self,
        world: &World,
        entity: Entity,
        status: &str,
    ) -> bool {
        self.modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(world, entity, status))
    }

    fn has_contain(&self, world: &World, entity: Entity) -> bool {
        self.contain_view(world, entity).is_some()
    }

    pub(super) fn faction_structure(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        const KINDOFMASK_FS: [&str; 5] = [
            "FS_POWER",
            "FS_FACTORY",
            "FS_BASE_DEFENSE",
            "FS_TECHNOLOGY",
            "FS_CASH_PRODUCER",
        ];
        KINDOFMASK_FS
            .iter()
            .any(|bit| self.has_kind_bit(world, ids, entity, bit))
    }

    fn insignificant_building(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        let faction = self.faction_structure(world, ids, entity);
        if faction || self.has_ai(world, ids, entity) || !self.has_contain(world, entity) {
            return false;
        }
        let garrison = self
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.is_garrison(world, entity));
        let occupied = world
            .get(ids.passengers, entity)
            .is_some_and(|held| !held.0.is_empty());
        !garrison || !occupied
    }

    pub(super) fn object_able_to_attack(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.able_to_attack_within(world, ids, entity, SPAWN_DEPTH)
    }

    fn able_to_attack_within(&self, world: &World, ids: Ids, entity: Entity, depth: u8) -> bool {
        if world.contains(ids.no_attack, entity) || self.holds_condition_in(world, entity, "STONED")
        {
            return false;
        }
        let held = world.get(ids.contained, entity).or_else(|| {
            let horde = world.get(ids.member, entity)?.horde;
            world.get(ids.contained, horde)
        });
        if let Some(held) = held {
            if !self.carries_object_status(world, entity, "CONTESTING_BUILDING")
                && !held.status.has(super::contain::ContainStatus::CAN_ATTACK)
            {
                return false;
            }
            if self.collision_height_refuses(world, ids, entity, held.container) {
                return false;
            }
        }
        if world.contains(ids.construction, entity) || world.contains(ids.sold, entity) {
            return false;
        }
        self.has_kind_bit(world, ids, entity, "CAN_ATTACK")
            || self.carries_object_status(world, entity, "CAN_ATTACK")
            || (self.lets_passengers_fire(world, entity)
                && world
                    .get(ids.passengers, entity)
                    .is_some_and(|held| !held.0.is_empty()))
            || (self.has_ai(world, ids, entity) && world.contains(ids.armed, entity))
            || (depth > 0
                && self
                    .modules
                    .as_ref()
                    .and_then(|wiring| wiring.spawn_behavior(world, entity))
                    .is_some_and(|spawner| {
                        spawner.spawns().iter().any(|&slave| {
                            world.is_live(slave)
                                && self.able_to_attack_within(
                                    world,
                                    ids,
                                    slave,
                                    depth.saturating_sub(1),
                                )
                        })
                    }))
    }

    fn collision_height_refuses(
        &self,
        world: &World,
        ids: Ids,
        passenger: Entity,
        container: Entity,
    ) -> bool {
        let Some(height) = self
            .contain_view(world, container)
            .map(|view| view.open.passengers_test_collision_height())
            .filter(|height| *height >= 0.0)
        else {
            return false;
        };
        let Some(target) = world
            .get(ids.attack_order, passenger)
            .map(|order| order.target)
        else {
            return false;
        };
        let (Some(p), Some(c), Some(t)) = (
            world.get(ids.body, passenger),
            world.get(ids.body, container),
            world.get(ids.body, target),
        ) else {
            return false;
        };
        let z = |entity: Entity, at: Vec2| {
            world
                .get(ids.elevation, entity)
                .map_or_else(|| self.ground_height(at.x, at.y), |e| e.0)
        };
        let (p, c, t) = (p.0.position, c.0.position, t.0.position);
        refuses_by_height(
            height,
            z(passenger, p) - z(container, c),
            c.minus(p),
            t.minus(p),
        )
    }

    fn group_member(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<GroupMember> {
        let position = tick.state().get(ids.body, entity)?.0.position;
        let kind = tick.state().get(ids.kind, entity).copied();
        let entry = kind.and_then(|kind| self.kinds.get(kind));
        let units = |value: Option<f32>| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            value
                .filter(|v| v.is_finite())
                .map_or(1, |v| v.round().clamp(0.0, 64.0) as u32)
        };
        Some(GroupMember {
            position,
            priority: entry
                .and_then(|entry| entry.flight)
                .map(|flight| flight.formation_priority),
            width: units(entry.and_then(|entry| entry.record.formation_width)),
            depth: units(entry.and_then(|entry| entry.record.formation_depth)),
        })
    }

    fn group_path(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        members: &[(Entity, GroupMember)],
        goal: Vec2,
        pipeline: &mut Pipeline,
    ) -> Result<Option<Vec<Vec2>>> {
        let world = tick.state();
        #[allow(clippy::cast_precision_loss)]
        let count = members.len() as f32;
        let mut centre = members
            .iter()
            .fold(Vec2::ZERO, |sum, (_, m)| sum.plus(m.position))
            .scale(1.0 / count);
        let grounded: Vec<&(Entity, GroupMember)> = members
            .iter()
            .filter(|(entity, _)| !self.has_kind_bit(world, ids, *entity, "AIRCRAFT"))
            .collect();
        let nearest = grounded.iter().min_by(|(a, ma), (b, mb)| {
            let d = |m: &GroupMember| m.position.minus(centre).length_squared();
            d(ma).total_cmp(&d(mb)).then(a.index().cmp(&b.index()))
        });
        if let Some((entity, member)) = nearest {
            let mover = self.mover_class(tick, ids, *entity);
            if !self.line_clear(tick, ids, mover, member.position, centre) {
                centre = member.position;
            }
        }
        let Some(representative) = self.representative(tick, ids, members) else {
            return Ok(None);
        };
        let representative = &representative;
        let mover = self.mover_class(tick, ids, *representative);
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let (Some(start), Some(raw_goal_cell)) = (
            self.start_cell_toward(tick, ids, centre, goal, mover),
            self.passable_cell_for(tick, ids, goal, mover),
        ) else {
            return Ok(None);
        };
        let own: Vec<Entity> = members.iter().map(|(entity, _)| *entity).collect();
        let idle = self.idle_unit_cells(tick, ids, &own);
        let base_view = self.dynamic_view(tick, ids);
        let cost_view =
            self.unit_cost_view_for(tick, ids, *representative, &base_view, Footprint::EIGHT);
        let view = GroundPathView {
            inner: &cost_view,
            extent,
            idle: &idle,
        };
        let goal_cell = if clear_diameter(&view, extent, &idle, raw_goal_cell, 4) {
            raw_goal_cell
        } else {
            let Some(probed) = goal_probe(&view, extent, &idle, raw_goal_cell) else {
                return Ok(None);
            };
            probed
        };
        let request = PathRequest {
            start,
            goal: goal_cell,
            mover,
            goal_id: GoalId(u64::from(representative.index())),
            origin: Some(centre),
            corner_radius: 0.0,
            departure: None,
            approach: None,
            prune: false,
            final_goal: true,
        };
        let mut budget = NodeBudget::new(super::movement::PATH_NODE_BUDGET);
        let refuse = |cell: Cell| refuses_group_cell(view.flags(cell));
        let outcome = pipeline
            .plan_first_visit(&view, geometry, &request, &mut budget, &refuse)
            .map_err(|e| mint_sim::Error::Rules {
                frame: tick.frame().get(),
                reason: format!("planning a group path: {e}"),
            })?;
        let PlanOutcome::Ready(plan) = outcome else {
            return Ok(None);
        };
        let straightened = straighten_group_path(&view, mover, &plan.cells.cells);
        let inner = straightened.len().saturating_sub(1);
        let mut nodes = vec![centre];
        nodes.extend(
            straightened
                .iter()
                .take(inner)
                .skip(1)
                .map(|cell| geometry.centre_of(*cell)),
        );
        nodes.push(goal);
        Ok(Some(nodes))
    }

    fn idle_unit_cells(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        exclude: &[Entity],
    ) -> BTreeSet<(i32, i32)> {
        let world = tick.state();
        let mut cells = BTreeSet::new();
        let Some(store) = world.store(ids.shape) else {
            return cells;
        };
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        for (entity, shape) in store.iter() {
            if exclude.contains(&entity)
                || world.contains(ids.horde, entity)
                || world.contains(ids.member, entity)
                || shape.kind_of.contains(super::KindOf::IMMOBILE)
                || world.contains(ids.flight, entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.concealed, entity)
                || world.contains(ids.path, entity)
                || self
                    .crushable_level(tick, ids, entity)
                    .is_none_or(|level| level <= 0)
            {
                continue;
            }
            let Some(body) = world.get(ids.body, entity) else {
                continue;
            };
            let footprint = self.footprint_of(tick, ids, entity);
            let Some(rect) = goal_cells::rect(
                extent,
                goal_cells::anchor(geometry, body.0.position, footprint),
                footprint,
            ) else {
                continue;
            };
            let (x0, y0, x1, y1) = rect.span();
            for y in y0..y1 {
                for x in x0..x1 {
                    cells.insert((x, y));
                }
            }
        }
        cells
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn plan_via(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        pipeline: &mut Pipeline,
        entity: Entity,
        mover: MoverClass,
        from: (Cell, Vec2),
        via: &[Vec2],
        follow_path: bool,
    ) -> Result<(Cell, Vec2, Vec<Vec2>)> {
        let (mut cell, mut origin) = from;
        let mut points: Vec<Vec2> = Vec::new();
        for (index, point) in via.iter().enumerate() {
            let Some(goal) = self.passable_cell_for(tick, ids, *point, mover) else {
                continue;
            };
            let request = PathRequest {
                start: cell,
                goal,
                mover,
                goal_id: GoalId(u64::from(entity.index())),
                origin: Some(origin),
                corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
                departure: self.departure(tick, ids, entity),
                approach: None,
                prune: false,
                final_goal: !(follow_path && index > 0),
            };
            let mut budget = NodeBudget::new(super::movement::PATH_NODE_BUDGET);
            let view = self.dynamic_view(tick, ids);
            let view = self
                .unit_cost_view(tick, ids, entity, &view)
                .exempting(&[goal]);
            let outcome = pipeline
                .plan_closest_accepting(
                    &view,
                    self.passability.geometry,
                    &self.stepped_back(&request),
                    &mut budget,
                    &self.footprint_accepts(tick, ids, entity),
                    0.0,
                )
                .map_err(|e| mint_sim::Error::Rules {
                    frame: tick.frame().get(),
                    reason: format!(
                        "planning a formation leg for entity {}: {e}",
                        entity.index()
                    ),
                })?;
            let (plan, exact) = match outcome {
                PlanOutcome::Ready(plan) => (plan, true),
                PlanOutcome::Closest(plan) => (plan, false),
                _ => continue,
            };
            let mut leg = plan.route.points;
            if exact && self.passability.cell_at(*point) == Some(goal) {
                if let Some(last) = leg.last_mut() {
                    *last = *point;
                }
            }
            let (Some(&end), Some(&end_cell)) = (leg.last(), plan.cells.cells.last()) else {
                continue;
            };
            let skip = usize::from(!points.is_empty());
            points.extend(leg.into_iter().skip(skip));
            cell = end_cell;
            origin = end;
        }
        Ok((cell, origin, points))
    }

    pub(super) fn narrow_at(&self, point: Vec2) -> bool {
        self.passability.cell_at(point).is_some_and(|cell| {
            self.passability
                .grid
                .flags(cell)
                .intersects(mint_sage_passability::NARROW_PASSAGE)
        })
    }

    pub(super) fn line_clear(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: MoverClass,
        from: Vec2,
        to: Vec2,
    ) -> bool {
        let (Some(a), Some(b)) = (self.passability.cell_at(from), self.passability.cell_at(to))
        else {
            return false;
        };
        let view = self.dynamic_view(tick, ids);
        mint_movement::smoothing::line_is_passable(&view, mover, a, b)
    }

    pub(super) fn adjust_ground_path_position(
        &self,
        view: &dyn Passability,
        idle: &BTreeSet<(i32, i32)>,
        anchor: Vec2,
        p: Vec2,
    ) -> Vec2 {
        adjust_ground_path_position(
            view,
            self.passability.geometry,
            self.passability.grid.extent(),
            idle,
            anchor,
            p,
        )
    }
}

fn adjust_ground_path_position(
    view: &dyn Passability,
    geometry: mint_movement::CellGeometry,
    extent: GridExtent,
    idle: &BTreeSet<(i32, i32)>,
    anchor: Vec2,
    p: Vec2,
) -> Vec2 {
    let (Some(from), Some(to)) = (
        geometry.cell_at(anchor, 0, extent),
        geometry.cell_at(p, 0, extent),
    ) else {
        return p;
    };
    if from == to {
        return p;
    }
    let cells = bresenham_cells(from, to);
    let mut last_clear: Option<Cell> = None;
    for cell in &cells {
        if clear_diameter(view, extent, idle, *cell, 4) {
            last_clear = Some(*cell);
        } else {
            break;
        }
    }
    match last_clear {
        None => anchor,
        Some(cell) if cells.last() == Some(&cell) => p,
        Some(cell) => geometry.centre_of(cell),
    }
}

fn clear_diameter(
    view: &dyn Passability,
    extent: GridExtent,
    idle: &BTreeSet<(i32, i32)>,
    cell: Cell,
    d: u16,
) -> bool {
    let r = i32::from(d / 2);
    let (lo, hi) = if r == 0 {
        (0, 0)
    } else {
        (-r, r.saturating_sub(1))
    };
    let cx = i32::from(cell.x);
    let cy = i32::from(cell.y);
    for dy in lo..=hi {
        for dx in lo..=hi {
            if r > 1 && (dx == lo || dx == hi) && (dy == lo || dy == hi) {
                continue;
            }
            let (Ok(nx), Ok(ny)) = (
                u16::try_from(cx.saturating_add(dx)),
                u16::try_from(cy.saturating_add(dy)),
            ) else {
                return false;
            };
            let n = Cell::new(nx, ny);
            if !extent.contains(n) {
                return false;
            }
            if view.flags(n).intersects(GROUND.blocked_by) {
                return false;
            }
            if d >= 2 && idle.contains(&(i32::from(n.x), i32::from(n.y))) {
                return false;
            }
        }
    }
    true
}

fn clear_width(
    view: &dyn Passability,
    extent: GridExtent,
    idle: &BTreeSet<(i32, i32)>,
    cell: Cell,
) -> u16 {
    if clear_diameter(view, extent, idle, cell, 4) {
        4
    } else if clear_diameter(view, extent, idle, cell, 2) {
        2
    } else {
        0
    }
}

struct GroundPathView<'a> {
    inner: &'a dyn Passability,
    extent: GridExtent,
    idle: &'a BTreeSet<(i32, i32)>,
}

impl Passability for GroundPathView<'_> {
    fn extent(&self) -> GridExtent {
        self.inner.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        self.inner.flags(cell)
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        let clear = u32::from(clear_width(self.inner, self.extent, self.idle, cell));
        self.inner
            .extra_cost(cell)
            .saturating_add(6u32.saturating_mul(4u32.saturating_sub(clear)))
    }

    fn goal_extra_cost(&self, cell: Cell, goal: Option<Cell>) -> u32 {
        self.inner.goal_extra_cost(cell, goal)
    }

    fn tail_clear(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.tail_clear(cell, mover)
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }

    fn is_passable(&self, cell: Cell, mover: MoverClass) -> bool {
        self.inner.is_passable(cell, mover) && !self.inner.flags(cell).intersects(PINCHED)
    }

    fn line_clear(&self, cell: Cell, _mover: MoverClass) -> bool {
        clear_diameter(self.inner, self.extent, self.idle, cell, 4)
            && !self.inner.flags(cell).intersects(PINCHED)
    }

    fn portal_at(&self, cell: Cell) -> Option<mint_movement::Portal> {
        self.inner.portal_at(cell)
    }

    fn portal_links(&self, cell: Cell) -> &[mint_movement::PortalLink] {
        self.inner.portal_links(cell)
    }

    fn admits_step(&self, from: Cell, to: Cell, mover: MoverClass) -> bool {
        self.inner.admits_step(from, to, mover)
    }
}

fn refuses_group_cell(flags: CellFlags) -> bool {
    flags.intersects(
        CLIFF
            .union(WATER)
            .union(DEEP_WATER)
            .union(OBSTACLE_CELL)
            .union(RUBBLE)
            .union(PINCHED),
    )
}

fn goal_probe(
    view: &dyn Passability,
    extent: GridExtent,
    idle: &BTreeSet<(i32, i32)>,
    cell: Cell,
) -> Option<Cell> {
    const RING: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    const RINGS: i32 = 7;
    (1..=RINGS).find_map(|k| {
        RING.iter().find_map(|(dx, dy)| {
            let (Ok(nx), Ok(ny)) = (
                u16::try_from(i32::from(cell.x).saturating_add(dx.saturating_mul(k))),
                u16::try_from(i32::from(cell.y).saturating_add(dy.saturating_mul(k))),
            ) else {
                return None;
            };
            let n = Cell::new(nx, ny);
            (extent.contains(n) && clear_diameter(view, extent, idle, n, 4)).then_some(n)
        })
    })
}

fn bresenham_cells(from: Cell, to: Cell) -> Vec<Cell> {
    if from.layer != to.layer {
        return vec![from];
    }
    let mut x = i32::from(from.x);
    let mut y = i32::from(from.y);
    let target_x = i32::from(to.x);
    let target_y = i32::from(to.y);
    let dx = target_x.saturating_sub(x).abs();
    let dy = target_y.saturating_sub(y).abs();
    let step_x = if target_x > x { 1 } else { -1 };
    let step_y = if target_y > y { 1 } else { -1 };
    let mut error = dx.saturating_sub(dy);
    let cap = dx.saturating_add(dy).saturating_add(1);
    let mut cells = Vec::new();
    for _ in 0..=cap {
        let (Ok(cx), Ok(cy)) = (u16::try_from(x), u16::try_from(y)) else {
            break;
        };
        cells.push(Cell::on_layer(cx, cy, from.layer));
        if x == target_x && y == target_y {
            break;
        }
        let doubled = error.saturating_mul(2);
        if doubled > dy.saturating_neg() {
            error = error.saturating_sub(dy);
            x = x.saturating_add(step_x);
        }
        if doubled < dx {
            error = error.saturating_add(dx);
            y = y.saturating_add(step_y);
        }
    }
    cells
}

fn straighten_group_path(view: &dyn Passability, mover: MoverClass, cells: &[Cell]) -> Vec<Cell> {
    let Some(&first) = cells.first() else {
        return Vec::new();
    };
    if cells.len() <= 2 {
        return cells.to_vec();
    }
    let Some(&last) = cells.last() else {
        return cells.to_vec();
    };
    let mut kept = vec![first];
    let mut anchor = first;
    for (i, &cell) in cells.iter().enumerate().skip(1) {
        if cell == last {
            break;
        }
        let Some(&next) = cells.get(i.saturating_add(1)) else {
            break;
        };
        if !mint_movement::smoothing::line_is_passable(view, mover, anchor, next) {
            kept.push(cell);
            anchor = cell;
        }
    }
    kept.push(last);
    kept
}

fn form_up_distance(row_depth: f32, reach: f32, recently_damaged: bool) -> f32 {
    let l = (4.0 * row_depth).max(reach + row_depth * 0.5);
    if recently_damaged {
        l * 2.0
    } else {
        l
    }
}

struct Walk {
    start: Vec2,
    form_point: Option<Vec2>,
    interior: Option<(usize, usize)>,
    end_node: Option<usize>,
}

impl Walk {
    fn member_points(
        &self,
        nodes: &[Vec2],
        slot: Vec2,
        use_form_point: bool,
        narrow_scale: &dyn Fn(Vec2) -> f32,
        adjust: &dyn Fn(Vec2, Vec2) -> Vec2,
    ) -> (Vec<Vec2>, Vec2) {
        let mut list = Vec::new();
        let mut previous = self.start;
        if let (true, Some(f)) = (use_form_point, self.form_point) {
            let point = adjust(
                f,
                f.plus(mint_sage_formation::rotate_offset(self.start, f, slot)),
            );
            list.push(point);
            previous = f;
            if let Some((first, last)) = self.interior {
                for (index, node) in nodes
                    .iter()
                    .enumerate()
                    .take(last.saturating_add(1))
                    .skip(first)
                {
                    let scaled = slot.scale(narrow_scale(*node));
                    let raw =
                        node.plus(mint_sage_formation::rotate_offset(previous, *node, scaled));
                    let point = adjust(*node, raw);
                    list.push(point);
                    if index != last {
                        previous = point;
                    }
                }
            }
        }
        (list, previous)
    }

    fn along(nodes: &[Vec2], form_up: f32, near_end: f32) -> Self {
        let start = nodes.first().copied().unwrap_or(Vec2::ZERO);
        let end = nodes.last().copied().unwrap_or(start);
        let total: f32 = nodes
            .windows(2)
            .filter_map(|pair| Some(pair.get(1)?.minus(*pair.first()?).length()))
            .sum();
        let mut form = None;
        let mut walked = 0.0f32;
        for (index, pair) in nodes.windows(2).enumerate() {
            let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            let leg = b.minus(a).length();
            if walked + leg >= form_up && leg > 0.0 {
                let point = a.plus(b.minus(a).scale((form_up - walked) / leg));
                form = Some((point, index.saturating_add(1)));
                break;
            }
            walked += leg;
        }
        let form = form.filter(|_| total - form_up > near_end);
        let last = nodes.len().saturating_sub(1);
        let end_node = nodes
            .iter()
            .take(last)
            .rposition(|node| node.minus(end).length() > near_end);
        let interior = match (form, end_node) {
            (Some((_, first)), Some(last)) if last > first => Some((first, last)),
            _ => None,
        };
        Self {
            start,
            form_point: form.map(|(point, _)| point),
            interior,
            end_node,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_movement::CostGrid;
    use mint_sage_passability::OBSTACLE;

    #[test]
    fn the_first_leg_after_the_form_point_turns_from_the_unoffset_form_point() {
        let nodes = [
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 40.0),
            Vec2::new(100.0, 400.0),
        ];
        let walk = Walk {
            start: Vec2::new(0.0, 0.0),
            form_point: Some(Vec2::new(50.0, 0.0)),
            interior: Some((1, 1)),
            end_node: Some(1),
        };
        let slot = Vec2::new(0.0, 30.0);
        let (points, _) = walk.member_points(&nodes, slot, true, &|_| 1.0, &|_, p| p);
        let node = Vec2::new(100.0, 40.0);
        let expected = node.plus(mint_sage_formation::rotate_offset(
            Vec2::new(50.0, 0.0),
            node,
            slot,
        ));
        let got = points[1];
        assert!(
            got.minus(expected).length() < 1e-3,
            "{got:?} against {expected:?}"
        );
    }

    #[test]
    fn the_final_leg_turns_from_the_offset_point_before_the_last_interior_node() {
        let nodes = [
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(100.0, 400.0),
        ];
        let walk = Walk {
            start: Vec2::new(0.0, 0.0),
            form_point: Some(Vec2::new(50.0, 0.0)),
            interior: Some((1, 2)),
            end_node: Some(2),
        };
        let (points, previous) =
            walk.member_points(&nodes, Vec2::new(0.0, 30.0), true, &|_| 1.0, &|_, p| p);
        assert_eq!(points.len(), 3);
        assert_eq!(previous, points[1]);
        assert_ne!(previous, points[2]);
    }

    #[test]
    fn form_up_distance_is_four_rows_at_least_and_doubles_when_recently_hit() {
        assert!((form_up_distance(55.0, 0.0, false) - 220.0).abs() < 1e-3);
        let deep = form_up_distance(55.0, 500.0, false);
        assert!((deep - 527.5).abs() < 1e-3, "{deep}");
        assert!((form_up_distance(55.0, 500.0, true) - deep * 2.0).abs() < 1e-3);
    }

    #[test]
    fn a_narrow_node_scales_the_members_slot() {
        let nodes = [Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0)];
        let walk = Walk {
            start: Vec2::new(0.0, 0.0),
            form_point: Some(Vec2::new(0.0, 0.0)),
            interior: Some((1, 1)),
            end_node: Some(1),
        };
        let slot = Vec2::new(0.0, 30.0);
        let (points, _) = walk.member_points(&nodes, slot, true, &|_| 0.4, &|_, p| p);
        let node = Vec2::new(100.0, 0.0);
        let expected = node.plus(mint_sage_formation::rotate_offset(
            Vec2::new(0.0, 0.0),
            node,
            slot.scale(0.4),
        ));
        let got = points[1];
        assert!(
            got.minus(expected).length() < 1e-3,
            "{got:?} against {expected:?}"
        );
    }

    fn half_blocked_grid() -> (CostGrid, mint_movement::CellGeometry, GridExtent) {
        let extent = GridExtent::new(10, 10).unwrap();
        let geometry = mint_movement::CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap();
        let mut grid = CostGrid::new(extent);
        for y in 0..10u16 {
            for x in 5..10u16 {
                grid.add_flags(Cell::new(x, y), OBSTACLE).unwrap();
            }
        }
        (grid, geometry, extent)
    }

    #[test]
    fn adjust_ground_path_position_into_a_block_returns_the_last_4_clear_cell() {
        let (grid, geometry, extent) = half_blocked_grid();
        let idle = BTreeSet::new();
        let anchor = geometry.centre_of(Cell::new(2, 4));
        let p = geometry.centre_of(Cell::new(9, 4));
        let got = adjust_ground_path_position(&grid, geometry, extent, &idle, anchor, p);
        let expected = geometry.centre_of(Cell::new(3, 4));
        assert!(
            got.minus(expected).length() < 1e-3,
            "{got:?} against {expected:?}"
        );
    }

    #[test]
    fn adjust_ground_path_position_whose_first_cell_fails_returns_the_anchor() {
        let (grid, geometry, extent) = half_blocked_grid();
        let idle = BTreeSet::new();
        let anchor = geometry.centre_of(Cell::new(4, 4));
        let p = geometry.centre_of(Cell::new(9, 4));
        let got = adjust_ground_path_position(&grid, geometry, extent, &idle, anchor, p);
        assert!(
            got.minus(anchor).length() < 1e-3,
            "{got:?} against anchor {anchor:?}"
        );
    }

    #[test]
    fn adjust_ground_path_position_over_a_clear_line_keeps_p() {
        let (grid, geometry, extent) = half_blocked_grid();
        let idle = BTreeSet::new();
        let anchor = geometry.centre_of(Cell::new(2, 4));
        let p = geometry.centre_of(Cell::new(3, 4));
        let got = adjust_ground_path_position(&grid, geometry, extent, &idle, anchor, p);
        assert!(got.minus(p).length() < 1e-3, "{got:?} against {p:?}");
    }

    fn one_clear_cell_grid() -> (CostGrid, GridExtent) {
        let extent = GridExtent::new(24, 24).unwrap();
        let mut grid = CostGrid::new(extent);
        for y in 0..24u16 {
            for x in 0..24u16 {
                if !(10..=13).contains(&x) || !(10..=13).contains(&y) {
                    grid.add_flags(Cell::new(x, y), OBSTACLE).unwrap();
                }
            }
        }
        (grid, extent)
    }

    #[test]
    fn the_goal_probe_walks_seven_rings() {
        let (grid, extent) = one_clear_cell_grid();
        let idle = BTreeSet::new();
        assert!(clear_diameter(&grid, extent, &idle, Cell::new(12, 12), 4));
        assert_eq!(
            goal_probe(&grid, extent, &idle, Cell::new(9, 12)),
            Some(Cell::new(12, 12))
        );
        assert_eq!(goal_probe(&grid, extent, &idle, Cell::new(2, 12)), None);
        assert_eq!(
            goal_probe(&grid, extent, &idle, Cell::new(14, 12)),
            Some(Cell::new(12, 12))
        );
    }

    #[test]
    fn ground_path_view_refuses_a_pinched_cell() {
        let extent = GridExtent::new(10, 10).unwrap();
        let mut grid = CostGrid::new(extent);
        let cell = Cell::new(5, 5);
        grid.add_flags(cell, mint_sage_passability::PINCHED)
            .unwrap();
        let idle = BTreeSet::new();
        let view = GroundPathView {
            inner: &grid,
            extent,
            idle: &idle,
        };
        assert!(!view.is_passable(cell, GROUND));
        assert!(view.is_passable(Cell::new(4, 4), GROUND));
    }

    #[test]
    fn a_non_clear_strip_blocks_the_group_path_whatever_the_leaders_surfaces() {
        let extent = GridExtent::new(12, 5).unwrap();
        let geometry = mint_movement::CellGeometry::new(10.0, Vec2::new(0.0, 0.0)).unwrap();
        let mut grid = CostGrid::new(extent);
        for y in 0..5u16 {
            grid.add_flags(Cell::new(6, y), WATER).unwrap();
        }
        let mover =
            mint_sage_passability::mover_class(&["GROUND".to_string(), "WATER".to_string()], false)
                .unwrap();
        let ask = |goal: Cell| {
            let request = PathRequest {
                start: Cell::new(1, 2),
                goal,
                mover,
                goal_id: GoalId(0),
                origin: None,
                corner_radius: 0.0,
                departure: None,
                approach: None,
                prune: false,
                final_goal: true,
            };
            let mut pipeline = Pipeline::new(PipelineConfig::default());
            pipeline
                .plan_first_visit(
                    &grid,
                    geometry,
                    &request,
                    &mut NodeBudget::unlimited(),
                    &|cell| refuses_group_cell(grid.flags(cell)),
                )
                .unwrap()
        };
        assert!(matches!(ask(Cell::new(10, 2)), PlanOutcome::Unreachable));
        assert!(matches!(ask(Cell::new(6, 2)), PlanOutcome::Ready(_)));
    }

    #[test]
    fn ground_path_view_charges_six_per_missing_clear_cell() {
        let extent = GridExtent::new(10, 10).unwrap();
        let mut grid = CostGrid::new(extent);
        grid.add_flags(Cell::new(3, 5), OBSTACLE).unwrap();
        grid.add_flags(Cell::new(7, 5), OBSTACLE).unwrap();
        let idle = BTreeSet::new();
        let view = GroundPathView {
            inner: &grid,
            extent,
            idle: &idle,
        };
        let cell = Cell::new(5, 5);
        assert!(!clear_diameter(&grid, extent, &idle, cell, 4));
        assert!(clear_diameter(&grid, extent, &idle, cell, 2));
        assert_eq!(view.extra_cost(cell), 12);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn gate(units: &[(f32, bool, bool)]) -> bool {
        let members: Vec<Scanned> = units
            .iter()
            .map(|(x, aircraft, held)| Scanned {
                position: Vec2::new(*x, 0.0),
                aircraft: *aircraft,
                held: *held,
            })
            .collect();
        move_reaches_min_distance(200.0, &members, Vec2::new(0.0, 0.0))
    }

    #[test]
    fn a_move_shorter_than_the_minimum_distance_is_refused() {
        assert!(!gate(&[(150.0, false, false), (160.0, false, false)]));
        assert!(gate(&[(250.0, false, false), (260.0, false, false)]));
    }

    #[test]
    fn a_wide_group_forms_columns_however_near_its_nearest_member_is() {
        assert!(gate(&[(50.0, false, false), (1000.0, false, false)]));
    }

    #[test]
    fn an_aircraft_beside_the_click_does_not_shorten_the_move() {
        assert!(gate(&[
            (10.0, true, false),
            (250.0, false, false),
            (260.0, false, false)
        ]));
        assert!(!gate(&[
            (10.0, false, false),
            (250.0, false, false),
            (260.0, false, false)
        ]));
    }

    #[test]
    fn a_held_member_beside_the_click_does_not_shorten_the_move() {
        assert!(gate(&[
            (10.0, false, true),
            (250.0, false, false),
            (260.0, false, false)
        ]));
    }
}
