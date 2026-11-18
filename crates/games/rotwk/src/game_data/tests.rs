use super::*;

#[test]
fn rebuild_time_seconds_truncate_to_retail_frames_with_a_one_frame_floor() {
    assert_eq!([0.3, 2.5, 0.0].map(rebuild_time_ticks), [6, 72, 6]);
}

#[test]
fn retail_ini_order_never_loads_an_inc_fragment_directly() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/default/subsystemlegendexpansion1.ini",
        b"LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\Armor.ini\nEnd\n".to_vec(),
    );
    sources.insert(
        "data/ini/armor.ini",
        b"#include \"Fragment.inc\"\nArmor Local\n  Armor = DEFAULT 100%\nEnd\n".to_vec(),
    );
    sources.insert(
        "data/ini/fragment.inc",
        b"Armor Included\n  Armor = DEFAULT 50%\nEnd\n".to_vec(),
    );

    let table = BlockTable::corpus_path();
    let order = retail_ini_order(&sources, &table);
    assert_eq!(
        order,
        vec!["data/ini/armor.ini"],
        "the legend names only armor.ini; the fragment is not a file-scope document"
    );

    let reader = Reader::new(&sources);
    let mut documents = Vec::new();
    for path in &order {
        documents.push(reader.read(path, &table).unwrap());
    }
    let armor_names: Vec<&str> = documents
        .iter()
        .flat_map(mint_sage_ini::Document::roots)
        .filter_map(|block| block.name())
        .collect();
    assert_eq!(
        armor_names,
        vec!["Included", "Local"],
        "the fragment's own Armor still appears, once, via the #include - \
             before Local, since the #include line precedes it in armor.ini"
    );
}

#[test]
fn retail_ini_order_follows_the_legend_not_the_alphabet() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/default/subsystemlegendexpansion1.ini",
        b"LoadSubsystem TheScienceStore\n  InitFile = Data\\INI\\Science.ini\nEnd\n\
              LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\Armor.ini\nEnd\n"
            .to_vec(),
    );
    sources.insert(
        "data/ini/armor.ini",
        b"Armor A\n  Armor = DEFAULT 100%\nEnd\n".to_vec(),
    );
    sources.insert(
        "data/ini/science.ini",
        b"Science SCIENCE_TEST\nEnd\n".to_vec(),
    );

    let table = BlockTable::corpus_path();
    let order = retail_ini_order(&sources, &table);
    let science_at = order.iter().position(|p| p == "data/ini/science.ini");
    let armor_at = order.iter().position(|p| p == "data/ini/armor.ini");
    assert!(
        science_at < armor_at,
        "Science is declared first in the legend"
    );
}

#[test]
fn retail_ini_order_never_double_loads_a_rest_file_another_document_includes() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/default/subsystemlegendexpansion1.ini",
        b"LoadSubsystem TheSpecialPowerStore\n  InitFile = Data\\INI\\SpecialPower.ini\nEnd\n"
            .to_vec(),
    );
    sources.insert(
        "data/ini/specialpower.ini",
        b"#include \"CreateAHeroSpecialPowers.ini\"\n\
              SpecialPower FromSpecialPower\nEnd\n"
            .to_vec(),
    );
    sources.insert(
        "data/ini/createaherospecialpowers.ini",
        b"SpecialPower FromInclude\nEnd\n".to_vec(),
    );

    let table = BlockTable::corpus_path();
    let order = retail_ini_order(&sources, &table);
    assert_eq!(
        order,
        vec!["data/ini/specialpower.ini"],
        "createaherospecialpowers.ini is reached only through the #include"
    );

    let reader = Reader::new(&sources);
    let names: Vec<String> = order
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .flat_map(|document| {
            document
                .roots()
                .iter()
                .filter_map(|block| block.name().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        names,
        vec!["FromInclude", "FromSpecialPower"],
        "the included file's own SpecialPower still loads, once"
    );
}

#[test]
fn retail_ini_order_still_loads_a_file_no_legend_entry_names() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/default/subsystemlegendexpansion1.ini",
        b"LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\Armor.ini\nEnd\n".to_vec(),
    );
    sources.insert(
        "data/ini/armor.ini",
        b"Armor A\n  Armor = DEFAULT 100%\nEnd\n".to_vec(),
    );
    sources.insert(
        "data/ini/commandbutton.ini",
        b"CommandButton Build_Foo\nEnd\n".to_vec(),
    );

    let table = BlockTable::corpus_path();
    let order = retail_ini_order(&sources, &table);
    assert!(order.contains(&"data/ini/commandbutton.ini".to_owned()));
}

#[test]
fn a_kind_of_macro_across_documents_resolves_and_the_object_is_selectable() {
    let table = BlockTable::corpus_path();
    let gamedata = mint_sage_ini::Document::parse(
        "gamedata.ini",
        "#define GONDOR_RANGER_KINDOF PRELOAD SELECTABLE INFANTRY ARCHER\n",
        &table,
    )
    .unwrap();
    let object = mint_sage_ini::Document::parse(
        "gondorranger.ini",
        "Object GondorRanger\n  KindOf = GONDOR_RANGER_KINDOF SUMMONED\nEnd\n",
        &table,
    )
    .unwrap();
    let (data, _) = game_data_from_documents(&[gamedata, object], 0, std::time::Instant::now());
    let record = data.templates.get("GondorRanger").unwrap();
    assert!(record.has_kind("SELECTABLE"));
    assert!(
        record.has_kind("SUMMONED"),
        "the literal token after the macro survives"
    );
}

#[test]
fn the_builder_cycle_timeout_comes_from_the_in_game_ui_block() {
    let data = crate::game::tests::data_from(
        "InGameUI
  MaxSelectionSize = 0
  SelectNearestBuilderCycleTimeOut = 1234
End
",
    );
    assert_eq!(data.builder_cycle_timeout_ms, 1234);
    let data = crate::game::tests::data_from(
        "InGameUI
  MaxSelectionSize = 0
End
",
    );
    assert_eq!(data.builder_cycle_timeout_ms, SHIPPED_BUILDER_CYCLE_MS);
}

#[test]
fn the_user_data_leaf_travels_in_the_parsed_data() {
    let table = BlockTable::corpus_path();
    let with = mint_sage_ini::Document::parse(
        "gamedata.ini",
        "GameData\n  UserDataLeafName = SomeLeaf\nEnd\n",
        &table,
    )
    .unwrap();
    let (data, _) = game_data_from_documents(&[with], 0, std::time::Instant::now());
    assert_eq!(data.user_data_leaf.as_deref(), Some("SomeLeaf"));
    let (without, _) = game_data_from_documents(&[], 0, std::time::Instant::now());
    assert_eq!(without.user_data_leaf, None);
}

#[test]
fn a_document_that_cannot_be_parsed_refuses_the_load() {
    const LEGEND: &str = "data/ini/default/subsystemlegend.ini";
    const LOOP: &str = "data/ini/loop.ini";
    struct Cyclic;
    impl mint_vfs::Mount for Cyclic {
        fn label(&self) -> &str {
            "test"
        }
        fn entries(&self) -> Vec<mint_vfs::VfsPath> {
            vec![mint_vfs::VfsPath::new(LEGEND), mint_vfs::VfsPath::new(LOOP)]
        }
        fn read(&self, path: &mint_vfs::VfsPath) -> mint_vfs::Result<Vec<u8>> {
            if path.as_str() == LEGEND {
                Ok(
                    b"LoadSubsystem TheArmorStore\n  InitFile = Data\\INI\\loop.ini\nEnd\n"
                        .to_vec(),
                )
            } else {
                Ok(b"#include \"loop.ini\"\n".to_vec())
            }
        }
    }
    let mut vfs = Vfs::new();
    let _ = vfs.mount(std::sync::Arc::new(Cyclic));
    assert!(
        load_game_data(&vfs).is_err(),
        "an include cycle is an unreadable document, not one to skip"
    );
}

struct FixedFile {
    path: &'static str,
    bytes: &'static [u8],
}

impl mint_vfs::Mount for FixedFile {
    fn label(&self) -> &str {
        "test"
    }
    fn entries(&self) -> Vec<mint_vfs::VfsPath> {
        vec![mint_vfs::VfsPath::new(self.path)]
    }
    fn read(&self, path: &mint_vfs::VfsPath) -> mint_vfs::Result<Vec<u8>> {
        if path.as_str() == self.path {
            Ok(self.bytes.to_vec())
        } else {
            Err(mint_vfs::Error::NotFound { path: path.clone() })
        }
    }
}

#[test]
fn the_languages_command_map_resolves_a_meta_the_content_tree_leaves_unbound() {
    let mut vfs = Vfs::new();
    let _ = vfs.mount(std::sync::Arc::new(FixedFile {
        path: "data/ini/commandmap.ini",
        bytes: b"CommandMap SOME_OTHER_META\n  Key = A\nEnd\n",
    }));
    let _ = vfs.mount(std::sync::Arc::new(FixedFile {
        path: "commandmap.ini",
        bytes: b"CommandMap ORDERMODE_WAYPOINT\n  Key = Alt\n  Transition = DOWN\nEnd\n",
    }));
    let documents = match parse_corpus(&vfs) {
        Ok(parsed) => parsed.documents,
        Err(_) => panic!("parse_corpus refused the load"),
    };
    assert_eq!(read_order_mode_key(&documents), Some(Key::Alt));
}

#[test]
fn the_content_trees_command_map_wins_a_meta_both_copies_declare() {
    let mut vfs = Vfs::new();
    let _ = vfs.mount(std::sync::Arc::new(FixedFile {
        path: "data/ini/commandmap.ini",
        bytes: b"CommandMap ORDERMODE_WAYPOINT\n  Key = Alt\n  Transition = DOWN\nEnd\n",
    }));
    let _ = vfs.mount(std::sync::Arc::new(FixedFile {
        path: "commandmap.ini",
        bytes: b"CommandMap ORDERMODE_WAYPOINT\n  Key = Z\n  Transition = DOWN\nEnd\n",
    }));
    let documents = match parse_corpus(&vfs) {
        Ok(parsed) => parsed.documents,
        Err(_) => panic!("parse_corpus refused the load"),
    };
    assert_eq!(
        read_order_mode_key(&documents),
        Some(Key::Alt),
        "the content tree's copy, parsed after the language one, must win"
    );
}

#[test]
fn view_last_radar_event_resolves_key_space() {
    let table = mint_sage_ini::table::BlockTable::corpus_path();
    let bound = mint_sage_ini::Document::parse(
        "commandmap.ini",
        "CommandMap VIEW_LAST_RADAR_EVENT\n  Key = KEY_SPACE\n  Transition = DOWN\nEnd\n",
        &table,
    )
    .unwrap();
    assert_eq!(read_view_last_radar_event_key(&[bound]), Some(Key::Space));
    let other = mint_sage_ini::Document::parse(
        "commandmap.ini",
        "CommandMap VIEW_LAST_RADAR_EVENT\n  Key = KEY_F10\nEnd\n",
        &table,
    )
    .unwrap();
    assert_eq!(read_view_last_radar_event_key(&[other]), None);
    assert_eq!(read_view_last_radar_event_key(&[]), None);
}

#[test]
fn the_move_hint_and_damage_thresholds_come_from_the_game_data_block() {
    let data = crate::game::tests::data_from(
        "GameData
  MoveHintName = ModHint
  UnitDamagedThreshold = 0.8
  UnitReallyDamagedThreshold = 0.25
End
",
    );
    assert_eq!(data.move_hint_name, "ModHint");
    assert_eq!(data.damage_thresholds.damaged, 0.8);
    assert_eq!(data.damage_thresholds.really, 0.25);

    let bare = crate::game::tests::data_from("GameData\n  MoveHintName = Hint\nEnd\n");
    assert_eq!(
        bare.damage_thresholds,
        crate::render_state::DamageThresholds::DEFAULT
    );
}

#[test]
fn retail_duration_rounds_up_to_a_whole_retail_frame() {
    use mint_sim::SIMULATION_HZ;
    let per_retail_frame = SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
    assert_eq!(
        retail_duration(201.0).get(),
        per_retail_frame.saturating_mul(2),
        "201ms is barely over one 200ms retail frame"
    );
    assert_eq!(retail_duration(0.0).get(), 0);
}

#[test]
fn engaged_defaults_fall_back_to_retails_gamedata_values_not_zero() {
    let bare = crate::game::tests::data_from("GameData\n  MoveHintName = Hint\nEnd\n");
    assert_eq!(bare.engaged_defaults.max_distance, 33.0);
    assert_eq!(bare.engaged_defaults.timeout_ms, 1000.0);

    let written = crate::game::tests::data_from(
        "GameData\n  DefaultMaxDistanceForEngaged = 50\n  DefaultEngagedStateTimeout = 2000\nEnd\n",
    );
    assert_eq!(written.engaged_defaults.max_distance, 50.0);
    assert_eq!(written.engaged_defaults.timeout_ms, 2000.0);
}

#[test]
fn the_movement_penalty_starts_at_the_rung_game_data_names() {
    let data = crate::game::tests::data_from(
        "GameData
  UnitDamagedThreshold = 0.65
  UnitReallyDamagedThreshold = 0.4
  MovementPenaltyDamageState = DAMAGED
End
Object Walker
  Body = ActiveBody ModuleTag_Body
    MaxHealth = 100
  End
End
",
    );
    let (kinds, _) = crate::templates::KindTable::closure(&data, &["Walker".to_owned()]);
    let walker = kinds
        .kind("Walker")
        .and_then(|kind| kinds.get(kind))
        .unwrap();
    assert_eq!(walker.movement_penalty, 0.65);
}

#[test]
fn refund_percent_and_sell_percentage_are_read_independently() {
    let refund_only = mint_sage_ini::Document::parse(
        "t.ini",
        "GameData\n  RefundPercent = 60%\nEnd\n",
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let documents = [refund_only];
    let block = mint_sage_ini::game_data::merged_game_data(&documents);
    assert_eq!(read_refund_percent_field(&block), Some(0.6));
    assert_eq!(read_sell_percentage(&block), None, "not the same field");

    let sell_only = mint_sage_ini::Document::parse(
        "t.ini",
        "GameData\n  SellPercentage = 40%\nEnd\n",
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let documents = [sell_only];
    let block = mint_sage_ini::game_data::merged_game_data(&documents);
    assert_eq!(read_sell_percentage(&block), Some(0.4));
    assert_eq!(
        read_refund_percent_field(&block),
        None,
        "not the same field"
    );
}

#[test]
fn multiplay_multipliers_read_their_own_slot_and_default_the_rest() {
    let data = crate::game::tests::data_from(
        "GameData
  MultiPlayMoneyMult = MP1:1.0 MP2:1.5 MP3:1.0
  MultiPlayUnitXPMult = MP2:2.0
  MultiPlayUnitSpeedMult = MP2:0.5
End
",
    );
    assert_eq!(data.multiplay.money(2), 1.5);
    assert_eq!(data.multiplay.money(1), 1.0, "MP1 not written by MP2");
    assert_eq!(data.multiplay.unit_xp(2), 2.0);
    assert_eq!(data.multiplay.unit_speed(2), 0.5);
    assert_eq!(data.multiplay.building_xp(2), 1.0);
    assert_eq!(data.multiplay.building_speed(2), 1.0);
}

#[test]
fn solo_command_points_are_read_independently_of_the_multiplayer_table() {
    let data = crate::game::tests::data_from(
        "GameData
  GoodCommandPoints = 100 150
  EvilCommandPointsAI = 600 650
  GoodCommandPointsBonus = 20
  EvilCommandPointLimit = 600
End
",
    );
    assert_eq!(data.command_points_solo.good, Some((100, 150)));
    assert_eq!(data.command_points_solo.evil, None);
    assert_eq!(data.command_points_solo.evil_ai, Some((600, 650)));
    assert_eq!(data.command_points_solo.good_bonus, Some(20));
    assert_eq!(data.command_points_solo.evil_limit, Some(600));
    assert_eq!(data.command_points_solo.fields(), 4);
}

#[test]
fn parallel_document_read_equals_the_serial_read() {
    let mut sources = MemorySources::new();
    sources.insert(
        "data/ini/shared.inc",
        b"#define SHARED_RANGE 120\nArmor FromInclude\n  Armor = DEFAULT 50%\nEnd\n".to_vec(),
    );
    let mut paths = Vec::new();
    for n in 0..64u32 {
        let path = format!("data/ini/file{n:02}.ini");
        let mut text = String::new();
        if n % 3 == 0 {
            text.push_str("#include \"shared.inc\"\n");
        }
        for k in 0..(n % 7).saturating_add(1) {
            text.push_str(&format!(
                "#define RANGE_{n}_{k} {k}\nArmor Armor{n}_{k}\n  Armor = DEFAULT {k}0%\nEnd\n"
            ));
        }
        if n == 41 {
            text.push_str("Armor Unclosed\n  Armor = DEFAULT 1%\n");
        }
        sources.insert(&path, text.into_bytes());
        paths.push(path);
    }
    paths.push("data/ini/missing.ini".to_owned());
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let serial: Vec<_> = paths.iter().map(|path| reader.read(path, &table)).collect();
    let parallel = read_documents(&reader, &paths, &table);
    assert_eq!(parallel.len(), serial.len());
    for (path, (p, s)) in paths.iter().zip(parallel.iter().zip(&serial)) {
        match (p, s) {
            (Ok(p), Ok(s)) => assert_eq!(p, s, "{path}"),
            (Err(p), Err(s)) => assert_eq!(p.to_string(), s.to_string(), "{path}"),
            _ => panic!("{path}: one read failed and the other did not"),
        }
    }
    assert!(serial.last().is_some_and(Result::is_err));
}
