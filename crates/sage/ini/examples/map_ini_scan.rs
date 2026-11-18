use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{Block, BlockTable, Document, MacroTable, Node};

#[derive(Default)]
struct Kind {
    blocks: usize,
    files: BTreeSet<String>,
    members: BTreeMap<String, usize>,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: map_ini_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    let mut inis: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut globals: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase().replace('\\', "/");
                if lower.starts_with("data/ini/") && lower.ends_with(".ini") {
                    if let Ok(bytes) = archive.read_entry(entry) {
                        globals.insert(lower.clone(), bytes);
                    }
                }
                if lower.ends_with("/map.ini") {
                    if let Ok(bytes) = archive.read_entry(entry) {
                        inis.insert(lower, bytes);
                    }
                }
            }
        }
    }
    let table = BlockTable::corpus_path();
    let macros = MacroTable::new();
    let mut declared: BTreeSet<(String, String)> = BTreeSet::new();
    for (path, bytes) in &globals {
        let text = String::from_utf8_lossy(bytes);
        if let Ok(document) = Document::parse(path, &text, &table) {
            for block in document.roots() {
                if let Some(name) = block.name() {
                    declared.insert((block.keyword.clone(), name.to_ascii_lowercase()));
                }
            }
        }
    }
    let mut overrides: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut kinds: BTreeMap<String, Kind> = BTreeMap::new();
    let mut defines = 0usize;
    let mut define_files = 0usize;
    let mut failed = 0usize;
    let mut strict_errors = 0usize;
    for (path, bytes) in &inis {
        let text = String::from_utf8_lossy(bytes);
        let document = match Document::parse(path, &text, &table) {
            Ok(document) => document,
            Err(error) => {
                failed = failed.saturating_add(1);
                println!("FAILED {path}: {error}");
                continue;
            }
        };
        if !document.defines().is_empty() {
            define_files = define_files.saturating_add(1);
            defines = defines.saturating_add(document.defines().len());
        }
        let report = mint_sage_ini::strict::check(
            &document,
            &macros,
            mint_sage_ini::tables::field_table::Titles::BOTH,
        );
        for error in &report.errors {
            strict_errors = strict_errors.saturating_add(1);
            println!("strict {error}");
        }
        let mut seen_here: BTreeSet<&str> = BTreeSet::new();
        for block in document.roots() {
            if let Some(name) = block.name() {
                let known = declared.contains(&(block.keyword.clone(), name.to_ascii_lowercase()));
                let slot = overrides.entry(block.keyword.clone()).or_default();
                if known {
                    slot.0 = slot.0.saturating_add(1);
                } else {
                    slot.1 = slot.1.saturating_add(1);
                    if block.keyword != "FXParticleSystem" {
                        println!("new {} {name} in {path}", block.keyword);
                    }
                }
            }
            let kind = kinds.entry(block.keyword.clone()).or_default();
            kind.blocks = kind.blocks.saturating_add(1);
            kind.files.insert(path.clone());
            tally(block, "", &mut kind.members, 0);
            if seen_here.insert(block.keyword.as_str()) && block.keyword == "Object" {
                println!("objects {path}");
            }
        }
    }
    println!(
        "map.ini files: {} ({failed} failed to parse, {strict_errors} strict error(s) with no \
         global #defines); #define lines {defines} in {define_files} file(s)",
        inis.len()
    );
    for (keyword, (known, new)) in &overrides {
        println!("NAMED {keyword}: {known} override(s) of a global definition, {new} new");
    }
    for (keyword, kind) in &kinds {
        println!(
            "ROOT {keyword}: {} block(s) in {} file(s)",
            kind.blocks,
            kind.files.len()
        );
        for (member, count) in &kind.members {
            println!("    {count:5} {member}");
        }
    }
    ExitCode::SUCCESS
}

fn tally(block: &Block, prefix: &str, members: &mut BTreeMap<String, usize>, depth: usize) {
    if depth > 4 {
        return;
    }
    for child in &block.children {
        let (key, nested) = match child {
            Node::Field(field) => (format!("{prefix}{}", field.keyword), None),
            Node::Block(inner) => (format!("{prefix}[{}]", inner.keyword), Some(inner)),
            Node::Script(_) => (format!("{prefix}<script>"), None),
        };
        let slot = members.entry(key.clone()).or_insert(0);
        *slot = slot.saturating_add(1);
        if let Some(inner) = nested {
            tally(inner, &format!("{key}/"), members, depth.saturating_add(1));
        }
    }
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
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
                    if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("big"))
                    {
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
