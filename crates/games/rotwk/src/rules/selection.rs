use super::*;

impl RotwkRules {
    pub(super) fn selection(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> Vec<Entity> {
        let mut held: Vec<(u32, Entity)> = tick
            .state()
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, mask)| (mask.rank(player), entity))
                    .collect()
            })
            .unwrap_or_default();
        held.sort_by_key(|(rank, entity)| (*rank, entity.index()));
        held.into_iter().map(|(_, entity)| entity).collect()
    }

    fn next_selection_rank(tick: &Tick<'_>, ids: Ids, player: PlayerId) -> u32 {
        tick.state()
            .store(ids.selected)
            .and_then(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(_, mask)| mask.rank(player).saturating_add(1))
                    .max()
            })
            .unwrap_or(0)
    }

    pub(super) fn note_order_origin(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(at) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        if self
            .current_locomotor(tick, ids, entity)
            .is_some_and(|limits| limits.wheel_backing.is_some())
        {
            tick.state_mut()
                .insert(ids.order_origin, entity, crate::unit::OrderOrigin(at))?;
        }
        Ok(())
    }

    pub(super) fn take_player_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        is_attack: bool,
        clear_queue: bool,
        horde_command: bool,
    ) -> Result<bool> {
        if !self.porcupine_gate(tick, ids, entity, is_attack)? {
            return Ok(false);
        }
        if horde_command || !tick.state().contains(ids.horde, entity) {
            self.order_ends_factory_exit(tick, ids, entity)?;
        }
        self.drop_bird_held_stop(tick, ids, entity)?;
        self.interrupt_emotion_state(tick, ids, entity)?;
        self.end_repulsed(tick, ids, entity)?;
        self.leave_guard(tick, ids, entity);
        self.stance_on_order(tick, ids, entity)?;
        tick.state_mut()
            .insert(ids.player_order, entity, PlayerOrder)?;
        tick.state_mut().remove(ids.ground_order, entity);
        tick.state_mut().remove(ids.suspended_order, entity);
        tick.state_mut().remove(ids.attack_move_order, entity);
        tick.state_mut().remove(ids.attack_order, entity);
        tick.state_mut().remove(ids.horde_target, entity);
        tick.state_mut().remove(ids.order_goal, entity);
        if clear_queue {
            tick.state_mut().remove(ids.order_queue, entity);
        }
        let members = tick
            .state()
            .get(ids.horde, entity)
            .map(|horde| horde.members.clone())
            .unwrap_or_default();
        for member in members {
            if tick.state().is_live(member) {
                tick.state_mut()
                    .insert(ids.player_order, member, PlayerOrder)?;
                tick.state_mut().remove(ids.ground_order, member);
                tick.state_mut().remove(ids.suspended_order, member);
                tick.state_mut().remove(ids.attack_move_order, member);
                tick.state_mut().remove(ids.attack_order, member);
                tick.state_mut().remove(ids.horde_target, member);
            }
        }
        Ok(true)
    }

    pub(super) fn selection_with_hordes(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        units: &[Entity],
    ) -> Vec<Entity> {
        let mut out = Vec::with_capacity(units.len());
        for entity in units {
            if let Some(horde) = tick.state().get(ids.horde, *entity) {
                out.extend(horde.members.iter().copied());
                continue;
            }
            out.push(*entity);
            let Some(member) = tick.state().get(ids.member, *entity) else {
                continue;
            };
            let horde = member.horde;
            let Some(horde) = tick.state().get(ids.horde, horde) else {
                continue;
            };
            out.extend(horde.members.iter().copied());
        }
        out
    }

    pub(super) fn set_selection(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        replace: bool,
        units: &[Entity],
    ) -> Result<()> {
        if units.len() > MAX_SELECTION_UNITS {
            return Ok(());
        }
        let Some(bit) = Selected::bit(player) else {
            return Ok(());
        };
        if replace {
            self.clear_selection(tick, ids, player)?;
        }
        let mut rank = Self::next_selection_rank(tick, ids, player);
        let units = self.selection_with_hordes(tick, ids, units);
        for entity in &units {
            if !tick.state().is_live(*entity) {
                continue;
            }
            let selectable = tick.state().get(ids.shape, *entity).is_some_and(|shape| {
                shape.kind_of.contains(KindOf::SELECTABLE)
                    || shape.kind_of.contains(KindOf::ALWAYS_SELECTABLE)
            });
            let dead_unselectable = tick.state().contains(ids.doomed, *entity)
                && !(self.is_rubble(tick.state(), ids, *entity)
                    && tick
                        .state()
                        .get(ids.kind, *entity)
                        .and_then(|kind| self.kinds.get(*kind))
                        .is_some_and(|entry| entry.record.keep_selectable_when_dead));
            let streaming = tick
                .state()
                .get(ids.member, *entity)
                .is_some_and(|member| tick.state().contains(ids.production_stream, member.horde));
            if !selectable
                || dead_unselectable
                || streaming
                || tick.state().contains(ids.concealed, *entity)
                || self
                    .contained_status(tick.state(), ids, *entity)
                    .is_some_and(|status| status.has(super::contain::ContainStatus::UNSELECTABLE))
                || self.modules.as_ref().is_some_and(|wiring| {
                    wiring.carries_status(tick.state(), *entity, "UNSELECTABLE")
                })
                || self.pad_is_occupied(tick.state(), ids, *entity)
            {
                continue;
            }
            let previous = tick
                .state()
                .get(ids.selected, *entity)
                .copied()
                .unwrap_or_default();
            if previous.0 & bit == 0 {
                tick.state_mut()
                    .insert(ids.selected, *entity, previous.with(player, rank))?;
                rank = rank.saturating_add(1);
            }
            self.click_reaction(tick, ids, player, *entity)?;
        }
        Ok(())
    }

    pub(super) fn select_for_script(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        units: &[Entity],
    ) -> Result<()> {
        let Some(bit) = Selected::bit(player) else {
            return Ok(());
        };
        let mut rank = Self::next_selection_rank(tick, ids, player);
        for entity in self.selection_with_hordes(tick, ids, units) {
            if !tick.state().is_live(entity) || tick.state().contains(ids.doomed, entity) {
                continue;
            }
            let previous = tick
                .state()
                .get(ids.selected, entity)
                .copied()
                .unwrap_or_default();
            if previous.0 & bit == 0 {
                tick.state_mut()
                    .insert(ids.selected, entity, previous.with(player, rank))?;
                rank = rank.saturating_add(1);
            }
        }
        Ok(())
    }

    pub(super) fn clear_selection(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        if Selected::bit(player).is_none() {
            return Ok(());
        }
        let held = self.selection(tick, ids, player);
        for entity in held {
            let remaining = tick
                .state()
                .get(ids.selected, entity)
                .map_or_else(Selected::default, |mask| mask.without(player));
            if remaining.0 == 0 {
                tick.state_mut().remove(ids.selected, entity);
            } else {
                tick.state_mut().insert(ids.selected, entity, remaining)?;
            }
        }
        Ok(())
    }
}
