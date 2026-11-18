use mint_sim::Entity;

use super::knockback::Phase;
use super::map_scripts::{ATTITUDE_PEACEFUL, ATTITUDE_SLEEP};
use super::{Ids, RotwkRules};
use mint_sim::Tick;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FireGate {
    Open,
    Held,
    Refused,
}

impl RotwkRules {
    pub(super) fn fire_gate(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        victim: Entity,
    ) -> FireGate {
        let world = tick.state();
        let calm = matches!(
            self.mood_attitude(world, ids, entity),
            ATTITUDE_SLEEP | ATTITUDE_PEACEFUL
        );
        if calm
            && !(self.has_kind_bit(world, ids, entity, "INFANTRY")
                && self.has_kind_bit(world, ids, entity, "IGNORED_IN_GUI"))
        {
            return FireGate::Held;
        }
        let own_player = world.get(ids.owner, entity).map(|owner| owner.0)
            == world.get(ids.owner, victim).map(|owner| owner.0)
            && world.contains(ids.owner, victim);
        let may_shoot_own = self.has_status_bit(world, entity, "BLOODTHIRSTY")
            || self.holds_model_condition(tick, entity, "EMOTION_DISSIDENT")
            || ["MINE", "WEBBED", "TREE"]
                .iter()
                .any(|kind| self.has_kind_bit(world, ids, victim, kind));
        let thrown = world
            .get(ids.knocked, victim)
            .is_some_and(|knocked| knocked.phase == Phase::Airborne);
        let horde_victim = self.has_kind_bit(world, ids, victim, "HORDE")
            && !self.has_kind_bit(world, ids, entity, "HORDE");
        let container = world
            .get(ids.member, victim)
            .map_or(victim, |member| member.horde);
        let contained =
            world.contains(ids.contained, victim) || world.contains(ids.contained, container);
        if (own_player && !may_shoot_own) || thrown || horde_victim || contained {
            FireGate::Refused
        } else {
            FireGate::Open
        }
    }
}
