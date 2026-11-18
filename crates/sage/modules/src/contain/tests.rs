use std::collections::BTreeMap;

use mint_sim::{Frame, ModuleFieldSource};

use super::*;

struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);

impl ModuleFieldSource for TestFields {
    fn field(&self, keyword: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
            .and_then(|(_, v)| v.first())
            .copied()
    }
    fn field_args(&self, keyword: &str) -> Vec<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

fn shipped_garrison() -> TestFields {
    TestFields(BTreeMap::from([
        (
            "ObjectStatusOfContained",
            vec!["UNSELECTABLE", "CAN_ATTACK", "ENCLOSED"],
        ),
        ("ContainMax", vec!["2"]),
        ("DamagePercentToUnits", vec!["0%"]),
        ("PassengerFilter", vec!["ANY", "+INFANTRY", "-CAVALRY"]),
        ("AllowEnemiesInside", vec!["No"]),
        ("AllowAlliesInside", vec!["No"]),
        ("AllowNeutralInside", vec!["No"]),
        ("AllowOwnPlayerInsideOverride", vec!["Yes"]),
        ("NumberOfExitPaths", vec!["1"]),
        (
            "PassengerBonePrefix",
            vec!["PassengerBone:ARROW_", "KindOf:INFANTRY"],
        ),
        ("EntryPosition", vec!["X:0.0", "Y:0.0", "Z:0.0"]),
        ("EntryOffset", vec!["X:50.0", "Y:0.0", "Z:0.0"]),
        ("ExitOffset", vec!["X:50.0", "Y:0.0", "Z:0.0"]),
        ("EnterSound", vec!["RuinedTowerEnterSound"]),
        ("ShowPips", vec!["No"]),
    ]))
}

#[test]
fn a_shipped_garrison_block_reads_its_capacity_status_and_geometry() {
    let module = horde_garrison_contain_from_fields(&shipped_garrison(), Frame::ZERO).unwrap();
    let open = module.open();
    assert_eq!(open.contain_max(), 2);
    assert_eq!(
        open.status_of_contained()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>(),
        ["UNSELECTABLE", "CAN_ATTACK", "ENCLOSED"]
    );
    assert_eq!(open.entry_offset().x, 50.0);
    assert_eq!(open.entry_position().x, 0.0);
    assert_eq!(open.passenger_bone_prefix(), Some("ARROW_"));
    assert_eq!(open.passenger_bone_kind(), Some("INFANTRY"));
    assert!(open.allow_own_player_inside());
    assert!(!open.allow_enemies_inside());
}

#[test]
fn an_undeclared_contain_admits_every_relationship_and_ejects_on_death() {
    let fields = TestFields(BTreeMap::new());
    let module = horde_garrison_contain_from_fields(&fields, Frame::ZERO).unwrap();
    let open = module.open();
    assert!(open.allow_allies_inside());
    assert!(open.allow_enemies_inside());
    assert!(open.allow_neutral_inside());
    assert!(open.allow_own_player_inside());
    assert!(open.eject_passengers_on_death());
    assert!(open.show_pips());
    assert!(!open.kill_passengers_on_death());
}

#[test]
fn a_contain_max_that_is_not_a_count_refuses_the_declaration() {
    let mut fields = shipped_garrison();
    fields.0.insert("ContainMax", vec!["lots"]);
    assert!(horde_garrison_contain_from_fields(&fields, Frame::ZERO).is_err());
}

#[test]
fn an_offset_that_is_not_x_y_z_refuses_the_declaration() {
    let mut fields = shipped_garrison();
    fields.0.insert("EntryOffset", vec!["50.0", "0.0"]);
    assert!(horde_garrison_contain_from_fields(&fields, Frame::ZERO).is_err());
}

#[test]
fn a_declaration_with_no_exit_path_count_still_has_one_way_out() {
    let fields = TestFields(BTreeMap::new());
    let module = horde_garrison_contain_from_fields(&fields, Frame::ZERO).unwrap();
    assert_eq!(module.open().number_of_exit_paths(), 1);
}

#[test]
fn a_horde_garrison_reads_forceorientationcontainer_no() {
    let mut fields = shipped_garrison();
    fields.0.insert("ForceOrientationContainer", vec!["No"]);
    let module = horde_garrison_contain_from_fields(&fields, Frame::ZERO).unwrap();
    assert!(!module.open().forces_orientation());

    let unwritten = horde_garrison_contain_from_fields(&shipped_garrison(), Frame::ZERO).unwrap();
    assert!(unwritten.open().forces_orientation());
}

#[test]
fn a_shipped_citadel_block_reads_two_hundred_percent_cash_back_and_the_ring_entry() {
    let fields = TestFields(BTreeMap::from([
        ("PassengerFilter", vec!["ANY", "+INFANTRY", "-HERO"]),
        ("ObjectStatusOfContained", vec!["UNSELECTABLE", "ENCLOSED"]),
        ("CashBackPercent", vec!["200%"]),
        ("ContainMax", vec!["99"]),
        ("StatusForRingEntry", vec!["HOLDING_THE_RING"]),
        (
            "UpgradeForRingEntry",
            vec!["Upgrade_RingHero", "Upgrade_FortressRingHero"],
        ),
        (
            "ObjectToDestroyForRingEntry",
            vec!["NONE", "+TheDroppedRing"],
        ),
        ("FXForRingEntry", vec!["FX_OneRingFlare"]),
    ]));
    let module = citadel_slaughter_horde_contain_from_fields(&fields, Frame::ZERO).unwrap();
    assert_eq!(module.cash_back_percent(), 2.0);
    assert_eq!(module.open().contain_max(), 99);
    let ring = module.ring().unwrap();
    assert_eq!(ring.status(), "HOLDING_THE_RING");
    assert_eq!(ring.upgrades().len(), 2);
    assert_eq!(ring.fx(), Some("FX_OneRingFlare"));
    assert!(ring.to_destroy().passes(&[], "TheDroppedRing"));
    assert!(!ring.to_destroy().passes(&[], "GondorArcher"));
}

#[test]
fn a_siege_engine_contain_defaults_speedpercentpercrew_to_one() {
    let fields = TestFields(BTreeMap::from([
        ("CrewFilter", vec!["ANY", "+INFANTRY"]),
        ("CrewMax", vec!["6"]),
    ]));
    let module = siege_engine_contain_from_fields(&fields, Frame::ZERO).unwrap();
    assert_eq!(module.crew_max(), 6);
    assert_eq!(module.speed_percent_per_crew(), 1.0);
}

#[test]
fn a_shipped_siege_engine_contain_reads_its_crew_fields() {
    let fields = TestFields(BTreeMap::from([
        ("CrewFilter", vec!["ANY", "+INFANTRY"]),
        ("CrewMax", vec!["6"]),
        ("InitialCrew", vec!["Crewman", "6"]),
        ("SpeedPercentPerCrew", vec!["50%"]),
        ("ContainMax", vec!["6"]),
    ]));
    let module = siege_engine_contain_from_fields(&fields, Frame::ZERO).unwrap();
    assert_eq!(module.crew_max(), 6);
    let crew = module.initial_crew().unwrap();
    assert_eq!((crew.template.as_str(), crew.count), ("Crewman", 6));
    assert_eq!(module.speed_percent_per_crew(), 0.5);
    assert!(module.crew_filter().passes(&["INFANTRY".to_owned()], "x"));
    assert!(!module.crew_filter().passes(&["CAVALRY".to_owned()], "x"));
}

#[test]
fn initial_crew_is_an_object_and_a_count() {
    for bad in [vec!["Crewman", "lots"], vec!["Crewman", "6", "7"]] {
        let fields = TestFields(BTreeMap::from([("InitialCrew", bad.clone())]));
        assert!(
            siege_engine_contain_from_fields(&fields, Frame::ZERO).is_err(),
            "accepted {bad:?}"
        );
    }
    for (args, count) in [(vec!["Crewman", "2"], 2), (vec!["Crewman"], 1)] {
        let fields = TestFields(BTreeMap::from([("InitialCrew", args)]));
        let module = siege_engine_contain_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.initial_crew().unwrap().count, count);
    }
}

#[test]
fn a_speedpercentpercrew_that_is_not_a_percentage_refuses_the_declaration() {
    let fields = TestFields(BTreeMap::from([("SpeedPercentPerCrew", vec!["lots"])]));
    assert!(siege_engine_contain_from_fields(&fields, Frame::ZERO).is_err());
}

#[test]
fn every_contain_module_name_is_a_module_the_parse_tables_declare() {
    for name in crate::CONTAIN_MODULE_NAMES {
        assert!(
            mint_sage_ini::tables::field_table::module(name).is_some(),
            "{name} names no module context"
        );
    }
}
