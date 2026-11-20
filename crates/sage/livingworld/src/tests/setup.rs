use super::*;
use crate::rules::SetupError;
use crate::setup::act_spawns::ActRecords;
use crate::setup::{assign_start_regions, set_seat_faction, set_start_region, Setup as SetupValue};
use crate::PalantirMarker;

const NO_DISABLED: &str = "  DisabledFactions =\n";

fn scenario_data(acts: &str, scenario: &str) -> LivingWorldData {
    data_with(acts, &format!("{NO_DISABLED}{scenario}"))
}

fn start(data: &LivingWorldData, setup: &SetupValue) -> LivingWorldState {
    LivingWorldState::start(data, &catalog(), setup).unwrap()
}

fn two_seats() -> SetupValue {
    setup("", 1).1
}

fn units_of(state: &LivingWorldState, player: PlayerId) -> u32 {
    state
        .armies_of(player)
        .flat_map(|a| a.roster.iter().map(|m| m.count))
        .sum()
}

fn record(extra: &str) -> String {
    format!("  SpawnArmy\n   PlayerArmy = Levy\n{extra}  End\n")
}

fn act(name: &str, body: &str) -> String {
    format!(" Act {name}\n{body} End\n")
}

fn playable(names: &[&str]) -> Catalog {
    let mut c = catalog();
    c.playable_factions = names.iter().map(|n| (*n).to_owned()).collect();
    c
}

#[test]
fn a_random_seat_is_refused_only_when_the_scenario_disables_factions() {
    let mut setup = two_seats();
    setup.players[1].template.clear();
    let cat = playable(&["elves", "orcs"]);
    let disabled = data("");
    assert_eq!(
        LivingWorldState::start(&disabled, &cat, &setup).err(),
        Some(SetupError::RandomFaction(1))
    );
    let open = scenario_data("", "");
    assert!(LivingWorldState::start(&open, &cat, &setup).is_ok());
}

#[test]
fn random_seats_draw_playable_templates_from_the_session_rng() {
    let data = scenario_data("", "");
    let cat = playable(&["elves", "orcs"]);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..40 {
        let mut setup = two_seats();
        setup.seed = seed;
        setup.players[0].template.clear();
        setup.players[1].template.clear();
        let s = LivingWorldState::start(&data, &cat, &setup).unwrap();
        let again = LivingWorldState::start(&data, &cat, &setup).unwrap();
        assert_eq!(s.state_hash(), again.state_hash());
        for p in &s.players {
            let name = &s.rules.templates[usize::from(p.template.0)].name;
            assert_ne!(name, "Ents", "seed {seed}");
            seen.insert((p.id.0, name.clone()));
        }
    }
    assert_eq!(seen.len(), 4, "{seen:?}");
}

#[test]
fn a_drawn_faction_takes_its_historical_start_region_and_team() {
    let scenario = "  HistoricalScenario = Yes\n  StartingRestriction\n   Regions = C\n   \
                    Factions = Elves\n   Teams = 2\n  End\n";
    let data = scenario_data("", scenario);
    let mut setup = two_seats();
    setup.players[0].template.clear();
    setup.players[0].start_region = None;
    setup.players[0].team = 0;
    setup.players[1].team = 0;
    let s = LivingWorldState::start(&data, &playable(&["elves"]), &setup).unwrap();
    assert_eq!(s.players[0].base_region, Some(rid(&s, "C")));
    assert_eq!(s.players[0].team, 1);
}

#[test]
fn default_start_spots_go_to_seats_in_order_skipping_non_regions_and_disallowed() {
    let data = scenario_data(
        "",
        "  DefaultStartSpots = Nowhere D B C\n  DisallowStartInRegions = D\n",
    );
    let mut setup = two_seats();
    assign_start_regions(&data, &catalog(), &mut setup).unwrap();
    let starts: Vec<_> = setup
        .players
        .iter()
        .map(|p| p.start_region.clone())
        .collect();
    assert_eq!(starts, [Some("B".to_owned()), Some("C".to_owned())]);

    let short = scenario_data("", "  DefaultStartSpots = B\n");
    assign_start_regions(&short, &catalog(), &mut setup).unwrap();
    assert_eq!(setup.players[0].start_region.as_deref(), Some("B"));
    assert_eq!(setup.players[1].start_region, None);
    assert_eq!(
        LivingWorldState::start(&short, &catalog(), &setup).err(),
        Some(SetupError::NoStartRegion(1))
    );
}

#[test]
fn a_region_another_seat_holds_cannot_be_clicked() {
    let data = scenario_data("", "");
    let mut setup = two_seats();
    assert_eq!(
        set_start_region(&data, &catalog(), &mut setup, 1, "A"),
        Err(SetupError::StartRegionShared("A".to_owned()))
    );
    set_start_region(&data, &catalog(), &mut setup, 1, "D").unwrap();
    assert_eq!(setup.players[1].start_region.as_deref(), Some("D"));
}

#[test]
fn a_restricted_region_sets_faction_and_team_outside_historical_scenarios_only() {
    let restriction =
        "  StartingRestriction\n   Regions = C\n   Factions = Orcs\n   Teams = 2\n  End\n";
    let cat = playable(&["elves", "orcs"]);
    let open = scenario_data("", restriction);
    let mut setup = two_seats();
    set_start_region(&open, &cat, &mut setup, 0, "C").unwrap();
    assert_eq!(setup.players[0].template, "Orcs");
    assert_eq!(setup.players[0].team, 1);

    let historical = scenario_data("", &format!("  HistoricalScenario = Yes\n{restriction}"));
    let mut setup = two_seats();
    set_start_region(&historical, &cat, &mut setup, 0, "C").unwrap();
    assert_eq!(setup.players[0].template, "Elves");
    set_seat_faction(&historical, &cat, &mut setup, 0, "Orcs").unwrap();
    assert_eq!(setup.players[0].start_region.as_deref(), Some("C"));
    assert_eq!(setup.players[0].team, 1);
}

const OWNERSHIP: &str = "  OwnershipSet\n   Regions = A B\n   StartRegion = A\n  End\n";

#[test]
fn a_default_act_army_spawns_with_or_without_ownership_sets_unless_it_opts_out() {
    let acts = act(
        "One",
        &format!(
            "{}{}",
            record("   SpawnForTemplates = Elves\n"),
            record("   SpawnForTemplates = Orcs\n   SpawnAtActStart = No\n")
        ),
    );
    for scenario in ["", OWNERSHIP] {
        let s = start(&scenario_data(&acts, scenario), &two_seats());
        assert_eq!(units_of(&s, P0), 2, "scenario {scenario:?}");
        assert_eq!(units_of(&s, P1), 0, "scenario {scenario:?}");
    }
}

#[test]
fn an_act_army_without_templates_spawns_for_nobody() {
    let acts = act("One", &record(""));
    let s = start(&scenario_data(&acts, ""), &two_seats());
    assert_eq!(s.armies.len(), 0);
}

#[test]
fn only_the_first_act_and_the_acts_it_calls_spawn() {
    let acts = format!(
        "{}{}{}",
        act("One", "  CallActSubroutine = Two\n"),
        act("Two", &record("   SpawnForTemplates = Elves\n")),
        act("Three", &record("   SpawnForTemplates = Orcs\n"))
    );
    let s = start(&scenario_data(&acts, ""), &two_seats());
    assert_eq!(units_of(&s, P0), 2);
    assert_eq!(units_of(&s, P1), 0);
}

#[test]
fn a_force_battle_with_a_movie_adds_a_marker_and_remove_event_takes_it_away() {
    let force = |event: &str, movie: Option<&str>, remove: &str| {
        let movie = movie.map_or(String::new(), |m| format!("   Movie = {m}\n"));
        format!(
            "  ForceBattle\n   Region = A\n   EventName = {event}\n{movie}   Position = X:3 Y:4\n   \
             RemoveEvent = {remove}\n  End\n"
        )
    };
    let body = format!(
        "{}{}{}{}",
        force("Keep", Some("Intro"), "No"),
        force("Gone", Some("Other"), "No"),
        force("Never", None, "No"),
        force("Gone", Some("Other"), "Yes")
    );
    let acts = act("One", &body);
    let s = start(&scenario_data(&acts, ""), &two_seats());
    assert_eq!(
        s.palantir_markers,
        vec![PalantirMarker {
            event: "Keep".to_owned(),
            movie: "Intro".to_owned(),
            position: [3.0, 4.0],
            play_next_act_after_movie: false,
        }]
    );
}

const GUARD_SET: &str =
    "  OwnershipSet\n   Regions = A B\n   StartRegion = A\n   SpawnArmies\n    \
                         Armies = Guard\n    Region = A\n   End\n  End\n";

#[test]
fn a_named_spawn_has_no_fallback_to_a_record_for_another_template() {
    let acts = act(
        "One",
        &record("   ScriptingName = Guard\n   SpawnForTemplates = Orcs\n   SpawnAtActStart = No\n"),
    );
    let s = start(&scenario_data(&acts, GUARD_SET), &two_seats());
    assert_eq!(units_of(&s, P0), 0);
}

#[test]
fn a_record_the_ownership_pass_spawned_is_not_spawned_again_by_the_act() {
    let acts = act(
        "One",
        &record("   ScriptingName = Guard\n   SpawnForTemplates = Elves\n"),
    );
    let s = start(&scenario_data(&acts, GUARD_SET), &two_seats());
    assert_eq!(units_of(&s, P0), 2);
}

#[test]
fn a_city_record_makes_no_army() {
    let acts = act(
        "One",
        &record("   SpawnForTemplates = Elves\n   IsCity = Yes\n"),
    );
    let s = start(&scenario_data(&acts, ""), &two_seats());
    assert_eq!(s.armies.len(), 0);
}

#[test]
fn an_initial_region_places_the_army_and_a_position_alone_places_none() {
    let positioned = "   SpawnForTemplates = Elves\n   Position = X:5 Y:5\n";
    let none = start(
        &scenario_data(&act("One", &record(positioned)), ""),
        &two_seats(),
    );
    assert_eq!(none.armies.len(), 0);
    let acts = act(
        "One",
        &record(&format!("{positioned}   InitialRegion = D\n")),
    );
    let s = start(&scenario_data(&acts, ""), &two_seats());
    let d = rid(&s, "D");
    assert_eq!(s.armies_of(P0).map(|a| a.region).collect::<Vec<_>>(), [d]);
}

#[test]
fn a_missing_spawn_point_ends_the_pass_and_a_player_without_a_base_is_skipped() {
    let acts = act("One", &record("   SpawnForTemplates = Elves Orcs\n"));
    let data = scenario_data(&acts, "");
    let records = ActRecords::new(data.campaigns.get("Camp").unwrap());

    let mut s = start(&data, &two_seats());
    s.armies.clear();
    let a = rid(&s, "A");
    s.spawn_act_armies(&data, &records, &[], &|r| r != a);
    assert_eq!(s.armies.len(), 0);

    s.players[0].base_region = None;
    s.spawn_act_armies(&data, &records, &[], &|_| true);
    assert_eq!((units_of(&s, P0), units_of(&s, P1)), (0, 2));
}
