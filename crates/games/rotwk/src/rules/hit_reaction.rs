use mint_sage_modules::{hit_level_condition, Effect, HIT_REACTION_CONDITION};
use mint_sim::{Component, Entity, Frame, LogicFrames, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};
use crate::diag::diag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecialConditions {
    names: Vec<String>,
    ends: Frame,
}

impl Component for SpecialConditions {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.names.len());
        for name in &self.names {
            hasher.write_bytes(name.as_bytes());
        }
        hasher.write_u64(self.ends.get());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HitReaction {
    pub(crate) last_health: f32,
    pub(crate) level: u8,
    pub(crate) ends: Frame,
}

impl Component for HitReaction {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.last_health.to_bits());
        hasher.write_u8(self.level);
        hasher.write_u64(self.ends.get());
    }
}

impl RotwkRules {
    pub(super) fn step_hit_reactions(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let now = tick.frame();
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        let mut expired: Vec<(Entity, HitReaction)> = tick
            .state()
            .store(ids.hit_reaction)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, running)| running.level > 0 && now >= running.ends)
                    .map(|(entity, running)| (entity, *running))
                    .collect()
            })
            .unwrap_or_default();
        expired.sort_unstable_by_key(|(entity, _)| entity.index());
        for (entity, running) in expired {
            tick.state_mut().insert(
                ids.hit_reaction,
                entity,
                HitReaction {
                    level: 0,
                    ..running
                },
            )?;
            effects.push((entity, conditions(running.level, false)));
        }
        let ticks = u64::from(mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ).max(1);
        if now.get() % ticks == 0 {
            let mut holders: Vec<Entity> = tick
                .state()
                .store(wiring.hit_reaction_state)
                .map(|store| store.entities().to_vec())
                .unwrap_or_default();
            holders.sort_unstable_by_key(|entity| entity.index());
            for entity in holders {
                self.poll_hit_reaction(tick, ids, entity, &mut effects)?;
            }
        }
        for (entity, names) in self.expired_special_conditions(tick, ids) {
            tick.state_mut().remove(ids.special_conditions, entity);
            effects.push((entity, set_conditions(&names, false)));
        }
        if effects.is_empty() {
            return Ok(());
        }
        self.apply_effects(tick, ids, effects)
    }

    fn poll_hit_reaction(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        effects: &mut Vec<(Entity, Effect)>,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let (Some(module), Some(health)) = (
            tick.state()
                .get(wiring.hit_reaction_state, entity)
                .and_then(|column| column.0.first())
                .map(|slot| slot.state.clone()),
            tick.state().get(ids.health, entity).map(|health| health.0),
        ) else {
            return Ok(());
        };
        let now = tick.frame();
        let current = health.current();
        let polled = tick.state().get(ids.hit_reaction, entity).copied();
        if health.is_dead() {
            if let Some(running) = polled.filter(|running| running.level > 0) {
                effects.push((entity, conditions(running.level, false)));
            }
            tick.state_mut().remove(ids.hit_reaction, entity);
            return Ok(());
        }
        let mut state = polled.unwrap_or(HitReaction {
            last_health: current,
            level: 0,
            ends: now,
        });
        let idle = self.regular_state_idle(tick.state(), ids, entity);
        if state.level > 0 && !idle {
            effects.push((entity, conditions(state.level, false)));
            state.level = 0;
            state.ends = now;
        }
        let free = state.level == 0 || module.fast_hits_reset_reaction();
        let willing = module.hits_paralyze() || idle;
        let lost = state.last_health - current;
        if free && willing && lost > 0.0 {
            if let Some(level) = module.level_for(lost) {
                let lasts = module.timer(level);
                if !lasts.is_zero() {
                    if state.level > 0 && state.level != level {
                        effects.push((entity, conditions(state.level, false)));
                    }
                    effects.push((entity, conditions(level, true)));
                    state.level = level;
                    state.ends = now.checked_add(lasts).unwrap_or(Frame::FOREVER);
                    diag!(
                        "hit reaction: entity {} lost {:.1} this frame and wears level {} until frame {}",
                        entity.index(),
                        lost,
                        level,
                        state.ends.get(),
                    );
                }
            }
        }
        state.last_health = current;
        tick.state_mut().insert(ids.hit_reaction, entity, state)?;
        Ok(())
    }

    pub(super) fn wear_special_conditions(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        names: &[String],
        lasts: LogicFrames,
    ) -> Result<()> {
        if names.is_empty() || lasts.is_zero() {
            return Ok(());
        }
        let now = tick.frame();
        let ends = now.checked_add(lasts).unwrap_or(Frame::FOREVER);
        let mut effects: Vec<(Entity, Effect)> = Vec::new();
        if let Some(running) = tick.state().get(ids.special_conditions, victim).cloned() {
            effects.push((victim, set_conditions(&running.names, false)));
        }
        effects.push((victim, set_conditions(names, true)));
        tick.state_mut().insert(
            ids.special_conditions,
            victim,
            SpecialConditions {
                names: names.to_vec(),
                ends,
            },
        )?;
        diag!(
            "nugget: entity {} wears {} until frame {}",
            victim.index(),
            names.join(" "),
            ends.get()
        );
        self.apply_effects(tick, ids, effects)
    }

    fn expired_special_conditions(&self, tick: &Tick<'_>, ids: Ids) -> Vec<(Entity, Vec<String>)> {
        let now = tick.frame();
        let mut out: Vec<(Entity, Vec<String>)> = tick
            .state()
            .store(ids.special_conditions)
            .map(|store| {
                store
                    .iter()
                    .filter(|(_, worn)| now >= worn.ends)
                    .map(|(entity, worn)| (entity, worn.names.clone()))
                    .collect()
            })
            .unwrap_or_default();
        out.sort_unstable_by_key(|(entity, _)| entity.index());
        out
    }
}

fn set_conditions(names: &[String], starting: bool) -> Effect {
    let names = mint_sage_modules::effects::names(names.to_vec());
    let empty = mint_sage_modules::effects::names(Vec::new());
    if starting {
        Effect::ModelConditions {
            add: names,
            remove: empty,
        }
    } else {
        Effect::ModelConditions {
            add: empty,
            remove: names,
        }
    }
}

fn conditions(level: u8, starting: bool) -> Effect {
    let names = mint_sage_modules::effects::names(
        core::iter::once(HIT_REACTION_CONDITION.to_owned())
            .chain(hit_level_condition(level).map(str::to_owned))
            .collect::<Vec<String>>(),
    );
    if starting {
        Effect::ModelConditions {
            add: names,
            remove: mint_sage_modules::effects::names(Vec::new()),
        }
    } else {
        Effect::ModelConditions {
            add: mint_sage_modules::effects::names(Vec::new()),
            remove: names,
        }
    }
}
