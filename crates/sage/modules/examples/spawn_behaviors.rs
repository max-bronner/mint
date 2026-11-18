use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, MemorySources, Reader};
use mint_sage_modules::{
    is_slaved, spawn_behavior_from_fields, BlockFields, TemplateIndex, TemplateRecords, UpgradeBits,
};
use mint_sim::Frame;

const INI_ROOT: &str = "data/ini/";

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: spawn_behaviors <install dir> [<install dir> ...]  (ROTWK last)");
        return ExitCode::FAILURE;
    }
    let mut sources = MemorySources::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.path.as_str().starts_with(INI_ROOT) {
                    continue;
                }
                if let Ok(bytes) = archive.read_entry(entry) {
                    sources.insert(entry.path.as_str(), bytes);
                }
            }
        }
    }
    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents: Vec<_> = paths
        .iter()
        .filter_map(|path| reader.read(path, &table).ok())
        .collect();
    let _ = mint_sage_ini::expand_macros(&mut documents);
    let mut index = TemplateIndex::new();
    for document in &documents {
        index.insert_document(document);
    }
    let bits = UpgradeBits::from_index(index.upgrades());
    let (records, refused) = TemplateRecords::build_collecting(&index);
    for error in &refused {
        println!("refused: {error}");
    }

    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let (mut built, mut refused, mut inherited) = (0usize, 0usize, 0usize);
    let names: Vec<String> = index.names().map(str::to_owned).collect();
    for name in &names {
        for declaration in index.declared_modules(name) {
            if declaration.name != "SpawnBehavior" {
                continue;
            }
            if !seen.insert(std::ptr::from_ref(declaration.block) as usize) {
                inherited += 1;
                continue;
            }
            let fields = BlockFields(declaration.block, index.macros(), &bits);
            match spawn_behavior_from_fields(&fields, Frame::ZERO) {
                Ok(module) => {
                    built += 1;
                    let spawns: Vec<String> = module
                        .templates()
                        .map(|spawn| match records.get(spawn) {
                            Some(record) if is_slaved(record) => format!("{spawn} (slaved)"),
                            Some(_) => spawn.to_owned(),
                            None => format!("{spawn} (MISSING)"),
                        })
                        .collect();
                    let parsed_only: Vec<String> = module
                        .parsed_only()
                        .into_iter()
                        .map(|(field, value)| format!("{field}={value}"))
                        .collect();
                    println!(
                        "{name}: SpawnNumber {} ReplaceDelay {} frames, spawns [{}]; \
                         TriggeredBy {}; parsed only: {}",
                        module.spawn_number(),
                        module.replace_delay().get(),
                        spawns.join(", "),
                        declaration
                            .block
                            .field("TriggeredBy")
                            .map_or("-", |field| field.args.first().map_or("-", String::as_str)),
                        if parsed_only.is_empty() {
                            String::from("-")
                        } else {
                            parsed_only.join(" ")
                        }
                    );
                }
                Err(err) => {
                    refused += 1;
                    println!("{name}: REFUSED {err}");
                }
            }
        }
    }
    println!(
        "SpawnBehavior: {built} declaration(s) built, {refused} refused, {inherited} inherited \
         by a ChildObject"
    );
    if refused == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
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
