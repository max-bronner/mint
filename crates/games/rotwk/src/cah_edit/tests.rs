#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::sync::Arc;

use mint_sage_asset::HeroStore;
use mint_sage_cah::HeroRecord;
use mint_sage_ini::{
    read_create_a_hero_system, AwardDef, AwardTrigger, BlockTable, CommandSetDef, Document,
    MacroTable, UpgradeIndex, ValueErrors,
};
use mint_vfs::Vfs;

use super::*;

fn fixture_text(points: u32, bling_lines: &str) -> String {
    format!(
        "CreateAHeroSystem\n\
         SpecialPowerDiscountPerLevel = 10\n\
         CreateAHeroBling\n GroupName = G_Hat\n BlingUpgradeName = Up_Hat1\nEnd\n\
         CreateAHeroBling\n GroupName = G_Hat\n BlingUpgradeName = Up_Hat2\nEnd\n\
         CreateAHeroBling\n GroupName = G_Hat\n BlingUpgradeName = Up_Hat3\nEnd\n\
         CreateAHeroBling\n GroupName = G_Arm\n BlingUpgradeName = Up_Arm1\nEnd\n\
         CreateAHeroBling\n GroupName = G_Arm\n BlingUpgradeName = Up_Arm2\nEnd\n\
         CreateAHeroBling\n GroupName = G_Arm\n BlingUpgradeName = Up_Arm3\nEnd\n\
         CreateAHeroBling\n GroupName = G_Dmg\n BlingUpgradeName = Up_Dmg1\nEnd\n\
         CreateAHeroBling\n GroupName = G_Dmg\n BlingUpgradeName = Up_Dmg2\nEnd\n\
         CreateAHeroBling\n GroupName = G_Dmg\n BlingUpgradeName = Up_Dmg3\nEnd\n\
         CreateAHeroBlingBinder\n GroupName = G_Hat\n UISlot = 0\n BlingType = APPEARANCE\nEnd\n\
         CreateAHeroBlingBinder\n GroupName = G_Shadowed\n UISlot = 0\n BlingType = APPEARANCE\nEnd\n\
         CreateAHeroBlingBinder\n GroupName = G_Arm\n UISlot = 0\n BlingType = ATTRIBUTE\nEnd\n\
         CreateAHeroBlingBinder\n GroupName = G_Dmg\n UISlot = 1\n BlingType = ATTRIBUTE\nEnd\n\
         CreateAHeroClass\n UpgradeName = Up_ClassA\n\
           SubClass\n SpendableAttributePoints = {points}\n\
             {bling_lines}\n\
             BlingUpgrades = Up_Arm1 Up_Arm2 Up_Arm3\n\
             BlingUpgrades = Up_Dmg1 Up_Dmg2 Up_Dmg3\n\
             Awards = AwardTest1 AwardTest2 AwardTest3 AwardTest4\n\
             Stats = StatTestA StatTestMissing StatTestB\n\
             Attribute\n GroupName = G_Arm\n MinValueUpgrade = Up_Arm1\n MaxValueUpgrade = Up_Arm3\n DefaultValueUpgrade = Up_Arm2\n End\n\
             Attribute\n GroupName = G_Dmg\n MinValueUpgrade = Up_Dmg1\n MaxValueUpgrade = Up_Dmg3\n DefaultValueUpgrade = Up_Dmg2\n End\n\
           End\n\
         End\n\
         CreateAHeroClass\n UpgradeName = Up_ClassB\n SubClass\n End\nEnd\n\
         End\n"
    )
}

fn upgrades(orders: &[(&str, i64)]) -> UpgradeIndex {
    let text: String = orders
        .iter()
        .map(|(name, order)| format!("Upgrade {name}\n  GroupOrder = {order}\nEnd\n"))
        .collect();
    let doc = Document::parse("u.ini", &text, &BlockTable::corpus_path()).unwrap();
    let mut index = UpgradeIndex::new();
    index.insert_document(&doc);
    index
}

fn definition(points: u32, bling_lines: &str) -> mint_sage_ini::CreateAHeroSystemDef {
    let text = fixture_text(points, bling_lines);
    let doc = Document::parse("cah.ini", &text, &BlockTable::corpus_path()).unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let errors = ValueErrors::new();
    let block = doc.roots_named("CreateAHeroSystem").next().unwrap();
    let def = read_create_a_hero_system(block, &errors, &MacroTable::new());
    assert!(errors.is_empty());
    def
}

pub(crate) fn system_with(
    points: u32,
    bling_lines: &str,
    orders: &[(&str, i64)],
    buttons: Vec<PowerButton>,
    template: Option<&CommandSetDef>,
) -> Arc<CahSystem> {
    Arc::new(
        CahSystem::new(
            &definition(points, bling_lines),
            &upgrades(orders),
            buttons,
            awards(),
            template,
        )
        .with_thing_stats(
            ["StatTestA", "StatTestB"]
                .map(|name| mint_sage_ini::ThingStatDef {
                    stat_name: Some(name.to_owned()),
                    name_tag: Some(format!("{name}Tag")),
                    description_tag: None,
                })
                .to_vec(),
        ),
    )
}

fn plain(points: u32) -> Arc<CahSystem> {
    system_with(
        points,
        "BlingUpgrades = Up_Hat1 @Up_Hat2 Up_Hat3",
        &[],
        Vec::new(),
        None,
    )
}

fn hero(sys: &Arc<CahSystem>) -> HeroEdit {
    HeroEdit::new_hero(Arc::clone(sys), 0, 0).unwrap()
}

fn options(sys: &CahSystem) -> &BinderOptions {
    sys.binder_options(0, 0, "G_Hat").unwrap()
}

#[test]
fn a_mark_counts_only_after_the_binders_first_bling() {
    for (line, upgrades, default) in [
        (
            "BlingUpgrades = Up_Hat1 @Up_Hat2",
            ["Up_Hat1", "Up_Hat2"],
            1,
        ),
        (
            "BlingUpgrades = @Up_Hat1 Up_Hat2",
            ["Up_Hat1", "Up_Hat2"],
            0,
        ),
        (
            "BlingUpgrades = Up_Hat1 Up_Hat2\nBlingUpgrades = @Up_Hat3",
            ["Up_Hat1", "Up_Hat2"],
            2,
        ),
    ] {
        let sys = system_with(0, line, &[], Vec::new(), None);
        let got = options(&sys);
        assert_eq!(got.default, default, "{line}");
        assert!(
            got.upgrades.starts_with(&upgrades.map(String::from)),
            "{line}"
        );
    }
}

#[test]
fn options_merge_across_lines_and_sort_stably_by_group_order() {
    let sys = system_with(
        0,
        "BlingUpgrades = Up_Hat1 Up_Hat2\nBlingUpgrades = Up_Hat3 Up_Hat2",
        &[("Up_Hat1", 5), ("Up_Hat2", 1), ("Up_Hat3", 1)],
        Vec::new(),
        None,
    );
    assert_eq!(options(&sys).upgrades, ["Up_Hat2", "Up_Hat3", "Up_Hat1"]);
}

#[test]
fn slots_come_from_the_first_binder_and_the_attribute_upgrades() {
    let sys = plain(4);
    let edit = hero(&sys);
    assert_eq!(edit.slot_group(SlotKind::Appearance, 0), Some("G_Hat"));
    let hat = edit.slot(SlotKind::Appearance, 0).unwrap();
    assert_eq!((hat.min, hat.max, hat.cur), (0, 2, 1));
    let arm = edit.slot(SlotKind::Attribute, 0).unwrap();
    assert_eq!((arm.min, arm.max, arm.default, arm.cur), (0, 2, 1, 0));
    assert_eq!(edit.points(), (4, 4));
}

#[test]
fn an_attribute_group_offers_its_blings_without_a_bling_line() {
    let text = fixture_text(9, "").replace("BlingUpgrades = Up_Arm1 Up_Arm2 Up_Arm3\n", "");
    let doc = Document::parse("cah.ini", &text, &BlockTable::corpus_path()).unwrap();
    let def = read_create_a_hero_system(
        doc.roots_named("CreateAHeroSystem").next().unwrap(),
        &ValueErrors::new(),
        &MacroTable::new(),
    );
    let upgrades_text = "Upgrade Up_Arm3\n GroupName = G_Arm\n GroupOrder = 3\nEnd\n\
                         Upgrade Up_Arm1\n GroupName = G_Arm\n GroupOrder = 1\nEnd\n\
                         Upgrade Up_Arm2\n GroupName = G_Arm\n GroupOrder = 2\nEnd\n";
    let udoc = Document::parse("u.ini", upgrades_text, &BlockTable::corpus_path()).unwrap();
    let mut index = UpgradeIndex::new();
    index.insert_document(&udoc);
    let sys = Arc::new(CahSystem::new(&def, &index, Vec::new(), awards(), None));
    let arm = sys.binder_options(0, 0, "G_Arm").unwrap();
    assert_eq!(arm.upgrades, ["Up_Arm1", "Up_Arm2", "Up_Arm3"]);
    let mut edit = hero(&sys);
    let slot = *edit.slot(SlotKind::Attribute, 0).unwrap();
    assert_eq!((slot.min, slot.max, slot.default), (0, 2, 1));
    edit.auto_change(AutoChange::Recommend);
    assert_eq!(edit.slot(SlotKind::Attribute, 0).unwrap().cur, 1);
}

#[test]
fn appearance_wraps_and_attributes_clamp_and_spend() {
    let sys = plain(4);
    let mut edit = hero(&sys);
    let hat = |e: &HeroEdit| e.slot(SlotKind::Appearance, 0).unwrap().cur;
    edit.step_appearance(0, 1);
    assert_eq!(hat(&edit), 2);
    edit.step_appearance(0, 1);
    assert_eq!(hat(&edit), 0);
    edit.step_appearance(0, -1);
    assert_eq!(hat(&edit), 2);

    for _ in 0..3 {
        edit.step_attribute(0, 1);
    }
    assert_eq!(edit.slot(SlotKind::Attribute, 0).unwrap().cur, 2);
    assert_eq!(edit.points(), (2, 4));
    edit.step_attribute(1, 1);
    edit.step_attribute(1, 1);
    edit.step_attribute(1, 1);
    assert_eq!(edit.slot(SlotKind::Attribute, 1).unwrap().cur, 2);
    assert_eq!(edit.points(), (0, 4));
}

#[test]
fn a_decrease_at_full_points_and_an_increase_at_none_do_nothing() {
    let sys = plain(1);
    let mut edit = hero(&sys);
    edit.step_attribute(0, -1);
    assert_eq!(edit.points(), (1, 1));
    edit.step_attribute(0, 1);
    edit.step_attribute(1, 1);
    assert_eq!(edit.slot(SlotKind::Attribute, 1).unwrap().cur, 0);
    assert_eq!(edit.points(), (0, 1));
}

#[test]
fn recommend_spends_in_slot_order_and_stops_when_points_run_out() {
    let sys = plain(1);
    let mut edit = hero(&sys);
    edit.auto_change(AutoChange::Recommend);
    assert_eq!(edit.slot(SlotKind::Attribute, 0).unwrap().cur, 1);
    assert_eq!(edit.slot(SlotKind::Attribute, 1).unwrap().cur, 0);
    edit.auto_change(AutoChange::Reset);
    assert_eq!(edit.points(), (1, 1));
}

#[test]
fn random_and_default_move_every_appearance_slot() {
    let sys = plain(0);
    let mut edit = hero(&sys);
    edit.auto_change(AutoChange::Random(&mut |_, max| max));
    assert_eq!(edit.slot(SlotKind::Appearance, 0).unwrap().cur, 2);
    edit.auto_change(AutoChange::Default);
    assert_eq!(edit.slot(SlotKind::Appearance, 0).unwrap().cur, 1);
}

#[test]
fn a_stored_record_restores_its_slots_and_spends_the_points() {
    let sys = plain(4);
    let record = HeroRecord {
        blings: vec![(b"G_Arm".to_vec(), 2), (b"G_Hat".to_vec(), 0)],
        ..HeroRecord::default()
    };
    let edit = HeroEdit::from_record(Arc::clone(&sys), record);
    assert_eq!(edit.slot(SlotKind::Attribute, 0).unwrap().cur, 2);
    assert_eq!(edit.slot(SlotKind::Appearance, 0).unwrap().cur, 0);
    assert_eq!(edit.points(), (2, 4));
}

pub(crate) fn button(
    name: &str,
    min_level: i64,
    prerequisite: Option<&str>,
    cost: u32,
) -> PowerButton {
    PowerButton {
        name: name.to_owned(),
        min_level,
        prerequisite: prerequisite.map(str::to_owned),
        allowable_upgrades: vec!["Up_ClassA".to_owned()],
        cost_if_selected: cost,
        icon: None,
        in_palantir: true,
        descript_label: None,
    }
}

fn powers(buttons: Vec<PowerButton>, template: Option<&CommandSetDef>) -> HeroEdit {
    let sys = system_with(0, "", &[], buttons, template);
    let mut edit = hero(&sys);
    edit.open_powers();
    edit
}

fn at(edit: &HeroEdit, name: &str) -> (usize, usize) {
    (0..edit.power_rows())
        .flat_map(|r| (0..4).map(move |c| (r, c)))
        .find(|&(r, c)| edit.power_button(r, c) == Some(name))
        .unwrap()
}

fn pick(edit: &mut HeroEdit, name: &str) -> PowerSelect {
    let (r, c) = at(edit, name);
    edit.select_power(r, c)
}

#[test]
fn rows_are_prerequisite_chains_and_a_new_root_starts_a_row() {
    let edit = powers(
        vec![
            button("PowerA", 1, None, 0),
            button("PowerB", 3, Some("PowerA"), 0),
            button("PowerC", 7, Some("PowerB"), 0),
            button("PowerD", 1, None, 0),
        ],
        None,
    );
    assert_eq!(edit.power_rows(), 2);
    assert_eq!(at(&edit, "PowerA"), (0, 0));
    assert_eq!(at(&edit, "PowerB"), (0, 1));
    assert_eq!(at(&edit, "PowerC"), (0, 2));
    assert_eq!(at(&edit, "PowerD"), (1, 0));
}

#[test]
fn a_high_minimum_level_moves_the_cell_right() {
    let edit = powers(
        vec![
            button("PowerA", 1, None, 0),
            button("PowerB", 7, Some("PowerA"), 0),
        ],
        None,
    );
    assert_eq!(at(&edit, "PowerB"), (0, 2));
}

#[test]
fn rows_sort_by_occupancy_and_equal_rows_keep_reverse_build_order() {
    let edit = powers(
        vec![
            button("PowerA", 1, None, 0),
            button("PowerB", 1, None, 0),
            button("PowerC", 7, Some("PowerA"), 0),
            button("PowerD", 3, Some("PowerB"), 0),
        ],
        None,
    );
    assert_eq!(at(&edit, "PowerB").0, 0);
    assert_eq!(at(&edit, "PowerA").0, 1);

    let tie = powers(
        vec![button("PowerX", 1, None, 0), button("PowerY", 1, None, 0)],
        None,
    );
    assert_eq!(at(&tie, "PowerY"), (0, 0));
    assert_eq!(at(&tie, "PowerX"), (1, 0));
}

#[test]
fn a_prerequisite_cycle_ends_and_leaves_its_members_out() {
    let edit = powers(
        vec![
            button("PowerP", 1, Some("PowerQ"), 0),
            button("PowerQ", 1, Some("PowerP"), 0),
            button("PowerR", 1, None, 0),
        ],
        None,
    );
    assert_eq!(edit.power_rows(), 1);
    assert_eq!(edit.power_button(0, 0), Some("PowerR"));
}

fn template(slots: &[u32]) -> CommandSetDef {
    CommandSetDef {
        slots: slots.iter().map(|&s| (s, format!("Button{s}"))).collect(),
        initial_visible: None,
    }
}

fn pick_set() -> Vec<PowerButton> {
    let mut set = vec![
        button("PowerA", 1, None, 0),
        button("PowerB", 1, Some("PowerA"), 0),
        button("PowerLate", 5, None, 0),
        button(NO_POWER_BUTTON, 0, None, 0),
    ];
    set.extend((1..=6).map(|n| button(&format!("PowerR{n}"), 1, None, 0)));
    set
}

#[test]
fn the_palantir_holds_six_less_the_leading_template_slots() {
    let tmpl = template(&[1, 3]);
    assert_eq!(leading_template_slots(&tmpl), 1);
    let mut edit = powers(pick_set(), Some(&tmpl));
    for n in 1..=5 {
        assert!(matches!(
            pick(&mut edit, &format!("PowerR{n}")),
            PowerSelect::Committed { .. }
        ));
    }
    assert_eq!(
        pick(&mut edit, "PowerR6"),
        PowerSelect::Rejected(CellState::PalantirFull)
    );
}

#[test]
fn every_cell_state_is_reachable() {
    let tmpl = template(&[1]);
    let mut edit = powers(pick_set(), Some(&tmpl));
    let state = |e: &HeroEdit, name: &str| {
        let (r, c) = at(e, name);
        e.power_state(r, c)
    };
    assert_eq!(state(&edit, "PowerLate"), CellState::LevelTooLow);
    assert_eq!(state(&edit, "PowerB"), CellState::NeedsPrereq);
    assert_eq!(state(&edit, "PowerA"), CellState::Available);
    assert_eq!(edit.power_state(9, 3), CellState::NoButton);

    pick(&mut edit, "PowerA");
    assert_eq!(state(&edit, "PowerA"), CellState::Selected);
    assert_eq!(state(&edit, "PowerB"), CellState::Available);
    for n in 1..=4 {
        pick(&mut edit, &format!("PowerR{n}"));
    }
    assert_eq!(state(&edit, "PowerR5"), CellState::PalantirFull);
}

#[test]
fn ten_picks_fill_the_power_book() {
    let mut set = vec![
        button("PowerE", 1, None, 0),
        button("PowerF", 1, None, 0),
        button(NO_POWER_BUTTON, 0, None, 0),
    ];
    for chain in ["C", "D"] {
        set.push(button(&format!("Power{chain}1"), 1, None, 0));
        for n in 2..=4 {
            let before = format!("Power{chain}{}", n - 1);
            set.push(button(&format!("Power{chain}{n}"), 1, Some(&before), 0));
        }
    }
    let mut edit = powers(set, None);
    for name in ["C1", "C2", "C3", "C4", "D1", "D2", "D3", "D4", "E"] {
        assert!(matches!(
            pick(&mut edit, &format!("Power{name}")),
            PowerSelect::Committed { .. }
        ));
    }
    edit.select_no_power();
    assert_eq!(edit.power_count(), 10);
    assert_eq!(
        pick(&mut edit, "PowerF"),
        PowerSelect::Rejected(CellState::BookFull)
    );
}

#[test]
fn an_upgrade_reuses_its_left_cell_slot_and_a_skip_takes_position_seven() {
    let tmpl = template(&[1]);
    let mut edit = powers(pick_set(), Some(&tmpl));
    assert_eq!(
        pick(&mut edit, "PowerA"),
        PowerSelect::Committed { flash: Some(1) }
    );
    assert_eq!(
        pick(&mut edit, "PowerB"),
        PowerSelect::Committed { flash: Some(1) }
    );
    assert_eq!(edit.palantir_used(), 1);
    assert_eq!(
        edit.select_no_power(),
        PowerSelect::Committed { flash: None }
    );
    let powers = &edit.record().powers;
    assert_eq!(powers[0].button_index, 1);
    assert_eq!(powers[1].button_index, 1);
    assert_eq!((powers[1].exp_level, powers[2].button_index), (1, 8));
}

#[test]
fn undoing_the_last_pick_frees_its_palantir_slot() {
    let mut edit = powers(pick_set(), Some(&template(&[1])));
    for n in 1..=5 {
        pick(&mut edit, &format!("PowerR{n}"));
    }
    edit.undo_power(2);
    assert_eq!(edit.power_count(), 5);
    edit.undo_power(5);
    assert_eq!((edit.power_count(), edit.palantir_used()), (4, 4));
    assert!(edit.record().powers[4].button.is_empty());
    edit.reset_powers();
    assert_eq!((edit.power_count(), edit.palantir_used()), (0, 0));
}

#[test]
fn the_wizard_line_follows_the_pick_count() {
    let mut edit = powers(pick_set(), Some(&template(&[1])));
    assert_eq!(edit.wizard(), Some((WizardLine::SelectFirstPower, None)));
    pick(&mut edit, "PowerR1");
    assert_eq!(
        edit.wizard(),
        Some((
            WizardLine::SelectSecondPower,
            Some(WizardSuffix::UpgradeOrNewPower)
        ))
    );
    pick(&mut edit, "PowerR2");
    assert_eq!(
        edit.wizard().unwrap().0,
        WizardLine::SelectNextPower { group: 1, level: 3 }
    );
}

#[test]
fn the_power_cost_discounts_per_level_above_the_minimum() {
    let mut edit = powers(
        vec![
            button("PowerP", 1, None, 100),
            button("PowerQ", 1, None, 100),
            button(NO_POWER_BUTTON, 0, None, 0),
        ],
        None,
    );
    edit.select_no_power();
    pick(&mut edit, "PowerP");
    pick(&mut edit, "PowerQ");
    assert_eq!(edit.power_cost(), 90 + 80);
    assert_eq!(edit.build_cost(50), 220);
}

fn award(name: &str, triggers: &[(&[&str], i64)]) -> AwardDef {
    AwardDef {
        name: Some(name.to_owned()),
        triggers: triggers
            .iter()
            .map(|(stats, threshold)| AwardTrigger {
                stats: stats.iter().map(|s| (*s).to_owned()).collect(),
                threshold: Some(*threshold),
            })
            .collect(),
        ..AwardDef::default()
    }
}

fn awards() -> Vec<AwardDef> {
    vec![
        award("AwardTest1", &[(&["StatA"], 3), (&["StatB"], 3)]),
        AwardDef {
            description_tag: Some("AwardDescTest2".to_owned()),
            ..award("AwardTest2", &[(&["StatA"], 3), (&["StatB"], 1)])
        },
        award("AwardTest3", &[]),
        award("AwardTest4", &[(&["StatA", "StatB"], 5)]),
    ]
}

#[test]
fn an_award_needs_a_trigger_and_every_trigger_met_over_summed_stats() {
    let sys = plain(0);
    let edit = hero(&sys);
    let stats = BTreeMap::from([("StatA".to_owned(), 3), ("StatB".to_owned(), 2)]);
    assert_eq!(edit.award_state(0, &stats), Some(false));
    assert_eq!(edit.award_state(1, &stats), Some(true));
    assert_eq!(edit.award_state(2, &stats), Some(false));
    assert_eq!(edit.award_state(3, &stats), Some(true));
    assert_eq!(edit.award_state(4, &stats), None);
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mint-cah-edit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn listed(tag: &str, names: &[&str]) -> HeroStore {
    let mut store = HeroStore::load(&Vfs::new(), scratch(tag), &[]).unwrap();
    for (i, name) in names.iter().enumerate() {
        store
            .save(HeroRecord {
                name: name.encode_utf16().collect(),
                id: Some(format!("T{i}").into_bytes()),
                ..HeroRecord::default()
            })
            .unwrap();
    }
    store
}

#[test]
fn a_name_is_taken_ignoring_case_and_blank_is_refused() {
    let store = listed("names", &["Bob"]);
    assert!(!name_is_valid("bob", &store));
    assert!(!name_is_valid("  ", &store));
    assert!(name_is_valid(" Alice ", &store));
}

#[test]
fn only_a_confirmed_delete_removes_the_hero() {
    let mut store = listed("delete", &["One", "Two"]);
    let declined = request_delete(&store, 0).unwrap();
    drop(declined);
    assert_eq!(store.heroes().len(), 2);
    let pending = request_delete(&store, 0).unwrap();
    let id = confirm_delete(&mut store, pending).unwrap();
    let file = store.own_dir().join(mint_sage_cah::file_name(&id));
    assert!(file.exists());
    store.remove_file(&id).unwrap();
    assert!(!file.exists());
    let left: Vec<_> = store
        .heroes()
        .iter()
        .map(|h| h.record.name_string())
        .collect();
    assert_eq!(left, ["Two"]);
}

#[test]
fn a_new_hero_is_stored_and_an_edit_keeps_the_heros_id() {
    let sys = plain(0);
    let mut store = listed("save", &["Old"]);
    let mut edit = hero(&sys);
    edit.set_name("  Fresh ");
    save_new(&mut store, &edit, "NEW1").unwrap();
    assert_eq!(store.heroes().len(), 2);
    assert_eq!(store.heroes()[1].record.name_string(), "Fresh");
    edit.set_name("Renamed");
    assert!(save_over(&mut store, &edit, 0).unwrap());
    assert_eq!(store.heroes().len(), 2);
    assert_eq!(store.heroes()[0].record.name_string(), "Renamed");
    assert_eq!(store.heroes()[0].record.id_str().as_deref(), Some("T0"));
}

#[test]
fn the_no_power_cell_can_be_picked_again_and_again() {
    let mut buttons = pick_set();
    buttons.push(button(NO_POWER_BUTTON, 0, None, 0));
    let mut edit = powers(buttons, None);
    for _ in 0..3 {
        assert!(matches!(
            edit.select_no_power(),
            PowerSelect::Committed { .. }
        ));
    }
    assert_eq!((edit.power_count(), edit.palantir_used()), (3, 0));
}

#[test]
fn undoing_a_pick_of_a_button_outside_the_palantir_keeps_its_slot() {
    let mut buttons = vec![button("PowerOut", 0, None, 0)];
    buttons[0].in_palantir = false;
    let mut edit = powers(buttons, None);
    let (r, c) = at(&edit, "PowerOut");
    edit.select_power(r, c);
    edit.undo_power(1);
    assert_eq!(edit.palantir_used(), 1);
}

#[test]
fn a_negative_minimum_level_does_not_move_the_cell() {
    let mut edit = powers(vec![button("PowerNeg", -3, None, 0)], None);
    assert_eq!(at(&edit, "PowerNeg"), (0, 0));
    edit.reset_powers();
}

#[test]
fn choosing_the_same_type_again_keeps_the_name_and_another_type_clears_it() {
    let sys = plain(0);
    let mut edit = hero(&sys);
    edit.set_name("Bob");
    assert!(edit.set_class_and_type(0, 0));
    assert_eq!(edit.name(), "Bob");
    assert!(edit.set_class_and_type(1, 0));
    assert_eq!(edit.name(), "");
}

#[test]
fn an_attribute_binder_with_no_line_takes_the_formula_not_the_group() {
    let sys = plain(4);
    let edit = HeroEdit::new_hero(Arc::clone(&sys), 1, 0).unwrap();
    let arm = edit.slot(SlotKind::Attribute, 0).unwrap();
    assert_eq!((arm.min, arm.max, arm.cur), (11, 19, 11));
    let dmg = edit.slot(SlotKind::Attribute, 1).unwrap();
    assert_eq!((dmg.min, dmg.max), (13, 24));
}

#[test]
fn retaking_a_type_keeps_the_hero_and_another_type_starts_over_with_its_colours() {
    let sys = system_with(
        0,
        "BlingUpgrades = Up_Hat1\nDefaultPrimaryColor = R:1 G:2 B:3",
        &[],
        Vec::new(),
        None,
    );
    let mut edit = hero(&sys);
    assert_eq!(edit.record().colors[0], 0xFF01_0203);
    edit.set_name("Keeper");
    edit.set_colour(0, 7);
    edit.set_class_and_type(0, 0);
    assert_eq!(
        (edit.name().as_str(), edit.record().colors[0]),
        ("Keeper", 7)
    );
    edit.set_class_and_type(1, 0);
    assert_eq!(edit.name(), "");
    assert_eq!(edit.record().colors[0], 0xFFFF_FFFF);
    edit.set_class_and_type(0, 0);
    assert_eq!(edit.record().colors[0], 0xFF01_0203);
    edit.set_colour(0, 7);
    edit.auto_change(AutoChange::Default);
    assert_eq!(edit.record().colors[0], 0xFF01_0203);
}

#[test]
fn dressing_lists_each_picked_bling_then_the_class_upgrade() {
    let sys = plain(10);
    let mut edit = hero(&sys);
    let before = edit.dress_upgrades();
    assert_eq!(before.last().map(String::as_str), Some("Up_ClassA"));
    assert!(before.iter().any(|name| name == "Up_Hat2"));
    edit.step_appearance(0, 1);
    let after = edit.dress_upgrades();
    assert!(after.iter().any(|name| name == "Up_Hat3") && !after.iter().any(|n| n == "Up_Hat2"));
}

#[test]
fn a_power_button_draws_its_button_image() {
    let def = mint_sage_ini::CommandButtonDef {
        button_image: Some("TestIcon_42x42".to_owned()),
        ..mint_sage_ini::CommandButtonDef::default()
    };
    assert_eq!(
        PowerButton::new("PowerTest", &def).icon.as_deref(),
        Some("TestIcon_42x42")
    );
}
