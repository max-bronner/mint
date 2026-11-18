use super::*;
use mint_movement::{CellFlags, GridExtent, HeightRule};
use mint_sage_passability::{covered_cells, GatedView, OBSTACLE};
use std::collections::BTreeMap;

pub(super) const PATH_NODE_BUDGET: u32 = 20_000;

const ARRIVAL_FACING_SNAP: f32 = core::f32::consts::PI / 10.0;

const TERROR_CONDITION: &str = "EMOTION_TERROR";

const ENTRY_REACHED: f32 = 2.5;

pub(super) const LOOSE_GROUP_OFFSET_CAP_RADII: f32 = 6.0;

pub(super) const PATH_NODE_BUDGET_PER_TICK: u32 = 4_000;

pub(super) const STRAGGLER_RESCUE_BUDGET_PER_TICK: u32 = 4_000;

#[cfg(test)]
thread_local! {
    static LAST_STRAGGLER_RESCUE_SPENT: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn record_straggler_rescue_spent(spent: u32) {
    LAST_STRAGGLER_RESCUE_SPENT.with(|cell| cell.set(spent));
}

#[cfg(not(test))]
pub(super) const fn record_straggler_rescue_spent(_spent: u32) {}

#[cfg(test)]
pub(crate) fn last_straggler_rescue_spent() -> u32 {
    LAST_STRAGGLER_RESCUE_SPENT.with(std::cell::Cell::get)
}

pub(super) const MIN_REPATH_FRAMES: u32 = 36;

pub(super) const PATH_RETRY_DELAY_FRAMES: u32 = 3 * SIMULATION_HZ;

const PATH_QUEUE_CAP: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathRetry {
    pub(crate) next_attempt: Frame,
    pub(crate) failures: u32,
}

impl Component for PathRetry {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.next_attempt.get());
        hasher.write_u32(self.failures);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathTicket {
    pub(crate) entity: Entity,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PathQueue {
    waiting: VecDeque<PathTicket>,
}

impl PathQueue {
    fn contains(&self, entity: Entity) -> bool {
        self.waiting.iter().any(|ticket| ticket.entity == entity)
    }

    fn push(&mut self, entity: Entity) {
        if self.waiting.len() < PATH_QUEUE_CAP && !self.contains(entity) {
            self.waiting.push_back(PathTicket { entity });
        }
    }

    fn pop_front(&mut self) -> Option<Entity> {
        self.waiting.pop_front().map(|ticket| ticket.entity)
    }

    fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.waiting.iter().map(|ticket| ticket.entity)
    }
}

impl Component for PathQueue {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.waiting.len());
        for ticket in &self.waiting {
            ticket.entity.hash_into(hasher);
        }
    }
}

pub(crate) struct PathBudgets {
    head: Option<(Entity, NodeBudget)>,
    shared: NodeBudget,
    deferred: Vec<Entity>,
    none: NodeBudget,
    attempted: Vec<Entity>,
    starved: Vec<Entity>,
}

impl PathBudgets {
    pub(super) fn new(head: Option<Entity>, deferred: Vec<Entity>) -> Self {
        Self {
            head: head.map(|entity| (entity, NodeBudget::new(PATH_NODE_BUDGET))),
            shared: NodeBudget::new(PATH_NODE_BUDGET_PER_TICK),
            deferred,
            none: NodeBudget::new(0),
            attempted: Vec::new(),
            starved: Vec::new(),
        }
    }

    pub(super) fn is_head(&self, entity: Entity) -> bool {
        self.head.as_ref().is_some_and(|(head, _)| *head == entity)
    }

    pub(super) fn budget_for(&mut self, entity: Entity) -> &mut NodeBudget {
        match &mut self.head {
            Some((head, budget)) if *head == entity => {
                if self.attempted.last() != Some(&entity) {
                    self.attempted.push(entity);
                }
                budget
            }
            _ => {
                if self
                    .deferred
                    .binary_search_by_key(&entity.index(), |waiting| waiting.index())
                    .is_ok()
                {
                    return &mut self.none;
                }
                if self.shared.remaining() == 0 {
                    if self.starved.last() != Some(&entity) {
                        self.starved.push(entity);
                    }
                    return &mut self.shared;
                }
                if self.attempted.last() != Some(&entity) {
                    self.attempted.push(entity);
                }
                &mut self.shared
            }
        }
    }
}

pub(super) fn goal_moved(from: Vec2, planned: Vec2, goal: Vec2) -> bool {
    let moved = goal.minus(planned);
    let to_goal = goal.minus(from);
    moved.dot(moved) > to_goal.dot(to_goal) * 0.01
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplannedAt(pub(crate) Frame);

impl Component for ReplannedAt {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IgnoredObstacle(pub(crate) Entity);

impl Component for IgnoredObstacle {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

const DIVE_DOWN_TOLERANCE: f32 = 0.5;

const HORDE_RAMP_DESCENT_STEP: f32 = 9.0;

const HORDE_RAMP_DESCENT_WINDOW: f32 = 20.0;

fn ramp_descent_step(current_z: f32, target_z: f32) -> f32 {
    if current_z - target_z < HORDE_RAMP_DESCENT_WINDOW {
        (current_z - HORDE_RAMP_DESCENT_STEP).max(target_z)
    } else {
        current_z
    }
}

#[cfg(test)]
mod ramp_descent_step_tests {
    use super::{ramp_descent_step, HORDE_RAMP_DESCENT_WINDOW};

    #[test]
    fn a_member_within_the_window_eases_down_at_most_nine_per_frame() {
        let target = 100.0;
        let first = ramp_descent_step(target + 15.0, target);
        assert_eq!(first, target + 6.0, "drops by 9, from 15 above to 6 above");
        let second = ramp_descent_step(first, target);
        assert_eq!(
            second, target,
            "the remaining 6 clamps to the target, not below it"
        );
    }

    #[test]
    fn a_member_outside_the_window_holds() {
        let target = 100.0;
        let far = target + HORDE_RAMP_DESCENT_WINDOW;
        assert_eq!(ramp_descent_step(far, target), far);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FlightLevel(pub(crate) f32);

impl Component for FlightLevel {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Held(pub(crate) Option<Frame>);

impl Component for Held {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.0.is_some());
        hasher.write_u64(self.0.map_or(0, Frame::get));
    }
}

impl RotwkRules {
    pub(super) fn hold_until(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        until: Frame,
    ) -> Result<()> {
        let next = match tick.state().get(ids.held, entity).copied() {
            Some(Held(None)) => return Ok(()),
            Some(Held(Some(existing))) => Held(Some(existing.max(until))),
            None => Held(Some(until)),
        };
        tick.state_mut().insert(ids.held, entity, next)?;
        Ok(())
    }

    pub(super) fn expire_timed_holds(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let due: Vec<Entity> = tick
            .state()
            .store(ids.held)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, held)| held.0.is_some_and(|until| until <= now))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in due {
            tick.state_mut().remove(ids.held, entity);
        }
        Ok(())
    }
}

struct ObstacleExemptView<'a> {
    inner: GatedView<'a>,
    excluded: &'a [Cell],
    tunnelling: bool,
}

impl mint_movement::Passability for ObstacleExemptView<'_> {
    fn extent(&self) -> GridExtent {
        self.inner.extent()
    }

    fn flags(&self, cell: Cell) -> CellFlags {
        let flags = self.inner.flags(cell);
        if self.tunnelling || self.excluded.contains(&cell) {
            CellFlags::from_bits(flags.bits() & !OBSTACLE.bits())
        } else {
            flags
        }
    }

    fn extra_cost(&self, cell: Cell) -> u32 {
        self.inner.extra_cost(cell)
    }

    fn admits(&self, cell: Cell, asker: mint_movement::Asker) -> bool {
        self.inner.admits(cell, asker)
    }

    fn portal_at(&self, cell: Cell) -> Option<mint_movement::Portal> {
        self.inner.portal_at(cell)
    }
}

pub(super) const REPLAN_COOLDOWN_FRAMES: u64 = 30;

pub(super) const BOARDING_REPLAN_FRAMES: u64 =
    10 * mint_sim::SIMULATION_HZ as u64 / crate::game_data::RETAIL_LOGIC_HZ as u64;

pub(super) const BOARDING_REPLAN_DISTANCE: f32 = 200.0;

const CLIMBING: &str = "CLIMBING";

const RAPPELLING: &str = "RAPPELLING";

const CLIMB_PROBE_CAP: f32 = 7.0;

const CLIMB_PROBE_CAP_PER_TICK: f32 =
    CLIMB_PROBE_CAP * crate::game_data::RETAIL_LOGIC_HZ as f32 / SIMULATION_HZ as f32;

const CLIMB_RATE_SHARE: f32 = 0.6;

const CLIMB_LANDING_TOLERANCE: f32 = 1.0;

const SCALING_WALL_HORDE: &str = "SCALING_WALL_HORDE";

pub(super) const PORCUPINE: &str = "PORCUPINE";

#[allow(clippy::cast_precision_loss)]
const DIVE_EASE_TICKS: f32 = SIMULATION_HZ as f32 / 2.0;

impl RotwkRules {
    pub(super) fn is_immobile(tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        tick.state()
            .get(ids.shape, entity)
            .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::IMMOBILE))
    }

    pub(super) fn move_refused(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<&'static str> {
        if Self::is_immobile(tick, ids, entity) {
            return Some("KindOf IMMOBILE");
        }
        if tick
            .state()
            .get(ids.knocked, entity)
            .is_some_and(|knocked| knocked.off_its_feet())
        {
            return Some("thrown, and not back on its feet");
        }
        if self
            .modules
            .as_ref()
            .is_some_and(|wiring| tick.state().contains(wiring.ability_hold, entity))
        {
            return Some(
                "its ability holds it (FreezeAfterTriggerDuration, BusyForDuration, toggle)",
            );
        }
        if tick.state().contains(ids.held, entity) {
            return Some("DISABLED_HELD (spawn-point exit)");
        }
        if self.script_status_bit(
            tick.state(),
            ids,
            entity,
            map_scripts::ScriptStatusBit::Disabled,
        ) {
            return Some("DISABLED_SCRIPT_DISABLED (Enabled panel flag off)");
        }
        if self.boarding(tick, ids, entity) {
            return Some("DISABLED_TEMPORARILY_BUSY (a horde is boarding)");
        }
        (self.locomotor_limits(tick, ids, entity).max_speed <= 0.0)
            .then_some("LocomotorSet Speed = 0")
    }

    fn boarding(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        tick.state().store(ids.entering).is_some_and(|store| {
            store.iter().any(|(subject, entering)| {
                entering.container == entity
                    && (tick.state().contains(ids.horde, subject)
                        || tick.state().contains(ids.member, subject))
            })
        })
    }

    fn commandable(tick: &Tick<'_>, ids: Ids, entity: Entity, player: PlayerId) -> bool {
        tick.state()
            .get(ids.owner, entity)
            .is_some_and(|owner| owner.0 == player)
            || tick.state().contains(ids.dummy, entity)
    }

    pub(super) fn move_to(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
    ) -> Result<()> {
        self.move_to_inner(tick, ids, player, goal, false, None, false)
    }

    pub(super) fn move_to_queued(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
        queue: bool,
    ) -> Result<()> {
        self.move_to_inner(tick, ids, player, goal, false, None, queue)
    }

    pub(super) fn move_to_exact_goal(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
    ) -> Result<()> {
        self.move_to_inner(tick, ids, player, goal, true, None, false)
    }

    pub(super) fn move_to_with_drag_formation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        anchor: Vec2,
        order: super::group_move::FormationOrder,
    ) -> Result<()> {
        self.move_to_inner(tick, ids, player, anchor, false, Some(order), false)
    }

    pub(super) fn move_order_targets_and_movers(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        force_hordes: bool,
    ) -> (Vec<Entity>, Vec<Entity>) {
        let modes = self.modes(tick, ids);
        let mut targets = self.selection(tick, ids, player);
        if modes.formation == FormationMode::On || force_hordes {
            let owned: Vec<Entity> = targets
                .iter()
                .copied()
                .filter(|entity| Self::commandable(tick, ids, *entity, player))
                .collect();
            let hordes = crate::horde::hordes_of(tick.state(), Self::horde_ids(ids), &owned);
            targets.retain(|entity| !tick.state().contains(ids.member, *entity));
            let mut ordered = hordes;
            ordered.append(&mut targets);
            targets = ordered;
        }

        let movers: Vec<Entity> = targets
            .iter()
            .copied()
            .filter(|entity| {
                (tick.state().contains(ids.horde, *entity)
                    || Self::commandable(tick, ids, *entity, player))
                    && self.move_refused(tick, ids, *entity).is_none()
                    && tick.state().contains(ids.body, *entity)
                    && !Self::emotion_refuses_order(tick.state(), ids, *entity)
                    && !self.siege_refuses_order(tick.state(), ids, *entity)
            })
            .collect();
        (targets, movers)
    }

    fn move_to_inner(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        goal: Vec2,
        exact_goal: bool,
        drag: Option<super::group_move::FormationOrder>,
        queue: bool,
    ) -> Result<()> {
        let modes = self.modes(tick, ids);
        let mut pipeline = Pipeline::new(modes.config());
        let geometry = self.passability.geometry;
        let on_grid = self.passability.cell_at(goal).is_some();

        let is_drag = drag.is_some();
        let (targets, movers) = self.move_order_targets_and_movers(tick, ids, player, is_drag);
        let off_map_movers: Vec<Entity> = movers
            .iter()
            .copied()
            .filter(|&mover| {
                tick.state()
                    .get(ids.body, mover)
                    .is_some_and(|body| self.both_off_extent(body.0.position, goal))
            })
            .collect();
        if !on_grid && off_map_movers.is_empty() {
            return Ok(());
        }
        self.cohere_move_speed(tick, ids, &movers)?;
        let formation = if let Some(order) = drag {
            Some(order)
        } else if !exact_goal
            && on_grid
            && modes.formation == FormationMode::On
            && !self.group_move_tightens(tick, ids, &movers, goal)
        {
            self.formation_order(tick, ids, &movers, goal, &mut pipeline)?
        } else {
            None
        };
        let destinations = if exact_goal || !on_grid {
            BTreeMap::new()
        } else {
            let wanted = formation.as_ref().map_or_else(
                || Self::group_destinations(tick, ids, &movers, goal, true),
                super::group_move::FormationOrder::destinations,
            );
            self.reserve_destinations(
                tick,
                ids,
                &movers,
                &wanted,
                goal,
                modes.formation == FormationMode::On || formation.is_some(),
                formation.as_ref().map(|order| order.facing),
            )
        };

        let mut routed: Vec<(Entity, Vec<Vec2>)> = Vec::new();
        for entity in targets {
            let owned = Self::commandable(tick, ids, entity, player)
                || tick.state().contains(ids.horde, entity);
            if !owned {
                continue;
            }
            if let Some(rule) = self.move_refused(tick, ids, entity) {
                diag!("order: move refused for {entity:?} by {rule}");
                continue;
            }
            if Self::emotion_refuses_order(tick.state(), ids, entity)
                || self.siege_refuses_order(tick.state(), ids, entity)
            {
                diag!("order: move refused for {entity:?} by its emotion's or siege power's lock");
                continue;
            }
            if !on_grid && !off_map_movers.contains(&entity) {
                continue;
            }
            let entity_goal = destinations.get(&entity).map_or(goal, |(at, _)| *at);
            if queue && self.order_queue_busy(tick.state(), ids, entity) {
                Self::append_queued_leg(
                    tick,
                    ids,
                    entity,
                    crate::combat::QueuedLeg {
                        point: entity_goal,
                        attack: false,
                    },
                )?;
                continue;
            }
            if !self.take_player_order(tick, ids, entity, false, !queue)? {
                continue;
            }
            if queue {
                tick.state_mut().insert(
                    ids.order_queue,
                    entity,
                    crate::combat::OrderQueue {
                        current: Some(crate::combat::QueuedLeg {
                            point: entity_goal,
                            attack: false,
                        }),
                        queued: Vec::new(),
                        idle_checks: 0,
                    },
                )?;
            }
            Self::clear_path_retry(tick, ids, entity);
            if is_drag {
                tick.state_mut().insert(
                    ids.in_formation_template,
                    entity,
                    super::drag_formation::InFormationTemplate,
                )?;
            } else {
                tick.state_mut().remove(ids.in_formation_template, entity);
            }
            self.cancel_build_order(tick, ids, entity)?;
            if let Some(horde) = tick.state().get(ids.horde, entity).cloned() {
                for member in &horde.members {
                    tick.state_mut().remove(ids.path, *member);
                    if let Some(entry) = tick.state().get(ids.member, *member).copied() {
                        Self::park(tick, ids, *member, entry)?;
                    }
                }
            }
            let Some(body) = tick.state().get(ids.body, entity).copied() else {
                continue;
            };
            if self.is_giant_bird(tick.state(), ids, entity) {
                let target_point = destinations.get(&entity).map_or(goal, |(at, _)| *at);
                let target_point = if self.passability.cell_at(target_point).is_some() {
                    target_point
                } else {
                    goal
                };
                crate::unit::give_path(
                    tick.state_mut(),
                    (ids.body, ids.path),
                    entity,
                    Path {
                        route: Route::to_point(target_point),
                        goal: target_point,
                        kind: PathKind::Ordered,
                        facing: None,
                    },
                )?;
                if !exact_goal {
                    self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::Move, None);
                }
                continue;
            }
            if self.give_off_map_route(
                tick,
                ids,
                entity,
                (body.0.position, entity_goal),
                PathKind::Ordered,
            )? {
                if !exact_goal {
                    self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::Move, None);
                }
                continue;
            }
            let mover = self.mover_class(tick, ids, entity);
            if self.passable_cell_for(tick, ids, goal, mover).is_none()
                && self.is_amphibious_ship(tick, ids, entity)
            {
                self.order_amphibious_ship(tick, ids, entity, body.0.position, goal, mover)?;
                if !exact_goal {
                    self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::Move, None);
                }
                continue;
            }
            let Some(start) = self.start_cell_toward(tick, ids, body.0.position, goal, mover)
            else {
                continue;
            };
            let Some(goal_cell) = self.order_goal_cell(tick, ids, goal, mover) else {
                continue;
            };
            let (entity_goal, _) = destinations
                .get(&entity)
                .copied()
                .map_or((goal, None), |(at, facing)| (at, Some(facing)));
            let (target_cell, target_point) = self
                .order_goal_cell(tick, ids, entity_goal, mover)
                .map_or((goal_cell, goal), |cell| (cell, entity_goal));
            let via = formation
                .as_ref()
                .map_or(&[][..], |order| order.via(entity));
            let (start, origin, prefix) = self.plan_via(
                tick,
                ids,
                &mut pipeline,
                entity,
                mover,
                (start, body.0.position),
                via,
            )?;
            let request = PathRequest {
                start,
                goal: target_cell,
                mover,
                goal_id: GoalId(u64::from(entity.index())),
                origin: Some(origin),
                corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
                departure: self.departure(tick, ids, entity),
            };
            let mut budget = NodeBudget::new(PATH_NODE_BUDGET);
            let ground = self.dynamic_view(tick, ids);
            let costs = self
                .unit_cost_view(tick, ids, entity, &ground)
                .exempting(&[target_cell]);
            let view = self.footprint_view(tick, ids, entity, mover, &costs, &ground);
            let outcome = pipeline
                .plan_closest(&view, geometry, &request, &mut budget)
                .map_err(|e| mint_sim::Error::Rules {
                    frame: tick.frame().get(),
                    reason: format!("planning a path for entity {}: {e}", entity.index()),
                })?;
            let planned = match outcome {
                PlanOutcome::Ready(plan) => Some((plan, true)),
                PlanOutcome::Closest(plan) => Some((plan, false)),
                _ => None,
            };
            match planned {
                Some((plan, exact)) => {
                    let mut state = body.0;
                    state.waypoint = 0;
                    tick.state_mut().insert(ids.body, entity, Body(state))?;
                    let mut route = plan.route;
                    if exact && self.passability.cell_at(target_point) == Some(target_cell) {
                        if let Some(last) = route.points.last_mut() {
                            *last = target_point;
                        }
                    }
                    if !prefix.is_empty() {
                        let mut points = prefix;
                        points.extend(route.points.iter().skip(1).copied());
                        route.points = points;
                    }
                    let goal = if exact {
                        target_point
                    } else {
                        match route.points.last().copied() {
                            Some(point) => point,
                            None => continue,
                        }
                    };
                    routed.push((entity, route.points.clone()));
                    tick.state_mut().insert(
                        ids.path,
                        entity,
                        Path {
                            route,
                            goal,
                            kind: PathKind::Ordered,
                            facing: formation.as_ref().map(|order| order.facing),
                        },
                    )?;
                    if !exact_goal {
                        self.emit_voice(tick, ids, entity, crate::voice::VoiceKind::Move, None);
                    }
                }
                None => {
                    tick.state_mut().remove(ids.path, entity);
                    let mut state = body.0;
                    state.speed = 0.0;
                    tick.state_mut().insert(ids.body, entity, Body(state))?;
                }
            }
        }
        for (entity, route) in routed {
            self.move_allies(tick, ids, &mut pipeline, entity, &route)?;
        }
        Ok(())
    }

    pub(super) fn run_burning(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let entities: Vec<(Entity, Vec2)> = world
            .store(ids.burning)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        !world.contains(ids.doomed, *entity) && !world.contains(ids.path, *entity)
                    })
                    .map(|(entity, burning)| (entity, burning.origin))
                    .collect()
            })
            .unwrap_or_default();
        if entities.is_empty() {
            return Ok(());
        }
        let mut pipeline = Pipeline::new(self.modes(tick, ids).config());
        let geometry = self.passability.geometry;
        for (entity, origin) in entities {
            let Some(run) = tick
                .state()
                .get(ids.kind, entity)
                .copied()
                .and_then(|kind| self.kinds.get(kind))
                .and_then(|entry| entry.burning_run)
            else {
                continue;
            };
            let Some(body) = tick.state().get(ids.body, entity).copied() else {
                continue;
            };
            let bearing = tick.rng().next_f32() * core::f32::consts::TAU;
            let distance = if run.is_cavalry {
                run.radius
            } else {
                tick.rng().next_f32() * run.radius
            };
            let (sin, cos) = mint_math::f32::sin_cos(bearing);
            let goal = origin.plus(Vec2::new(cos * distance, sin * distance));
            let mover = self.mover_class(tick, ids, entity);
            let (Some(start), Some(goal_cell)) = (
                self.passable_cell_for(tick, ids, body.0.position, mover),
                self.passable_cell_for(tick, ids, goal, mover),
            ) else {
                continue;
            };
            let request = PathRequest {
                start,
                goal: goal_cell,
                mover,
                goal_id: GoalId(u64::from(entity.index())),
                origin: Some(body.0.position),
                corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
                departure: self.departure(tick, ids, entity),
            };
            let mut budget = NodeBudget::new(PATH_NODE_BUDGET);
            let ground = self.dynamic_view(tick, ids);
            let view = self.footprint_view(tick, ids, entity, mover, &ground, &ground);
            let outcome = pipeline
                .plan_closest(&view, geometry, &request, &mut budget)
                .map_err(|e| mint_sim::Error::Rules {
                    frame: tick.frame().get(),
                    reason: format!("planning a burning run for entity {}: {e}", entity.index()),
                })?;
            let route = match outcome {
                PlanOutcome::Ready(plan) | PlanOutcome::Closest(plan) => plan.route,
                _ => continue,
            };
            diag!(
                "death: entity {} burning run to ({:.0},{:.0}), {:.0} from ({:.0},{:.0}) of \
                 {:.0} radius{}",
                entity.index(),
                goal.x,
                goal.y,
                distance,
                origin.x,
                origin.y,
                run.radius,
                if run.is_cavalry { ", cavalry" } else { "" }
            );
            crate::unit::give_path(
                tick.state_mut(),
                (ids.body, ids.path),
                entity,
                Path {
                    route,
                    goal,
                    kind: PathKind::Burning,
                    facing: None,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn report_crush_census(&self) {
        let (mut armed, mut unarmed, mut revenge, mut resisting, mut unresolved) =
            (0usize, 0usize, 0usize, 0usize, 0usize);
        let mut resolves = |name: &str| {
            if self.combat.weapons.id(name).is_none() {
                unresolved = unresolved.saturating_add(1);
            }
        };
        for entry in (0..self.kinds.len())
            .filter_map(|index| u16::try_from(index).ok())
            .filter_map(|index| self.kinds.get(crate::unit::Kind(index)))
        {
            let crush = &entry.record.crush;
            if crush.crusher_level > 0 {
                match &crush.weapon {
                    Some(name) => {
                        armed = armed.saturating_add(1);
                        resolves(name);
                    }
                    None => unarmed = unarmed.saturating_add(1),
                }
            }
            if let Some(name) = &crush.revenge_weapon {
                revenge = revenge.saturating_add(1);
                resolves(name);
            }
            if crush.resisted.is_some() {
                resisting = resisting.saturating_add(1);
            }
        }
        println!(
            "crush: {armed} crusher kind(s) in this match fire a CrushWeapon, {unarmed} kill \
             outright for want of one, {revenge} kind(s) fire a CrushRevengeWeapon, {resisting} \
             declare CrusherLevelResisted (inert, Q790), {unresolved} crush weapon name(s) no \
             Weapon declares"
        );
    }

    pub(super) fn crush_states(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Vec<CrushState> {
        living
            .iter()
            .map(|(foe, _)| self.crush_state(tick, ids, foe.entity, foe.radius))
            .collect()
    }

    pub(super) fn crush_state(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        radius: f32,
    ) -> CrushState {
        let body = tick.state().get(ids.body, entity).map(|body| body.0);
        let velocity = body.map_or(Vec2::ZERO, |body| body.velocity());
        let facing = body.map_or(mint_movement::Facing::EAST, |body| body.facing);
        let volume = self
            .volume_of(tick, ids, entity)
            .unwrap_or(crate::combat::Volume::Circle(radius));
        let uncollided = self
            .flight_profile(tick, ids, entity)
            .is_some_and(|flight| flight.uncollided);
        let Some(crush) = self
            .record_of(tick, ids, entity)
            .map(|record| &record.crush)
            .filter(|_| !uncollided)
        else {
            return CrushState {
                crusher_level: 0,
                crushable_level: i16::from(mint_sage_modules::UNCRUSHABLE),
                crushes_allies: false,
                velocity,
                facing,
                volume,
                too_slow: false,
            };
        };
        let mounted = (crush.mounted_crusher_level.is_some()
            || crush.mounted_crushable_level.is_some())
            && self.is_mounted(tick, entity);
        let level = crush
            .mounted_crusher_level
            .filter(|_| mounted)
            .unwrap_or(crush.crusher_level);
        let crushable = crush
            .mounted_crushable_level
            .filter(|_| mounted)
            .unwrap_or(crush.crushable_level);
        let bonus = tick
            .state()
            .get(ids.bonuses, entity)
            .map_or(0, |bonuses| bonuses.0.crushable_level_bonus());
        let (gate, share) = self
            .crushing_horde(tick, ids, entity)
            .and_then(|horde| Some((horde, self.record_of(tick, ids, horde)?.crush.min_velocity)))
            .unwrap_or((entity, crush.min_velocity));
        let speed = tick
            .state()
            .get(ids.body, gate)
            .map_or(0.0, |body| body.0.speed);
        let charging = self.holds_condition(tick, entity, "CHARGING");
        let armed = level > 0 && (charging || !crush.only_while_charging);
        CrushState {
            crusher_level: if armed { level } else { 0 },
            crushable_level: if tick.state().contains(ids.knocked, entity) {
                i16::from(mint_sage_modules::UNCRUSHABLE)
            } else {
                i16::from(crushable).saturating_add(bonus)
            },
            crushes_allies: crush.allies || charging || self.is_rampaging(tick.state(), entity),
            velocity,
            facing,
            volume,
            too_slow: armed && !self.fast_enough_to_crush(tick, ids, gate, share, speed),
        }
    }

    pub(super) fn is_mounted(&self, tick: &Tick<'_>, entity: Entity) -> bool {
        self.holds_condition(tick, entity, "MOUNTED")
    }

    pub(super) fn holds_condition(&self, tick: &Tick<'_>, entity: Entity, name: &str) -> bool {
        self.holds_condition_in(tick.state(), entity, name)
    }

    pub(super) fn holds_condition_in(&self, world: &World, entity: Entity, name: &str) -> bool {
        self.modules.as_ref().is_some_and(|wiring| {
            world.get(wiring.state, entity).is_some_and(|state| {
                state
                    .model_conditions
                    .iter()
                    .any(|held| held.eq_ignore_ascii_case(name))
            })
        })
    }

    fn fast_enough_to_crush(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        share: f32,
        speed: f32,
    ) -> bool {
        if !share.is_finite() || share <= 0.0 {
            return true;
        }
        speed >= share * self.mover_limits(tick, ids, entity).max_speed
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_crush_pass(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
        grid: &TargetGrid,
    ) -> Result<()> {
        let crush = self.crush_states(tick, ids, living);
        let allied = |a: Entity, b: Entity| self.object_allied(tick.state(), a, b);
        let (refused, hits): (Vec<_>, Vec<_>) = run_crush(living, &crush, grid, &allied)
            .into_iter()
            .partition(|&(at, _)| crush.get(at as usize).is_some_and(|state| state.too_slow));
        for &(at, blocked_at) in &refused {
            let (Some((crusher, _)), Some((blocker, _))) = (
                living.get(at as usize).copied(),
                living.get(blocked_at as usize).copied(),
            ) else {
                continue;
            };
            self.hold_crush_cap(tick, ids, crusher.entity)?;
            if !self.object_hostile(tick.state(), crusher.entity, blocker.entity) {
                continue;
            }
            self.melee_attack_adjacent_victim(tick, ids, crusher.entity, blocker.entity)?;
            let blocker_is_mine = tick
                .state()
                .get(ids.shape, blocker.entity)
                .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::MINE));
            if !blocker_is_mine {
                self.melee_attack_adjacent_victim(tick, ids, blocker.entity, crusher.entity)?;
            }
        }
        let crush: Vec<CrushState> = crush
            .into_iter()
            .map(|state| CrushState {
                crusher_level: if state.too_slow {
                    0
                } else {
                    state.crusher_level
                },
                ..state
            })
            .collect();
        let fresh = self.begin_crush_contacts(tick, ids, living, &crush, &hits)?;
        let mut kills = Vec::new();
        for &(crusher_at, crushed_at) in &fresh {
            let (Some((crusher, _)), Some((crushed, _)), Some(_state)) = (
                living.get(crusher_at as usize),
                living.get(crushed_at as usize),
                crush.get(crusher_at as usize),
            ) else {
                continue;
            };
            let (Some(own), Some(theirs)) = (
                self.record_of(tick, ids, crusher.entity)
                    .map(|record| record.crush.clone()),
                self.record_of(tick, ids, crushed.entity)
                    .map(|record| record.crush.clone()),
            ) else {
                continue;
            };
            match &own.weapon {
                Some(name) => {
                    self.fire_crush_weapon(tick, ids, name, crusher, crushed, events, living)?
                }
                None => kills.push((crusher_at, crushed_at)),
            }
            if let Some(name) = theirs.revenge_weapon {
                if !self.flanks(tick, ids, crusher.entity, crushed.entity)? {
                    self.fire_crush_weapon(tick, ids, &name, crushed, crusher, events, living)?;
                }
            }
            let driver = self
                .crushing_horde(tick, ids, crusher.entity)
                .unwrap_or(crusher.entity);
            let fields = if driver == crusher.entity {
                Some(own)
            } else {
                self.record_of(tick, ids, driver)
                    .map(|record| record.crush.clone())
            };
            let (share, knockback, z_factor) = fields.map_or((0.0, 0.0, 1.0), |fields| {
                (fields.deceleration, fields.knockback, fields.z_factor)
            });
            self.cap_crusher(tick, ids, driver, crusher.entity, crushed.entity, share)?;
            if knockback > 0.0 {
                let away = self.crush_throw_direction(tick, ids, driver, crusher, crushed);
                let thrown =
                    self.crush_throw(tick, ids, away, crushed.entity, knockback, z_factor)?;
                diag!(
                    "crush: frame {} entity {} at ({:.0},{:.0}) knocked back by entity {} at {:.0}/s: {thrown}",
                    tick.frame().get(),
                    crushed.entity.index(),
                    crushed.position.x,
                    crushed.position.y,
                    crusher.entity.index(),
                    knockback
                );
            }
            if let Some((power, z_mult)) = self
                .record_of(tick, ids, crusher.entity)
                .map(|record| (record.crush.ram_power, record.crush.ram_z_mult))
                .filter(|(power, _)| *power > 0.0)
            {
                self.ram_throw(tick, ids, crusher.position, crushed.entity, power, z_mult)?;
            }
        }
        if !kills.is_empty() {
            if let Some(crush_damage_type) = self.combat.damage_types.lookup("CRUSH") {
                diag!(
                    "crush: frame {} {} crush(es) by a crusher with no CrushWeapon kill outright",
                    tick.frame().get(),
                    kills.len()
                );
                events.extend(crush_damage_events(living, &kills, crush_damage_type));
            }
        }
        Ok(())
    }

    pub(super) fn run_topple_pass(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(forest_entity) = crate::forest::singleton_of(tick.state(), ids.forest) else {
            return Ok(());
        };
        let mut fellers: Vec<(Vec2, f32, crate::forest::MoverShape)> = Vec::new();
        for (foe, _) in living {
            let moving = tick
                .state()
                .get(ids.body, foe.entity)
                .is_some_and(|body| body.0.velocity().length_squared() > f32::EPSILON);
            if !moving {
                continue;
            }
            let Some(record) = self.record_of(tick, ids, foe.entity) else {
                continue;
            };
            if record.has_kind("IMMOBILE") || record.has_kind("ARMY_OF_DEAD") {
                continue;
            }
            let machine = record.has_kind("MACHINE");
            let eligible_kind = ["INFANTRY", "CAVALRY", "MACHINE", "MONSTER"]
                .iter()
                .any(|kind| record.has_kind(kind));
            if !eligible_kind {
                continue;
            }
            if !machine
                && self
                    .crush_state(tick, ids, foe.entity, foe.radius)
                    .crusher_level
                    < 2
            {
                continue;
            }
            let shape = match (
                record.geometry.shape.as_deref(),
                record.geometry.major_radius,
                record.geometry.minor_radius,
            ) {
                (Some(shape), Some(major), Some(minor)) if shape.eq_ignore_ascii_case("BOX") => {
                    let facing = tick
                        .state()
                        .get(ids.body, foe.entity)
                        .map_or(Facing::EAST, |body| body.0.facing);
                    crate::forest::MoverShape::Box {
                        major,
                        minor,
                        facing,
                    }
                }
                _ => crate::forest::MoverShape::Disc,
            };
            fellers.push((foe.position, foe.radius, shape));
        }
        if fellers.is_empty() {
            return Ok(());
        }
        let Some(forest) = tick.state_mut().get_mut(ids.forest, forest_entity) else {
            return Ok(());
        };
        let mut felled = 0usize;
        for (pos, radius, shape) in fellers {
            felled = felled.saturating_add(forest.topple_near(pos, radius, shape).len());
        }
        if felled > 0 {
            diag!("forest: {felled} tree(s) toppled this tick");
        }
        Ok(())
    }

    fn crushing_horde(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Entity> {
        let horde = tick.state().get(ids.member, entity)?.horde;
        tick.state().contains(ids.horde, horde).then_some(horde)
    }

    fn crush_cap_ticks() -> u64 {
        6 * u64::from(mint_sim::SIMULATION_HZ) / u64::from(crate::game_data::RETAIL_LOGIC_HZ)
    }

    fn crush_cap(&self, tick: &Tick<'_>, ids: Ids, horde: Entity) -> Option<f32> {
        tick.state()
            .get(ids.crush_speed_cap, horde)
            .filter(|cap| tick.frame().get() < cap.until)
            .map(|cap| cap.cap)
    }

    fn crush_capped(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        mut limits: MotionLimits,
    ) -> MotionLimits {
        if let Some(cap) = self.crush_cap(tick, ids, entity) {
            limits.max_speed = limits.max_speed.min(cap);
        }
        limits
    }

    fn has_melee_weapon(&self, tick: &Tick<'_>, ids: Ids, attacker: Entity) -> bool {
        let is_melee = |weapon: mint_sage_combat::WeaponId| {
            self.combat
                .weapons
                .get(weapon)
                .is_some_and(|template| template.melee)
        };
        if let Some(armed) = tick.state().get(ids.armed, attacker) {
            return is_melee(armed.weapon);
        }
        tick.state().get(ids.horde, attacker).is_some_and(|horde| {
            horde.members.iter().any(|member| {
                tick.state()
                    .get(ids.armed, *member)
                    .is_some_and(|armed| is_melee(armed.weapon))
            })
        })
    }

    fn can_crush_instead(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> bool {
        let radius = |entity: Entity| {
            tick.state()
                .get(ids.shape, entity)
                .map_or(0.0, |shape| shape.radius)
        };
        let crusher = self.crush_state(tick, ids, attacker, radius(attacker));
        let crushed = self.crush_state(tick, ids, victim, radius(victim));
        i16::from(crusher.crusher_level) > crushed.crushable_level && !crusher.too_slow
    }

    pub(super) fn melee_attacking(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        entity: Entity,
    ) -> bool {
        world.contains(ids.member_attack, entity)
            || world
                .get(ids.loose_melee, entity)
                .is_some_and(|plan| plan.melee_attacking(!world.contains(ids.path, entity)))
    }

    fn horde_of(tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Entity> {
        tick.state()
            .get(ids.member, entity)
            .map(|member| member.horde)
            .or_else(|| tick.state().contains(ids.horde, entity).then_some(entity))
    }

    pub(super) fn melee_attack_adjacent_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Result<()> {
        if Self::emotion_locked(tick.state(), ids, attacker) {
            return Ok(());
        }
        if !self.has_melee_weapon(tick, ids, attacker) {
            return Ok(());
        }
        if self.can_crush_instead(tick, ids, attacker, victim) {
            return Ok(());
        }
        if tick.state().contains(ids.member, attacker)
            && !self.regular_state_idle(tick.state(), ids, attacker)
        {
            tick.state_mut().remove(ids.path, attacker);
            tick.state_mut().remove(ids.attack_order, attacker);
            tick.state_mut().remove(ids.member_attack, attacker);
            tick.state_mut().remove(ids.loose_melee, attacker);
        }
        let commanded = Self::commanded_unit(tick.state(), ids, attacker);
        let idle = self.regular_state_idle(tick.state(), ids, commanded);
        let attacking = self.charge_state(tick, ids, commanded).attacking
            || self.charge_state(tick, ids, attacker).attacking;
        if !(idle || attacking) || self.melee_attacking(tick.state(), ids, attacker) {
            return Ok(());
        }
        let attacker_player = tick.state().get(ids.owner, attacker).map(|owner| owner.0);
        if attacker_player.is_some_and(|player| !self.sees(tick.state(), ids, player, victim)) {
            return Ok(());
        }
        if self.is_hidden(tick.state(), ids, attacker) {
            return Ok(());
        }
        if let Some(order) = tick.state().get(ids.attack_order, commanded).copied() {
            if order.target == victim {
                return Ok(());
            }
            let same_horde = Self::horde_of(tick, ids, victim).is_some()
                && Self::horde_of(tick, ids, victim) == Self::horde_of(tick, ids, order.target);
            let is_horde_member = tick.state().contains(ids.member, attacker);
            let victim_is_structure = tick
                .state()
                .get(ids.shape, victim)
                .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE));
            if !(same_horde || (is_horde_member && !victim_is_structure)) {
                return Ok(());
            }
        }
        tick.state_mut().insert(
            ids.attack_order,
            commanded,
            AttackOrder {
                target: victim,
                planned: false,
                horde: None,
                guard: false,
                forced: false,
                order_owner: None,
            },
        )?;
        diag!(
            "contact: entity {} attacks entity {} beside it",
            attacker.index(),
            victim.index()
        );
        Ok(())
    }

    fn hold_crush_cap(&self, tick: &mut Tick<'_>, ids: Ids, crusher: Entity) -> Result<()> {
        let horde = self.crushing_horde(tick, ids, crusher).unwrap_or(crusher);
        if let Some(cap) = self.crush_cap(tick, ids, horde) {
            let until = tick.frame().get().saturating_add(Self::crush_cap_ticks());
            tick.state_mut().insert(
                ids.crush_speed_cap,
                horde,
                crate::combat::CrushSpeedCap { cap, until },
            )?;
        }
        Ok(())
    }

    fn cap_crusher(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde: Entity,
        crusher: Entity,
        crushed: Entity,
        share: f32,
    ) -> Result<()> {
        if !share.is_finite() || share <= 0.0 {
            return Ok(());
        }
        let members = tick
            .state()
            .get(ids.horde, horde)
            .map_or(1, |horde| horde.members.len());
        let Some(members) = core::num::NonZeroUsize::new(members) else {
            return Ok(());
        };
        let min_velocity = self
            .record_of(tick, ids, horde)
            .map_or(0.0, |record| record.crush.min_velocity);
        let factor = mint_sage_combat::Attributes::crush_slow_factor(
            &self.bonuses_of(tick.state(), ids, crusher),
            &self.bonuses_of(tick.state(), ids, crushed),
        ) * self
            .bonuses_of(tick.state(), ids, horde)
            .minimum_crush_velocity_factor(min_velocity);
        let top = self.mover_limits(tick, ids, horde).max_speed;
        #[allow(clippy::cast_precision_loss)]
        let lost = share * factor * top / members.get() as f32;
        if !lost.is_finite() || lost <= 0.0 {
            return Ok(());
        }
        let from = self.crush_cap(tick, ids, horde).unwrap_or(top);
        let cap = (from - lost).max(0.0);
        let until = tick.frame().get().saturating_add(Self::crush_cap_ticks());
        tick.state_mut().insert(
            ids.crush_speed_cap,
            horde,
            crate::combat::CrushSpeedCap { cap, until },
        )?;
        if let Some(mut body) = tick.state().get(ids.body, horde).copied() {
            if body.0.speed > cap {
                body.0.speed = cap;
                tick.state_mut().insert(ids.body, horde, body)?;
            }
        }
        Ok(())
    }

    fn crush_throw_direction(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        driver: Entity,
        crusher: &Foe,
        crushed: &Foe,
    ) -> Vec2 {
        let facing = tick
            .state()
            .get(ids.body, driver)
            .map_or(Facing::from_radians(0.0), |body| body.0.facing);
        let towards = Facing::from_vector(crushed.position.minus(crusher.position))
            .map_or(0.0, |towards| facing.delta_to(towards));
        let turn = (HORDE_THROW_SHARE * towards)
            .clamp(-core::f32::consts::FRAC_PI_2, core::f32::consts::FRAC_PI_2);
        Facing::from_radians(facing.radians() + turn).to_vector()
    }

    fn begin_crush_contacts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
        crush: &[CrushState],
        hits: &[(u32, u32)],
    ) -> Result<Vec<(u32, u32)>> {
        let mut fresh = Vec::new();
        for (at, ((foe, _), state)) in living.iter().zip(crush).enumerate() {
            let before = tick.state().get(ids.crush_contacts, foe.entity);
            if state.crusher_level == 0 && before.is_none() {
                continue;
            }
            let now: Vec<(u32, Entity)> = hits
                .iter()
                .filter(|(crusher, _)| *crusher as usize == at)
                .filter_map(|&(_, crushed)| Some((crushed, living.get(crushed as usize)?.0.entity)))
                .collect();
            let Ok(crusher) = u32::try_from(at) else {
                continue;
            };
            fresh.extend(
                now.iter()
                    .filter(|(_, entity)| !before.is_some_and(|b| b.0.contains(entity)))
                    .map(|&(crushed, _)| (crusher, crushed)),
            );
            let contacts =
                crate::combat::CrushContacts(now.into_iter().map(|(_, entity)| entity).collect());
            if contacts.0.is_empty() {
                tick.state_mut().remove(ids.crush_contacts, foe.entity);
            } else if before != Some(&contacts) {
                tick.state_mut()
                    .insert(ids.crush_contacts, foe.entity, contacts)?;
            }
        }
        Ok(fresh)
    }

    #[allow(clippy::too_many_arguments)]
    fn fire_crush_weapon(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        weapon: &str,
        source: &Foe,
        victim: &Foe,
        events: &mut Vec<DamageEvent>,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(weapon_id) = self.combat.weapons.id(weapon) else {
            diag!("crush: weapon {weapon} is not declared; nothing fires");
            return Ok(());
        };
        diag!(
            "crush: frame {} entity {} fires {weapon} at entity {}",
            tick.frame().get(),
            source.entity.index(),
            victim.entity.index()
        );
        let origin = self.launch_of(tick.state(), source.entity, source);
        let shot = Shot::at_victim(
            &self.combat,
            weapon_id,
            source.entity,
            origin,
            (victim.entity, victim.position, victim.aim_point().z),
            victim.z,
        );
        let melee = self
            .combat
            .weapons
            .get(weapon_id)
            .is_some_and(|template| template.melee);
        let fx = if melee { victim.entity } else { source.entity };
        self.release_shot(tick, ids, &shot, fx, victim.position, None, living, events)
    }

    pub(crate) fn footprint_cells_of(
        &self,
        world: &World,
        ids: Ids,
        structure: Entity,
    ) -> Vec<Cell> {
        let Some(body) = world.get(ids.body, structure).map(|body| body.0) else {
            return Vec::new();
        };
        let Some(record) = world
            .get(ids.kind, structure)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .map(|entry| &entry.record)
        else {
            return Vec::new();
        };
        let records = footprint_of(record);
        if records.is_empty() {
            return Vec::new();
        }
        let obstacle = Obstacle {
            id: 0,
            centre: body.position,
            angle: body.facing.radians(),
            records,
        };
        covered_cells(
            self.passability.geometry,
            self.passability.grid.extent(),
            &obstacle,
        )
    }

    pub(crate) fn members_inside_a_structure_footprint(
        &self,
        world: &World,
        ids: Ids,
    ) -> Vec<Entity> {
        let Some(bodies) = world.store(ids.body) else {
            return Vec::new();
        };
        let structures: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, _)| {
                        !world.contains(ids.member, *entity) && !world.contains(ids.horde, *entity)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        let footprints: Vec<BTreeSet<(u16, u16)>> = structures
            .iter()
            .map(|structure| {
                self.footprint_cells_of(world, ids, *structure)
                    .into_iter()
                    .map(|cell| (cell.x, cell.y))
                    .collect()
            })
            .collect();
        let extent = self.passability.grid.extent();
        bodies
            .iter()
            .filter(|(entity, _)| world.contains(ids.member, *entity))
            .filter_map(|(entity, body)| {
                let cell = self
                    .passability
                    .geometry
                    .cell_at(body.0.position, 0, extent)?;
                footprints
                    .iter()
                    .any(|cells| cells.contains(&(cell.x, cell.y)))
                    .then_some(entity)
            })
            .collect()
    }

    pub(super) fn advance_movers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let modes = self.modes(tick, ids);
        let mut pipeline = Pipeline::new(modes.config());
        pipeline.use_sliding_follower();
        let geometry = self.passability.geometry;
        let dt = Self::tick_seconds();
        let avoidance_population = if modes.avoidance == AvoidanceMode::Off {
            Vec::new()
        } else {
            Self::avoidance_population(tick, ids)
        };
        let avoidance_grid = AvoidanceGrid::build(avoidance_population);

        let moving: Vec<(Entity, Route)> = tick
            .state()
            .store(ids.path)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, path)| {
                        let mut route = path.route.clone();
                        if path.kind == PathKind::Closing
                            && self.is_melee_horde(tick.state(), ids, entity)
                        {
                            route.extra_distance = MELEE_HORDE_APPROACH_EXTRA;
                        }
                        (entity, route)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut replan: Vec<Entity> = Vec::new();
        let mut avoid_scratch: Vec<Neighbour> = Vec::new();

        for (entity, route) in moving {
            if tick
                .state()
                .get(ids.knocked, entity)
                .is_some_and(super::knockback::Knocked::off_its_feet)
                || self.flies_itself(tick.state(), ids, entity)
                || self.holds_for_reload(tick, ids, entity)
            {
                continue;
            }
            let Some(body) = tick.state().get(ids.body, entity).copied() else {
                continue;
            };
            #[allow(clippy::cast_precision_loss)]
            let climb_speed = self
                .crush_capped(tick, ids, entity, self.mover_limits(tick, ids, entity))
                .max_speed
                / SIMULATION_HZ as f32;
            if self.climb_step(tick, ids, entity, body.0, climb_speed)? {
                continue;
            }
            let view = self.dynamic_view(tick, ids);
            let ignored = tick.state().get(ids.ignored_obstacle, entity).copied();
            let ignored_cells = ignored
                .map(|ignored| self.footprint_cells_of(tick.state(), ids, ignored.0))
                .unwrap_or_default();
            let tunnelling = ignored.is_some()
                && geometry
                    .cell_at(body.0.position, 0, view.extent())
                    .is_some_and(|cell| view.flags(cell).intersects(OBSTACLE));
            let exempt = ObstacleExemptView {
                inner: view,
                excluded: &ignored_cells,
                tunnelling,
            };
            let context = MoveContext {
                grid: &exempt,
                geometry,
                mover: self.mover_class(tick, ids, entity),
                layer: 0,
            };
            let mut limits =
                self.crush_capped(tick, ids, entity, self.mover_limits(tick, ids, entity));
            limits.desired_speed = if self.move_blocked(tick, ids, entity) {
                0.0
            } else {
                tick.state()
                    .get(ids.desired_speed, entity)
                    .map_or(f32::INFINITY, |desired| desired.0)
            };
            if let Some(kinematics) = self.kinematics_of(tick, ids, entity) {
                limits.turn_pivot =
                    kinematics.turn_pivot_offset * self.object_radius(tick, ids, entity);
                limits.backs = kinematics.backs_in_terror
                    && self.holds_condition(tick, entity, TERROR_CONDITION);
            }
            let limits = self.cavalry_creep(tick, ids, entity, body.0, &route, limits);
            let (start, reformed) = self.reform_turn(tick, ids, entity, body.0, &route, &limits);
            let entry_reached = start.speed <= 0.0
                && start.waypoint == 0
                && route
                    .points
                    .last()
                    .is_some_and(|end| end.minus(start.position).length() <= ENTRY_REACHED);
            let motion = if entry_reached {
                mint_movement::Motion {
                    state: start,
                    velocity: Vec2::ZERO,
                    status: MotionStatus::Arrived,
                    phase: mint_movement::MotionPhase::NONE,
                }
            } else {
                pipeline.follow(&context, start, &route, &limits, dt)
            };
            if motion
                .phase
                .contains(mint_movement::MotionPhase::TURNING_LEFT)
                || motion
                    .phase
                    .contains(mint_movement::MotionPhase::TURNING_RIGHT)
            {
                diag!(
                    "turn: entity {} frame {} facing {:.1} speed {:.1} rate {:.2}",
                    entity.index(),
                    tick.frame().get(),
                    motion.state.facing.radians().to_degrees(),
                    motion.state.speed,
                    limits.turn_rate
                );
            }
            if limits.desired_speed <= 0.0 {
                diag!(
                    "contact: entity {} held at frame {} speed {:.2} -> {:.2}",
                    entity.index(),
                    tick.frame().get(),
                    start.speed,
                    motion.state.speed
                );
            }
            let radius = Self::mover_radius(tick, ids, entity);
            let agent = Agent {
                id: MoverId(entity.index()),
                position: motion.state.position,
                radius,
            };
            let query = IgnoringTarget {
                grid: &avoidance_grid,
                ignored: Self::attack_goal(tick, ids, entity).map(|goal| MoverId(goal.index())),
            };
            let motion = pipeline.avoid(
                &context,
                &agent,
                body.0.position,
                motion,
                &query,
                &mut avoid_scratch,
                dt,
            );

            let mut arrived_state = motion.state;
            let mut reformed = reformed;
            let mut status = motion.status;
            if status == MotionStatus::Arrived {
                if let Some(facing) = tick.state().get(ids.path, entity).and_then(|p| p.facing) {
                    let error = arrived_state.facing.delta_to(facing).abs();
                    let reform = tick.state().contains(ids.horde, entity)
                        && self
                            .max_turn_without_reform(tick, ids, entity)
                            .is_some_and(|limit| error > limit);
                    if reform || error < ARRIVAL_FACING_SNAP {
                        reformed |= reform;
                        arrived_state.facing = facing;
                    } else {
                        arrived_state.facing = arrived_state
                            .facing
                            .turned_towards(facing, limits.turn_rate * dt);
                        status = MotionStatus::Moving;
                    }
                }
            }
            tick.state_mut()
                .insert(ids.body, entity, Body(arrived_state))?;
            if reformed {
                self.reform_horde(tick, ids, entity)?;
            }
            self.stand(tick, ids, entity, arrived_state.position)?;
            tick.state_mut()
                .insert(ids.motive, entity, crate::unit::Motive(motion.phase))?;
            if !ignored_cells.is_empty() {
                let reached_first_waypoint = route.points.first().is_some_and(|first| {
                    arrived_state.position.minus(*first).length() <= limits.capture_radius()
                });
                if reached_first_waypoint || motion.status == MotionStatus::Arrived {
                    tick.state_mut().remove(ids.ignored_obstacle, entity);
                    diag!(
                        "obstacles: entity {} cleared its ignored obstacle at frame {}",
                        entity.index(),
                        tick.frame().get()
                    );
                }
            }
            match status {
                MotionStatus::Moving => {}
                MotionStatus::Arrived => {
                    tick.state_mut().remove(ids.path, entity);
                    tick.state_mut().remove(ids.formation_speed, entity);
                    if tick.state().contains(ids.build_task, entity) {
                        self.begin_building(tick, ids, entity)?;
                    }
                }
                MotionStatus::Blocked => {
                    replan.push(entity);
                }
            }
        }

        for entity in replan {
            self.replan_if_due(tick, ids, entity, budgets)?;
        }
        Ok(())
    }

    pub(super) fn coast_while_waiting(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(clock_entity) = tick
            .state()
            .store(ids.path_queue)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(queue) = tick.state().get(ids.path_queue, clock_entity).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        let dt = Self::tick_seconds();
        let waiting: Vec<Entity> = queue
            .iter()
            .filter(|entity| tick.state().get(ids.path, *entity).is_none())
            .filter(|entity| {
                tick.state()
                    .get(ids.path_retry, *entity)
                    .is_none_or(|retry| retry.next_attempt <= now)
            })
            .collect();
        for entity in waiting {
            let Some(body) = tick.state().get(ids.body, entity).copied() else {
                continue;
            };
            let speed = body.0.speed;
            let max_speed = self.mover_limits(tick, ids, entity).max_speed.max(0.0);
            if max_speed <= 0.0 || speed <= 0.25 * max_speed {
                continue;
            }
            let heading = body.0.facing.to_vector();
            let probe = body.0.position.plus(heading.scale(2.0 * speed * dt));
            let mover = self.mover_class(tick, ids, entity);
            if self.passable_cell_for(tick, ids, probe, mover).is_none() {
                continue;
            }
            let mut state = body.0;
            state.position = state.position.plus(heading.scale(speed * dt));
            tick.state_mut().insert(ids.body, entity, Body(state))?;
        }
        Ok(())
    }

    pub(super) fn hold_still(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let world = tick.state();
        let waiting = world
            .store(ids.path_queue)
            .and_then(|store| store.iter().next().map(|(_, queue)| queue.clone()));
        let still: Vec<Entity> = world
            .store(ids.body)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, body)| {
                        body.0.speed != 0.0
                            || world
                                .get(ids.motive, *entity)
                                .is_some_and(|motive| motive.0 != mint_movement::MotionPhase::NONE)
                    })
                    .map(|(entity, _)| entity)
                    .filter(|entity| {
                        !world.contains(ids.path, *entity)
                            && !world.contains(ids.member, *entity)
                            && !world.contains(ids.giant_bird, *entity)
                            && !waiting
                                .as_ref()
                                .is_some_and(|queue| queue.contains(*entity))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for entity in still {
            if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
                body.0.speed = 0.0;
                tick.state_mut().insert(ids.body, entity, body)?;
            }
            if tick.state().contains(ids.motive, entity) {
                tick.state_mut().insert(
                    ids.motive,
                    entity,
                    crate::unit::Motive(mint_movement::MotionPhase::NONE),
                )?;
            }
        }
        Ok(())
    }

    fn reform_turn(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: MoverState,
        route: &Route,
        limits: &MotionLimits,
    ) -> (MoverState, bool) {
        let Some(horde) = tick.state().get(ids.horde, entity) else {
            return (state, false);
        };
        let Some(limit) = self.max_turn_without_reform(tick, ids, entity) else {
            return (state, false);
        };
        let Some(look) = mint_movement::look_ahead(route, state, limits) else {
            return (state, false);
        };
        let Some(heading) = Facing::from_vector(look.point.minus(state.position)) else {
            return (state, false);
        };
        if state.facing.delta_to(heading).abs() <= limit || look.path_left <= REFORM_MIN_PATH_LEFT {
            return (state, false);
        }
        let horse = self
            .formations
            .get(usize::from(horde.formation))
            .is_some_and(|table| table.settings().horse);
        let mut snapped = state;
        snapped.facing = heading;
        if horse {
            snapped.speed = 0.0;
        }
        (snapped, true)
    }

    fn cavalry_creep(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: MoverState,
        route: &Route,
        mut limits: MotionLimits,
    ) -> MotionLimits {
        let Some(horde) = tick.state().get(ids.horde, entity) else {
            return limits;
        };
        let horse = self
            .formations
            .get(usize::from(horde.formation))
            .is_some_and(|table| table.settings().horse);
        if !horse || state.speed > HORSE_CREEP_BELOW * limits.max_speed {
            return limits;
        }
        let Some(heading) = mint_movement::look_ahead(route, state, &limits)
            .and_then(|look| Facing::from_vector(look.point.minus(state.position)))
        else {
            return limits;
        };
        let frame = 1.0 / crate::game_data::RETAIL_LOGIC_HZ as f32;
        let mut step = None;
        let ready = horde
            .members
            .iter()
            .filter(|member| !tick.state().contains(ids.member_attack, **member))
            .all(|member| {
                let Some(body) = tick.state().get(ids.body, *member) else {
                    return true;
                };
                let step = *step
                    .get_or_insert_with(|| self.mover_limits(tick, ids, *member).turn_rate * frame);
                body.0.facing.delta_to(heading).abs() <= step
            });
        if !ready {
            limits.desired_speed = limits.desired_speed.min(limits.max_speed) * HORSE_CREEP_SHARE;
        }
        limits
    }

    fn max_turn_without_reform(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<f32> {
        self.kinematics_of(tick, ids, entity)?
            .max_turn_without_reform
    }

    fn kinematics_of(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<crate::unit::Kinematics> {
        tick.state()
            .get(ids.locomotion, entity)
            .map(|locomotion| locomotion.0)
            .or_else(|| {
                let kind = tick.state().get(ids.kind, entity)?;
                self.kinds.get(*kind)?.limits
            })
    }

    pub(super) fn departure(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<mint_movement::Departure> {
        let radii = self.kinematics_of(tick, ids, entity)?.departure;
        let body = tick.state().get(ids.body, entity)?.0;
        let fast = body.speed > 0.25 * self.mover_limits(tick, ids, entity).max_speed;
        Some(mint_movement::Departure {
            heading: body.facing,
            radius: if fast { radii.fast } else { radii.slow },
            max_turn: if fast {
                radii.max_turn.min(radii.max_turn_fast)
            } else {
                radii.max_turn
            },
        })
    }

    pub(super) fn horde_in_contact(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde: &Horde,
        goal: Entity,
    ) -> bool {
        let targets: Vec<Foe> = match tick.state().get(Self::horde_ids(ids).horde, goal) {
            Some(theirs) => theirs
                .members
                .iter()
                .filter_map(|member| self.foe_of(tick, ids, *member))
                .collect(),
            None => self.foe_of(tick, ids, goal).into_iter().collect(),
        };
        horde.members.iter().any(|member| {
            let (Some(armed), Some(body)) = (
                tick.state().get(ids.armed, *member),
                tick.state().get(ids.body, *member),
            ) else {
                return false;
            };
            let Some(template) = self.combat.weapons.get(armed.weapon) else {
                return false;
            };
            let range = self.attack_range_of(tick.state(), ids, *member, template);
            let radius = tick
                .state()
                .get(ids.shape, *member)
                .map_or(0.0, |shape| shape.radius);
            targets.iter().any(|foe| {
                in_range(
                    body.0.position,
                    radius,
                    foe.position,
                    foe.radius,
                    0.0,
                    range,
                )
            })
        })
    }

    pub(super) fn holds_for_reload(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let Some(mut armed) = tick.state().get(ids.armed, entity).copied() else {
            return false;
        };
        self.combat
            .weapons
            .get(armed.weapon)
            .is_some_and(|template| armed.state.holds_orders(template, tick.frame()))
    }

    pub(super) fn attack_goal(tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<Entity> {
        let target = tick
            .state()
            .get(ids.attack_order, entity)
            .map(|order| order.target)
            .or_else(|| {
                tick.state()
                    .get(ids.horde_target, entity)
                    .and_then(|hordetarget| hordetarget.target)
            })?;
        Some(
            tick.state()
                .get(ids.member, target)
                .map_or(target, |member| member.horde),
        )
    }

    fn has_near_victim(&self, tick: &Tick<'_>, ids: Ids, mover: Entity) -> bool {
        const VETO_RANGE_SQUARED: f32 = 600.0 * 600.0;
        let target = Self::attack_goal(tick, ids, mover).or_else(|| {
            tick.state()
                .get(ids.armed, mover)
                .and_then(|armed| armed.target)
        });
        let Some(target) = target else {
            return false;
        };
        let (Some(my), Some(their)) = (
            tick.state()
                .get(ids.body, mover)
                .map(|body| body.0.position),
            tick.state()
                .get(ids.body, target)
                .map(|body| body.0.position),
        ) else {
            return false;
        };
        let dz = Self::feet(tick.state(), ids, mover) - Self::feet(tick.state(), ids, target);
        my.minus(their).length_squared() + dz * dz < VETO_RANGE_SQUARED
    }

    pub(super) fn front_reach(
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
    ) -> f32 {
        let (Some(mine), Some(theirs)) = (
            tick.state().get(ids.body, horde_entity),
            tick.state().get(ids.body, goal),
        ) else {
            return Self::mover_radius(tick, ids, horde_entity);
        };
        let towards = theirs
            .0
            .position
            .minus(mine.0.position)
            .normalised()
            .unwrap_or(Vec2::new(1.0, 0.0));
        let back = towards.scale(-1.0);
        Self::edge_towards(tick, ids, horde_entity, towards)
            + Self::edge_towards(tick, ids, goal, back)
    }

    pub(super) fn edge_towards(tick: &Tick<'_>, ids: Ids, entity: Entity, direction: Vec2) -> f32 {
        let Some(horde) = tick.state().get(Self::horde_ids(ids).horde, entity) else {
            return tick
                .state()
                .get(ids.shape, entity)
                .map_or(0.0, |shape| shape.radius);
        };
        let Some(body) = tick.state().get(ids.body, entity) else {
            return Self::mover_radius(tick, ids, entity);
        };
        let member_radius = horde
            .members
            .first()
            .and_then(|member| tick.state().get(ids.shape, *member))
            .map_or(5.0, |shape| shape.radius);
        let basis = mint_sage_formation::FormationBasis::of(body.0.facing);
        let extent = horde.offsets().fold(0.0f32, |extent, offset| {
            let placed = basis.place(Vec2::ZERO, offset);
            extent.max(placed.x * direction.x + placed.y * direction.y)
        });
        extent + member_radius
    }

    pub(super) fn mover_class(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> MoverClass {
        self.mover_class_in(tick.state(), ids, entity)
    }

    pub(super) fn mover_class_in(&self, world: &World, ids: Ids, entity: Entity) -> MoverClass {
        let terrain = world
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| entry.mover)
            .unwrap_or(GROUND);
        let asker = world
            .get(ids.owner, entity)
            .map_or(Asker::NOBODY, |owner| Asker::new(owner.0.get()));
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        self.restriction(world, ids)
            .apply(terrain.asking(mint_sage_passability::PORTAL, asker), owner)
    }

    pub(crate) fn selection_reaches(&self, world: &World, player: PlayerId, at: Vec2) -> bool {
        use mint_movement::Passability as _;
        let Some(ids) = self.ids else {
            return true;
        };
        let seen = crate::shroud::with_grid(world, Self::shroud_ids(ids), |grid| {
            let (x, y) = grid.world_to_cell(at.x, at.y);
            grid.cell_state(player, x, y) == mint_sage_shroud::CellVisibility::Visible
        });
        if seen == Some(false) {
            return true;
        }
        let Some(to) = self.passability.cell_at(at) else {
            return false;
        };
        self.command_subjects_in(world, ids, player)
            .into_iter()
            .any(|entity| {
                let mobile = world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| {
                        !entry
                            .record
                            .kind_of
                            .iter()
                            .any(|k| k.eq_ignore_ascii_case("IMMOBILE"))
                    });
                let from = world
                    .get(ids.body, entity)
                    .and_then(|body| self.passability.cell_at(body.0.position));
                let mover = self.mover_class_in(world, ids, entity);
                mobile
                    && from.is_some_and(|from| {
                        self.passability.grid.is_passable(to, mover)
                            && self.reachable(mover, from, to)
                    })
            })
    }

    pub(super) fn reachable(&self, mover: MoverClass, from: Cell, to: Cell) -> bool {
        let reachability = self.reachability.borrow();
        reachability.mover() != mover.terrain() || reachability.same_component(from, to)
    }

    pub(super) fn climbs_walls(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        if tick.state().contains(ids.member, entity)
            || tick.state().contains(ids.horde_member_mark, entity)
            || tick.state().contains(ids.contained, entity)
        {
            return false;
        }
        let Some(shape) = tick.state().get(ids.shape, entity) else {
            return false;
        };
        if shape.kind_of.contains(mint_sage_asset::KindOf::HORDE)
            || !shape
                .kind_of
                .contains(mint_sage_asset::KindOf::CAN_CLIMB_WALLS)
        {
            return false;
        }
        self.kinematics_of(tick, ids, entity)
            .is_some_and(|kinematics| kinematics.scales_walls)
    }

    pub(super) fn wall_at(
        &self,
        world: &World,
        ids: Ids,
        point: Vec2,
    ) -> Option<mint_sage_passability::WallGeometry> {
        if let Some(wall) = self
            .passability
            .cell_at(point)
            .and_then(|cell| self.passability.obstacle_at(cell))
            .and_then(|id| self.wall_geometry_at(id))
        {
            return Some(wall);
        }
        let walls = crate::obstacles::scaleable_walls_of(world, ids.scaleable_walls)?;
        walls.iter().find_map(|(_, wall)| {
            let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
            let dx = point.x - wall.centre.x;
            let dy = point.y - wall.centre.y;
            let local_x = dx * cos + dy * sin;
            let local_y = -dx * sin + dy * cos;
            (local_x.abs() <= wall.major_radius && local_y.abs() <= wall.minor_radius)
                .then_some(*wall)
        })
    }

    pub(super) fn wall_entity_at(&self, world: &World, ids: Ids, point: Vec2) -> Option<Entity> {
        let walls = crate::obstacles::scaleable_walls_of(world, ids.scaleable_walls)?;
        walls.iter().find_map(|(entity, wall)| {
            let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
            let dx = point.x - wall.centre.x;
            let dy = point.y - wall.centre.y;
            let local_x = dx * cos + dy * sin;
            let local_y = -dx * sin + dy * cos;
            (local_x.abs() <= wall.major_radius && local_y.abs() <= wall.minor_radius)
                .then_some(*entity)
        })
    }

    fn write_climb(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        state: mint_movement::MoverState,
        z: f32,
    ) -> Result<()> {
        tick.state_mut().insert(ids.body, entity, Body(state))?;
        match tick.state_mut().get_mut(ids.elevation, entity) {
            Some(elevation) => elevation.0 = z,
            None => {
                tick.state_mut()
                    .insert(ids.elevation, entity, crate::unit::Elevation::new(z))?;
            }
        }
        Ok(())
    }

    pub(super) fn climb_step(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        body: mint_movement::MoverState,
        speed: f32,
    ) -> Result<bool> {
        if !self.climbs_walls(tick, ids, entity) {
            return Ok(false);
        }
        let heights = &self.passability.heights;
        let position = body.position;
        let facing = body.facing;
        let ground_here = heights.height_at_point(position).unwrap_or(0.0);
        let current_z = tick
            .state()
            .get(ids.elevation, entity)
            .map_or(ground_here, |elevation| elevation.0);
        let was_climbing = self.holds_model_condition(tick, entity, CLIMBING);
        let was_rappelling = self.holds_model_condition(tick, entity, RAPPELLING);
        let step = speed.min(CLIMB_PROBE_CAP_PER_TICK);
        let probe = position.plus(facing.to_vector().scale(step));
        if let Some(wall) = self.wall_at(tick.state(), ids, probe) {
            let wall_ground = heights.height_at_point(wall.centre).unwrap_or(ground_here);
            let top = wall_ground + wall.height;
            let (sin, cos) = mint_math::f32::sin_cos(wall.angle);
            let probe_dx = probe.x - wall.centre.x;
            let probe_dy = probe.y - wall.centre.y;
            let local_x = probe_dx * cos + probe_dy * sin;
            let local_y = -probe_dx * sin + probe_dy * cos;
            let inside = local_x.abs() <= wall.major_radius
                && local_y.abs() <= wall.minor_radius
                && current_z <= top;
            if !inside {
                if was_climbing || was_rappelling {
                    self.set_conditions(tick, ids, entity, &[], &[CLIMBING, RAPPELLING])?;
                }
                return Ok(false);
            }
            if current_z >= top {
                self.write_climb(
                    tick,
                    ids,
                    entity,
                    mint_movement::MoverState {
                        position: probe,
                        ..body
                    },
                    top,
                )?;
                if was_climbing || was_rappelling {
                    self.set_conditions(tick, ids, entity, &[], &[CLIMBING, RAPPELLING])?;
                }
                return Ok(true);
            }
            if !was_climbing {
                let wall_facing = Facing::from_radians(wall.angle);
                let final_facing = if facing.to_vector().dot(wall_facing.to_vector()) < 0.0 {
                    Facing::from_radians(wall.angle + core::f32::consts::PI)
                } else {
                    wall_facing
                };
                let pos_dx = position.x - wall.centre.x;
                let pos_dy = position.y - wall.centre.y;
                let pos_local_x = pos_dx * cos + pos_dy * sin;
                let pos_local_y = -pos_dx * sin + pos_dy * cos;
                let clamped_x = pos_local_x.clamp(-wall.major_radius, wall.major_radius);
                let snapped = Vec2::new(
                    wall.centre.x + clamped_x * cos - pos_local_y * sin,
                    wall.centre.y + clamped_x * sin + pos_local_y * cos,
                );
                self.write_climb(
                    tick,
                    ids,
                    entity,
                    mint_movement::MoverState {
                        position: snapped,
                        facing: final_facing,
                        ..body
                    },
                    current_z,
                )?;
                self.set_conditions(tick, ids, entity, &[CLIMBING], &[RAPPELLING])?;
                diag!("climb: {} CLIMBING set", entity.index());
                if let Some(wall_entity) = self.wall_entity_at(tick.state(), ids, probe) {
                    Self::note_combat_fact(tick, ids, |facts, now| {
                        facts.push_wall_climb(now, entity, wall_entity);
                    });
                }
                return Ok(true);
            }
            let next_z = (current_z + speed * CLIMB_RATE_SHARE).min(top);
            self.write_climb(tick, ids, entity, body, next_z)?;
            return Ok(true);
        }
        let ground_at_probe = heights.height_at_point(probe).unwrap_or(ground_here);
        let slack = (ground_here - ground_at_probe + 0.1).max(0.0);
        if current_z > ground_at_probe + slack {
            if !was_rappelling {
                self.write_climb(tick, ids, entity, body, current_z)?;
                self.set_conditions(tick, ids, entity, &[RAPPELLING], &[CLIMBING])?;
                return Ok(true);
            }
            let next_z = current_z - speed * CLIMB_RATE_SHARE;
            if next_z <= ground_at_probe + CLIMB_LANDING_TOLERANCE {
                self.write_climb(tick, ids, entity, body, ground_at_probe)?;
                self.set_conditions(tick, ids, entity, &[], &[CLIMBING, RAPPELLING])?;
                diag!(
                    "climb: {} CLIMBING/RAPPELLING cleared (landed)",
                    entity.index()
                );
                tick.state_mut().remove(ids.path, entity);
                return Ok(true);
            }
            self.write_climb(tick, ids, entity, body, next_z)?;
            return Ok(true);
        }
        if was_climbing || was_rappelling {
            self.set_conditions(tick, ids, entity, &[], &[CLIMBING, RAPPELLING])?;
        }
        Ok(false)
    }

    fn height_rule(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> HeightRule {
        let rule = self
            .flight_profile(tick, ids, entity)
            .map_or(HeightRule::ON_SURFACE, |flight| flight.height);
        match tick.state().get(ids.flight_level, entity) {
            Some(level) => rule.at_level(level.0).undamped(),
            None => rule,
        }
    }

    pub(super) fn flight_profile(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<crate::templates::FlightProfile> {
        tick.state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
            .and_then(|entry| entry.flight)
    }

    fn step_flight_level(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        position: Vec2,
    ) -> Result<()> {
        let Some(flight) = self.flight_profile(tick, ids, entity) else {
            return Ok(());
        };
        let Some(dive) = flight.dive else {
            return Ok(());
        };
        let own = flight.height.level();
        let current = tick
            .state()
            .get(ids.flight_level, entity)
            .map_or(own, |level| level.0);
        let limits = self.mover_limits(tick, ids, entity);
        let target = self.dive_target(tick, ids, entity, position, dive.reach);
        let goal = if target.is_some() { dive.level } else { own };
        if current.to_bits() == goal.to_bits() {
            return self.set_flight_pose(tick, ids, entity, current, current);
        }
        let step = match target {
            _ if limits.turn_circle.is_some() => {
                dive.slope * self.flown_this_tick(tick, ids, entity)
            }
            _ => {
                #[allow(clippy::cast_precision_loss)]
                let per_tick = limits.max_speed / SIMULATION_HZ as f32;
                let slope_step = dive.slope * per_tick;
                match target {
                    Some((approach, _)) if goal < current && per_tick > 0.0 => {
                        let ticks = if approach > 0.0 {
                            (approach / per_tick).max(1.0)
                        } else {
                            DIVE_EASE_TICKS
                        };
                        slope_step.max((current - goal) / ticks)
                    }
                    _ => slope_step,
                }
            }
        };
        let next = mint_movement::step_level(current, goal, step);
        self.set_flight_pose(tick, ids, entity, current, next)?;
        diag!(
            "height: {} {} at frame {} level={next:.1} goal={goal:.1}",
            entity.index(),
            if goal < current { "dives" } else { "climbs" },
            tick.frame().get()
        );
        if next.to_bits() == own.to_bits() {
            tick.state_mut().remove(ids.flight_level, entity);
        } else {
            tick.state_mut()
                .insert(ids.flight_level, entity, FlightLevel(next))?;
        }
        Ok(())
    }

    fn set_flight_pose(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        current: f32,
        next: f32,
    ) -> Result<()> {
        let climbing = next > current;
        if climbing == self.holds_model_condition(tick, entity, CLIMBING) {
            return Ok(());
        }
        if climbing {
            self.set_conditions(tick, ids, entity, &[CLIMBING], &[])
        } else {
            self.set_conditions(tick, ids, entity, &[], &[CLIMBING])
        }
    }

    fn flown_this_tick(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let hz = SIMULATION_HZ as f32;
        tick.state()
            .get(ids.body, entity)
            .map_or(0.0, |body| body.0.speed.max(0.0))
            / hz
    }

    pub(super) fn down_from_dive(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let Some(dive) = self
            .flight_profile(tick, ids, entity)
            .and_then(|flight| flight.dive)
        else {
            return true;
        };
        tick.state()
            .get(ids.flight_level, entity)
            .is_some_and(|level| level.0 <= dive.level + DIVE_DOWN_TOLERANCE)
    }

    fn dive_target(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        position: Vec2,
        reach: f32,
    ) -> Option<(f32, Vec2)> {
        let armed = tick.state().get(ids.armed, entity)?;
        let weapon = self
            .combat
            .weapons
            .get(armed.weapon)
            .filter(|weapon| weapon.can_swoop)?;
        let target = Self::attack_goal(tick, ids, entity)
            .or(armed.target)
            .filter(|target| !self.is_unattackable(tick.state(), ids, *target))?;
        let body = tick.state().get(ids.body, target)?;
        let offset = body.0.position.minus(position);
        let distance = offset.length();
        if distance > reach {
            return None;
        }
        let radius = |of: Entity| {
            tick.state()
                .get(ids.shape, of)
                .map_or(0.0, |shape| shape.radius)
        };
        Some((
            (distance - weapon.attack_range - radius(entity) - radius(target)).max(0.0),
            offset,
        ))
    }

    pub(super) fn feet(world: &World, ids: Ids, entity: Entity) -> f32 {
        world.get(ids.elevation, entity).map_or(0.0, |e| e.0)
            + world.get(ids.ground_offset, entity).map_or(0.0, |o| o.0)
    }

    pub(super) fn fx_point_of(
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<crate::fx::FxPoint> {
        let at = world.get(ids.body, entity)?.0.position;
        Some(if world.contains(ids.elevation, entity) {
            mint_sage_combat::Point3::new(at.x, at.y, Self::feet(world, ids, entity)).into()
        } else {
            at.into()
        })
    }

    fn stand(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        position: Vec2,
    ) -> Result<Option<(f32, f32)>> {
        let heights = &self.passability.heights;
        let rule = self.height_rule(tick, ids, entity);
        let ground = heights.height_at_point(position);
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let wall_layer = crate::wall_layer::layer_of(tick.state(), ids.wall_layer);
        let current_layer = tick
            .state()
            .get(ids.layer, entity)
            .copied()
            .unwrap_or(crate::wall_layer::Layer::GROUND);
        let previous = tick.state().get(ids.elevation, entity).map(|e| e.0);
        let current_z = previous.unwrap_or_else(|| ground.unwrap_or(0.0));
        let horde = Self::horde_of(tick, ids, entity);
        let is_member = horde.is_some_and(|horde| horde != entity);
        let is_idle_body = horde == Some(entity) && !tick.state().contains(ids.path, entity);
        let layer = wall_layer.map_or(crate::wall_layer::Layer::GROUND, |layer| {
            if is_member || is_idle_body {
                layer.layer_at(geometry, extent, position)
            } else {
                layer.sticky_layer(geometry, extent, position, current_layer, current_z)
            }
        });
        let surface = wall_layer.and_then(|wall_layer| {
            wall_layer.wall_height(geometry, extent, position, layer, ground)
        });
        let target = match rule {
            HeightRule::Surface { above, .. } => surface.or(ground).map(|s| s + above),
            HeightRule::Fixed { height, .. } => Some(height),
            HeightRule::Unchanged => surface
                .or(ground)
                .map(|floor| previous.map_or(floor, |z| z.max(floor))),
        };
        let (Some(target), Some(ground)) = (target, ground) else {
            return Ok(None);
        };
        let mut stepped = None;
        let z = match (rule.damping(), previous) {
            (Some(_), None) => ground,
            (Some(share), Some(current)) => {
                let ticks = u16::try_from(
                    mint_sim::SIMULATION_HZ
                        .checked_div(crate::game_data::RETAIL_LOGIC_HZ)
                        .unwrap_or(1),
                )
                .unwrap_or(1);
                let next = mint_movement::step_damped(
                    current,
                    target,
                    share,
                    f32::from(u16::try_from(crate::game_data::RETAIL_LOGIC_HZ).unwrap_or(5)),
                    ticks,
                );
                stepped = Some((next - current) * f32::from(ticks));
                next
            }
            (None, _) => target,
        };
        if let Some(rise) = stepped {
            let climbing = rise > 0.5;
            if climbing != self.holds_model_condition(tick, entity, CLIMBING) {
                if climbing {
                    self.set_conditions(tick, ids, entity, &[CLIMBING], &[])?;
                } else {
                    self.set_conditions(tick, ids, entity, &[], &[CLIMBING])?;
                }
            }
        }
        let trail_placed = tick.state().contains(ids.glided, entity);
        let z = if is_member && !trail_placed && !tick.state().contains(ids.path, entity) {
            let layer_height = surface.unwrap_or(ground);
            if current_z > layer_height {
                Self::horde_member_height(tick, ids, entity, layer, current_z, layer_height)
            } else {
                layer_height
            }
        } else if current_z > z {
            Self::horde_member_height(tick, ids, entity, layer, current_z, z)
        } else {
            z
        };
        match tick.state_mut().get_mut(ids.elevation, entity) {
            Some(elevation) => elevation.0 = z,
            None => {
                tick.state_mut()
                    .insert(ids.elevation, entity, crate::unit::Elevation::new(z))?;
            }
        }
        match tick.state_mut().get_mut(ids.layer, entity) {
            Some(slot) => *slot = layer,
            None => {
                tick.state_mut().insert(ids.layer, entity, layer)?;
            }
        }
        Ok(Some((z, ground)))
    }

    fn horde_member_height(
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        layer: crate::wall_layer::Layer,
        current_z: f32,
        z: f32,
    ) -> f32 {
        let Some(horde) = Self::horde_of(tick, ids, entity) else {
            return z;
        };
        if horde == entity {
            return z;
        }
        if layer.is_wall_top() {
            return current_z;
        }
        if layer == crate::wall_layer::Layer::RAMPS {
            return ramp_descent_step(current_z, z);
        }
        if layer == crate::wall_layer::Layer::GROUND
            && tick
                .state()
                .get(ids.layer, horde)
                .is_some_and(|horde_layer| *horde_layer != crate::wall_layer::Layer::GROUND)
        {
            return current_z;
        }
        z
    }

    pub(super) fn stand_on_terrain(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let bodies: Vec<(Entity, Vec2)> = tick
            .state()
            .store(ids.body)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, body)| (entity, body.0.position))
                    .collect()
            })
            .unwrap_or_default();
        let report = tick.frame().get() % u64::from(SIMULATION_HZ) == 0;
        let walls = crate::wall_layer::layer_of(tick.state(), ids.wall_layer)
            .map(crate::wall_layer::WallLayer::generation);
        for (entity, position) in bodies {
            if Self::stood_still(tick.state(), ids, entity, position, walls) {
                continue;
            }
            if tick.state().contains(ids.contained, entity)
                || tick.state().contains(ids.attach_link, entity)
                || self.is_giant_bird(tick.state(), ids, entity)
                || (self.climbs_walls(tick, ids, entity)
                    && (self.holds_model_condition(tick, entity, CLIMBING)
                        || self.holds_model_condition(tick, entity, RAPPELLING)))
                || self.holds_model_condition(tick, entity, SCALING_WALL_HORDE)
            {
                continue;
            }
            let placed = !tick.state().contains(ids.elevation, entity);
            let before = (
                tick.state()
                    .get(ids.elevation, entity)
                    .map(|e| e.0.to_bits()),
                tick.state().get(ids.layer, entity).copied(),
            );
            self.step_flight_level(tick, ids, entity, position)?;
            let Some((z, ground)) = self.stand(tick, ids, entity, position)? else {
                continue;
            };
            self.note_stood_still(tick, ids, entity, position, walls, before);
            if (placed && tick.state().contains(ids.health, entity))
                || (report && tick.state().contains(ids.path, entity))
            {
                diag!(
                    "height: {} {} at ({:.0},{:.0}) z={z:.1} ground={ground:.1} above={:.1}",
                    entity.index(),
                    if placed { "placed" } else { "moving" },
                    position.x,
                    position.y,
                    z - ground
                );
            }
        }
        Ok(())
    }

    pub(super) fn stood_still(
        world: &World,
        ids: Ids,
        entity: Entity,
        position: Vec2,
        walls: Option<u64>,
    ) -> bool {
        let Some(crate::unit::Elevation(z, Some(memo))) = world.get(ids.elevation, entity).copied()
        else {
            return false;
        };
        memo.z == z.to_bits()
            && memo.walls == walls
            && memo.x == position.x.to_bits()
            && memo.y == position.y.to_bits()
            && world.get(ids.layer, entity) == Some(&memo.layer)
            && world.get(ids.kind, entity) == Some(&memo.kind)
            && !world.contains(ids.flight_level, entity)
            && Self::horde_inputs(world, ids, entity)
                == (memo.horde, memo.horde_layer, memo.glided, memo.path)
    }

    fn horde_inputs(
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> (Option<Entity>, Option<crate::wall_layer::Layer>, bool, bool) {
        let horde = world
            .get(ids.member, entity)
            .map(|member| member.horde)
            .or_else(|| world.contains(ids.horde, entity).then_some(entity));
        match horde {
            None => (None, None, false, false),
            Some(horde) => (
                Some(horde),
                world.get(ids.layer, horde).copied(),
                world.contains(ids.glided, entity),
                world.contains(ids.path, entity),
            ),
        }
    }

    fn note_stood_still(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        position: Vec2,
        walls: Option<u64>,
        before: (Option<u32>, Option<crate::wall_layer::Layer>),
    ) {
        let (Some(z), Some(layer)) = before else {
            return;
        };
        let world = tick.state();
        let unchanged = world.get(ids.elevation, entity).map(|e| e.0.to_bits()) == Some(z)
            && world.get(ids.layer, entity) == Some(&layer);
        if !unchanged
            || world.contains(ids.flight_level, entity)
            || self
                .flight_profile(tick, ids, entity)
                .is_some_and(|flight| flight.dive.is_some() || flight.height.damping().is_some())
        {
            return;
        }
        let Some(kind) = world.get(ids.kind, entity).copied() else {
            return;
        };
        let (horde, horde_layer, glided, path) = Self::horde_inputs(world, ids, entity);
        if let Some(elevation) = tick.state_mut().get_mut(ids.elevation, entity) {
            elevation.1 = Some(crate::unit::StoodStill {
                x: position.x.to_bits(),
                y: position.y.to_bits(),
                z,
                layer,
                kind,
                walls,
                horde,
                horde_layer,
                glided,
                path,
            });
        }
    }

    pub(super) fn mover_limits(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> MotionLimits {
        let limits = crate::experience::with_speed(
            self.locomotor_limits(tick, ids, entity),
            tick.state().get(ids.bonuses, entity),
        );
        let river = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.flight)
            .map_or(1.0, |flight| flight.river_modifier);
        let wet = river != 1.0
            && tick
                .state()
                .get(ids.body, entity)
                .is_some_and(|body| self.passability.in_river(body.0.position));
        let limits = if wet {
            MotionLimits {
                max_speed: limits.max_speed * river,
                ..limits
            }
        } else {
            limits
        };
        match tick.state().get(ids.formation_speed, entity) {
            Some(cap) if cap.speed < limits.max_speed && limits.max_speed > 0.0 => {
                let share = cap.speed.max(0.0) / limits.max_speed;
                MotionLimits {
                    max_speed: limits.max_speed * share,
                    acceleration: limits.acceleration * share,
                    braking: limits.braking * share,
                    ..limits
                }
            }
            _ => limits,
        }
    }

    pub(super) fn cohere_move_speed(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        movers: &[Entity],
    ) -> Result<()> {
        for mover in movers {
            tick.state_mut().remove(ids.formation_speed, *mover);
        }
        if movers
            .iter()
            .any(|mover| self.has_near_victim(tick, ids, *mover))
        {
            return Ok(());
        }
        let slowest = movers
            .iter()
            .map(|mover| self.mover_limits(tick, ids, *mover).max_speed)
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .fold(f32::INFINITY, f32::min);
        let Some(first) = movers.iter().copied().min() else {
            return Ok(());
        };
        if !slowest.is_finite() {
            return Ok(());
        }
        let group = crate::unit::CommandGroup {
            issued: tick.frame(),
            first,
        };
        diag!(
            "order: {} mover(s) held to the slowest top speed {slowest:.2}",
            movers.len()
        );
        for mover in movers {
            tick.state_mut().insert(
                ids.formation_speed,
                *mover,
                crate::unit::FormationSpeed {
                    speed: slowest,
                    group,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn uncohere_move_speed(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) {
        let selection = self.selection(tick, ids, player);
        let hordes = crate::horde::hordes_of(tick.state(), Self::horde_ids(ids), &selection);
        for entity in selection.into_iter().chain(hordes) {
            tick.state_mut().remove(ids.formation_speed, entity);
        }
    }

    pub(super) fn attack_state_uncohere(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        player: PlayerId,
        target: Entity,
    ) {
        let Some(group) = tick
            .state()
            .get(ids.formation_speed, attacker)
            .map(|cap| cap.group)
        else {
            return;
        };
        if self.computer_controlled(player) {
            return;
        }
        let vision = tick
            .state()
            .get(ids.kind, attacker)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.record.vision_range)
            .unwrap_or(0.0)
            * (1.0
                + tick
                    .state()
                    .get(ids.bonuses, attacker)
                    .map_or(0.0, |bonuses| bonuses.0.vision))
            .max(0.0);
        let lim = Self::attack_uncohere_reach(vision);
        let centre = |entity: Entity| {
            let world = tick.state();
            let position = world.get(ids.body, entity)?.0.position;
            let height = world
                .get(ids.shape, entity)
                .map_or(0.0, |shape| shape.height);
            Some((position, Self::feet(world, ids, entity) + height * 0.5))
        };
        let (Some((me, my_z)), Some((them, their_z))) = (centre(attacker), centre(target)) else {
            return;
        };
        let dz = their_z - my_z;
        if them.minus(me).length_squared() + dz * dz > lim * lim {
            return;
        }
        let members: Vec<Entity> = tick
            .state()
            .store(ids.formation_speed)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|entity| {
                        tick.state()
                            .get(ids.formation_speed, *entity)
                            .is_some_and(|cap| cap.group == group)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for entity in members {
            tick.state_mut().remove(ids.formation_speed, entity);
        }
    }

    pub(super) fn attack_uncohere_reach(vision: f32) -> f32 {
        vision.max(600.0)
    }

    fn locomotor_limits(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> MotionLimits {
        self.current_locomotor(tick, ids, entity)
            .unwrap_or(PLACEHOLDER_LIMITS)
    }

    pub(super) fn cannot_stop(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        self.current_locomotor(tick, ids, entity)
            .is_some_and(|limits| limits.min_speed > 0.0 || limits.turn_circle.is_some())
    }

    pub(super) fn current_locomotor(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Option<MotionLimits> {
        crate::unit::current_limits(
            tick.state(),
            &self.kinds,
            (ids.kind, ids.locomotion, ids.health),
            entity,
            Some(self.charge_state(tick, ids, entity)),
            self.crew_speed_factor(tick.state(), ids, entity),
        )
    }

    pub(super) fn is_attacking(world: &mint_sim::World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.attack_order, entity)
            || world.contains(ids.horde_target, entity)
            || world.contains(ids.horde_melee, entity)
            || world.contains(ids.loose_melee, entity)
    }

    pub(super) fn is_attacking_self_or_horde(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> bool {
        let world = tick.state();
        Self::is_attacking(world, ids, entity)
            || world
                .get(ids.member, entity)
                .is_some_and(|member| Self::is_attacking(world, ids, member.horde))
    }

    pub(super) fn charge_state(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> crate::unit::ChargeState {
        let world = tick.state();
        let attacking = Self::is_attacking(world, ids, entity);
        let ability = world.get(ids.power_book, entity).is_some_and(|book| {
            book.0.iter().any(|instance| {
                self.power_cast_rules
                    .get(usize::from(instance.power.get()))
                    .is_some_and(|rule| rule.charge_attack)
            })
        }) && self.holds_condition(tick, entity, "CHARGING");
        crate::unit::ChargeState { attacking, ability }
    }

    pub(super) fn turn_towards(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        towards: Vec2,
    ) -> Result<()> {
        let Some(mut body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        let Some(desired) = Facing::from_vector(towards.minus(body.0.position)) else {
            return Ok(());
        };
        let limits = self.mover_limits(tick, ids, entity);
        let step = limits.turn_rate * Self::tick_seconds();
        body.0.facing = body.0.facing.turned_towards(desired, step);
        tick.state_mut().insert(ids.body, entity, body)?;
        Ok(())
    }

    pub(super) fn mover_radius(tick: &Tick<'_>, ids: Ids, entity: Entity) -> f32 {
        if let Some(horde) = tick.state().get(Self::horde_ids(ids).horde, entity) {
            let member_radius = horde
                .members
                .first()
                .and_then(|member| tick.state().get(ids.shape, *member))
                .map_or(5.0, |shape| shape.radius);
            return horde
                .offsets()
                .fold(0.0f32, |farthest, offset| farthest.max(offset.length()))
                .max(0.0)
                + member_radius;
        }
        tick.state()
            .get(ids.shape, entity)
            .map_or(5.0, |shape| shape.radius)
    }

    pub(super) fn plan_path(
        &self,
        config: PipelineConfig,
        grid: &dyn mint_movement::Passability,
        request: &PathRequest,
        budget: &mut NodeBudget,
    ) -> mint_movement::Result<PlanOutcome> {
        let _perf = crate::perf::sub(crate::perf::sub_row!("path_plan"));
        let geometry = self.passability.geometry;
        let Ok(mut pipeline) = self.path_search.try_borrow_mut() else {
            return Pipeline::new(config).plan(grid, geometry, request, budget);
        };
        if pipeline.config() != config {
            *pipeline = Pipeline::new(config);
        }
        pipeline.plan(grid, geometry, request, budget)
    }

    pub(super) fn replan_if_due(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let Some(path) = tick.state().get(ids.path, entity).cloned() else {
            return Ok(());
        };
        let now = tick.frame();
        let is_head = budgets.is_head(entity);
        let due = tick
            .state()
            .get(ids.replanned_at, entity)
            .is_none_or(|last| now.get().saturating_sub(last.0.get()) >= REPLAN_COOLDOWN_FRAMES);
        if !due && !is_head {
            return Ok(());
        }
        let budget = budgets.budget_for(entity);
        if budget.remaining() == 0 {
            return Ok(());
        }
        tick.state_mut()
            .insert(ids.replanned_at, entity, ReplannedAt(now))?;
        let Some(body) = tick.state().get(ids.body, entity).copied() else {
            return Ok(());
        };
        if self.give_off_map_route(tick, ids, entity, (body.0.position, path.goal), path.kind)? {
            return Ok(());
        }
        let modes = self.modes(tick, ids);
        let mover = self.mover_class(tick, ids, entity);
        let Some(start) = self.start_cell_toward(tick, ids, body.0.position, path.goal, mover)
        else {
            return Ok(());
        };
        let exact_goal = self.passable_cell_for(tick, ids, path.goal, mover);
        let Some(goal_cell) = exact_goal.or_else(|| self.passability.cell_at(path.goal)) else {
            return Ok(());
        };
        if exact_goal.is_some() && !self.reachable(mover, start, goal_cell) {
            self.note_path_failure(tick, ids, entity)?;
            return Ok(());
        }
        let request = PathRequest {
            start,
            goal: goal_cell,
            mover,
            goal_id: GoalId(u64::from(entity.index())),
            origin: Some(body.0.position),
            corner_radius: self.mover_limits(tick, ids, entity).corner_radius,
            departure: self.departure(tick, ids, entity),
        };
        let ground = self.dynamic_view(tick, ids);
        let costs = self
            .unit_cost_view(tick, ids, entity, &ground)
            .exempting(&[goal_cell]);
        let view = self.footprint_view(tick, ids, entity, mover, &costs, &ground);
        crate::perf::note_plan_call();
        let planning_error = |e| mint_sim::Error::Rules {
            frame: tick.frame().get(),
            reason: format!(
                "re-planning a blocked path for entity {}: {e}",
                entity.index()
            ),
        };
        let outcome = if exact_goal.is_some() {
            self.plan_path(modes.config(), &view, &request, budget)
                .map_err(planning_error)?
        } else {
            let mut whole = NodeBudget::new(PATH_NODE_BUDGET);
            Pipeline::new(modes.config())
                .plan_closest(&view, self.passability.geometry, &request, &mut whole)
                .map_err(planning_error)?
        };
        match outcome {
            PlanOutcome::Ready(plan) => {
                Self::clear_path_retry(tick, ids, entity);
                let mut state = body.0;
                state.waypoint = 0;
                tick.state_mut().insert(ids.body, entity, Body(state))?;
                tick.state_mut().insert(
                    ids.path,
                    entity,
                    Path {
                        route: plan.route,
                        goal: path.goal,
                        kind: path.kind,
                        facing: path.facing,
                    },
                )?;
            }
            PlanOutcome::Closest(plan) => {
                Self::clear_path_retry(tick, ids, entity);
                let mut state = body.0;
                state.waypoint = 0;
                tick.state_mut().insert(ids.body, entity, Body(state))?;
                let goal = plan.route.points.last().copied().unwrap_or(path.goal);
                tick.state_mut().insert(
                    ids.path,
                    entity,
                    Path {
                        route: plan.route,
                        goal,
                        kind: path.kind,
                        facing: path.facing,
                    },
                )?;
            }
            PlanOutcome::BudgetExhausted => {
                if is_head {
                    self.note_path_failure(tick, ids, entity)?;
                } else {
                    self.enqueue_path_request(tick, ids, entity)?;
                }
            }
            PlanOutcome::Unreachable => self.note_path_failure(tick, ids, entity)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn pop_path_queue_head(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        clock_entity: Option<Entity>,
    ) -> Result<Option<Entity>> {
        let Some(clock_entity) = clock_entity else {
            return Ok(None);
        };
        let Some(mut queue) = tick.state().get(ids.path_queue, clock_entity).cloned() else {
            return Ok(None);
        };
        let head = queue.pop_front();
        tick.state_mut()
            .insert(ids.path_queue, clock_entity, queue)?;
        Ok(head)
    }

    pub(super) fn deferred_path_requests(tick: &Tick<'_>, ids: Ids, now: Frame) -> Vec<Entity> {
        let Some(store) = tick.state().store(ids.path_retry) else {
            return Vec::new();
        };
        let mut deferred: Vec<Entity> = store
            .iter()
            .filter(|(_, retry)| retry.next_attempt > now)
            .map(|(entity, _)| entity)
            .collect();
        deferred.sort_by_key(|entity: &Entity| entity.index());
        deferred
    }

    pub(super) fn settle_path_requests(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        budgets: &PathBudgets,
    ) -> Result<()> {
        let now = tick.frame();
        for &entity in &budgets.attempted {
            let failures = tick
                .state()
                .get(ids.path_retry, entity)
                .map_or(0, |retry| retry.failures);
            let next = Self::frame_after(now, MIN_REPATH_FRAMES);
            let held = tick
                .state()
                .get(ids.path_retry, entity)
                .is_some_and(|retry| retry.next_attempt >= next);
            if !held {
                tick.state_mut().insert(
                    ids.path_retry,
                    entity,
                    PathRetry {
                        next_attempt: next,
                        failures,
                    },
                )?;
            }
        }
        for &entity in &budgets.starved {
            self.enqueue_path_request(tick, ids, entity)?;
        }
        Ok(())
    }

    pub(super) fn frame_after(now: Frame, frames: u32) -> Frame {
        now.checked_add(LogicFrames::from_frames(frames))
            .unwrap_or(Frame::FOREVER)
    }

    pub(super) fn note_path_failure(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let now = tick.frame();
        let failures = tick
            .state()
            .get(ids.path_retry, entity)
            .map_or(0, |retry| retry.failures)
            .saturating_add(1);
        tick.state_mut().insert(
            ids.path_retry,
            entity,
            PathRetry {
                next_attempt: Self::frame_after(now, PATH_RETRY_DELAY_FRAMES),
                failures,
            },
        )?;
        Ok(())
    }

    pub(super) fn clear_path_retry(tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        tick.state_mut().remove(ids.path_retry, entity);
    }

    pub(super) fn enqueue_path_request(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(clock_entity) = tick
            .state()
            .store(ids.path_queue)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let Some(mut queue) = tick.state().get(ids.path_queue, clock_entity).cloned() else {
            return Ok(());
        };
        queue.push(entity);
        tick.state_mut()
            .insert(ids.path_queue, clock_entity, queue)?;
        Ok(())
    }

    pub(super) fn advance_members(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let modes = self.modes(tick, ids);
        if modes.formation != FormationMode::On {
            return Ok(());
        }
        self.park_members(tick, ids)?;
        let mut held_elsewhere: BTreeSet<Entity> = tick
            .state()
            .store(ids.knocked)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, knocked)| knocked.off_its_feet())
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(store) = tick.state().store(ids.contained) {
            held_elsewhere.extend(store.entities().iter().copied());
        }
        held_elsewhere.extend(Self::backing_members(tick.state(), ids));
        let horde_of = |member: &Entity| tick.state().get(ids.member, *member).map(|m| m.horde);
        let mut touched: BTreeSet<Entity> = tick
            .state()
            .store(ids.member_attack)
            .map(|store| store.entities().iter().filter_map(horde_of).collect())
            .unwrap_or_default();
        touched.extend(held_elsewhere.iter().filter_map(horde_of));
        if let Some(store) = tick.state().store(ids.path) {
            touched.extend(store.entities().iter().filter_map(horde_of));
        }
        let attacking: BTreeSet<Entity> = tick
            .state()
            .store(ids.armed)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, armed)| {
                        tick.state().contains(ids.member, *entity)
                            && armed.target.is_some_and(|target| {
                                tick.state()
                                    .get(ids.health, target)
                                    .is_some_and(|health| !health.0.is_dead())
                            })
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        touched.extend(attacking.iter().filter_map(horde_of));
        let reachability = self.reachability.borrow();
        let reachability_extra = self.reachability_extra.borrow();
        let (stragglers, porcupine_arrivals, oath_effects) = crate::horde::advance_members(
            tick,
            Self::horde_ids(ids),
            ids.obstacles,
            &self.passability.grid,
            self.passability.geometry,
            Self::tick_seconds(),
            &crate::horde::PassInputs {
                held_elsewhere: &held_elsewhere,
                touched: &touched,
                attacking: &attacking,
                kinds: &self.kinds,
                formations: &self.formations,
                extent: self.passability.grid.extent(),
                heights: &self.passability.heights,
                reachability: &reachability,
                reachability_extra: &reachability_extra,
                model_conditions: self.modules.as_ref().map(|wiring| wiring.state),
                restriction: self.restriction(tick.state(), ids),
            },
        )?;
        let mut effects = porcupine_arrivals
            .into_iter()
            .filter(|member| !self.holds_model_condition(tick, *member, PORCUPINE))
            .map(|member| {
                (
                    member,
                    mint_sage_modules::Effect::ModelConditions {
                        add: mint_sage_modules::effects::names(vec![PORCUPINE.to_owned()]),
                        remove: mint_sage_modules::effects::names(Vec::new()),
                    },
                )
            })
            .collect::<Vec<_>>();
        effects.extend(oath_effects);
        effects.extend(crate::horde::advance_wall_climbers(
            tick,
            Self::horde_ids(ids),
            &self.passability.heights,
            &self.kinds,
            self.modules.as_ref().map(|wiring| wiring.state),
        )?);
        if !effects.is_empty() {
            self.apply_effects(tick, ids, effects)?;
        }
        self.rescue_stragglers(tick, ids, modes.config(), stragglers)
    }

    fn park_members(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let moving: Vec<Entity> = tick
            .state()
            .store(ids.horde)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|horde| tick.state().contains(ids.path, *horde))
                    .filter(|horde| {
                        tick.state()
                            .get(ids.horde_melee, *horde)
                            .is_none_or(|melee| melee.victim.is_none())
                    })
                    .collect()
            })
            .unwrap_or_default();
        for horde_entity in moving {
            let Some(horde) = tick.state().get(ids.horde, horde_entity).cloned() else {
                continue;
            };
            let Some(body) = tick.state().get(ids.body, horde_entity).copied() else {
                continue;
            };
            let leash = self
                .formations
                .get(usize::from(horde.formation))
                .map_or(0.0, |table| table.settings().melee_attack_leash_distance);
            let poses = crate::horde::slot_poses(&horde, &body.0);
            for member in horde.members.iter().copied() {
                let Some(entry) = tick.state().get(ids.member, member).copied() else {
                    continue;
                };
                if entry.parked {
                    continue;
                }
                let Some(position) = tick.state().get(ids.body, member).map(|b| b.0.position)
                else {
                    continue;
                };
                let to_slot = entry
                    .slot
                    .and_then(|slot| poses.get(usize::from(slot)))
                    .map_or(f32::INFINITY, |pose| position.minus(*pose).length());
                let gap = position.minus(body.0.position).length().min(to_slot);
                if gap <= leash {
                    continue;
                }
                Self::park(tick, ids, member, entry)?;
            }
        }
        Ok(())
    }

    pub(super) fn park(
        tick: &mut Tick<'_>,
        ids: Ids,
        member: Entity,
        entry: crate::horde::Member,
    ) -> Result<()> {
        tick.state_mut().remove(ids.attack_order, member);
        tick.state_mut().remove(ids.member_attack, member);
        let own_route = tick
            .state()
            .get(ids.path, member)
            .is_some_and(|path| !matches!(path.kind, PathKind::Formation | PathKind::Burning));
        if own_route {
            tick.state_mut().remove(ids.path, member);
        }
        if let Some(mut armed) = tick.state().get(ids.armed, member).copied() {
            if armed.target.is_some() {
                armed.retarget(None);
                tick.state_mut().insert(ids.armed, member, armed)?;
            }
        }
        tick.state_mut().insert(
            ids.member,
            member,
            crate::horde::Member {
                parked: true,
                ..entry
            },
        )?;
        Ok(())
    }

    pub(super) fn rescue_stragglers(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        config: PipelineConfig,
        mut stragglers: Vec<crate::horde::Straggler>,
    ) -> Result<()> {
        crate::perf::note_stragglers(u32::try_from(stragglers.len()).unwrap_or(u32::MAX));
        if stragglers.is_empty() {
            record_straggler_rescue_spent(0);
            return Ok(());
        }
        stragglers.sort_by_key(|straggler| straggler.member.index());
        let len = u64::try_from(stragglers.len()).unwrap_or(1).max(1);
        let rotate = usize::try_from(tick.frame().get() % len).unwrap_or(0);
        stragglers.rotate_left(rotate);

        let mut budget = NodeBudget::new(STRAGGLER_RESCUE_BUDGET_PER_TICK);
        let mut head_budget = NodeBudget::new(PATH_NODE_BUDGET);
        let now = tick.frame();
        let deferred = Self::deferred_path_requests(tick, ids, now);
        let mut rescued = Vec::new();
        let mut failed: Vec<Entity> = Vec::new();
        let mut placed: Vec<(Entity, Vec2)> = Vec::new();
        let mut first = true;
        {
            let view = self.dynamic_view(tick, ids);
            for straggler in stragglers {
                if deferred
                    .binary_search_by_key(&straggler.member.index(), |waiting| waiting.index())
                    .is_ok()
                {
                    continue;
                }
                let budget = if std::mem::take(&mut first) {
                    &mut head_budget
                } else {
                    &mut budget
                };
                if budget.remaining() == 0 {
                    break;
                }
                let Some(body) = tick.state().get(ids.body, straggler.member).copied() else {
                    continue;
                };
                let mover = self.mover_class(tick, ids, straggler.member);
                let (Some(start), Some(goal_cell)) = (
                    self.passable_cell_for(tick, ids, body.0.position, mover),
                    self.passable_cell_for(tick, ids, straggler.rescue_target, mover),
                ) else {
                    continue;
                };
                if !self.reachable(mover, start, goal_cell) {
                    if straggler.place_on_failure {
                        placed.push((straggler.member, straggler.rescue_target));
                    } else {
                        failed.push(straggler.member);
                    }
                    continue;
                }
                let request = PathRequest {
                    start,
                    goal: goal_cell,
                    mover: mover.without_crossing_layers(),
                    goal_id: GoalId(u64::from(straggler.member.index())),
                    origin: Some(body.0.position),
                    corner_radius: self.mover_limits(tick, ids, straggler.member).corner_radius,
                    departure: self.departure(tick, ids, straggler.member),
                };
                let outcome = self
                    .plan_path(config, &view, &request, budget)
                    .map_err(|e| mint_sim::Error::Rules {
                        frame: tick.frame().get(),
                        reason: format!(
                            "rescuing a straggling horde member {}: {e}",
                            straggler.member.index()
                        ),
                    })?;
                match outcome {
                    PlanOutcome::Ready(plan) => rescued.push((straggler, body, plan.route)),
                    PlanOutcome::Unreachable if straggler.place_on_failure => {
                        placed.push((straggler.member, straggler.rescue_target));
                    }
                    _ => failed.push(straggler.member),
                }
            }
        }
        record_straggler_rescue_spent(budget.spent().saturating_add(head_budget.spent()));
        for member in failed {
            self.note_path_failure(tick, ids, member)?;
        }
        for (member, goal) in placed {
            if let Some(mut body) = tick.state().get(ids.body, member).copied() {
                body.0.position = goal;
                tick.state_mut().insert(ids.body, member, body)?;
            }
        }
        for (straggler, body, route) in rescued {
            Self::clear_path_retry(tick, ids, straggler.member);
            let mut state = body.0;
            state.waypoint = 0;
            state.speed = straggler.speed;
            tick.state_mut()
                .insert(ids.body, straggler.member, Body(state))?;
            tick.state_mut().insert(
                ids.path,
                straggler.member,
                Path {
                    route,
                    goal: straggler.rescue_target,
                    kind: PathKind::Formation,
                    facing: None,
                },
            )?;
        }
        Ok(())
    }
}

const HORDE_THROW_SHARE: f32 = 0.3;

const MELEE_HORDE_APPROACH_EXTRA: f32 = 100.0;

const HORSE_CREEP_BELOW: f32 = 0.25;

const HORSE_CREEP_SHARE: f32 = 0.1;

const REFORM_MIN_PATH_LEFT: f32 = 20.0;
