use crate::battle::{battle_sides, reborrow};
use crate::model::{
    ArmyId, Battle, BattleMode, Event, PlayerId, PlayerKind, RegionId, Rejected, UnitId,
};
use crate::state::LivingWorldState;

const AUTO_RESOLVE: u8 = 1;
const RETREAT: u8 = 2;
const REAL_TIME: u8 = 3;

impl LivingWorldState {
    fn accepting_plot(&self, region: RegionId, unit: UnitId) -> Option<u8> {
        let state = self.region(region)?;
        state.plots.iter().enumerate().find_map(|(i, plot)| {
            let plot = plot.as_ref().filter(|p| !p.under_construction)?;
            let (per_turn, units) = self.rules.building(plot.building)?.upgrade.as_ref()?;
            (plot.upgrades_used < *per_turn && units.contains(&unit))
                .then(|| u8::try_from(i).ok())
                .flatten()
        })
    }

    pub(crate) fn order_link_upgrade(
        &mut self,
        player: PlayerId,
        army_id: ArmyId,
        member_id: u32,
    ) -> Result<Vec<Event>, Rejected> {
        let army = self.armies.get(&army_id).ok_or(Rejected::NoSuchArmy)?;
        if army.player != player {
            return Err(Rejected::NotYourArmy);
        }
        let region = army.region;
        if self.owner(region) != Some(player) {
            return Err(Rejected::NotYourRegion);
        }
        let member = army
            .roster
            .iter()
            .find(|m| m.id == member_id)
            .ok_or(Rejected::NoSuchMember)?;
        if self.rules.unit(member.unit).hero {
            return Err(Rejected::MemberIsHero);
        }
        if member.is_upgraded() {
            return Err(Rejected::AlreadyUpgraded);
        }
        let plot = self
            .accepting_plot(region, member.unit)
            .ok_or(Rejected::NotUpgradeable)?;
        if let Some(p) = self
            .region_mut(region)
            .and_then(|r| r.plots.get_mut(usize::from(plot)))
            .and_then(|p| p.as_mut())
        {
            p.upgrades_used = p.upgrades_used.saturating_add(1);
        }
        if let Some(m) = self
            .armies
            .get_mut(&army_id)
            .and_then(|a| a.roster.iter_mut().find(|m| m.id == member_id))
        {
            m.upgraded_at = Some((region, plot));
        }
        Ok(vec![Event::MemberLinked {
            army: army_id,
            member: member_id,
            region,
            plot,
        }])
    }

    pub(crate) fn order_unlink_upgrade(
        &mut self,
        player: PlayerId,
        army_id: ArmyId,
        member_id: u32,
    ) -> Result<Vec<Event>, Rejected> {
        let army = self.armies.get_mut(&army_id).ok_or(Rejected::NoSuchArmy)?;
        if army.player != player {
            return Err(Rejected::NotYourArmy);
        }
        let member = army
            .roster
            .iter_mut()
            .find(|m| m.id == member_id)
            .ok_or(Rejected::NoSuchMember)?;
        let (region, plot) = member.upgraded_at.take().ok_or(Rejected::NotUpgraded)?;
        if let Some(p) = self
            .region_mut(region)
            .and_then(|r| r.plots.get_mut(usize::from(plot)))
            .and_then(|p| p.as_mut())
        {
            p.upgrades_used = p.upgrades_used.saturating_sub(1).max(0);
        }
        Ok(vec![Event::MemberUnlinked {
            army: army_id,
            member: member_id,
        }])
    }

    fn battle_by_id(&self, id: u32) -> Result<&Battle, Rejected> {
        self.battles
            .iter()
            .find(|b| b.id == id)
            .ok_or(Rejected::NoSuchBattle)
    }

    pub(crate) fn order_choose_mode(
        &mut self,
        player: PlayerId,
        battle: u32,
        method: u8,
        resolver: Option<&mut dyn crate::battle::AutoResolve>,
    ) -> Result<Vec<Event>, Rejected> {
        if !matches!(method, AUTO_RESOLVE | RETREAT | REAL_TIME) {
            return Err(Rejected::InvalidMethod);
        }
        let index = self
            .battles
            .iter()
            .position(|b| b.id == battle)
            .ok_or(Rejected::NoSuchBattle)?;
        let entry = self.battles.get_mut(index).ok_or(Rejected::NoSuchBattle)?;
        if !entry.players.contains(&player) {
            return Err(Rejected::NotInBattle);
        }
        entry.choices.retain(|(p, _)| *p != player);
        entry.choices.push((player, method));
        let voted = entry.clone();
        let undecided_human = voted.players.iter().any(|p| {
            self.kind(*p) == Some(PlayerKind::Human) && !voted.choices.iter().any(|(c, _)| c == p)
        });
        if undecided_human {
            return Ok(Vec::new());
        }
        let real_time = voted.choices.iter().any(|(_, m)| *m == REAL_TIME)
            || voted.players.iter().any(|p| {
                self.kind(*p) == Some(PlayerKind::Inert)
                    && !voted.choices.iter().any(|(c, _)| c == p)
            });
        let retreaters: Vec<PlayerId> = self
            .players
            .iter()
            .map(|p| p.id)
            .filter(|p| voted.choices.contains(&(*p, RETREAT)))
            .collect();
        let mut resolver = resolver;
        for withdrawer in retreaters {
            let human = self.kind(withdrawer) == Some(PlayerKind::Human);
            self.withdraw(index, withdrawer, reborrow(&mut resolver));
            if human {
                break;
            }
        }
        let snapshot = self
            .battles
            .get(index)
            .cloned()
            .ok_or(Rejected::NoSuchBattle)?;
        let sides = battle_sides(self, &snapshot);
        let mut events = Vec::new();
        match sides.len() {
            0 => {
                self.battles.remove(index);
                events.push(Event::BattleFinished {
                    region: snapshot.region,
                    winner: None,
                });
            }
            1 => {
                let result = crate::model::BattleResult {
                    winner: Some(0),
                    rosters: Vec::new(),
                    fallen_heroes: Vec::new(),
                    fallen_records: Vec::new(),
                    players: Vec::new(),
                };
                self.finish_battle(index, &result, BattleMode::Auto, &mut events);
            }
            _ => {
                let human_left = snapshot
                    .players
                    .iter()
                    .any(|p| self.kind(*p) == Some(PlayerKind::Human));
                let mode = if real_time && human_left && self.scripted.is_none() {
                    BattleMode::RealTime
                } else {
                    BattleMode::Auto
                };
                let first = snapshot.players.first().copied().unwrap_or(player);
                events = self.order_resolve(first, snapshot.region, mode, resolver)?;
            }
        }
        Ok(events)
    }

    fn withdraw(
        &mut self,
        index: usize,
        player: PlayerId,
        resolver: Option<&mut dyn crate::battle::AutoResolve>,
    ) {
        let Some(battle) = self.battles.get(index).cloned() else {
            return;
        };
        let rounds = self.rules.army_retreat_rounds;
        if let (Some(resolver), true) = (resolver, rounds > 0) {
            let mut spec = self.spec_for(&battle);
            spec.weapon_scale.push((player, 0.0));
            spec.round_cap = Some(rounds);
            let result = resolver.resolve(&spec, &mut self.rng);
            self.apply_survivors(&result);
        }
        let present: Vec<ArmyId> = self
            .region(battle.region)
            .map(|r| r.present.clone())
            .unwrap_or_default();
        for id in present {
            let Some(army) = self.armies.get(&id).filter(|a| a.player == player) else {
                continue;
            };
            if !army.is_hero_army() {
                self.queue_destroy(id);
            } else if !self.retreat_entries.contains(&id) {
                self.retreat_entries.push(id);
            }
        }
        if let Some(entry) = self.battles.get_mut(index) {
            entry.players.retain(|p| *p != player);
            entry.choices.retain(|(p, _)| *p != player);
            entry.withdrawn.push(player);
        }
    }

    pub(crate) fn order_swap_in_battle(
        &mut self,
        player: PlayerId,
        battle: u32,
        side: u8,
        slot: u8,
        a: u8,
        b: u8,
    ) -> Result<Vec<Event>, Rejected> {
        let battle = self.battle_by_id(battle)?.clone();
        if !battle.players.contains(&player) {
            return Err(Rejected::NotInBattle);
        }
        let sides = battle_sides(self, &battle);
        let slot_player = *sides
            .get(usize::from(side))
            .and_then(|s| s.get(usize::from(slot)))
            .ok_or(Rejected::InvalidSwap)?;
        let present = self
            .region(battle.region)
            .map_or(&[][..], |r| r.present.as_slice());
        let positions: Vec<usize> = present
            .iter()
            .enumerate()
            .filter(|(_, id)| {
                self.armies
                    .get(id)
                    .is_some_and(|army| army.player == slot_player)
            })
            .map(|(i, _)| i)
            .collect();
        let at_a = *positions.get(usize::from(a)).ok_or(Rejected::InvalidSwap)?;
        let at_b = *positions.get(usize::from(b)).ok_or(Rejected::InvalidSwap)?;
        if let Some(r) = self.region_mut(battle.region) {
            r.present.swap(at_a, at_b);
        }
        Ok(Vec::new())
    }

    pub(crate) fn order_force_real_time(&mut self, battle: u32) -> Result<Vec<Event>, Rejected> {
        let battle = self.battle_by_id(battle)?.clone();
        self.awaiting = Some(battle.id);
        Ok(vec![Event::BattleSpec(self.spec_for(&battle))])
    }

    pub(crate) fn order_surrender(
        &mut self,
        issuer: PlayerId,
        target: PlayerId,
    ) -> Result<Vec<Event>, Rejected> {
        if issuer != target {
            return Err(Rejected::NotYourself);
        }
        let humans = self
            .players
            .iter()
            .filter(|p| p.kind == PlayerKind::Human)
            .count();
        if humans == 1 && self.awaiting.is_some() {
            self.ended = Some(None);
            return Ok(vec![Event::SessionEnded { winning_team: None }]);
        }
        let side: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|p| !p.defeated && self.allied(target, p.id))
            .map(|p| p.id)
            .collect();
        let mut events = Vec::new();
        for p in side {
            self.defeat_player(p, &mut events);
        }
        Ok(events)
    }

    pub(crate) fn order_dialog_step(
        &mut self,
        player: PlayerId,
        battle: u32,
        round: Option<u32>,
    ) -> Result<Vec<Event>, Rejected> {
        let dialog = self
            .dialog
            .as_mut()
            .filter(|d| d.battle == battle)
            .ok_or(Rejected::NoDialog)?;
        match round {
            Some(round) => match dialog.rounds.iter_mut().find(|(p, _)| *p == player) {
                Some(entry) => entry.1 = round,
                None => {
                    dialog.rounds.push((player, round));
                    dialog.rounds.sort_by_key(|(p, _)| p.0);
                }
            },
            None => {
                if !dialog.skipped.contains(&player) {
                    dialog.skipped.push(player);
                    dialog.skipped.sort_by_key(|p| p.0);
                }
            }
        }
        Ok(Vec::new())
    }
}
