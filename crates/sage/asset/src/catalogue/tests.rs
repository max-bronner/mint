use super::*;
use crate::synth::{put_u16, put_u32, CatalogueBuilder};

const MAGIC_AT: usize = 0;
const VERSION_AT: usize = 4;
const RECORD_COUNT_AT: usize = 8;
const DEPENDENCY_COUNT_AT: usize = 12;
const FIRST_NAME_LEN_AT: usize = 16;
const FIRST_ENTRY_COUNT_AT: usize = 30;

fn sample() -> Vec<u8> {
    CatalogueBuilder::new()
        .record(
            "a.tga",
            0x01D6_0000_0000_0000,
            &[("A", AssetKind::Texture, 0, 0)],
        )
        .record(
            "b.w3d",
            0x01D6_0000_0000_0080,
            &[
                ("B_SKN", AssetKind::Mesh, 0, 64),
                ("HB", AssetKind::Hierarchy, 64, 32),
            ],
        )
        .dependency("b.w3d", "b_skn", &["a.tga"])
        .build()
}

fn parse(bytes: &[u8]) -> Result<Catalogue> {
    Catalogue::parse(bytes, "<synthetic>")
}

#[test]
fn a_well_formed_catalogue_lists_its_records_in_stored_order() {
    let catalogue = parse(&sample()).unwrap();

    let names: Vec<_> = catalogue
        .records()
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(names, ["a.tga", "b.w3d"]);
}

#[test]
fn a_well_formed_catalogue_consumes_every_byte() {
    assert_eq!(parse(&sample()).unwrap().trailing_bytes(), 0);
}

#[test]
fn an_entrys_byte_range_is_read_back_as_stored() {
    let catalogue = parse(&sample()).unwrap();

    let entry = catalogue.record("b.w3d").unwrap().entry("b_skn").unwrap();
    assert_eq!(entry.byte_range(), Some(0..64));
    assert_eq!(entry.kind, AssetKind::Mesh);
}

#[test]
fn a_texture_entry_covers_the_whole_file_rather_than_a_range() {
    let catalogue = parse(&sample()).unwrap();

    let entry = catalogue.record("a.tga").unwrap().entry("a").unwrap();
    assert_eq!(entry.byte_range(), None);
    assert_eq!(entry.slice(b"whatever").unwrap(), b"whatever");
}

#[test]
fn the_timestamp_is_two_dwords_low_first() {
    let catalogue = parse(&sample()).unwrap();

    assert_eq!(
        catalogue.record("a.tga").unwrap().timestamp,
        0x01D6_0000_0000_0000
    );
    assert_eq!(
        catalogue.record("b.w3d").unwrap().timestamp,
        0x01D6_0000_0000_0080
    );
}

#[test]
fn a_name_differing_only_in_case_resolves_to_the_same_record() {
    let catalogue = parse(
        &CatalogueBuilder::new()
            .record("MixedCase.TGA", 1, &[])
            .build(),
    )
    .unwrap();

    let canonical = catalogue.record("mixedcase.tga").unwrap();
    assert_eq!(catalogue.record("MIXEDCASE.TGA").unwrap(), canonical);
    assert_eq!(catalogue.record("MixedCase.TGA").unwrap(), canonical);
    assert_eq!(catalogue.record("mIxEdCaSe.tGa").unwrap(), canonical);
}

#[test]
fn every_record_is_findable_under_an_arbitrary_recasing_of_its_name() {
    let names = ["Apple.tga", "aPPLE2.tga", "Zebra.w3d", "zulu.w3d", "_x.tga"];
    let mut builder = CatalogueBuilder::new();
    for name in names {
        builder = builder.record(name, 1, &[]);
    }
    let catalogue = parse(&builder.build()).unwrap();

    for name in names {
        let upper = name.to_ascii_uppercase();
        let lower = name.to_ascii_lowercase();
        assert_eq!(
            catalogue.record(&upper).map(|r| r.name.as_str()),
            Some(name)
        );
        assert_eq!(
            catalogue.record(&lower).map(|r| r.name.as_str()),
            Some(name)
        );
    }
}

#[test]
fn a_lookup_that_differs_by_more_than_case_still_misses() {
    let catalogue = parse(&sample()).unwrap();

    assert!(catalogue.record("a.tgb").is_none());
    assert!(catalogue.record("a").is_none());
    assert!(catalogue.record("").is_none());
}

#[test]
fn a_duplicated_name_resolves_to_the_last_record_carrying_it() {
    let catalogue = parse(
        &CatalogueBuilder::new()
            .record("dup.tga", 1, &[("FIRST", AssetKind::Texture, 0, 0)])
            .record("DUP.TGA", 2, &[("SECOND", AssetKind::Texture, 0, 0)])
            .build(),
    )
    .unwrap();

    assert_eq!(catalogue.record("dup.tga").unwrap().timestamp, 2);
}

#[test]
fn an_entry_is_found_across_the_case_difference_between_the_two_tables() {
    let catalogue = parse(&sample()).unwrap();

    assert!(catalogue.record("b.w3d").unwrap().entry("B_SKN").is_some());
    assert!(catalogue.record("b.w3d").unwrap().entry("b_skn").is_some());
}

#[test]
fn dependencies_are_keyed_case_insensitively_on_both_halves() {
    let catalogue = parse(&sample()).unwrap();

    let found = catalogue.dependencies("B.W3D", "B_SKN").unwrap();
    assert_eq!(found.references, ["a.tga"]);
    assert!(catalogue.dependencies("b.w3d", "hb").is_none());
}

#[test]
fn a_catalogue_with_intact_references_passes_its_own_integrity_check() {
    parse(&sample()).unwrap().check_references().unwrap();
}

#[test]
fn an_empty_file_is_a_truncated_header() {
    assert!(matches!(
        parse(&[]),
        Err(Error::HeaderTruncated { len: 0, .. })
    ));
}

#[test]
fn a_file_shorter_than_the_header_is_a_truncated_header() {
    assert!(matches!(
        parse(&sample()[..15]),
        Err(Error::HeaderTruncated { len: 15, .. })
    ));
}

#[test]
fn wrong_magic_is_not_a_catalogue() {
    let mut bytes = sample();
    bytes[MAGIC_AT..MAGIC_AT.saturating_add(4)].copy_from_slice(b"BIG4");

    assert!(matches!(parse(&bytes), Err(Error::NotCatalogue { .. })));
}

#[test]
fn an_unknown_version_is_refused_rather_than_guessed_at() {
    let mut bytes = sample();
    put_u32(&mut bytes, VERSION_AT, 0x0000_0103);

    assert!(matches!(
        parse(&bytes),
        Err(Error::UnsupportedVersion {
            version: 0x0000_0103,
            ..
        })
    ));
}

#[test]
fn a_record_count_larger_than_the_file_could_hold_is_refused_before_allocating() {
    let mut bytes = sample();
    put_u32(&mut bytes, RECORD_COUNT_AT, u32::MAX);

    assert!(matches!(
        parse(&bytes),
        Err(Error::CountTooLarge {
            what: "asset records",
            count: 4_294_967_295,
            ..
        })
    ));
}

#[test]
fn a_dependency_count_larger_than_the_file_could_hold_is_refused() {
    let mut bytes = sample();
    put_u32(&mut bytes, DEPENDENCY_COUNT_AT, u32::MAX);

    assert!(matches!(
        parse(&bytes),
        Err(Error::CountTooLarge {
            what: "dependency records",
            ..
        })
    ));
}

#[test]
fn an_entry_count_larger_than_the_file_could_hold_is_refused() {
    let mut bytes = sample();
    put_u16(&mut bytes, FIRST_ENTRY_COUNT_AT, u16::MAX);

    assert!(matches!(
        parse(&bytes),
        Err(Error::CountTooLarge {
            what: "entries",
            count: 65_535,
            ..
        })
    ));
}

#[test]
fn a_name_length_running_past_the_end_is_truncation_not_a_panic() {
    let mut bytes = sample();
    bytes[FIRST_NAME_LEN_AT] = 255;

    assert!(matches!(parse(&bytes), Err(Error::Truncated { .. })));
}

#[test]
fn a_zero_length_name_is_refused() {
    let mut bytes = sample();
    bytes[FIRST_NAME_LEN_AT] = 0;

    assert!(matches!(
        parse(&bytes),
        Err(Error::EmptyName {
            what: "an asset record's name",
            offset: 16,
            ..
        })
    ));
}

#[test]
fn a_byte_range_running_past_its_target_file_is_out_of_bounds() {
    let catalogue = parse(&sample()).unwrap();
    let entry = catalogue.record("b.w3d").unwrap().entry("hb").unwrap();

    assert!(matches!(
        entry.slice(&[0u8; 80]),
        Err(Error::EntryOutOfBounds {
            offset: 64,
            end: 96,
            file_len: 80,
            ..
        })
    ));
    assert!(entry.slice(&[0u8; 96]).is_ok());
}

#[test]
fn a_dependency_on_a_name_that_does_not_exist_is_reported() {
    let catalogue = parse(
        &CatalogueBuilder::new()
            .record("b.w3d", 1, &[("B_SKN", AssetKind::Mesh, 0, 8)])
            .dependency("b.w3d", "b_skn", &["missing.tga"])
            .build(),
    )
    .unwrap();

    assert!(matches!(
        catalogue.check_references(),
        Err(Error::DanglingReference {
            reference,
            ..
        }) if reference == "missing.tga"
    ));
}

#[test]
fn a_dependency_record_owned_by_nothing_is_reported() {
    let catalogue = parse(
        &CatalogueBuilder::new()
            .record("b.w3d", 1, &[("B_SKN", AssetKind::Mesh, 0, 8)])
            .dependency("b.w3d", "nosuchmesh", &["b_skn"])
            .build(),
    )
    .unwrap();

    assert!(matches!(
        catalogue.check_references(),
        Err(Error::UnknownDependencyOwner { sub_asset, .. }) if sub_asset == "nosuchmesh"
    ));
}

#[test]
fn trailing_bytes_are_counted_rather_than_rejected() {
    let mut bytes = sample();
    bytes.extend_from_slice(b"junk");

    assert_eq!(parse(&bytes).unwrap().trailing_bytes(), 4);
}

#[test]
fn a_record_with_no_entries_parses() {
    let catalogue = parse(&CatalogueBuilder::new().record("empty.w3d", 1, &[]).build()).unwrap();

    assert!(catalogue.record("empty.w3d").unwrap().entries.is_empty());
}

#[test]
fn a_catalogue_with_no_records_at_all_parses() {
    let catalogue = parse(&CatalogueBuilder::new().build()).unwrap();

    assert!(catalogue.records().is_empty());
    assert_eq!(catalogue.trailing_bytes(), 0);
}

#[test]
fn every_truncation_of_a_valid_catalogue_errors_rather_than_panicking() {
    let bytes = sample();
    for len in 0..bytes.len() {
        let prefix = &bytes[..len];
        assert!(
            parse(prefix).is_err(),
            "a {len}-byte prefix parsed, but every strict prefix cuts a field short"
        );
    }
    assert!(parse(&bytes).is_ok());
}

#[test]
fn corrupting_any_single_byte_never_panics() {
    let bytes = sample();
    for at in 0..bytes.len() {
        for bit in 0..8u32 {
            let mut damaged = bytes.clone();
            damaged[at] ^= 1u8 << bit;
            let _ = parse(&damaged);
        }
    }
}
