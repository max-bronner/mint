use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mint_sage_asset::{AssetKind, Catalogue, Resolver};
use mint_sage_big::BigArchive;
use mint_vfs::Vfs;

const W3D_HEADER_LEN: u32 = 8;

const W3D_HAS_SUBCHUNKS: u32 = 0x8000_0000;

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("MINT_CORPUS").map(PathBuf::from)
}

fn archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in listing.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    let is_big = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("big"));
                    if is_big {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

fn mount(root: &Path) -> (Vfs, usize) {
    let paths = archives(root);
    let mut vfs = Vfs::new();
    let mut mounted = 0usize;
    for path in &paths {
        if let Ok(archive) = BigArchive::open(path) {
            vfs.mount(Arc::new(archive));
            mounted += 1;
        }
    }
    println!(
        "{}: {mounted} of {} archives mounted, {} visible paths",
        root.display(),
        paths.len(),
        vfs.len()
    );
    (vfs, mounted)
}

fn open_catalogue(root: &Path) -> mint_sage_asset::Result<Catalogue> {
    let path = root.join("asset.dat");
    assert!(
        path.is_file(),
        "{} has no asset.dat; is MINT_CORPUS pointing at an install root?",
        root.display()
    );
    Catalogue::open(&path)
}

fn lowercased(name: &str) -> Vec<u8> {
    name.bytes().map(|byte| byte.to_ascii_lowercase()).collect()
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn the_shipped_catalogue_parses_with_every_byte_accounted_for() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let started = Instant::now();
    let catalogue = open_catalogue(&root).unwrap_or_else(|error| panic!("{error}"));
    let elapsed = started.elapsed();

    let entries: usize = catalogue.records().iter().map(|r| r.entries.len()).sum();
    let references: usize = catalogue
        .dependency_records()
        .iter()
        .map(|r| r.references.len())
        .sum();

    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut extensions: BTreeMap<String, usize> = BTreeMap::new();
    for record in catalogue.records() {
        let extension = record
            .name
            .rsplit_once('.')
            .map_or_else(|| "<none>".to_owned(), |(_, ext)| ext.to_ascii_lowercase());
        *extensions.entry(extension).or_default() += 1;
        for entry in &record.entries {
            *kinds.entry(entry.kind.to_string()).or_default() += 1;
        }
    }

    println!(
        "{}: {} asset records, {} entries, {} dependency records, {} references, parsed in {:.0} ms",
        catalogue.label(),
        catalogue.records().len(),
        entries,
        catalogue.dependency_records().len(),
        references,
        elapsed.as_secs_f64() * 1000.0,
    );
    println!("  extensions: {extensions:?}");
    println!("  kinds:      {kinds:?}");

    assert_eq!(
        catalogue.trailing_bytes(),
        0,
        "{} bytes are unaccounted for after both tables",
        catalogue.trailing_bytes()
    );

    let mut inversions = Vec::new();
    for pair in catalogue.records().windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if lowercased(&a.name) >= lowercased(&b.name) {
            inversions.push(format!("{} then {}", a.name, b.name));
        }
    }
    println!(
        "  case-insensitive ordering: {} of {} adjacent pairs increase",
        catalogue.records().len().saturating_sub(1) - inversions.len(),
        catalogue.records().len().saturating_sub(1)
    );
    for line in inversions.iter().take(20) {
        println!("    {line}");
    }
    assert!(
        inversions.is_empty(),
        "{} adjacent name pairs are not strictly increasing under a lowercased comparison",
        inversions.len()
    );

    let mut miscased = Vec::new();
    for record in catalogue.records() {
        for spelling in [
            record.name.to_ascii_uppercase(),
            record.name.to_ascii_lowercase(),
        ] {
            match catalogue.record(&spelling) {
                Some(found) if found.name == record.name => {}
                Some(found) => miscased.push(format!("{spelling} found {} instead", found.name)),
                None => miscased.push(format!("{spelling} was not found at all")),
            }
        }
    }
    for line in miscased.iter().take(20) {
        println!("    {line}");
    }
    assert!(
        miscased.is_empty(),
        "{} recased lookups did not return their own record",
        miscased.len()
    );
    println!(
        "  case-insensitive lookup: {} of {} records found under both an all-upper and an all-lower spelling",
        catalogue.records().len(),
        catalogue.records().len()
    );

    catalogue
        .check_references()
        .unwrap_or_else(|error| panic!("{error}"));
    println!(
        "  referential integrity: all {} dependency records and {references} references resolve",
        catalogue.dependency_records().len()
    );
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_catalogued_name_resolves_against_this_installs_own_archives() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let catalogue = open_catalogue(&root).unwrap_or_else(|error| panic!("{error}"));
    let (vfs, _) = mount(&root);
    let resolver = Resolver::new();

    let started = Instant::now();
    let mut resolved = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut buckets: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut ambiguous: BTreeMap<String, usize> = BTreeMap::new();
    let mut winners: BTreeMap<String, usize> = BTreeMap::new();
    let mut ambiguous_names = 0usize;

    for record in catalogue.records() {
        match resolver.resolve(&vfs, &record.name) {
            Ok(path) => {
                resolved += 1;
                let bucket = if path.as_str().starts_with("art/w3d/") {
                    "art/w3d"
                } else if path.as_str().starts_with("data/editor/molds/") {
                    "data/editor/molds"
                } else if path.as_str().starts_with("shaders/compiled/") {
                    "shaders/compiled"
                } else if path.as_str().starts_with("art/textures/") {
                    "art/textures (apt_)"
                } else {
                    "art/compiledtextures"
                };
                *buckets.entry(bucket).or_default() += 1;
                if let Some((_, extension)) = path.as_str().rsplit_once('.') {
                    *winners.entry(extension.to_owned()).or_default() += 1;
                }
            }
            Err(error) => failures.push(format!("{}: {error}", record.name)),
        }

        if let Ok(present) = resolver.present_candidates(&vfs, &record.name) {
            if present.len() > 1 {
                ambiguous_names += 1;
                let mut spellings: Vec<&str> = present
                    .iter()
                    .filter_map(|path| path.as_str().rsplit_once('.').map(|(_, ext)| ext))
                    .collect();
                spellings.sort_unstable();
                *ambiguous.entry(spellings.join("+")).or_default() += 1;
            }
        }
    }
    let elapsed = started.elapsed();

    println!(
        "{}: resolved {resolved} of {} catalogued names in {:.2} s",
        catalogue.label(),
        catalogue.records().len(),
        elapsed.as_secs_f64()
    );
    println!("  by directory: {buckets:?}");
    println!("  winning extension: {winners:?}");
    println!("  {ambiguous_names} names have more than one candidate present: {ambiguous:?}");
    for line in failures.iter().take(40) {
        println!("  UNRESOLVED {line}");
    }
    if failures.len() > 40 {
        println!("  ... and {} more", failures.len() - 40);
    }

    assert!(
        failures.is_empty(),
        "{} of {} catalogued names did not resolve",
        failures.len(),
        catalogue.records().len()
    );
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_catalogued_byte_range_lands_on_the_w3d_chunk_its_kind_names() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let catalogue = open_catalogue(&root).unwrap_or_else(|error| panic!("{error}"));
    let (vfs, _) = mount(&root);
    let resolver = Resolver::new();

    let started = Instant::now();
    let mut bytes_read = 0u64;
    let mut checked = 0usize;
    let mut matched = 0usize;
    let mut payload_sized = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    let mut sums_agree = 0usize;
    let mut sums_disagree = 0usize;
    let mut files = 0usize;

    for record in catalogue.records() {
        if !record.name.to_ascii_lowercase().ends_with(".w3d") || record.entries.is_empty() {
            continue;
        }
        let Ok(path) = resolver.resolve(&vfs, &record.name) else {
            continue;
        };
        let file = match vfs.read_path(&path) {
            Ok(file) => file,
            Err(error) => {
                unreadable.push(format!("{}: {error}", record.name));
                continue;
            }
        };
        files += 1;
        bytes_read += file.len() as u64;

        let mut indexed = 0u64;
        for entry in &record.entries {
            checked += 1;
            indexed += u64::from(entry.size);

            let at = entry.offset as usize;
            let Some(header) = file.get(at..at + 8) else {
                mismatches.push(format!(
                    "{} :: {}: offset {} is past the end of a {}-byte file",
                    record.name,
                    entry.name,
                    entry.offset,
                    file.len()
                ));
                continue;
            };
            let chunk_type = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let payload = u32::from_le_bytes([header[4], header[5], header[6], header[7]])
                & !W3D_HAS_SUBCHUNKS;

            let type_ok = chunk_ids(entry.kind).contains(&chunk_type);
            if type_ok && entry.size == payload.saturating_add(W3D_HEADER_LEN) {
                matched += 1;
            } else {
                if type_ok && entry.size == payload {
                    payload_sized += 1;
                }
                mismatches.push(format!(
                    "{} :: {} ({}) at {}: chunk {chunk_type:#05x}, payload {payload}, entry size {}",
                    record.name, entry.name, entry.kind, entry.offset, entry.size
                ));
            }
        }

        if indexed <= file.len() as u64 {
            sums_agree += 1;
        } else {
            sums_disagree += 1;
        }
    }
    let elapsed = started.elapsed();

    println!(
        "{}: {matched} of {checked} catalogued byte ranges land on the chunk their kind names, \
         across {files} models",
        catalogue.label()
    );
    println!(
        "  read {:.1} MiB in {:.2} s ({:.1} MiB/s)",
        bytes_read as f64 / 1_048_576.0,
        elapsed.as_secs_f64(),
        bytes_read as f64 / 1_048_576.0 / elapsed.as_secs_f64().max(f64::MIN_POSITIVE)
    );
    println!(
        "  {sums_agree} models index no more bytes than they contain, {sums_disagree} index more"
    );
    if payload_sized > 0 {
        println!(
            "  {payload_sized} entries match the chunk type but size the payload without its \
             8-byte header"
        );
    }
    for line in mismatches.iter().take(40) {
        println!("  MISMATCH {line}");
    }
    if mismatches.len() > 40 {
        println!("  ... and {} more", mismatches.len() - 40);
    }
    for line in unreadable.iter().take(10) {
        println!("  UNREADABLE {line}");
    }

    assert!(
        unreadable.is_empty(),
        "{} models could not be read",
        unreadable.len()
    );
    assert!(
        mismatches.is_empty(),
        "{} of {checked} catalogued byte ranges do not land on the chunk their kind names",
        mismatches.len()
    );
    assert_eq!(
        sums_disagree, 0,
        "{sums_disagree} models index more bytes than they hold"
    );
}

#[test]
#[ignore = "needs two retail installations; set MINT_CORPUS and MINT_CORPUS_BASE"]
fn a_layered_install_resolves_its_catalogue_through_both_mounts() {
    let (Some(root), Some(base)) = (corpus_root(), std::env::var_os("MINT_CORPUS_BASE")) else {
        println!("SKIPPED: needs MINT_CORPUS and MINT_CORPUS_BASE, base first in mount order");
        return;
    };
    let base = PathBuf::from(base);

    let catalogue = open_catalogue(&root).unwrap_or_else(|error| panic!("{error}"));
    let mut vfs = Vfs::new();
    let mut mounted = 0usize;
    for path in archives(&base).into_iter().chain(archives(&root)) {
        if let Ok(archive) = BigArchive::open(&path) {
            vfs.mount(Arc::new(archive));
            mounted += 1;
        }
    }
    let resolver = Resolver::new();

    let mut resolved = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut ambiguous = 0usize;
    for record in catalogue.records() {
        match resolver.resolve(&vfs, &record.name) {
            Ok(_) => resolved += 1,
            Err(error) => failures.push(format!("{}: {error}", record.name)),
        }
        if resolver
            .present_candidates(&vfs, &record.name)
            .is_ok_and(|present| present.len() > 1)
        {
            ambiguous += 1;
        }
    }

    println!(
        "{} over {}: {mounted} archives, {} visible paths; resolved {resolved} of {} names, \
         {ambiguous} ambiguous",
        root.display(),
        base.display(),
        vfs.len(),
        catalogue.records().len()
    );
    for line in failures.iter().take(40) {
        println!("  UNRESOLVED {line}");
    }

    assert!(
        failures.is_empty(),
        "{} of {} names did not resolve through the layered mount",
        failures.len(),
        catalogue.records().len()
    );
}

fn chunk_ids(kind: AssetKind) -> &'static [u32] {
    match kind {
        AssetKind::Mesh => &[0x000],
        AssetKind::Hierarchy => &[0x100],
        AssetKind::Animation => &[0x200, 0x280],
        AssetKind::Particles => &[0x500],
        AssetKind::Hlod => &[0x700],
        AssetKind::BoundingBox => &[0x740],
        AssetKind::Texture | AssetKind::Shader | AssetKind::Unrecognised(_) => &[],
    }
}
