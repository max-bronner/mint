use super::*;
use crate::melee::{HordeMelee, MeleePhase};

const SQUISH_REPLAN_TICKS: u64 =
    5 * (mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ) as u64;

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
        if self.stands_ground(tick.state(), ids, horde_entity) {
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

    pub(super) fn run_horde_squish(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        horde_entity: Entity,
        melee: &mut HordeMelee,
        budgets: &mut PathBudgets,
        now: Frame,
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
