use super::*;

impl RotwkRules {
    pub(super) fn selection(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> Vec<Entity> {
        tick.state()
            .store(ids.selected)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, mask)| mask.holds(player))
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn take_player_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        is_attack: bool,
        clear_queue: bool,
    ) -> Result<bool> {
        if !self.porcupine_gate(tick, ids, entity, is_attack)? {
            return Ok(false);
        }
        self.interrupt_emotion_state(tick, ids, entity)?;
        self.leave_guard(tick, ids, entity);
        self.stance_on_order(tick, ids, entity)?;
        tick.state_mut()
            .insert(ids.player_order, entity, PlayerOrder)?;
        tick.state_mut().remove(ids.ground_order, entity);
        tick.state_mut().remove(ids.suspended_order, entity);
        tick.state_mut().remove(ids.attack_move_order, entity);
        tick.state_mut().remove(ids.attack_order, entity);
        tick.state_mut().remove(ids.horde_target, entity);
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
                .unwrap_or(Selected(0));
            tick.state_mut()
                .insert(ids.selected, *entity, Selected(previous.0 | bit))?;
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
        for entity in self.selection_with_hordes(tick, ids, units) {
            if !tick.state().is_live(entity) || tick.state().contains(ids.doomed, entity) {
                continue;
            }
            let previous = tick
                .state()
                .get(ids.selected, entity)
                .copied()
                .unwrap_or(Selected(0));
            tick.state_mut()
                .insert(ids.selected, entity, Selected(previous.0 | bit))?;
        }
        Ok(())
    }

    pub(super) fn clear_selection(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
    ) -> Result<()> {
        let Some(bit) = Selected::bit(player) else {
            return Ok(());
        };
        let held = self.selection(tick, ids, player);
        for entity in held {
            let remaining = tick
                .state()
                .get(ids.selected, entity)
                .map_or(0, |mask| mask.0 & !bit);
            if remaining == 0 {
                tick.state_mut().remove(ids.selected, entity);
            } else {
                tick.state_mut()
                    .insert(ids.selected, entity, Selected(remaining))?;
            }
        }
        Ok(())
    }
}
