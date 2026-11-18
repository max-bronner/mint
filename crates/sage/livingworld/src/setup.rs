#![allow(missing_docs)]

pub(crate) mod act_spawns;
mod regions;
mod seats;

use std::collections::BTreeMap;

use mint_math::rng::Pcg32;
use mint_sage_ini::living_world::campaign::ScenarioDef;
use mint_sage_ini::living_world::LivingWorldData;

use crate::model::{
    Army, ArmyId, Bonus, Member, Phase, Player, PlayerId, PlayerKind, PlotBuilding, RegionId,
    RegionState, UnitId,
};
use crate::rules::{Catalog, Rules, SetupError};
use crate::state::LivingWorldState;
use act_spawns::ActRecords;
pub use regions::{
    assign_start_regions, clear_start_region, default_start_regions, reset_start_regions,
    set_seat_faction, set_start_region,
};
use seats::resolve_random_seats;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerSetup {
    pub template: String,
    pub kind: PlayerKind,
    pub skill: crate::ai::Skill,
    pub team: i32,
    pub start_region: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub campaign: String,
    pub scenario: usize,
    pub victory_type: usize,
    pub battle_type: crate::screen_queries::BattleType,
    pub seed: u64,
    pub players: Vec<PlayerSetup>,
}

const MAX_SLOTS: i32 = 8;

fn initial_regions(rules: &Rules, scenario: &ScenarioDef) -> Vec<RegionState> {
    rules
        .regions
        .iter()
        .map(|r| RegionState {
            owner: None,
            enabled: !listed(&scenario.disable_regions.0, &r.name),
            changed_turn: 1,
            present: Vec::new(),
            plots: vec![None; r.plots],
        })
        .collect()
}

fn listed(tokens: &[String], name: &str) -> bool {
    tokens.iter().any(|t| t.eq_ignore_ascii_case(name))
}

impl LivingWorldState {
    pub fn start(
        data: &LivingWorldData,
        catalog: &Catalog,
        setup: &Setup,
    ) -> Result<Self, SetupError> {
        let rules = Rules::build(
            data,
            catalog,
            &setup.campaign,
            setup.scenario,
            setup.victory_type,
        )?;
        let campaign = data
            .campaigns
            .get(&setup.campaign)
            .ok_or_else(|| SetupError::UnknownCampaign(setup.campaign.clone()))?;
        let scenario = campaign
            .scenarios
            .get(setup.scenario)
            .ok_or(SetupError::UnknownScenario(setup.scenario))?;
        let count = setup.players.len();
        let min = scenario.min_players.unwrap_or(0);
        let max = scenario.max_players.unwrap_or(MAX_SLOTS).min(MAX_SLOTS);
        if max <= 0 || min > max {
            return Err(SetupError::InvalidPlayerLimits);
        }
        if usize::try_from(min).is_ok_and(|min| count < min) {
            return Err(SetupError::TooFewPlayers { min, got: count });
        }
        if usize::try_from(max).is_ok_and(|max| count > max) {
            return Err(SetupError::TooManyPlayers { max, got: count });
        }
        if u8::try_from(count).is_err() {
            return Err(SetupError::TooManyPlayersForIds);
        }
        let mut rng = Pcg32::new(setup.seed);
        let mut seats = setup.players.clone();
        resolve_random_seats(data, catalog, scenario, &mut seats, &mut rng)?;
        for (slot, seat) in seats.iter().enumerate() {
            if seat.start_region.is_none() {
                return Err(SetupError::NoStartRegion(slot));
            }
        }
        let mut teams: Vec<i32> = seats.iter().map(|p| p.team).filter(|t| *t >= 0).collect();
        teams.sort_unstable();
        teams.dedup();
        let teamless = seats.iter().filter(|p| p.team < 0).count();
        if teams.len().saturating_add(teamless) < 2 {
            return Err(SetupError::TooFewTeams);
        }

        let mut regions = initial_regions(&rules, scenario);

        let mut templates = Vec::new();
        for seat in &seats {
            let id = rules
                .template_id(&seat.template)
                .ok_or_else(|| SetupError::UnknownTemplate(seat.template.clone()))?;
            if listed(&scenario.disabled_factions.0, &seat.template) {
                return Err(SetupError::DisabledFaction(seat.template.clone()));
            }
            templates.push(id);
        }

        let mut starts = Vec::new();
        for seat in &seats {
            let name = seat.start_region.as_deref().unwrap_or_default();
            let region = rules.region(name)?;
            if !regions.get(region.index()).is_some_and(|r| r.enabled) {
                return Err(SetupError::StartRegionDisabled(name.to_owned()));
            }
            starts.push(region);
        }
        let set_of = |region: RegionId| {
            let name = rules.regions.get(region.index()).map(|r| r.name.as_str())?;
            scenario
                .ownership_sets
                .iter()
                .position(|set| listed(&set.regions.0, name))
        };
        let mut bases: Vec<RegionId> = Vec::new();
        for start in &starts {
            let mapped = match set_of(*start).and_then(|i| scenario.ownership_sets.get(i)) {
                Some(set) => match set.start_region.as_deref().filter(|n| !n.is_empty()) {
                    Some(name) => rules.region(name)?,
                    None => *start,
                },
                None => *start,
            };
            bases.push(mapped);
        }

        let mut named: Vec<(PlayerId, RegionId, &str)> = Vec::new();
        for (i, base) in bases.iter().enumerate() {
            let player = PlayerId(u8::try_from(i).unwrap_or(u8::MAX));
            let Some(set) = set_of(*base).and_then(|k| scenario.ownership_sets.get(k)) else {
                set_owner(&mut regions, *base, i);
                continue;
            };
            for name in &set.regions.0 {
                set_owner(&mut regions, rules.region(name)?, i);
            }
            for spawn in &set.spawn_buildings {
                let region = match &spawn.region {
                    Some(name) => rules.region(name)?,
                    None => *base,
                };
                for name in &spawn.buildings.0 {
                    let Some(building) = rules.building_id(name) else {
                        continue;
                    };
                    let owned = rules.building(building).is_some_and(|b| {
                        b.available_to.is_empty()
                            || templates.get(i).is_some_and(|t| b.available_to.contains(t))
                    });
                    if !owned {
                        continue;
                    }
                    if let Some(state) = regions
                        .get_mut(region.index())
                        .filter(|r| r.owner == Some(player))
                    {
                        if let Some(slot) = state.plots.iter_mut().find(|p| p.is_none()) {
                            *slot = Some(PlotBuilding::new(building, 0, false));
                        }
                    }
                }
            }
            for spawn in &set.spawn_armies {
                let region = match &spawn.region {
                    Some(name) => rules.region(name)?,
                    None => *base,
                };
                for name in &spawn.armies.0 {
                    named.push((player, region, name.as_str()));
                }
            }
        }

        let players: Vec<Player> = seats
            .iter()
            .zip(&templates)
            .zip(&bases)
            .enumerate()
            .map(|(i, ((seat, template), base))| Player {
                id: PlayerId(u8::try_from(i).unwrap_or(u8::MAX)),
                template: *template,
                kind: seat.kind,
                team: seat.team.max(-1),
                base_region: Some(*base),
                resources: rules
                    .templates
                    .get(usize::from(template.0))
                    .map_or(0, |t| t.start_resources),
                reserved_cp: 0,
                ready: false,
                defeated: false,
                bonus_a: Bonus::default(),
                territories: Vec::new(),
                previous_territories: Vec::new(),
                fallen_heroes: Vec::new(),
                skill: seat.skill,
                skill_points: 1.0,
                rank_points: 0,
                sciences: Vec::new(),
                upgrades: Vec::new(),
            })
            .collect();

        let mut state = Self {
            rules: rules.clone(),
            turn: 1,
            phase: Phase::Planning,
            rng,
            players,
            regions,
            armies: BTreeMap::new(),
            next_army_id: 1,
            next_member_id: 1,
            next_battle_id: 0,
            battles: Vec::new(),
            captures: Vec::new(),
            decisions: BTreeMap::new(),
            retreat_entries: Vec::new(),
            destroy_pending: Vec::new(),
            awaiting: None,
            ended: None,
            dialog: None,
            scripted: None,
            battle_type: setup.battle_type,
            seat_colours: Vec::new(),
            palantir_markers: Vec::new(),
        };
        if campaign.is_scripted_campaign == Some(true) {
            state.scripted = Some(crate::state::Scripted::default());
        }
        let records = ActRecords::new(campaign);
        let mut registered = Vec::new();
        for (player, region, name) in named {
            state.spawn_named(data, &records, &mut registered, player, region, name);
        }
        state.spawn_act_armies(data, &records, &registered, &|_| true);
        state.palantir_markers = records.palantir_markers(campaign);
        let mut events = Vec::new();
        state.rebuild_bonuses(&mut events);
        state.enter_planning(&mut events);
        Ok(state)
    }

    pub fn spawn_army(
        &mut self,
        player: PlayerId,
        region: RegionId,
        hero: Option<&str>,
        entries: &[(String, u32)],
    ) -> ArmyId {
        let hero = hero.map(|h| self.rules.intern(h));
        let roster: Vec<(UnitId, u32)> = entries
            .iter()
            .map(|(name, count)| (self.rules.intern(name), *count))
            .collect();
        self.spawn_units(player, region, hero, &roster)
    }

    pub(crate) fn spawn_units(
        &mut self,
        player: PlayerId,
        region: RegionId,
        hero: Option<UnitId>,
        roster: &[(UnitId, u32)],
    ) -> ArmyId {
        let id = self.fresh_army_id();
        let roster = roster
            .iter()
            .map(|(unit, count)| Member {
                id: self.fresh_member_id(),
                unit: *unit,
                level: 1,
                count: *count,
                experience: 0,
                battles: 0,
                kills: 0,
                kills_battle: 0,
                upgraded_at: None,
            })
            .collect();
        self.armies.insert(
            id,
            Army {
                id,
                player,
                hero,
                roster,
                region,
                prev_region: Some(region),
                turns_here: 0,
                order: None,
                destroy_queued: false,
            },
        );
        id
    }

    pub(crate) fn spawn_merging(
        &mut self,
        player: PlayerId,
        region: RegionId,
        hero: Option<UnitId>,
        roster: &[(UnitId, u32)],
    ) -> ArmyId {
        let existing = if hero.is_none() {
            self.armies_of(player)
                .find(|a| !a.is_hero_army() && a.region == region)
                .map(|a| a.id)
        } else {
            None
        };
        let Some(existing) = existing else {
            return self.spawn_units(player, region, hero, roster);
        };
        let members: Vec<Member> = roster
            .iter()
            .map(|(unit, count)| Member {
                id: self.fresh_member_id(),
                unit: *unit,
                level: 1,
                count: *count,
                experience: 0,
                battles: 0,
                kills: 0,
                kills_battle: 0,
                upgraded_at: None,
            })
            .collect();
        if let Some(a) = self.armies.get_mut(&existing) {
            a.roster.extend(members);
        }
        existing
    }
}

fn set_owner(regions: &mut [RegionState], region: RegionId, seat: usize) {
    if let Some(r) = regions.get_mut(region.index()) {
        if r.enabled {
            r.owner = Some(PlayerId(u8::try_from(seat).unwrap_or(u8::MAX)));
        }
    }
}
