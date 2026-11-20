use super::melee_engage::{Engage, ENGAGE_WAIT_MS};
use super::*;

const SETTLE_RETRY_MS: u32 = 2000;

const REPLAN_TICKS: u64 = mint_sim::SIMULATION_HZ as u64;

const CUT_OFF_LEAD: f32 = 60.0;

pub(super) const RUN_DOWN_REACH: f32 = 10.0;

const APPROACH_SPOT_MIN: f32 = 20.0;

pub(super) fn squish_goal(from: (Vec2, f32), to: (Vec2, f32), radius: f32) -> Vec2 {
    let flat = to.0.minus(from.0);
    let dz = to.1 - from.1;
    let length = (flat.length_squared() + dz * dz).sqrt();
    if length <= 0.0 {
        return to.0;
    }
    to.0.plus(flat.scale(2.0 * radius / length))
}

const ARMY_OF_DEAD: &str = "ARMY_OF_DEAD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Approach,
    Engage,
    Squish,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LooseMelee {
    victim: Entity,
    phase: Phase,
    spot: Option<Vec2>,
    planned: Option<(Vec2, Frame)>,
    waits_until: Option<Frame>,
    routeless: bool,
    fast: bool,
}

impl LooseMelee {
    pub(super) const fn victim(&self) -> Entity {
        self.victim
    }

    pub(super) fn squishing(&self) -> bool {
        self.phase == Phase::Squish
    }

    pub(super) fn melee_attacking(&self, standing: bool) -> bool {
        (self.phase == Phase::Squish && self.fast) || (standing && self.waits_until.is_none())
    }

    const fn on(victim: Entity) -> Self {
        Self {
            victim,
            phase: Phase::Approach,
            spot: None,
            planned: None,
            waits_until: None,
            routeless: false,
            fast: false,
        }
    }
}

impl Component for LooseMelee {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.victim.hash_into(hasher);
        hasher.write_u8(match self.phase {
            Phase::Approach => 0,
            Phase::Engage => 1,
            Phase::Squish => 2,
        });
        match self.spot {
            Some(spot) => {
                hasher.write_u8(1);
                hasher.write_f32(spot.x);
                hasher.write_f32(spot.y);
            }
            None => hasher.write_u8(0),
        }
        match self.planned {
            Some((seen, when)) => {
                hasher.write_u8(1);
                hasher.write_f32(seen.x);
                hasher.write_f32(seen.y);
                when.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        match self.waits_until {
            Some(until) => {
                hasher.write_u8(1);
                until.hash_into(hasher);
            }
            None => hasher.write_u8(0),
        }
        hasher.write_bool(self.routeless);
        hasher.write_bool(self.fast);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ApproachPlan {
    Walk(Vec2),
    Engage,
    Fail,
}

fn approach_plan(spot: Option<Vec2>, at: Vec2, entering: bool) -> ApproachPlan {
    match spot {
        Some(spot) if spot.minus(at).length() >= APPROACH_SPOT_MIN => ApproachPlan::Walk(spot),
        _ if entering => ApproachPlan::Engage,
        _ => ApproachPlan::Fail,
    }
}

fn engage_settles(walking: bool, stand_free: bool, routeless_in_reach: bool) -> bool {
    !walking && (stand_free || routeless_in_reach)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LooseStep {
    Fight,
    Walk,
    Strike,
    Fail,
}

impl RotwkRules {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn loose_melee_step(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        range: f32,
        budgets: &mut PathBudgets,
    ) -> Result<LooseStep> {
        let before = tick.state().get(ids.loose_melee, entity).copied();
        let mut plan = before
            .filter(|plan| plan.victim == victim.entity)
            .unwrap_or(LooseMelee::on(victim.entity));
        let walking = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing);
        let now = tick.frame();
        if let Some(until) = plan.waits_until {
            if now < until {
                return Ok(LooseStep::Walk);
            }
            plan = LooseMelee::on(victim.entity);
        }
        let squishing = match plan.phase {
            Phase::Squish => true,
            Phase::Approach => self.crush_run_open(tick, ids, entity, me, victim),
            Phase::Engage => false,
        };
        if squishing {
            self.squish(tick, ids, entity, me, victim, &mut plan, budgets)?;
            if before != Some(plan) {
                tick.state_mut().insert(ids.loose_melee, entity, plan)?;
            }
            return Ok(LooseStep::Walk);
        }
        let flight = self.victim_flight(tick, ids, me, victim);
        if flight.is_some() && plan.phase == Phase::Approach && plan.planned.is_none() {
            plan.phase = Phase::Engage;
        }
        match (flight, plan.phase) {
            (Some(flight), Phase::Engage) => {
                let step =
                    self.run_down(tick, ids, entity, me, victim, flight, &mut plan, budgets)?;
                if before != Some(plan) {
                    tick.state_mut().insert(ids.loose_melee, entity, plan)?;
                }
                return Ok(step);
            }
            (Some(_), _) => {
                let in_reach = self.melee_in_reach(tick, ids, me, victim);
                if in_reach {
                    plan.phase = Phase::Engage;
                    plan.spot = None;
                    plan.planned = None;
                } else if Self::replan_due(tick, me, victim, &plan, walking) {
                    return Ok(LooseStep::Fail);
                }
                if before != Some(plan) {
                    tick.state_mut().insert(ids.loose_melee, entity, plan)?;
                }
                return Ok(LooseStep::Walk);
            }
            (None, _) => self.end_run_down(tick, ids, entity)?,
        }
        let in_reach = self.melee_in_reach(tick, ids, me, victim);
        let settles = in_reach
            && match plan.phase {
                Phase::Approach => {
                    !walking || self.melee_stand_free(tick, ids, entity, victim.entity)
                }
                Phase::Engage | Phase::Squish => engage_settles(
                    walking,
                    self.melee_stand_free(tick, ids, entity, victim.entity),
                    plan.routeless
                        && plan.spot.is_some_and(|spot| {
                            let there = Foe {
                                position: spot,
                                ..*me
                            };
                            self.melee_in_reach(tick, ids, &there, victim)
                        }),
                ),
            };
        let step = if settles {
            self.claim_spot(tick, ids, entity, me.position);
            plan = LooseMelee::on(victim.entity);
            LooseStep::Fight
        } else if in_reach {
            if !walking {
                plan.spot = None;
                plan.planned = None;
                plan.waits_until = Some(Self::after(now, SETTLE_RETRY_MS));
            }
            LooseStep::Walk
        } else {
            if self.plan_loose_melee(tick, ids, entity, me, victim, range, &mut plan, walking)? {
                return Ok(LooseStep::Fail);
            }
            if let Some(spot) = plan.spot {
                self.close_on_target(
                    tick,
                    ids,
                    entity,
                    me.position,
                    spot,
                    0.0,
                    PathKind::Closing,
                    budgets,
                )?;
                plan.routeless = !tick.state().contains(ids.path, entity)
                    && tick
                        .state()
                        .get(ids.path_retry, entity)
                        .is_some_and(|retry| retry.failures > 0);
            } else {
                plan.routeless = false;
                if walking {
                    tick.state_mut().remove(ids.path, entity);
                }
            }
            LooseStep::Walk
        };
        if before != Some(plan) {
            tick.state_mut().insert(ids.loose_melee, entity, plan)?;
        }
        Ok(step)
    }

    fn replan_due(
        tick: &Tick<'_>,
        me: &Foe,
        victim: &Foe,
        plan: &LooseMelee,
        walking: bool,
    ) -> bool {
        let now = tick.frame();
        plan.spot.is_none()
            || plan.planned.is_none_or(|(seen, when)| {
                now.get().saturating_sub(when.get()) >= REPLAN_TICKS
                    && (!walking || super::movement::goal_moved(me.position, seen, victim.position))
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_loose_melee(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        range: f32,
        plan: &mut LooseMelee,
        walking: bool,
    ) -> Result<bool> {
        let now = tick.frame();
        let placement = self.melee_placement;
        let engage_within = placement.approach_dist + placement.approach_tolerance;
        let near = victim.position.minus(me.position).length() <= engage_within;
        let arrived = !walking
            && plan.spot.is_some_and(|spot| {
                spot.minus(me.position).length() <= self.passability.geometry.cell_size()
            });
        let blocked = tick
            .state()
            .get(ids.contact, entity)
            .is_some_and(|contact| contact.blocked());
        let fresh = plan.planned.is_none();
        match plan.phase {
            Phase::Approach if (!walking && (near || arrived)) || (fresh && blocked) => {
                plan.phase = Phase::Engage;
                plan.spot = None;
                plan.planned = None;
            }
            Phase::Engage if arrived => {
                plan.spot = None;
                plan.planned = None;
                plan.waits_until = Some(Self::after(now, SETTLE_RETRY_MS));
                return Ok(false);
            }
            _ => {}
        }
        if !Self::replan_due(tick, me, victim, plan, walking) {
            return Ok(false);
        }
        if plan.phase == Phase::Approach && (blocked || (walking && near)) {
            return Ok(true);
        }
        plan.planned = Some((victim.position, now));
        if plan.phase == Phase::Approach {
            let spot = self.melee_approach_spot(tick, ids, entity, victim.entity);
            match approach_plan(spot, me.position, fresh) {
                ApproachPlan::Walk(spot) => {
                    plan.spot = Some(spot);
                    self.claim_spot(tick, ids, entity, spot);
                    return Ok(false);
                }
                ApproachPlan::Engage => plan.phase = Phase::Engage,
                ApproachPlan::Fail => return Ok(true),
            }
        }
        let keep = walking
            && plan.spot.is_some_and(|spot| {
                let there = Foe {
                    position: spot,
                    ..*me
                };
                self.melee_in_reach(tick, ids, &there, victim)
            });
        if keep {
            return Ok(false);
        }
        match self.melee_engage_spot(tick, ids, entity, victim.entity, range) {
            Engage::At(spot) => {
                plan.spot = Some(spot);
                self.claim_spot(tick, ids, entity, spot);
            }
            Engage::Wait => {
                let walk = self.engage_fallback_point(tick, ids, me, victim);
                let spot = walk.and_then(|point| {
                    self.adjust_destination(
                        tick,
                        ids,
                        super::adjust_destination::Request {
                            mover: entity,
                            dest: point,
                            group_goal: None,
                        },
                        &mut super::adjust_destination::OrderClaims::none(),
                    )
                });
                if let Some(spot) = spot {
                    plan.spot = Some(spot);
                    self.claim_spot(tick, ids, entity, spot);
                } else {
                    plan.spot = None;
                    plan.waits_until = Some(Self::after(now, ENGAGE_WAIT_MS));
                }
            }
        }
        Ok(false)
    }

    fn crush_run_open(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        !self.holds_ground(tick.state(), ids, entity)
            && self.squish_gate(tick, ids, entity, me.radius, victim)
            && !self.crush_state(tick, ids, entity, me.radius).too_slow
    }

    pub(super) fn squish_gate(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        radius: f32,
        victim: &Foe,
    ) -> bool {
        if !self
            .record_of(tick, ids, entity)
            .is_some_and(|record| record.crush.use_crush_attack)
        {
            return false;
        }
        if !self.object_hostile(tick.state(), entity, victim.entity) {
            return false;
        }
        let crusher = self.crush_state(tick, ids, entity, radius);
        let crushed = self.crush_state(tick, ids, victim.entity, victim.radius);
        crusher.crusher_level > 0
            && i16::from(crusher.crusher_level) > crushed.crushable_level
            && (self.is_rampaging(tick.state(), entity)
                || self.carries_object_status(tick.state(), entity, "FLEE_OFF_MAP")
                || self.charging_with_horde(tick, ids, entity)
                || self.has_melee_weapon(tick, ids, entity))
    }

    #[allow(clippy::too_many_arguments)]
    fn squish(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        plan: &mut LooseMelee,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        let walking = tick
            .state()
            .get(ids.path, entity)
            .is_some_and(|path| path.kind == PathKind::Closing);
        let entering = plan.phase != Phase::Squish;
        if !entering && !self.crush_run_open(tick, ids, entity, me, victim) {
            plan.phase = Phase::Engage;
            plan.spot = None;
            plan.planned = None;
            return Ok(());
        }
        let ended = !entering && !walking;
        if ended && !self.has_kind_bit(tick.state(), ids, entity, ARMY_OF_DEAD) {
            plan.phase = Phase::Engage;
            plan.spot = None;
            plan.planned = None;
            return Ok(());
        }
        if entering {
            crate::diag::diag!(
                "melee: frame {} entity {} crush-runs at {}",
                tick.frame().get(),
                entity.index(),
                victim.entity.index()
            );
        }
        plan.phase = Phase::Squish;
        plan.fast = !self.crush_state(tick, ids, entity, me.radius).too_slow;
        let spot = squish_goal((me.position, me.z), (victim.position, victim.z), me.radius);
        plan.spot = Some(spot);
        plan.planned = Some((victim.position, tick.frame()));
        self.close_on_target(
            tick,
            ids,
            entity,
            me.position,
            spot,
            0.0,
            PathKind::Closing,
            budgets,
        )
    }

    fn victim_flight(&self, tick: &Tick<'_>, ids: Ids, me: &Foe, victim: &Foe) -> Option<Vec2> {
        if !self.fleeing(tick, ids, me.position, victim.entity) {
            return None;
        }
        let body = tick.state().get(ids.body, victim.entity)?.0;
        Some(body.facing.to_vector())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_down(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        heading: Vec2,
        plan: &mut LooseMelee,
        budgets: &mut PathBudgets,
    ) -> Result<LooseStep> {
        self.update_running_down(tick, ids, entity, victim.entity)?;
        let held = tick.state().contains(ids.running_down, entity);
        let striker = super::melee_contact::Striker {
            entity,
            at: me.position,
            footprint: self.footprint_of(tick, ids, entity),
        };
        if self.melee_in_range(tick, ids, striker, victim.entity, held) {
            return Ok(LooseStep::Strike);
        }
        let now = tick.frame();
        let due = plan.planned.is_none_or(|(seen, when)| {
            now.get().saturating_sub(when.get()) >= REPLAN_TICKS
                && super::movement::goal_moved(me.position, seen, victim.position)
        });
        if due {
            plan.planned = Some((victim.position, now));
            plan.spot = Some(
                victim
                    .position
                    .plus(heading.scale(2.0 * victim.radius + CUT_OFF_LEAD)),
            );
        }
        if let Some(spot) = plan.spot {
            self.close_on_target(
                tick,
                ids,
                entity,
                me.position,
                spot,
                0.0,
                PathKind::Closing,
                budgets,
            )?;
        }
        Ok(LooseStep::Walk)
    }

    fn end_run_down(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) -> Result<()> {
        if tick.state().contains(ids.running_down, entity) {
            self.write_running_down(tick, ids, entity, false)?;
        }
        tick.state_mut().remove(ids.desired_speed, entity);
        Ok(())
    }

    pub(super) fn end_loose_attack(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        tick.state_mut().remove(ids.loose_melee, entity);
        tick.state_mut().remove(ids.running_down, entity);
        tick.state_mut().remove(ids.desired_speed, entity);
        let held = crate::goal_cells::layer(tick.state(), ids.goal_cells)
            .is_some_and(|layer| layer.held(entity).is_some());
        if held {
            if let Some(layer) = crate::goal_cells::layer_mut(tick, ids.goal_cells) {
                layer.release(entity);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approach_spot_under_twenty_away_ends_a_replanned_approach() {
        let at = Vec2::new(0.0, 0.0);
        let near = Some(Vec2::new(15.0, 0.0));
        let far = Some(Vec2::new(25.0, 0.0));
        assert_eq!(approach_plan(near, at, false), ApproachPlan::Fail);
        assert_eq!(approach_plan(None, at, false), ApproachPlan::Fail);
        assert_eq!(approach_plan(near, at, true), ApproachPlan::Engage);
        assert_eq!(
            approach_plan(far, at, false),
            ApproachPlan::Walk(Vec2::new(25.0, 0.0))
        );
    }

    #[test]
    fn a_routeless_engage_in_reach_settles_on_an_occupied_cell() {
        assert!(engage_settles(false, false, true));
        assert!(!engage_settles(false, false, false));
        assert!(!engage_settles(true, true, true));
        assert!(engage_settles(false, true, false));
    }

    #[test]
    fn a_crush_run_aims_two_radii_past_its_victim() {
        let flat = squish_goal(
            (Vec2::new(0.0, 0.0), 0.0),
            (Vec2::new(100.0, 0.0), 0.0),
            10.0,
        );
        assert!(
            (flat.x - 120.0).abs() < 1e-3 && flat.y.abs() < 1e-3,
            "{flat:?}"
        );
        let rising = squish_goal(
            (Vec2::new(0.0, 0.0), 0.0),
            (Vec2::new(30.0, 0.0), 40.0),
            10.0,
        );
        assert!((rising.x - 42.0).abs() < 1e-3, "{rising:?}");
    }
}
