use mint_sim::StateHasher;

use super::*;
use crate::defeat::ObjectFilter;

const MODULE: &str = "PillageModule";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PillageCount(pub(crate) u32);

impl Component for PillageCount {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.0);
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn whole(module: &mint_sage_modules::CapturedModule, keyword: &str) -> Option<u32> {
    module
        .field(keyword)?
        .args
        .first()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.min(f64::from(u32::MAX)) as u32)
}

impl RotwkRules {
    pub(super) fn pillage(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Result<()> {
        let world = tick.state();
        let Some(module) = world
            .get(ids.kind, attacker)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| {
                entry
                    .record
                    .declared_modules
                    .iter()
                    .find(|module| module.name.eq_ignore_ascii_case(MODULE))
            })
        else {
            return Ok(());
        };
        let passes = module.field("PillageFilter").is_none_or(|field| {
            world
                .get(ids.kind, victim)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| {
                    ObjectFilter::parse(&field.args)
                        .passes(&entry.record.kind_of, &entry.record.name)
                })
        });
        let (Some(payer), Some(payee)) = (
            world.get(ids.owner, victim).map(|o| o.0),
            world.get(ids.owner, attacker).map(|o| o.0),
        ) else {
            return Ok(());
        };
        if !passes {
            return Ok(());
        }
        let every = whole(module, "NumDamageEventsPerPillage").unwrap_or(0);
        let amount = i64::from(whole(module, "PillageAmount").unwrap_or(0));
        let count = world
            .get(ids.pillage_count, attacker)
            .map_or(0, |held| held.0)
            .saturating_add(1);
        if count < every {
            tick.state_mut()
                .insert(ids.pillage_count, attacker, PillageCount(count))?;
            return Ok(());
        }
        tick.state_mut()
            .insert(ids.pillage_count, attacker, PillageCount(0))?;
        let economy = Self::economy_ids(ids);
        let (Some(payer_entity), Some(payee_entity)) = (
            crate::economy::player_entity(tick.state(), economy, payer),
            crate::economy::player_entity(tick.state(), economy, payee),
        ) else {
            return Ok(());
        };
        if payer_entity == payee_entity {
            return Ok(());
        }
        let mut purse = tick
            .state()
            .get(ids.purse, payer_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        let taken = purse.withdraw(amount);
        tick.state_mut()
            .insert(ids.purse, payer_entity, crate::economy::Purse(purse))?;
        let mut purse = tick
            .state()
            .get(ids.purse, payee_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        purse.deposit(taken);
        tick.state_mut()
            .insert(ids.purse, payee_entity, crate::economy::Purse(purse))?;
        Ok(())
    }
}
