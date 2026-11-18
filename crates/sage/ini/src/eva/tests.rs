use super::*;
use crate::table::BlockTable;

fn read(texts: &[&str]) -> EvaDefs {
    let table = BlockTable::corpus_path();
    let documents: Vec<Document> = texts
        .iter()
        .enumerate()
        .map(|(i, text)| Document::parse(&format!("doc{i}.ini"), text, &table).unwrap())
        .collect();
    let mut macros = MacroTable::new();
    for document in &documents {
        macros.insert_defines(document.defines());
    }
    EvaDefs::read(&documents, &macros)
}

const DEFAULT: &str = "PredefinedEvaEvent DefaultEvaEvent\n  Priority = 4\n  TimeBetweenEventsMS = 7000\n  ExpirationTimeMS = 900\n  CountAsJumpToLocation = Yes\nEnd\n";

#[test]
fn a_new_event_starts_from_the_default_event() {
    let defs = read(&[
        DEFAULT,
        "#define WAIT 2500\nNewEvaEvent Alpha\n  Priority = 9\n  ExpirationTimeMS = WAIT\n  SideSound\n    Side = North\n    Sound = AlphaNorth\n  End\n  SideSound\n    Side = South\n  End\nEnd\n",
    ]);
    let alpha = &defs.events[defs.position("alpha").unwrap()];
    assert_eq!(alpha.priority, 9);
    assert_eq!(alpha.time_between_events_ms, 7000);
    assert_eq!(alpha.expiration_time_ms, 2500);
    assert!(alpha.count_as_jump_to_location);
    assert_eq!(alpha.sound_for("north"), Some("AlphaNorth"));
    assert_eq!(alpha.sound_for("South"), None);
}

#[test]
fn engine_events_hold_the_first_ids() {
    let defs = read(&[
        DEFAULT,
        "NewEvaEvent Beta\nEnd\nPredefinedEvaEvent UnitLevelUp\n  Priority = 2\nEnd\n",
    ]);
    assert_eq!(defs.position("UnitLevelUp"), Some(3));
    assert_eq!(
        defs.position("Beta"),
        Some(PREDEFINED_EVA_EVENT_NAMES.len())
    );
    assert_eq!(defs.events[3].priority, 2);
}

#[test]
fn an_announcer_keeps_the_constructor_values_it_does_not_write() {
    let defs = read(&[
        "ScoredKillEvaAnnouncer Gamma\n  EvaEvent = Beta\n  CountOnlyKillsByLocalPlayer = No\n  ObjectFilter = ANY +STRUCTURE\nEnd\n",
    ]);
    let gamma = &defs.announcers[0];
    assert_eq!(gamma.event, "Beta");
    assert!(!gamma.only_kills_by_local_player);
    assert!(gamma.only_kills_against_local_player);
    assert_eq!(gamma.minimum_count, 10);
    assert_eq!(gamma.maximum_time_ms, 1000);
    assert_eq!(gamma.object_filter, ["ANY", "+STRUCTURE"]);
}

#[test]
fn a_field_with_an_unparseable_value_is_counted_not_silently_dropped() {
    let defs = read(&[
        DEFAULT,
        "NewEvaEvent Alpha\n  Priority = banana\n  CountAsJumpToLocation = Sideways\nEnd\n",
    ]);
    let alpha = &defs.events[defs.position("alpha").unwrap()];
    assert_eq!(alpha.priority, 4, "the default's value, left unwritten");
    assert!(alpha.count_as_jump_to_location, "the default's own flag");
    assert_eq!(defs.malformed.len(), 2);
    assert!(defs
        .malformed
        .iter()
        .any(|e| e.keyword == "Priority" && e.token == "banana"));
    assert!(defs
        .malformed
        .iter()
        .any(|e| e.keyword == "CountAsJumpToLocation" && e.token == "Sideways"));
}

#[test]
fn jump_settings_read_what_the_block_writes_and_default_the_rest() {
    let documents =
        parse_documents(&["MiscEvaData\n  MinDistanceBetweenJumpToEvents = 400\nEnd\n"]);
    let settings = read_jump_settings(&documents);
    assert_eq!(settings.min_distance_between_jump_to_events, 400.0);
    assert_eq!(
        settings.max_milliseconds_before_resetting_last_jump_to,
        2000
    );
}

#[test]
fn a_later_misc_eva_data_block_replaces_the_earlier_one_s_field() {
    let documents = parse_documents(&[
        "MiscEvaData\n  MinDistanceBetweenJumpToEvents = 400\nEnd\n",
        "MiscEvaData\n  MinDistanceBetweenJumpToEvents = 250\nEnd\n",
    ]);
    let settings = read_jump_settings(&documents);
    assert_eq!(settings.min_distance_between_jump_to_events, 250.0);
}

#[test]
fn absent_misc_eva_data_keeps_the_code_defaults() {
    let settings = read_jump_settings(&parse_documents(&[]));
    assert_eq!(settings, JumpSettings::default());
}

fn parse_documents(texts: &[&str]) -> Vec<Document> {
    let table = BlockTable::corpus_path();
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| Document::parse(&format!("doc{i}.ini"), text, &table).unwrap())
        .collect()
}

#[test]
fn a_filter_macro_from_another_file_splices_in_mid_statement() {
    let defs = read(&[
        "#define NOT_LAIRS -Den -Burrow\n",
        "ScoredKillEvaAnnouncer Gamma\n  EvaEvent = Beta\n  ObjectFilter = ANY +STRUCTURE NOT_LAIRS -WALL\nEnd\n",
    ]);
    assert_eq!(
        defs.announcers[0].object_filter,
        ["ANY", "+STRUCTURE", "-Den", "-Burrow", "-WALL"]
    );
}
