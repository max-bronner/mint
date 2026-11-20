use std::collections::BTreeSet;

use crate::model::{bonus, Army, ArmyId, Event, PlayerId, PlayerKind, RegionId, Rejected};
use crate::state::LivingWorldState;

const HERO_RANGE: u8 = 2;
const HEROLESS_RANGE: u8 = 1;
const FRIENDLY_STEP: u8 = 1;
const BORDER_STEP: u8 = 2;

struct Group {
    region: RegionId,
    dest: RegionId,
    armies: Vec<ArmyId>,
}

impl LivingWorldState {
    pub(crate) fn step_cost(&self, a: RegionId, b: RegionId) -> u8 {
        let (x, y) = (self.owner(a), self.owner(b));
        let same = match (x, y) {
            (None, None) => true,
            (Some(p), Some(q)) => self.allied(p, q),
            _ => false,
        };
        if same {
            FRIENDLY_STEP
        } else {
            BORDER_STEP
        }
    }

    fn path_cost(&self, player: PlayerId, from: RegionId, dest: RegionId) -> Option<u8> {
        let mut dist: Vec<Option<u8>> = vec![None; self.regions.len()];
        *dist.get_mut(from.index())? = Some(0);
        let passable = |r: RegionId| {
            self.region(r).is_some_and(|s| {
                s.enabled && (s.owner.is_none() || self.allied_owner(s.owner, player))
            })
        };
        let mut changed = true;
        while changed {
            changed = false;
            for index in 0..self.regions.len() {
                let Ok(raw) = u16::try_from(index) else { break };
                let u = RegionId(raw);
                let Some(Some(du)) = dist.get(index).copied() else {
                    continue;
                };
                if u == dest || (u != from && !passable(u)) {
                    continue;
                }
                let Some(rules) = self.rules.region_rules(u) else {
                    continue;
                };
                for v in &rules.neighbours {
                    let candidate = du.saturating_add(self.step_cost(u, *v));
                    if let Some(slot) = dist.get_mut(v.index()) {
                        if slot.is_none_or(|old| candidate < old) {
                            *slot = Some(candidate);
                            changed = true;
                        }
                    }
                }
            }
        }
        dist.get(dest.index()).copied().flatten()
    }

    pub(crate) fn check_order(
        &self,
        player: PlayerId,
        army_id: ArmyId,
        dest: RegionId,
        retreat: bool,
    ) -> Result<(), Rejected> {
        let army = self.armies.get(&army_id).ok_or(Rejected::NoSuchArmy)?;
        if army.player != player {
            return Err(Rejected::NotYourArmy);
        }
        let target = self.region(dest).ok_or(Rejected::NoSuchRegion)?;
        if !target.enabled {
            return Err(Rejected::RegionDisabled);
        }
        let friendly = target.owner == Some(player) || self.allied_owner(target.owner, player);
        if (!army.is_hero_army() || retreat) && !friendly {
            return Err(Rejected::NotFriendly);
        }
        let range = if army.is_hero_army() {
            HERO_RANGE
        } else {
            HEROLESS_RANGE
        };
        match self.path_cost(player, army.region, dest) {
            Some(cost) if cost <= range => Ok(()),
            _ => Err(Rejected::OutOfRange),
        }
    }

    pub(crate) fn set_order(&mut self, army: ArmyId, dest: RegionId, events: &mut Vec<Event>) {
        let Some(a) = self.armies.get_mut(&army) else {
            return;
        };
        if a.region == dest {
            a.order = None;
            events.push(Event::OrderCancelled { army });
        } else {
            a.order = Some(dest);
            events.push(Event::OrderGiven { army, dest });
        }
    }

    fn hero_level(&self, army: &Army) -> u32 {
        let Some(hero) = army.hero else { return 0 };
        army.roster
            .iter()
            .rev()
            .find(|m| m.unit == hero)
            .map_or(0, |m| m.level)
    }

    pub(crate) fn head_on_clashes(&mut self, events: &mut Vec<Event>) {
        let teams: BTreeSet<i32> = self.live_players().map(|p| p.team).collect();
        let mut by_team: Vec<Vec<Group>> = Vec::new();
        for team in &teams {
            let mut groups: Vec<Group> = Vec::new();
            for player in self.live_players().filter(|p| p.team == *team) {
                for army in self.armies_of(player.id) {
                    let (true, Some(dest)) = (army.is_hero_army(), army.order) else {
                        continue;
                    };
                    match groups
                        .iter_mut()
                        .find(|g| g.region == army.region && g.dest == dest)
                    {
                        Some(g) => g.armies.push(army.id),
                        None => groups.push(Group {
                            region: army.region,
                            dest,
                            armies: vec![army.id],
                        }),
                    }
                }
            }
            by_team.push(groups);
        }
        for i in 0..by_team.len() {
            for j in i.saturating_add(1)..by_team.len() {
                let (Some(left), Some(right)) = (by_team.get(i), by_team.get(j)) else {
                    continue;
                };
                for a in left {
                    for b in right {
                        if a.region != b.dest || a.dest != b.region {
                            continue;
                        }
                        let loser = if self.group_beats(a, b) { b } else { a };
                        for id in &loser.armies {
                            if let Some(army) = self.armies.get_mut(id) {
                                if army.order.take().is_some() {
                                    events.push(Event::HeadOnLost { army: *id });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn troop_cp(&self, group: &Group) -> i32 {
        group.armies.iter().fold(0i32, |sum, id| {
            let Some(army) = self.armies.get(id) else {
                return sum;
            };
            let hero = army.hero.map_or(0, |h| self.rules.unit(h).command_points);
            sum.saturating_add(self.army_cp(army).saturating_sub(hero))
        })
    }

    fn group_level(&self, group: &Group) -> u32 {
        group
            .armies
            .iter()
            .filter_map(|id| self.armies.get(id))
            .map(|a| self.hero_level(a))
            .max()
            .unwrap_or(0)
    }

    fn group_beats(&mut self, a: &Group, b: &Group) -> bool {
        let (ta, tb) = (self.troop_cp(a), self.troop_cp(b));
        if ta != tb {
            return ta > tb;
        }
        let (la, lb) = (self.group_level(a), self.group_level(b));
        if la != lb {
            return la > lb;
        }
        self.rng.below(2) == 0
    }

    pub(crate) fn snap_orders(&mut self) {
        for army in self.armies.values_mut() {
            if let Some(dest) = army.order.take() {
                army.region = dest;
            }
        }
    }

    pub(crate) fn register_armies(&mut self) {
        for region in &mut self.regions {
            region.present.clear();
        }
        let mut order: Vec<(PlayerId, ArmyId, RegionId, bool)> = Vec::new();
        for player in &self.players {
            for army in self.armies_of(player.id) {
                let empty = !army.is_hero_army() && army.roster.is_empty();
                order.push((player.id, army.id, army.region, empty));
            }
        }
        for (player, army, region, empty) in order {
            if empty && !(self.owner(region) == Some(player) && self.is_defended(region)) {
                continue;
            }
            if let Some(r) = self.region_mut(region) {
                r.present.push(army);
            }
        }
    }

    pub(crate) fn garrison_merge(&mut self, events: &mut Vec<Event>) {
        for index in 0..self.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            let Some(owner) = self.owner(region) else {
                continue;
            };
            let allies: Vec<PlayerId> = self
                .players
                .iter()
                .filter(|p| self.allied(owner, p.id))
                .map(|p| p.id)
                .collect();
            for ally in allies {
                let heroless: Vec<ArmyId> = self
                    .armies_of(ally)
                    .filter(|a| !a.is_hero_army() && a.region == region)
                    .map(|a| a.id)
                    .collect();
                let Some((first, rest)) = heroless.split_first() else {
                    continue;
                };
                if rest.is_empty() {
                    continue;
                }
                for other in rest {
                    self.merge_into(*first, *other);
                    if self.armies.get(other).is_some_and(|a| a.roster.is_empty()) {
                        self.delete_army(*other);
                    }
                }
                events.push(Event::ArmiesMerged {
                    region,
                    into: *first,
                });
            }
        }
    }

    pub(crate) fn troop_upgrades(&mut self) {
        for index in 0..self.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            let Some(owner) = self.owner(region) else {
                continue;
            };
            let mut members: Vec<(ArmyId, usize, u32)> = Vec::new();
            for army in self.armies_of(owner).filter(|a| a.region == region) {
                for (i, m) in army.roster.iter().enumerate() {
                    if !self.rules.unit(m.unit).hero {
                        members.push((army.id, i, m.level));
                    }
                }
            }
            members.sort_by_key(|m| std::cmp::Reverse(m.2));
            let plot_count = self.region(region).map_or(0, |r| r.plots.len());
            for plot in 0..plot_count {
                let Ok(plot_id) = u8::try_from(plot) else {
                    break;
                };
                let Some((per_turn, units)) = self
                    .region(region)
                    .and_then(|r| r.plots.get(plot))
                    .and_then(|p| p.as_ref())
                    .filter(|p| !p.under_construction)
                    .and_then(|p| self.rules.building(p.building))
                    .and_then(|b| b.upgrade.clone())
                else {
                    continue;
                };
                for (army, i, _) in &members {
                    let used = self
                        .region(region)
                        .and_then(|r| r.plots.get(plot))
                        .and_then(|p| p.as_ref())
                        .map_or(i32::MAX, |p| p.upgrades_used);
                    if used >= per_turn {
                        break;
                    }
                    let Some(member) = self.armies.get_mut(army).and_then(|a| a.roster.get_mut(*i))
                    else {
                        continue;
                    };
                    if member.is_upgraded() || !units.contains(&member.unit) {
                        continue;
                    }
                    member.upgraded_at = Some((region, plot_id));
                    if let Some(p) = self
                        .region_mut(region)
                        .and_then(|r| r.plots.get_mut(plot))
                        .and_then(|p| p.as_mut())
                    {
                        p.upgrades_used = p.upgrades_used.saturating_add(1);
                    }
                }
            }
        }
    }

    pub(crate) fn reset_upgrade_counts(&mut self) {
        for plot in self
            .regions
            .iter_mut()
            .flat_map(|r| r.plots.iter_mut().flatten())
        {
            plot.upgrades_used = 0;
        }
    }

    pub(crate) fn battles_and_captures(&mut self, events: &mut Vec<Event>) {
        for index in 0..self.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            let mut players: Vec<PlayerId> = Vec::new();
            if let Some(state) = self.region(region) {
                for id in &state.present {
                    if let Some(a) = self.armies.get(id) {
                        if !players.contains(&a.player) {
                            players.push(a.player);
                        }
                    }
                }
            }
            let owner = self.owner(region);
            match players.as_slice() {
                [] => {}
                [p] => {
                    let p = *p;
                    match owner {
                        None => self.queue_capture(region, p, 0),
                        Some(o) if self.allied(o, p) => {}
                        Some(o) => {
                            if self.defended_against(region, p) {
                                self.create_battle(region, vec![p, o], events);
                            } else {
                                self.queue_capture(region, p, 1);
                            }
                        }
                    }
                }
                [first, ..] => {
                    let first = *first;
                    if !players.iter().all(|q| self.allied(first, *q)) {
                        self.create_battle(region, players, events);
                    } else if self.defended_against(region, first) {
                        let mut all = players;
                        all.extend(owner);
                        self.create_battle(region, all, events);
                    } else if self.allied_owner(owner, first) {
                    } else {
                        let flags = if owner.is_none() { 0x20 } else { 0x21 };
                        let human = players
                            .iter()
                            .any(|p| self.kind(*p) == Some(PlayerKind::Human));
                        if human {
                            self.shared_capture(region, &players, flags);
                        } else {
                            self.queue_capture(region, first, flags | 0x20);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn discount(&self, player: PlayerId, field: usize) -> i32 {
        self.player(player).map_or(0, |p| p.bonus_a.get(field))
    }

    pub(crate) fn unit_price(&self, player: PlayerId, unit: crate::model::UnitId) -> i32 {
        use crate::rules::PriceClass;
        let info = self.rules.unit(unit);
        let base = info.build_cost;
        let price = match info.price_class {
            PriceClass::Siege if self.discount(player, bonus::DISCOUNTED_SIEGE_UNITS) > 0 => {
                base / 2
            }
            PriceClass::Hero if self.discount(player, bonus::DISCOUNTED_HERO_UNITS) > 0 => base / 2,
            PriceClass::Fortress => base,
            PriceClass::Other => {
                let off = self.discount(player, bonus::DISCOUNTED_BARRACKS_UNITS);
                if off > 0 {
                    base.saturating_sub(off)
                } else {
                    base
                }
            }
            _ => base,
        };
        price.max(0)
    }
}
