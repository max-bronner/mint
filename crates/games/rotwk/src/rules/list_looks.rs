use mint_sage_combat::ModifierId;
use mint_sage_modules::effects::{names, Effect};

use super::*;

impl RotwkRules {
    pub(super) fn list_given(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        id: ModifierId,
        until: Frame,
        new: bool,
    ) -> Result<()> {
        let Some(list) = self.combat.modifiers.get(id) else {
            return Ok(());
        };
        let now = tick.frame();
        if new {
            if !list.model_condition.is_empty() {
                self.set_list_conditions(tick, ids, entity, &list.model_condition, &[])?;
            }
            if !list.clear_model_condition.is_empty() {
                self.set_list_conditions(tick, ids, entity, &[], &list.clear_model_condition)?;
            }
            if let Some(upgrade) = &list.upgrade {
                if upgrade.delay_ms == 0 {
                    self.give_list_upgrade(tick, ids, entity, &upgrade.name)?;
                } else {
                    let at = now
                        .checked_add(mint_sage_modules::retail_duration_ticks(upgrade.delay_ms))
                        .unwrap_or(Frame::FOREVER);
                    if let Some(timed) = tick.state_mut().get_mut(ids.timed_modifiers, entity) {
                        timed.1.push((id, at));
                    }
                }
            }
        }
        let held = tick
            .state()
            .get(ids.timed_modifiers, entity)
            .map_or(0, |timed| {
                self.held_in_category(&timed.0, list.category.as_deref())
            });
        let counter = if new { held.saturating_sub(1) } else { held };
        let fx = if until > now {
            Some(&list.fx)
        } else if !new {
            Some(&list.end_fx)
        } else {
            None
        };
        if let Some(fx) = fx {
            self.play_list_fx(tick, ids, entity, fx, list.multi_level_fx, counter);
        }
        Ok(())
    }

    pub(super) fn held_in_category(
        &self,
        lists: &[(ModifierId, Frame)],
        category: Option<&str>,
    ) -> usize {
        lists
            .iter()
            .filter(|(held, _)| {
                let other = self
                    .combat
                    .modifiers
                    .get(*held)
                    .and_then(|list| list.category.as_deref());
                match (other, category) {
                    (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                    (None, None) => true,
                    _ => false,
                }
            })
            .count()
    }

    pub(super) fn list_ended(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        id: ModifierId,
        counter: usize,
    ) -> Result<()> {
        let Some(list) = self.combat.modifiers.get(id) else {
            return Ok(());
        };
        if !list.model_condition.is_empty() {
            self.set_list_conditions(tick, ids, entity, &[], &list.model_condition)?;
            if self.map_snowy
                && list
                    .model_condition
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case("SNOW"))
            {
                self.set_list_conditions(tick, ids, entity, &["SNOW".to_owned()], &[])?;
            }
        }
        self.play_list_fx(
            tick,
            ids,
            entity,
            &list.end_fx,
            list.multi_level_fx,
            counter,
        );
        Ok(())
    }

    pub(super) fn give_due_list_upgrades(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        now: Frame,
    ) -> Result<()> {
        let Some(timed) = tick.state_mut().get_mut(ids.timed_modifiers, entity) else {
            return Ok(());
        };
        let due: Vec<ModifierId> = timed
            .1
            .iter()
            .filter(|(_, at)| *at <= now)
            .map(|(list, _)| *list)
            .collect();
        timed.1.retain(|(_, at)| *at > now);
        for id in due {
            if let Some(upgrade) = self
                .combat
                .modifiers
                .get(id)
                .and_then(|list| list.upgrade.as_ref())
            {
                self.give_list_upgrade(tick, ids, entity, &upgrade.name)?;
            }
        }
        Ok(())
    }

    fn set_list_conditions(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        self.apply_effects(
            tick,
            ids,
            vec![(
                entity,
                Effect::ModelConditions {
                    add: names(add.to_vec()),
                    remove: names(remove.to_vec()),
                },
            )],
        )
    }

    fn give_list_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
    ) -> Result<()> {
        match self.upgrade_bits.bit(name) {
            Some(bit) if !self.upgrade_bits.is_player(bit) => {
                self.grant_object_upgrade(tick, ids, entity, bit)
            }
            _ => {
                diag!(
                    "power: ModifierList Upgrade {name} is not an object upgrade this match knows"
                );
                Ok(())
            }
        }
    }

    fn play_list_fx(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        fx: &[Option<String>; 3],
        multi_level: bool,
        counter: usize,
    ) {
        let world = tick.state();
        if world.contains(ids.horde, entity) {
            return;
        }
        let index = if multi_level {
            counter.saturating_sub(1).min(2)
        } else {
            0
        };
        let Some(name) = fx
            .get(index)
            .and_then(Option::as_ref)
            .or(fx.first().and_then(Option::as_ref))
        else {
            return;
        };
        let Some(at) = world.get(ids.body, entity).map(|body| body.0.position) else {
            return;
        };
        let power = crate::fx::FxSource::Power;
        self.emit_fx_object_named(tick, ids, name, at, power, entity, None);
    }
}
