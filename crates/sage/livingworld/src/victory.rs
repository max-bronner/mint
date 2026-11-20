use std::collections::BTreeSet;

use crate::model::{Event, PlayerId, RegionId};
use crate::rules::TeamVictory;
use crate::state::LivingWorldState;

pub(crate) fn names_team(teams: &[u32], team: i32) -> bool {
    u32::try_from(team).is_ok_and(|t| teams.contains(&t))
}

impl LivingWorldState {
    fn team_regions(&self, team: i32) -> i32 {
        self.players
            .iter()
            .filter(|p| p.team == team)
            .fold(0i32, |sum, p| sum.saturating_add(self.regions_owned(p.id)))
    }

    fn victory_holds(&self, c: &TeamVictory, team: i32) -> bool {
        if c.min_regions.is_none() && c.controlled.is_empty() {
            return false;
        }
        if c.min_regions
            .is_some_and(|min| self.team_regions(team) < min)
        {
            return false;
        }
        let turn = i64::from(self.turn);
        c.controlled.iter().all(|r: &RegionId| {
            let Some(state) = self.region(*r) else {
                return true;
            };
            if !state.enabled {
                return true;
            }
            let held = state
                .owner
                .and_then(|o| self.player(o))
                .is_some_and(|p| p.team == team);
            held && (c.held_turns <= 0
                || turn.saturating_sub(i64::from(state.changed_turn)) >= i64::from(c.held_turns))
        })
    }

    fn team_wins(&self, team: i32) -> bool {
        self.rules
            .conditions
            .team_victory
            .iter()
            .filter(|c| names_team(&c.teams, team))
            .any(|c| self.victory_holds(c, team))
    }

    fn team_defeated(&self, team: i32) -> bool {
        self.rules
            .conditions
            .team_defeat
            .iter()
            .filter(|c| names_team(&c.teams, team))
            .any(|c| {
                c.max_regions
                    .is_some_and(|max| self.team_regions(team) <= max)
            })
    }

    fn player_defeated(&self, p: PlayerId) -> bool {
        let Some(player) = self.player(p) else {
            return false;
        };
        self.rules
            .conditions
            .player_defeat
            .iter()
            .filter(|c| names_team(&c.teams, player.team))
            .any(|c| {
                let capital_lost = c.lose_if_capital_lost
                    && player
                        .base_region
                        .is_some_and(|r| self.region(r).is_some() && self.owner(r) != Some(p));
                capital_lost
                    || c.max_regions
                        .is_some_and(|max| self.regions_owned(p) <= max)
            })
    }

    pub(crate) fn check_victory(&mut self, events: &mut Vec<Event>) {
        if self.ended.is_some() {
            return;
        }
        let teams: BTreeSet<i32> = self
            .live_players()
            .map(|p| p.team)
            .filter(|t| *t >= 0)
            .collect();
        let winner = teams.iter().copied().find(|t| self.team_wins(*t));
        if let Some(team) = winner {
            events.push(Event::TeamWon { team });
            let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
            for id in ids.into_iter().rev() {
                if self.player(id).is_some_and(|p| p.team != team) {
                    self.defeat_player(id, events);
                }
            }
        } else {
            for team in teams {
                if self.team_defeated(team) {
                    let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
                    for id in ids.into_iter().rev() {
                        if self.player(id).is_some_and(|p| p.team == team) {
                            self.defeat_player(id, events);
                        }
                    }
                }
            }
        }
        let defeated: Vec<PlayerId> = self
            .live_players()
            .map(|p| p.id)
            .filter(|id| self.player_defeated(*id))
            .collect();
        for id in defeated.into_iter().rev() {
            self.defeat_player(id, events);
        }
        self.check_session_end(events);
    }

    fn check_session_end(&mut self, events: &mut Vec<Event>) {
        let free_for_all = self.players.iter().all(|p| p.team < 0);
        let (alive, total) = if free_for_all {
            (self.live_players().count(), self.players.len())
        } else {
            let alive: BTreeSet<i32> = self.live_players().map(|p| p.team).collect();
            let all: BTreeSet<i32> = self.players.iter().map(|p| p.team).collect();
            (alive.len(), all.len())
        };
        if alive <= 1 && total > 1 {
            let team = self
                .live_players()
                .next()
                .map(|p| p.team)
                .filter(|t| *t >= 0);
            self.ended = Some(team);
            events.push(Event::SessionEnded { winning_team: team });
        }
    }

    pub(crate) fn defeat_player(&mut self, p: PlayerId, events: &mut Vec<Event>) {
        match self.player_mut(p) {
            Some(player) if !player.defeated => player.defeated = true,
            _ => return,
        }
        events.push(Event::PlayerDefeated { player: p });
        for index in 0..self.regions.len() {
            let Ok(raw) = u16::try_from(index) else { break };
            let region = RegionId(raw);
            if self.owner(region) != Some(p) {
                continue;
            }
            let mut best: Option<(usize, PlayerId)> = None;
            if let Some(state) = self.region(region) {
                for id in &state.present {
                    let Some(army) = self.armies.get(id) else {
                        continue;
                    };
                    if army.player != p && best.is_none_or(|(n, _)| army.roster.len() > n) {
                        best = Some((army.roster.len(), army.player));
                    }
                }
            }
            self.set_region_owner(region, best.map(|(_, player)| player));
        }
        let mine: Vec<_> = self.armies_of(p).map(|a| a.id).collect();
        for id in mine {
            self.delete_army(id);
        }
    }
}
