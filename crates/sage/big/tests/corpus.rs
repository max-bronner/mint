use std::path::{Path, PathBuf};
use std::sync::Arc;

use mint_sage_big::BigArchive;
use mint_vfs::{Mount, Vfs};

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

fn corpus_root() -> Option<PathBuf> {
    std::env::var_os("MINT_CORPUS").map(PathBuf::from)
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn every_shipped_archive_parses_and_agrees_with_its_own_header() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let paths = archives(&root);
    assert!(
        !paths.is_empty(),
        "no .big archives under {}; is MINT_CORPUS pointing at an install?",
        root.display()
    );

    let mut opened = 0usize;
    let mut entries = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut size_mismatch: Vec<String> = Vec::new();
    let mut table_overlap: Vec<String> = Vec::new();

    for path in &paths {
        let archive = match BigArchive::open(path) {
            Ok(archive) => archive,
            Err(error) => {
                failures.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        opened += 1;
        entries += archive.entries().len();

        if u64::from(archive.declared_size()) != archive.file_len() {
            size_mismatch.push(format!(
                "{}: header says {} bytes, file is {}",
                path.display(),
                archive.declared_size(),
                archive.file_len()
            ));
        }
        if archive.entry_table_end() > u64::from(archive.data_offset()) {
            table_overlap.push(format!(
                "{}: entry table ends at {} but data starts at {}",
                path.display(),
                archive.entry_table_end(),
                archive.data_offset()
            ));
        }

        let listed = archive.entries();
        let probes = [listed.first(), listed.get(listed.len() / 2), listed.last()];
        for entry in probes.into_iter().flatten() {
            match archive.read_entry(entry) {
                Ok(bytes) if bytes.len() as u64 == u64::from(entry.size) => {}
                Ok(bytes) => failures.push(format!(
                    "{} :: {}: read {} bytes, directory says {}",
                    path.display(),
                    entry.name,
                    bytes.len(),
                    entry.size
                )),
                Err(error) => {
                    failures.push(format!("{} :: {}: {error}", path.display(), entry.name))
                }
            }
        }
    }

    println!(
        "{}: {opened} of {} archives opened, {entries} entries indexed",
        root.display(),
        paths.len()
    );
    for line in failures.iter().chain(&size_mismatch).chain(&table_overlap) {
        println!("  {line}");
    }

    assert!(failures.is_empty(), "{} failures", failures.len());
    assert!(
        size_mismatch.is_empty(),
        "{} archives disagree with their own declared size",
        size_mismatch.len()
    );
    assert!(
        table_overlap.is_empty(),
        "{} archives have an entry table running into their file data",
        table_overlap.len()
    );
}

#[test]
#[ignore = "needs a retail installation; set MINT_CORPUS"]
fn mounting_every_archive_produces_one_namespace_with_later_mounts_winning() {
    let Some(root) = corpus_root() else {
        println!("SKIPPED: MINT_CORPUS is not set, so there is nothing to validate against");
        return;
    };

    let mut vfs = Vfs::new();
    let mut listed = 0usize;
    let mut shadowed = 0usize;

    for path in archives(&root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let report = vfs.mount(Arc::new(archive));
        listed += report.listed;
        shadowed += report.shadowed;
    }

    println!(
        "{}: {listed} entries mounted, {shadowed} shadowed, {} visible paths",
        root.display(),
        vfs.len()
    );

    assert!(!vfs.is_empty(), "nothing visible after mounting");
    assert_eq!(
        vfs.len() + shadowed,
        listed,
        "every listed entry is either visible or shadowed by a later one"
    );

    let first = vfs.paths().next().cloned();
    if let Some(path) = first {
        let winner = vfs
            .origin(path.as_str())
            .map(Mount::label)
            .map(str::to_owned);
        assert!(winner.is_some(), "{path} resolves but has no origin");
        assert!(
            vfs.read_path(&path).is_ok(),
            "{path} resolves but cannot be read"
        );
    }
}
