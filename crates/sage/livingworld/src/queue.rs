use crate::model::{Event, PlayerId, PlotBuilding, RegionId, Rejected, UnitId};
use crate::rules::{QueueRules, QueuedArmy};
use crate::state::LivingWorldState;

impl LivingWorldState {
    fn plot_at(&self, region: RegionId, plot: u8) -> Option<&PlotBuilding> {
        self.region(region)?
            .plots
            .get(usize::from(plot))?
            .as_ref()
            .filter(|p| !p.under_construction)
    }

    fn plot_at_mut(&mut self, region: RegionId, plot: u8) -> Option<&mut PlotBuilding> {
        self.region_mut(region)?
            .plots
            .get_mut(usize::from(plot))?
            .as_mut()
            .filter(|p| !p.under_construction)
    }

    fn queue_rules(&self, region: RegionId, plot: u8) -> Result<QueueRules, Rejected> {
        let building = self
            .plot_at(region, plot)
            .ok_or(Rejected::NoQueue)?
            .building;
        self.rules
            .building(building)
            .and_then(|b| b.queue.clone())
            .ok_or(Rejected::NoQueue)
    }

    fn entry_cp(&self, entry: &QueuedArmy) -> i32 {
        entry.roster.iter().fold(0i32, |sum, (unit, count)| {
            let each = self.rules.unit(*unit).command_points;
            sum.saturating_add(each.saturating_mul(i32::try_from(*count).unwrap_or(i32::MAX)))
        })
    }

    fn entry_price(&self, owner: PlayerId, entry: &QueuedArmy) -> i32 {
        entry
            .roster
            .first()
            .map_or(0, |(unit, _)| self.unit_price(owner, *unit))
    }

    pub(crate) fn queued_hero(&self, entry: &QueuedArmy) -> Option<UnitId> {
        entry.hero.or_else(|| {
            entry
                .roster
                .first()
                .map(|(unit, _)| *unit)
                .filter(|unit| self.rules.unit(*unit).hero)
        })
    }

    pub(crate) fn hero_fielded(&self, player: PlayerId, hero: UnitId) -> bool {
        self.armies_of(player).any(|a| a.hero == Some(hero))
    }

    #[cfg(test)]
    pub(crate) fn hero_fallen(&self, player: PlayerId, hero: UnitId) -> bool {
        self.player(player)
            .is_some_and(|p| p.fallen_heroes.iter().any(|f| f.unit == hero))
    }

    pub(crate) fn hero_recruitable(&self, player: PlayerId, hero: UnitId) -> bool {
        !self.hero_fielded(player, hero) && !self.hero_queued(player, hero)
    }

    pub(crate) fn hero_queued(&self, owner: PlayerId, hero: UnitId) -> bool {
        self.regions
            .iter()
            .filter(|region| region.owner == Some(owner))
            .flat_map(|region| region.plots.iter().flatten())
            .filter(|plot| !plot.under_construction)
            .any(|plot| {
                let Some(queue) = self
                    .rules
                    .building(plot.building)
                    .and_then(|b| b.queue.as_ref())
                else {
                    return false;
                };
                plot.queue.iter().any(|entry| {
                    queue
                        .entries
                        .get(usize::from(*entry))
                        .is_some_and(|army| self.queued_hero(army) == Some(hero))
                })
            })
    }

    fn release_cp(&mut self, owner: PlayerId, cp: i32) {
        if let Some(p) = self.player_mut(owner) {
            p.reserved_cp = p.reserved_cp.saturating_sub(cp).max(0);
        }
    }

    pub(crate) fn order_queue(
        &mut self,
        player: PlayerId,
        region: RegionId,
        plot: u8,
        entry: u8,
    ) -> Result<Vec<Event>, Rejected> {
        let queue = self.queue_rules(region, plot)?;
        let owner = self.owner(region);
        if owner.is_some_and(|o| o != player) {
            return Err(Rejected::NotYourRegion);
        }
        let held = self.plot_at(region, plot).map_or(0, |p| p.queue.len());
        if i64::try_from(held).unwrap_or(i64::MAX) >= i64::from(queue.size) {
            return Err(Rejected::QueueFull);
        }
        let army = queue
            .entries
            .get(usize::from(entry))
            .ok_or(Rejected::NoSuchEntry)?;
        army.roster.first().ok_or(Rejected::NoSuchEntry)?;
        if let Some(hero) = self.queued_hero(army) {
            if self.hero_fielded(player, hero) {
                return Err(Rejected::HeroFielded);
            }
            if self.hero_queued(owner.unwrap_or(player), hero) {
                return Err(Rejected::HeroQueued);
            }
        }
        let charge = owner.map(|o| (o, self.entry_price(o, army), self.entry_cp(army)));
        if let Some((o, price, _)) = charge {
            if self.player(o).is_none_or(|p| p.resources < price) {
                return Err(Rejected::CannotAfford);
            }
        }
        if let Some(p) = self.plot_at_mut(region, plot) {
            if p.queue.is_empty() {
                p.queue_turns = 0;
            }
            p.queue.push(entry);
        }
        if let Some((o, price, cp)) = charge {
            if let Some(p) = self.player_mut(o) {
                p.reserved_cp = p.reserved_cp.saturating_add(cp);
            }
            self.add_resources(o, price.saturating_neg());
        }
        Ok(vec![Event::ArmyQueued { region, plot }])
    }

    pub(crate) fn order_cancel_queued(
        &mut self,
        player: PlayerId,
        region: RegionId,
        plot: u8,
        index: u8,
    ) -> Result<Vec<Event>, Rejected> {
        let queue = self.queue_rules(region, plot)?;
        let owner = self.owner(region);
        if owner.is_some_and(|o| o != player) {
            return Err(Rejected::NotYourRegion);
        }
        let index = usize::from(index);
        let id = self
            .plot_at(region, plot)
            .and_then(|p| p.queue.get(index).copied())
            .ok_or(Rejected::NoSuchEntry)?;
        if let Some(p) = self.plot_at_mut(region, plot) {
            if index == 0 {
                p.queue_turns = 0;
            }
            p.queue.remove(index);
        }
        let mut refund = 0;
        if let (Some(o), Some(army)) = (owner, queue.entries.get(usize::from(id))) {
            refund = self.entry_price(o, army);
            self.add_resources(o, refund);
            let cp = self.entry_cp(army);
            self.release_cp(o, cp);
        }
        Ok(vec![Event::QueueCancelled {
            region,
            plot,
            refund,
        }])
    }

    pub(crate) fn queue_tick(&mut self, region: RegionId, events: &mut Vec<Event>) {
        let plots = self.region(region).map_or(0, |r| r.plots.len());
        for index in 0..plots {
            let Ok(plot) = u8::try_from(index) else { break };
            let Some(head) = self
                .plot_at(region, plot)
                .and_then(|p| p.queue.first().copied())
            else {
                continue;
            };
            let Ok(queue) = self.queue_rules(region, plot) else {
                continue;
            };
            let Some(entry) = queue.entries.get(usize::from(head)) else {
                if let Some(p) = self.plot_at_mut(region, plot) {
                    p.queue.remove(0);
                    p.queue_turns = 0;
                }
                continue;
            };
            let turns = match self.plot_at_mut(region, plot) {
                Some(p) => {
                    p.queue_turns = p.queue_turns.saturating_add(1);
                    p.queue_turns
                }
                None => continue,
            };
            let needed = entry.build_time;
            if i64::from(turns) < i64::from(needed) {
                continue;
            }
            let Some(owner) = self.owner(region) else {
                continue;
            };
            if self
                .queued_hero(entry)
                .is_some_and(|hero| self.hero_fielded(owner, hero))
            {
                continue;
            }
            let cp = self.entry_cp(entry);
            let present = self
                .armies_of(owner)
                .find(|a| !a.is_hero_army() && a.region == region)
                .map_or(0, |a| self.army_cp(a));
            let limit = self.rules.region_rules(region).map_or(0, |r| r.cp_limit);
            if present.saturating_add(cp) > limit || entry.is_city {
                continue;
            }
            if let Some(hero) = self.queued_hero(entry) {
                if let Some(p) = self.player_mut(owner) {
                    p.fallen_heroes.retain(|fallen| fallen.unit != hero);
                }
            }
            let army = self.spawn_merging(owner, region, entry.hero, &entry.roster);
            events.push(Event::ArmySpawned { army, region });
            if let Some(p) = self.plot_at_mut(region, plot) {
                p.queue.remove(0);
                p.queue_turns = 0;
            }
            self.release_cp(owner, cp);
        }
    }
}
