use mint_sage_modules::effects::Effect;
use mint_sim::{Entity, Result, Tick};

use super::{Ids, RotwkRules};
use crate::fx::FxSource;

const DISGUISED: &str = "DISGUISED";
const MOUNTED: &str = "MOUNTED";

impl RotwkRules {
    pub(super) fn apply_toggle_disguise(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        force_mounted: bool,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let was_disguised = tick
            .state()
            .get(wiring.state, entity)
            .is_some_and(|state| state.has_model_condition(DISGUISED));
        self.set_model_condition(tick, ids, entity, DISGUISED, !was_disguised)?;
        self.stamp_creation(tick, ids, entity)?;
        crate::diag::diag!(
            "disguise: entity {} -> DISGUISED {}",
            entity.index(),
            if was_disguised { "cleared" } else { "set" }
        );
        if !was_disguised && force_mounted {
            let already_mounted = tick
                .state()
                .get(wiring.state, entity)
                .is_some_and(|state| state.has_model_condition(MOUNTED));
            if !already_mounted {
                self.set_model_condition(tick, ids, entity, MOUNTED, true)?;
            }
        }
        Ok(())
    }

    pub(super) fn break_disguise_at_unpack(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        power: &str,
    ) -> Result<()> {
        let cancels = self
            .power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .is_some_and(|rule| rule.smite_cancels_disguise);
        if cancels {
            self.break_disguise(tick, ids, entity, true)?;
        }
        Ok(())
    }

    pub(super) fn break_disguise_if_power_cancels(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        power: &str,
    ) -> Result<()> {
        let cancels = self
            .power_names
            .id(power)
            .and_then(|id| self.power_cast_rules.get(usize::from(id.get())))
            .is_some_and(|rule| rule.cancels_disguise);
        if cancels {
            self.break_disguise(tick, ids, entity, true)?;
        }
        Ok(())
    }

    pub(super) fn break_disguise(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        with_fx: bool,
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let disguised = tick
            .state()
            .get(wiring.state, entity)
            .is_some_and(|state| state.has_model_condition(DISGUISED));
        if !disguised {
            return Ok(());
        }
        self.set_model_condition(tick, ids, entity, DISGUISED, false)?;
        self.stamp_creation(tick, ids, entity)?;
        crate::diag::diag!(
            "disguise: entity {} -> DISGUISED cleared (break)",
            entity.index()
        );
        if with_fx {
            if let Some(list) = wiring.disguise_fx(tick.state(), entity).map(str::to_owned) {
                if let Some(at) = tick
                    .state()
                    .get(ids.body, entity)
                    .map(|body| body.0.position)
                {
                    self.emit_fx_object_named(tick, ids, &list, at, FxSource::Power, entity, None);
                }
            }
        }
        Ok(())
    }

    pub(super) fn reveal_disguised_climber(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let disguised = self.modules.as_ref().is_some_and(|wiring| {
            tick.state()
                .get(wiring.state, entity)
                .is_some_and(|state| state.has_model_condition(DISGUISED))
        });
        let detected = tick
            .state()
            .get(ids.stealth, entity)
            .is_some_and(|state| state.detected_at(tick.frame()));
        if !disguised || detected {
            return Ok(());
        }
        self.break_disguise(tick, ids, entity, false)?;
        if tick.state().contains(ids.stealth, entity) {
            self.apply_lose_stealth_trigger(tick, ids, entity)?;
        }
        Ok(())
    }

    pub(super) fn set_model_condition(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        name: &str,
        set: bool,
    ) -> Result<()> {
        let effect = if set {
            Effect::ModelConditions {
                add: mint_sage_modules::effects::names(vec![name.to_owned()]),
                remove: mint_sage_modules::effects::names(Vec::new()),
            }
        } else {
            Effect::ModelConditions {
                add: mint_sage_modules::effects::names(Vec::new()),
                remove: mint_sage_modules::effects::names(vec![name.to_owned()]),
            }
        };
        self.apply_effects(tick, ids, vec![(entity, effect)])
    }
}
