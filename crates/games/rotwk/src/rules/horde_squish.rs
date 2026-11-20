use super::*;
use crate::melee::{HordeMelee, MeleePhase};

const SQUISH_REPLAN_TICKS: u64 =
    5 * (mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ) as u64;

const ENGAGED_GAP: f32 = 100.0;

const ARMY_OF_DEAD: &str = "ARMY_OF_DEAD";

impl RotwkRules {
    pub(super) fn on_crush_run(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.loose_melee, entity)
            .is_some_and(|plan| plan.squishing())
            || world
                .get(ids.horde_melee, entity)
                .is_some_and(|melee| matches!(melee.phase, MeleePhase::Squish { .. }))
    }

    pub(super) fn horde_squish_opens(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
    ) -> bool {
        if self.holds_ground(tick.state(), ids, horde_entity) {
            return false;
        }
        let Some(target) = self.goal_members(tick, ids, goal).first().copied() else {
            return false;
        };
        !tick.state().contains(ids.knocked, target.entity)
            && self.squish_gate(
                tick,
                ids,
                horde_entity,
                self.object_radius(tick, ids, horde_entity),
                &target,
            )
    }

    fn horde_engaged_with(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        goal: Entity,
        living: &[(Foe, PlayerId)],
    ) -> Result<bool> {
        let Some(members) = tick
            .state()
            .get(ids.horde, horde_entity)
            .map(|horde| horde.members.clone())
        else {
            return Ok(false);
        };
        let place = |entity| tick.state().get(ids.body, entity).map(|b| b.0.position);
        let (Some(here), Some(there)) = (place(horde_entity), place(goal)) else {
            return Ok(false);
        };
        let theirs: Vec<Entity> = tick
            .state()
            .get(ids.horde, goal)
            .map_or_else(|| vec![goal], |horde| horde.members.clone());
        let attacking = |state: &World, member: Entity| {
            state
                .get(ids.member_attack, member)
                .map(|attack| attack.victim)
                .or_else(|| state.get(ids.armed, member).and_then(|armed| armed.target))
        };
        if here.minus(there).length_squared() < ENGAGED_GAP * ENGAGED_GAP {
            for member in &members {
                if attacking(tick.state(), *member).is_some() {
                    return Ok(true);
                }
                if let Some(foe) = self.foe_of(tick, ids, *member) {
                    self.engage_on_spot(tick, ids, &foe, None, living)?;
                    if attacking(tick.state(), *member).is_some() {
                        return Ok(true);
                    }
                }
            }
            return Ok(false);
        }
        Ok(members.iter().any(|member| {
            attacking(tick.state(), *member).is_some_and(|victim| theirs.contains(&victim))
        }))
    }

    pub(super) fn squish_retarget(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        living: &[(Foe, PlayerId)],
        now: Frame,
    ) -> Result<bool> {
        if !tick
            .state()
            .get(ids.horde_melee, horde_entity)
            .is_some_and(|melee| matches!(melee.phase, MeleePhase::Squish { .. }))
        {
            return Ok(false);
        }
        let Some(at) = tick
            .state()
            .get(ids.body, horde_entity)
            .map(|b| b.0.position)
        else {
            return Ok(false);
        };
        let radius = self.object_radius(tick, ids, horde_entity);
        if self.crush_state(tick, ids, horde_entity, radius).too_slow {
            return Ok(false);
        }
        let reach = self.melee_placement.approach_dist;
        let mut closest: Option<(f32, Foe)> = None;
        for (foe, _) in living {
            let gap = foe.position.minus(at).length();
            if closest.is_some_and(|(best, _)| best <= gap)
                || !crate::combat::in_range_volumes(
                    (at, radius, None),
                    foe.reach_volume(),
                    0.0,
                    0.0,
                    reach,
                )
                || tick.state().contains(ids.knocked, foe.entity)
                || !self.squish_gate(tick, ids, horde_entity, radius, foe)
            {
                continue;
            }
            closest = Some((gap, *foe));
        }
        let Some((_, foe)) = closest else {
            return Ok(false);
        };
        let next_acquire = tick
            .state()
            .get(ids.horde_target, horde_entity)
            .map_or(now, |held| held.next_acquire);
        tick.state_mut().insert(
            ids.horde_target,
            horde_entity,
            HordeTarget {
                target: Some(foe.entity),
                next_acquire,
            },
        )?;
        Ok(true)
    }

    pub(super) fn run_horde_squish(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
        budgets: &mut PathBudgets,
        now: Frame,
        living: &[(Foe, PlayerId)],
    ) -> Result<bool> {
        let Some(body) = tick.state().get(ids.body, horde_entity).map(|b| b.0) else {
            return Ok(false);
        };
        let radius = self.object_radius(tick, ids, horde_entity);
        let Some(target) = self.goal_members(tick, ids, melee.goal).first().copied() else {
            melee.phase = MeleePhase::Approach;
            return Ok(false);
        };
        if !self.squish_gate(tick, ids, horde_entity, radius, &target) {
            melee.phase = MeleePhase::Approach;
            self.end_group_closing(tick, ids, horde_entity);
            return Ok(false);
        }
        let fast = !self.crush_state(tick, ids, horde_entity, radius).too_slow;
        melee.phase = MeleePhase::Squish { fast };
        if !fast && self.horde_engaged_with(tick, ids, horde_entity, melee.goal, living)? {
            melee.phase = MeleePhase::Approach;
            self.end_group_closing(tick, ids, horde_entity);
            return Ok(false);
        }
        let walking = tick
            .state()
            .get(ids.path, horde_entity)
            .is_some_and(|path| path.kind == PathKind::Closing);
        let entering = melee.planned.is_none();
        if !entering && !walking {
            if !self.has_kind_bit(tick.state(), ids, horde_entity, ARMY_OF_DEAD) {
                melee.phase = MeleePhase::Approach;
                return Ok(false);
            }
        } else if let Some((seen, when)) = melee.planned.filter(|_| walking) {
            let due = now.get().saturating_sub(when.get()) >= SQUISH_REPLAN_TICKS
                && super::movement::goal_moved(body.position, seen, target.position);
            if !due {
                return Ok(false);
            }
        }
        let me_z = self
            .foe_of(tick, ids, horde_entity)
            .map_or(target.z, |foe| foe.z);
        let spot = super::loose_melee::squish_goal(
            (body.position, me_z),
            (target.position, target.z),
            radius,
        );
        melee.planned = Some((target.position, now));
        self.close_on_target(
            tick,
            ids,
            horde_entity,
            body.position,
            spot,
            0.0,
            PathKind::Closing,
            budgets,
        )?;
        let routed = tick.state().contains(ids.path, horde_entity);
        if !routed {
            if entering {
                melee.phase = MeleePhase::Approach;
            } else {
                self.abandon_attack(tick, ids, horde_entity, melee.goal)?;
                return Ok(true);
            }
        }
        Ok(false)
    }
}
