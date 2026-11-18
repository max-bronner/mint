#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn s(text: &str) -> AsciiString {
    AsciiString::from(text)
}

fn unit(name: &str, level: i32) -> CarryoverUnit {
    CarryoverUnit {
        template_name: s(name),
        count: 1,
        experience: 12.5,
        carried: CarriedSub {
            missions: 2,
            kills_total: 9,
            kills_mission: 4,
            text: WideString::from("tag"),
            label: s("label"),
            flag: true,
        },
        upgrades: UpgradeList(vec![s("UpgradeA"), s("UpgradeB")]),
        level,
    }
}

fn campaign() -> CampaignBlock {
    let long = "x".repeat(300);
    CampaignBlock {
        campaign: Some(CurrentCampaign {
            name: s("TestCampaign"),
            difficulty: 2,
            state: CampaignState {
                mission_index: 3,
                index_advanced: true,
                available: vec![unit("UnitOne", 3), unit(&long, 7)],
                mission_won: true,
                mission_lost: false,
                mission_ended: true,
                hero_record_count: 1,
                used: vec![],
                skill_points: 250.0,
                sciences: vec![s("ScienceA")],
                player_upgrades: UpgradeList(vec![s("UpgradeC")]),
                cash_bounty: 0.0,
            },
        }),
    }
}

fn full_file() -> Vec<u8> {
    let (lw, au, gsm, gl) = between_missions_defaults();
    let mut file = SaveFile::default();
    file.push_with(CHUNK_LIVING_WORLD_LOGIC, |_| lw.encode())
        .unwrap();
    file.push_with(CHUNK_AUDIO, |_| au.encode()).unwrap();
    file.push_with(b"CHUNK_Unknown", |_| vec![1, 2, 3, 4, 5])
        .unwrap();
    let info = GameStateBlock {
        save_file_type: 1,
        description: WideString::from("desc"),
        map_label: s("maps/next"),
        is_auto_save: 1,
        date: [2026, 10, 15, 4, 12, 30, 5, 0],
        ..GameStateBlock::default()
    };
    file.push_with(CHUNK_GAME_STATE, |_| info.encode()).unwrap();
    file.push_with(CHUNK_CAMPAIGN, |off| campaign().encode(off))
        .unwrap();
    file.push_with(CHUNK_GAME_STATE_MAP, |_| gsm.encode())
        .unwrap();
    file.push_with(CHUNK_GAME_LOGIC, |_| gl.encode()).unwrap();
    file.to_bytes()
}

#[test]
fn a_file_parses_and_writes_back_to_the_same_bytes_with_every_block_typed_or_raw() {
    let bytes = full_file();
    let file = SaveFile::parse(&bytes).unwrap();
    assert_eq!(file.to_bytes(), bytes);
    assert_eq!(file.blocks.len(), 7);
    let (i, block) = file.find(CHUNK_CAMPAIGN).unwrap();
    let decoded = CampaignBlock::decode(block.payload(), file.payload_offset(i).unwrap()).unwrap();
    assert_eq!(decoded, campaign());
    assert_eq!(
        file.find(b"CHUNK_Unknown").unwrap().1.payload(),
        [1, 2, 3, 4, 5]
    );
}

fn str_bytes(text: &str) -> Vec<u8> {
    let mut v = vec![u8::try_from(text.len()).unwrap()];
    v.extend_from_slice(text.as_bytes());
    v
}

#[test]
fn a_hand_assembled_record_reads_each_field_from_its_position() {
    let mut b = vec![1u8];
    b.extend(str_bytes("Tmpl"));
    b.extend(7i32.to_le_bytes());
    b.extend(1.5f32.to_le_bytes());
    b.push(5);
    b.extend(10i32.to_le_bytes());
    b.extend(30i32.to_le_bytes());
    b.extend(20i32.to_le_bytes());
    b.extend([2, b'h', 0, b'i', 0]);
    b.extend(str_bytes("lab"));
    b.push(1);
    b.extend([1, 1, 0]);
    b.extend(str_bytes("Up"));
    b.extend(9i32.to_le_bytes());
    let mut r = Reader::new(&b, 0);
    let u = CarryoverUnit::read(&mut r).unwrap();
    assert_eq!(u.template_name, s("Tmpl"));
    assert_eq!(u.count, 7);
    assert_eq!(u.experience, 1.5);
    assert_eq!(
        (
            u.carried.missions,
            u.carried.kills_total,
            u.carried.kills_mission
        ),
        (10, 30, 20)
    );
    assert_eq!(u.carried.text, WideString::from("hi"));
    assert_eq!(u.carried.label, s("lab"));
    assert!(u.carried.flag);
    assert_eq!(u.upgrades, UpgradeList(vec![s("Up")]));
    assert_eq!(u.level, 9);
    assert_eq!(r.remaining(), 0);
    let mut w = Writer::new(0);
    u.write(&mut w);
    assert_eq!(w.into_bytes(), b);
}

fn state_bytes(version: u8) -> Vec<u8> {
    let mut b = vec![version];
    b.extend(4i32.to_le_bytes());
    if version >= 6 {
        b.push(1);
    }
    b.extend([1, 0, 0, 0, 0]);
    b.extend([1, 0, 1]);
    if version >= 2 {
        b.extend(3i32.to_le_bytes());
        b.extend([1, 0, 0, 0, 0]);
    }
    if version >= 3 {
        b.extend(75.0f32.to_le_bytes());
        b.extend([1, 0, 0, 0, 0]);
    }
    if version >= 4 {
        b.extend([1, 0, 0]);
    }
    if version >= 5 {
        b.extend(0.5f32.to_le_bytes());
    }
    b
}

#[test]
fn every_state_version_reads_with_the_defaults_of_its_missing_fields() {
    for version in 1..=6u8 {
        let b = state_bytes(version);
        let st = CampaignState::read(&mut Reader::new(&b, 0)).unwrap();
        assert_eq!(st.mission_index, 4, "v{version}");
        assert!(
            st.mission_won && !st.mission_lost && st.mission_ended,
            "v{version}"
        );
        assert_eq!(st.index_advanced, version >= 6, "v{version}");
        assert_eq!(st.hero_record_count, if version >= 2 { 3 } else { 0 });
        assert_eq!(st.skill_points, if version >= 3 { 75.0 } else { 0.0 });
        assert_eq!(st.cash_bounty, if version >= 5 { 0.5 } else { 0.0 });
    }
}

#[test]
fn older_sub_records_skip_the_extra_i32_and_widen_the_early_string() {
    let head = |v: u8| {
        let mut b = vec![v];
        for n in [1i32, 2, 3] {
            b.extend(n.to_le_bytes());
        }
        b
    };
    let mut v4 = head(4);
    v4.extend(99i32.to_le_bytes());
    v4.extend([1, b'a', 0]);
    v4.extend(str_bytes("q"));
    v4.push(1);
    let sub = CarriedSub::read(&mut Reader::new(&v4, 0)).unwrap();
    assert_eq!((sub.kills_total, sub.kills_mission), (2, 3));
    assert_eq!(sub.text, WideString::from("a"));
    assert_eq!(sub.label, s("q"));
    assert!(sub.flag);

    let mut v1 = head(1);
    v1.extend(99i32.to_le_bytes());
    v1.extend(str_bytes("a"));
    let sub = CarriedSub::read(&mut Reader::new(&v1, 0)).unwrap();
    assert_eq!(sub.text, WideString::from("a"));
    assert_eq!(sub.label, AsciiString::default());
    assert!(!sub.flag);
}

fn hand_audio(view_type: u32, bits: u32) -> Vec<u8> {
    let mut b = vec![4u8];
    b.extend(view_type.to_le_bytes());
    b.extend(bits.to_le_bytes());
    b.push(0);
    for _ in 0..2 {
        b.extend(0u32.to_le_bytes());
        b.push(1);
        for _ in 0..12 {
            b.extend(1.0f32.to_le_bytes());
        }
        b.extend([1, 0, 0, 0, 0]);
        b.extend(1.0f32.to_le_bytes());
        b.push(0);
        b.extend([1, 0, 0, 0, 0]);
        b.extend([1, 0, 0, 0, 0]);
        b.extend(0u32.to_le_bytes());
    }
    b.extend(0i32.to_le_bytes());
    b.extend(0i32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(0i32.to_le_bytes());
    for _ in 0..4 {
        b.extend(0i32.to_le_bytes());
    }
    b.extend([1, 0, 0, 0, 0]);
    b.extend([1, 0, 0, 0, 0]);
    b
}

#[test]
fn the_defaults_write_the_observed_block_sizes_and_read_back_equal() {
    let (lw, au, gsm, gl) = between_missions_defaults();
    let sizes = [
        lw.encode().len(),
        au.encode().len(),
        gsm.encode().len(),
        gl.encode().len(),
    ];
    assert_eq!(sizes, [22, 206, 18, 130]);
    assert_eq!(LivingWorldBlock::decode(&lw.encode(), 0).unwrap(), lw);
    assert_eq!(AudioBlock::decode(&au.encode(), 0).unwrap(), au);
    assert_eq!(GameStateMapBlock::decode(&gsm.encode(), 0).unwrap(), gsm);
    assert_eq!(GameLogicBlock::decode(&gl.encode(), 0).unwrap(), gl);
}

#[test]
fn hand_assembled_living_world_and_audio_vectors_name_their_fields() {
    let mut lw = vec![6u8];
    for n in [0u32, 0, 0, 1094, u32::MAX] {
        lw.extend(n.to_le_bytes());
    }
    lw.push(0);
    let b = LivingWorldBlock::decode(&lw, 0).unwrap();
    assert_eq!((b.next_spawn_army_id, b.player_id), (1094, u32::MAX));
    assert_eq!(b.encode(), lw);

    let raw = hand_audio(2, 7);
    let a = AudioBlock::decode(&raw, 0).unwrap();
    assert_eq!((a.view_type, a.view_type_bits), (2, 7));
    let mut d = between_missions_defaults().1;
    d.view_type_bits = 7;
    assert_eq!(d.encode(), raw);
}

#[test]
fn in_game_state_in_the_four_blocks_is_refused_not_guessed() {
    let (lw, au, gsm, _) = between_missions_defaults();
    let mut active = lw.encode();
    *active.last_mut().unwrap() = 1;
    assert!(matches!(
        LivingWorldBlock::decode(&active, 0),
        Err(SaveError::NotBetweenMissions(_))
    ));
    let mut playing = au.encode();
    let at = playing.len() - 10 - 16 - 4;
    playing[at] = 1;
    assert!(AudioBlock::decode(&playing, 0).is_err());
    let mut mapped = gsm.encode();
    mapped[1] = 1;
    mapped.insert(2, b'm');
    assert!(matches!(
        GameStateMapBlock::decode(&mapped, 0),
        Err(SaveError::NotBetweenMissions(_))
    ));
}

#[test]
fn a_nested_end_past_its_parent_is_an_error() {
    let mut bytes = full_file();
    let file = SaveFile::parse(&bytes).unwrap();
    let (i, block) = file.find(CHUNK_CAMPAIGN).unwrap();
    let off = file.payload_offset(i).unwrap();
    let mut payload = block.payload().to_vec();
    let end_at = payload.windows(4).position(|w| w == b"KOLB").unwrap() + 4;
    let parent_end = off + payload.len();
    payload[end_at..end_at + 4]
        .copy_from_slice(&u32::try_from(parent_end + 1).unwrap().to_le_bytes());
    assert!(matches!(
        CampaignBlock::decode(&payload, off),
        Err(SaveError::BlockEnd { .. })
    ));
    let block_end_at = off - 4;
    let past = u32::try_from(bytes.len() + 1).unwrap().to_le_bytes();
    bytes[block_end_at..off].copy_from_slice(&past);
    assert!(matches!(
        SaveFile::parse(&bytes),
        Err(SaveError::BlockEnd { .. })
    ));
}

#[test]
fn every_proper_prefix_of_a_file_and_of_the_campaign_payload_errors() {
    let bytes = full_file();
    for len in 0..bytes.len() {
        assert!(SaveFile::parse(&bytes[..len]).is_err(), "prefix {len}");
    }
    let payload = campaign().encode(100);
    for len in 0..payload.len() {
        assert!(
            CampaignBlock::decode(&payload[..len], 100).is_err(),
            "prefix {len}"
        );
    }
}

#[test]
fn flipping_bytes_never_panics_and_whatever_still_parses_writes_back_identically() {
    let bytes = full_file();
    for i in 0..bytes.len() {
        for mask in [0xFFu8, 0x01, 0x80] {
            let mut copy = bytes.clone();
            copy[i] ^= mask;
            if let Ok(file) = SaveFile::parse(&copy) {
                assert_eq!(file.to_bytes(), copy, "byte {i}");
                for (n, block) in file.blocks.iter().enumerate() {
                    let off = file.payload_offset(n).unwrap();
                    let _ = CampaignBlock::decode(block.payload(), off);
                    let _ = AudioBlock::decode(block.payload(), off);
                    let _ = GameLogicBlock::decode(block.payload(), off);
                    let _ = GameStateBlock::decode(block.payload(), off);
                }
            }
        }
    }
}

#[test]
fn blocks_nested_past_the_depth_cap_error_instead_of_recursing() {
    fn nest(w: &mut Writer, depth: usize) {
        if depth > 0 {
            w.nested(|inner| nest(inner, depth - 1));
        }
    }
    let mut w = Writer::new(0);
    nest(&mut w, MAX_NEST_DEPTH + 40);
    let bytes = w.into_bytes();
    let mut levels = 0;
    let mut cur = Reader::new(&bytes, 0).nested();
    let err = loop {
        match cur {
            Ok(mut inner) => {
                levels += 1;
                cur = inner.nested();
            }
            Err(e) => break e,
        }
    };
    assert_eq!(err, SaveError::NestingTooDeep(MAX_NEST_DEPTH));
    assert_eq!(levels, MAX_NEST_DEPTH);
}

#[test]
fn headers_that_are_not_untagged_v1_are_refused() {
    let good = full_file();
    for (at, byte) in [(0usize, b'X'), (8, 2), (12, 1)] {
        let mut bad = good.clone();
        bad[at] = byte;
        assert_eq!(SaveFile::parse(&bad), Err(SaveError::BadHeader));
    }
    let mut extra = good;
    extra.push(0);
    assert!(matches!(
        SaveFile::parse(&extra),
        Err(SaveError::TrailingBytes { .. })
    ));
}

fn info_file(blocks_before: &[(&[u8], Vec<u8>)], info: &GameStateBlock) -> Vec<u8> {
    let mut file = SaveFile::default();
    for (name, payload) in blocks_before {
        file.push_with(name, |_| payload.clone()).unwrap();
    }
    file.push_with(CHUNK_GAME_STATE, |_| info.encode()).unwrap();
    file.push_with(b"CHUNK_After", |_| vec![9; 7]).unwrap();
    file.to_bytes()
}

#[test]
fn read_info_skips_blocks_before_the_info_and_does_not_look_past_it() {
    let info = GameStateBlock {
        save_file_type: 2,
        description: WideString::from("d"),
        ..GameStateBlock::default()
    };
    let mut bytes = info_file(&[(b"CHUNK_Early", vec![1, 2, 3])], &info);
    assert_eq!(read_info(&bytes).unwrap(), info);
    let tail = bytes.len() - (1 + END_TOKEN.len());
    bytes.truncate(tail);
    assert_eq!(read_info(&bytes).unwrap(), info);
}

#[test]
fn read_info_ignores_bytes_after_the_info_record_in_its_block() {
    let info = GameStateBlock::default();
    let mut payload = info.encode();
    payload.extend_from_slice(&[7; 40]);
    let mut file = SaveFile::default();
    file.push_with(CHUNK_GAME_STATE, |_| payload.clone())
        .unwrap();
    let bytes = file.to_bytes();
    assert_eq!(read_info(&bytes).unwrap(), info);
    assert!(SaveFile::parse(&bytes)
        .map(|f| GameStateBlock::decode(f.blocks[0].payload(), 0))
        .unwrap()
        .is_err());
}

#[test]
fn read_info_without_an_info_block_or_on_damaged_input_errors_without_panicking() {
    let mut file = SaveFile::default();
    file.push_with(b"CHUNK_Early", |_| vec![1]).unwrap();
    assert_eq!(
        read_info(&file.to_bytes()),
        Err(SaveError::MissingBlock("CHUNK_GameState"))
    );
    let bytes = info_file(&[], &GameStateBlock::default());
    for end in 0..bytes.len() - (27 + 7) {
        assert!(read_info(&bytes[..end]).is_err(), "prefix {end}");
    }
    for i in 0..bytes.len() {
        let mut copy = bytes.clone();
        copy[i] ^= 0xFF;
        let _ = read_info(&copy);
    }
}

#[test]
fn dates_order_by_every_field_but_the_day_of_week() {
    let base = SaveDate([2026, 10, 15, 4, 12, 30, 5, 100]);
    let same_but_weekday = SaveDate([2026, 10, 15, 0, 12, 30, 5, 100]);
    assert!(!base.is_newer_than(same_but_weekday));
    assert!(!same_but_weekday.is_newer_than(base));
    for later in [
        [2027, 1, 1, 0, 0, 0, 0, 0],
        [2026, 11, 1, 0, 0, 0, 0, 0],
        [2026, 10, 16, 0, 0, 0, 0, 0],
        [2026, 10, 15, 0, 13, 0, 0, 0],
        [2026, 10, 15, 0, 12, 31, 0, 0],
        [2026, 10, 15, 0, 12, 30, 6, 0],
        [2026, 10, 15, 0, 12, 30, 5, 101],
    ] {
        let later = SaveDate(later);
        assert!(later.is_newer_than(base), "{later:?}");
        assert!(!base.is_newer_than(later), "{later:?}");
    }
}

#[test]
fn sorting_puts_the_newest_first_and_keeps_ties_in_arrival_order() {
    let mut items = vec![
        ("a", [2026, 1, 1, 0, 0, 0, 0, 0]),
        ("b", [2026, 3, 1, 0, 0, 0, 0, 0]),
        ("c", [2026, 1, 1, 6, 0, 0, 0, 0]),
        ("d", [2026, 2, 1, 0, 0, 0, 0, 0]),
    ];
    sort_newest_first(&mut items, |i| SaveDate(i.1));
    let names: Vec<_> = items.iter().map(|i| i.0).collect();
    assert_eq!(names, ["b", "d", "a", "c"]);
}

#[test]
fn each_tab_keeps_its_save_types_and_names_its_extension() {
    let kept = |tab| {
        (0..12)
            .filter(|t| tab_accepts(tab, *t))
            .collect::<Vec<u32>>()
    };
    assert_eq!(kept(SaveTab::Campaign), [0, 1, 8]);
    assert_eq!(kept(SaveTab::Skirmish), [2]);
    assert_eq!(kept(SaveTab::WotrSingle), [3, 4]);
    assert_eq!(kept(SaveTab::WotrMulti), [6]);
    for (t, ext) in [
        (0, ".BfME2Campaign"),
        (1, ".BfME2Campaign"),
        (2, ".BfME2Skirmish"),
        (3, ".BfME2WotR"),
        (4, ".BfME2WotR"),
        (5, ".BfME2Campaign"),
        (6, ".BfME2WotRMP"),
        (9, ".BfME2Campaign"),
    ] {
        assert_eq!(extension(t), ext, "type {t}");
    }
    assert_eq!(SaveTab::WotrSingle.extension(), ".BfME2WotR");
}

#[test]
fn a_tab_lists_its_retail_and_its_mint_files_but_not_the_others() {
    assert_eq!(SaveTab::Campaign.mint_extension(), ".mintCampaign");
    assert_eq!(SaveTab::WotrMulti.mint_extension(), ".mintWotRMP");
    assert!(SaveTab::Campaign.lists_file("A.BfME2Campaign"));
    assert!(SaveTab::Campaign.lists_file("a.MINTCAMPAIGN"));
    assert!(!SaveTab::Campaign.lists_file("a.mintSkirmish"));
    assert!(!SaveTab::Skirmish.lists_file("a.BfME2Campaign"));
    assert!(has_retail_extension("x.bfme2wotrmp"));
    assert!(!has_retail_extension("x.mintWotRMP"));
}

#[test]
fn the_default_name_is_the_first_free_number_and_zero_when_none_is_free() {
    let ext = ".BfME2Campaign";
    assert_eq!(default_save_name("%d", ext, &[]), "1");
    assert_eq!(
        default_save_name(
            "Save %d",
            ext,
            &["save 1.bfme2campaign", "Save 3.BfME2Campaign"]
        ),
        "Save 2"
    );
    assert_eq!(default_save_name("%d", ext, &["1.BfME2Skirmish"]), "1");
    assert_eq!(
        default_save_name("fixed", ext, &["fixed.BfME2Campaign"]),
        "0"
    );
}
