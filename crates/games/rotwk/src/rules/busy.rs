use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HeldOrder {
    MoveTo(Vec2, bool),
    AttackObject(Entity),
    AttackMove(Vec2, bool),
    AttackGround { slot: u8, at: Vec2 },
    ForceAttackObject(Entity),
    ScriptAttackObject(Entity),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeferredOrder {
    pub(crate) order: HeldOrder,
    pub(crate) player: PlayerId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IgnoreAiCommand(pub(crate) Frame);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiBusy {
    until: Frame,
    stash: Option<crate::unit::Path>,
}

impl Component for AiBusy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.until.hash_into(hasher);
        hasher.write_bool(self.stash.is_some());
        if let Some(path) = &self.stash {
            Component::hash_into(path, hasher);
        }
    }
}

impl Component for IgnoreAiCommand {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0.get());
    }
}

impl Component for DeferredOrder {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u8(self.player.get());
        match self.order {
            HeldOrder::MoveTo(at, queue) => {
                hasher.write_u8(0);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
                hasher.write_bool(queue);
            }
            HeldOrder::AttackObject(target) => {
                hasher.write_u8(1);
                target.hash_into(hasher);
            }
            HeldOrder::AttackMove(at, queue) => {
                hasher.write_u8(2);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
                hasher.write_bool(queue);
            }
            HeldOrder::AttackGround { slot, at } => {
                hasher.write_u8(3);
                hasher.write_u8(slot);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
            }
            HeldOrder::ForceAttackObject(target) => {
                hasher.write_u8(4);
                target.hash_into(hasher);
            }
            HeldOrder::ScriptAttackObject(target) => {
                hasher.write_u8(5);
                target.hash_into(hasher);
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn busy(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        if Self::ai_busy(world, ids, entity, now) {
            return true;
        }
        let Some(armed) = world.get(ids.armed, entity) else {
            return false;
        };
        let Some(hold) = self
            .combat
            .weapons
            .get(armed.weapon)
            .map(|template| template.hold_after_firing)
            .filter(|hold| hold.get() > 0)
        else {
            return false;
        };
        armed
            .state
            .last_shot()
            .and_then(|shot| shot.checked_add(hold))
            .is_some_and(|until| now < until)
    }

    pub(super) fn ai_busy(world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        world
            .get(ids.ai_busy, entity)
            .is_some_and(|busy| now < busy.until)
    }

    pub(super) fn start_ai_busy(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        frames: LogicFrames,
    ) -> Result<()> {
        if frames.is_zero() || !tick.state().is_live(entity) {
            return Ok(());
        }
        let frames = frames.min(LogicFrames::from_engine_milliseconds(60_000));
        let until = tick.frame().checked_add(frames).unwrap_or(Frame::FOREVER);
        let stash = match tick.state_mut().remove(ids.ai_busy, entity) {
            Some(running) => running.stash,
            None => super::emotion_states::take_route(tick, ids, entity),
        };
        diag!(
            "busy: entity {} in AIBusyState until frame {}",
            entity.index(),
            until.get()
        );
        tick.state_mut()
            .insert(ids.ai_busy, entity, AiBusy { until, stash })?;
        Ok(())
    }

    pub(super) fn end_ai_busy(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let ended: Vec<Entity> = tick
            .state()
            .store(ids.ai_busy)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, busy)| now >= busy.until)
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in ended {
            let Some(busy) = tick.state_mut().remove(ids.ai_busy, entity) else {
                continue;
            };
            if let Some(path) = busy.stash {
                if !tick.state().contains(ids.path, entity) {
                    tick.state_mut().insert(ids.path, entity, path)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn issue_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        order: HeldOrder,
    ) -> Result<()> {
        let now = tick.frame();
        let own = |entity: &Entity| {
            !tick.state().contains(ids.member, *entity)
                && tick
                    .state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
        };
        let selection = self.selection(tick, ids, player);
        let asleep: Vec<Entity> = selection
            .iter()
            .copied()
            .filter(|entity| self.mood_asleep(tick.state(), ids, *entity))
            .collect();
        let busy: Vec<Entity> = selection
            .iter()
            .copied()
            .filter(|entity| {
                own(entity)
                    && !asleep.contains(entity)
                    && self.busy(tick.state(), ids, *entity, now)
            })
            .collect();
        let ignored: Vec<Entity> = selection
            .iter()
            .copied()
            .filter(|entity| {
                own(entity)
                    && tick
                        .state()
                        .get(ids.ignore_ai_command, *entity)
                        .is_some_and(|held| now < held.0)
            })
            .collect();
        for entity in &busy {
            tick.state_mut().insert(
                ids.deferred_order,
                *entity,
                DeferredOrder { order, player },
            )?;
            diag!(
                "busy: entity {} holds {order:?} until its HoldAfterFiringDelay ends",
                entity.index()
            );
        }
        for entity in &ignored {
            diag!(
                "busy: entity {} ignores {order:?} - still under its toggle's command freeze",
                entity.index()
            );
        }
        let excluded: Vec<Entity> = busy
            .iter()
            .chain(&ignored)
            .chain(&asleep)
            .copied()
            .collect();
        let saved = Self::deselect(tick, ids, player, &excluded)?;
        let result = match order {
            HeldOrder::MoveTo(at, queue) => self.run_move_by_mood(tick, ids, player, at, queue),
            _ => self.run_order(tick, ids, player, order),
        };
        Self::restore_selection(tick, ids, &saved)?;
        result
    }

    fn run_move_by_mood(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        at: Vec2,
        queue: bool,
    ) -> Result<()> {
        let (hot, calm): (Vec<Entity>, Vec<Entity>) = self
            .selection(tick, ids, player)
            .into_iter()
            .partition(|&entity| {
                matches!(
                    self.mood_attitude(tick.state(), ids, entity),
                    map_scripts::ATTITUDE_ALERT | map_scripts::ATTITUDE_AGGRESSIVE
                )
            });
        if hot.is_empty() {
            return self.run_order(tick, ids, player, HeldOrder::MoveTo(at, queue));
        }
        let saved = Self::deselect(tick, ids, player, &calm)?;
        let result = self.run_order(tick, ids, player, HeldOrder::AttackMove(at, queue));
        Self::restore_selection(tick, ids, &saved)?;
        result?;
        if calm.is_empty() {
            return Ok(());
        }
        let saved = Self::deselect(tick, ids, player, &hot)?;
        let result = self.run_order(tick, ids, player, HeldOrder::MoveTo(at, queue));
        Self::restore_selection(tick, ids, &saved)?;
        result
    }

    pub(super) fn replay_deferred_orders(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let held: Vec<Entity> = tick
            .state()
            .store(ids.deferred_order)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in held {
            if self.busy(tick.state(), ids, entity, now) {
                continue;
            }
            let Some(deferred) = tick.state().get(ids.deferred_order, entity).copied() else {
                continue;
            };
            tick.state_mut().remove(ids.deferred_order, entity);
            if !tick.state().is_live(entity)
                || tick.state().contains(ids.doomed, entity)
                || self.mood_asleep(tick.state(), ids, entity)
            {
                continue;
            }
            diag!(
                "busy: entity {} runs its held {:?} at frame {}",
                entity.index(),
                deferred.order,
                now.get()
            );
            let others: Vec<Entity> = self
                .selection(tick, ids, deferred.player)
                .into_iter()
                .filter(|other| *other != entity)
                .collect();
            let saved_others = Self::deselect(tick, ids, deferred.player, &others)?;
            let own = tick.state().get(ids.selected, entity).copied();
            let bit = Selected::bit(deferred.player).unwrap_or(0);
            tick.state_mut().insert(
                ids.selected,
                entity,
                Selected(own.map_or(0, |mask| mask.0) | bit),
            )?;
            let result = match deferred.order {
                HeldOrder::MoveTo(at, queue) => {
                    self.run_move_by_mood(tick, ids, deferred.player, at, queue)
                }
                order => self.run_order(tick, ids, deferred.player, order),
            };
            Self::restore_selection(tick, ids, &[(entity, own)])?;
            Self::restore_selection(tick, ids, &saved_others)?;
            result?;
        }
        Ok(())
    }

    fn run_order(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        order: HeldOrder,
    ) -> Result<()> {
        match order {
            HeldOrder::MoveTo(goal, queue) => self.move_to_queued(tick, ids, player, goal, queue),
            HeldOrder::AttackObject(target) => {
                self.uncohere_move_speed(tick, ids, player);
                self.attack_object(tick, ids, player, target)
            }
            HeldOrder::AttackMove(goal, queue) => {
                self.attack_move_queued(tick, ids, player, goal, queue)
            }
            HeldOrder::AttackGround { slot, at } => {
                self.uncohere_move_speed(tick, ids, player);
                self.attack_ground(tick, ids, player, slot, at)
            }
            HeldOrder::ForceAttackObject(target) => {
                self.uncohere_move_speed(tick, ids, player);
                self.force_attack_object(tick, ids, player, target)
            }
            HeldOrder::ScriptAttackObject(target) => {
                self.uncohere_move_speed(tick, ids, player);
                self.script_attack_object(tick, ids, player, target)
            }
        }
    }

    pub(super) fn deselect(
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        entities: &[Entity],
    ) -> Result<Vec<(Entity, Option<Selected>)>> {
        let Some(bit) = Selected::bit(player) else {
            return Ok(Vec::new());
        };
        let mut saved = Vec::with_capacity(entities.len());
        for entity in entities {
            let held = tick.state().get(ids.selected, *entity).copied();
            saved.push((*entity, held));
            if let Some(mask) = held {
                let rest = mask.0 & !bit;
                if rest == 0 {
                    tick.state_mut().remove(ids.selected, *entity);
                } else {
                    tick.state_mut()
                        .insert(ids.selected, *entity, Selected(rest))?;
                }
            }
        }
        Ok(saved)
    }

    pub(super) fn restore_selection(
        tick: &mut Tick<'_>,
        ids: Ids,
        saved: &[(Entity, Option<Selected>)],
    ) -> Result<()> {
        for (entity, held) in saved {
            match held {
                Some(mask) => {
                    tick.state_mut().insert(ids.selected, *entity, *mask)?;
                }
                None => {
                    tick.state_mut().remove(ids.selected, *entity);
                }
            }
        }
        Ok(())
    }
}
