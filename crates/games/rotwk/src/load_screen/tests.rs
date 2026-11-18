use super::*;
use crate::lobby::{Catalogue, Difficulty, Occupant, SlotSetup};

fn labels() -> GridLabels {
    GridLabels {
        human: "Me".to_owned(),
        difficulties: ["E", "M", "H", "B"].map(str::to_owned),
        random: "Random".to_owned(),
        none: "-".to_owned(),
        teams: ["T1", "T2", "T3", "T4"].map(str::to_owned),
        colours: vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        ..GridLabels::default()
    }
}

fn setup() -> SkirmishSetup {
    let mut slots = vec![SlotSetup::closed(); 6];
    slots[0] = SlotSetup::new(Occupant::Human);
    slots[2] = SlotSetup::new(Occupant::Ai(Difficulty::Hard));
    slots[5] = SlotSetup::new(Occupant::Ai(Difficulty::Brutal));
    slots[0].faction = Some("Mine".to_owned());
    slots[0].team = Some(0);
    slots[2].team = Some(0);
    slots[2].faction = None;
    slots[5].faction = None;
    slots[5].colour = None;
    SkirmishSetup {
        map_label: "maps/m.map".to_owned(),
        slots,
        rules: crate::lobby::MatchRules::default(),
        army: None,
        spawn: None,
        spawn_buildings: None,
        dummies: None,
        dummies_attack: false,
        drive_templates: Vec::new(),
        wotr: None,
        cah_hero: None,
        random_seed: 5,
        resolved: None,
    }
}

fn rows_of(setup: &SkirmishSetup) -> Vec<Row> {
    let catalogue = Catalogue {
        factions: vec!["Mine".to_owned()],
        random_pool: vec!["PoolOnly".to_owned()],
        colours: 3,
        starts: 4,
        ..Catalogue::default()
    };
    let starts = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
    let players = setup
        .resolve(setup.random_seed, &catalogue, &starts)
        .unwrap();
    let show = ShowRandom {
        template: true,
        colour: true,
    };
    rows(
        setup,
        &players,
        &labels(),
        show,
        &|name| format!("<{name}>"),
        &|_| true,
    )
}

#[test]
fn rank_badge_is_by_side_then_side_less() {
    let only = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
    let all = only(&["AptRankIconMordor9", "AptRankIcon9", "AptRankIconWild4"]);
    assert_eq!(
        rank_icon("Mordor", 9, &all).as_deref(),
        Some("AptRankIconMordor9")
    );
    assert_eq!(
        rank_icon("Goblins", 4, &all).as_deref(),
        Some("AptRankIconWild4")
    );
    assert_eq!(
        rank_icon("Whatever", 9, &all).as_deref(),
        Some("AptRankIcon9")
    );
    assert_eq!(rank_icon("Arnor", 9, &all).as_deref(), Some("AptRankIcon9"));
    assert_eq!(rank_icon("Arnor", 3, &all), None);
}

#[test]
fn occupied_slots_take_consecutive_rows_in_slot_order() {
    let rows = rows_of(&setup());
    let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Me", "H", "B"]);
    assert_eq!(
        rows.iter().map(|r| r.local).collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn a_setup_with_only_resolved_seats_marks_its_human_row_local() {
    let mut setup = setup();
    let players = {
        let catalogue = Catalogue {
            factions: vec!["Mine".to_owned()],
            random_pool: vec!["PoolOnly".to_owned()],
            colours: 3,
            starts: 4,
            ..Catalogue::default()
        };
        let starts = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        setup.resolve(5, &catalogue, &starts).unwrap()
    };
    setup.slots.clear();
    let show = ShowRandom {
        template: true,
        colour: true,
    };
    let rows = rows(
        &setup,
        &players,
        &labels(),
        show,
        &|n| n.to_owned(),
        &|_| true,
    );
    assert_eq!(
        rows.iter().map(|r| r.local).collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn only_non_allies_show_their_random_picks_hidden() {
    let rows = rows_of(&setup());
    assert_eq!(rows[1].army, "<PoolOnly>");
    assert_ne!(rows[1].colour, [255; 3]);
    assert_eq!(rows[2].army, "Random");
    assert_eq!(rows[2].colour, [255; 3]);
}
