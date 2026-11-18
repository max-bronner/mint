use super::*;
use crate::synth::{put_be_u32, BigBuilder, TempArchive};

const COUNT_AT: usize = 8;
const FIRST_ENTRY_OFFSET_AT: usize = 16;
const FIRST_ENTRY_SIZE_AT: usize = 20;

fn sample() -> Vec<u8> {
    BigBuilder::new()
        .padding(8)
        .file(r"data\ini\gamedata.ini", b"GameData")
        .file(r"art\w3d\Ai\AiCamp.w3d", b"mesh bytes")
        .file("empty.txt", b"")
        .build()
}

fn open(bytes: &[u8]) -> Result<BigArchive> {
    let file = TempArchive::new(bytes);
    BigArchive::open(file.path())
}

fn readable(bytes: &[u8]) -> (TempArchive, BigArchive) {
    let file = TempArchive::new(bytes);
    let archive = BigArchive::open(file.path()).unwrap();
    (file, archive)
}

#[test]
fn a_well_formed_archive_lists_its_entries_in_stored_order() {
    let archive = open(&sample()).unwrap();

    let names: Vec<_> = archive.entries().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            r"data\ini\gamedata.ini",
            r"art\w3d\Ai\AiCamp.w3d",
            "empty.txt"
        ]
    );
}

#[test]
fn an_entry_reads_back_the_bytes_that_were_stored() {
    let (_file, archive) = readable(&sample());

    assert_eq!(
        archive
            .read(&VfsPath::new(r"DATA/INI/GameData.ini"))
            .unwrap(),
        b"GameData"
    );
}

#[test]
fn a_zero_length_entry_reads_back_as_no_bytes() {
    let (_file, archive) = readable(&sample());

    assert!(archive.read(&VfsPath::new("empty.txt")).unwrap().is_empty());
}

#[test]
fn the_declared_size_is_little_endian_and_matches_the_file() {
    let archive = open(&sample()).unwrap();

    assert_eq!(u64::from(archive.declared_size()), archive.file_len());
    assert_ne!(
        archive.declared_size().swap_bytes(),
        archive.declared_size()
    );
}

#[test]
fn the_entry_table_ends_at_or_before_the_declared_data_offset() {
    let archive = open(&sample()).unwrap();

    assert!(archive.entry_table_end() <= u64::from(archive.data_offset()));
}

#[test]
fn both_magic_numbers_are_accepted() {
    let big4 = open(&BigBuilder::new().file("a", b"1").build()).unwrap();
    let bigf = open(&BigBuilder::new().magic(b"BIGF").file("a", b"1").build()).unwrap();

    assert_eq!(big4.format(), BigFormat::Big4);
    assert_eq!(bigf.format(), BigFormat::BigF);
}

#[test]
fn an_archive_with_no_entries_opens_and_is_empty() {
    let archive = open(&BigBuilder::new().build()).unwrap();

    assert!(archive.entries().is_empty());
}

#[test]
fn names_are_addressable_by_their_normalised_form() {
    let archive = open(&sample()).unwrap();

    assert!(archive
        .entry(&VfsPath::new("art/w3d/ai/aicamp.w3d"))
        .is_some());
}

#[test]
fn a_name_repeated_with_different_case_resolves_to_the_last_one() {
    let (_file, archive) = readable(
        &BigBuilder::new()
            .file(r"Art\Textures\X.tga", b"first")
            .file("art/textures/x.tga", b"second")
            .build(),
    );

    assert_eq!(archive.entries().len(), 2);
    assert_eq!(
        archive.read(&VfsPath::new("art/textures/x.tga")).unwrap(),
        b"second"
    );
}

#[test]
fn a_missing_entry_is_not_found() {
    let archive = open(&sample()).unwrap();

    assert!(matches!(
        archive.read(&VfsPath::new("nope.txt")),
        Err(Error::NotFound { .. })
    ));
}

#[test]
fn an_empty_file_is_a_truncated_header() {
    assert!(matches!(
        open(&[]),
        Err(Error::HeaderTruncated { len: 0, .. })
    ));
}

#[test]
fn a_file_shorter_than_the_header_is_a_truncated_header() {
    let bytes = sample();

    assert!(matches!(
        open(bytes.get(..15).unwrap()),
        Err(Error::HeaderTruncated { len: 15, .. })
    ));
}

#[test]
fn the_wrong_magic_is_not_an_archive() {
    let bytes = BigBuilder::new().magic(b"REFP").file("a", b"1").build();

    assert!(matches!(
        open(&bytes),
        Err(Error::NotArchive { magic, .. }) if &magic == b"REFP"
    ));
}

#[test]
fn an_entry_count_larger_than_the_file_can_hold_is_rejected_before_allocating() {
    let mut bytes = sample();
    put_be_u32(&mut bytes, COUNT_AT, u32::MAX);

    assert!(matches!(
        open(&bytes),
        Err(Error::EntryCountTooLarge {
            count: u32::MAX,
            ..
        })
    ));
}

#[test]
fn a_table_cut_off_mid_entry_is_a_truncated_table() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BIG4");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(b"a-name-long-enough\0");

    assert!(matches!(
        open(&bytes),
        Err(Error::EntryTableTruncated {
            entry: 1,
            count: 2,
            ..
        })
    ));
}

#[test]
fn truncating_a_valid_archive_to_its_header_is_caught_by_the_count_check() {
    let bytes = sample();

    assert!(matches!(
        open(bytes.get(..20).unwrap()),
        Err(Error::EntryCountTooLarge {
            count: 3,
            needed: 27,
            available: 4,
            ..
        })
    ));
}

#[test]
fn a_name_with_no_terminator_is_reported_as_unterminated() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BIG4");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&16u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&vec![b'A'; 8192]);

    assert!(matches!(
        open(&bytes),
        Err(Error::UnterminatedName {
            entry: 0,
            limit: MAX_NAME_LEN,
            ..
        })
    ));
}

#[test]
fn an_entry_with_an_empty_name_is_rejected() {
    let bytes = BigBuilder::new().file("", b"x").build();

    assert!(matches!(
        open(&bytes),
        Err(Error::EmptyName { entry: 0, .. })
    ));
}

#[test]
fn an_entry_starting_past_the_end_of_the_file_is_out_of_bounds() {
    let mut bytes = sample();
    put_be_u32(&mut bytes, FIRST_ENTRY_OFFSET_AT, u32::MAX);

    assert!(matches!(open(&bytes), Err(Error::EntryOutOfBounds { .. })));
}

#[test]
fn an_entry_whose_size_runs_past_the_end_of_the_file_is_out_of_bounds() {
    let mut bytes = sample();
    put_be_u32(&mut bytes, FIRST_ENTRY_SIZE_AT, u32::MAX);

    assert!(matches!(open(&bytes), Err(Error::EntryOutOfBounds { .. })));
}

#[test]
fn an_offset_and_size_that_would_overflow_are_added_in_wider_arithmetic() {
    let mut bytes = sample();
    put_be_u32(&mut bytes, FIRST_ENTRY_OFFSET_AT, u32::MAX);
    put_be_u32(&mut bytes, FIRST_ENTRY_SIZE_AT, u32::MAX);

    assert!(matches!(
        open(&bytes),
        Err(Error::EntryOutOfBounds { end, .. }) if end == 8_589_934_590
    ));
}

#[test]
fn an_entry_from_another_archive_is_refused_rather_than_read() {
    let (_small_file, small) = readable(&BigBuilder::new().file("a", b"1").build());
    let (_large_file, large) = readable(&BigBuilder::new().file("b", &vec![b'x'; 4096]).build());
    let foreign = large.entries().first().unwrap().clone();

    assert!(matches!(
        small.read_entry(&foreign),
        Err(Error::EntryOutOfBounds { .. })
    ));
}

#[test]
fn every_truncation_of_a_valid_archive_errors_rather_than_panicking() {
    let bytes = sample();

    for len in 0..bytes.len() {
        let prefix = bytes.get(..len).unwrap();
        assert!(
            open(prefix).is_err(),
            "a {len}-byte prefix of a {}-byte archive was accepted",
            bytes.len()
        );
    }
    assert!(open(&bytes).is_ok());
}

#[test]
fn every_single_byte_corruption_is_handled_without_panicking() {
    let bytes = sample();

    for at in 0..bytes.len() {
        for mask in [0x01u8, 0x80, 0xff] {
            let mut damaged = bytes.clone();
            if let Some(byte) = damaged.get_mut(at) {
                *byte ^= mask;
            }
            let file = TempArchive::new(&damaged);
            if let Ok(archive) = BigArchive::open(file.path()) {
                for entry in archive.entries() {
                    let _ = archive.read_entry(entry);
                }
            }
        }
    }
}

#[test]
fn an_archive_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BigArchive>();
}
