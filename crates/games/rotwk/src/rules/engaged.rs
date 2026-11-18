use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Engaged {
    until: Frame,
}

#[cfg(test)]
impl Engaged {
    pub(crate) fn later(self) -> Self {
        Self {
            until: self.until.next().unwrap_or(self.until),
        }
    }
}

impl Component for Engaged {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.until.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct EngagedDefaults {
    pub(crate) max_distance: f32,
    pub(crate) timeout_ms: f32,
}

impl RotwkRules {
    pub(in crate::rules) fn has_ai(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world.get(ids.kind, entity).is_some_and(|kind| {
            self.ai_kinds
                .get(usize::from(kind.0))
                .copied()
                .unwrap_or(false)
        })
    }

    fn engaged_tuning(&self, world: &World, ids: Ids, entity: Entity) -> (f32, LogicFrames) {
        let record = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| &entry.record);
        let max = record
            .and_then(|r| r.max_distance_for_engaged)
            .unwrap_or(self.engaged_defaults.max_distance);
        let ms = record
            .and_then(|r| r.engaged_state_timeout_ms)
            .unwrap_or(self.engaged_defaults.timeout_ms);
        (max, crate::game_data::retail_duration(ms))
    }

    fn attacking_far(&self, world: &World, ids: Ids, entity: Entity, now: Frame) -> bool {
        let firing = |weapon| self.weapon_firing_duration(weapon);
        if crate::weapon_use::attack_of(world, ids, entity, now, &firing)
            == crate::weapon_use::Attack::None
        {
            return false;
        }
        let goal = world
            .get(ids.armed, entity)
            .and_then(|armed| armed.target)
            .or_else(|| {
                world
                    .get(ids.attack_order, entity)
                    .map(|order| order.target)
            });
        let (Some(goal), Some(body)) = (goal, world.get(ids.body, entity)) else {
            return false;
        };
        let Some(target) = world.get(ids.body, goal) else {
            return false;
        };
        let (max, _) = self.engaged_tuning(world, ids, entity);
        target.0.position.minus(body.0.position).length() > max
    }

    pub(super) fn step_engaged(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let frame = tick.frame();
        let now = frame.next().unwrap_or(frame);
        let world = tick.state();
        let mut candidates: Vec<Entity> = world
            .store(ids.armed)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        if let Some(store) = world.store(ids.engaged) {
            candidates.extend_from_slice(store.entities());
        }
        candidates.sort_unstable_by_key(|e| e.index());
        candidates.dedup();
        let firing = |weapon| self.weapon_firing_duration(weapon);
        for entity in candidates {
            let world = tick.state();
            let held = world.get(ids.engaged, entity).copied();
            if world.contains(ids.doomed, entity)
                || !world.is_live(entity)
                || !self.has_ai(world, ids, entity)
            {
                if held.is_some() {
                    tick.state_mut().remove(ids.engaged, entity);
                }
                continue;
            }
            let using = crate::weapon_use::using_weapon(world, ids, entity, now, &firing);
            let Some(held) = held else {
                if using {
                    tick.state_mut()
                        .insert(ids.engaged, entity, Engaged { until: frame })?;
                }
                continue;
            };
            let horde = world.get(ids.member, entity).map(|member| member.horde);
            let keep = using
                || self.attacking_far(world, ids, entity, now)
                || horde.is_some_and(|horde| self.attacking_far(world, ids, horde, now));
            if keep {
                let (_, timeout) = self.engaged_tuning(world, ids, entity);
                tick.state_mut().insert(
                    ids.engaged,
                    entity,
                    Engaged {
                        until: frame.checked_add(timeout).unwrap_or(frame),
                    },
                )?;
            } else if frame >= held.until {
                tick.state_mut().remove(ids.engaged, entity);
            }
        }
        Ok(())
    }
}
