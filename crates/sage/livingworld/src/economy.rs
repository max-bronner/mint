use crate::model::{bonus, BuildingId, Event, PlayerId, PlotBuilding, RegionId, Rejected};
use crate::state::LivingWorldState;
use mint_sage_ini::living_world::players::BuildingType;

#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn resource_gain(amount: i32, effective_bonus: i32) -> i32 {
    let scale = 1.0f32 + effective_bonus as f32 * 0.01f32;
    (amount as f32 * scale).ceil() as i32
}

#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn effective_resource_bonus(sum: i32, multiplier_limit: f32) -> i32 {
    let ceiling = (-100i32).saturating_sub((multiplier_limit * -100.0f32) as i32);
    sum.min(ceiling)
}

impl LivingWorldState {
    pub(crate) fn add_resources(&mut self, player: PlayerId, amount: i32) {
        let max = self
            .player(player)
            .and_then(|p| self.rules.templates.get(usize::from(p.template.0)))
            .map(|t| t.max_resources);
        if let (Some(max), Some(p)) = (max, self.player_mut(player)) {
            let total = p.resources.saturating_add(amount);
            p.resources = if total > max { max } else { total };
        }
    }

    pub(crate) fn region_income(&mut self, region: RegionId, events: &mut Vec<Event>) {
        let Some(owner) = self.owner(region) else {
            return;
        };
        let fertile = self
            .rules
            .region_rules(region)
            .map_or(0, |r| r.bonus.get(bonus::FERTILE_TERRITORY));
        self.add_resources(owner, fertile);
        events.push(Event::Income {
            player: owner,
            region,
            amount: fertile,
        });
        let finished: Vec<BuildingId> = self
            .region(region)
            .map(|r| {
                r.plots
                    .iter()
                    .flatten()
                    .filter(|p| !p.under_construction)
                    .map(|p| p.building)
                    .collect()
            })
            .unwrap_or_default();
        for building in finished {
            let amounts = self
                .rules
                .building(building)
                .map(|b| b.treasury.clone())
                .unwrap_or_default();
            for amount in amounts {
                self.add_resources(owner, amount);
                events.push(Event::Income {
                    player: owner,
                    region,
                    amount,
                });
            }
        }
    }

    pub(crate) fn construction_tick(&mut self, region: RegionId, events: &mut Vec<Event>) {
        let Some(state) = self.region_mut(region) else {
            return;
        };
        let Some((index, plot)) = state
            .plots
            .iter_mut()
            .enumerate()
            .find_map(|(i, p)| p.as_mut().filter(|p| p.under_construction).map(|p| (i, p)))
        else {
            return;
        };
        plot.turns_left = plot.turns_left.saturating_sub(1);
        if plot.turns_left <= 0 {
            plot.turns_left = 0;
            plot.under_construction = false;
            events.push(Event::BuildingCompleted {
                region,
                plot: u8::try_from(index).unwrap_or(u8::MAX),
            });
        }
    }

    pub(crate) fn order_build(
        &mut self,
        player: PlayerId,
        region: RegionId,
        plot: u8,
        building: BuildingId,
    ) -> Result<Vec<Event>, Rejected> {
        let state = self.region(region).ok_or(Rejected::NoSuchRegion)?;
        if state.owner != Some(player) {
            return Err(Rejected::NotYourRegion);
        }
        match state.plots.get(usize::from(plot)) {
            None => return Err(Rejected::NoSuchPlot),
            Some(Some(_)) => return Err(Rejected::PlotTaken),
            Some(None) => {}
        }
        let rules = self
            .rules
            .building(building)
            .ok_or(Rejected::NotAvailable)?;
        let template = self.player(player).ok_or(Rejected::UnknownPlayer)?.template;
        if !rules.available_to.is_empty() && !rules.available_to.contains(&template) {
            return Err(Rejected::NotAvailable);
        }
        if self.build_restricted(region, building) {
            return Err(Rejected::RestrictedBuilding);
        }
        let (cost, turns) = (rules.cost, rules.turns_to_build);
        if self.player(player).is_some_and(|p| p.resources < cost) {
            return Err(Rejected::CannotAfford);
        }
        self.add_resources(player, cost.saturating_neg());
        let mut events = vec![Event::BuildingStarted { region, plot }];
        if let Some(slot) = self
            .region_mut(region)
            .and_then(|r| r.plots.get_mut(usize::from(plot)))
        {
            *slot = Some(PlotBuilding::new(building, turns.max(0), turns > 0));
        }
        if turns <= 0 {
            events.push(Event::BuildingCompleted { region, plot });
        }
        Ok(events)
    }

    pub(crate) fn build_restricted(&self, region: RegionId, building: BuildingId) -> bool {
        let (Some(state), Some(rules), Some(region_rules)) = (
            self.region(region),
            self.rules.building(building),
            self.rules.region_rules(region),
        ) else {
            return false;
        };
        let kind = rules.kind.unwrap_or(BuildingType::None);
        let cap = region_rules
            .restrict
            .iter()
            .find(|(kinds, _)| kinds.contains(&kind))
            .map_or(
                i32::try_from(region_rules.plots).unwrap_or(i32::MAX),
                |(_, allowed)| allowed.unwrap_or(1),
            );
        let held = state
            .plots
            .iter()
            .flatten()
            .filter(|p| {
                self.rules
                    .building(p.building)
                    .is_some_and(|b| b.kind.unwrap_or(BuildingType::None) == kind)
            })
            .count();
        i32::try_from(held).unwrap_or(i32::MAX) >= cap
    }

    pub(crate) fn order_demolish(
        &mut self,
        player: PlayerId,
        region: RegionId,
        plot: u8,
    ) -> Result<Vec<Event>, Rejected> {
        let state = self.region(region).ok_or(Rejected::NoSuchRegion)?;
        if state.owner != Some(player) {
            return Err(Rejected::NotYourRegion);
        }
        let built = state
            .plots
            .get(usize::from(plot))
            .cloned()
            .flatten()
            .ok_or(Rejected::NoSuchPlot)?;
        if built.under_construction {
            let refund = self.rules.building(built.building).map_or(0, |b| b.cost);
            self.add_resources(player, refund);
        }
        if let Some(slot) = self
            .region_mut(region)
            .and_then(|r| r.plots.get_mut(usize::from(plot)))
        {
            *slot = None;
        }
        Ok(Vec::new())
    }
}
