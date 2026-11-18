#![allow(clippy::arithmetic_side_effects)]

mod decisions;
mod session;

use mint_math::rng::Pcg32;
use mint_sage_ini::living_world::{load_files, LivingWorldData};
use mint_sage_ini::MemorySources;

use crate::ai::{Prediction, Predictor, Scale};
use crate::battle::BattleSpec;
use crate::model::{PlayerId, PlayerKind, RegionId};
use crate::rules::{Catalog, PriceClass, UnitInfo};
use crate::setup::{PlayerSetup, Setup};
use crate::state::LivingWorldState;

pub(crate) const P0: PlayerId = PlayerId(0);
pub(crate) const P1: PlayerId = PlayerId(1);

const NAMES: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

fn ai_template(extra: &str) -> String {
    format!(
        "LivingWorldAITemplate DefaultAITemplate\n DesiredSoldierRatio = 3\n DesiredHeroRatio = 1\n{extra}End\n"
    )
}

const BUILDINGS: &str = "
LivingWorldBuilding Farm
 Type = Resource
 TurnsToBuild = 1
 StrategicResourceCost = 50
 BuildingNugget IncreaseCommandPoints Cp
  Type = WORLD
  Amount = 2
 End
 BuildingNugget IncreaseTreasury Gold
  TreasureAmount = 30
 End
End
LivingWorldBuilding Keep
 Type = Fortress
 TurnsToBuild = 1
 StrategicResourceCost = 80
 CanDefendTerritory = Yes
 BuildingNugget StrengthenArmy Str
  StrengtheningRange = THIS_TERRIORITY
  BonusKey = K
 End
End
LivingWorldBuilding Armory
 Type = Armory
 TurnsToBuild = 1
 StrategicResourceCost = 60
 BuildingNugget UpgradeTroops Up
  NumUpgradesPerTurn = 1
  UpgradeableUnits = elf
 End
End
LivingWorldBuilding Barracks
 Type = Barracks
 TurnsToBuild = 1
 StrategicResourceCost = 40
 BuildingNugget SpawnArmy Q
  QueueSize = 2
  ArmyToSpawn
   PlayerArmy = Squad
   BuildTime = 1
  End
  ArmyToSpawn
   PlayerArmy = ChampionA
   HeroTemplateName = hero_a
   BuildTime = 2
  End
  ArmyToSpawn
   PlayerArmy = ChampionB
   HeroTemplateName = hero_b
   BuildTime = 2
  End
  ArmyToSpawn
   PlayerArmy = ChampionC
   BuildTime = 2
  End
 End
End
LivingWorldPlayerArmy
 Name = Squad
 ArmyEntry
  ThingTemplate = elf
  Quantity = 2
 End
End
LivingWorldPlayerArmy
 Name = ChampionA
 ArmyEntry
  ThingTemplate = hero_a
  Quantity = 1
 End
End
LivingWorldPlayerArmy
 Name = ChampionB
 ArmyEntry
  ThingTemplate = hero_b
  Quantity = 1
 End
End
LivingWorldPlayerArmy
 Name = ChampionC
 ArmyEntry
  ThingTemplate = hero_c
  Quantity = 1
 End
End
";

fn templates() -> String {
    let mut text = String::new();
    for name in ["Elves", "Orcs", "Men", "Dwarves", "Goblins", "Ents"] {
        text.push_str(&format!(
            "LivingWorldPlayerTemplate {name}\n Faction = {name}\n StartingWorldCP = 4\n StartingTacticalCP = 5\n \
             MaxWorldCP = 9\n MaxTacticalCP = 8\n MaxHeroCP = 6\n ScenarioStartResources = 100\n \
             ScenarioMaxResources = 1000\nEnd\n"
        ));
    }
    text
}

fn regions(count: usize) -> String {
    let mut text = String::from("LivingWorldRegionCampaign World\n");
    for (i, name) in NAMES.iter().take(count).enumerate() {
        text.push_str(&format!(
            " Region {name}\n  CPLimit = 3\n  AllyCPLimit = 2\n  StrategicValue = 5\n"
        ));
        if i > 0 {
            text.push_str(&format!("  ConnectsTo = {}\n", NAMES[i - 1]));
        }
        if i + 1 < count {
            text.push_str(&format!("  ConnectsTo = {}\n", NAMES[i + 1]));
        }
        let spots = if *name == "E" { 3 } else { 2 };
        for s in 0..spots {
            text.push_str(&format!("  BuildingSpot = X:{s} Y:{s}\n"));
        }
        text.push_str(" End\n");
    }
    text.push_str("End\n");
    text
}

fn campaign(players: usize) -> String {
    let teams: Vec<String> = (1..=players).map(|t| t.to_string()).collect();
    format!(
        "LivingWorldCampaign Camp\n LivingWorldVictoryType\n  PlayerDefeatCondition\n   Teams = {}\n   \
         NumControlledRegionsLessOrEqualTo = 0\n  End\n End\n Scenario\n  RegionCampaign = World\n  \
         MinPlayers = 2\n  MaxPlayers = 6\n  UseMpRulesVictoryCondition = Yes\n End\nEnd\n",
        teams.join(" ")
    )
}

pub(crate) fn data(players: usize, regions_count: usize, ai_extra: &str) -> LivingWorldData {
    let mut sources = MemorySources::new();
    let text = format!(
        "{}{BUILDINGS}{}{}{}",
        templates(),
        ai_template(ai_extra),
        regions(regions_count),
        campaign(players)
    );
    sources.insert("data/ini/t.ini", text.into_bytes());
    let load = load_files(&sources, &["data/ini/t.ini".to_owned()]);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    load.data
}

fn unit(command_points: i32, build_cost: i32, kind: u8) -> UnitInfo {
    UnitInfo {
        command_points,
        build_cost,
        price_class: if kind == 5 {
            PriceClass::Hero
        } else {
            PriceClass::Other
        },
        hero: kind == 5,
        unit_type: Some(kind),
        counter_target: None,
    }
}

pub(crate) fn catalog() -> Catalog {
    let mut catalog = Catalog {
        resource_multiplier_limit: 2.0,
        ..Catalog::default()
    };
    for (name, info) in [
        ("hero_a", unit(3, 0, 5)),
        ("hero_b", unit(2, 0, 5)),
        ("hero_c", unit(2, 0, 5)),
        ("elf", unit(1, 10, 0)),
        ("orc", unit(1, 10, 0)),
    ] {
        catalog.units.insert(name.to_owned(), info);
    }
    catalog
}

const FACTIONS: [&str; 6] = ["Elves", "Orcs", "Men", "Dwarves", "Goblins", "Ents"];

pub(crate) fn session(
    seats: &[(PlayerKind, &str)],
    regions_count: usize,
    ai_extra: &str,
    seed: u64,
) -> LivingWorldState {
    let players = seats
        .iter()
        .enumerate()
        .map(|(i, (kind, start))| PlayerSetup {
            template: FACTIONS[i].to_owned(),
            kind: *kind,
            team: i32::try_from(i + 1).unwrap(),
            start_region: Some((*start).to_owned()),
            skill: crate::ai::Skill::Medium,
        })
        .collect();
    let setup = Setup {
        campaign: "Camp".to_owned(),
        scenario: 0,
        victory_type: 0,
        battle_type: crate::screen_queries::BattleType::default(),
        seed,
        players,
    };
    LivingWorldState::start(
        &data(seats.len(), regions_count, ai_extra),
        &catalog(),
        &setup,
    )
    .unwrap()
}

pub(crate) fn rid(state: &LivingWorldState, name: &str) -> RegionId {
    state.rules.region(name).unwrap()
}

pub(crate) struct Verdict {
    pub win: bool,
    pub rounds: u32,
    pub asked: Vec<(u8, Option<Scale>)>,
}

impl Verdict {
    pub(crate) fn new(win: bool, rounds: u32) -> Self {
        Self {
            win,
            rounds,
            asked: Vec::new(),
        }
    }
}

impl Predictor for Verdict {
    fn predict(
        &mut self,
        _spec: &BattleSpec,
        ai_side: u8,
        scale: Option<Scale>,
        _rng: &mut Pcg32,
    ) -> Prediction {
        self.asked.push((ai_side, scale));
        Prediction {
            winner: Some(if self.win { ai_side } else { 1 - ai_side }),
            rounds: self.rounds,
        }
    }
}
