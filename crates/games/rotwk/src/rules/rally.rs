use mint_sim::{Component, Entity, Frame, PlayerId, StateHasher};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RallyOutcome {
    Set,
    NoPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RallyEvent {
    pub(crate) player: PlayerId,
    pub(crate) structure: Entity,
    pub(crate) outcome: RallyOutcome,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RallyEvents {
    frame: Frame,
    events: Vec<RallyEvent>,
}

impl RallyEvents {
    fn push(&mut self, now: Frame, event: RallyEvent) {
        if self.frame != now {
            self.events.clear();
            self.frame = now;
        }
        self.events.push(event);
    }
}

impl Component for RallyEvents {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        hasher.write_usize(self.events.len());
        for event in &self.events {
            hasher.write_u8(event.player.get());
            hasher.write_u32(event.structure.index());
            hasher.write_u8(match event.outcome {
                RallyOutcome::Set => 0,
                RallyOutcome::NoPath => 1,
            });
        }
    }
}

const SLAUGHTER_REACH: f32 = 1.0;

const SLAUGHTER_CENTRE_RANGE: f32 = 15.0;

impl RotwkRules {
    pub(super) fn set_rally_point(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        structures: &[Entity],
        point: Vec2,
        global: bool,
        target: Option<Entity>,
    ) -> Result<()> {
        if global {
            let owned: Vec<Entity> = tick
                .state()
                .store(ids.producer)
                .map(|store| {
                    store
                        .iter()
                        .filter(|(entity, _)| {
                            tick.state()
                                .get(ids.owner, *entity)
                                .is_some_and(|owner| owner.0 == player)
                                && Self::is_auto_rallypoint(tick.state(), ids, *entity)
                        })
                        .map(|(entity, _)| entity)
                        .collect()
                })
                .unwrap_or_default();
            let mut stored = 0usize;
            for structure in owned {
                if self.do_set_rally_point(tick, ids, player, structure, point, None, false)? {
                    stored = stored.saturating_add(1);
                }
            }
            if stored > 0 {
                self.push_misc_sound(tick, ids, MiscSound::RallyPointSet, player, point);
            }
            return Ok(());
        }
        for &structure in structures {
            if !Self::is_auto_rallypoint(tick.state(), ids, structure)
                || tick
                    .state()
                    .get(ids.owner, structure)
                    .is_none_or(|owner| owner.0 != player)
            {
                continue;
            }
            self.do_set_rally_point(tick, ids, player, structure, point, target, true)?;
        }
        Ok(())
    }

    fn is_auto_rallypoint(world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.shape, entity)
            .is_some_and(|shape| shape.kind_of.contains(KindOf::AUTO_RALLYPOINT))
    }

    #[allow(clippy::too_many_arguments)]
    fn do_set_rally_point(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
        point: Vec2,
        target: Option<Entity>,
        feedback: bool,
    ) -> Result<bool> {
        if !tick.state().contains(ids.producer, structure) {
            return Ok(false);
        }
        let requested = match target {
            Some(target) => tick
                .state()
                .get(ids.body, target)
                .map(|body| body.0.position),
            None => Some(point),
        };
        let Some(requested) = requested else {
            return Ok(false);
        };
        if target.is_none() {
            let from = tick
                .state()
                .get(ids.body, structure)
                .map(|body| body.0.position)
                .unwrap_or(requested);
            if !self.rally_path_exists(from, requested) {
                self.push_rally_event(tick, ids, player, structure, RallyOutcome::NoPath);
                if feedback {
                    self.push_misc_sound(
                        tick,
                        ids,
                        MiscSound::UnableToSetRallyPoint,
                        player,
                        requested,
                    );
                }
                return Ok(false);
            }
        }
        let stored = self
            .find_rally_slaughter(tick.state(), ids, structure, player, requested)
            .unwrap_or(requested);
        let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) else {
            return Ok(false);
        };
        producer.player_rally = Some(stored);
        self.push_rally_event(tick, ids, player, structure, RallyOutcome::Set);
        if feedback {
            self.push_misc_sound(tick, ids, MiscSound::RallyPointSet, player, requested);
        }
        Ok(true)
    }

    fn push_rally_event(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
        outcome: RallyOutcome,
    ) {
        let now = tick.frame();
        let Some(store) = tick.state_mut().store_mut(ids.rally_events) else {
            return;
        };
        if let Some((_, events)) = store.iter_mut().next() {
            events.push(
                now,
                RallyEvent {
                    player,
                    structure,
                    outcome,
                },
            );
        }
    }

    pub(super) fn rally_path_exists(&self, from: Vec2, to: Vec2) -> bool {
        let (Some(start), Some(goal)) =
            (self.passability.cell_at(from), self.passability.cell_at(to))
        else {
            return false;
        };
        self.reachability.borrow().same_component(start, goal)
    }

    fn find_rally_slaughter(
        &self,
        world: &World,
        ids: Ids,
        structure: Entity,
        _: PlayerId,
        point: Vec2,
    ) -> Option<Vec2> {
        let allowed = world
            .get(ids.kind, structure)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.can_rally_to_slaughter);
        if !allowed {
            return None;
        }
        let bodies = world.store(ids.body)?;
        let mut best: Option<(f32, Vec2)> = None;
        for (candidate, body) in bodies.iter() {
            if candidate == structure
                || !(self.has_kind_bit(world, ids, candidate, "GARRISON")
                    || self.has_kind_bit(world, ids, candidate, "FS_CASH_PRODUCER"))
            {
                continue;
            }
            let Some(_) = world.get(ids.owner, candidate).map(|owner| owner.0) else {
                continue;
            };
            if !self.object_allied(world, structure, candidate) {
                continue;
            }
            if self
                .contain_view(world, candidate)
                .is_none_or(|view| view.cash_back <= 0.0)
            {
                continue;
            }
            let centre = body.0.position;
            let radius = world
                .get(ids.shape, candidate)
                .map_or(0.0, |shape| shape.radius);
            let distance = centre.minus(point).length();
            if distance - radius > SLAUGHTER_REACH || distance > SLAUGHTER_CENTRE_RANGE {
                continue;
            }
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, centre));
            }
        }
        best.map(|(_, centre)| centre)
    }
}
