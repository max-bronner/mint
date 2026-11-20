#![allow(clippy::arithmetic_side_effects)]

mod battles;
mod economy;
mod pass6;
mod phases;
mod real_time;
mod screen_queries;
mod screens;
mod setup;
mod stand_in;
mod tutorial;

use mint_math::rng::Pcg32;
use mint_sage_ini::living_world::{load_files, LivingWorldData};
use mint_sage_ini::MemorySources;

use crate::battle::{AutoResolve, BattleSpec};
use crate::engine::apply;
use crate::model::{ArmyId, BattleResult, Command, Event, PlayerId, PlayerKind, RegionId};
use crate::rules::{Catalog, PriceClass, UnitInfo};
use crate::setup::{PlayerSetup, Setup};
use crate::state::LivingWorldState;

const TEMPLATES: &str = "
LivingWorldPlayerTemplate Elves
 Faction = Elves
 StartingWorldCP = 4
 StartingTacticalCP = 5
 MaxWorldCP = 9
 MaxTacticalCP = 8
 MaxHeroCP = 6
 ScenarioStartResources = 100
 ScenarioMaxResources = 1000
End
LivingWorldPlayerTemplate Orcs
 Faction = Orcs
 StartingWorldCP = 4
 StartingTacticalCP = 5
 MaxWorldCP = 9
 MaxTacticalCP = 8
 MaxHeroCP = 6
 ScenarioStartResources = 100
 ScenarioMaxResources = 1000
End
LivingWorldPlayerTemplate Ents
 Faction = Ents
 StartingWorldCP = 4
 StartingTacticalCP = 5
 MaxWorldCP = 9
 MaxTacticalCP = 8
 MaxHeroCP = 6
 ScenarioStartResources = 100
End
LivingWorldBuilding Farm
 Type = Resource
 TurnsToBuild = 2
 StrategicResourceCost = 50
 BuildingNugget IncreaseTreasury Gold
  TreasureAmount = 30
 End
End
LivingWorldPlayerArmy
 Name = Levy
 ArmyEntry
  ThingTemplate = elf
  Quantity = 2
 End
End
LivingWorldPlayerArmy
 Name = Champ
 ArmyEntry
  ThingTemplate = hero_a
 End
End
LivingWorldBuilding Barracks
 Type = Barracks
 BuildingNugget SpawnArmy Queue
  QueueSize = 2
  ArmyToSpawn
   PlayerArmy = Levy
   BuildTime = 2
  End
  ArmyToSpawn
   PlayerArmy = Champ
   HeroTemplateName = hero_a
   BuildTime = 1
  End
 End
End
LivingWorldBuilding Armoury
 Type = Armory
 BuildingNugget UpgradeTroops Up
  NumUpgradesPerTurn = 1
  UpgradeableUnits = elf
 End
End
LivingWorldBuilding Hall
 Type = Armory
 BuildingNugget UpgradeTroops Up
  NumUpgradesPerTurn = 1
 End
End
LivingWorldBuilding Keep
 Type = Fortress
 TurnsToBuild = 3
 StrategicResourceCost = 80
 CanDefendTerritory = Yes
 BattleThingTemplate = KeepBattle
End
";

const DEFAULT_VICTORY: &str = "
 LivingWorldVictoryType
  PlayerDefeatCondition
   Teams = 1 2
   NumControlledRegionsLessOrEqualTo = 0
  End
 End
";

fn regions() -> String {
    let mut text = String::from("LivingWorldRegionCampaign World\n");
    let names = ["A", "B", "C", "D", "E"];
    for (i, name) in names.iter().enumerate() {
        text.push_str(&format!(
            " Region {name}\n  CPLimit = 3\n  AllyCPLimit = 2\n"
        ));
        if i > 0 {
            text.push_str(&format!("  ConnectsTo = {}\n", names[i - 1]));
        }
        if i + 1 < names.len() {
            text.push_str(&format!("  ConnectsTo = {}\n", names[i + 1]));
        }
        if *name == "A" {
            text.push_str("  FertileTerritoryBonus = 7\n  ArmyBonus = 1\n");
            text.push_str("  BuildingSpot = X:1 Y:1\n  BuildingSpot = X:2 Y:2\n");
        }
        if *name == "B" {
            text.push_str("  MapIsCampaignMap = Yes\n");
        }
        if *name == "E" {
            text.push_str("  CreateAutoFort = Yes\n");
        }
        text.push_str(" End\n");
    }
    text.push_str(" ConcurrentRegionBonus Pair\n  Regions = A B\n  ArmyBonus = 5\n End\nEnd\n");
    text
}

fn data(victory: &str) -> LivingWorldData {
    data_with(victory, "")
}

fn data_with(victory: &str, scenario_extra: &str) -> LivingWorldData {
    let campaign = format!(
        "LivingWorldCampaign Camp\n SecondsPerReinforcement = 300\n StartingCashRTS = 1000\n \
         StartingCashRTSWithFort = 2000\n{victory}\n Scenario\n  RegionCampaign = World\n  MinPlayers = 2\n  \
         MaxPlayers = 2\n  UseMpRulesVictoryCondition = Yes\n  DisabledFactions = Dwarves\n{scenario_extra} End\nEnd\n"
    );
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/t.ini",
        format!("{TEMPLATES}{}{campaign}", regions()).into_bytes(),
    );
    let load = load_files(&sources, &["data/ini/t.ini".to_owned()]);
    assert!(load.errors.is_empty(), "{:?}", load.errors);
    load.data
}

fn unit(command_points: i32, build_cost: i32, hero: bool) -> UnitInfo {
    UnitInfo {
        command_points,
        build_cost,
        price_class: if hero {
            PriceClass::Hero
        } else {
            PriceClass::Other
        },
        hero,
        unit_type: Some(if hero { 5 } else { 0 }),
        ..UnitInfo::default()
    }
}

pub(crate) fn catalog() -> Catalog {
    let mut catalog = Catalog {
        resource_multiplier_limit: 2.0,
        ..Catalog::default()
    };
    for (name, info) in [
        ("hero_a", unit(3, 0, true)),
        ("hero_b", unit(2, 0, true)),
        ("elf", unit(1, 10, false)),
        ("orc", unit(1, 10, false)),
    ] {
        catalog.units.insert(name.to_owned(), info);
    }
    catalog
}

fn seat(template: &str, kind: PlayerKind, team: i32, start: &str) -> PlayerSetup {
    PlayerSetup {
        template: template.to_owned(),
        kind,
        team,
        start_region: Some(start.to_owned()),
        skill: crate::ai::Skill::Medium,
    }
}

pub(crate) fn setup(victory: &str, seed: u64) -> (LivingWorldData, Setup) {
    let setup = Setup {
        campaign: "Camp".to_owned(),
        scenario: 0,
        victory_type: 0,
        battle_type: crate::screen_queries::BattleType::default(),
        seed,
        players: vec![
            seat("Elves", PlayerKind::Human, 0, "A"),
            seat("Orcs", PlayerKind::Computer, 1, "E"),
        ],
    };
    (data(victory), setup)
}

pub(crate) fn session() -> LivingWorldState {
    session_with(DEFAULT_VICTORY)
}

pub(crate) fn session_with(victory: &str) -> LivingWorldState {
    let (data, setup) = setup(victory, 1);
    let mut state = LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    for name in ["B", "C", "D"] {
        let id = rid(&state, name);
        state.regions[id.index()].owner = Some(P0);
    }
    state
}

pub(crate) const P0: PlayerId = PlayerId(0);
pub(crate) const P1: PlayerId = PlayerId(1);

pub(crate) fn rid(state: &LivingWorldState, name: &str) -> RegionId {
    state.rules.region(name).unwrap()
}

pub(crate) fn hero_army(
    state: &mut LivingWorldState,
    player: PlayerId,
    at: &str,
    troops: u32,
) -> ArmyId {
    let region = rid(state, at);
    let (hero, troop) = if player == P0 {
        ("hero_a", "elf")
    } else {
        ("hero_b", "orc")
    };
    state.spawn_army(
        player,
        region,
        Some(hero),
        &[(hero.to_owned(), 1), (troop.to_owned(), troops)],
    )
}

pub(crate) fn place_done(state: &mut LivingWorldState, region: &str, plot: usize, building: &str) {
    let id = state.rules.building_id(building).unwrap();
    let region = rid(state, region);
    state.regions[region.index()].plots[plot] = Some(crate::model::PlotBuilding::new(id, 0, false));
}

pub(crate) fn garrison(
    state: &mut LivingWorldState,
    player: PlayerId,
    at: &str,
    troops: u32,
) -> ArmyId {
    let region = rid(state, at);
    state.spawn_army(player, region, None, &[("elf".to_owned(), troops)])
}

pub(crate) fn command(
    state: &mut LivingWorldState,
    player: PlayerId,
    command: Command,
) -> Vec<Event> {
    apply(state, player, command).unwrap()
}

pub(crate) fn end_planning(state: &mut LivingWorldState) -> Vec<Event> {
    let mut events = command(state, P0, Command::EndPhase);
    events.extend(command(state, P1, Command::EndPhase));
    events
}

pub(crate) fn boundaries(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::PhaseChanged { .. }))
        .count()
}

pub(crate) struct AttackerLoses;

impl AutoResolve for AttackerLoses {
    fn resolve(&mut self, spec: &BattleSpec, _rng: &mut Pcg32) -> BattleResult {
        BattleResult {
            winner: u8::try_from(spec.sides.len().saturating_sub(1)).ok(),
            rosters: Vec::new(),
            fallen_heroes: Vec::new(),
            fallen_records: Vec::new(),
            players: Vec::new(),
        }
    }
}
