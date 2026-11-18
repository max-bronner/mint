use std::time::{Duration, UNIX_EPOCH};

use mint_sage_save::{
    AudioBlock, CarryoverUnit, GameLogicBlock, GameStateMapBlock, LivingWorldBlock,
};

use super::*;

fn synthetic() -> Autosave {
    let state = CampaignState {
        mission_index: 2,
        index_advanced: true,
        mission_won: true,
        mission_ended: true,
        available: vec![CarryoverUnit {
            template_name: "SyntheticHero".into(),
            count: 1,
            experience: 750.0,
            level: 3,
            ..CarryoverUnit::default()
        }],
        hero_record_count: 1,
        skill_points: 1200.0,
        ..CampaignState::default()
    };
    Autosave {
        campaign: "SYNTHETIC_CAMPAIGN".to_owned(),
        difficulty: 1,
        state,
        map_label: r"maps\synthetic two\synthetic two.map".to_owned(),
        date: [2026, 10, 15, 4, 12, 30, 5, 250],
        description: DESCRIPTION.to_owned(),
        is_auto_save: 1,
    }
}

#[test]
fn the_file_holds_the_six_blocks_in_retails_order_and_each_reads_back_exactly() {
    let autosave = synthetic();
    let file = SaveFile::parse(&encode(&autosave).unwrap()).unwrap();
    let names: Vec<&[u8]> = file.blocks.iter().map(|block| block.name()).collect();
    assert_eq!(
        names,
        [
            CHUNK_LIVING_WORLD_LOGIC,
            CHUNK_AUDIO,
            CHUNK_GAME_STATE,
            CHUNK_CAMPAIGN,
            CHUNK_GAME_STATE_MAP,
            CHUNK_GAME_LOGIC
        ]
    );
    let payload = |at: usize| (file.blocks[at].payload(), file.payload_offset(at).unwrap());
    let (bytes, offset) = payload(0);
    LivingWorldBlock::decode(bytes, offset).unwrap();
    let (bytes, offset) = payload(1);
    AudioBlock::decode(bytes, offset).unwrap();
    let (bytes, offset) = payload(2);
    let info = GameStateBlock::decode(bytes, offset).unwrap();
    assert_eq!(info.save_file_type, 1);
    assert_eq!(info.description.to_string_lossy(), DESCRIPTION);
    assert_eq!(info.map_label.to_string_lossy(), autosave.map_label);
    let (bytes, offset) = payload(3);
    let campaign = CampaignBlock::decode(bytes, offset)
        .unwrap()
        .campaign
        .unwrap();
    assert_eq!(campaign.state, autosave.state);
    let (bytes, offset) = payload(4);
    GameStateMapBlock::decode(bytes, offset).unwrap();
    let (bytes, offset) = payload(5);
    GameLogicBlock::decode(bytes, offset).unwrap();
}

#[test]
fn the_name_formats_the_map_name_and_a_missing_label_is_the_placeholder() {
    assert_eq!(
        file_name(Some("Auto Save for %s"), "Mission: Two"),
        "Auto Save for Mission_ Two.mintCampaign"
    );
    assert_eq!(file_name(None, "ignored"), "00000000.mintCampaign");
}

#[test]
fn a_timestamp_becomes_its_civil_date() {
    let at = UNIX_EPOCH + Duration::from_millis(1_000_000_000_250);
    assert_eq!(date_of(at), [2001, 9, 9, 0, 1, 46, 40, 250]);
}

#[test]
fn a_write_under_a_retail_extension_is_refused_and_leaves_the_file_alone() {
    let folder = std::env::temp_dir().join("mint-sc1-guard-test");
    std::fs::create_dir_all(&folder).unwrap();
    let retail = folder.join("Synthetic.BfME2Campaign");
    std::fs::write(&retail, b"retail").unwrap();
    assert!(write_into(&folder, "Synthetic.BfME2Campaign", b"mint").is_err());
    assert_eq!(std::fs::read(&retail).unwrap(), b"retail");
    for name in ["Synthetic.map", "Synthetic.WAK", "Synthetic.lws"] {
        assert!(write_into(&folder, name, b"mint").is_err(), "{name}");
    }
    let own = write_into(&folder, "Synthetic.mintCampaign", b"mint").unwrap();
    assert_eq!(std::fs::read(own).unwrap(), b"mint");
    std::fs::remove_dir_all(&folder).unwrap();
}
