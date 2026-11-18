#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::path::PathBuf;

use mint_sage_cah::HeroRecord;
use mint_vfs::Vfs;

use super::*;
use crate::cah_edit::tests::{button, system_with};

fn scratch(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("mint-cah-screen-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn open(
    dir: &std::path::Path,
    heroes: &[&str],
    buttons: Vec<crate::cah_edit::PowerButton>,
) -> Shared {
    let mut store = HeroStore::load(&Vfs::new(), dir, &[]).unwrap();
    for id in heroes {
        let record = HeroRecord {
            name: id.encode_utf16().collect(),
            id: Some(id.as_bytes().to_vec()),
            object_id: 9,
            ..HeroRecord::default()
        };
        store.save(record).unwrap();
    }
    let sys = system_with(0, "", &[], buttons, None);
    CahState::open(sys, Arc::new(StringTable::new()), store, false)
}

#[test]
fn entering_the_manager_writes_the_selected_hero_as_my_hero() {
    let dir = scratch("mine");
    let state = open(&dir, &["Aaa1", "Bbb2"], Vec::new());
    state.borrow_mut().handle("OnShowScreen", "Manager");
    let written = mint_sage_cah::parse(&std::fs::read(dir.join("MyHero.dat")).unwrap()).unwrap();
    assert_eq!(written.record.id_str().as_deref(), Some("Aaa1"));
    assert_eq!(written.record.object_id, 0);
    state.borrow_mut().select_row(1);
    state.borrow_mut().handle("OnShowScreen", "Manager");
    let written = mint_sage_cah::parse(&std::fs::read(dir.join("MyHero.dat")).unwrap()).unwrap();
    assert_eq!(written.record.id_str().as_deref(), Some("Bbb2"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn deleting_a_hero_asks_yes_or_no_and_only_yes_removes_it() {
    let dir = scratch("delete");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.handle("OnShowScreen", "Manager");
    st.handle("OnDeleteHero", "");
    let boxes = st.take_boxes();
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].kind, BoxKind::YesNo);
    st.answer_box(BoxButton::No);
    assert_eq!(st.store.heroes().len(), 1);
    st.handle("OnDeleteHero", "");
    st.answer_box(BoxButton::Yes);
    assert!(st.store.heroes().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_wizard_message_fires_only_while_a_power_is_available() {
    let dir = scratch("wizard");
    let fired = |state: &Shared| {
        let mut st = state.borrow_mut();
        st.handle("OnShowScreen", "Powers");
        st.fires.iter().any(|f| f.function == "OnNewWizardMessage")
    };
    let with_power = open(&dir, &[], vec![button("PowerTest", 1, None, 0)]);
    assert!(fired(&with_power));
    let without = open(&dir, &[], Vec::new());
    assert!(!fired(&without));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picked_colour_is_stored_in_its_slot_and_read_back() {
    let dir = scratch("colour");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.handle("OnShowScreen", "Manager");
    st.handle("OnShowScreen", "Appearance");
    st.handle("OnSkinColor", "4278255360");
    st.handle("OnPaintColor", "-1");
    let read = |st: &mut CahState, name: &str| st.extern_value(name);
    assert_eq!(
        read(&mut st, "CahAppearance::SkinColor"),
        str_value("4278255360")
    );
    assert_eq!(
        read(&mut st, "CahAppearance::PaintColor"),
        str_value("4294967295")
    );
    assert_ne!(
        read(&mut st, "CahAppearance::HairColor"),
        str_value("4278255360")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_selection_or_a_list_rebuild_tells_the_movie_which_buttons_to_enable() {
    let dir = scratch("enable");
    let state = open(&dir, &["Aaa1", "Bbb2"], Vec::new());
    let mut st = state.borrow_mut();
    let enables = |st: &mut CahState| {
        let n = st
            .fires
            .iter()
            .filter(|f| f.function == "EnableCustomizeButtons" && f.args == [Value::Bool(true)])
            .count();
        st.fires.clear();
        n
    };
    st.handle("OnShowScreen", "Manager");
    assert_eq!(enables(&mut st), 1);
    st.select_row(1);
    assert_eq!(enables(&mut st), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_cheer_starts_from_the_class_page_only_and_ends_on_entering_class() {
    assert!(cheer_on_enter(Page::Appearance, Page::Class, false));
    assert!(!cheer_on_enter(Page::Appearance, Page::Powers, false));
    assert!(cheer_on_enter(Page::Powers, Page::Appearance, true));
    assert!(!cheer_on_enter(Page::Class, Page::Manager, true));
}

#[test]
fn held_rotation_steps_only_on_a_due_shell_update() {
    let dir = scratch("rotate");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.held.rotate_right = true;
    let before = st.view.yaw;
    st.tick_view(false);
    assert_eq!(st.view.yaw, before);
    st.tick_view(true);
    assert!(st.view.yaw > before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_the_unqualified_rotate_name_is_dispatched() {
    let dir = scratch("rotate");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.handle("AptCreateAHero::Appearance::RotateLeft", "true");
    assert!(!st.held.rotate_left);
    st.handle("AptCreateAHero::RotateLeft", "true");
    assert!(st.held.rotate_left);
    st.handle("RotateLeft", "false");
    st.handle("RotateLeft", "true");
    assert!(st.held.rotate_left);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_own_hero_row_shows_the_marker_in_column_0_never_a_portrait() {
    let dir = scratch("list");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let list = state.borrow().hero_list();
    assert_eq!(
        list.rows[0][0],
        CellSpec::ScaledImage {
            image: OWN_HERO_MARKER.to_owned(),
            percent: OWN_HERO_MARKER_PERCENT,
        }
    );
    assert_eq!(list.rows[0][1], CellSpec::Text("Aaa1".to_owned()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picked_power_fills_its_palantir_slot_and_the_unreached_slots_are_cleared() {
    let dir = scratch("palantir");
    let mut power = button("PowerTest", 1, None, 0);
    power.icon = Some("IconTest".to_owned());
    let state = open(&dir, &[], vec![power]);
    let mut st = state.borrow_mut();
    st.handle("OnShowScreen", "Powers");
    st.handle("OnPowerSelect", "1,1");
    let writes = st.palantir_images(st.edit.as_ref().unwrap());
    assert_eq!(writes.first(), Some(&(0, Some("IconTest".to_owned()))));
    assert_eq!(writes.last(), Some(&(5, None)));
    assert!(!writes.contains(&(0, None)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn selecting_an_award_shows_its_description_and_no_award_clears_it() {
    let dir = scratch("award");
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.handle("OnShowScreen", "Manager");
    st.handle("AptCreateAHero::Manager::OnSelectAward", "1");
    assert!(
        st.award_description.contains("AwardDescTest2"),
        "{}",
        st.award_description
    );
    st.handle("AptCreateAHero::Manager::OnSelectAward", "-1");
    assert!(st.award_description.is_empty());
    drop(st);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_stats_list_shows_each_resolved_subclass_stat_with_the_heros_counter() {
    let dir = scratch("stats");
    std::fs::write(dir.join("Aaa1"), "StatTestB = 4\r\n").unwrap();
    let state = open(&dir, &["Aaa1"], Vec::new());
    let mut st = state.borrow_mut();
    st.handle("OnShowScreen", "Manager");
    st.refresh_stats();
    let list = st.stats_list();
    let text = |row: &Vec<CellSpec>| match (&row[0], &row[1]) {
        (CellSpec::Text(name), CellSpec::Text(value)) => (name.clone(), value.clone()),
        _ => panic!("{row:?}"),
    };
    let rows: Vec<_> = list.rows.iter().map(text).collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(
        rows[0].0.contains("StatTestATag") && rows[0].1 == "0",
        "{rows:?}"
    );
    assert!(
        rows[1].0.contains("StatTestBTag") && rows[1].1 == "4",
        "{rows:?}"
    );
    assert_eq!(list.columns, STATS_LIST_COLUMNS.to_vec());
    drop(st);
    let _ = std::fs::remove_dir_all(&dir);
}
