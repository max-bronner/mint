use mint_math::rng::Pcg32;
use mint_sim::PlayerId;

use crate::setup_heroes::{self, HeroEntry};

pub(crate) const MAX_SLOTS: usize = 8;
pub(crate) const TEAMS: u8 = 4;
pub(crate) const HANDICAP_STEP: u8 = 5;
pub(crate) const HANDICAP_MAX: u8 = 95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Difficulty {
    Easy,
    Medium,
    Hard,
    Brutal,
}

impl Difficulty {
    pub(crate) const ALL: [Self; 4] = [Self::Easy, Self::Medium, Self::Hard, Self::Brutal];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Easy => "GUI:EasyAI",
            Self::Medium => "GUI:MediumAI",
            Self::Hard => "GUI:HardAI",
            Self::Brutal => "GUI:BrutalAI",
        }
    }

    pub(crate) const fn tuning_row(self) -> u8 {
        match self {
            Self::Easy => 0,
            Self::Medium => 1,
            Self::Hard => 2,
            Self::Brutal => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Occupant {
    Closed,
    Human,
    Ai(Difficulty),
}

impl Occupant {
    pub(crate) const fn occupied(self) -> bool {
        !matches!(self, Self::Closed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HeroPick {
    None,
    Random,
    Stored(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SlotSetup {
    pub(crate) occupant: Occupant,
    pub(crate) faction: Option<String>,
    pub(crate) hero: HeroPick,
    pub(crate) team: Option<u8>,
    pub(crate) colour: Option<u8>,
    pub(crate) handicap: u8,
    pub(crate) start: Option<u8>,
}

impl SlotSetup {
    pub(crate) const fn new(occupant: Occupant) -> Self {
        Self {
            occupant,
            faction: None,
            hero: HeroPick::None,
            team: None,
            colour: None,
            handicap: 0,
            start: None,
        }
    }

    pub(crate) const fn closed() -> Self {
        Self::new(Occupant::Closed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MatchRules {
    pub(crate) starting_money: i64,
    pub(crate) command_point_factor: u32,
    pub(crate) allow_ring_heroes: bool,
    pub(crate) allow_custom_heroes: bool,
}

pub(crate) fn check_starting_money(money: i64) -> Result<(), SetupError> {
    if money < 0 {
        return Err(SetupError::StartingMoney(money));
    }
    Ok(())
}

impl Default for MatchRules {
    fn default() -> Self {
        Self {
            starting_money: mint_sage_economy::DEFAULT_STARTING_CASH,
            command_point_factor: 100,
            allow_ring_heroes: true,
            allow_custom_heroes: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SkirmishSetup {
    pub(crate) map_label: String,
    pub(crate) slots: Vec<SlotSetup>,
    pub(crate) rules: MatchRules,
    pub(crate) army: Option<u32>,
    pub(crate) spawn: Option<String>,
    pub(crate) spawn_buildings: Option<String>,
    pub(crate) dummies: Option<String>,
    pub(crate) dummies_attack: bool,
    pub(crate) cah_hero: Option<String>,
    pub(crate) random_seed: u64,
    pub(crate) resolved: Option<Vec<ResolvedPlayer>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Catalogue {
    pub(crate) factions: Vec<String>,
    pub(crate) random_pool: Vec<String>,
    pub(crate) colours: u8,
    pub(crate) starts: u8,
    pub(crate) heroes: Vec<HeroEntry>,
    pub(crate) sides: Vec<(String, u8)>,
}

impl Catalogue {
    pub(crate) fn faction_bit(&self, template: &str) -> Option<u8> {
        if template.is_empty() {
            return None;
        }
        Some(
            self.sides
                .iter()
                .find(|(name, _)| name == template)
                .map_or(setup_heroes::NO_SIDE, |(_, bit)| *bit),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SetupError {
    TooManySlots(usize),
    TooFewPlayers(usize),
    TooManyPlayers { players: usize, starts: u8 },
    UnknownFaction(String),
    UnknownHero(usize),
    HeroNotUsable(usize),
    ColourOutOfRange(u8),
    ColourTaken(u8),
    StartOutOfRange(u8),
    StartTaken(u8),
    TeamOutOfRange(u8),
    Handicap(u8),
    StartingMoney(i64),
    CommandPointFactor(u32),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManySlots(n) => write!(f, "{n} slots, at most {MAX_SLOTS}"),
            Self::TooFewPlayers(n) => write!(f, "{n} player(s), a match needs two"),
            Self::TooManyPlayers { players, starts } => {
                write!(
                    f,
                    "{players} players on a map with {starts} start position(s)"
                )
            }
            Self::UnknownFaction(name) => write!(f, "{name} is not a playable PlayerTemplate"),
            Self::UnknownHero(h) => write!(f, "hero {h} is not in the hero list"),
            Self::HeroNotUsable(h) => write!(f, "hero {h} cannot serve this army"),
            Self::ColourOutOfRange(c) => write!(f, "colour {c} is not declared"),
            Self::ColourTaken(c) => write!(f, "colour {c} is taken twice"),
            Self::StartOutOfRange(s) => write!(f, "start position {s} is not on the map"),
            Self::StartTaken(s) => write!(f, "start position {s} is taken twice"),
            Self::TeamOutOfRange(t) => write!(f, "team {t} is not one of {TEAMS}"),
            Self::Handicap(h) => write!(
                f,
                "handicap {h}% is not a step of {HANDICAP_STEP} to {HANDICAP_MAX}"
            ),
            Self::StartingMoney(m) => write!(f, "starting money {m} is negative"),
            Self::CommandPointFactor(p) => write!(f, "a command point factor of {p}% is none"),
        }
    }
}

pub(crate) fn fresh_seed(fallback: u64) -> u64 {
    if let Some(fixed) = std::env::var("MINT_LOBBY_SEED")
        .ok()
        .and_then(|raw| raw.parse().ok())
    {
        return fixed;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| u64::try_from(since.as_millis()).ok())
        .unwrap_or(fallback)
}

impl SkirmishSetup {
    pub(crate) fn two_player(
        faction: String,
        opponent: String,
        map_label: String,
        ai: [bool; 2],
        army: Option<u32>,
    ) -> Self {
        let slot = |faction: String, ai: bool| SlotSetup {
            faction: Some(faction).filter(|name| !name.eq_ignore_ascii_case("random")),
            ..SlotSetup::new(if ai {
                Occupant::Ai(Difficulty::Easy)
            } else {
                Occupant::Human
            })
        };
        let slots = vec![slot(faction, ai[0]), slot(opponent, ai[1])];
        let random_seed = if slots.iter().any(|s| s.faction.is_none()) {
            fresh_seed(crate::skirmish::SKIRMISH_SEED)
        } else {
            crate::skirmish::SKIRMISH_SEED
        };
        Self {
            map_label,
            slots,
            rules: MatchRules::default(),
            army,
            spawn: None,
            spawn_buildings: None,
            dummies: None,
            dummies_attack: false,
            cah_hero: None,
            random_seed,
            resolved: None,
        }
    }

    #[must_use]
    pub(crate) fn with_more_opponents(mut self, factions: &[String], ai: bool) -> Self {
        for faction in factions {
            self.slots.push(SlotSetup {
                faction: Some(faction.clone()).filter(|name| !name.eq_ignore_ascii_case("random")),
                ..SlotSetup::new(if ai {
                    Occupant::Ai(Difficulty::Easy)
                } else {
                    Occupant::Human
                })
            });
        }
        self
    }

    #[must_use]
    pub(crate) fn with_spawn_flags(
        mut self,
        spawn: Option<String>,
        spawn_buildings: Option<String>,
        dummies: Option<String>,
    ) -> Self {
        self.spawn = spawn;
        self.spawn_buildings = spawn_buildings;
        self.dummies = dummies;
        self
    }

    #[must_use]
    pub(crate) fn with_cah_hero(mut self, id: Option<String>) -> Self {
        self.cah_hero = id;
        self
    }

    #[must_use]
    pub(crate) fn with_dummies_attack(mut self, attack: bool) -> Self {
        self.dummies_attack = attack;
        self
    }

    pub(crate) fn with_starting_money(mut self, money: Option<i64>) -> Self {
        if let Some(money) = money {
            self.rules.starting_money = money;
        }
        self
    }

    pub(crate) fn players(&self) -> usize {
        self.slots.iter().filter(|s| s.occupant.occupied()).count()
    }

    pub(crate) fn colour_taken(&self, colour: u8, except: usize) -> bool {
        self.slots
            .iter()
            .enumerate()
            .any(|(i, s)| i != except && s.occupant.occupied() && s.colour == Some(colour))
    }

    pub(crate) fn start_taken(&self, start: u8, except: usize) -> bool {
        self.slots
            .iter()
            .enumerate()
            .any(|(i, s)| i != except && s.occupant.occupied() && s.start == Some(start))
    }

    fn check_slot(&self, index: usize, catalogue: &Catalogue) -> Result<(), SetupError> {
        let Some(slot) = self.slots.get(index) else {
            return Ok(());
        };
        if !slot.occupant.occupied() {
            return Ok(());
        }
        if let Some(name) = &slot.faction {
            if !catalogue.factions.iter().any(|f| f == name) {
                return Err(SetupError::UnknownFaction(name.clone()));
            }
        }
        if let HeroPick::Stored(h) = self.hero_of(slot) {
            let entry = catalogue.heroes.get(h).ok_or(SetupError::UnknownHero(h))?;
            let faction = slot
                .faction
                .as_deref()
                .and_then(|name| catalogue.faction_bit(name));
            if !entry.usable_by(faction) {
                return Err(SetupError::HeroNotUsable(h));
            }
        }
        if let Some(colour) = slot.colour {
            if colour >= catalogue.colours {
                return Err(SetupError::ColourOutOfRange(colour));
            }
            if self.colour_taken(colour, index) {
                return Err(SetupError::ColourTaken(colour));
            }
        }
        if let Some(start) = slot.start {
            if start >= catalogue.starts {
                return Err(SetupError::StartOutOfRange(start));
            }
            if self.start_taken(start, index) {
                return Err(SetupError::StartTaken(start));
            }
        }
        if let Some(team) = slot.team {
            if team >= TEAMS {
                return Err(SetupError::TeamOutOfRange(team));
            }
        }
        if slot.handicap > HANDICAP_MAX || slot.handicap % HANDICAP_STEP != 0 {
            return Err(SetupError::Handicap(slot.handicap));
        }
        Ok(())
    }

    pub(crate) fn validate(&self, catalogue: &Catalogue) -> Result<(), SetupError> {
        if self.slots.len() > MAX_SLOTS {
            return Err(SetupError::TooManySlots(self.slots.len()));
        }
        let players = self.players();
        if players < 2 {
            return Err(SetupError::TooFewPlayers(players));
        }
        if players > usize::from(catalogue.starts) {
            return Err(SetupError::TooManyPlayers {
                players,
                starts: catalogue.starts,
            });
        }
        check_starting_money(self.rules.starting_money)?;
        if self.rules.command_point_factor == 0 {
            return Err(SetupError::CommandPointFactor(0));
        }
        for index in 0..self.slots.len() {
            self.check_slot(index, catalogue)?;
        }
        Ok(())
    }

    pub(crate) fn try_edit(
        &mut self,
        index: usize,
        catalogue: &Catalogue,
        edit: impl FnOnce(&mut SlotSetup),
    ) -> bool {
        let Some(slot) = self.slots.get(index) else {
            return false;
        };
        let before = slot.clone();
        let occupied_before = before.occupant.occupied();
        if let Some(slot) = self.slots.get_mut(index) {
            edit(slot);
            if slot.occupant.occupied() && (!occupied_before || slot.faction != before.faction) {
                let faction = slot
                    .faction
                    .as_deref()
                    .and_then(|name| catalogue.faction_bit(name));
                slot.hero = setup_heroes::follow_army(slot.hero, faction, &catalogue.heroes);
            }
        }
        let now = self.slots.get(index).cloned().unwrap_or(before.clone());
        let room = now.occupant.occupied()
            && !occupied_before
            && self.players() > usize::from(catalogue.starts);
        let human_moved = now.occupant == Occupant::Human && index != 0
            || before.occupant == Occupant::Human && now.occupant != Occupant::Human;
        if room || human_moved || self.check_slot(index, catalogue).is_err() {
            if let Some(slot) = self.slots.get_mut(index) {
                *slot = before;
            }
            return false;
        }
        if !now.occupant.occupied() {
            if let Some(slot) = self.slots.get_mut(index) {
                *slot = SlotSetup::closed();
            }
        }
        self.slots.get(index) != Some(&before)
    }

    pub(crate) fn click_start(&mut self, start: u8, catalogue: &Catalogue) -> bool {
        if start >= catalogue.starts {
            return false;
        }
        let random_after = |setup: &Self, from: usize| {
            (from..setup.slots.len()).find(|&i| {
                setup
                    .slots
                    .get(i)
                    .is_some_and(|s| s.occupant.occupied() && s.start.is_none())
            })
        };
        let holder = self
            .slots
            .iter()
            .position(|s| s.occupant.occupied() && s.start == Some(start));
        let (from, to) = match holder {
            Some(holder) => (Some(holder), random_after(self, holder.saturating_add(1))),
            None => (None, random_after(self, 0).or(Some(0))),
        };
        if let Some(slot) = from.and_then(|i| self.slots.get_mut(i)) {
            slot.start = None;
        }
        if let Some(slot) = to.and_then(|i| self.slots.get_mut(i)) {
            if slot.occupant.occupied() {
                slot.start = Some(start);
            }
        }
        from.is_some()
            || to.is_some_and(|i| self.slots.get(i).is_some_and(|s| s.start == Some(start)))
    }

    pub(crate) fn clear_start(&mut self, start: u8) -> bool {
        match self
            .slots
            .iter_mut()
            .find(|s| s.occupant.occupied() && s.start == Some(start))
        {
            Some(slot) => {
                slot.start = None;
                true
            }
            None => false,
        }
    }

    pub(crate) fn fit_to_map(&mut self, starts: u8) {
        while self.players() > usize::from(starts).max(1) {
            let Some(last) = self
                .slots
                .iter()
                .rposition(|s| matches!(s.occupant, Occupant::Ai(_)))
            else {
                break;
            };
            if let Some(slot) = self.slots.get_mut(last) {
                *slot = SlotSetup::closed();
            }
        }
        for slot in &mut self.slots {
            if slot.start.is_some_and(|s| s >= starts) {
                slot.start = None;
            }
        }
    }

    pub(crate) fn resolved_players(
        &self,
        catalogue: &Catalogue,
        starts: &[[f32; 2]],
    ) -> Result<Vec<ResolvedPlayer>, SetupError> {
        match &self.resolved {
            Some(players) => Ok(players.clone()),
            None => self.resolve(self.random_seed, catalogue, starts),
        }
    }

    pub(crate) fn hero_of(&self, slot: &SlotSetup) -> HeroPick {
        if self.rules.allow_custom_heroes {
            slot.hero
        } else {
            HeroPick::None
        }
    }

    pub(crate) fn resolve(
        &self,
        seed: u64,
        catalogue: &Catalogue,
        starts: &[[f32; 2]],
    ) -> Result<Vec<ResolvedPlayer>, SetupError> {
        self.validate(catalogue)?;
        let mut rng = Pcg32::with_stream(seed, 0x5E7A_0B5E);
        let below = |rng: &mut Pcg32, n: usize| -> usize {
            usize::try_from(rng.below(u32::try_from(n).unwrap_or(u32::MAX).max(1))).unwrap_or(0)
        };
        let mut taken_colours: Vec<u8> = self
            .slots
            .iter()
            .filter(|s| s.occupant.occupied())
            .filter_map(|s| s.colour)
            .collect();
        let mut out = Vec::new();
        for (slot_index, slot) in self.slots.iter().enumerate() {
            if !slot.occupant.occupied() {
                continue;
            }
            out.push(ResolvedPlayer {
                slot: slot_index,
                player: PlayerId::new(u8::try_from(out.len()).unwrap_or(u8::MAX)),
                faction: slot.faction.clone().unwrap_or_default(),
                hero_pick: self.hero_of(slot),
                hero: None,
                team: slot.team,
                colour: slot.colour.unwrap_or(u8::MAX),
                handicap: slot.handicap,
                start: slot.start.unwrap_or(u8::MAX),
                ai: match slot.occupant {
                    Occupant::Ai(d) => Some(d),
                    _ => None,
                },
            });
        }
        assign_starts(&mut out, starts, catalogue.starts, &mut rng);
        for player in &mut out {
            let Some(slot) = self.slots.get(player.slot) else {
                continue;
            };
            if slot.faction.is_none() {
                let usable_by_hero = |name: &String| match self.hero_of(slot) {
                    HeroPick::Stored(h) => catalogue
                        .heroes
                        .get(h)
                        .is_some_and(|hero| hero.usable_by(catalogue.faction_bit(name))),
                    HeroPick::None | HeroPick::Random => true,
                };
                let narrowed: Vec<&String> = catalogue
                    .random_pool
                    .iter()
                    .filter(|name| usable_by_hero(name))
                    .collect();
                let pool: Vec<&String> = if narrowed.is_empty() {
                    catalogue.random_pool.iter().collect()
                } else {
                    narrowed
                };
                player.faction = pool
                    .get(below(&mut rng, pool.len()))
                    .map(|name| (*name).clone())
                    .unwrap_or_default();
            }
            if slot.colour.is_none() {
                let free: Vec<u8> = (0..catalogue.colours)
                    .filter(|c| !taken_colours.contains(c))
                    .collect();
                let pick = free.get(below(&mut rng, free.len())).copied().unwrap_or(0);
                taken_colours.push(pick);
                player.colour = pick;
            }
        }
        setup_heroes::pick_random_heroes(&mut out, catalogue, seed);
        for player in &mut out {
            if let HeroPick::Stored(h) = player.hero_pick {
                player.hero = catalogue
                    .heroes
                    .get(h)
                    .map(|hero| CarriedHero(hero.record.clone()));
            }
        }
        Ok(out)
    }
}

fn assign_starts(
    players: &mut [ResolvedPlayer],
    waypoints: &[[f32; 2]],
    starts: u8,
    rng: &mut Pcg32,
) {
    let n = usize::from(starts);
    let mut taken = vec![false; n];
    for p in players.iter() {
        if let Some(t) = taken.get_mut(usize::from(p.start)) {
            *t = true;
        }
    }
    let mut picked = taken.iter().any(|t| *t);
    let distance = |a: usize, b: usize| -> f32 {
        match (waypoints.get(a), waypoints.get(b)) {
            (Some(p), Some(q)) => {
                let (dx, dy) = (p[0] - q[0], p[1] - q[1]);
                (dx * dx + dy * dy).sqrt()
            }
            _ => 1_000_000.0,
        }
    };
    let mut team_at: [Option<usize>; TEAMS as usize] = [None; TEAMS as usize];
    for p in players.iter_mut() {
        if usize::from(p.start) < n {
            continue;
        }
        let free: Vec<usize> = (0..n)
            .filter(|i| !taken.get(*i).copied().unwrap_or(true))
            .collect();
        if free.is_empty() {
            break;
        }
        let team_first = p
            .team
            .and_then(|t| team_at.get(usize::from(t)).copied().flatten());
        let pick = if !picked {
            let at =
                usize::try_from(rng.below(u32::try_from(free.len()).unwrap_or(1))).unwrap_or(0);
            free.get(at).copied().unwrap_or(0)
        } else if let Some(first) = team_first {
            let mut best = (f32::MAX, free.first().copied().unwrap_or(0));
            for &i in &free {
                let d = distance(first, i);
                if d < best.0 {
                    best = (d, i);
                }
            }
            best.1
        } else {
            let mut best = (f32::MIN, free.first().copied().unwrap_or(0));
            for &i in &free {
                let d: f32 = (0..n)
                    .filter(|m| *m != i && taken.get(*m).copied().unwrap_or(false))
                    .map(|m| distance(i, m))
                    .sum();
                if d > best.0 {
                    best = (d, i);
                }
            }
            best.1
        };
        picked = true;
        if let Some(t) = taken.get_mut(pick) {
            *t = true;
        }
        p.start = u8::try_from(pick).unwrap_or(0);
        if let Some(team) = p.team {
            if let Some(slot) = team_at.get_mut(usize::from(team)) {
                slot.get_or_insert(pick);
            }
        }
    }
}

pub(crate) fn random_pool(data: &crate::game_data::GameData) -> Vec<String> {
    data.players
        .iter()
        .filter(|def| def.playable == Some(true))
        .map(|def| def.name.clone())
        .collect()
}

pub(crate) fn template_sides(data: &crate::game_data::GameData) -> Vec<(String, u8)> {
    data.players
        .iter()
        .filter(|def| def.playable == Some(true))
        .map(|def| {
            let side = def.side.as_deref().unwrap_or(&def.name);
            (def.name.clone(), setup_heroes::faction_bit(side))
        })
        .collect()
}

pub(crate) fn playable_factions(data: &crate::game_data::GameData) -> Vec<String> {
    let mut sides: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for def in data.players.iter() {
        if def.playable != Some(true) || def.starting_building.is_none() {
            continue;
        }
        let side = def.side.clone().unwrap_or_else(|| def.name.clone());
        if sides.contains(&side) {
            continue;
        }
        sides.push(side);
        out.push(def.name.clone());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedHero(pub(crate) std::sync::Arc<mint_sage_cah::HeroRecord>);

impl std::ops::Deref for CarriedHero {
    type Target = mint_sage_cah::HeroRecord;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::hash::Hash for CarriedHero {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        mint_sage_cah::write(&self.0).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ResolvedPlayer {
    pub(crate) slot: usize,
    pub(crate) player: PlayerId,
    pub(crate) faction: String,
    pub(crate) hero_pick: HeroPick,
    pub(crate) hero: Option<CarriedHero>,
    pub(crate) team: Option<u8>,
    pub(crate) colour: u8,
    pub(crate) handicap: u8,
    pub(crate) start: u8,
    pub(crate) ai: Option<Difficulty>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
