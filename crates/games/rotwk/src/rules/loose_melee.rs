use super::melee_engage::{Engage, ENGAGE_WAIT_MS};
use super::*;

const SETTLE_RETRY_MS: u32 = 2000;

const REPLAN_TICKS: u64 = mint_sim::SIMULATION_HZ as u64;

const CUT_OFF_LEAD: f32 = 60.0;

const RUN_DOWN_REACH: f32 = 10.0;

const SQUISH_OVERRUN: f32 = 50.0;

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
}

impl LooseMelee {
    pub(super) const fn victim(&self) -> Entity {
        self.victim
    }

    pub(super) fn squishing(&self) -> bool {
        self.phase == Phase::Squish
    }

    pub(super) fn melee_attacking(&self, standing: bool) -> bool {
        self.phase == Phase::Squish || (standing && self.waits_until.is_none())
    }

    const fn on(victim: Entity) -> Self {
        Self {
            victim,
            phase: Phase::Approach,
            spot: None,
            planned: None,
            waits_until: None,
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LooseStep {
    Fight,
    Walk,
    Strike,
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
        if let Some(flight) = self.victim_flight(tick, ids, me, victim) {
            let step = self.run_down(tick, ids, entity, me, victim, flight, &mut plan, budgets)?;
            if before != Some(plan) {
                tick.state_mut().insert(ids.loose_melee, entity, plan)?;
            }
            return Ok(step);
        }
        let in_reach = self.melee_in_reach(tick, ids, me, victim, range);
        let settles = in_reach
            && match plan.phase {
                Phase::Approach => {
                    !walking || self.melee_stand_free(tick, ids, entity, victim.entity)
                }
                Phase::Engage | Phase::Squish => {
                    !walking && self.melee_stand_free(tick, ids, entity, victim.entity)
                }
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
            self.plan_loose_melee(tick, ids, entity, me, victim, range, &mut plan, walking)?;
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
            } else if walking {
                tick.state_mut().remove(ids.path, entity);
            }
            LooseStep::Walk
        };
        if before != Some(plan) {
            tick.state_mut().insert(ids.loose_melee, entity, plan)?;
        }
        Ok(step)
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
    ) -> Result<()> {
        let now = tick.frame();
        let placement = self.melee_placement;
        let engage_within = placement.approach_dist + placement.approach_tolerance;
        let near = victim.position.minus(me.position).length() <= engage_within;
        let arrived = !walking
            && plan.spot.is_some_and(|spot| {
                spot.minus(me.position).length() <= self.passability.geometry.cell_size()
            });
        match plan.phase {
            Phase::Approach if near || arrived => {
                plan.phase = Phase::Engage;
                plan.spot = None;
                plan.planned = None;
            }
            Phase::Engage if arrived => {
                plan.spot = None;
                plan.planned = None;
                plan.waits_until = Some(Self::after(now, SETTLE_RETRY_MS));
                return Ok(());
            }
            _ => {}
        }
        let due = plan.spot.is_none()
            || plan.planned.is_none_or(|(seen, when)| {
                now.get().saturating_sub(when.get()) >= REPLAN_TICKS
                    && (!walking || super::movement::goal_moved(me.position, seen, victim.position))
            });
        if !due {
            return Ok(());
        }
        plan.planned = Some((victim.position, now));
        if plan.phase == Phase::Approach {
            match self.melee_approach_spot(tick, ids, entity, victim.entity) {
                Some(spot) => {
                    plan.spot = Some(spot);
                    self.claim_spot(tick, ids, entity, spot);
                    return Ok(());
                }
                None => plan.phase = Phase::Engage,
            }
        }
        let keep = walking
            && plan.spot.is_some_and(|spot| {
                let there = Foe {
                    position: spot,
                    ..*me
                };
                self.melee_in_reach(tick, ids, &there, victim, range)
            });
        if keep {
            return Ok(());
        }
        match self.melee_engage_spot(tick, ids, entity, victim.entity, range) {
            Engage::At(spot) => {
                plan.spot = Some(spot);
                self.claim_spot(tick, ids, entity, spot);
            }
            Engage::Wait => {
                plan.spot = None;
                plan.waits_until = Some(Self::after(now, ENGAGE_WAIT_MS));
            }
        }
        Ok(())
    }

    fn crush_run_open(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
    ) -> bool {
        if !self
            .record_of(tick, ids, entity)
            .is_some_and(|record| record.crush.use_crush_attack && record.crush.crusher_level > 0)
        {
            return false;
        }
        let owner = |of: Entity| tick.state().get(ids.owner, of).map(|owner| owner.0);
        let (Some(mine), Some(theirs)) = (owner(entity), owner(victim.entity)) else {
            return false;
        };
        if self.object_allied(tick.state(), entity, victim.entity) {
            return false;
        }
        let states = self.crush_states(tick, ids, &[(*me, mine), (*victim, theirs)]);
        let (Some(crusher), Some(crushed)) = (states.first(), states.get(1)) else {
            return false;
        };
        crusher.crusher_level > 0
            && !crusher.too_slow
            && i16::from(crusher.crusher_level) > crushed.crushable_level
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
        let ahead = victim
            .position
            .minus(me.position)
            .normalised()
            .unwrap_or(Vec2::ZERO);
        let spot = victim.position.plus(ahead.scale(SQUISH_OVERRUN));
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

    fn victim_flight(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        me: &Foe,
        victim: &Foe,
    ) -> Option<(Vec2, f32)> {
        if !self.fleeing(tick, ids, me.position, victim.entity) {
            return None;
        }
        let body = tick.state().get(ids.body, victim.entity)?.0;
        Some((body.facing.to_vector(), body.speed))
    }

    #[allow(clippy::too_many_arguments)]
    fn run_down(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        me: &Foe,
        victim: &Foe,
        (heading, speed): (Vec2, f32),
        plan: &mut LooseMelee,
        budgets: &mut PathBudgets,
    ) -> Result<LooseStep> {
        #[allow(clippy::cast_precision_loss)]
        let frame = 1.0 / crate::game_data::RETAIL_LOGIC_HZ as f32;
        let mine = tick
            .state()
            .get(ids.body, entity)
            .map_or(0.0, |b| b.0.speed);
        let centre = victim.position.minus(me.position).length();
        let gap = (centre - (me.radius + RUN_DOWN_REACH) - victim.radius + speed * frame).max(0.0);
        let border = (centre - me.radius - victim.radius).max(0.0);
        if gap < mine * frame && border <= self.melee_placement.chase_from_behind {
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

    pub(super) fn end_loose_attack(&self, tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        tick.state_mut().remove(ids.loose_melee, entity);
        let held = crate::goal_cells::layer(tick.state(), ids.goal_cells)
            .is_some_and(|layer| layer.held(entity).is_some());
        if held {
            if let Some(layer) = crate::goal_cells::layer_mut(tick, ids.goal_cells) {
                layer.release(entity);
            }
        }
    }
}
