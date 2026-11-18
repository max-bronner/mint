use mint_sage_combat::ModifierId;
use mint_sage_modules::effects::{Name, Names};
use mint_sage_modules::{PowerSides, WeatherKind};
use mint_sim::{Entity, Frame, LogicFrames, PlayerId, Result, Tick};

use super::{spell_effects::filter_of, Ids, RotwkRules};
use crate::defeat::ObjectFilter;
use crate::diag::diag;
use crate::spell_state::{Weather, WeatherEffect};

impl RotwkRules {
    fn weather_admits(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        entity: Entity,
        caster: PlayerId,
        filter: &ObjectFilter,
        affect_good: bool,
        affect_evil: bool,
    ) -> bool {
        if !self.admits(world, ids, entity, caster, filter, PowerSides::All) {
            return false;
        }
        let evil = world
            .get(ids.owner, entity)
            .is_some_and(|owner| self.is_evil_owner(owner.0));
        !((affect_good && evil) || (affect_evil && !evil))
    }

    fn give_weather_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        list: Option<ModifierId>,
        anti: &Names,
    ) -> Result<()> {
        if let Some(id) = list {
            let until = self
                .combat
                .modifiers
                .get(id)
                .and_then(|list| list.duration_ms)
                .filter(|ms| *ms > 0)
                .map_or(Frame::FOREVER, |ms| {
                    tick.frame()
                        .checked_add(LogicFrames::from_milliseconds(ms))
                        .unwrap_or(Frame::FOREVER)
                });
            self.give_timed_list(tick, ids, entity, id, until)?;
        }
        if !anti.is_empty() {
            let mut suppressed = tick
                .state()
                .get(ids.suppressed, entity)
                .cloned()
                .unwrap_or_default();
            for category in anti.iter() {
                suppressed.add(category, Frame::FOREVER);
            }
            tick.state_mut()
                .insert(ids.suppressed, entity, suppressed)?;
            self.recompute_bonuses(tick, ids, entity)?;
        }
        Ok(())
    }

    fn take_weather_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        list: Option<ModifierId>,
        anti: &Names,
    ) -> Result<()> {
        if let Some(id) = list {
            if let Some(mut timed) = tick.state().get(ids.timed_modifiers, entity).cloned() {
                if timed.remove(id) {
                    if timed.0.is_empty() {
                        tick.state_mut().remove(ids.timed_modifiers, entity);
                    } else {
                        tick.state_mut()
                            .insert(ids.timed_modifiers, entity, timed)?;
                    }
                }
            }
        }
        if !anti.is_empty() {
            if let Some(mut suppressed) = tick.state().get(ids.suppressed, entity).cloned() {
                let mut changed = false;
                for category in anti.iter() {
                    changed |= suppressed.remove(category);
                }
                if changed {
                    if suppressed.0.is_empty() {
                        tick.state_mut().remove(ids.suppressed, entity);
                    } else {
                        tick.state_mut()
                            .insert(ids.suppressed, entity, suppressed)?;
                    }
                }
            }
        }
        self.recompute_bonuses(tick, ids, entity)?;
        Ok(())
    }

    pub(super) fn clear_weather_effect(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<usize> {
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(0);
        };
        let Some(effect) = tick.state().get(ids.weather_effect, clock).cloned() else {
            return Ok(0);
        };
        for &entity in &effect.recipients {
            self.take_weather_effect(tick, ids, entity, effect.list, &effect.anti)?;
        }
        tick.state_mut().remove(ids.weather_effect, clock);
        Ok(effect.recipients.len())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn set_weather_effect(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        owner: PlayerId,
        list: Option<&Name>,
        anti: &Names,
        affects: &Names,
        affect_good: bool,
        affect_evil: bool,
        lasts: LogicFrames,
    ) -> Result<usize> {
        let lasts = Self::weather_timer(lasts);
        self.clear_weather_effect(tick, ids)?;
        let list_id = match list {
            Some(name) => match self.combat.modifiers.id(name) {
                Some(id) => Some(id),
                None => {
                    diag!("power: ModifierList {name} is not declared");
                    None
                }
            },
            None => None,
        };
        if list_id.is_none() && anti.is_empty() {
            return Ok(0);
        }
        let filter = filter_of(affects);
        let world = tick.state();
        let mut recipients: Vec<Entity> = world
            .store(ids.kind)
            .map(|store| {
                store
                    .entities()
                    .iter()
                    .copied()
                    .filter(|entity| {
                        self.weather_admits(
                            world,
                            ids,
                            *entity,
                            owner,
                            &filter,
                            affect_good,
                            affect_evil,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        recipients.sort_unstable_by_key(|entity| entity.index());
        for &entity in &recipients {
            self.give_weather_effect(tick, ids, entity, list_id, anti)?;
        }
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(recipients.len());
        };
        tick.state_mut().insert(
            ids.weather_effect,
            clock,
            WeatherEffect {
                list: list_id,
                anti: Names::clone(anti),
                affects: Names::clone(affects),
                affect_good,
                affect_evil,
                caster: owner,
                timer: lasts,
                recipients: recipients.clone(),
            },
        )?;
        Ok(recipients.len())
    }

    pub(super) fn apply_weather_to_new_object(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(());
        };
        let Some(effect) = tick.state().get(ids.weather_effect, clock).cloned() else {
            return Ok(());
        };
        if effect.recipients.contains(&entity) {
            return Ok(());
        }
        let filter = filter_of(&effect.affects);
        if !self.weather_admits(
            tick.state(),
            ids,
            entity,
            effect.caster,
            &filter,
            effect.affect_good,
            effect.affect_evil,
        ) {
            return Ok(());
        }
        self.give_weather_effect(tick, ids, entity, effect.list, &effect.anti)?;
        if let Some(mut updated) = tick.state().get(ids.weather_effect, clock).cloned() {
            updated.recipients.push(entity);
            tick.state_mut()
                .insert(ids.weather_effect, clock, updated)?;
        }
        Ok(())
    }

    fn weather_timer(lasts: LogicFrames) -> LogicFrames {
        if lasts.is_zero() {
            LogicFrames::MAX
        } else {
            lasts
        }
    }

    pub(super) fn set_weather_kind(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        kind: WeatherKind,
        lasts: LogicFrames,
    ) -> Result<()> {
        let lasts = Self::weather_timer(lasts);
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(());
        };
        let current = tick
            .state()
            .get(ids.weather, clock)
            .copied()
            .unwrap_or_default();
        if current.kind == kind {
            diag!("power: weather is already {kind:?} - its timer is not refreshed");
            return Ok(());
        }
        tick.state_mut()
            .insert(ids.weather, clock, Weather { kind, timer: lasts })?;
        Ok(())
    }

    pub(super) fn fear_everyone_admitted(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        caster: Entity,
        owner: PlayerId,
        affects: &Names,
    ) -> Result<usize> {
        let filter = filter_of(affects);
        let targets = {
            let world = tick.state();
            world
                .store(ids.kind)
                .map(|store| {
                    store
                        .entities()
                        .iter()
                        .copied()
                        .filter(|entity| {
                            self.admits(world, ids, *entity, owner, &filter, PowerSides::All)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        for &entity in &targets {
            self.pulse_emotion(
                tick,
                ids,
                entity,
                mint_sage_ini::EmotionType::UncontrollableFear,
                Some(caster),
                LogicFrames::ONE,
            )?;
        }
        Ok(targets.len())
    }

    pub(crate) fn weather_kind(
        world: &mint_sim::World,
        ids: Ids,
    ) -> mint_sage_modules::WeatherKind {
        crate::clock_entity(world, ids)
            .and_then(|clock| world.get(ids.weather, clock))
            .map_or(mint_sage_modules::WeatherKind::None, |weather| weather.kind)
    }

    pub(super) fn expire_weather(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(clock) = crate::clock_entity(tick.state(), ids) else {
            return Ok(());
        };
        if let Some(mut weather) = tick.state().get(ids.weather, clock).copied() {
            weather.timer = weather.timer.saturating_sub(LogicFrames::ONE);
            if weather.timer.is_zero() {
                tick.state_mut().remove(ids.weather, clock);
            } else {
                tick.state_mut().insert(ids.weather, clock, weather)?;
            }
        }
        let expired = tick
            .state()
            .get(ids.weather_effect, clock)
            .is_some_and(|effect| effect.timer.saturating_sub(LogicFrames::ONE).is_zero());
        if expired {
            self.clear_weather_effect(tick, ids)?;
        } else if let Some(mut effect) = tick.state().get(ids.weather_effect, clock).cloned() {
            effect.timer = effect.timer.saturating_sub(LogicFrames::ONE);
            let world = tick.state();
            effect.recipients.retain(|&entity| world.is_live(entity));
            tick.state_mut().insert(ids.weather_effect, clock, effect)?;
        }
        Ok(())
    }
}
