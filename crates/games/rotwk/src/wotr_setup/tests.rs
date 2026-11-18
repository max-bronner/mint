use super::*;
use crate::lobby::SlotSetup;
use mint_sage_ini::living_world::{load_files, Tokens};
use mint_sage_ini::MemorySources;
use mint_sage_livingworld::LivingWorldState;

fn seat(template: &str, team: i32, region: Option<&str>) -> PlayerSetup {
    PlayerSetup {
        template: template.to_owned(),
        kind: PlayerKind::Computer,
        skill: Skill::Medium,
        team,
        start_region: region.map(str::to_owned),
    }
}

fn setup(players: Vec<PlayerSetup>) -> Setup {
    Setup {
        campaign: String::new(),
        scenario: 0,
        victory_type: 0,
        battle_type: BattleType::default(),
        seed: 1,
        players,
    }
}

fn scenario(min: Option<i32>, max: Option<i32>, disabled: &[&str]) -> ScenarioDef {
    ScenarioDef {
        min_players: min,
        max_players: max,
        disabled_factions: Tokens(disabled.iter().map(|s| (*s).to_owned()).collect()),
        ..ScenarioDef::default()
    }
}

fn message(key: &'static str) -> Verdict {
    Verdict::Refused(Refusal::Message(key))
}

#[test]
fn the_start_test_answers_in_the_order_of_the_screen() {
    let two = || vec![seat("A", 0, Some("r")), seat("B", 1, Some("s"))];
    let silent = Verdict::Refused(Refusal::Silent);
    let cases = [
        (scenario(None, Some(0), &[]), two(), silent),
        (scenario(Some(3), Some(2), &[]), two(), silent),
        (
            scenario(None, Some(1), &["B"]),
            two(),
            Verdict::Refused(Refusal::Counted("LAN:TooManyPlayers", 1)),
        ),
        (
            scenario(Some(3), Some(4), &["B"]),
            two(),
            Verdict::Refused(Refusal::Counted("LAN:TooFewPlayers", 3)),
        ),
        (
            scenario(None, None, &["B"]),
            vec![seat("A", 0, None), seat("B", 1, None)],
            message("GUI:DisabledFaction"),
        ),
        (
            scenario(None, None, &["B"]),
            vec![seat("", 0, Some("r")), seat("A", 1, Some("s"))],
            message("GUI:DisabledFaction"),
        ),
        (
            scenario(None, None, &[]),
            vec![seat("A", 0, None), seat("B", 1, Some("s"))],
            message("GUI:NeedStartSpots"),
        ),
        (
            scenario(None, None, &[]),
            vec![seat("A", 0, Some("r")), seat("B", 0, Some("s"))],
            message("GUI:NeedWOTREnemy"),
        ),
        (
            scenario(None, None, &[]),
            vec![seat("A", -1, Some("r")), seat("B", -1, Some("s"))],
            Verdict::Ready,
        ),
        (scenario(None, None, &[]), two(), Verdict::Ready),
        (scenario(Some(2), Some(2), &[]), two(), Verdict::Ready),
    ];
    for (index, (def, players, want)) in cases.into_iter().enumerate() {
        assert_eq!(verdict(&def, &setup(players)), want, "case {index}");
    }
}

#[test]
fn a_computer_seat_is_named_by_its_difficulty_and_the_human_by_the_profile_name() {
    let table = |key: &str| Some(format!("<{key}>"));
    let name = |kind, skill| seat_name(kind, skill, "Elanor", table);
    assert_eq!(name(PlayerKind::Computer, Skill::Easy), "<GUI:EasyAI>");
    assert_eq!(name(PlayerKind::Computer, Skill::Medium), "<GUI:MediumAI>");
    assert_eq!(name(PlayerKind::Computer, Skill::Hard), "<GUI:HardAI>");
    assert_eq!(name(PlayerKind::Computer, Skill::Brutal), "<GUI:BrutalAI>");
    assert_eq!(name(PlayerKind::Human, Skill::Brutal), "Elanor");
    assert_eq!(
        seat_name(PlayerKind::Human, Skill::Brutal, "", table),
        "",
        "retail has no fall-back name"
    );
    assert_eq!(
        seat_name(PlayerKind::Computer, Skill::Hard, "Elanor", |_| None),
        "GUI:HardAI",
        "an unlocalised label shows as its key"
    );
}

fn choice() -> ScenarioChoice {
    ScenarioChoice {
        campaign: "Camp".to_owned(),
        display_name: None,
        description: None,
    }
}

#[test]
fn closed_slots_are_not_seats_and_the_human_keeps_its_kind() {
    let mut lobby = SkirmishSetup::two_player(
        "FactionMen".to_owned(),
        "FactionMordor".to_owned(),
        "m".to_owned(),
        [false, true],
        None,
    );
    lobby.slots.push(SlotSetup::closed());
    lobby
        .slots
        .push(SlotSetup::new(Occupant::Ai(Difficulty::Hard)));
    let value = setup_of(
        &choice(),
        2,
        BattleType::RealTimeOnly,
        9,
        &lobby,
        &StartPicks::default(),
    );
    let kinds: Vec<PlayerKind> = value.players.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        [
            PlayerKind::Human,
            PlayerKind::Computer,
            PlayerKind::Computer
        ]
    );
    assert_eq!(value.victory_type, 2, "rule 5 reaches the setup value");
    assert_eq!(
        value.battle_type,
        BattleType::RealTimeOnly,
        "rule 7 reaches the setup value"
    );
}

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
";

const VICTORY: &str = " LivingWorldVictoryType
  DisplayGameType = TEST:First
  PlayerDefeatCondition
   Teams = 1 2
   NumControlledRegionsLessOrEqualTo = 0
  End
 End
 LivingWorldVictoryType
  DisplayGameType = TEST:Second
  PlayerDefeatCondition
   Teams = 1 2
   NumControlledRegionsLessOrEqualTo = 0
  End
 End
";

fn regions() -> String {
    let names = ["A", "B", "C", "D", "E", "F"];
    let mut text = String::from("LivingWorldRegionCampaign World\n");
    for (i, name) in names.iter().enumerate() {
        text.push_str(&format!(
            " Region {name}\n  CPLimit = 3\n  AllyCPLimit = 2\n  CenterPoint = X:{} Y:{}\n End\n",
            i * 100,
            50 + i * 10
        ));
    }
    text.push_str("End\n");
    text
}

fn data(scenario_extra: &str) -> LivingWorldData {
    let campaign = format!(
        "LivingWorldCampaign Camp\n SecondsPerReinforcement = 300\n StartingCashRTS = 1000\n \
         StartingCashRTSWithFort = 2000\n{VICTORY} Scenario\n  RegionCampaign = World\n  \
         MinPlayers = 2\n  MaxPlayers = 4\n  DisabledFactions =\n{scenario_extra} End\nEnd\n"
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

fn catalog() -> Catalog {
    Catalog {
        playable_factions: ["elves", "orcs"].map(str::to_owned).into(),
        ..Catalog::default()
    }
}

fn lobby() -> SkirmishSetup {
    let mut lobby = SkirmishSetup::two_player(
        "Elves".to_owned(),
        "Orcs".to_owned(),
        "m".to_owned(),
        [false, true],
        None,
    );
    lobby.slots.push(SlotSetup::closed());
    lobby.slots.push(SlotSetup::closed());
    lobby
}

fn open_a_computer(lobby: &mut SkirmishSetup, slot: usize) {
    if let Some(row) = lobby.slots.get_mut(slot) {
        *row = SlotSetup::new(Occupant::Ai(Difficulty::Easy));
        row.faction = Some("Orcs".to_owned());
    }
}

fn picked(picks: &StartPicks, slots: usize) -> Vec<Option<&str>> {
    (0..slots).map(|slot| picks.get(slot)).collect()
}

fn names(spots: &[RegionSpot]) -> Vec<&str> {
    spots.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn a_click_gives_a_free_region_to_the_first_seat_without_one_and_a_held_one_is_released() {
    let (data, catalog, choice) = (data(""), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    let region = |slot, name: &str| Click::Assigned {
        slot,
        region: name.to_owned(),
    };
    assert_eq!(picks.click(&cx, &mut lobby, "B"), region(0, "B"));
    assert_eq!(picks.click(&cx, &mut lobby, "c"), region(1, "C"));
    assert_eq!(
        picks.click(&cx, &mut lobby, "D"),
        Click::Refused(ClickRefusal::NoFreeSeat)
    );
    assert_eq!(
        picks.click(&cx, &mut lobby, "B"),
        Click::Released {
            slot: 0,
            region: "B".to_owned()
        },
        "a click on a held region lets it go, not a second seat take it"
    );
    assert_eq!(picked(&picks, 3), [None, Some("C"), None]);
    assert_eq!(picks.click(&cx, &mut lobby, "D"), region(0, "D"));
}

#[test]
fn the_engine_refuses_a_region_another_seat_holds() {
    let (data, catalog, choice) = (data(""), catalog(), choice());
    let mut setup = setup_of(
        &choice,
        0,
        BattleType::default(),
        1,
        &lobby(),
        &StartPicks::default(),
    );
    set_start_region(&data, &catalog, &mut setup, 0, "B").unwrap();
    assert_eq!(
        set_start_region(&data, &catalog, &mut setup, 1, "B"),
        Err(SetupError::StartRegionShared("B".to_owned()))
    );
}

#[test]
fn a_region_the_preview_does_not_offer_is_refused_by_the_click() {
    let (data, catalog, choice) = (
        data("  DisableRegions = E\n  DisallowStartInRegions = F\n"),
        catalog(),
        choice(),
    );
    assert_eq!(
        names(&startable_regions(&data, &choice)),
        ["A", "B", "C", "D"]
    );
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    for (region, refusal) in [
        ("E", ClickRefusal::NotStartable),
        ("F", ClickRefusal::Disallowed),
        ("Nowhere", ClickRefusal::NotStartable),
    ] {
        assert_eq!(
            picks.click(&cx, &mut lobby, region),
            Click::Refused(refusal),
            "{region}"
        );
    }
    assert_eq!(picked(&picks, 2), [None, None]);
}

#[test]
fn choosing_a_scenario_resets_every_start_and_deals_the_defaults_to_the_occupied_seats() {
    let (data, catalog, choice) = (
        data("  DefaultStartSpots = Nowhere D B C\n  DisallowStartInRegions = D\n"),
        catalog(),
        choice(),
    );
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    picks.click(&cx, &mut lobby, "A");
    picks.choose_scenario(&cx, &mut lobby);
    assert_eq!(picked(&picks, 3), [Some("B"), Some("C"), None]);
}

#[test]
fn a_slot_opened_later_takes_the_next_default_and_a_closed_slot_gives_its_region_back() {
    let (data, catalog, choice) = (data("  DefaultStartSpots = A C D\n"), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    picks.choose_scenario(&cx, &mut lobby);
    assert_eq!(picked(&picks, 4), [Some("A"), Some("C"), None, None]);

    let before = snapshot(&lobby);
    open_a_computer(&mut lobby, 2);
    picks.reconcile(&cx, &mut lobby, &before);
    assert_eq!(picked(&picks, 4), [Some("A"), Some("C"), Some("D"), None]);

    let before = snapshot(&lobby);
    lobby.slots[1] = SlotSetup::closed();
    picks.reconcile(&cx, &mut lobby, &before);
    assert_eq!(picked(&picks, 4), [Some("A"), None, Some("D"), None]);
}

#[test]
fn a_restricted_region_dictates_the_seat_faction_and_team() {
    let restriction =
        "  StartingRestriction\n   Regions = C\n   Factions = Orcs\n   Teams = 2\n  End\n";
    let (data, catalog, choice) = (data(restriction), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    assert_eq!(
        picks.click(&cx, &mut lobby, "C"),
        Click::Assigned {
            slot: 0,
            region: "C".to_owned()
        }
    );
    assert_eq!(lobby.slots[0].faction.as_deref(), Some("Orcs"));
    assert_eq!(lobby.slots[0].team, Some(1));
}

#[test]
fn a_faction_pick_in_a_historical_scenario_sets_the_start_region_and_team() {
    let historical = "  HistoricalScenario = Yes\n  StartingRestriction\n   Regions = C\n   \
                      Factions = Elves\n   Teams = 2\n  End\n";
    let (data, catalog, choice) = (data(historical), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    picks.faction_picked(&cx, &mut lobby, 0);
    assert_eq!(picks.get(0), Some("C"));
    assert_eq!(lobby.slots[0].team, Some(1));
}

#[test]
fn the_preview_marks_a_held_group_and_the_regions_still_startable() {
    let extra = "  DisallowStartInRegions = F\n  DisableRegions = E\n  OwnershipSet\n   \
                 Regions = A B C\n   StartRegion = C\n  End\n";
    let (data, catalog, choice) = (data(extra), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    lobby.slots[0].colour = Some(1);
    picks.click(&cx, &mut lobby, "B");
    let red = [1.0, 0.0, 0.0];
    let colour = |slot: usize| (slot == 0).then_some(red);
    let id = |i: u16| RegionId(i);
    let marks = preview_marks(&cx, &picks, &lobby, colour, Some("A"));
    assert_eq!(marks.enabled, [0, 1, 2, 3, 5].map(id), "E is disabled");
    assert_eq!(
        marks.filled,
        [(id(0), Some(red)), (id(1), Some(red)), (id(2), Some(red))]
    );
    assert_eq!(marks.homes, [(id(1), Some(red)), (id(2), Some(red))]);
    assert_eq!(marks.startable, [id(3)]);
    assert_eq!(marks.hovered, [0, 1, 2].map(id));
    let over_f = preview_marks(&cx, &picks, &lobby, colour, Some("F"));
    assert!(over_f.hovered.is_empty(), "a disallowed region is not lit");
    assert_eq!(picked_region(&data, &choice, Some(id(4))), None);
    assert_eq!(
        picked_region(&data, &choice, Some(id(5))).as_deref(),
        Some("F")
    );
}

#[test]
fn the_rules_panel_lists_the_campaigns_victory_types_by_index() {
    let (data, catalog, choice) = (data("  DefaultStartSpots = A C\n"), catalog(), choice());
    let options = victory_options(&data, &choice);
    let labels: Vec<_> = options.iter().map(|o| o.label.as_deref()).collect();
    assert_eq!(labels, [Some("TEST:First"), Some("TEST:Second")]);
    let mut lobby = lobby();
    let mut picks = StartPicks::default();
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 1,
    };
    picks.choose_scenario(&cx, &mut lobby);
    let value = setup_of(&choice, 1, BattleType::default(), 3, &lobby, &picks);
    let state = LivingWorldState::start(&data, &catalog, &value).unwrap();
    assert_eq!(state.rules.victory_type, 1);
}

#[test]
fn the_battle_type_rule_has_three_entries_in_the_games_order() {
    assert_eq!(battle_type_at(0), Some(BattleType::Both));
    assert_eq!(battle_type_at(1), Some(BattleType::AutoResolveOnly));
    assert_eq!(battle_type_at(2), Some(BattleType::RealTimeOnly));
    assert_eq!(battle_type_at(3), None);
}

#[test]
fn seats_keep_the_colour_they_chose_and_the_random_ones_take_the_lowest_free() {
    let mut lobby = lobby();
    open_a_computer(&mut lobby, 2);
    lobby.slots[0].colour = Some(1);
    lobby.slots[2].colour = Some(0);
    assert_eq!(seat_colours(&lobby, 6), [1, 2, 0]);
    lobby.slots[0].colour = None;
    lobby.slots[2].colour = None;
    assert_eq!(seat_colours(&lobby, 6), [0, 1, 2]);
    assert_eq!(
        seat_colours(&lobby, 2),
        [0, 1, 0],
        "more seats than colours wrap"
    );
}

#[test]
fn prepare_refuses_a_seat_without_a_region_and_accepts_one_with_regions() {
    let (data, catalog, choice) = (data("  DefaultStartSpots = A B\n"), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let mut lobby = lobby();
    lobby.slots[1].team = Some(1);
    let mut picks = StartPicks::default();
    assert_eq!(
        prepare(&cx, BattleType::AutoResolveOnly, 5, &lobby, &picks).err(),
        Some(Refusal::Message("GUI:NeedStartSpots"))
    );
    picks.choose_scenario(&cx, &mut lobby);
    let setup = prepare(&cx, BattleType::AutoResolveOnly, 5, &lobby, &picks).unwrap();
    assert_eq!(setup.battle_type, BattleType::AutoResolveOnly);
    let starts: Vec<_> = setup
        .players
        .iter()
        .map(|p| p.start_region.as_deref())
        .collect();
    assert_eq!(starts, [Some("A"), Some("B")]);
}

#[test]
fn a_click_acts_on_the_held_member_of_the_regions_ownership_group() {
    let group = "  OwnershipSet
   Regions = B C
   StartRegion = B
  End
";
    let (data, catalog, choice) = (data(group), catalog(), choice());
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    assert_eq!(
        picks.click(&cx, &mut lobby, "C"),
        Click::Assigned {
            slot: 0,
            region: "C".to_owned()
        },
        "no member held: the clicked region"
    );
    assert_eq!(
        picks.click(&cx, &mut lobby, "B"),
        Click::Released {
            slot: 0,
            region: "C".to_owned()
        },
        "C is held, so a click on its group mate B lets C go"
    );
}

#[test]
fn every_click_is_dropped_in_a_historical_scenario() {
    let (data, catalog, choice) = (
        data(
            "  HistoricalScenario = Yes
",
        ),
        catalog(),
        choice(),
    );
    let cx = Scenario {
        data: &data,
        catalog: &catalog,
        choice: &choice,
        victory_type: 0,
    };
    let (mut lobby, mut picks) = (lobby(), StartPicks::default());
    assert_eq!(
        picks.click(&cx, &mut lobby, "B"),
        Click::Refused(ClickRefusal::Historical)
    );
}
