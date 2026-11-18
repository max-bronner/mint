use mint_sage_cah::PowerEntry;
use mint_sage_ini::{BlingChoices, BlingDef, ClassDef, SubClassDef};

use super::*;

fn bling(upgrade: &str, group: &str) -> BlingDef {
    BlingDef {
        upgrade: Some(upgrade.to_owned()),
        group: Some(group.to_owned()),
        ..BlingDef::default()
    }
}

fn data() -> CahData {
    let system = CreateAHeroSystemDef {
        map_mode_upgrade: Some("Up_Map".to_owned()),
        game_mode_upgrade: Some("Up_Game".to_owned()),
        blings: vec![
            bling("Up_HatLate", "G_Hat"),
            bling("Up_HatEarly", "G_Hat"),
            bling("Up_Cape", "G_Cape"),
        ],
        classes: vec![ClassDef {
            upgrade: Some("Up_Class".to_owned()),
            subclasses: vec![SubClassDef {
                upgrade: Some("Up_Sub".to_owned()),
                bling_upgrades: vec![BlingChoices {
                    upgrades: vec![
                        "Up_HatLate".to_owned(),
                        "Up_Cape".to_owned(),
                        "Up_HatEarly".to_owned(),
                    ],
                    at: Vec::new(),
                }],
                ..SubClassDef::default()
            }],
            ..ClassDef::default()
        }],
        ..CreateAHeroSystemDef::default()
    };
    CahData {
        system,
        group_orders: BTreeMap::from([("up_hatlate".to_owned(), 5), ("up_hatearly".to_owned(), 1)]),
        upgrade_groups: Vec::new(),
    }
}

#[test]
fn a_bound_hero_is_given_mode_then_bling_then_class_then_subclass() {
    let record = HeroRecord {
        blings: vec![(b"G_Hat".to_vec(), 1), (b"G_Cape".to_vec(), 0)],
        ..HeroRecord::default()
    };
    let (grants, removes) = upgrade_names(&data(), &record, &|_| true);
    assert_eq!(
        grants,
        ["Up_Game", "Up_HatLate", "Up_Cape", "Up_Class", "Up_Sub"]
    );
    assert_eq!(removes, ["Up_Map"]);
}

#[test]
fn the_mode_swap_needs_both_upgrades_to_exist() {
    let (grants, removes) =
        upgrade_names(&data(), &HeroRecord::default(), &|name| name != "Up_Map");
    assert!(!grants.iter().any(|name| name == "Up_Game"));
    assert!(removes.is_empty());
}

#[test]
fn a_power_costs_nothing_extra_below_its_minimum_level_and_less_per_level_above_it() {
    let cost = |level| button_cost(1000, level, 3, 10);
    assert_eq!(cost(2), 1000);
    assert_eq!(cost(3), 1000);
    assert_eq!(cost(5), 800);
    assert_eq!(button_cost(0, 5, 3, 10), 0);
    assert_eq!(button_cost(999, 4, 3, 10), 900);
}

#[test]
fn a_later_pick_for_a_slot_replaces_the_first_only_once_the_hero_is_past_its_level() {
    let template = CommandSetDef {
        slots: BTreeMap::from([
            (1, "Template_One".to_owned()),
            (2, "Template_Two".to_owned()),
        ]),
        initial_visible: None,
    };
    let mut record = HeroRecord::default();
    record.powers[0] = PowerEntry {
        button: b"Early".to_vec(),
        exp_level: 0,
        button_index: 1,
    };
    record.powers[1] = PowerEntry {
        button: b"Late".to_vec(),
        exp_level: 2,
        button_index: 1,
    };
    let at = |rank| command_set_at(&template, &record, rank);
    assert_eq!(at(1).slot(2), Some("Early"));
    assert_eq!(at(2).slot(2), Some("Early"));
    assert_eq!(at(3).slot(2), Some("Late"));
    assert_eq!(at(3).slot(1), Some("Template_One"));
    assert_eq!(at(1).slot(ATTACK_MOVE_SLOT + 1), Some(ATTACK_MOVE_BUTTON));
}
