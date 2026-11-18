use super::*;

fn stat(
    kind_of: &[&str],
    excluded_kind_of: &[&str],
    names: &[&str],
    excluded: &[&str],
) -> ThingStatDef {
    let own = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect();
    ThingStatDef {
        kind_of: own(kind_of),
        excluded_kind_of: own(excluded_kind_of),
        thing_template_names: own(names),
        excluded_things: own(excluded),
        ..ThingStatDef::default()
    }
}

#[test]
fn a_victim_counts_by_kind_or_name_unless_excluded() {
    let has = |kinds: &'static [&'static str]| move |k: &str| kinds.contains(&k);
    let by_kind = stat(&["INFANTRY"], &["SUMMONED"], &[], &["Decoy"]);
    assert!(stat_counts(&by_kind, "Orc", has(&["INFANTRY"])));
    assert!(!stat_counts(
        &by_kind,
        "Orc",
        has(&["INFANTRY", "SUMMONED"])
    ));
    assert!(!stat_counts(&by_kind, "decoy", has(&["INFANTRY"])));
    assert!(!stat_counts(&by_kind, "Orc", has(&["HERO"])));
    let by_name = stat(&[], &[], &["MordorBalrog"], &[]);
    assert!(stat_counts(&by_name, "mordorbalrog", has(&[])));
    assert!(!stat_counts(&by_name, "Troll", has(&[])));
}

#[test]
fn only_skirmish_and_unranked_internet_matches_name_an_end_stat() {
    assert_eq!(
        end_keyword(GameMode::Skirmish, false, true),
        Some("SkirmishVictoryStatName")
    );
    assert_eq!(
        end_keyword(GameMode::Skirmish, false, false),
        Some("SkirmishDefeatStatName")
    );
    assert_eq!(
        end_keyword(GameMode::Internet, true, true),
        Some("OpenPlayVictoryStatName")
    );
    assert_eq!(end_keyword(GameMode::Internet, false, true), None);
    assert_eq!(end_keyword(GameMode::Lan, false, true), None);
    assert_eq!(end_keyword(GameMode::SinglePlayer, false, false), None);
}
