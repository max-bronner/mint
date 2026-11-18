use mint_sage_ini::{BlockTable, Document};

use super::*;

const TEXT: &str = "LinearCampaign TEST_ALPHA
  Mission First
    Map = one
  End
  Mission Second
    Map = two
  End
End
LinearCampaign TEST_BETA
  Mission Only
    Map = three
  End
End
";

fn index() -> LinearCampaignIndex {
    let document = Document::parse("test.ini", TEXT, &BlockTable::corpus_path()).unwrap();
    let mut index = LinearCampaignIndex::new();
    index.insert_document(&document);
    index
}

#[test]
fn a_mission_resolves_by_name_or_by_one_based_position() {
    let index = index();
    let def = resolve_campaign(&index, "test_alpha").unwrap();
    let by = |text: &str| resolve_mission(def, Some(&MissionSelector::parse(text)));
    assert_eq!(by("Second"), Ok(1));
    assert_eq!(by("second"), Ok(1));
    assert_eq!(by("1"), Ok(0));
    assert_eq!(by("2"), Ok(1));
    assert_eq!(resolve_mission(def, None), Ok(0));
}

#[test]
fn a_missing_mission_or_campaign_errors_listing_the_valid_names() {
    let index = index();
    let def = resolve_campaign(&index, "TEST_ALPHA").unwrap();
    for text in ["0", "3", "Third"] {
        let error = resolve_mission(def, Some(&MissionSelector::parse(text))).unwrap_err();
        assert!(error.contains("1 First, 2 Second"), "{text}: {error}");
    }
    let error = resolve_campaign(&index, "NOPE").unwrap_err();
    assert!(error.contains("TEST_ALPHA, TEST_BETA"), "{error}");
}

#[test]
fn difficulty_words_map_to_the_three_campaign_levels() {
    assert_eq!(parse_difficulty("Easy"), Ok(Difficulty::Easy));
    assert_eq!(parse_difficulty("normal"), Ok(Difficulty::Medium));
    assert_eq!(parse_difficulty("HARD"), Ok(Difficulty::Hard));
    assert!(parse_difficulty("brutal").is_err());
}

#[test]
fn mission_and_difficulty_without_a_campaign_are_refused() {
    let flags = |c: Option<&str>, m: Option<&str>, d: Option<&str>| {
        CampaignDev::from_flags(
            c.map(str::to_owned),
            m.map(str::to_owned),
            d.map(str::to_owned),
        )
    };
    assert_eq!(flags(None, None, None), Ok(None));
    assert!(flags(None, Some("2"), None).is_err());
    assert!(flags(None, None, Some("hard")).is_err());
    let dev = flags(Some("X"), Some("3"), Some("hard")).unwrap().unwrap();
    assert_eq!(dev.mission, Some(MissionSelector::Position(3)));
    assert_eq!(dev.difficulty, Difficulty::Hard);
    assert_eq!(
        flags(Some("X"), None, None).unwrap().unwrap().difficulty,
        Difficulty::Medium
    );
}

#[test]
fn the_tutorial_flag_takes_basic_or_advanced_and_not_with_a_campaign() {
    use crate::tutorial::TutorialKind;
    assert_eq!(parse_tutorial(None, false), Ok(None));
    assert_eq!(
        parse_tutorial(Some("Advanced"), false),
        Ok(Some(TutorialKind::Advanced))
    );
    assert!(parse_tutorial(Some("strategic"), false).is_err());
    assert!(parse_tutorial(Some("basic"), true).is_err());
}
