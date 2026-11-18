#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use mint_sage_ini::living_world::campaign::Conditions;
use mint_sage_ini::living_world::players::{
    BuildingNuggetKind, BuildingType, CommandPointKind, SpawnArmyDef,
};
use mint_sage_ini::living_world::LivingWorldData;

use crate::model::{Bonus, BuildingId, RegionId, TemplateId, UnitId};

const DEFAULT_MAX_RESOURCES: i32 = 25_000;
const DEFAULT_ARMY_RETREAT_ROUNDS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupError {
    UnknownCampaign(String),
    UnknownScenario(usize),
    NoRegionCampaign(String),
    UnknownRegion(String),
    UnknownTemplate(String),
    TooFewPlayers { min: i32, got: usize },
    TooManyPlayers { max: i32, got: usize },
    DisabledFaction(String),
    RandomFaction(usize),
    NoStartRegion(usize),
    NoPlayableFaction(usize),
    UnknownSeat(usize),
    TooFewTeams,
    InvalidPlayerLimits,
    StartRegionDisabled(String),
    StartRegionShared(String),
    TooManyPlayersForIds,
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SetupError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriceClass {
    #[default]
    Other,
    Siege,
    Hero,
    Fortress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UnitInfo {
    pub command_points: i32,
    pub build_cost: i32,
    pub price_class: PriceClass,
    pub hero: bool,
    pub unit_type: Option<u8>,
    pub counter_target: Option<u8>,
}

impl UnitInfo {
    pub fn set_resolve_type(&mut self, name: &str) {
        self.unit_type = mint_sage_ini::living_world::auto_resolve::UnitType::NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .and_then(|i| u8::try_from(i).ok());
        if name.eq_ignore_ascii_case("AutoResolveUnit_Siege") {
            self.price_class = PriceClass::Siege;
        } else if name.eq_ignore_ascii_case("AutoResolveUnit_Fortress") {
            self.price_class = PriceClass::Fortress;
        }
    }
}

pub const RESOLVE_TYPE_HERO: u8 = 5;

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub units: BTreeMap<String, UnitInfo>,
    pub portraits: BTreeMap<String, String>,
    pub resource_multiplier_limit: f32,
    pub playable_factions: BTreeSet<String>,
    pub ranks: Vec<RankGrant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RankGrant {
    pub threshold: f32,
    pub granted: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionRules {
    pub name: String,
    pub strategic_value: i32,
    pub map: Option<String>,
    pub campaign_map: bool,
    pub lose_campaign_if_lose_map: bool,
    pub cp_limit: i32,
    pub ally_cp_limit: i32,
    pub create_auto_fort: bool,
    pub plots: usize,
    pub bonus: Bonus,
    pub neighbours: Vec<RegionId>,
    pub restrict: Vec<(Vec<BuildingType>, Option<i32>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Territory {
    pub name: String,
    pub regions: Vec<RegionId>,
    pub bonus: Bonus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateRules {
    pub name: String,
    pub faction: Option<String>,
    pub dozer: Option<String>,
    pub inn_unit: Option<String>,
    pub start_world_cp: i32,
    pub start_tactical_cp: i32,
    pub max_world_cp: i32,
    pub max_tactical_cp: i32,
    pub max_hero_cp: i32,
    pub start_resources: i32,
    pub max_resources: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedArmy {
    pub build_time: i32,
    pub hero: Option<UnitId>,
    pub roster: Vec<(UnitId, u32)>,
    pub is_city: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRules {
    pub size: i32,
    pub entries: Vec<QueuedArmy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildingRules {
    pub name: String,
    pub battle_thing: Option<String>,
    pub turns_to_build: i32,
    pub cost: i32,
    pub can_defend: bool,
    pub kind: Option<BuildingType>,
    pub available_to: Vec<TemplateId>,
    pub treasury: Vec<i32>,
    pub world_cp: i32,
    pub tactical_cp: i32,
    pub upgrade: Option<(i32, Vec<UnitId>)>,
    pub queue: Option<QueueRules>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerDefeat {
    pub teams: Vec<u32>,
    pub lose_if_capital_lost: bool,
    pub max_regions: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamDefeat {
    pub teams: Vec<u32>,
    pub max_regions: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamVictory {
    pub teams: Vec<u32>,
    pub min_regions: Option<i32>,
    pub controlled: Vec<RegionId>,
    pub held_turns: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConditionSet {
    pub player_defeat: Vec<PlayerDefeat>,
    pub team_defeat: Vec<TeamDefeat>,
    pub team_victory: Vec<TeamVictory>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rules {
    pub regions: Vec<RegionRules>,
    pub territories: Vec<Territory>,
    pub templates: Vec<TemplateRules>,
    pub buildings: Vec<BuildingRules>,
    pub unit_names: Vec<String>,
    pub units: Vec<UnitInfo>,
    portraits: BTreeMap<String, String>,
    pub conditions: ConditionSet,
    pub ai: crate::ai::AiRules,
    pub resource_multiplier_limit: f32,
    pub ranks: Vec<RankGrant>,
    pub campaign: String,
    pub scenario: usize,
    pub victory_type: usize,
    pub seconds_per_reinforcement: i32,
    pub starting_cash_rts: i32,
    pub starting_cash_rts_with_fort: i32,
    pub revival_cost_multiplier: f32,
    pub revival_time_multiplier: f32,
    pub army_retreat_rounds: u32,
    unit_ids: BTreeMap<String, UnitId>,
    region_ids: BTreeMap<String, RegionId>,
}

pub(crate) fn army_roster(
    data: &LivingWorldData,
    def: &SpawnArmyDef,
) -> (Option<String>, Vec<(String, u32)>) {
    let mut entries: Vec<(String, u32)> = Vec::new();
    if let Some(army) = def
        .player_army
        .as_deref()
        .and_then(|name| data.armies.get(name))
    {
        for entry in &army.entries {
            if let Some(unit) = &entry.thing_template {
                let quantity = u32::try_from(entry.quantity.unwrap_or(1)).unwrap_or(1);
                entries.push((unit.clone(), quantity.max(1)));
            }
        }
    }
    let hero = def.hero_template_name.clone().filter(|h| !h.is_empty());
    if let Some(h) = &hero {
        if !entries.iter().any(|(n, _)| n.eq_ignore_ascii_case(h)) {
            entries.push((h.clone(), 1));
        }
    }
    (hero, entries)
}

fn lower(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn cash(value: Option<u32>) -> i32 {
    value.map_or(0, |v| i32::try_from(v).unwrap_or(i32::MAX))
}

impl Rules {
    pub fn build(
        data: &LivingWorldData,
        catalog: &Catalog,
        campaign: &str,
        scenario: usize,
        victory_type: usize,
    ) -> Result<Self, SetupError> {
        let def = data
            .campaigns
            .get(campaign)
            .ok_or_else(|| SetupError::UnknownCampaign(campaign.to_owned()))?;
        let scenario_def = def
            .scenarios
            .get(scenario)
            .ok_or(SetupError::UnknownScenario(scenario))?;
        let region_campaign_name = scenario_def.region_campaign.clone().unwrap_or_default();
        let region_campaign = data
            .region_campaigns
            .get(&region_campaign_name)
            .ok_or_else(|| SetupError::NoRegionCampaign(region_campaign_name.clone()))?;

        let mut region_ids = BTreeMap::new();
        for (index, region) in region_campaign.regions.iter().enumerate() {
            let id = u16::try_from(index).map_err(|_| SetupError::TooManyPlayersForIds)?;
            region_ids.insert(lower(&region.name), RegionId(id));
        }
        let region_id = |name: &str| {
            region_ids
                .get(&lower(name))
                .copied()
                .ok_or_else(|| SetupError::UnknownRegion(name.to_owned()))
        };

        let mut regions = Vec::new();
        for region in &region_campaign.regions {
            let mut neighbours = Vec::new();
            for name in region.connected() {
                neighbours.push(region_id(name)?);
            }
            regions.push(RegionRules {
                name: region.name.clone(),
                strategic_value: region.strategic_value.unwrap_or(0),
                map: region.map_name.clone(),
                campaign_map: region.map_is_campaign_map.unwrap_or(false),
                lose_campaign_if_lose_map: region.lose_campaign_if_lose_map.unwrap_or(false),
                cp_limit: region.cp_limit.unwrap_or(0),
                ally_cp_limit: region.ally_cp_limit.unwrap_or(0),
                create_auto_fort: region.create_auto_fort.unwrap_or(false),
                plots: region.building_spot.len(),
                bonus: crate::bonus_text::region_bonus(region),
                neighbours,
                restrict: region
                    .restrict_buildings
                    .iter()
                    .map(|r| (r.buildings.0.clone(), r.number_allowed))
                    .collect(),
            });
        }

        let mut territories = Vec::new();
        for t in &region_campaign.concurrent_bonuses {
            let mut listed = Vec::new();
            for name in &t.regions.0 {
                listed.push(region_id(name)?);
            }
            territories.push(Territory {
                name: t.territory.clone().unwrap_or_else(|| t.name.clone()),
                regions: listed,
                bonus: crate::bonus_text::territory_bonus(t),
            });
        }

        let templates: Vec<TemplateRules> = data
            .player_templates
            .iter()
            .map(|t| TemplateRules {
                name: t.name.clone(),
                faction: t.faction.clone(),
                dozer: t.faction_dozer_template_name.clone(),
                inn_unit: t.faction_inn_unit_template_name.clone(),
                start_world_cp: t.starting_world_cp.unwrap_or(0),
                start_tactical_cp: t.starting_tactical_cp.unwrap_or(0),
                max_world_cp: t.max_world_cp.unwrap_or(0),
                max_tactical_cp: t.max_tactical_cp.unwrap_or(0),
                max_hero_cp: t.max_hero_cp.unwrap_or(0),
                start_resources: t.scenario_start_resources.unwrap_or(0),
                max_resources: t.scenario_max_resources.unwrap_or(DEFAULT_MAX_RESOURCES),
            })
            .collect();

        let mut rules = Self {
            regions,
            territories,
            templates,
            buildings: Vec::new(),
            unit_names: Vec::new(),
            units: Vec::new(),
            portraits: catalog.portraits.clone(),
            conditions: ConditionSet::default(),
            ai: crate::ai::AiRules::default(),
            resource_multiplier_limit: catalog.resource_multiplier_limit,
            ranks: catalog.ranks.clone(),
            campaign: def.name.clone(),
            scenario,
            victory_type,
            seconds_per_reinforcement: def.seconds_per_reinforcement.unwrap_or(0),
            starting_cash_rts: cash(def.starting_cash_rts),
            starting_cash_rts_with_fort: cash(def.starting_cash_rts_with_fort),
            revival_cost_multiplier: def.initial_revival_cost_multiplier.unwrap_or(1.0),
            revival_time_multiplier: def.initial_revival_time_multiplier.unwrap_or(1.0),
            army_retreat_rounds: region_campaign
                .army_retreat_rounds
                .map_or(DEFAULT_ARMY_RETREAT_ROUNDS, |r| {
                    u32::try_from(r).unwrap_or(0)
                }),
            unit_ids: BTreeMap::new(),
            region_ids: region_ids.clone(),
        };
        for (name, info) in &catalog.units {
            rules.intern_with(name, Some(*info));
        }

        for b in data.buildings.iter() {
            let mut treasury = Vec::new();
            let (mut world_cp, mut tactical_cp) = (0i32, 0i32);
            let mut upgrade = None;
            let mut queue = None;
            for nugget in &b.nuggets {
                match &nugget.kind {
                    BuildingNuggetKind::IncreaseTreasury(t) => {
                        treasury.push(t.treasure_amount.unwrap_or(0));
                    }
                    BuildingNuggetKind::IncreaseCommandPoints(c) => {
                        let amount = c.amount.unwrap_or(0);
                        match c.kind {
                            Some(CommandPointKind::World) => {
                                world_cp = world_cp.saturating_add(amount);
                            }
                            Some(CommandPointKind::Tactical) => {
                                tactical_cp = tactical_cp.saturating_add(amount);
                            }
                            None => {}
                        }
                    }
                    BuildingNuggetKind::UpgradeTroops(u) => {
                        let units = u
                            .upgradeable_units
                            .0
                            .iter()
                            .map(|n| rules.intern(n))
                            .collect();
                        upgrade = Some((u.num_upgrades_per_turn.unwrap_or(0), units));
                    }
                    BuildingNuggetKind::SpawnArmy(spawn) => {
                        let entries = spawn
                            .armies
                            .iter()
                            .map(|def| {
                                let (hero, named) = army_roster(data, def);
                                QueuedArmy {
                                    build_time: def.build_time.unwrap_or(1),
                                    hero: hero.as_deref().map(|h| rules.intern(h)),
                                    roster: named
                                        .iter()
                                        .map(|(n, count)| (rules.intern(n), *count))
                                        .collect(),
                                    is_city: def.is_city.unwrap_or(false),
                                }
                            })
                            .collect();
                        queue = Some(QueueRules {
                            size: spawn.queue_size.unwrap_or(0),
                            entries,
                        });
                    }
                    BuildingNuggetKind::StrengthenArmy(_) => {}
                }
            }
            let available_to = b
                .available_to
                .0
                .iter()
                .filter_map(|n| rules.template_id(n))
                .collect();
            rules.buildings.push(BuildingRules {
                name: b.name.clone(),
                battle_thing: b.battle_thing_template.clone(),
                turns_to_build: b.turns_to_build.unwrap_or(0),
                cost: b.strategic_resource_cost.unwrap_or(0),
                can_defend: b.can_defend_territory.unwrap_or(false),
                kind: b.building_type,
                available_to,
                treasury,
                world_cp,
                tactical_cp,
                upgrade,
                queue,
            });
        }

        rules.ai = crate::ai::data::build(data, &mut rules);

        let source: &Conditions = if scenario_def.use_mp_rules_victory_condition == Some(true) {
            def.victory_types
                .get(victory_type)
                .or_else(|| def.victory_types.first())
                .map_or(&scenario_def.conditions, |v| &v.conditions)
        } else {
            &scenario_def.conditions
        };
        let resolve = |names: &[String]| -> Result<Vec<RegionId>, SetupError> {
            names.iter().map(|n| rules.region(n)).collect()
        };
        let mut set = ConditionSet::default();
        for c in &source.player_defeat {
            set.player_defeat.push(PlayerDefeat {
                teams: c.teams.0.clone(),
                lose_if_capital_lost: c.lose_if_capital_lost.unwrap_or(false),
                max_regions: c.num_controlled_regions_less_or_equal_to,
            });
        }
        for c in &source.team_defeat {
            set.team_defeat.push(TeamDefeat {
                teams: c.teams.0.clone(),
                max_regions: c.num_controlled_regions_less_or_equal_to,
            });
        }
        for c in &source.team_victory {
            set.team_victory.push(TeamVictory {
                teams: c.teams.0.clone(),
                min_regions: c.num_controlled_regions_greater_or_equal_to,
                controlled: resolve(&c.controlled_regions.0)?,
                held_turns: c.controlled_regions_held_for_turns.unwrap_or(0),
            });
        }
        rules.conditions = set;
        Ok(rules)
    }

    pub fn region(&self, name: &str) -> Result<RegionId, SetupError> {
        self.region_ids
            .get(&lower(name))
            .copied()
            .ok_or_else(|| SetupError::UnknownRegion(name.to_owned()))
    }

    #[must_use]
    pub fn template_id(&self, name: &str) -> Option<TemplateId> {
        let index = self
            .templates
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(name))?;
        u8::try_from(index).ok().map(TemplateId)
    }

    #[must_use]
    pub fn building_id(&self, name: &str) -> Option<BuildingId> {
        let index = self
            .buildings
            .iter()
            .position(|b| b.name.eq_ignore_ascii_case(name))?;
        u16::try_from(index).ok().map(BuildingId)
    }

    pub fn intern(&mut self, name: &str) -> UnitId {
        self.intern_with(name, None)
    }

    fn intern_with(&mut self, name: &str, info: Option<UnitInfo>) -> UnitId {
        let key = lower(name);
        if let Some(id) = self.unit_ids.get(&key) {
            return *id;
        }
        let id = UnitId(u16::try_from(self.units.len()).unwrap_or(u16::MAX));
        self.unit_ids.insert(key.clone(), id);
        self.unit_names.push(key);
        self.units.push(info.unwrap_or_default());
        id
    }

    #[must_use]
    pub fn unit(&self, id: UnitId) -> UnitInfo {
        self.units
            .get(usize::from(id.0))
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn unit_portrait(&self, id: UnitId) -> Option<&str> {
        self.portraits
            .get(self.unit_names.get(usize::from(id.0))?)
            .map(String::as_str)
    }

    #[must_use]
    pub fn unit_id(&self, name: &str) -> Option<UnitId> {
        self.unit_ids.get(&lower(name)).copied()
    }

    #[must_use]
    pub fn unit_name(&self, id: UnitId) -> &str {
        self.unit_names
            .get(usize::from(id.0))
            .map_or("", String::as_str)
    }

    #[must_use]
    pub fn fresh_rank_grants(&self, points: f32) -> i32 {
        if points == 0.0 || !points.is_finite() {
            return 0;
        }
        let cap = self.ranks.last().map_or(f32::MAX, |r| r.threshold);
        let whole = points.min(cap).floor();
        self.ranks
            .iter()
            .take_while(|r| whole >= r.threshold)
            .fold(0i32, |sum, r| sum.saturating_add(r.granted))
    }

    #[must_use]
    pub fn region_rules(&self, id: RegionId) -> Option<&RegionRules> {
        self.regions.get(id.index())
    }

    #[must_use]
    pub fn building(&self, id: BuildingId) -> Option<&BuildingRules> {
        self.buildings.get(usize::from(id.0))
    }
}
