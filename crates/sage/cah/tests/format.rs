#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use mint_sage_cah::{
    compute_crc, file_name, id_from_guid, parse, stats, write, CahError, Crc32, HeroRecord,
    PowerEntry,
};

fn sample() -> HeroRecord {
    let mut r = HeroRecord {
        object_id: 7,
        name: "Zed \u{e9}\u{1F600}".encode_utf16().collect(),
        class_index: 2,
        subclass_index: 1,
        colors: [0xFF01_0203, 0xFF04_0506, 0xFF07_0809],
        blings: vec![(b"GroupB".to_vec(), 3), (b"GroupA".to_vec(), 0)],
        id: Some(b"ABC123".to_vec()),
        is_system: Some(true),
        ..HeroRecord::default()
    };
    r.powers[0] = PowerEntry {
        button: vec![b'x'; 255],
        exp_level: 0,
        button_index: 1,
    };
    r.powers[1] = PowerEntry {
        button: b"PowerTwo".to_vec(),
        exp_level: 1,
        button_index: 4,
    };
    r
}

#[test]
fn a_written_record_reads_back_equal_with_a_valid_trailer() {
    let bytes = write(&sample());
    let parsed = parse(&bytes).unwrap();
    assert!(parsed.crc_ok);
    assert_eq!(parsed.trailing, 0);
    let mut expected = sample();
    expected.stored_crc = Some(compute_crc(&expected));
    assert_eq!(parsed.record, expected);
    assert_eq!(write(&parsed.record), bytes);
}

#[test]
fn crc32_matches_the_standard_check_value() {
    let mut crc = Crc32::new();
    crc.update(b"12345");
    crc.update(b"6789");
    assert_eq!(crc.finish(), 0xCBF4_3926);
}

#[test]
fn the_trailer_hashes_values_without_length_prefixes_or_ids() {
    let r = HeroRecord {
        object_id: 5,
        name: "Ab".encode_utf16().collect(),
        class_index: 1,
        subclass_index: 2,
        dummy: [0, 0],
        colors: [1, 2, 3],
        blings: vec![(b"G".to_vec(), 9)],
        id: Some(b"IGNORED".to_vec()),
        is_system: Some(true),
        ..HeroRecord::default()
    };
    let mut hashed = Vec::new();
    hashed.extend_from_slice(&5u32.to_le_bytes());
    hashed.extend_from_slice(b"Ab");
    for word in [1u32, 2, 0, 0, 1, 2, 3] {
        hashed.extend_from_slice(&word.to_le_bytes());
    }
    for _ in 0..15 {
        hashed.extend_from_slice(&[0; 8]);
    }
    hashed.extend_from_slice(&1u32.to_le_bytes());
    hashed.extend_from_slice(b"G");
    hashed.extend_from_slice(&9u32.to_le_bytes());
    hashed.push(1);
    let mut crc = Crc32::new();
    crc.update(&hashed);
    assert_eq!(compute_crc(&r), crc.finish());
}

#[test]
fn a_corrupted_trailer_is_reported_not_rejected() {
    let mut bytes = write(&sample());
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert!(!parse(&bytes).unwrap().crc_ok);
}

#[test]
fn old_record_versions_omit_the_id_flag_and_trailer() {
    let mut b = write(&HeroRecord::default());
    b[16] = 6;
    b.truncate(b.len() - 1 - 1 - 4);
    let parsed = parse(&b).unwrap();
    assert!(parsed.crc_ok);
    assert_eq!(parsed.record.id, None);
    assert_eq!(parsed.record.is_system, None);
    assert_eq!(parsed.trailing, 0);
}

#[test]
fn unsupported_versions_and_headers_error() {
    for version in [0u8, 9] {
        let mut b = write(&sample());
        b[16] = version;
        assert!(matches!(parse(&b), Err(CahError::RecordVersion(v)) if v == version));
    }
    let mut b = write(&sample());
    b[12] = 1;
    assert!(matches!(parse(&b), Err(CahError::Tagged)));
    let mut b = write(&sample());
    b[8] = 2;
    assert!(matches!(parse(&b), Err(CahError::ContainerVersion(2))));
    let mut b = write(&sample());
    b[0] = b'X';
    assert!(matches!(parse(&b), Err(CahError::BadMagic)));
}

#[test]
fn the_first_container_form_has_no_version_word() {
    let mut b = write(&HeroRecord::default());
    b[4] = b'1';
    b.drain(8..12);
    assert!(parse(&b).unwrap().crc_ok);
}

#[test]
fn every_truncation_errors() {
    let bytes = write(&sample());
    for n in 0..bytes.len() {
        assert!(parse(&bytes[..n]).is_err(), "prefix {n} parsed");
    }
}

#[test]
fn byte_flips_never_panic() {
    let bytes = write(&sample());
    for i in 0..bytes.len() {
        for mask in [0x01u8, 0x80, 0xFF] {
            let mut b = bytes.clone();
            b[i] ^= mask;
            let _ = parse(&b);
        }
    }
}

#[test]
fn a_huge_declared_length_on_a_short_buffer_errors() {
    let mut b = write(&HeroRecord::default());
    b.truncate(21);
    b.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(parse(&b).is_err());

    let mut b = write(&HeroRecord::default());
    let at = b.len() - 1 - 1 - 4 - 4;
    b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse(&b).is_err());
}

#[test]
fn ids_and_names_follow_the_game_format() {
    let guid = [
        0x78, 0x56, 0x34, 0x12, 0x0A, 0x00, 0xBC, 0x0D, 1, 2, 3, 0xAB, 9, 9, 9, 9,
    ];
    assert_eq!(id_from_guid(guid), "12345678ADBC123AB");
    assert_eq!(file_name("X1"), "MyHero_X1.cah");
}

#[test]
fn stat_lines_split_at_the_first_equals_after_leading_ones() {
    let map = stats::parse("a = 3\r\n b=\nnoequals\n==5\nc=1=2\nd = -7x\n");
    let got: Vec<(&str, i32)> = map.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    assert_eq!(
        got,
        vec![
            ("5", 0),
            ("a", 3),
            ("b", 0),
            ("c", 1),
            ("d", -7),
            ("noequals", 0)
        ]
    );
    assert_eq!(stats::write(&map).lines().next(), Some("5 = 0"));
}
