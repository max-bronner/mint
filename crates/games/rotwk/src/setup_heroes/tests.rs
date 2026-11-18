use super::*;
use crate::lobby::{Difficulty, MatchRules, Occupant, SkirmishSetup, SlotSetup};

const MEN: u8 = 0;
const ELVES: u8 = 1;
const MORDOR: u8 = 4;

fn hero(name: &str, system: bool, usable: &[u8], default_faction: Option<u8>) -> HeroEntry {
    HeroEntry {
        name: name.to_owned(),
        system,
        factions: SubclassFactions {
            usable: usable.iter().fold(0, |m, b| m | (1 << b)),
            default_faction,
        },
        record: Arc::new(HeroRecord::default()),
    }
}

fn catalogue() -> Catalogue {
    Catalogue {
        factions: vec![
            "TplMen".to_owned(),
            "TplElf".to_owned(),
            "TplMordor".to_owned(),
        ],
        random_pool: vec![
            "TplMen".to_owned(),
            "TplElf".to_owned(),
            "TplMordor".to_owned(),
        ],
        colours: 8,
        starts: 4,
        heroes: vec![
            hero("SysMen", true, &[MEN], Some(MEN)),
            hero("SysMordor", true, &[MORDOR], Some(MORDOR)),
            hero("Mine", false, &[MEN, ELVES], None),
        ],
        sides: vec![
            ("TplMen".to_owned(), MEN),
            ("TplElf".to_owned(), ELVES),
            ("TplMordor".to_owned(), MORDOR),
            ("TplNone".to_owned(), 8),
        ],
    }
}

fn pick(faction: Option<u8>, hero: HeroPick) -> HeroPick {
    follow_army(hero, faction, &catalogue().heroes)
}

#[test]
fn an_army_change_keeps_a_usable_custom_hero_and_replaces_the_rest() {
    let (sys_men, sys_mordor, mine) = (
        HeroPick::Stored(0),
        HeroPick::Stored(1),
        HeroPick::Stored(2),
    );
    for (hero, faction, expected) in [
        (sys_men, Some(MORDOR), sys_mordor),
        (sys_men, Some(MEN), sys_men),
        (mine, Some(ELVES), mine),
        (mine, Some(MORDOR), sys_mordor),
        (mine, None, mine),
        (sys_men, None, HeroPick::Random),
        (HeroPick::None, Some(MORDOR), sys_mordor),
        (HeroPick::Random, Some(MEN), sys_men),
        (HeroPick::None, Some(ELVES), HeroPick::None),
        (sys_mordor, Some(ELVES), HeroPick::Random),
    ] {
        assert_eq!(pick(faction, hero), expected, "{hero:?} -> {faction:?}");
    }
}

fn player(faction: &str, hero: HeroPick) -> ResolvedPlayer {
    ResolvedPlayer {
        slot: 0,
        player: mint_sim::PlayerId::new(0),
        faction: faction.to_owned(),
        hero_pick: hero,
        hero: None,
        team: None,
        colour: 0,
        handicap: 0,
        start: 0,
        ai: None,
    }
}

#[test]
fn a_random_hero_is_any_listed_hero_the_army_can_use() {
    let c = catalogue();
    for (faction, expected) in [
        ("TplMordor", HeroPick::Stored(1)),
        ("TplElf", HeroPick::Stored(2)),
        ("TplNone", HeroPick::None),
    ] {
        for seed in 0..50 {
            let mut players = [player(faction, HeroPick::Random)];
            pick_random_heroes(&mut players, &c, seed);
            assert_eq!(players[0].hero_pick, expected, "{faction} seed {seed}");
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..50 {
        let mut players = [player("TplMen", HeroPick::Random)];
        pick_random_heroes(&mut players, &c, seed);
        if let HeroPick::Stored(i) = players[0].hero_pick {
            seen.insert(i);
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [0, 2]);
}

fn two_slots(first: SlotSetup) -> SkirmishSetup {
    SkirmishSetup {
        map_label: "maps/m.map".to_owned(),
        slots: vec![first, SlotSetup::new(Occupant::Ai(Difficulty::Easy))],
        rules: MatchRules::default(),
        army: None,
        spawn: None,
        spawn_buildings: None,
        dummies: None,
        dummies_attack: false,
        drive_templates: Vec::new(),
        wotr: None,
        cah_hero: None,
        random_seed: 0,
        resolved: None,
    }
}

#[test]
fn a_random_army_with_a_hero_resolves_to_an_army_the_hero_can_use() {
    let c = catalogue();
    for seed in 0..50 {
        let setup = two_slots(SlotSetup {
            hero: HeroPick::Stored(0),
            ..SlotSetup::new(Occupant::Human)
        });
        let players = setup.resolve(seed, &c, &[]).unwrap();
        assert_eq!(players[0].faction, "TplMen", "seed {seed}");
        assert!(players[0].hero.is_some());
    }
}

#[test]
fn the_custom_hero_rule_off_carries_no_hero_and_keeps_the_pick() {
    let c = catalogue();
    let mut setup = two_slots(SlotSetup {
        faction: Some("TplMen".to_owned()),
        hero: HeroPick::Stored(0),
        ..SlotSetup::new(Occupant::Human)
    });
    setup.rules.allow_custom_heroes = false;
    let players = setup.resolve(1, &c, &[]).unwrap();
    assert_eq!(
        (players[0].hero_pick, players[0].hero.is_some()),
        (HeroPick::None, false)
    );
    assert_eq!(setup.slots[0].hero, HeroPick::Stored(0));
}

#[test]
fn picking_an_army_on_the_screen_selects_its_default_hero() {
    let c = catalogue();
    let mut setup = two_slots(SlotSetup::new(Occupant::Human));
    assert!(setup.try_edit(0, &c, |s| s.faction = Some("TplMordor".to_owned())));
    assert_eq!(setup.slots[0].hero, HeroPick::Stored(1));
    assert!(!setup.try_edit(0, &c, |s| s.hero = HeroPick::Stored(2)));
    assert!(setup.try_edit(0, &c, |s| s.faction = Some("TplMen".to_owned())));
    assert_eq!(setup.slots[0].hero, HeroPick::Stored(0));
}
