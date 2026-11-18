use crate::model::{bonus, Army, ArmyId, Bonus, Event, Phase, PlayerId, RegionId};
use crate::state::LivingWorldState;

impl LivingWorldState {
    pub(crate) fn rebuild_bonuses(&mut self, events: &mut Vec<Event>) {
        let opening = self.turn <= 1 && self.phase == Phase::Planning;
        for index in 0..self.players.len() {
            let Ok(raw) = u8::try_from(index) else { break };
            let p = PlayerId(raw);
            let mut sum = Bonus::default();
            for (region, state) in self.rules.regions.iter().zip(&self.regions) {
                if state.owner == Some(p) {
                    sum.add(&region.bonus);
                }
            }
            let mut held = Vec::new();
            for (t_index, territory) in self.rules.territories.iter().enumerate() {
                if let Some(share) = self.territory_share(&territory.regions, p) {
                    sum.add(&territory.bonus.scaled(share));
                    held.push(u16::try_from(t_index).unwrap_or(u16::MAX));
                }
            }
            let Some(player) = self.players.get_mut(index) else {
                continue;
            };
            player.previous_territories = std::mem::replace(&mut player.territories, held);
            player.bonus_a = sum;
            for t in &player.territories {
                if !opening && !player.previous_territories.contains(t) {
                    events.push(Event::TerritoryGained {
                        player: p,
                        territory: *t,
                    });
                }
            }
            for t in &player.previous_territories {
                if !player.territories.contains(t) {
                    events.push(Event::TerritoryLost {
                        player: p,
                        territory: *t,
                    });
                }
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn territory_share(&self, listed: &[RegionId], p: PlayerId) -> Option<f32> {
        if listed.is_empty() {
            return None;
        }
        let mut own = 0usize;
        for region in listed {
            let owner = self.owner(*region)?;
            if !self.allied(owner, p) {
                return None;
            }
            if owner == p {
                own = own.saturating_add(1);
            }
        }
        if own == 0 {
            return None;
        }
        Some(own as f32 / listed.len() as f32)
    }

    #[must_use]
    pub fn army_cp(&self, army: &Army) -> i32 {
        army.roster.iter().fold(0i32, |sum, m| {
            let each = self.rules.unit(m.unit).command_points;
            sum.saturating_add(each.saturating_mul(i32::try_from(m.count).unwrap_or(i32::MAX)))
        })
    }

    #[must_use]
    pub fn army_cap(&self, army: &Army) -> i32 {
        let Some(player) = self.player(army.player) else {
            return 0;
        };
        if army.is_hero_army() {
            return self
                .rules
                .templates
                .get(usize::from(player.template.0))
                .map_or(0, |t| t.max_hero_cp);
        }
        let Some(rules) = self.rules.region_rules(army.region) else {
            return 0;
        };
        let owner = self.owner(army.region);
        match owner {
            Some(o) if o != army.player && self.allied(o, army.player) => rules.ally_cp_limit,
            _ => rules.cp_limit,
        }
    }

    fn building_cp(&self, p: PlayerId) -> (i32, i32) {
        let (mut world, mut tactical) = (0i32, 0i32);
        for state in self.regions.iter().filter(|r| r.owner == Some(p)) {
            for plot in state
                .plots
                .iter()
                .flatten()
                .filter(|b| !b.under_construction)
            {
                if let Some(b) = self.rules.building(plot.building) {
                    world = world.saturating_add(b.world_cp);
                    tactical = tactical.saturating_add(b.tactical_cp);
                }
            }
        }
        (world, tactical)
    }

    #[must_use]
    pub fn world_cp_in_use(&self, p: PlayerId) -> i32 {
        self.armies_of(p)
            .fold(0i32, |sum, a| sum.saturating_add(self.army_cp(a)))
    }

    #[must_use]
    pub fn world_cp_limit(&self, p: PlayerId) -> i32 {
        let Some(player) = self.player(p) else {
            return 0;
        };
        let Some(t) = self.rules.templates.get(usize::from(player.template.0)) else {
            return 0;
        };
        let total = t
            .start_world_cp
            .saturating_add(self.building_cp(p).0)
            .saturating_add(player.bonus_a.get(bonus::ARMY));
        total.min(t.max_world_cp)
    }

    pub(crate) fn merge_into(&mut self, into: ArmyId, from: ArmyId) {
        let Some(moved) = self
            .armies
            .get_mut(&from)
            .map(|a| std::mem::take(&mut a.roster))
        else {
            return;
        };
        if let Some(a) = self.armies.get_mut(&into) {
            a.roster.extend(moved);
        }
    }
}
