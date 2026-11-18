use mint_sage_save::CampaignState;

use super::*;
use crate::campaign_save::{encode, Autosave};

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mint-save-load-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, save_type: u32, auto: u32, minute: u16, label: &str) {
    let autosave = Autosave {
        campaign: "SYNTHETIC_CAMPAIGN".to_owned(),
        difficulty: 1,
        state: CampaignState::default(),
        map_label: label.to_owned(),
        date: [2026, 10, 1, 4, 12, minute, 0, 0],
        description: "synthetic".to_owned(),
        is_auto_save: auto,
    };
    let bytes = if save_type == 1 {
        encode(&autosave).unwrap()
    } else {
        let info = mint_sage_save::GameStateBlock {
            save_file_type: save_type,
            date: autosave.date,
            ..mint_sage_save::GameStateBlock::default()
        };
        let mut file = mint_sage_save::SaveFile::default();
        file.push_with(mint_sage_save::CHUNK_GAME_STATE, |_| info.encode())
            .unwrap();
        file.to_bytes()
    };
    std::fs::write(dir.join(name), bytes).unwrap();
}

fn env(dir: &Path) -> Env {
    Env {
        folder: dir.to_path_buf(),
        new_save: "New".to_owned(),
        default_name: "Save %d".to_owned(),
        map_name: Rc::new(|label| label.to_owned()),
    }
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.stem.as_str()).collect()
}

#[test]
fn campaign_tab_lists_mint_and_retail_files_newest_first_and_splits_autosaves() {
    let dir = folder("scan");
    write(&dir, "older.mintCampaign", 1, 0, 5, "a");
    write(&dir, "newer.mintCampaign", 1, 0, 40, "b");
    write(&dir, "retail.BfME2Campaign", 1, 0, 20, "c");
    write(&dir, "auto.mintCampaign", 1, 1, 30, "d");
    write(&dir, "skirmish.BfME2Skirmish", 2, 0, 50, "e");
    std::fs::write(dir.join("broken.mintCampaign"), b"not a save").unwrap();
    let scan = scan(&dir, SaveTab::Campaign, &|l| l.to_owned());
    assert_eq!(names(&scan.game), ["newer", "retail", "older"]);
    assert_eq!(names(&scan.auto), ["auto"]);
    assert!(scan.game[1].retail && !scan.game[0].retail);
}

#[test]
fn game_types_follow_the_movies_order() {
    assert_eq!(game_types(0x0B), "CampaignSkirmishWOTRSP");
    assert_eq!(game_types(0x10), "WOTRMP");
}

#[test]
fn a_retail_row_cannot_be_deleted_and_saving_onto_its_name_asks_for_nothing() {
    let dir = folder("retail");
    write(&dir, "kept.BfME2Campaign", 1, 0, 20, "c");
    let mut screen = SaveLoad::open(Mode::Load, 1, false, env(&dir));
    assert_eq!(screen.enables(), [false, true, false]);
    screen.command("Delete");
    assert_eq!(screen.pending, Pending::None);
    assert!(dir.join("kept.BfME2Campaign").exists());

    let mut screen = SaveLoad::open(Mode::Save, 1, false, env(&dir));
    screen.select_game_row(1);
    assert_eq!(screen.entry, "kept");
    screen.command("Save");
    assert_eq!(screen.pending, Pending::None);
    let exit = screen.command("OnClosed");
    assert_eq!(
        exit,
        Some(Exit::Save {
            stem: "kept".to_owned(),
            description: "kept".to_owned()
        })
    );
}

#[test]
fn saving_onto_a_mint_file_asks_before_overwriting_and_a_confirmed_load_closes_with_the_path() {
    let dir = folder("confirm");
    write(&dir, "mine.mintCampaign", 1, 0, 20, "c");
    let mut screen = SaveLoad::open(Mode::Save, 1, false, env(&dir));
    screen.select_game_row(1);
    screen.command("Save");
    assert_eq!(screen.pending, Pending::Save);
    assert_eq!(screen.command("ConfirmationCancel"), None);
    assert_eq!(screen.pending, Pending::None);
    screen.command("Save");
    screen.command("ConfirmationOk");
    assert!(matches!(
        screen.command("OnClosed"),
        Some(Exit::Save { .. })
    ));

    let mut screen = SaveLoad::open(Mode::Load, 1, true, env(&dir));
    screen.command("Load");
    assert_eq!(screen.pending, Pending::Load);
    screen.command("ConfirmationOk");
    assert_eq!(
        screen.command("OnClosed"),
        Some(Exit::Load(dir.join("mine.mintCampaign")))
    );
}

#[test]
fn the_new_save_row_suggests_the_first_free_default_name() {
    let dir = folder("default");
    write(&dir, "Save 1.mintCampaign", 1, 0, 20, "c");
    let screen = SaveLoad::open(Mode::Save, 1, false, env(&dir));
    assert_eq!(screen.entry, "Save 2");
}
