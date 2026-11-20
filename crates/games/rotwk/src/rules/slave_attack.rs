use mint_sage_modules::Effect;
use mint_sim::Frame;

use super::*;
use crate::combat::{AttackOrder, GroundOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OneShotOrder {
    pub(crate) since: Frame,
}

impl Component for OneShotOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.since.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SlaveTarget {
    Goal(Entity),
    Point(Vec2),
}

impl RotwkRules {
    pub(super) fn watched_slave(&self, world: &World, ids: Ids, master: Entity) -> Option<Entity> {
        let wiring = self.modules.as_ref()?;
        let slave = world.get(wiring.watched_slave, master)?.0;
        (world.is_live(slave)
            && !world.contains(ids.doomed, slave)
            && world.contains(ids.armed, slave))
        .then_some(slave)
    }

    pub(super) fn goal_object(&self, world: &World, ids: Ids, master: Entity) -> Option<Entity> {
        world
            .get(ids.attack_order, master)
            .map(|order| order.target)
            .or_else(|| world.get(ids.armed, master).and_then(|armed| armed.target))
            .filter(|goal| world.is_live(*goal))
    }

    pub(super) fn order_watched_slave(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        master: Entity,
        target: SlaveTarget,
    ) -> Result<bool> {
        if !tick.state().contains(ids.armed, master) {
            return Ok(false);
        }
        let Some(slave) = self.watched_slave(tick.state(), ids, master) else {
            return Ok(false);
        };
        let busy = tick.state().contains(ids.attack_order, slave)
            || tick.state().contains(ids.ground_order, slave)
            || tick
                .state()
                .get(ids.armed, slave)
                .is_some_and(|armed| armed.target.is_some());
        match target {
            SlaveTarget::Goal(goal) => {
                if busy {
                    return Ok(false);
                }
                self.apply_effects(
                    tick,
                    ids,
                    vec![(slave, Effect::LockWeaponSlotTemporarily(0))],
                )?;
                tick.state_mut().remove(ids.player_order, slave);
                tick.state_mut().insert(
                    ids.attack_order,
                    slave,
                    AttackOrder {
                        target: goal,
                        planned: false,
                        horde: None,
                        guard: false,
                        relation_free: false,
                        order_owner: None,
                    },
                )?;
            }
            SlaveTarget::Point(at) => {
                self.apply_effects(
                    tick,
                    ids,
                    vec![(slave, Effect::LockWeaponSlotTemporarily(0))],
                )?;
                tick.state_mut().remove(ids.player_order, slave);
                tick.state_mut().remove(ids.attack_order, slave);
                tick.state_mut()
                    .insert(ids.ground_order, slave, GroundOrder { slot: 0, at })?;
            }
        }
        let since = tick.frame();
        tick.state_mut()
            .insert(ids.one_shot_order, slave, OneShotOrder { since })?;
        Ok(true)
    }

    pub(super) fn end_one_shot_orders(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let mut orders: Vec<(Entity, Frame)> = tick
            .state()
            .store(ids.one_shot_order)
            .map(|store| {
                store
                    .iter()
                    .map(|(entity, order)| (entity, order.since))
                    .collect()
            })
            .unwrap_or_default();
        if orders.is_empty() {
            return Ok(());
        }
        orders.sort_unstable_by_key(|(entity, _)| entity.index());
        let mut releases = Vec::new();
        for (entity, since) in orders {
            let alive = tick.state().is_live(entity);
            let ordered = tick.state().contains(ids.attack_order, entity)
                || tick.state().contains(ids.ground_order, entity);
            let fired = tick
                .state()
                .get(ids.armed, entity)
                .and_then(|armed| armed.state.last_shot())
                .is_some_and(|shot| shot >= since);
            if !alive || (ordered && !fired) {
                continue;
            }
            if fired {
                tick.state_mut().remove(ids.attack_order, entity);
                tick.state_mut().remove(ids.ground_order, entity);
                if let Some(mut armed) = tick.state().get(ids.armed, entity).copied() {
                    armed.retarget(None);
                    tick.state_mut().insert(ids.armed, entity, armed)?;
                }
            }
            tick.state_mut().remove(ids.one_shot_order, entity);
            if self.holds_temporary_weapon_lock(tick, entity) {
                releases.push((entity, Effect::ReleaseTemporaryWeaponLock));
            }
        }
        self.apply_effects(tick, ids, releases)
    }
}
