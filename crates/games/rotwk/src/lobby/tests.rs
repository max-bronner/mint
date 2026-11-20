use super::*;

fn catalogue(starts: u8) -> Catalogue {
    Catalogue {
        factions: vec![
            "FactionA".to_owned(),
            "FactionB".to_owned(),
            "FactionC".to_owned(),
        ],
        random_pool: vec!["PoolA".to_owned(), "PoolB".to_owned()],
        colours: 10,
        starts,
        ..Catalogue::default()
    }
}

fn ai() -> SlotSetup {
    SlotSetup::new(Occupant::Ai(Difficulty::Easy))
}

fn setup(slots: Vec<SlotSetup>) -> SkirmishSetup {
    SkirmishSetup {
        map_label: "maps/m.map".to_owned(),
        slots,
        rules: MatchRules::default(),
        army: None,
        spawn: None,
        spawn_buildings: None,
        dummies: None,
        dummies_attack: false,
        drive_templates: Vec::new(),
        wotr: None,
        cah_hero: None,
        random_seed: crate::skirmish::SKIRMISH_SEED,
        resolved: None,
    }
}

fn two() -> SkirmishSetup {
    setup(vec![SlotSetup::new(Occupant::Human), ai()])
}

#[test]
fn a_setup_breaking_a_rule_is_refused_with_the_rule() {
    let c = catalogue(4);
    assert_eq!(two().validate(&c), Ok(()));
    assert_eq!(
        setup(vec![SlotSetup::new(Occupant::Human), SlotSetup::closed()]).validate(&c),
        Ok(())
    );
    assert_eq!(setup(vec![ai(), ai()]).validate(&c), Ok(()));
    assert_eq!(
        setup(vec![ai(), ai()]).validate_play(&c),
        Err(SetupError::NeedHumanPlayers)
    );
    assert_eq!(
        setup(vec![ai(), ai(), ai(), ai(), ai()]).validate(&c),
        Err(SetupError::TooManyPlayers {
            players: 5,
            starts: 4
        })
    );
    assert_eq!(
        setup(vec![SlotSetup::closed(); 9]).validate(&c),
        Err(SetupError::TooManySlots(9))
    );
    let mut s = two();
    s.slots[1].faction = Some("FactionZ".to_owned());
    assert_eq!(
        s.validate(&c),
        Err(SetupError::UnknownFaction("FactionZ".to_owned()))
    );
    let mut s = two();
    s.slots[0].colour = Some(3);
    s.slots[1].colour = Some(3);
    assert_eq!(s.validate(&c), Err(SetupError::ColourTaken(3)));
    s.slots[1].colour = Some(10);
    assert_eq!(s.validate(&c), Err(SetupError::ColourOutOfRange(10)));
    let mut s = two();
    s.slots[0].start = Some(1);
    s.slots[1].start = Some(1);
    assert_eq!(s.validate(&c), Err(SetupError::StartTaken(1)));
    s.slots[1].start = Some(4);
    assert_eq!(s.validate(&c), Err(SetupError::StartOutOfRange(4)));
    let mut s = two();
    s.slots[1].team = Some(4);
    assert_eq!(s.validate(&c), Err(SetupError::TeamOutOfRange(4)));
    let mut s = two();
    s.slots[1].handicap = 7;
    assert_eq!(s.validate(&c), Err(SetupError::Handicap(7)));
    s.slots[1].handicap = 105;
    assert_eq!(s.validate(&c), Err(SetupError::Handicap(105)));
    s.slots[1].handicap = 100;
    assert_eq!(
        s.validate(&c),
        Err(SetupError::Handicap(100)),
        "retail's handicap list stops at -95%"
    );
    s.slots[1].handicap = 95;
    assert_eq!(s.validate(&c), Ok(()));
    let mut s = two();
    s.rules.starting_money = -1;
    assert_eq!(s.validate(&c), Err(SetupError::StartingMoney(-1)));
}

#[test]
fn a_closed_slot_holds_no_colour_or_start() {
    let c = catalogue(4);
    let mut s = setup(vec![
        SlotSetup::new(Occupant::Human),
        ai(),
        SlotSetup::closed(),
    ]);
    s.slots[2].colour = Some(1);
    s.slots[2].start = Some(0);
    s.slots[0].colour = Some(1);
    s.slots[0].start = Some(0);
    assert_eq!(s.validate(&c), Ok(()));
}

#[test]
fn an_edit_that_breaks_a_rule_is_refused() {
    let c = catalogue(2);
    let mut s = two();
    assert!(s.try_edit(0, &c, |slot| slot.colour = Some(2)));
    assert!(!s.try_edit(1, &c, |slot| slot.colour = Some(2)), "taken");
    assert_eq!(s.slots[1].colour, None);
    assert!(s.try_edit(1, &c, |slot| slot.start = Some(1)));
    assert!(!s.try_edit(0, &c, |slot| slot.start = Some(1)), "taken");
    assert!(
        !s.try_edit(0, &c, |slot| slot.start = Some(2)),
        "not on the map"
    );
    s.slots.push(SlotSetup::closed());
    assert!(
        !s.try_edit(2, &c, |slot| slot.occupant = Occupant::Ai(Difficulty::Hard)),
        "the map holds two"
    );
    assert!(
        !s.try_edit(0, &c, |slot| slot.occupant = Occupant::Closed),
        "the local player"
    );
    assert!(
        !s.try_edit(1, &c, |slot| slot.occupant = Occupant::Human),
        "one local seat"
    );
    assert!(s.try_edit(1, &c, |slot| slot.occupant =
        Occupant::Ai(Difficulty::Brutal)));
    assert!(s.try_edit(1, &c, |slot| {
        slot.occupant = Occupant::Closed;
    }));
    assert_eq!(s.slots[1], SlotSetup::closed());
    assert!(
        !s.try_edit(1, &c, |slot| slot.team = Some(1)),
        "a closed slot has no team"
    );
    assert_eq!(s.slots[1], SlotSetup::closed());
    assert!(
        !s.try_edit(9, &c, |slot| slot.team = Some(1)),
        "no such slot"
    );
    assert!(!s.try_edit(0, &c, |slot| slot.handicap = 3));
    assert!(!s.try_edit(0, &c, |_| {}), "no change");
}

#[test]
fn a_smaller_map_closes_the_last_computer_slots() {
    let mut s = setup(vec![
        SlotSetup::new(Occupant::Human),
        ai(),
        ai(),
        SlotSetup::closed(),
        ai(),
    ]);
    s.slots[1].start = Some(3);
    s.fit_to_map(2);
    assert_eq!(s.players(), 2);
    assert!(s.slots[1].occupant.occupied());
    assert_eq!(s.slots[2], SlotSetup::closed());
    assert_eq!(s.slots[4], SlotSetup::closed());
    assert_eq!(s.slots[1].start, None);
    assert_eq!(s.slots[0].occupant, Occupant::Human);
    let mut s = two();
    s.slots[1].start = Some(2);
    s.slots[0].start = Some(1);
    s.fit_to_map(2);
    assert_eq!(s.slots[1].start, None, "a 2-start map has no start 2");
    assert_eq!(s.slots[0].start, Some(1));
    let mut people = setup(vec![
        SlotSetup::new(Occupant::Human),
        SlotSetup::new(Occupant::Human),
    ]);
    people.fit_to_map(1);
    assert_eq!(people.players(), 2, "a map change never removes a person");
}

fn corners() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1000.0, 0.0], [0.0, 1000.0], [1000.0, 1000.0]]
}

#[test]
fn resolving_numbers_the_players_and_fills_every_random_choice() {
    let c = catalogue(4);
    let mut s = setup(vec![
        SlotSetup::new(Occupant::Human),
        SlotSetup::closed(),
        ai(),
        ai(),
    ]);
    s.slots[2].colour = Some(0);
    let a = s.resolve(7, &c, &corners()).unwrap();
    assert_eq!(a, s.resolve(7, &c, &corners()).unwrap());
    assert_eq!(a.len(), 3);
    assert_eq!(a.iter().map(|p| p.slot).collect::<Vec<_>>(), vec![0, 2, 3]);
    assert_eq!(
        a.iter().map(|p| p.player.get()).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(a[1].colour, 0);
    let mut colours: Vec<u8> = a.iter().map(|p| p.colour).collect();
    colours.sort_unstable();
    colours.dedup();
    assert_eq!(colours.len(), 3, "unique");
    assert!(a.iter().all(|p| c.random_pool.contains(&p.faction)));
    assert_eq!(a[0].ai, None);
    assert_eq!(a[1].ai, Some(Difficulty::Easy));
    let mut starts: Vec<u8> = a.iter().map(|p| p.start).collect();
    starts.sort_unstable();
    starts.dedup();
    assert_eq!(starts.len(), 3);
    assert!(starts.iter().all(|s| *s < 4));
    let all_random = setup(vec![SlotSetup::new(Occupant::Human), ai(), ai()]);
    let three = Catalogue {
        colours: 3,
        ..catalogue(4)
    };
    for seed in 0..32 {
        let mut got: Vec<u8> = all_random
            .resolve(seed, &three, &corners())
            .unwrap()
            .iter()
            .map(|p| p.colour)
            .collect();
        got.sort_unstable();
        assert_eq!(got, vec![0, 1, 2], "seed {seed}: every colour once");
    }
    let seeds: Vec<Vec<ResolvedPlayer>> = (0..16)
        .map(|seed| s.resolve(seed, &c, &corners()).unwrap())
        .collect();
    assert!(seeds.iter().any(|r| r != &a), "the seed matters");
    assert!(
        s.clone().resolve(0, &catalogue(1), &corners()).is_err(),
        "validated first"
    );
}

#[test]
fn random_starts_put_opponents_far_and_teammates_near() {
    let c = catalogue(4);
    let mut s = setup(vec![SlotSetup::new(Occupant::Human), ai()]);
    s.slots[0].start = Some(0);
    let r = s.resolve(1, &c, &corners()).unwrap();
    assert_eq!(r[1].start, 3, "the far corner");
    let mut s = setup(vec![SlotSetup::new(Occupant::Human), ai(), ai()]);
    s.slots[0].start = Some(0);
    s.slots[1].team = Some(0);
    s.slots[2].team = Some(0);
    let r = s.resolve(1, &c, &corners()).unwrap();
    assert_eq!(r[1].start, 3, "the team's first is placed far");
    assert!(
        r[2].start == 1 || r[2].start == 2,
        "its mate beside it: {}",
        r[2].start
    );
    let d = |a: u8, b: u8| {
        let (p, q) = (corners()[usize::from(a)], corners()[usize::from(b)]);
        let (dx, dy) = (p[0] - q[0], p[1] - q[1]);
        (dx * dx + dy * dy).sqrt()
    };
    assert!(d(r[2].start, r[1].start) < d(r[2].start, 0) + 1.0);
}

#[test]
fn a_teammate_takes_the_nearest_free_start() {
    let c = catalogue(4);
    let spots = vec![[0.0, 0.0], [0.0, 900.0], [900.0, 1000.0], [1000.0, 1000.0]];
    let mut s = setup(vec![SlotSetup::new(Occupant::Human), ai(), ai()]);
    s.slots[0].start = Some(0);
    s.slots[1].team = Some(1);
    s.slots[2].team = Some(1);
    let r = s.resolve(3, &c, &spots).unwrap();
    assert_eq!((r[1].start, r[2].start), (3, 2));
}

#[test]
fn the_command_lines_match_is_two_slots() {
    let s = SkirmishSetup::two_player(
        "FactionA".to_owned(),
        "FactionB".to_owned(),
        "maps/m.map".to_owned(),
        [false, true],
        Some(3),
    );
    assert_eq!(s.slots[0].occupant, Occupant::Human);
    assert_eq!(s.slots[1].occupant, Occupant::Ai(Difficulty::Easy));
    assert_eq!(s.slots[1].faction.as_deref(), Some("FactionB"));
    assert_eq!(s.army, Some(3));
    assert_eq!(s.validate(&catalogue(2)), Ok(()));
}

fn starts(s: &SkirmishSetup) -> Vec<Option<u8>> {
    s.slots.iter().map(|slot| slot.start).collect()
}

#[test]
fn a_start_click_passes_the_start_along() {
    let c = catalogue(4);
    let mut s = setup(vec![
        SlotSetup::new(Occupant::Human),
        SlotSetup::closed(),
        ai(),
    ]);
    assert!(s.click_start(2, &c));
    assert_eq!(starts(&s), vec![Some(2), None, None], "the first random");
    assert!(s.click_start(3, &c));
    assert_eq!(
        starts(&s),
        vec![Some(2), None, Some(3)],
        "a closed row skipped"
    );
    assert!(s.click_start(2, &c));
    assert_eq!(
        starts(&s),
        vec![None, None, Some(3)],
        "the holder's next random is past the last: freed"
    );
    s.slots[2].start = None;
    s.slots[0].start = Some(1);
    assert!(s.click_start(1, &c));
    assert_eq!(starts(&s), vec![None, None, Some(1)], "passed to the next");
    s.slots[0].start = Some(0);
    assert!(s.click_start(3, &c));
    assert_eq!(
        starts(&s),
        vec![Some(3), None, Some(1)],
        "none random: the local player moves"
    );
    assert!(!s.click_start(4, &c), "not on the map");
    s.slots[0].start = None;
    assert!(s.click_start(1, &c));
    assert_eq!(
        starts(&s),
        vec![None, None, None],
        "slot 0 is before the holder"
    );
    assert_eq!(s.validate(&c), Ok(()));
}

#[test]
fn a_command_point_factor_must_be_some() {
    let c = catalogue(4);
    let mut s = two();
    s.rules.command_point_factor = 0;
    assert_eq!(s.validate(&c), Err(SetupError::CommandPointFactor(0)));
    s.rules.command_point_factor = 10_000;
    assert_eq!(s.validate(&c), Ok(()));
}

#[test]
fn a_right_click_frees_a_start() {
    let mut s = two();
    s.slots[1].start = Some(2);
    assert!(s.clear_start(2));
    assert_eq!(starts(&s), vec![None, None]);
    assert!(!s.clear_start(2));
}

#[test]
fn a_resolved_setup_keeps_its_picks_for_the_match() {
    let mut s = two();
    s.slots[1].faction = None;
    s.slots[1].colour = None;
    s.slots[1].start = None;
    s.random_seed = 99;
    let shell = s.resolve(s.random_seed, &catalogue(4), &corners()).unwrap();
    assert_eq!(
        s.resolved_players(&catalogue(4), &corners()).unwrap(),
        shell
    );
    s.resolved = Some(shell.clone());
    let other = Catalogue {
        random_pool: vec!["PoolZ".to_owned()],
        ..catalogue(4)
    };
    assert_eq!(s.resolved_players(&other, &corners()).unwrap(), shell);
}
