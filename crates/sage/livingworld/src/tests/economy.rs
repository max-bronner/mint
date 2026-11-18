use super::*;
use crate::economy::{effective_resource_bonus, resource_gain};
use crate::model::{bonus, BuildingId, Phase, PlotBuilding, RegionId, Rejected};
use crate::rules::SetupError;
use mint_sage_ini::living_world::players::BuildingType;

#[test]
fn resource_bonus_rounds_in_f32_then_up() {
    assert_eq!(resource_gain(100, -10), 90);
    assert_eq!(resource_gain(100, -99), 2);
    assert_eq!(resource_gain(100, 10), 110);
    assert_eq!(resource_gain(1000, -50), 500);
    assert_eq!(effective_resource_bonus(500, 2.0), 100);
    assert_eq!(effective_resource_bonus(50, 2.0), 50);
}

fn farm(state: &LivingWorldState) -> BuildingId {
    state.rules.building_id("Farm").unwrap()
}

fn restricted(state: &LivingWorldState) -> bool {
    state.build_restricted(RegionId(0), farm(state))
}

#[test]
fn a_restriction_without_number_allowed_caps_its_types_at_one() {
    let mut s = session();
    s.rules.regions[0].restrict = vec![(vec![BuildingType::Resource], None)];
    assert!(!restricted(&s));
    place(&mut s, 0, 0, false);
    assert!(restricted(&s));
}

#[test]
fn only_the_first_restriction_naming_the_type_sets_the_cap() {
    let mut s = session();
    s.rules.regions[0].restrict = vec![
        (vec![BuildingType::Resource], Some(1)),
        (vec![BuildingType::Resource], Some(5)),
    ];
    place(&mut s, 0, 0, false);
    assert!(restricted(&s));
}

#[test]
fn a_type_no_restriction_names_is_capped_at_the_building_spot_count() {
    let mut s = session();
    s.rules.regions[0].restrict = Vec::new();
    s.rules.regions[0].plots = 1;
    assert!(!restricted(&s));
    place(&mut s, 0, 0, false);
    assert!(restricted(&s));
}

fn place(state: &mut LivingWorldState, plot: usize, turns_left: i32, under_construction: bool) {
    let building = farm(state);
    state.regions[0].plots[plot] =
        Some(PlotBuilding::new(building, turns_left, under_construction));
}

#[test]
fn income_is_fertile_then_treasury_clipped_at_the_maximum() {
    let mut s = session();
    place(&mut s, 0, 0, false);
    s.rules.templates[0].max_resources = 120;
    end_planning(&mut s);
    assert_eq!(s.player(P0).unwrap().resources, 120);
}

#[test]
fn a_building_finished_this_turn_pays_from_the_next() {
    let mut s = session();
    place(&mut s, 0, 1, true);
    end_planning(&mut s);
    assert_eq!(s.player(P0).unwrap().resources, 107);
    end_planning(&mut s);
    assert_eq!(s.player(P0).unwrap().resources, 107 + 7 + 30);
}

#[test]
fn only_the_first_building_under_construction_advances() {
    let mut s = session();
    place(&mut s, 0, 3, true);
    place(&mut s, 1, 3, true);
    end_planning(&mut s);
    assert_eq!(s.regions[0].plots[0].clone().unwrap().turns_left, 2);
    assert_eq!(s.regions[0].plots[1].clone().unwrap().turns_left, 3);
}

#[test]
fn building_charges_the_cost_and_needs_a_free_plot() {
    let mut s = session();
    let building = farm(&s);
    let region = rid(&s, "A");
    command(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 0,
            building,
        },
    );
    assert_eq!(s.player(P0).unwrap().resources, 50);
    let again = apply(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 0,
            building,
        },
    );
    assert_eq!(again, Err(Rejected::PlotTaken));
    let broke = apply(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 1,
            building,
        },
    );
    assert_eq!(
        broke,
        Ok(vec![crate::model::Event::BuildingStarted {
            region,
            plot: 1
        }])
    );
    let none = apply(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 1,
            building,
        },
    );
    assert_eq!(none, Err(Rejected::PlotTaken));
}

#[test]
fn a_capped_structure_on_an_empty_plot_is_refused_and_costs_nothing() {
    let mut s = session();
    let building = farm(&s);
    let region = rid(&s, "A");
    s.rules.regions[region.index()].restrict = vec![(vec![BuildingType::Resource], Some(1))];
    place_in(&mut s, region, 0);
    let money = s.player(P0).unwrap().resources;
    assert!(s.buildable_here(P0).contains(&building), "still offered");
    let capped = apply(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 1,
            building,
        },
    );
    assert_eq!(capped, Err(Rejected::RestrictedBuilding));
    assert_eq!(s.player(P0).unwrap().resources, money);
    assert!(s.region(region).unwrap().plots[1].is_none());
}

fn place_in(state: &mut LivingWorldState, region: RegionId, plot: usize) {
    let building = farm(state);
    state.regions[region.index()].plots[plot] = Some(PlotBuilding::new(building, 0, false));
}

#[test]
fn a_territory_pays_when_all_its_regions_are_held() {
    let mut s = session();
    let mut events = Vec::new();
    s.rebuild_bonuses(&mut events);
    assert_eq!(s.player(P0).unwrap().bonus_a.get(bonus::ARMY), 6);
    assert_eq!(s.world_cp_limit(P0), 9);
    let b = rid(&s, "B");
    s.regions[b.index()].owner = Some(P1);
    s.rebuild_bonuses(&mut events);
    assert_eq!(s.player(P0).unwrap().bonus_a.get(bonus::ARMY), 1);
    assert_eq!(s.world_cp_limit(P0), 5);
}

#[test]
fn a_territory_is_announced_once_when_it_enters_the_held_set_and_not_at_the_opening() {
    let mut s = session();
    let b = rid(&s, "B");
    let gained = |events: &[Event]| {
        events
            .iter()
            .filter(|e| matches!(e, Event::TerritoryGained { .. }))
            .count()
    };
    let mut events = Vec::new();
    s.regions[b.index()].owner = Some(P1);
    s.rebuild_bonuses(&mut events);
    s.regions[b.index()].owner = Some(P0);
    s.rebuild_bonuses(&mut events);
    assert_eq!(gained(&events), 0, "turn 1 Planning is the opening");

    s.phase = Phase::ResolveBattles;
    s.regions[b.index()].owner = Some(P1);
    s.rebuild_bonuses(&mut events);
    s.regions[b.index()].owner = Some(P0);
    s.rebuild_bonuses(&mut events);
    assert_eq!(
        events
            .iter()
            .filter(
                |e| matches!(e, Event::TerritoryGained { player, territory: 0 } if *player == P0)
            )
            .count(),
        1
    );
    s.rebuild_bonuses(&mut events);
    assert_eq!(
        gained(&events),
        1,
        "a refresh that changes nothing announces nothing"
    );
}

#[test]
fn an_unplayable_setup_is_refused() {
    let (data, mut setup) = setup(DEFAULT_VICTORY_FOR_TESTS, 1);
    setup
        .players
        .push(seat("Orcs", PlayerKind::Computer, 2, "C"));
    let too_many = LivingWorldState::start(&data, &catalog(), &setup).err();
    assert_eq!(
        too_many,
        Some(SetupError::TooManyPlayers { max: 2, got: 3 })
    );
    setup.players.pop();
    setup.players[1].template = "Dwarves".to_owned();
    let unknown = LivingWorldState::start(&data, &catalog(), &setup).err();
    assert_eq!(
        unknown,
        Some(SetupError::UnknownTemplate("Dwarves".to_owned()))
    );
}

#[test]
fn a_session_needs_chosen_factions_start_regions_two_teams_and_sane_limits() {
    let start = |data: &LivingWorldData, setup: &Setup| {
        LivingWorldState::start(data, &catalog(), setup).err()
    };
    let (data, ok) = setup(DEFAULT_VICTORY_FOR_TESTS, 1);
    assert_eq!(start(&data, &ok), None);

    let mut random = ok.clone();
    random.players[1].template.clear();
    assert_eq!(start(&data, &random), Some(SetupError::RandomFaction(1)));

    let mut no_start = ok.clone();
    no_start.players[0].start_region = None;
    assert_eq!(start(&data, &no_start), Some(SetupError::NoStartRegion(0)));

    let mut one_team = ok.clone();
    one_team.players[1].team = 0;
    assert_eq!(start(&data, &one_team), Some(SetupError::TooFewTeams));

    let teamless = {
        let mut s = ok.clone();
        s.players[0].team = -1;
        s.players[1].team = -1;
        s
    };
    assert_eq!(start(&data, &teamless), None);

    let zero_max = data_with(DEFAULT_VICTORY_FOR_TESTS, "  MaxPlayers = 0\n");
    assert_eq!(start(&zero_max, &ok), Some(SetupError::InvalidPlayerLimits));
    let min_over_max = data_with(DEFAULT_VICTORY_FOR_TESTS, "  MinPlayers = 3\n");
    assert_eq!(
        start(&min_over_max, &ok),
        Some(SetupError::InvalidPlayerLimits)
    );
}

#[test]
fn the_player_maximum_is_capped_at_eight() {
    let (_, mut ok) = setup(DEFAULT_VICTORY_FOR_TESTS, 1);
    let data = data_with(DEFAULT_VICTORY_FOR_TESTS, "  MaxPlayers = 12\n");
    let seat = ok.players[1].clone();
    ok.players.resize(9, seat);
    assert_eq!(
        LivingWorldState::start(&data, &catalog(), &ok).err(),
        Some(SetupError::TooManyPlayers { max: 8, got: 9 })
    );
}

const DEFAULT_VICTORY_FOR_TESTS: &str = "";

#[test]
fn a_scenario_without_a_resource_maximum_caps_at_25000_not_zero() {
    let (data, mut setup) = setup(DEFAULT_VICTORY_FOR_TESTS, 1);
    setup.players[0].template = "Ents".to_owned();
    let mut s = LivingWorldState::start(&data, &catalog(), &setup).unwrap();
    let building = farm(&s);
    let region = rid(&s, "A");
    command(
        &mut s,
        P0,
        Command::Build {
            region,
            plot: 0,
            building,
        },
    );
    assert_eq!(s.player(P0).unwrap().resources, 50);
}
