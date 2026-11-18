use super::*;

const SELECT_JITTER_LOW: f32 = 0.66;
const SELECT_JITTER_HIGH: f32 = 1.33;

pub(super) const CLAIM_BUCKET: f32 = 128.0;

const STOP_CHASE_DISTANCE: f32 = 500.0;

const SHORTCUT_STEP: f32 = 5.0;
const SHORTCUT_STEPS: u8 = 9;
const SHORTCUT_EXTRA: f32 = 10.0;

const MELEE_GOAL_STEP: f32 = 10.0;

const ATTACK_MOVE_RETRY_HOLD_SECONDS: u64 = 3;

pub(super) fn regoal_due(now: (Vec2, f32), taken: (Vec2, f32)) -> bool {
    super::movement::octagonal_length_3d(now.0.x - taken.0.x, now.0.y - taken.0.y, now.1 - taken.1)
        > ATTACK_MOVE_OBJECT_REGOAL
}

const ATTACK_MOVE_OBJECT_REGOAL: f32 = 5.0;

const ATTACK_MOVE_DONE_RADIUS: f32 = 80.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admission {
    Order,
    Force,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RangedApproach {
    goal: Vec2,
    victim: Entity,
    victim_at: Vec2,
    attack_path: bool,
}

impl Component for RangedApproach {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.goal.x);
        hasher.write_f32(self.goal.y);
        self.victim.hash_into(hasher);
        hasher.write_f32(self.victim_at.x);
        hasher.write_f32(self.victim_at.y);
        hasher.write_bool(self.attack_path);
    }
}

pub(super) fn melee_search_offsets(count: u32) -> &'static [(i32, i32)] {
    static OFFSETS: std::sync::OnceLock<Vec<(i32, i32)>> = std::sync::OnceLock::new();
    let all = OFFSETS.get_or_init(|| {
        let reach: i32 = 40;
        let mut offsets: Vec<(i32, i32)> = (-reach..=reach)
            .flat_map(|y| (-reach..=reach).map(move |x| (x, y)))
            .collect();
        offsets.sort_by_key(|&(x, y)| {
            (
                x.saturating_mul(x).saturating_add(y.saturating_mul(y)),
                y,
                x,
            )
        });
        offsets
    });
    let count = usize::try_from(count).unwrap_or(usize::MAX).min(all.len());
    all.get(..count).unwrap_or(all)
}

fn square_cells(
    geometry: mint_movement::CellGeometry,
    centre: Vec2,
    half: f32,
) -> impl Iterator<Item = (i32, i32)> {
    let size = geometry.cell_size();
    let origin = geometry.origin();
    #[allow(clippy::cast_possible_truncation)]
    let cell = |v: f32, o: f32| ((v - o) / size - 0.5).clamp(-1.0e6, 1.0e6);
    #[allow(clippy::cast_possible_truncation)]
    let (x0, x1) = (
        cell(centre.x - half, origin.x).ceil() as i32,
        cell(centre.x + half, origin.x).floor() as i32,
    );
    #[allow(clippy::cast_possible_truncation)]
    let (y0, y1) = (
        cell(centre.y - half, origin.y).ceil() as i32,
        cell(centre.y + half, origin.y).floor() as i32,
    );
    (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
}

pub(super) const CLAIM_CELL_BIAS: f32 = 0.3;

pub(super) const CLAIM_MAX_RADIUS: u32 = 2;

pub(super) const HORDE_CLAIM_MAX_RADIUS: u32 = 4;

pub(super) struct Claimant {
    pub(super) entity: Entity,
    pub(super) goals: Vec<(
        mint_movement::destination::Footprint,
        mint_movement::destination::Footprint,
        super::unit_cells::GoalHolder,
    )>,
}

#[derive(Debug, Clone, Copy)]
enum GroupDecision {
    Skip,
    Ground,
    Unarmed,
    Engage(GroupFacts),
}

#[derive(Debug, Clone, Copy)]
struct GroupFacts {
    max_range: f32,
    body: Body,
    owner: Option<PlayerId>,
    guarded: bool,
    scan: Option<crate::combat::IdleScan>,
}

impl RotwkRules {
    pub(super) fn tick_snapshot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
    ) -> (Vec<(Foe, PlayerId)>, TargetGrid, f32) {
        let living = self.living(tick, ids);
        let grid = TargetGrid::build(&living);
        let global_max_radius = living
            .iter()
            .map(|(foe, _)| foe.radius)
            .fold(0.0f32, f32::max);
        (living, grid, global_max_radius)
    }

    pub(super) fn living(&self, tick: &Tick<'_>, ids: Ids) -> Vec<(Foe, PlayerId)> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("living_walk"));
        let Some(health) = tick.state().store(ids.health) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(health.len());
        for (entity, pool) in health.iter() {
            if pool.0.is_dead() || tick.state().contains(ids.concealed, entity) {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity) else {
                continue;
            };
            let Some(shape) = tick.state().get(ids.shape, entity) else {
                continue;
            };
            if shape.kind_of.contains(KindOf::UNATTACKABLE) {
                continue;
            }
            let owner = tick
                .state()
                .get(ids.owner, entity)
                .map_or(PlayerId::new(0), |owner| owner.0);
            let enclosed = self
                .contained_status(tick.state(), ids, entity)
                .is_some_and(|status| status.has(super::contain::ContainStatus::ENCLOSED));
            out.push((
                Foe {
                    entity,
                    position: body.0.position,
                    z: Self::feet(tick.state(), ids, entity),
                    radius: shape.radius,
                    height: shape.height,
                    is_structure: shape.kind_of.contains(KindOf::STRUCTURE),
                    airborne: self.airborne_category(tick.state(), ids, entity),
                    unattackable: self.is_unattackable(tick.state(), ids, entity),
                    not_autoacquirable: shape.kind_of.contains(KindOf::NOT_AUTOACQUIRABLE)
                        || enclosed,
                    ai_kind_of: self.ai_kind_of_of(tick.state(), ids, entity),
                },
                owner,
            ));
        }
        out
    }

    pub(super) fn attack_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> Result<()> {
        self.attack_object_order(tick, ids, player, target, Admission::Order)
    }

    pub(super) fn force_attack_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> Result<()> {
        self.attack_object_order(tick, ids, player, target, Admission::Force)
    }

    pub(super) fn script_attack_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
    ) -> Result<()> {
        self.attack_object_order(tick, ids, player, target, Admission::Script)
    }

    fn attack_object_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        target: Entity,
        admission: Admission,
    ) -> Result<()> {
        let forced = admission == Admission::Force;
        let relation_free = admission != Admission::Order;
        if !tick.state().is_live(target) || tick.state().contains(ids.concealed, target) {
            return Ok(());
        }
        if !tick.state().contains(ids.health, target) {
            return Ok(());
        }
        let target_kind_of = tick
            .state()
            .get(ids.shape, target)
            .map_or(KindOf::NONE, |shape| shape.kind_of);
        let target_owner = tick.state().get(ids.owner, target).map(|owner| owner.0);
        let relation = self.view_relation(tick.state(), player, target).victim();
        if !mint_sage_combat::can_target(
            mint_sage_combat::Attacker {
                relation,
                class_allowed: true,
                scans_structures: true,
            },
            mint_sage_combat::Victim {
                live: true,
                concealed: false,
                contained: false,
                kind_of: target_kind_of,
                is_structure: target_kind_of.contains(KindOf::STRUCTURE),
                seen: admission == Admission::Script
                    || self.sees(tick.state(), ids, player, target),
                script_targetable: self.script_status_bit(
                    tick.state(),
                    ids,
                    target,
                    super::map_scripts::ScriptStatusBit::Targetable,
                ),
            },
            if relation_free {
                mint_sage_combat::Mode::Force
            } else {
                mint_sage_combat::Mode::Order
            },
        ) {
            return Ok(());
        }
        let modes = self.modes(tick, ids);
        let selection = self.selection(tick, ids, player);
        let hordes: Vec<Entity> = if modes.formation == FormationMode::On {
            let owned_members: Vec<Entity> = selection
                .iter()
                .copied()
                .filter(|entity| {
                    tick.state()
                        .get(ids.owner, *entity)
                        .is_some_and(|owner| owner.0 == player)
                })
                .collect();
            crate::horde::hordes_of(tick.state(), Self::horde_ids(ids), &owned_members)
        } else {
            Vec::new()
        };
        for horde_entity in &hordes {
            if !self.porcupine_gate(tick, ids, *horde_entity, !forced)? {
                continue;
            }
            let Some(horde) = tick.state().get(ids.horde, *horde_entity).cloned() else {
                continue;
            };
            let armed = horde
                .members
                .iter()
                .any(|member| tick.state().contains(ids.armed, *member));
            if !armed
                || tick.state().contains(ids.no_attack, *horde_entity)
                || Self::emotion_refuses_order(tick.state(), ids, *horde_entity)
                || (forced && self.is_rampaging(tick.state(), *horde_entity))
            {
                continue;
            }
            if !self.take_player_order(tick, ids, *horde_entity, !forced, true, true)? {
                continue;
            }
            let was_attacking = self.is_attacking_self_or_horde(tick, ids, *horde_entity);
            for member in &horde.members {
                tick.state_mut().remove(ids.attack_order, *member);
                tick.state_mut().remove(ids.path, *member);
            }
            tick.state_mut().insert(
                ids.attack_order,
                *horde_entity,
                AttackOrder {
                    target,
                    planned: false,
                    horde: None,
                    guard: false,
                    relation_free,
                    order_owner: target_owner,
                },
            )?;
            self.emit_voice_with(
                tick,
                ids,
                *horde_entity,
                crate::voice::VoiceKind::Attack,
                Some(target),
                was_attacking,
            );
        }
        for entity in selection {
            if entity == target {
                continue;
            }
            if modes.formation == FormationMode::On {
                if let Some(member) = tick.state().get(ids.member, entity) {
                    if hordes.contains(&member.horde) {
                        continue;
                    }
                }
            }
            let owned = tick
                .state()
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player);
            if !owned
                || !tick.state().contains(ids.armed, entity)
                || tick.state().contains(ids.no_attack, entity)
                || Self::emotion_refuses_order(tick.state(), ids, entity)
                || self.siege_refuses_order(tick.state(), ids, entity)
                || (forced && self.is_rampaging(tick.state(), entity))
            {
                continue;
            }
            if !self.take_player_order(tick, ids, entity, !forced, true, true)? {
                continue;
            }
            let horde = tick
                .state()
                .get(ids.member, target)
                .map(|member| member.horde);
            let victim = horde
                .and_then(|horde| self.adjust_victim(tick, ids, entity, horde, Some(target), None))
                .unwrap_or(target);
            let victim_owner = tick.state().get(ids.owner, victim).map(|owner| owner.0);
            let was_attacking = self.is_attacking_self_or_horde(tick, ids, entity);
            tick.state_mut().insert(
                ids.attack_order,
                entity,
                AttackOrder {
                    target: victim,
                    planned: false,
                    horde,
                    guard: false,
                    relation_free,
                    order_owner: victim_owner,
                },
            )?;
            self.emit_voice_with(
                tick,
                ids,
                entity,
                crate::voice::VoiceKind::Attack,
                Some(target),
                was_attacking,
            );
        }
        Ok(())
    }

    pub(super) fn attack_move(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
    ) -> Result<()> {
        self.attack_move_inner(
            tick,
            ids,
            player,
            goal,
            None,
            false,
            busy::MovePath::PlayerGroup,
        )
    }

    pub(super) fn attack_move_queued(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
        queue: bool,
        path: busy::MovePath,
    ) -> Result<()> {
        self.attack_move_inner(tick, ids, player, goal, None, queue, path)
    }

    pub(super) fn attack_move_with_drag_formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        anchor: Vec2,
        order: super::group_move::FormationOrder,
        path: busy::MovePath,
    ) -> Result<()> {
        self.attack_move_inner(tick, ids, player, anchor, Some(order), false, path)
    }

    #[allow(clippy::too_many_arguments)]
    fn attack_move_inner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
        drag: Option<super::group_move::FormationOrder>,
        queue: bool,
        path: busy::MovePath,
    ) -> Result<()> {
        let per_member = path == busy::MovePath::PerMember;
        let follow = matches!(
            path,
            busy::MovePath::AttackFollow | busy::MovePath::AttackFollowLeg
        );
        let non_final = path == busy::MovePath::AttackFollowLeg;
        let is_drag = drag.is_some();
        if drag.is_none() && self.modes(tick, ids).formation != FormationMode::On {
            return Ok(());
        }
        let (_, mut hordes) = self.move_order_targets_and_movers(tick, ids, player, drag.is_some());
        hordes.retain(|mover| !self.is_rampaging(tick.state(), *mover));
        let mut through = Vec::with_capacity(hordes.len());
        for horde in hordes {
            if self.porcupine_gate(tick, ids, horde, false)? {
                through.push(horde);
            }
        }
        let hordes = through;
        let facing = drag.as_ref().map(|order| order.facing);
        let formation = if let Some(order) = drag {
            Some(order)
        } else if !per_member && self.group_formation.attack_move_uses_formations {
            let mut pipeline = Pipeline::new(self.modes(tick, ids).config());
            self.formation_order(tick, ids, &hordes, goal, &mut pipeline)?
        } else {
            None
        };
        self.record_formation_slots(tick, ids, &hordes, formation.as_ref())?;
        let destinations = match (&formation, per_member) {
            (Some(order), true) => Self::slot_points_as_claimed(order),
            (None, true) => {
                let starting = self.movers_starting_now(tick.state(), ids, &hordes, queue);
                self.claim_in_member_order(tick, ids, &starting, goal, true)
            }
            (_, false) => {
                let wanted = formation.as_ref().map_or_else(
                    || Self::group_destinations(tick, ids, &hordes, goal, false),
                    super::group_move::FormationOrder::destinations,
                );
                self.reserve_destinations(tick, ids, &hordes, &wanted, goal, true, None)
            }
        };
        if path.is_player_dispatch() {
            self.cohere_move_speed(tick, ids, &hordes)?;
        }
        let tag = (per_member && (queue || !is_drag)).then(|| crate::combat::OrderTag {
            id: Self::next_order_id(tick.state(), ids),
            click: goal,
            members: hordes.clone(),
            copy_of: None,
            lap_clone: None,
        });
        for horde_entity in hordes {
            let destination = destinations.get(&horde_entity).map_or(goal, |(at, _)| *at);
            if queue && self.order_queue_busy(tick.state(), ids, horde_entity) {
                Self::append_queued_leg(
                    tick,
                    ids,
                    horde_entity,
                    crate::combat::QueuedLeg {
                        point: if per_member { goal } else { destination },
                        attack: true,
                        resolve: per_member,
                        tag: tag.clone(),
                        non_final,
                        follow,
                    },
                )?;
                continue;
            }
            if !self.take_player_order(tick, ids, horde_entity, false, !queue, true)? {
                continue;
            }
            self.note_order_origin(tick, ids, horde_entity)?;
            tick.state_mut().remove(ids.attack_order, horde_entity);
            tick.state_mut().remove(ids.horde_target, horde_entity);
            tick.state_mut().remove(ids.path, horde_entity);
            Self::clear_path_retry(tick, ids, horde_entity);
            let order = if follow {
                AttackMoveOrder::follow_leg(destination, non_final)
            } else {
                AttackMoveOrder::new(destination, facing, None)
            };
            tick.state_mut()
                .insert(ids.attack_move_order, horde_entity, order)?;
            if queue || tag.is_some() {
                tick.state_mut().insert(
                    ids.order_queue,
                    horde_entity,
                    crate::combat::OrderQueue {
                        current: Some(crate::combat::QueuedLeg {
                            point: destination,
                            attack: true,
                            resolve: false,
                            tag: tag.clone(),
                            non_final,
                            follow,
                        }),
                        queued: Vec::new(),
                        idle_checks: 0,
                        loop_start: None,
                    },
                )?;
            }
        }
        if let Some(tag) = tag.filter(|_| queue) {
            self.link_waypoint_loop(tick, ids, &tag)?;
        }
        Ok(())
    }

    pub(super) fn group_destinations(
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        goal: Vec2,
        offsets: bool,
    ) -> BTreeMap<Entity, Vec2> {
        let mut destinations = BTreeMap::new();
        let mut placed: Vec<(Entity, Vec2)> = movers
            .iter()
            .filter_map(|entity| Some((*entity, tick.state().get(ids.body, *entity)?.0.position)))
            .collect();
        if placed.len() < 2 || !offsets {
            return destinations;
        }
        placed.sort_by(|(a, pa), (b, pb)| {
            pa.minus(goal)
                .length_squared()
                .total_cmp(&pb.minus(goal).length_squared())
                .then(a.index().cmp(&b.index()))
        });
        let Some(&(_, reference)) = placed.first() else {
            return destinations;
        };
        for (entity, position) in placed {
            let cap = LOOSE_GROUP_OFFSET_CAP_RADII * Self::mover_radius(tick, ids, entity);
            let offset = position.minus(reference);
            let distance = offset.length();
            let capped = if distance > cap && distance > 0.0 {
                offset.scale(cap / distance)
            } else {
                offset
            };
            destinations.insert(entity, goal.plus(capped));
        }
        destinations
    }

    pub(super) fn claim_in_member_order(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        goal: Vec2,
        formations_on: bool,
    ) -> BTreeMap<Entity, (Vec2, Facing)> {
        use super::adjust_destination::{OrderClaims, Request};
        let ground: Vec<Entity> = movers
            .iter()
            .copied()
            .filter(|mover| !self.is_giant_bird(tick.state(), ids, *mover))
            .collect();
        let mut order = OrderClaims::for_order(self, tick, ids, formations_on, &ground);
        let mut placed = BTreeMap::new();
        for mover in ground {
            let Some(from) = tick.state().get(ids.body, mover).map(|body| body.0) else {
                continue;
            };
            let request = Request {
                mover,
                dest: goal,
                group_goal: Some(goal),
            };
            let at = self
                .adjust_destination(tick, ids, request, &mut order)
                .unwrap_or(goal);
            let heading = Facing::from_vector(at.minus(from.position)).unwrap_or(from.facing);
            let facing = self.claim_facing(tick, ids, mover, heading);
            let is_horde = tick.state().contains(ids.horde, mover);
            let cells = if is_horde {
                self.claim_horde_cells(tick, ids, mover, at, facing)
            } else {
                self.claim_cells(tick, ids, mover, at, heading.radians())
            };
            let body = self
                .claim_at(tick, ids, mover, at, facing)
                .filter(|_| !is_horde)
                .map(|(_, body, offset)| body.at(at.plus(offset)));
            order.claim(cells, body);
            placed.insert(mover, (at, facing));
        }
        placed
    }

    pub(super) fn movers_starting_now(
        &self,
        world: &World,
        ids: Ids,
        movers: &[Entity],
        queue: bool,
    ) -> Vec<Entity> {
        movers
            .iter()
            .copied()
            .filter(|mover| !queue || !self.order_queue_busy(world, ids, *mover))
            .collect()
    }

    pub(super) fn slot_points_as_claimed(
        order: &super::group_move::FormationOrder,
    ) -> BTreeMap<Entity, (Vec2, Facing)> {
        order
            .destinations()
            .into_iter()
            .map(|(entity, at)| (entity, (at, order.facing)))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn reserve_destinations(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        movers: &[Entity],
        wanted: &BTreeMap<Entity, Vec2>,
        goal: Vec2,
        formations_on: bool,
        facing: Option<Facing>,
    ) -> BTreeMap<Entity, (Vec2, Facing)> {
        use mint_movement::destination::{nearest_free, Claims};
        let world = tick.state();
        let ordered: BTreeSet<Entity> = movers.iter().copied().collect();
        let mut others = self.claimants(tick, ids, formations_on);
        others.retain(|claimant| !ordered.contains(&claimant.entity));
        let mut claims = Claims::new(CLAIM_BUCKET);
        let mut bodies = Claims::new(CLAIM_BUCKET);
        let mut asker: Option<super::unit_cells::DestinationAsker> = None;
        let mut reserved: Vec<(
            mint_movement::destination::Footprint,
            mint_movement::destination::Footprint,
        )> = Vec::new();

        let mut queue: Vec<(Entity, Vec2, f32)> = movers
            .iter()
            .filter_map(|entity| {
                let at = wanted.get(entity).copied().unwrap_or(goal);
                let from = world.get(ids.body, *entity)?.0.position;
                Some((*entity, at, from.minus(goal).length_squared()))
            })
            .collect();
        queue.sort_by(|(a, pa, fa), (b, pb, fb)| {
            pa.minus(goal)
                .length_squared()
                .total_cmp(&pb.minus(goal).length_squared())
                .then(fa.total_cmp(fb))
                .then(a.index().cmp(&b.index()))
        });

        let view = self.dynamic_view(tick, ids);
        let geometry = self.passability.geometry;
        let cell_size = geometry.cell_size();
        let mut placed = BTreeMap::new();
        for (entity, at, _) in queue {
            let Some(body) = world.get(ids.body, entity) else {
                continue;
            };
            let this = self.destination_asker(tick, ids, entity);
            if asker != Some(this) {
                claims = Claims::new(CLAIM_BUCKET);
                bodies = Claims::new(CLAIM_BUCKET);
                for claimant in &others {
                    for (square, member_body, holder) in &claimant.goals {
                        if self.goal_verdict(this, *holder) != super::unit_cells::GoalVerdict::Clear
                        {
                            claims.insert(*square);
                            bodies.insert(*member_body);
                        }
                    }
                }
                for (square, body) in &reserved {
                    claims.insert(*square);
                    bodies.insert(*body);
                }
                asker = Some(this);
            }
            let arrival = facing.unwrap_or_else(|| {
                Facing::from_vector(at.minus(body.0.position)).unwrap_or(body.0.facing)
            });
            let Some((wanted_box, wanted_body, offset)) =
                self.claim_at(tick, ids, entity, at, arrival)
            else {
                placed.insert(entity, (at, arrival));
                continue;
            };
            let start = self.passable_cell(tick, ids, body.0.position);
            let mover = self.mover_class(tick, ids, entity);
            let accept = |centre: Vec2| {
                let origin_cell = self.passability.cell_at(centre.minus(offset));
                let reachable = origin_cell
                    .is_some_and(|cell| start.is_none_or(|s| self.reachable(mover, s, cell)));
                if !reachable || bodies.crowded(&wanted_body.at(centre)) {
                    return false;
                }
                let half = wanted_box.half_along;
                let corner = |dx: f32, dy: f32| centre.plus(Vec2::new(dx, dy));
                let (low, high) = (corner(-half, -half), corner(half, half));
                let (Some(lo), Some(hi)) = (
                    self.passability.cell_at(low),
                    self.passability.cell_at(high),
                ) else {
                    return false;
                };
                (lo.y..=hi.y).all(|y| {
                    (lo.x..=hi.x).all(|x| view.is_passable(mint_movement::Cell::new(x, y), mover))
                })
            };
            let chosen = [1.0f32, 2.0, 4.0]
                .into_iter()
                .find_map(|k| nearest_free(&claims, wanted_box, cell_size * k, accept))
                .unwrap_or(wanted_box);
            claims.insert(chosen);
            bodies.insert(wanted_body.at(chosen.centre));
            reserved.push((chosen, wanted_body.at(chosen.centre)));
            placed.insert(entity, (chosen.centre.minus(offset), arrival));
        }
        placed
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn melee_destination(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        from: Vec2,
        victim: Entity,
        victim_at: Vec2,
    ) -> Option<Vec2> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("melee_destination"));
        let back = from.minus(victim_at);
        let facing = Facing::from_vector(back.scale(-1.0))?;
        let (square, _, offset) = self.claim_at(tick, ids, horde_entity, victim_at, facing)?;
        let geometry = self.passability.geometry;
        let cell_size = geometry.cell_size();
        #[allow(clippy::cast_precision_loss)]
        let search = (self.max_cells_adjust_horde_melee as f32 / core::f32::consts::PI).sqrt();
        let reach = back.length()
            + (search + 1.0) * cell_size
            + 2.0 * core::f32::consts::SQRT_2 * square.half_along;
        let units = self.unit_cells(tick, ids, horde_entity, Some(victim), victim_at, reach);
        let cells_of = |centre: Vec2| square_cells(geometry, centre, square.half_along);
        let score = |centre: Vec2| units.melee_count(cells_of(centre));
        let clear = |centre: Vec2| units.destination_clear(cells_of(centre));
        let mover = self.mover_class(tick, ids, horde_entity);
        let start = self.passable_cell(tick, ids, from);
        let view = self.dynamic_view(tick, ids);
        let passable = |centre: Vec2| {
            let reachable = self
                .passability
                .cell_at(centre.minus(offset))
                .is_some_and(|cell| start.is_none_or(|s| self.reachable(mover, s, cell)));
            let half = square.half_along;
            let corner = |d: f32| self.passability.cell_at(centre.plus(Vec2::new(d, d)));
            let (Some(lo), Some(hi)) = (corner(-half), corner(half)) else {
                return false;
            };
            reachable
                && (lo.y..=hi.y).all(|y| {
                    (lo.x..=hi.x).all(|x| view.is_passable(mint_movement::Cell::new(x, y), mover))
                })
        };
        let victim_centre = victim_at.plus(offset);
        let step = back
            .normalised()
            .map_or(Vec2::ZERO, |dir| dir.scale(MELEE_GOAL_STEP));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps =
            ((back.length() / (2.0 * MELEE_GOAL_STEP)).floor() - 1.0).clamp(0.0, 4096.0) as u32;
        let wanted = (0..steps)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                victim_centre.plus(step.scale(i as f32))
            })
            .find(|&centre| clear(centre) && passable(centre))
            .unwrap_or(victim_centre);
        let mut best: Option<(u32, Vec2)> = None;
        for &(x, y) in melee_search_offsets(self.max_cells_adjust_horde_melee) {
            #[allow(clippy::cast_precision_loss)]
            let centre = wanted.plus(Vec2::new(x as f32 * cell_size, y as f32 * cell_size));
            let Some(count) = score(centre) else {
                continue;
            };
            if best.is_some_and(|(held, _)| held <= count) || !passable(centre) {
                continue;
            }
            best = Some((count, centre));
            if count == 0 {
                break;
            }
        }
        crate::diag::diag!(
            "dest: frame {} horde {} from ({:.0}, {:.0}) victim {} at ({:.0}, {:.0}) wanted ({:.0}, {:.0}) best {:?}",
            tick.frame().get(),
            horde_entity.index(),
            from.x,
            from.y,
            victim.index(),
            victim_at.x,
            victim_at.y,
            wanted.x,
            wanted.y,
            best.map(|(count, at)| (count, at.x.round(), at.y.round()))
        );
        best.map(|(_, centre)| centre.minus(offset))
    }

    pub(super) fn claim_half_side(
        diameter: f32,
        pathfind_diameter: Option<f32>,
        horde: bool,
        cell_size: f32,
    ) -> f32 {
        let cells = match pathfind_diameter {
            Some(overridden) if overridden > 0.0 => overridden / cell_size,
            _ if diameter > cell_size && diameter < 2.0 * cell_size => 2.0,
            _ => diameter / cell_size,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut count = (cells + CLAIM_CELL_BIAS).floor().clamp(0.0, 64.0) as u32;
        if count == 0 {
            count = 1;
        }
        let mut centred = count % 2 == 1;
        let mut radius = count / 2;
        let cap = if horde {
            HORDE_CLAIM_MAX_RADIUS
        } else {
            CLAIM_MAX_RADIUS
        };
        if radius > cap {
            radius = cap;
            centred = true;
        }
        let radius = f32::from(u16::try_from(radius).unwrap_or(u16::MAX));
        if centred {
            (radius + 0.5) * cell_size
        } else {
            radius * cell_size
        }
    }

    pub(super) fn claim_at(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        at: Vec2,
        facing: Facing,
    ) -> Option<(
        mint_movement::destination::Footprint,
        mint_movement::destination::Footprint,
        Vec2,
    )> {
        use mint_movement::destination::Footprint;
        let world = tick.state();
        let cell_size = self.passability.geometry.cell_size();
        let (body, offset, horde) = if world.contains(ids.horde, entity) {
            let block = crate::horde::horde_block(world, Self::horde_ids(ids), entity)?;
            let forward = facing.to_vector();
            let left = Vec2::new(-forward.y, forward.x);
            let offset = forward
                .scale(block.centre.x)
                .plus(left.scale(block.centre.y));
            let body = Footprint::new(
                at.plus(offset),
                facing,
                block.half_along,
                block.half_across,
                block.member_radius,
            );
            (body, offset, true)
        } else {
            let radius = world.get(ids.shape, entity)?.radius;
            (
                Footprint::new(at, facing, radius, radius, radius),
                Vec2::ZERO,
                false,
            )
        };
        let diameter = body.enclosing_radius() * 2.0;
        let pathfind_diameter = self
            .record_of(tick, ids, entity)
            .and_then(|record| record.pathfind_diameter);
        let half = Self::claim_half_side(diameter, pathfind_diameter, horde, cell_size);
        let square = Footprint::new(body.centre, Facing::EAST, half, half, 0.0);
        Some((square, body, offset))
    }

    pub(super) fn claimants(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        formations_on: bool,
    ) -> Vec<Claimant> {
        let world = tick.state();
        let cell_size = self.passability.geometry.cell_size();
        let mut out = Vec::new();
        let mut claim = |entity: Entity| {
            use mint_movement::destination::Footprint;
            let Some(body) = world.get(ids.body, entity) else {
                return;
            };
            let (at, facing) =
                world
                    .get(ids.path, entity)
                    .map_or((body.0.position, body.0.facing), |path| {
                        let facing = path
                            .facing
                            .or_else(|| Facing::from_vector(path.goal.minus(body.0.position)))
                            .unwrap_or(body.0.facing);
                        (path.goal, facing)
                    });
            let standing = at.minus(body.0.position).length() <= cell_size;
            let goals = match world.get(ids.horde, entity) {
                Some(horde) => {
                    let poses = crate::horde::slot_poses(horde, &MoverState::new(at, facing));
                    horde
                        .members
                        .iter()
                        .filter_map(|unit| {
                            let slot = world.get(ids.member, *unit)?.slot?;
                            let centre = *poses.get(usize::from(slot))?;
                            let radius = world.get(ids.shape, *unit)?.radius;
                            let pathfind_diameter = self
                                .record_of(tick, ids, *unit)
                                .and_then(|record| record.pathfind_diameter);
                            let half = Self::claim_half_side(
                                radius * 2.0,
                                pathfind_diameter,
                                false,
                                cell_size,
                            );
                            let square = Footprint::new(centre, Facing::EAST, half, half, 0.0);
                            let body = Footprint::new(centre, Facing::EAST, radius, radius, radius);
                            Some((square, body, self.goal_holder(tick, ids, *unit, standing)))
                        })
                        .collect()
                }
                None => {
                    let Some((square, body, _)) = self.claim_at(tick, ids, entity, at, facing)
                    else {
                        return;
                    };
                    vec![(square, body, self.goal_holder(tick, ids, entity, standing))]
                }
            };
            out.push(Claimant { entity, goals });
        };
        if formations_on {
            if let Some(store) = world.store(ids.horde) {
                for entity in store.entities() {
                    claim(*entity);
                }
            }
        }
        if let Some(store) = world.store(ids.shape) {
            for (entity, shape) in store.iter() {
                if (formations_on && world.contains(ids.member, entity))
                    || shape.kind_of.contains(KindOf::IMMOBILE)
                    || world.contains(ids.flight, entity)
                    || world.contains(ids.doomed, entity)
                    || world.contains(ids.concealed, entity)
                {
                    continue;
                }
                claim(entity);
            }
        }
        out.sort_by_key(|claimant| claimant.entity.index());
        out
    }

    pub(super) fn horde_max_range(&self, tick: &Tick<'_>, ids: Ids, horde: &Horde) -> Option<f32> {
        horde
            .members
            .iter()
            .filter_map(|member| {
                let armed = tick.state().get(ids.armed, *member)?;
                let template = self.combat.weapons.get(armed.weapon)?;
                Some(self.attack_range_of(tick.state(), ids, *member, template))
            })
            .fold(None::<f32>, |max, range| {
                Some(max.map_or(range, |current| current.max(range)))
            })
    }

    pub(super) fn attack_range_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &mint_sage_combat::WeaponTemplate,
    ) -> f32 {
        self.attack_range_at(world, ids, entity, template, 0.0)
    }

    pub(super) fn attack_range_at(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &mint_sage_combat::WeaponTemplate,
        height_above: f32,
    ) -> f32 {
        let range = template.reach_at(
            self.weapon_bonus(world, ids, entity, template).range(),
            height_above,
        );
        world
            .get(ids.bonuses, entity)
            .map_or(range, |bonuses| bonuses.0.range(range))
    }

    pub(super) fn min_range_of(
        &self,
        world: &World,
        entity: Entity,
        template: &mint_sage_combat::WeaponTemplate,
    ) -> f32 {
        let contesting = self
            .modules
            .as_ref()
            .is_some_and(|wiring| wiring.carries_status(world, entity, "CONTESTING_BUILDING"));
        if contesting {
            0.0
        } else {
            template.minimum_reach()
        }
    }

    pub(super) fn idle_scan_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        player: PlayerId,
        max_range: f32,
        during_idle: bool,
    ) -> Option<crate::combat::IdleScan> {
        let Some(entry) = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
        else {
            return Some(crate::combat::IdleScan {
                reach: max_range,
                pursues: false,
                structures: true,
                cadence: MOOD_ATTACK_CHECK,
                retaliate_only: false,
            });
        };
        if during_idle
            && self.is_hidden(tick.state(), ids, entity)
            && !entry
                .record
                .auto_acquire
                .contains(mint_sage_modules::AutoAcquire::STEALTHED)
        {
            return None;
        }
        let scan = crate::combat::idle_scan(crate::combat::ScanInput {
            during_idle,
            flags: entry.record.auto_acquire,
            vision: entry.record.vision_range,
            vision_bonus: tick
                .state()
                .get(ids.bonuses, entity)
                .map_or(0.0, |bonuses| bonuses.0.vision),
            height_bonus: entry
                .record
                .vision_bonus
                .as_ref()
                .zip(tick.state().get(ids.body, entity))
                .map_or(0.0, |(bonus, body)| {
                    let ground = |x, y| self.passability.heights.height_at(x, y);
                    let at = body.0.position;
                    crate::combat::ground_rise(bonus, &ground, at.x, at.y)
                        .map_or(0.0, |rise| crate::combat::height_bonus(bonus, rise))
                }),
            mood_check_ms: entry.record.mood_attack_check_ms,
            computer: self.computer_controlled(player),
            attitude: self.mood_attitude(tick.state(), ids, entity),
            ranges: self.attack_priorities.ranges,
        })?;
        if scan.pursues && self.move_refused(tick, ids, entity).is_some() {
            return Some(crate::combat::IdleScan {
                reach: scan.reach.min(max_range),
                pursues: false,
                ..scan
            });
        }
        Some(scan)
    }

    pub(super) fn ai_table_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<&mint_sage_modules::AiTable> {
        let kind = tick.state().get(ids.kind, entity)?;
        Some(&self.kinds.get(*kind)?.record.ai)
    }

    pub(super) fn chase_leash_spent(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        owner: PlayerId,
        target: Entity,
    ) -> bool {
        if self.computer_controlled(owner) {
            return false;
        }
        let Some(limit) = self
            .ai_table_of(tick, ids, attacker)
            .map(|ai| ai.stop_chase_distance.unwrap_or(STOP_CHASE_DISTANCE))
            .filter(|limit| *limit > 0.0)
        else {
            return false;
        };
        let world = tick.state();
        let (Some(me), Some(them)) = (world.get(ids.body, attacker), world.get(ids.body, target))
        else {
            return false;
        };
        let flat = me.0.position.minus(them.0.position).length_squared();
        let rise = Self::feet(world, ids, attacker) - Self::feet(world, ids, target);
        flat + rise * rise > limit * limit && self.fleeing(tick, ids, me.0.position, target)
    }

    pub(super) fn computer_controlled(&self, player: PlayerId) -> bool {
        self.alliances.is_map_player(player)
            || self.ai_seeds.iter().any(|seed| seed.player == player)
    }

    pub(super) fn target_is_live(&self, tick: &Tick<'_>, ids: Ids, target: Entity) -> bool {
        let dead = tick
            .state()
            .get(ids.health, target)
            .is_none_or(|health| health.0.is_dead());
        !dead
            && !tick.state().contains(ids.concealed, target)
            && !self.is_unattackable(tick.state(), ids, target)
    }

    pub(super) fn horde_rangefinder(&self, horde: &Horde) -> Option<f32> {
        self.horde_rangefinders
            .get(usize::from(horde.formation))
            .copied()
            .flatten()
    }

    pub(super) fn horde_rangefinder_weapon(
        &self,
        horde: &Horde,
    ) -> Option<(
        mint_sage_combat::WeaponId,
        &mint_sage_combat::WeaponTemplate,
    )> {
        let id = self
            .horde_rangefinder_weapons
            .get(usize::from(horde.formation))
            .copied()
            .flatten()?;
        Some((id, self.combat.weapons.get(id)?))
    }

    fn horde_weapon_refused(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        victim: Entity,
    ) -> bool {
        let world = tick.state();
        let primary = |member: Entity| match self.selected_set(tick, ids, member) {
            Some((_, set)) => set
                .arms
                .first()
                .copied()
                .flatten()
                .map(|arm| (member, arm.weapon)),
            None => world
                .get(ids.armed, member)
                .map(|armed| (member, armed.weapon)),
        };
        let representative = if self.has_kind_bit(world, ids, horde_entity, "COMBO_HORDE") {
            let mut longest: Option<(f32, (Entity, mint_sage_combat::WeaponId))> = None;
            for (member, weapon) in horde.members.iter().filter_map(|member| primary(*member)) {
                let Some(template) = self.combat.weapons.get(weapon) else {
                    continue;
                };
                let reach = self.attack_range_of(world, ids, member, template);
                if longest.is_none_or(|(best, _)| reach > best) {
                    longest = Some((reach, (member, weapon)));
                }
            }
            longest.map(|(_, found)| found)
        } else {
            horde
                .members
                .iter()
                .find(|member| !self.has_kind_bit(world, ids, **member, "BANNER"))
                .or_else(|| horde.members.first())
                .and_then(|member| primary(*member))
        };
        let Some((member, weapon)) = representative else {
            return false;
        };
        let armour = crate::combat::armour_of(
            &self.combat,
            world.get(ids.armour, victim).copied(),
            &self.unarmoured,
        );
        let spent = world.get(ids.armed, member).is_some_and(|armed| {
            let mut state = armed.state;
            armed.weapon == weapon
                && state.status(tick.frame()) == mint_sage_combat::WeaponStatus::OutOfAmmo
                && self
                    .combat
                    .weapons
                    .get(weapon)
                    .is_some_and(|template| !template.auto_reloads_clip)
        });
        spent || !self.combat.weapons.valid_against(weapon, armour)
    }

    pub(super) fn horde_rangefinder_min(&self, horde: &Horde) -> f32 {
        self.horde_rangefinder_weapon(horde)
            .map_or(0.0, |(_, template)| template.minimum_reach())
    }

    pub(super) fn rangefinder_fired(&self, horde: &Horde, now: Frame) -> bool {
        self.horde_rangefinder_weapon(horde).is_none()
            || horde
                .rangefinder
                .is_some_and(|state| state.last_shot() == Some(now))
    }

    fn fire_rangefinder(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        goal: Entity,
    ) -> Result<()> {
        let (Some(range), Some((weapon, template))) = (
            self.horde_rangefinder(horde),
            self.horde_rangefinder_weapon(horde),
        ) else {
            return Ok(());
        };
        let bonuses = tick.state().get(ids.bonuses, horde_entity).map(|b| b.0);
        let (Some(here), Some(theirs)) = (
            tick.state()
                .get(ids.body, horde_entity)
                .map(|b| b.0.position),
            tick.state().get(ids.body, goal).map(|b| b.0.position),
        ) else {
            return Ok(());
        };
        let reaches = in_range(
            here,
            Self::mover_radius(tick, ids, horde_entity),
            theirs,
            Self::mover_radius(tick, ids, goal),
            self.horde_rangefinder_min(horde),
            bonuses.map_or(range, |bonuses| bonuses.range(range)),
        );
        if !reaches {
            return Ok(());
        }
        let now = tick.frame();
        let mut state = horde
            .rangefinder
            .unwrap_or_else(|| mint_sage_combat::WeaponState::new(template));
        if state.status(now) != mint_sage_combat::WeaponStatus::ReadyToFire {
            return Ok(());
        }
        let rate = bonuses.map_or(1.0, |bonuses| bonuses.rate_of_fire);
        state.fire_at_rate(template, now, rate, tick.rng());
        if let Some(live) = tick.state_mut().get_mut(ids.horde, horde_entity) {
            live.rangefinder = Some(state);
        }
        let locked: Vec<u8> = (0..template.nuggets.len())
            .filter_map(
                |index| match self.combat.effects.get(weapon.index(), index) {
                    Some(mint_sage_combat::EffectNugget::HordeAttack(nugget)) => nugget
                        .lock_weapon_slot
                        .as_deref()
                        .and_then(mint_sage_combat::WeaponSlot::parse)
                        .and_then(|slot| u8::try_from(slot.index()).ok()),
                    _ => None,
                },
            )
            .collect();
        if let Some(slot) = locked.first().copied() {
            let members = self.horde_members(tick, ids, &[horde_entity]);
            let effects: Vec<_> = members
                .into_iter()
                .chain([horde_entity])
                .map(|entity| {
                    (
                        entity,
                        mint_sage_modules::Effect::LockWeaponSlotTemporarily(slot),
                    )
                })
                .collect();
            self.apply_effects(tick, ids, effects)?;
        }
        Ok(())
    }

    fn ground_test<'a>(
        &'a self,
        tick: &'a Tick<'_>,
        ids: Ids,
        mover: Entity,
        here: Vec2,
    ) -> impl Fn(Vec2) -> bool + 'a {
        let class = self.mover_class(tick, ids, mover);
        let start = self.passable_cell(tick, ids, here);
        let view = self.dynamic_view(tick, ids);
        move |at: Vec2| {
            self.passability.cell_at(at).is_some_and(|cell| {
                view.is_passable(cell, class)
                    && start.is_none_or(|s| self.reachable(class, s, cell))
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn attack_shortcut(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        mover: Entity,
        (here, mover_radius): (Vec2, f32),
        (theirs, their_radius): (Vec2, f32),
        (min_range, max_range): (f32, f32),
        allowed: impl Fn(Vec2) -> bool,
    ) -> Option<Vec2> {
        let direction = theirs.minus(here).normalised()?;
        let class = self.mover_class(tick, ids, mover);
        let ground = self.ground_test(tick, ids, mover, here);
        (1u8..=SHORTCUT_STEPS)
            .map(|i| here.plus(direction.scale(SHORTCUT_STEP * f32::from(i))))
            .find(|&at| {
                crate::combat::in_range_beyond(
                    at,
                    mover_radius,
                    theirs,
                    their_radius,
                    SHORTCUT_EXTRA,
                    min_range,
                    max_range,
                ) && ground(at)
                    && allowed(at)
                    && self.line_clear(tick, ids, class, here, at)
            })
    }

    fn ranged_horde_spot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        horde: &Horde,
        goal: Entity,
    ) -> Result<Option<Vec2>> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("ranged_horde_spot"));
        let Some(range) = self.horde_rangefinder(horde) else {
            return Ok(None);
        };
        let range = tick
            .state()
            .get(ids.bonuses, horde_entity)
            .map_or(range, |bonuses| bonuses.0.range(range));
        let (Some(here), Some(theirs)) = (
            tick.state()
                .get(ids.body, horde_entity)
                .map(|b| b.0.position),
            tick.state().get(ids.body, goal).map(|b| b.0.position),
        ) else {
            return Ok(None);
        };
        let (mine_r, their_r) = (
            Self::mover_radius(tick, ids, horde_entity),
            Self::mover_radius(tick, ids, goal),
        );
        let minimum = self.horde_rangefinder_min(horde);
        let reaches = |at: Vec2| in_range(at, mine_r, theirs, their_r, minimum, range);
        let routed = tick
            .state()
            .get(ids.path, horde_entity)
            .filter(|path| path.kind == PathKind::Closing)
            .map(|path| path.goal);
        let mut plan = tick
            .state()
            .get(ids.ranged_approach, horde_entity)
            .copied()
            .filter(|plan| Some(plan.goal) == routed);
        if plan.is_some_and(|plan| plan.victim != goal) {
            tick.state_mut().remove(ids.path, horde_entity);
            tick.state_mut().remove(ids.ranged_approach, horde_entity);
            plan = None;
        }
        match plan {
            None if reaches(here) => return Ok(Some(here)),
            None => {}
            Some(plan) => {
                if !plan.attack_path && reaches(here) {
                    return Ok(Some(here));
                }
                let now = tick.frame().get();
                let cooled = tick
                    .state()
                    .get(ids.replanned_at, horde_entity)
                    .is_none_or(|last| {
                        now.saturating_sub(last.0.get()) >= super::movement::REPLAN_COOLDOWN_FRAMES
                    });
                if !(cooled && super::movement::goal_moved(here, plan.victim_at, theirs)) {
                    return Ok(plan.attack_path.then_some(plan.goal));
                }
                if reaches(here) {
                    return Ok(Some(here));
                }
                tick.state_mut().insert(
                    ids.ranged_approach,
                    horde_entity,
                    RangedApproach {
                        victim_at: theirs,
                        ..plan
                    },
                )?;
            }
        }
        if self.fleeing(tick, ids, here, goal) {
            return Ok(None);
        }
        let away = here.minus(theirs);
        let facing = Facing::from_vector(away.scale(-1.0)).unwrap_or(Facing::EAST);
        let Some((square, _, offset)) = self.claim_at(tick, ids, horde_entity, here, facing) else {
            return Ok(None);
        };
        let geometry = self.passability.geometry;
        let cell_size = geometry.cell_size();
        #[allow(clippy::cast_precision_loss)]
        let search = (self.max_cells_adjust_destination as f32 / core::f32::consts::PI).sqrt();
        let reach = away.length() + (search + 2.0) * cell_size + 2.0 * square.half_along;
        let units = self.unit_cells(tick, ids, horde_entity, None, here, reach);
        let allies = |at: Vec2| {
            units.allied_goals(square_cells(geometry, at.plus(offset), square.half_along))
        };
        let ground = self.ground_test(tick, ids, horde_entity, here);
        if let Some(at) = self.attack_shortcut(
            tick,
            ids,
            horde_entity,
            (here, mine_r),
            (theirs, their_r),
            (minimum, range),
            |at| allies(at).is_some(),
        ) {
            return Ok(Some(at));
        }
        let mut fewest: Option<(u32, Vec2)> = None;
        for &(x, y) in melee_search_offsets(self.max_cells_adjust_destination) {
            #[allow(clippy::cast_precision_loss)]
            let at = here.plus(Vec2::new(x as f32 * cell_size, y as f32 * cell_size));
            if at.minus(here).length() < 5.0 || !reaches(at) {
                continue;
            }
            let Some(held) = allies(at) else {
                continue;
            };
            if fewest.is_some_and(|(best, _)| best <= held) || !ground(at) {
                continue;
            }
            fewest = Some((held, at));
            if held <= 1 {
                break;
            }
        }
        Ok(fewest.map(|(_, at)| at))
    }

    pub(super) fn ranged_plan_victim_at(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        planned: Vec2,
        victim: Entity,
    ) -> Option<Vec2> {
        tick.state()
            .get(ids.ranged_approach, entity)
            .filter(|plan| plan.goal == planned && plan.victim == victim)
            .map(|plan| plan.victim_at)
    }

    pub(super) fn walking_attack_path(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        victim: Entity,
    ) -> bool {
        tick.state()
            .get(ids.path, entity)
            .filter(|path| path.kind == PathKind::Closing)
            .is_some_and(|path| {
                tick.state()
                    .get(ids.ranged_approach, entity)
                    .is_some_and(|plan| {
                        plan.attack_path && plan.goal == path.goal && plan.victim == victim
                    })
            })
    }

    pub(super) fn note_ranged_plan(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Vec2,
        (victim, victim_at): (Entity, Vec2),
        attack_path: bool,
    ) -> Result<()> {
        let planned_now = tick
            .state()
            .get(ids.replanned_at, horde_entity)
            .is_some_and(|last| last.0 == tick.frame())
            && tick
                .state()
                .get(ids.path, horde_entity)
                .is_some_and(|path| path.goal == goal);
        if planned_now {
            tick.state_mut().insert(
                ids.ranged_approach,
                horde_entity,
                RangedApproach {
                    goal,
                    victim,
                    victim_at,
                    attack_path,
                },
            )?;
        }
        Ok(())
    }

    fn live_member_of_horde_of(&self, tick: &Tick<'_>, ids: Ids, member: Entity) -> Option<Entity> {
        let horde = tick.state().get(ids.member, member)?.horde;
        tick.state()
            .get(ids.horde, horde)?
            .members
            .iter()
            .copied()
            .find(|candidate| self.target_is_live(tick, ids, *candidate))
    }

    pub(super) fn is_unattackable(&self, world: &World, ids: Ids, target: Entity) -> bool {
        world
            .get(ids.shape, target)
            .is_some_and(|shape| shape.kind_of.contains(KindOf::UNATTACKABLE))
            || self
                .modules
                .as_ref()
                .is_some_and(|wiring| wiring.carries_status(world, target, "UNATTACKABLE"))
            || self
                .contained_status(world, ids, target)
                .is_some_and(|status| {
                    status.has(super::contain::ContainStatus::ENCLOSED)
                        || status.has(super::contain::ContainStatus::UNATTACKABLE)
                })
            || world
                .get(ids.rebuild_hole, target)
                .is_some_and(|hole| hole.reconstruction.is_some())
    }

    pub(super) fn end_group_closing(&self, tick: &mut Tick<'_>, ids: Ids, horde_entity: Entity) {
        let is_explicit_move = tick
            .state()
            .get(ids.path, horde_entity)
            .is_some_and(|path| path.kind == PathKind::Ordered);
        if !is_explicit_move {
            tick.state_mut().remove(ids.path, horde_entity);
        }
        tick.state_mut().remove(ids.ranged_approach, horde_entity);
    }

    pub(super) fn foe_of(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Foe> {
        let health = tick.state().get(ids.health, entity)?;
        if health.0.is_dead() || tick.state().contains(ids.concealed, entity) {
            return None;
        }
        self.foe_unchecked(tick, ids, entity)
    }

    pub(super) fn foe_unchecked(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Foe> {
        let body = tick.state().get(ids.body, entity)?;
        let shape = tick.state().get(ids.shape, entity)?;
        Some(Foe {
            entity,
            position: body.0.position,
            z: Self::feet(tick.state(), ids, entity),
            radius: shape.radius,
            height: shape.height,
            is_structure: shape.kind_of.contains(KindOf::STRUCTURE),
            airborne: self.airborne_category(tick.state(), ids, entity),
            unattackable: self.is_unattackable(tick.state(), ids, entity),
            not_autoacquirable: shape.kind_of.contains(KindOf::NOT_AUTOACQUIRABLE),
            ai_kind_of: self.ai_kind_of_of(tick.state(), ids, entity),
        })
    }

    pub(super) fn ai_kind_of_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<mint_sage_modules::AiKindOf> {
        let kind = world.get(ids.kind, entity)?;
        self.kinds.get(*kind)?.record.ai_kind_of
    }

    pub(super) fn airborne_category(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<crate::templates::AirborneCategory> {
        let airborne = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.flight)
            .and_then(|flight| flight.airborne)?;
        let at = world.get(ids.body, entity)?.0.position;
        let ground = self.passability.heights.height_at_point(at)?;
        (Self::feet(world, ids, entity) - ground > airborne.above).then_some(airborne.category)
    }

    pub(super) fn horde_victims(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        member: Entity,
    ) -> Option<Vec<Foe>> {
        let horde_entity = tick.state().get(ids.member, member)?.horde;
        let target = tick
            .state()
            .get(ids.attack_order, horde_entity)
            .map(|order| order.target)
            .or_else(|| {
                tick.state()
                    .get(ids.horde_target, horde_entity)
                    .and_then(|hordetarget| hordetarget.target)
            })?;
        Some(match tick.state().get(ids.member, target) {
            Some(target_member) => {
                let target_horde = tick
                    .state()
                    .get(Self::horde_ids(ids).horde, target_member.horde)
                    .cloned()?;
                target_horde
                    .members
                    .iter()
                    .filter_map(|candidate| self.foe_of(tick, ids, *candidate))
                    .collect()
            }
            None => self.foe_of(tick, ids, target).into_iter().collect(),
        })
    }

    pub(super) fn adjust_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        horde: Entity,
        clicked: Option<Entity>,
        spillover: Option<(&[(Foe, PlayerId)], PlayerId)>,
    ) -> Option<Entity> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("adjust_victim"));
        let mut members: Vec<Foe> = tick
            .state()
            .get(ids.horde, horde)?
            .members
            .iter()
            .filter_map(|member| self.foe_of(tick, ids, *member))
            .filter(|foe| !foe.unattackable)
            .collect();
        members.sort_by_key(|foe| foe.entity);
        if members.is_empty() {
            return None;
        }
        let template = tick
            .state()
            .get(ids.armed, attacker)
            .and_then(|armed| self.combat.weapons.get(armed.weapon));
        let melee = template.is_some_and(|template| template.melee);
        if !melee {
            if let Some(clicked) = clicked {
                if members.iter().any(|foe| foe.entity == clicked) {
                    return Some(clicked);
                }
            }
        }
        let range = template.map_or(0.0, |template| {
            self.attack_range_of(tick.state(), ids, attacker, template)
        });
        let from = tick.state().get(ids.body, attacker)?.0.position;
        let tick: &Tick<'_> = tick;
        let score = |tick: &Tick<'_>, foe: &Foe| {
            let (dx, dy) = (foe.position.x - from.x, foe.position.y - from.y);
            let squared = dx * dx + dy * dy;
            let in_range = squared <= range * range;
            let discounted = in_range
                && (melee || !self.view_blocked(tick, ids, from, foe.position, foe.radius));
            if discounted {
                squared * 0.25
            } else {
                squared
            }
        };
        let any_member_in_range = members.iter().any(|foe| {
            let (dx, dy) = (foe.position.x - from.x, foe.position.y - from.y);
            dx * dx + dy * dy <= range * range
        });
        if melee && !any_member_in_range {
            if let Some((living, _)) = spillover {
                let attacker_radius = tick
                    .state()
                    .get(ids.shape, attacker)
                    .map_or(0.0, |shape| shape.radius);
                let adjacent = living
                    .iter()
                    .filter(|(foe, _)| {
                        foe.entity != attacker
                            && !foe.unattackable
                            && !foe.is_structure
                            && !tick.state().contains(ids.horde, foe.entity)
                            && self.object_hostile(tick.state(), attacker, foe.entity)
                    })
                    .filter(|(foe, _)| {
                        in_range(from, attacker_radius, foe.position, foe.radius, 0.0, range)
                    })
                    .min_by(|(a, _), (b, _)| score(tick, a).total_cmp(&score(tick, b)))
                    .map(|(foe, _)| foe.entity);
                if let Some(adjacent) = adjacent {
                    return Some(adjacent);
                }
            }
        }
        members
            .iter()
            .min_by(|a, b| score(tick, a).total_cmp(&score(tick, b)))
            .map(|foe| foe.entity)
    }

    fn member_range_at(&self, world: &World, ids: Ids, member: &Foe, foe: &Foe, range: f32) -> f32 {
        world
            .get(ids.armed, member.entity)
            .and_then(|armed| self.combat.weapons.get(armed.weapon))
            .map_or(range, |template| {
                self.attack_range_at(world, ids, member.entity, template, member.z - foe.z)
            })
    }

    pub(super) fn ranged_member_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        range: f32,
        current: Option<Entity>,
        living: &[(Foe, PlayerId)],
    ) -> Result<Option<Foe>> {
        let now = tick.frame();
        let horde_state = tick
            .state()
            .get(ids.member, member.entity)
            .and_then(|entry| tick.state().get(ids.horde, entry.horde))
            .map(|horde| {
                (
                    self.horde_rangefinder(horde).is_some(),
                    self.rangefinder_fired(horde, now),
                )
            });
        if let Some((true, fired)) = horde_state {
            return self
                .rangefinder_member_victim(tick, ids, member, range, current, living, fired);
        }
        let released = tick
            .state()
            .get(ids.member, member.entity)
            .and_then(|entry| {
                let horde = tick.state().get(ids.horde, entry.horde)?;
                let rank = horde.slots.get(usize::from(entry.slot?))?.rank;
                let table = self.formations.get(usize::from(horde.formation))?;
                Some(table.settings().ranks_to_release.contains(&rank))
            })
            .unwrap_or(false);
        let victims = if released {
            self.horde_victims(tick, ids, member.entity)
        } else {
            None
        };
        let Some(victims) = victims else {
            return Ok(None);
        };
        let from_horde = victims
            .first()
            .is_some_and(|foe| tick.state().contains(ids.member, foe.entity));
        if !from_horde {
            return Ok(victims.into_iter().next());
        }
        let reaches = |foe: &Foe| {
            in_range(
                member.position,
                member.radius,
                foe.position,
                foe.radius,
                0.0,
                self.member_range_at(tick.state(), ids, member, foe, range),
            )
        };
        if let Some(foe) = victims
            .iter()
            .find(|foe| Some(foe.entity) == current && reaches(foe))
        {
            return Ok(Some(*foe));
        }
        let mut best: Option<(f32, Foe)> = None;
        for foe in victims {
            let (dx, dy, dz) = (
                foe.position.x - member.position.x,
                foe.position.y - member.position.y,
                foe.z - member.z,
            );
            let distance =
                ((dx * dx + dy * dy + dz * dz).sqrt() - member.radius - foe.radius).max(0.0);
            let range = self.member_range_at(tick.state(), ids, member, &foe, range);
            if range > 0.0 && distance >= range {
                continue;
            }
            let jittered = if range > 0.0 {
                let draw = tick.rng().next_f32();
                distance * (SELECT_JITTER_LOW + (SELECT_JITTER_HIGH - SELECT_JITTER_LOW) * draw)
            } else {
                distance
            };
            if best.is_none_or(|(held, _)| jittered < held) {
                best = Some((jittered, foe));
            }
        }
        Ok(best.map(|(_, foe)| foe))
    }

    #[allow(clippy::too_many_arguments)]
    fn rangefinder_member_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        member: &Foe,
        range: f32,
        current: Option<Entity>,
        living: &[(Foe, PlayerId)],
        fired: bool,
    ) -> Result<Option<Foe>> {
        let keep = current.and_then(|victim| {
            let at = living
                .binary_search_by_key(&victim, |(foe, _)| foe.entity)
                .ok()?;
            living.get(at).map(|(foe, _)| *foe)
        });
        if !fired {
            return Ok(keep);
        }
        let Some(entry) = tick.state().get(ids.member, member.entity).copied() else {
            return Ok(keep);
        };
        let rank = entry.slot.and_then(|slot| {
            let horde = tick.state().get(ids.horde, entry.horde)?;
            Some((horde.slots.get(usize::from(slot))?.rank, horde.formation))
        });
        let Some((rank, formation)) = rank else {
            return Ok(keep);
        };
        if super::upgrades::is_moving(tick, ids, member.entity) {
            return Ok(keep);
        }
        let melee = tick
            .state()
            .get(ids.armed, member.entity)
            .and_then(|armed| self.combat.weapons.get(armed.weapon))
            .is_some_and(|template| template.melee);
        if melee {
            let now = tick.frame();
            if Self::ai_busy(tick.state(), ids, member.entity, now) {
                Self::idle_busy_ai(tick, ids, member.entity);
            }
            if !tick.state().contains(ids.member_attack, member.entity) {
                self.engage_on_spot(tick, ids, member, keep.map(|foe| foe.entity), living)?;
            }
            return Ok(keep);
        }
        let released = self
            .formations
            .get(usize::from(formation))
            .is_some_and(|table| table.settings().ranks_to_release.contains(&rank));
        if !released {
            return Ok(keep);
        }
        let Some(victims) = self.horde_victims(tick, ids, member.entity) else {
            return Ok(keep);
        };
        if keep.is_some_and(|held| victims.iter().any(|foe| foe.entity == held.entity)) {
            return Ok(keep);
        }
        let from_horde = victims
            .first()
            .is_some_and(|foe| tick.state().contains(ids.member, foe.entity));
        if !from_horde {
            return Ok(victims.into_iter().next().or(keep));
        }
        let mut best: Option<(f32, Foe)> = None;
        for foe in victims {
            let (dx, dy, dz) = (
                foe.position.x - member.position.x,
                foe.position.y - member.position.y,
                foe.z - member.z,
            );
            let distance =
                ((dx * dx + dy * dy + dz * dz).sqrt() - member.radius - foe.radius).max(0.0);
            let range = self.member_range_at(tick.state(), ids, member, &foe, range);
            if range > 0.0 && distance >= range {
                continue;
            }
            let jittered = if range > 0.0 {
                let draw = tick.rng().next_f32();
                distance * (SELECT_JITTER_LOW + (SELECT_JITTER_HIGH - SELECT_JITTER_LOW) * draw)
            } else {
                distance
            };
            if best.is_none_or(|(held, _)| jittered < held) {
                best = Some((jittered, foe));
            }
        }
        Ok(best.map(|(_, foe)| foe).or(keep))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance_group_attacks(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
        grid: &TargetGrid,
        living: &[(Foe, PlayerId)],
        global_max_radius: f32,
        now: Frame,
    ) -> Result<()> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("advance_group_attacks"));
        let horde_ids = Self::horde_ids(ids);
        let hordes: Vec<Entity> = tick
            .state()
            .store(horde_ids.horde)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let formations_on = self.modes(tick, ids).formation == FormationMode::On;
        let mut fighting: BTreeSet<Entity> = BTreeSet::new();
        let decide_perf = crate::perf::sub(crate::perf::sub_row!("decide_group_attacks"));
        let decided: Vec<GroupDecision> = hordes
            .iter()
            .map(|&horde_entity| self.decide_group_attack(tick, ids, horde_entity))
            .collect();
        drop(decide_perf);
        for (horde_entity, decision) in hordes.into_iter().zip(decided) {
            debug_assert_eq!(
                format!("{decision:?}"),
                format!("{:?}", self.decide_group_attack(tick, ids, horde_entity)),
                "an earlier horde's turn changed horde {}'s decision",
                horde_entity.index()
            );
            let facts = match decision {
                GroupDecision::Skip => continue,
                GroupDecision::Ground => {
                    if let Some(order) = tick.state().get(ids.ground_order, horde_entity).copied() {
                        self.approach_ground(
                            tick,
                            ids,
                            horde_entity,
                            order,
                            formations_on,
                            budgets,
                        )?;
                    }
                    continue;
                }
                GroupDecision::Unarmed => {
                    tick.state_mut().remove(ids.attack_order, horde_entity);
                    tick.state_mut().remove(ids.horde_target, horde_entity);
                    if let (Some(attack_move), Some(body)) = (
                        tick.state()
                            .get(ids.attack_move_order, horde_entity)
                            .copied(),
                        tick.state().get(ids.body, horde_entity).copied(),
                    ) {
                        self.advance_attack_move(
                            tick,
                            ids,
                            horde_entity,
                            body,
                            attack_move,
                            budgets,
                        )?;
                    } else {
                        self.end_group_closing(tick, ids, horde_entity);
                    }
                    continue;
                }
                GroupDecision::Engage(facts) => facts,
            };
            let Some(horde) = tick.state().get(horde_ids.horde, horde_entity).cloned() else {
                continue;
            };
            let GroupFacts {
                max_range,
                body,
                owner,
                guarded,
                scan,
            } = facts;

            let mut ordered = tick.state().get(ids.attack_order, horde_entity).copied();
            if let Some(order) = ordered.as_mut() {
                if !self.target_is_live(tick, ids, order.target) {
                    if let Some(next) = self.live_member_of_horde_of(tick, ids, order.target) {
                        order.target = next;
                        tick.state_mut()
                            .insert(ids.attack_order, horde_entity, *order)?;
                    }
                }
            }
            if let (Some(order), Some(_)) = (ordered.filter(|order| order.relation_free), owner) {
                let target_owner = tick.state().get(ids.owner, order.target).map(|o| o.0);
                let changed = target_owner != order.order_owner;
                let still_non_enemy =
                    !self.object_hostile(tick.state(), horde_entity, order.target);
                if changed && still_non_enemy {
                    tick.state_mut().remove(ids.attack_order, horde_entity);
                    ordered = None;
                }
            }
            let ordered_target = ordered
                .filter(|order| {
                    order.relation_free
                        || self.object_hostile(tick.state(), horde_entity, order.target)
                })
                .map(|order| order.target)
                .filter(|target| self.target_is_live(tick, ids, *target));
            if ordered.is_some() && ordered_target.is_none() {
                tick.state_mut().remove(ids.attack_order, horde_entity);
            }

            let attack_move = tick
                .state()
                .get(ids.attack_move_order, horde_entity)
                .copied();
            let under_plain_move = attack_move.is_none()
                && tick
                    .state()
                    .get(ids.path, horde_entity)
                    .is_some_and(|path| path.kind == PathKind::Ordered);

            let existing = tick.state().get(ids.horde_target, horde_entity).copied();
            let melee_horde = self.is_melee_horde(tick.state(), ids, horde_entity);
            let sticky = (ordered_target.is_none() && !under_plain_move && !guarded)
                .then_some(())
                .and_then(|()| {
                    existing
                        .and_then(|hordetarget| hordetarget.target)
                        .filter(|target| {
                            self.target_is_live(tick, ids, *target)
                                && self.object_hostile(tick.state(), horde_entity, *target)
                                && (melee_horde
                                    || owner.is_none_or(|owner| {
                                        !self.chase_leash_spent(
                                            tick,
                                            ids,
                                            horde_entity,
                                            owner,
                                            *target,
                                        )
                                    }))
                        })
                });
            let mut horde_target = existing;
            if ordered_target.is_none() && sticky.is_none() && !under_plain_move {
                let due = existing.is_none_or(|hordetarget| now >= hordetarget.next_acquire);
                let common = (scan.is_some() || attack_move.is_some())
                    .then(|| self.common_target(tick.state(), ids, horde_entity, living, true))
                    .flatten();
                if let Some(common) = common {
                    horde_target = Some(HordeTarget {
                        target: Some(common.entity),
                        next_acquire: existing.map_or(now, |hordetarget| hordetarget.next_acquire),
                    });
                } else if due {
                    let human_ranged = (!melee_horde
                        && owner.is_some_and(|owner| !self.computer_controlled(owner)))
                        || self.stands_ground(tick.state(), ids, horde_entity);
                    let own_reach = self.horde_rangefinder(&horde).map_or(max_range, |range| {
                        tick.state()
                            .get(ids.bonuses, horde_entity)
                            .map_or(range, |bonuses| bonuses.0.range(range))
                    });
                    let retaliate_only = scan.is_some_and(|scan| scan.retaliate_only);
                    let found = match (owner, scan) {
                        (Some(owner), Some(scan)) => self.scan_for_horde_target(
                            tick.state(),
                            ids,
                            grid,
                            living,
                            global_max_radius,
                            body.0.position,
                            Self::mover_radius(tick, ids, horde_entity),
                            if human_ranged {
                                scan.reach.min(own_reach)
                            } else {
                                scan.reach
                            },
                            owner,
                            scan.structures,
                            horde_entity,
                            retaliate_only,
                        ),
                        (Some(owner), None) if attack_move.is_some() => self.scan_for_horde_target(
                            tick.state(),
                            ids,
                            grid,
                            living,
                            global_max_radius,
                            body.0.position,
                            Self::mover_radius(tick, ids, horde_entity),
                            if human_ranged {
                                max_range.min(own_reach)
                            } else {
                                max_range
                            },
                            owner,
                            false,
                            horde_entity,
                            retaliate_only,
                        ),
                        _ => None,
                    };
                    let cadence = scan.map_or(MOOD_ATTACK_CHECK, |scan| scan.cadence);
                    horde_target = Some(HordeTarget {
                        target: found,
                        next_acquire: now.checked_add(cadence).unwrap_or(now),
                    });
                }
            }
            if horde_target != existing {
                match horde_target {
                    Some(hordetarget) => {
                        tick.state_mut()
                            .insert(ids.horde_target, horde_entity, hordetarget)?;
                    }
                    None => {
                        tick.state_mut().remove(ids.horde_target, horde_entity);
                    }
                }
            }

            let resolved = ordered_target.or(if under_plain_move {
                None
            } else {
                sticky.or(horde_target
                    .and_then(|hordetarget| hordetarget.target)
                    .filter(|target| self.object_hostile(tick.state(), horde_entity, *target)))
            });
            let Some(target) = resolved else {
                if let Some(attack_move) = attack_move {
                    self.advance_attack_move(tick, ids, horde_entity, body, attack_move, budgets)?;
                } else {
                    self.end_group_closing(tick, ids, horde_entity);
                }
                continue;
            };
            let goal = tick
                .state()
                .get(ids.member, target)
                .map_or(target, |member| member.horde);
            let Some(target_body) = tick.state().get(ids.body, goal).copied() else {
                self.end_group_closing(tick, ids, horde_entity);
                continue;
            };
            if let Some(owner) = owner {
                self.attack_state_uncohere(tick, ids, horde_entity, owner, target);
            }
            if formations_on && self.melee_rangefinder(&horde).is_none() {
                if self.horde_weapon_refused(tick, ids, horde_entity, &horde, target) {
                    tick.state_mut().remove(ids.attack_order, horde_entity);
                    tick.state_mut().remove(ids.horde_target, horde_entity);
                    self.end_group_closing(tick, ids, horde_entity);
                    continue;
                }
                self.fire_rangefinder(tick, ids, horde_entity, &horde, goal)?;
            }
            let dx = body.0.position.x - target_body.0.position.x;
            let dy = body.0.position.y - target_body.0.position.y;
            let holds = ordered_target.is_none()
                && attack_move.is_none()
                && scan.is_some_and(|scan| !scan.pursues);
            let stand_off_spot = horde
                .members
                .iter()
                .filter_map(|member| {
                    let armed = tick.state().get(ids.armed, *member)?;
                    let template = self.combat.weapons.get(armed.weapon)?;
                    Some(self.min_range_of(tick.state(), *member, template))
                })
                .reduce(f32::min)
                .filter(|min_range| *min_range > 0.0 && !holds)
                .and_then(|min_range| {
                    let straight = super::shooting::stand_off(
                        body.0.position,
                        target_body.0.position,
                        Self::front_reach(tick, ids, horde_entity, goal),
                        (min_range, max_range),
                    )?;
                    let flagged = horde
                        .members
                        .iter()
                        .any(|member| self.needs_line_of_sight(tick, ids, *member));
                    let target_radius = tick
                        .state()
                        .get(ids.shape, goal)
                        .map_or(0.0, |shape| shape.radius);
                    Some(
                        self.los_search_goal(
                            tick,
                            ids,
                            horde_entity,
                            if flagged {
                                super::shooting::AttackSpot::InRangeInView
                            } else {
                                super::shooting::AttackSpot::Straight
                            },
                            body.0.position,
                            Self::mover_radius(tick, ids, horde_entity),
                            target_body.0.position,
                            target_radius,
                            horde
                                .members
                                .iter()
                                .copied()
                                .find(|member| tick.state().contains(ids.armed, *member))
                                .zip(
                                    self.goal_members(tick, ids, goal)
                                        .first()
                                        .map(|foe| foe.z + foe.height * 0.5),
                                ),
                            (min_range, max_range),
                            budgets,
                            (straight, 0.0),
                        )
                        .0,
                    )
                });
            if formations_on && stand_off_spot.is_none() && self.melee_rangefinder(&horde).is_some()
            {
                fighting.insert(horde_entity);
                self.advance_horde_melee(
                    tick,
                    ids,
                    horde_entity,
                    goal,
                    holds,
                    budgets,
                    &super::horde_melee::Snapshot {
                        grid,
                        living,
                        global_max_radius,
                    },
                    now,
                )?;
                continue;
            }
            if !formations_on {
                tick.state_mut().remove(ids.attack_order, horde_entity);
                tick.state_mut().remove(ids.horde_target, horde_entity);
                self.end_group_closing(tick, ids, horde_entity);
            } else if let Some(spot) = stand_off_spot {
                self.close_on_target(
                    tick,
                    ids,
                    horde_entity,
                    body.0.position,
                    spot,
                    0.0,
                    PathKind::Closing,
                    budgets,
                )?;
            } else if let Some(spot) = match self.horde_rangefinder(&horde) {
                Some(_) => self.ranged_horde_spot(tick, ids, horde_entity, &horde, goal)?,
                None => None,
            } {
                if spot.minus(body.0.position).length() < 1.0 {
                    self.end_group_closing(tick, ids, horde_entity);
                } else {
                    self.close_on_attack_spot(
                        tick,
                        ids,
                        horde_entity,
                        body.0.position,
                        spot,
                        budgets,
                    )?;
                    self.note_ranged_plan(
                        tick,
                        ids,
                        horde_entity,
                        spot,
                        (goal, target_body.0.position),
                        true,
                    )?;
                }
            } else if self.horde_rangefinder(&horde).is_none()
                && (self.horde_in_contact(tick, ids, &horde, goal) || {
                    let stop = Self::front_reach(tick, ids, horde_entity, goal);
                    dx * dx + dy * dy <= stop * stop
                })
            {
                self.end_group_closing(tick, ids, horde_entity);
            } else if holds || self.stands_ground(tick.state(), ids, horde_entity) {
                self.end_group_closing(tick, ids, horde_entity);
            } else {
                let target_reach = tick
                    .state()
                    .get(ids.shape, target)
                    .map_or(0.0, |shape| shape.radius);
                self.close_on_target(
                    tick,
                    ids,
                    horde_entity,
                    body.0.position,
                    target_body.0.position,
                    target_reach,
                    PathKind::Closing,
                    budgets,
                )?;
                if self.horde_rangefinder(&horde).is_some() {
                    let at = target_body.0.position;
                    self.note_ranged_plan(tick, ids, horde_entity, at, (goal, at), false)?;
                }
            }
        }
        let ended: Vec<Entity> = tick
            .state()
            .store(ids.horde_melee)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|horde| !fighting.contains(horde))
                    .collect()
            })
            .unwrap_or_default();
        for horde_entity in ended {
            self.end_melee(tick, ids, horde_entity)?;
        }
        Ok(())
    }

    fn decide_group_attack(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
    ) -> GroupDecision {
        let world = tick.state();
        if world
            .get(ids.path, horde_entity)
            .is_some_and(|path| path.kind == PathKind::Ability)
        {
            return GroupDecision::Skip;
        }
        if world
            .get(ids.guard, horde_entity)
            .is_some_and(super::guard::Guard::returning)
            || Self::emotion_holds(world, ids, horde_entity)
            || world.contains(ids.ai_busy, horde_entity)
        {
            return GroupDecision::Skip;
        }
        if world.contains(ids.ground_order, horde_entity) {
            return GroupDecision::Ground;
        }
        let Some(horde) = world.get(Self::horde_ids(ids).horde, horde_entity) else {
            return GroupDecision::Skip;
        };
        let max_range = if world.contains(ids.no_attack, horde_entity) {
            None
        } else {
            self.horde_max_range(tick, ids, horde)
        };
        let Some(max_range) = max_range else {
            return GroupDecision::Unarmed;
        };
        let Some(body) = world.get(ids.body, horde_entity).copied() else {
            return GroupDecision::Skip;
        };
        let owner = horde
            .members
            .iter()
            .find_map(|member| world.get(ids.owner, *member).map(|owner| owner.0));
        let guarded = world.contains(ids.guard, horde_entity);
        let scan = owner.filter(|_| !guarded).and_then(|owner| {
            let during_idle = !world.contains(ids.attack_move_order, horde_entity);
            self.idle_scan_of(tick, ids, horde_entity, owner, max_range, during_idle)
        });
        GroupDecision::Engage(GroupFacts {
            max_range,
            body,
            owner,
            guarded,
            scan,
        })
    }

    pub(super) fn advance_attack_move(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        body: Body,
        mut attack_move: AttackMoveOrder,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        if let Some((object, seen, seen_z)) = attack_move.object {
            let goal = if self.is_gone_object(tick.state(), ids, object) {
                None
            } else {
                tick.state()
                    .get(ids.body, object)
                    .map(|body| body.0.position)
            };
            let goal_z = self.object_z(tick.state(), ids, object);
            let Some(goal) = goal else {
                tick.state_mut().remove(ids.attack_move_order, horde_entity);
                tick.state_mut().remove(ids.path, horde_entity);
                Self::clear_path_retry(tick, ids, horde_entity);
                self.end_group_closing(tick, ids, horde_entity);
                return Ok(());
            };
            if regoal_due((goal, goal_z), (seen, seen_z)) {
                attack_move.destination = goal;
                attack_move.short_end = None;
                attack_move.object = Some((object, goal, goal_z));
                tick.state_mut()
                    .insert(ids.attack_move_order, horde_entity, attack_move)?;
                tick.state_mut().remove(ids.path, horde_entity);
            }
        }
        let arrival_radius =
            Self::mover_radius(tick, ids, horde_entity).max(self.passability.geometry.cell_size());
        let dx = body.0.position.x - attack_move.destination.x;
        let dy = body.0.position.y - attack_move.destination.y;
        let distance_squared = dx * dx + dy * dy;
        if distance_squared <= arrival_radius * arrival_radius {
            tick.state_mut().remove(ids.attack_move_order, horde_entity);
            Self::clear_path_retry(tick, ids, horde_entity);
            self.end_group_closing(tick, ids, horde_entity);
            if let Some(facing) = attack_move.facing {
                let mut state = body.0;
                state.facing = facing;
                tick.state_mut()
                    .insert(ids.body, horde_entity, Body(state))?;
            }
            return Ok(());
        }
        let now = tick.frame().get();
        if let Some(until) = attack_move.hold_until {
            if now < until {
                return Ok(());
            }
            attack_move.hold_until = None;
            attack_move.short_end = None;
            tick.state_mut()
                .insert(ids.attack_move_order, horde_entity, attack_move)?;
            tick.state_mut().remove(ids.path, horde_entity);
        }
        let at_short_end = attack_move.short_end.is_some_and(|end| {
            let (ex, ey) = (body.0.position.x - end.x, body.0.position.y - end.y);
            ex * ex + ey * ey <= arrival_radius * arrival_radius
        });
        let failed = tick
            .state()
            .get(ids.path_retry, horde_entity)
            .is_some_and(|retry| retry.failures > 0);
        if at_short_end || failed {
            Self::clear_path_retry(tick, ids, horde_entity);
            if distance_squared <= ATTACK_MOVE_DONE_RADIUS * ATTACK_MOVE_DONE_RADIUS
                || attack_move.retries_left == 0
            {
                tick.state_mut().remove(ids.attack_move_order, horde_entity);
                self.end_group_closing(tick, ids, horde_entity);
            } else {
                attack_move.retries_left = attack_move.retries_left.saturating_sub(1);
                attack_move.hold_until = Some(
                    now.saturating_add(
                        ATTACK_MOVE_RETRY_HOLD_SECONDS
                            .saturating_mul(u64::from(mint_sim::SIMULATION_HZ)),
                    ),
                );
                tick.state_mut()
                    .insert(ids.attack_move_order, horde_entity, attack_move)?;
            }
            return Ok(());
        }
        if tick
            .state()
            .get(ids.path, horde_entity)
            .is_some_and(|path| {
                path.goal != attack_move.destination && Some(path.goal) != attack_move.short_end
            })
        {
            tick.state_mut().remove(ids.path, horde_entity);
        }
        if let Some(closest) = self.walk_toward(
            tick,
            ids,
            horde_entity,
            body.0.position,
            attack_move.destination,
            budgets,
            !attack_move.non_final,
        )? {
            tick.state_mut().insert(
                ids.attack_move_order,
                horde_entity,
                AttackMoveOrder {
                    short_end: Some(closest),
                    ..attack_move
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn advance_loose_attack_moves(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let movers: Vec<Entity> = tick
            .state()
            .store(ids.attack_move_order)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in movers {
            let world = tick.state();
            if world.contains(Self::horde_ids(ids).horde, entity) {
                continue;
            }
            let fighting = world
                .get(ids.armed, entity)
                .is_some_and(|armed| armed.target.is_some())
                || world.contains(ids.attack_order, entity)
                || world.contains(ids.loose_melee, entity);
            let (Some(order), Some(body)) = (
                world.get(ids.attack_move_order, entity).copied(),
                world.get(ids.body, entity).copied(),
            ) else {
                continue;
            };
            if !fighting {
                self.advance_attack_move(tick, ids, entity, body, order, budgets)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn scan_for_horde_target(
        &self,
        world: &World,
        ids: Ids,
        grid: &TargetGrid,
        living: &[(Foe, PlayerId)],
        global_max_radius: f32,
        from: Vec2,
        shooter_radius: f32,
        max_range: f32,
        owner: PlayerId,
        scans_structures: bool,
        scanner: Entity,
        retaliate_only: bool,
    ) -> Option<Entity> {
        if self.scanner_off_map(world, ids, scanner) {
            return None;
        }
        if retaliate_only {
            return self
                .last_damager(world, ids, scanner, living)
                .map(|foe| foe.entity);
        }
        let search_radius = TargetGrid::search_radius(max_range, shooter_radius, global_max_radius);
        let mut candidate_indices: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(from.x),
            grid_cell(from.y),
            grid_radius_cells(search_radius),
            &mut candidate_indices,
        );
        let mut candidates: Vec<Foe> = Vec::new();
        let high = self.active_high(world, ids);
        let relation = self.relation_from(world, scanner);
        for at in candidate_indices {
            #[allow(clippy::cast_possible_truncation)]
            let at = at as usize;
            if let Some((foe, _)) = living
                .get(at)
                .filter(|(foe, _)| !super::targeting::outside(high, foe.position))
            {
                if mint_sage_combat::can_target(
                    mint_sage_combat::Attacker {
                        relation: relation.to(foe.entity).victim(),
                        class_allowed: true,
                        scans_structures,
                    },
                    self.scanned_victim(world, ids, owner, foe),
                    mint_sage_combat::Mode::AutoAcquire,
                ) {
                    candidates.push(*foe);
                }
            }
        }
        self.acquire_for(
            world,
            ids,
            scanner,
            from,
            shooter_radius,
            &candidates,
            0.0,
            max_range,
        )
    }
}
