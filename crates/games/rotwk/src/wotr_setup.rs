use mint_sage_ini::living_world::campaign::ScenarioDef;
use mint_sage_ini::living_world::LivingWorldData;
use mint_sage_livingworld::screen_queries::BattleType;
use mint_sage_livingworld::{
    assign_start_regions, default_start_regions, reset_start_regions, set_seat_faction,
    set_start_region, Catalog, PlayerKind, PlayerSetup, RegionId, Setup, SetupError, Skill,
};
use mint_sage_worldmap::PreviewMarks;

use crate::lobby::{Difficulty, Occupant, SkirmishSetup, TEAMS};

const MAX_SLOTS: i32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScenarioChoice {
    pub(crate) campaign: String,
    pub(crate) display_name: Option<String>,
    pub(crate) description: Option<String>,
}

pub(crate) fn scenario_choices(data: &LivingWorldData) -> Vec<ScenarioChoice> {
    data.campaigns
        .iter()
        .filter(|c| c.is_scripted_campaign != Some(true) && c.tutorials.is_empty())
        .filter_map(|c| {
            let scenario = c.scenarios.first()?;
            Some(ScenarioChoice {
                campaign: c.name.clone(),
                display_name: scenario.display_name.clone(),
                description: scenario.display_description.clone(),
            })
        })
        .collect()
}

fn scenario_of<'a>(data: &'a LivingWorldData, choice: &ScenarioChoice) -> Option<&'a ScenarioDef> {
    data.campaigns
        .iter()
        .find(|c| c.name == choice.campaign)
        .and_then(|c| c.scenarios.first())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Silent,
    Message(&'static str),
    Counted(&'static str, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Ready,
    Refused(Refusal),
}

fn listed(tokens: &[String], name: &str) -> bool {
    tokens.iter().any(|t| t.eq_ignore_ascii_case(name))
}

pub(crate) fn verdict(scenario: &ScenarioDef, setup: &Setup) -> Verdict {
    let max = scenario.max_players.unwrap_or(MAX_SLOTS).min(MAX_SLOTS);
    let min = scenario.min_players.unwrap_or(0);
    if max <= 0 || min > max {
        return Verdict::Refused(Refusal::Silent);
    }
    let occupied = i32::try_from(setup.players.len()).unwrap_or(i32::MAX);
    if occupied > max {
        return Verdict::Refused(Refusal::Counted("LAN:TooManyPlayers", max));
    }
    if occupied < min {
        return Verdict::Refused(Refusal::Counted("LAN:TooFewPlayers", min));
    }
    let disabled = !scenario.disabled_factions.0.is_empty()
        && setup
            .players
            .iter()
            .any(|p| p.template.is_empty() || listed(&scenario.disabled_factions.0, &p.template));
    if disabled {
        return Verdict::Refused(Refusal::Message("GUI:DisabledFaction"));
    }
    if setup.players.iter().any(|p| p.start_region.is_none()) {
        return Verdict::Refused(Refusal::Message("GUI:NeedStartSpots"));
    }
    let mut teams: Vec<i32> = setup
        .players
        .iter()
        .map(|p| p.team)
        .filter(|t| *t >= 0)
        .collect();
    teams.sort_unstable();
    teams.dedup();
    let teamless = setup.players.iter().filter(|p| p.team < 0).count();
    if teams.len().saturating_add(teamless) < 2 {
        return Verdict::Refused(Refusal::Message("GUI:NeedWOTREnemy"));
    }
    Verdict::Ready
}

pub(crate) struct Scenario<'a> {
    pub(crate) data: &'a LivingWorldData,
    pub(crate) catalog: &'a Catalog,
    pub(crate) choice: &'a ScenarioChoice,
    pub(crate) victory_type: usize,
}

impl Scenario<'_> {
    fn def(&self) -> Option<&ScenarioDef> {
        scenario_of(self.data, self.choice)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RegionSpot {
    pub(crate) name: String,
    pub(crate) at: [f32; 2],
}

fn enabled_regions(data: &LivingWorldData, choice: &ScenarioChoice) -> Vec<RegionSpot> {
    let Some(scenario) = scenario_of(data, choice) else {
        return Vec::new();
    };
    mint_sage_worldmap::region_views(data, &choice.campaign, 0)
        .into_iter()
        .filter(|r| !listed(&scenario.disable_regions.0, &r.name))
        .map(|r| RegionSpot {
            name: r.name,
            at: r.center,
        })
        .collect()
}

pub(crate) fn startable_regions(
    data: &LivingWorldData,
    choice: &ScenarioChoice,
) -> Vec<RegionSpot> {
    let Some(scenario) = scenario_of(data, choice) else {
        return Vec::new();
    };
    enabled_regions(data, choice)
        .into_iter()
        .filter(|r| !listed(&scenario.disallow_start_in_regions.0, &r.name))
        .collect()
}

pub(crate) fn scenario_description(
    data: &LivingWorldData,
    choice: &ScenarioChoice,
    victory_type: usize,
    text: &dyn Fn(&str) -> String,
) -> String {
    let described = |key: Option<&String>| key.map_or_else(String::new, |key| text(key));
    let victory = data
        .campaigns
        .iter()
        .find(|c| c.name == choice.campaign)
        .and_then(|c| c.victory_types.get(victory_type));
    format!(
        "{}

{}: {}
{}",
        described(choice.description.as_ref()),
        text("RULE:VictoryType"),
        described(victory.and_then(|v| v.display_game_type.as_ref())),
        described(victory.and_then(|v| v.display_objectives.as_ref())),
    )
}

pub(crate) fn territory_description(
    data: &LivingWorldData,
    choice: &ScenarioChoice,
    region: &str,
    text: &dyn Fn(&str) -> String,
) -> Option<String> {
    let campaign = data
        .region_campaigns
        .get(scenario_of(data, choice)?.region_campaign.as_deref()?)?;
    let def = campaign
        .regions
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(region))?;
    let territory = mint_sage_livingworld::territory_of(campaign, &def.name);
    Some(format!(
        "{}
{}",
        text(def.display_name.as_deref().unwrap_or(&def.name)),
        mint_sage_livingworld::territory_text(def, territory, text)
    ))
}

fn preview_regions(data: &LivingWorldData, choice: &ScenarioChoice) -> Vec<String> {
    mint_sage_worldmap::region_views(data, &choice.campaign, 0)
        .into_iter()
        .map(|r| r.name)
        .collect()
}

pub(crate) fn picked_region(
    data: &LivingWorldData,
    choice: &ScenarioChoice,
    pick: Option<RegionId>,
) -> Option<String> {
    let name = preview_regions(data, choice)
        .into_iter()
        .nth(pick?.index())?;
    enabled_regions(data, choice)
        .into_iter()
        .any(|r| r.name.eq_ignore_ascii_case(&name))
        .then_some(name)
}

pub(crate) fn preview_marks(
    cx: &Scenario,
    picks: &StartPicks,
    lobby: &SkirmishSetup,
    colour: impl Fn(usize) -> Option<[f32; 3]>,
    hovered: Option<&str>,
) -> PreviewMarks {
    let Some(scenario) = cx.def() else {
        return PreviewMarks::default();
    };
    let names = preview_regions(cx.data, cx.choice);
    let enabled: Vec<RegionId> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| !listed(&scenario.disable_regions.0, name))
        .filter_map(|(i, _)| Some(RegionId(u16::try_from(i).ok()?)))
        .collect();
    let id_of = |name: &str| {
        let i = names.iter().position(|n| n.eq_ignore_ascii_case(name))?;
        let id = RegionId(u16::try_from(i).ok()?);
        enabled.contains(&id).then_some(id)
    };
    let set_of = |name: &str| {
        scenario
            .ownership_sets
            .iter()
            .find(|set| listed(&set.regions.0, name))
    };
    let group = |name: &str| -> Vec<RegionId> {
        match set_of(name) {
            Some(set) => set.regions.0.iter().filter_map(|n| id_of(n)).collect(),
            None => id_of(name).into_iter().collect(),
        }
    };
    let mut marks = PreviewMarks {
        enabled: enabled.clone(),
        ..PreviewMarks::default()
    };
    for slot in occupied_slots(lobby) {
        let Some(start) = picks.get(slot) else {
            continue;
        };
        let colour = colour(slot);
        marks
            .filled
            .extend(group(start).into_iter().map(|r| (r, colour)));
        let base = set_of(start)
            .and_then(|set| set.start_region.as_deref())
            .filter(|n| !n.is_empty())
            .unwrap_or(start);
        for region in [id_of(start), id_of(base)].into_iter().flatten() {
            if !marks.homes.iter().any(|(r, _)| *r == region) {
                marks.homes.push((region, colour));
            }
        }
    }
    let disallowed = &scenario.disallow_start_in_regions.0;
    if !disallowed.is_empty() {
        marks.startable = enabled
            .iter()
            .copied()
            .filter(|r| names.get(r.index()).is_some_and(|n| !listed(disallowed, n)))
            .filter(|r| !marks.filled.iter().any(|(f, _)| f == r))
            .collect();
    }
    if let Some(name) = hovered.filter(|n| !listed(disallowed, n)) {
        marks.hovered = group(name);
    }
    marks
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StartPicks(Vec<Option<String>>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Click {
    Assigned { slot: usize, region: String },
    Released { slot: usize, region: String },
    Refused(ClickRefusal),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ClickRefusal {
    NotStartable,
    Disallowed,
    Historical,
    NoFreeSeat,
    Engine(SetupError),
}

fn occupied_slots(lobby: &SkirmishSetup) -> Vec<usize> {
    lobby
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.occupant.occupied())
        .map(|(i, _)| i)
        .collect()
}

impl StartPicks {
    pub(crate) fn get(&self, slot: usize) -> Option<&str> {
        self.0.get(slot)?.as_deref()
    }

    fn set(&mut self, slot: usize, region: Option<String>) {
        if self.0.len() <= slot {
            self.0.resize(slot.saturating_add(1), None);
        }
        if let Some(entry) = self.0.get_mut(slot) {
            *entry = region;
        }
    }

    pub(crate) fn holder(&self, lobby: &SkirmishSetup, region: &str) -> Option<usize> {
        occupied_slots(lobby).into_iter().find(|slot| {
            self.get(*slot)
                .is_some_and(|held| held.eq_ignore_ascii_case(region))
        })
    }

    fn seats(&self, cx: &Scenario, lobby: &SkirmishSetup) -> (Setup, Vec<usize>) {
        let setup = setup_of(
            cx.choice,
            cx.victory_type,
            BattleType::default(),
            0,
            lobby,
            self,
        );
        (setup, occupied_slots(lobby))
    }

    fn absorb(&mut self, setup: &Setup, slots: &[usize], lobby: &mut SkirmishSetup) {
        for (seat, slot) in setup.players.iter().zip(slots) {
            self.set(*slot, seat.start_region.clone());
            let Some(row) = lobby.slots.get_mut(*slot) else {
                continue;
            };
            if !seat.template.is_empty() && row.faction.as_deref() != Some(seat.template.as_str()) {
                row.faction = Some(seat.template.clone());
            }
            row.team = u8::try_from(seat.team).ok().filter(|t| *t < TEAMS);
        }
    }

    pub(crate) fn choose_scenario(&mut self, cx: &Scenario, lobby: &mut SkirmishSetup) {
        self.0.clear();
        let (mut setup, slots) = self.seats(cx, lobby);
        let _ = assign_start_regions(cx.data, cx.catalog, &mut setup);
        self.absorb(&setup, &slots, lobby);
    }

    pub(crate) fn fill_open_seats(&mut self, cx: &Scenario, lobby: &mut SkirmishSetup) {
        let Some(scenario) = cx.def() else { return };
        let allowed = startable_regions(cx.data, cx.choice);
        let defaults: Vec<&str> = scenario
            .default_start_spots
            .0
            .iter()
            .map(String::as_str)
            .filter(|name| allowed.iter().any(|s| s.name.eq_ignore_ascii_case(name)))
            .collect();
        let (mut setup, slots) = self.seats(cx, lobby);
        for seat in 0..setup.players.len() {
            if setup
                .players
                .get(seat)
                .is_none_or(|p| p.start_region.is_some())
            {
                continue;
            }
            let free = defaults.iter().find(|name| {
                !setup.players.iter().any(|p| {
                    p.start_region
                        .as_deref()
                        .is_some_and(|held| held.eq_ignore_ascii_case(name))
                })
            });
            if let Some(name) = free {
                let _ = set_start_region(cx.data, cx.catalog, &mut setup, seat, name);
            }
        }
        self.absorb(&setup, &slots, lobby);
    }

    pub(crate) fn faction_picked(&mut self, cx: &Scenario, lobby: &mut SkirmishSetup, slot: usize) {
        let (mut setup, slots) = self.seats(cx, lobby);
        let Some(seat) = slots.iter().position(|s| *s == slot) else {
            return;
        };
        let Some(template) = setup.players.get(seat).map(|p| p.template.clone()) else {
            return;
        };
        if template.is_empty() {
            return;
        }
        let _ = set_seat_faction(cx.data, cx.catalog, &mut setup, seat, &template);
        self.absorb(&setup, &slots, lobby);
    }

    pub(crate) fn click(
        &mut self,
        cx: &Scenario,
        lobby: &mut SkirmishSetup,
        region: &str,
    ) -> Click {
        let Some(scenario) = cx.def() else {
            return Click::Refused(ClickRefusal::NotStartable);
        };
        if scenario.historical_scenario == Some(true) {
            return Click::Refused(ClickRefusal::Historical);
        }
        let Some(spot) = enabled_regions(cx.data, cx.choice)
            .into_iter()
            .find(|s| s.name.eq_ignore_ascii_case(region))
        else {
            return Click::Refused(ClickRefusal::NotStartable);
        };
        if listed(&scenario.disallow_start_in_regions.0, &spot.name) {
            return Click::Refused(ClickRefusal::Disallowed);
        }
        let name = scenario
            .ownership_sets
            .iter()
            .find(|set| listed(&set.regions.0, &spot.name))
            .and_then(|set| {
                set.regions
                    .0
                    .iter()
                    .find(|member| self.holder(lobby, member).is_some())
            })
            .cloned()
            .unwrap_or(spot.name);
        if let Some(slot) = self.holder(lobby, &name) {
            self.set(slot, None);
            return Click::Released { slot, region: name };
        }
        let (mut setup, slots) = self.seats(cx, lobby);
        let Some(seat) = setup.players.iter().position(|p| p.start_region.is_none()) else {
            return Click::Refused(ClickRefusal::NoFreeSeat);
        };
        match set_start_region(cx.data, cx.catalog, &mut setup, seat, &name) {
            Ok(()) => {
                self.absorb(&setup, &slots, lobby);
                let slot = slots.get(seat).copied().unwrap_or_default();
                Click::Assigned { slot, region: name }
            }
            Err(error) => Click::Refused(ClickRefusal::Engine(error)),
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        cx: &Scenario,
        lobby: &mut SkirmishSetup,
        before: &[(bool, Option<String>)],
    ) {
        let now: Vec<(bool, Option<String>)> = lobby
            .slots
            .iter()
            .map(|s| (s.occupant.occupied(), s.faction.clone()))
            .collect();
        for (slot, (occupied, _)) in now.iter().enumerate() {
            if !occupied {
                self.set(slot, None);
            }
        }
        let was_open = |slot: usize| before.get(slot).is_some_and(|(occupied, _)| *occupied);
        if now
            .iter()
            .enumerate()
            .any(|(slot, (occupied, _))| *occupied && !was_open(slot))
        {
            self.fill_open_seats(cx, lobby);
        }
        for (slot, (occupied, faction)) in now.iter().enumerate() {
            let changed = before.get(slot).is_none_or(|(_, f)| f != faction);
            if *occupied && changed && faction.is_some() {
                self.faction_picked(cx, lobby, slot);
            }
        }
    }
}

pub(crate) fn snapshot(lobby: &SkirmishSetup) -> Vec<(bool, Option<String>)> {
    lobby
        .slots
        .iter()
        .map(|s| (s.occupant.occupied(), s.faction.clone()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VictoryOption {
    pub(crate) name: String,
    pub(crate) label: Option<String>,
}

pub(crate) fn victory_options(
    data: &LivingWorldData,
    choice: &ScenarioChoice,
) -> Vec<VictoryOption> {
    data.campaigns
        .iter()
        .find(|c| c.name == choice.campaign)
        .map(|c| {
            c.victory_types
                .iter()
                .map(|v| VictoryOption {
                    name: v.name.clone(),
                    label: v.display_game_type.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) const BATTLE_TYPES: [(BattleType, &str); 3] = [
    (BattleType::Both, "VALUE:AutoResolveAndRTS"),
    (BattleType::AutoResolveOnly, "VALUE:AutoResolve"),
    (BattleType::RealTimeOnly, "VALUE:RTS"),
];

pub(crate) fn battle_type_at(index: usize) -> Option<BattleType> {
    BATTLE_TYPES.get(index).map(|(kind, _)| *kind)
}

pub(crate) fn seat_colours(lobby: &SkirmishSetup, offered: usize) -> Vec<usize> {
    let seats: Vec<Option<usize>> = lobby
        .slots
        .iter()
        .filter(|s| s.occupant.occupied())
        .map(|s| s.colour.map(usize::from))
        .collect();
    let mut taken: Vec<usize> = seats.iter().flatten().copied().collect();
    seats
        .iter()
        .map(|chosen| {
            chosen.unwrap_or_else(|| {
                let free = (0..offered.max(1))
                    .find(|c| !taken.contains(c))
                    .unwrap_or(0);
                taken.push(free);
                free
            })
        })
        .collect()
}

pub(crate) fn setup_of(
    choice: &ScenarioChoice,
    victory_type: usize,
    battle_type: BattleType,
    seed: u64,
    lobby: &SkirmishSetup,
    picks: &StartPicks,
) -> Setup {
    let players = lobby
        .slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.occupant.occupied())
        .map(|(index, slot)| PlayerSetup {
            template: slot.faction.clone().unwrap_or_default(),
            kind: match slot.occupant {
                Occupant::Human => PlayerKind::Human,
                _ => PlayerKind::Computer,
            },
            skill: match slot.occupant {
                Occupant::Ai(Difficulty::Easy) => Skill::Easy,
                Occupant::Ai(Difficulty::Hard) => Skill::Hard,
                Occupant::Ai(Difficulty::Brutal) => Skill::Brutal,
                _ => Skill::Medium,
            },
            team: slot.team.map_or(-1, i32::from),
            start_region: picks.get(index).map(str::to_owned),
        })
        .collect();
    Setup {
        campaign: choice.campaign.clone(),
        scenario: 0,
        victory_type,
        battle_type,
        seed,
        players,
    }
}

pub(crate) fn give_start_regions(
    data: &LivingWorldData,
    catalog: &Catalog,
    setup: &mut Setup,
) -> Result<(), SetupError> {
    reset_start_regions(setup);
    let mut first = default_start_regions(data, catalog, setup).err();
    let picks: Vec<String> = setup.players.iter().map(|p| p.template.clone()).collect();
    for (seat, template) in picks.iter().enumerate() {
        if !template.is_empty() {
            if let Err(error) = set_seat_faction(data, catalog, setup, seat, template) {
                first.get_or_insert(error);
            }
        }
    }
    first.map_or(Ok(()), Err)
}

pub(crate) fn seat_name(
    kind: PlayerKind,
    skill: Skill,
    human_name: &str,
    localise: impl Fn(&str) -> Option<String>,
) -> String {
    if kind == PlayerKind::Human {
        return human_name.to_owned();
    }
    let label = match skill {
        Skill::Easy => Difficulty::Easy,
        Skill::Medium => Difficulty::Medium,
        Skill::Hard => Difficulty::Hard,
        Skill::Brutal => Difficulty::Brutal,
    }
    .label();
    localise(label).unwrap_or_else(|| label.to_owned())
}

pub(crate) fn refusal_of(error: &SetupError) -> Refusal {
    match error {
        SetupError::TooFewPlayers { min, .. } => Refusal::Counted("LAN:TooFewPlayers", *min),
        SetupError::TooManyPlayers { max, .. } => Refusal::Counted("LAN:TooManyPlayers", *max),
        SetupError::DisabledFaction(_)
        | SetupError::RandomFaction(_)
        | SetupError::NoPlayableFaction(_) => Refusal::Message("GUI:DisabledFaction"),
        SetupError::NoStartRegion(_)
        | SetupError::StartRegionDisabled(_)
        | SetupError::StartRegionShared(_) => Refusal::Message("GUI:NeedStartSpots"),
        SetupError::TooFewTeams => Refusal::Message("GUI:NeedWOTREnemy"),
        SetupError::InvalidPlayerLimits
        | SetupError::UnknownCampaign(_)
        | SetupError::UnknownScenario(_)
        | SetupError::NoRegionCampaign(_)
        | SetupError::UnknownRegion(_)
        | SetupError::UnknownTemplate(_)
        | SetupError::UnknownSeat(_)
        | SetupError::TooManyPlayersForIds => Refusal::Silent,
    }
}

pub(crate) fn prepare(
    cx: &Scenario,
    battle_type: BattleType,
    seed: u64,
    lobby: &SkirmishSetup,
    picks: &StartPicks,
) -> Result<Setup, Refusal> {
    let setup = setup_of(cx.choice, cx.victory_type, battle_type, seed, lobby, picks);
    let scenario = cx.def().ok_or(Refusal::Silent)?;
    match verdict(scenario, &setup) {
        Verdict::Ready => Ok(setup),
        Verdict::Refused(refusal) => Err(refusal),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
