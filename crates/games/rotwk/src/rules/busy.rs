use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MovePath {
    PerMember,
    PlayerGroup,
    Group,
    GroupLeg,
    AttackFollow,
    AttackFollowLeg,
}

impl MovePath {
    pub(crate) fn is_player_dispatch(self) -> bool {
        !matches!(
            self,
            Self::Group | Self::GroupLeg | Self::AttackFollow | Self::AttackFollowLeg
        )
    }
}

const AI_MOVE_TEMPORARY_MS: u32 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HeldOrder {
    MoveTo(Vec2, bool, MovePath),
    AttackObject(Entity),
    AttackMove(Vec2, bool, MovePath),
    AttackGround { slot: u8, at: Vec2 },
    ForceAttackObject(Entity),
    ScriptAttackObject(Entity),
    Stop,
    Enter(Entity),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeferredOrder {
    pub(crate) order: HeldOrder,
    pub(crate) player: PlayerId,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HeldAside {
    pub(crate) avoid: Vec<Vec2>,
    pub(crate) from_point: bool,
    pub(crate) radii: (f32, f32),
    pub(crate) destination: Vec2,
}

impl Component for HeldAside {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.from_point);
        for point in &self.avoid {
            hasher.write_f32(point.x);
            hasher.write_f32(point.y);
        }
        hasher.write_f32(self.radii.0);
        hasher.write_f32(self.radii.1);
        hasher.write_f32(self.destination.x);
        hasher.write_f32(self.destination.y);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IgnoreAiCommand(pub(crate) Frame);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AiBusy {
    until: Frame,
    stash: Option<crate::unit::Path>,
    walking: bool,
    aside: bool,
}

impl Component for AiBusy {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.until.hash_into(hasher);
        hasher.write_bool(self.walking);
        hasher.write_bool(self.aside);
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
            HeldOrder::MoveTo(at, queue, path) => {
                hasher.write_u8(0);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
                hasher.write_bool(queue);
                hasher.write_u8(path as u8);
            }
            HeldOrder::AttackObject(target) => {
                hasher.write_u8(1);
                target.hash_into(hasher);
            }
            HeldOrder::AttackMove(at, queue, path) => {
                hasher.write_u8(2);
                hasher.write_f32(at.x);
                hasher.write_f32(at.y);
                hasher.write_bool(queue);
                hasher.write_u8(path as u8);
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
            HeldOrder::Stop => hasher.write_u8(6),
            HeldOrder::Enter(container) => {
                hasher.write_u8(7);
                container.hash_into(hasher);
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn busy(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        Self::ai_busy(world, ids, entity, now) || self.weapon_busy(world, ids, entity, now)
    }

    pub(super) fn weapon_busy(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
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

    pub(super) fn mid_portal(world: &World, ids: Ids, entity: Entity) -> bool {
        let (Some(path), Some(body)) = (world.get(ids.path, entity), world.get(ids.body, entity))
        else {
            return false;
        };
        path.route.tagged_at(body.0.waypoint)
    }

    pub(super) fn holds_orders(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        self.weapon_busy(world, ids, entity, now)
            || Self::mid_portal(world, ids, entity)
            || world
                .get(ids.ignore_ai_command, entity)
                .is_some_and(|held| now < held.0)
            || self.carries_object_status(world, entity, "SUMMONING_REPLACEMENT")
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
        tick.state_mut().insert(
            ids.ai_busy,
            entity,
            AiBusy {
                until,
                stash,
                walking: false,
                aside: false,
            },
        )?;
        Ok(())
    }

    pub(super) fn start_move_aside_state(
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        stash: Option<crate::unit::Path>,
        for_good: bool,
    ) -> Result<()> {
        let until = if for_good {
            Frame::FOREVER
        } else {
            tick.frame()
                .checked_add(LogicFrames::from_engine_milliseconds(10_000))
                .unwrap_or(Frame::FOREVER)
        };
        tick.state_mut().insert(
            ids.ai_busy,
            entity,
            AiBusy {
                until,
                stash: if for_good { None } else { stash },
                walking: true,
                aside: true,
            },
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ai_move(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        (entity, mover): (Entity, Entity),
        from: Vec2,
        to: Vec2,
        kind: crate::unit::PathKind,
        budgets: &mut PathBudgets,
    ) -> Result<()> {
        Self::ai_move_with(tick, ids, (entity, mover), |tick| {
            self.close_on_target(tick, ids, mover, from, to, 0.0, kind, budgets)
        })
    }

    pub(super) fn ai_move_with(
        tick: &mut Tick<'_>,
        ids: Ids,
        (entity, mover): (Entity, Entity),
        install: impl FnOnce(&mut Tick<'_>) -> Result<()>,
    ) -> Result<()> {
        let world = tick.state();
        let idle = ![entity, mover].iter().any(|&e| {
            world.contains(ids.attack_order, e)
                || world.contains(ids.attack_move_order, e)
                || world.contains(ids.player_order, e)
        }) && !world.contains(ids.path, mover);
        if idle {
            tick.state_mut().remove(ids.path, mover);
            return install(tick);
        }
        let running = tick.state_mut().remove(ids.ai_busy, mover);
        let stash = match running {
            Some(running) => running.stash,
            None => super::emotion_states::take_route(tick, ids, mover),
        };
        install(tick)?;
        if !tick.state().contains(ids.path, mover) {
            if let Some(path) = stash {
                tick.state_mut().insert(ids.path, mover, path)?;
            }
            return Ok(());
        }
        let until = tick
            .frame()
            .checked_add(LogicFrames::from_engine_milliseconds(AI_MOVE_TEMPORARY_MS))
            .unwrap_or(Frame::FOREVER);
        tick.state_mut().insert(
            ids.ai_busy,
            mover,
            AiBusy {
                until,
                stash,
                walking: true,
                aside: false,
            },
        )?;
        Ok(())
    }

    fn resume_after_aside(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        old: crate::unit::Path,
    ) -> Result<()> {
        let Some(goal) = old.route.points.last().copied() else {
            return Ok(());
        };
        let Some(from) = tick.state().get(ids.body, entity).map(|b| b.0.position) else {
            return Ok(());
        };
        let class = self.mover_class(tick, ids, entity);
        let Some(start) = self.passable_cell_for(tick, ids, from, class) else {
            return Ok(());
        };
        let mut pipeline = Pipeline::new(self.modes(tick, ids).config());
        let (_, _, points) = self.plan_via(
            tick,
            ids,
            &mut pipeline,
            entity,
            class,
            (start, from),
            &[goal],
            false,
        )?;
        if points.len() < 2 {
            return Ok(());
        }
        if let Some(body) = tick.state().get(ids.body, entity).copied() {
            let mut state = body.0;
            state.waypoint = 0;
            tick.state_mut().insert(ids.body, entity, Body(state))?;
        }
        let route = Route {
            points,
            extra_distance: 0.0,
            unsearched: false,
            portals: Vec::new(),
        };
        tick.state_mut()
            .insert(ids.path, entity, crate::unit::Path { route, ..old })?;
        Ok(())
    }

    pub(super) fn idle_busy_ai(tick: &mut Tick<'_>, ids: Ids, entity: Entity) {
        if let Some(busy) = tick.state_mut().remove(ids.ai_busy, entity) {
            if busy.walking {
                tick.state_mut().remove(ids.path, entity);
            }
        }
    }

    pub(super) fn end_ai_busy(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let ended: Vec<Entity> = tick
            .state()
            .store(ids.ai_busy)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, busy)| {
                        now >= busy.until
                            || (busy.walking && !tick.state().contains(ids.path, *entity))
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in ended {
            let Some(busy) = tick.state_mut().remove(ids.ai_busy, entity) else {
                continue;
            };
            if busy.walking && !busy.aside && busy.stash.is_some() {
                continue;
            }
            if busy.walking {
                tick.state_mut().remove(ids.path, entity);
            }
            if let Some(path) = busy.stash {
                if busy.walking {
                    self.resume_after_aside(tick, ids, entity, path)?;
                } else if !tick.state().contains(ids.path, entity) {
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
        let selection = self.selection(tick, ids, player);
        let own = |entity: &Entity| {
            !tick.state().contains(ids.member, *entity)
                && tick
                    .state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
        };
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
                    && self.holds_orders(tick.state(), ids, *entity, now)
            })
            .collect();
        for entity in &busy {
            tick.state_mut().remove(ids.held_aside, *entity);
            tick.state_mut().insert(
                ids.deferred_order,
                *entity,
                DeferredOrder { order, player },
            )?;
            diag!(
                "busy: entity {} holds {order:?} until nothing holds its commands",
                entity.index()
            );
        }
        let excluded: Vec<Entity> = busy.iter().chain(&asleep).copied().collect();
        for entity in selection.iter().filter(|e| !excluded.contains(e)) {
            tick.state_mut().remove(ids.ai_busy, *entity);
        }
        self.release_temporary_locks(
            tick,
            ids,
            selection.iter().filter(|e| !excluded.contains(e)),
        )?;
        let saved = Self::deselect(tick, ids, player, &excluded)?;
        let result = match order {
            HeldOrder::MoveTo(at, queue, path) => {
                self.run_move_by_mood(tick, ids, player, at, queue, path)
            }
            _ => self.run_order(tick, ids, player, order),
        };
        Self::restore_selection(tick, ids, &saved)?;
        result
    }

    fn release_temporary_locks<'a>(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        commanded: impl Iterator<Item = &'a Entity>,
    ) -> Result<()> {
        let commanded: Vec<Entity> = commanded.copied().collect();
        let mut effects = Vec::new();
        for entity in self
            .horde_members(tick, ids, &commanded)
            .into_iter()
            .chain(commanded)
        {
            if self.holds_temporary_weapon_lock(tick, entity) {
                effects.push((
                    entity,
                    mint_sage_modules::Effect::ReleaseTemporaryWeaponLock,
                ));
            }
        }
        if effects.is_empty() {
            return Ok(());
        }
        self.apply_effects(tick, ids, effects)
    }

    fn run_move_by_mood(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        at: Vec2,
        queue: bool,
        path: MovePath,
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
            return self.run_order(tick, ids, player, HeldOrder::MoveTo(at, queue, path));
        }
        let saved = Self::deselect(tick, ids, player, &calm)?;
        let result = self.run_order(tick, ids, player, HeldOrder::AttackMove(at, queue, path));
        Self::restore_selection(tick, ids, &saved)?;
        result?;
        if calm.is_empty() {
            return Ok(());
        }
        let saved = Self::deselect(tick, ids, player, &hot)?;
        let result = self.run_order(tick, ids, player, HeldOrder::MoveTo(at, queue, path));
        Self::restore_selection(tick, ids, &saved)?;
        result
    }

    pub(super) fn replay_deferred_orders(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        self.replay_held_asides(tick, ids)?;
        let held: Vec<Entity> = tick
            .state()
            .store(ids.deferred_order)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in held {
            if self.holds_orders(tick.state(), ids, entity, now) {
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
            tick.state_mut().remove(ids.ai_busy, entity);
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
            tick.state_mut().insert(
                ids.selected,
                entity,
                own.unwrap_or_default().with(deferred.player, 0),
            )?;
            let result = match deferred.order {
                HeldOrder::MoveTo(at, queue, path) => {
                    self.run_move_by_mood(tick, ids, deferred.player, at, queue, path)
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
            HeldOrder::MoveTo(goal, queue, path) => {
                self.move_to_queued(tick, ids, player, goal, queue, path)
            }
            HeldOrder::AttackObject(target) => {
                self.uncohere_move_speed(tick, ids, player);
                self.attack_object(tick, ids, player, target)
            }
            HeldOrder::AttackMove(goal, queue, path) => {
                self.attack_move_queued(tick, ids, player, goal, queue, path)
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
            HeldOrder::Stop => self.stop(tick, ids, player),
            HeldOrder::Enter(container) => self.enter_container(tick, ids, player, container),
        }
    }

    pub(super) fn deselect(
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        entities: &[Entity],
    ) -> Result<Vec<(Entity, Option<Selected>)>> {
        if Selected::bit(player).is_none() {
            return Ok(Vec::new());
        }
        let mut saved = Vec::with_capacity(entities.len());
        for entity in entities {
            let held = tick.state().get(ids.selected, *entity).copied();
            saved.push((*entity, held));
            if let Some(mask) = held {
                let rest = mask.without(player);
                if rest.0 == 0 {
                    tick.state_mut().remove(ids.selected, *entity);
                } else {
                    tick.state_mut().insert(ids.selected, *entity, rest)?;
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
