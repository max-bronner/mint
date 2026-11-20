use super::*;

const STOP_RECURSION_LIMIT: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerStopped;

impl Component for PlayerStopped {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl RotwkRules {
    pub(crate) fn holds_ground(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        self.stands_ground(world, ids, entity) || world.contains(ids.player_stopped, entity)
    }

    pub(super) fn release_player_stop(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) {
        let mut pending: Vec<(Entity, u8)> = self
            .selection(tick, ids, player)
            .into_iter()
            .map(|entity| {
                (
                    Self::commanded_unit(tick.state(), ids, entity),
                    STOP_RECURSION_LIMIT,
                )
            })
            .collect();
        while let Some((unit, depth)) = pending.pop() {
            tick.state_mut().remove(ids.player_stopped, unit);
            if depth == 0 {
                continue;
            }
            let below = depth.saturating_sub(1);
            if let Some(horde) = tick.state().get(ids.horde, unit) {
                pending.extend(horde.members.iter().map(|member| (*member, below)));
            }
            if let Some(passengers) = tick.state().get(ids.passengers, unit) {
                pending.extend(passengers.0.iter().map(|inside| (*inside, below)));
            }
        }
    }

    pub(super) fn stop(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) -> Result<()> {
        let selected = self.selection(tick, ids, player);
        let mut units: Vec<Entity> = Vec::new();
        for entity in selected {
            let owned = tick
                .state()
                .get(ids.owner, entity)
                .is_some_and(|owner| owner.0 == player);
            if !owned {
                continue;
            }
            units.push(Self::commanded_unit(tick.state(), ids, entity));
        }
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        let now = tick.frame();
        for unit in units {
            self.stop_entity(tick, ids, unit, now, STOP_RECURSION_LIMIT)?;
        }
        Ok(())
    }

    fn stop_entity(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        unit: Entity,
        now: Frame,
        depth: u8,
    ) -> Result<()> {
        if !tick.state().is_live(unit) {
            return Ok(());
        }
        if self.is_porcupine_horde(tick, ids, unit) {
            return Ok(());
        }
        if self.is_rampaging(tick.state(), unit) {
            return Ok(());
        }
        tick.state_mut().remove(ids.deferred_order, unit);
        tick.state_mut()
            .insert(ids.player_stopped, unit, PlayerStopped)?;
        self.hold_bird_stop(tick, ids, unit)?;
        tick.state_mut().remove(ids.order_goal, unit);
        Self::clear_path_retry(tick, ids, unit);
        if depth > 0 {
            if let Some(horde) = tick.state().get(ids.horde, unit).cloned() {
                let walking = tick.state().contains(ids.path, unit)
                    && !tick.state().contains(ids.horde_melee, unit);
                if !walking {
                    for member in horde.members {
                        self.stop_entity(tick, ids, member, now, depth.saturating_sub(1))?;
                    }
                }
            }
            if let Some(passengers) = tick.state().get(ids.passengers, unit).cloned() {
                for passenger in passengers.0 {
                    self.stop_entity(tick, ids, passenger, now, depth.saturating_sub(1))?;
                }
            }
        }
        self.stop_unit(tick, ids, unit);
        let mobile = self.move_refused(tick, ids, unit).is_none();
        if mobile && !tick.state().contains(ids.member, unit) {
            self.leave_guard(tick, ids, unit);
            if let Some(guard) =
                self.guard_here(tick, ids, unit, super::guard::GuardMode::Normal, now)
            {
                tick.state_mut().insert(ids.guard, unit, guard)?;
            }
        }
        self.idle_slaves(tick, ids, unit);
        self.hold_back_stealth_scan(tick, ids, unit, now)
    }

    pub(super) fn is_rampaging(&self, world: &World, entity: Entity) -> bool {
        self.modules
            .as_ref()
            .and_then(|wiring| world.get(wiring.state, entity))
            .is_some_and(|state| state.has_weapon_set_flag("RAMPAGE"))
    }
}
