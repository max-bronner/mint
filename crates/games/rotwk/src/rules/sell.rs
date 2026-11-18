use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{Ids, RotwkRules};
use crate::construction::sell_value;
use crate::diag::diag;
use crate::economy::Purse;

impl RotwkRules {
    pub(crate) const fn with_sell_percentage(mut self, percent: f32) -> Self {
        self.sell_percentage = percent;
        self
    }

    pub(crate) fn sell_refusal(
        &self,
        world: &mint_sim::World,
        ids: Ids,
        player: PlayerId,
        structure: Entity,
    ) -> Option<&'static str> {
        if world.get(ids.owner, structure).map(|owner| owner.0) != Some(player) {
            return Some("not the seller's");
        }
        if !world.is_live(structure) || world.contains(ids.doomed, structure) {
            return Some("dead");
        }
        let is_structure = world
            .get(ids.kind, structure)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| entry.record.has_kind("STRUCTURE"));
        if !is_structure {
            return Some("not a structure");
        }
        if world.contains(ids.construction, structure) {
            return Some("still under construction - that is a cancel");
        }
        if self.script_status_bit(
            world,
            ids,
            structure,
            super::map_scripts::ScriptStatusBit::Unsellable,
        ) {
            return Some("unsellable (script status)");
        }
        None
    }

    pub(super) fn sell(&self, tick: &mut Tick<'_>, ids: Ids, player: PlayerId) -> Result<()> {
        let mut sold: Vec<(Entity, f32)> = Vec::new();
        for structure in self.selection(tick, ids, player) {
            if let Some(why) = self.sell_refusal(tick.state(), ids, player, structure) {
                diag!("sell: entity {} refused - {why}", structure.index());
                continue;
            }
            let refunded = self.cancel_all_production(tick, ids, structure, player)?;
            tick.state_mut().remove(ids.selected, structure);
            tick.state_mut().remove(ids.attack_order, structure);
            if let Some(armed) = tick.state_mut().get_mut(ids.armed, structure) {
                armed.retarget(None);
            }
            let record = tick
                .state()
                .get(ids.kind, structure)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| entry.record.clone());
            let value = record.map_or(0, |record| {
                let modifiers = self.cost_modifiers_for_record(tick.state(), ids, player, &record);
                sell_value(&record, self.sell_percentage, &modifiers)
            });
            if let Some(player_entity) =
                crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
            {
                let mut purse = tick
                    .state()
                    .get(ids.purse, player_entity)
                    .map_or(mint_sage_economy::Money::new(0), |held| held.0);
                purse.deposit(value);
                tick.state_mut()
                    .insert(ids.purse, player_entity, Purse(purse))?;
            }
            if let Some(mut health) = tick.state().get(ids.health, structure).copied() {
                health.0.set_current(0.0);
                tick.state_mut().insert(ids.health, structure, health)?;
            }
            diag!(
                "sell: entity {} sold by player {} - {value} refunded{}",
                structure.index(),
                player.get(),
                if refunded > 0 {
                    format!(", {refunded} more for its production")
                } else {
                    String::new()
                }
            );
            tick.state_mut()
                .insert(ids.sold, structure, super::rebuild::Sold)?;
            sold.push((structure, 0.0));
            for slave in self.slaves_to_destroy_with(tick.state(), ids, structure) {
                diag!(
                    "sell: entity {} removed with its sold master (entity {})",
                    slave.index(),
                    structure.index()
                );
                sold.push((slave, 0.0));
            }
        }
        self.resolve_deaths(tick, ids, &sold)
    }

    pub(super) fn cancel_all_production(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        structure: Entity,
        player: PlayerId,
    ) -> Result<i64> {
        let Some(player_entity) =
            crate::economy::player_entity(tick.state(), Self::economy_ids(ids), player)
        else {
            return Ok(0);
        };
        let mut purse = tick
            .state()
            .get(ids.purse, player_entity)
            .map_or(mint_sage_economy::Money::new(0), |held| held.0);
        let before = purse.amount();
        let mut heroes: Vec<u16> = Vec::new();
        let mut upgrades: Vec<u32> = Vec::new();
        if let Some(producer) = tick.state_mut().get_mut(ids.producer, structure) {
            for entry in producer.queue.entries() {
                match entry.kind {
                    mint_sage_economy::EntryKind::Revival { .. } => {
                        if let Ok(item) = u16::try_from(entry.item.index()) {
                            heroes.push(item);
                        }
                    }
                    mint_sage_economy::EntryKind::Upgrade => upgrades.push(entry.item.index()),
                    mint_sage_economy::EntryKind::Unit { .. } => {}
                }
            }
            producer.queue.cancel_all(&mut purse);
        }
        if let Some(horde) = tick.state().store(ids.production_stream).and_then(|store| {
            store
                .iter()
                .find(|(_, stream)| stream.source == structure)
                .map(|(entity, _)| entity)
        }) {
            self.finish_production_burst(tick, ids, horde, structure)?;
        }
        let refunded = purse.amount().saturating_sub(before);
        tick.state_mut()
            .insert(ids.purse, player_entity, Purse(purse))?;
        for item in heroes {
            self.hero_cancelled(tick, ids, structure, player, item)?;
        }
        self.drop_player_upgrades_in_progress(tick, ids, player_entity, &upgrades)?;
        Ok(refunded)
    }

    pub(super) fn drop_player_upgrades_in_progress(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player_entity: Entity,
        items: &[u32],
    ) -> Result<()> {
        let bits: Vec<mint_sim::UpgradeBit> = items
            .iter()
            .filter_map(|item| usize::try_from(*item).ok())
            .filter_map(|item| self.upgrade_buildables.get(item))
            .filter(|upgrade| upgrade.is_player)
            .map(|upgrade| upgrade.bit)
            .collect();
        if bits.is_empty() {
            return Ok(());
        }
        if let Some(mut in_progress) = tick
            .state()
            .get(ids.player_upgrades_in_progress, player_entity)
            .cloned()
        {
            in_progress.0.retain(|held| !bits.contains(held));
            tick.state_mut()
                .insert(ids.player_upgrades_in_progress, player_entity, in_progress)?;
        }
        Ok(())
    }
}
