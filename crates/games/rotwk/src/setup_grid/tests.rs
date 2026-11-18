use super::*;
use crate::lobby::MatchRules;
use crate::setup_heroes::{HeroEntry, SubclassFactions};

fn catalogue(starts: u8) -> Catalogue {
    Catalogue {
        factions: vec!["FactionA".to_owned(), "FactionB".to_owned()],
        random_pool: Vec::new(),
        colours: 3,
        starts,
        heroes: vec![
            hero("SysA", true, 0),
            hero("SysB", true, 4),
            hero("Mine", false, 0),
        ],
        sides: vec![("FactionA".to_owned(), 0), ("FactionB".to_owned(), 4)],
    }
}

fn hero(name: &str, system: bool, side: u8) -> HeroEntry {
    HeroEntry {
        name: name.to_owned(),
        system,
        factions: SubclassFactions {
            usable: 1 << side,
            default_faction: Some(side),
        },
        record: std::sync::Arc::new(mint_sage_cah::HeroRecord::default()),
    }
}

fn labels() -> GridLabels {
    GridLabels {
        human: "Me".to_owned(),
        closed: "Closed".to_owned(),
        difficulties: ["E", "M", "H", "B"].map(str::to_owned),
        random: "Random".to_owned(),
        none: "-".to_owned(),
        teams: ["1", "2", "3", "4"].map(str::to_owned),
        factions: vec!["A".to_owned(), "B".to_owned()],
        colours: vec![[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        random_colour: "Unknown".to_owned(),
        hero_default_suffix: " *".to_owned(),
        numbers: vec!["one".to_owned(), "two".to_owned()],
    }
}

fn setup() -> SkirmishSetup {
    let mut slots = vec![
        SlotSetup::new(Occupant::Human),
        SlotSetup::new(Occupant::Ai(Difficulty::Hard)),
    ];
    slots.resize(MAX_SLOTS, SlotSetup::closed());
    SkirmishSetup {
        map_label: "maps/m.map".to_owned(),
        slots,
        rules: MatchRules::default(),
        army: None,
        spawn: None,
        spawn_buildings: None,
        dummies: None,
        dummies_attack: false,
        cah_hero: None,
        random_seed: crate::skirmish::SKIRMISH_SEED,
        resolved: None,
    }
}

fn texts(spec: &ComboSpec) -> Vec<String> {
    spec.items.iter().map(|i| i.text.clone()).collect()
}

#[test]
fn a_cell_is_its_row_clips_path() {
    assert_eq!(
        cell_path("OpenPlay.PlayerList", 3, Column::Army),
        "OpenPlay.PlayerList.~3.PlayerTemplate"
    );
    assert_eq!(cell_path("L", 0, Column::Colour), "L.~0.Color");
    assert_eq!(connection_icon(7), "ConnectionIcon~7");
}

#[test]
fn the_player_column_is_the_local_name_or_closed_and_the_difficulties() {
    let (s, l) = (setup(), labels());
    let (human, _) = cell(&s, &catalogue(4), &l, 0, Column::Player);
    assert_eq!(texts(&human), vec!["Me"]);
    assert!(!human.enabled);
    let (ai, choices) = cell(&s, &catalogue(4), &l, 1, Column::Player);
    assert_eq!(texts(&ai), vec!["Closed", "E", "M", "H", "B"]);
    assert_eq!(ai.selected, Some(3));
    assert!(ai.enabled);
    assert_eq!(
        choices[4],
        Choice::Occupant(Occupant::Ai(Difficulty::Brutal))
    );
    let (open, _) = cell(&s, &catalogue(4), &l, 2, Column::Player);
    assert_eq!(open.items.len(), 5);
    assert_eq!(open.selected, Some(0));
    let (full, _) = cell(&s, &catalogue(2), &l, 2, Column::Player);
    assert_eq!(texts(&full), vec!["Closed"], "a full map offers no seat");
}

#[test]
fn a_closed_rows_cells_are_disabled_placeholders() {
    let (s, l, c) = (setup(), labels(), catalogue(4));
    for (column, text) in [
        (Column::Army, "-"),
        (Column::Hero, "-"),
        (Column::Team, "-"),
        (Column::Handicap, "0%"),
    ] {
        let (spec, choices) = cell(&s, &c, &l, 5, column);
        assert_eq!(texts(&spec), vec![text], "{column:?}");
        assert!(!spec.enabled);
        assert_eq!(choices, vec![Choice::Nothing]);
    }
    let (colour, _) = cell(&s, &c, &l, 5, Column::Colour);
    assert_eq!(colour.items[0].image.as_deref(), Some("Unknown"));
    assert!(!colour.enabled);
}

#[test]
fn an_occupied_rows_columns_offer_their_choices() {
    let (mut s, l, c) = (setup(), labels(), catalogue(4));
    s.slots[0].colour = Some(1);
    s.slots[1].faction = Some("FactionB".to_owned());
    s.slots[1].team = Some(2);
    s.slots[1].handicap = 95;
    let (army, _) = cell(&s, &c, &l, 1, Column::Army);
    assert_eq!(texts(&army), vec!["Random", "A", "B"]);
    assert_eq!(army.selected, Some(2));
    let (team, _) = cell(&s, &c, &l, 1, Column::Team);
    assert_eq!(texts(&team), vec!["-", "1", "2", "3", "4"]);
    assert_eq!(team.selected, Some(3));
    let (colour, choices) = cell(&s, &c, &l, 1, Column::Colour);
    assert_eq!(
        choices,
        vec![
            Choice::Colour(None),
            Choice::Colour(Some(0)),
            Choice::Colour(Some(2))
        ],
        "slot 0's colour is not offered"
    );
    assert_eq!(colour.items[1].swatch, Some([1, 2, 3, 255]));
    assert_eq!(colour.selected, Some(0));
    let (own, own_choices) = cell(&s, &c, &l, 0, Column::Colour);
    assert!(own_choices.contains(&Choice::Colour(Some(1))), "its own is");
    assert_eq!(own.selected, Some(2));
    let (handicap, _) = cell(&s, &c, &l, 1, Column::Handicap);
    assert_eq!(handicap.items.len(), 20, "0% to -95%, retail's ceiling");
    assert_eq!(handicap.items[0].text, "0%");
    assert_eq!(handicap.items[19].text, "-95%");
    assert_eq!(handicap.selected, Some(19));
    let (hero, _) = cell(&s, &c, &l, 1, Column::Hero);
    assert_eq!(texts(&hero), vec!["Random", "Mine", "SysB *", "SysA *"]);
    assert!(hero.enabled);
    s.slots[1].faction = Some("FactionA".to_owned());
    let (hero, choices) = cell(&s, &c, &l, 1, Column::Hero);
    let greyed: Vec<_> = hero
        .items
        .iter()
        .map(|i| i.colour == Some(UNUSABLE_HERO))
        .collect();
    assert_eq!(greyed, [false, false, true, false]);
    assert_eq!(choices[0], Choice::Hero(HeroPick::Random));
    assert_eq!(choices[2], Choice::Hero(HeroPick::Stored(1)));
    s.rules.allow_custom_heroes = false;
    let (hero, choices) = cell(&s, &c, &l, 1, Column::Hero);
    assert_eq!(
        (hero.items.len(), hero.enabled, choices[0].clone()),
        (1, false, Choice::Hero(HeroPick::None)),
        "the rule off leaves one disabled '-'"
    );
}

#[test]
fn a_chosen_entry_edits_through_the_lobbys_rules() {
    let (mut s, c) = (setup(), catalogue(4));
    assert!(apply(&mut s, &c, 0, &Choice::Colour(Some(2))));
    assert!(!apply(&mut s, &c, 1, &Choice::Colour(Some(2))));
    assert!(apply(&mut s, &c, 1, &Choice::Team(Some(0))));
    assert!(apply(
        &mut s,
        &c,
        2,
        &Choice::Occupant(Occupant::Ai(Difficulty::Easy))
    ));
    assert_eq!(s.players(), 3);
    assert!(apply(&mut s, &c, 1, &Choice::Occupant(Occupant::Closed)));
    assert_eq!(s.slots[1], SlotSetup::closed());
    assert!(!apply(&mut s, &c, 5, &Choice::Nothing));
}

#[test]
fn the_preview_numbers_each_held_start_with_its_slot() {
    let map = crate::mapcache::SkirmishMap {
        map_path: "maps/m/m.map".to_owned(),
        display_name: "M".to_owned(),
        players: 3,
        description: String::new(),
        extent: Some([0.0, 0.0, 4000.4, 4250.6]),
        starts: vec![[10.4, 20.6], [30.0, 40.0], [50.0, 60.0], [70.0, 80.0]],
        official: true,
    };
    let mut lobby = setup();
    lobby.slots[1].start = Some(0);
    lobby.slots[2] = SlotSetup {
        start: Some(2),
        ..SlotSetup::new(Occupant::Ai(Difficulty::Easy))
    };
    lobby.slots[3].start = Some(1);
    let spec = preview(&map, &lobby, &labels(), "Back");
    assert_eq!(
        spec.spots,
        vec![
            ([10, 21], Some("two".to_owned())),
            ([30, 40], None),
            ([50, 60], Some("3".to_owned())),
        ],
        "three offered; slot 1 is 'two', slot 2 past the labels is 3"
    );
    assert_eq!(spec.art.as_deref(), Some("maps/m/m_art.tga"));
    assert_eq!(spec.extent, [0, 0, 4000, 4251]);
    assert_eq!(spec.backdrop.as_deref(), Some("Back"));
    let none = crate::mapcache::SkirmishMap {
        extent: None,
        ..map
    };
    assert_eq!(
        preview(&none, &lobby, &labels(), "Back").extent,
        [0, 0, 1, 1]
    );
}
