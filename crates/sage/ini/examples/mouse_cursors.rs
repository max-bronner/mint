use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_ini::mouse_cursor::{cursor_file, MouseCursorIndex};
use mint_sage_ini::{BlockTable, MemorySources, Reader, Sources};

const INI_ROOT: &str = "data/ini/";
const MAX_LISTED: usize = 20;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: mouse_cursors <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    for root in &roots {
        println!("== {} ==", root.display());
        let archives = big_archives(root);
        let sources = ini_sources(&archives);
        let tops = top_level_paths(&sources);
        let table = BlockTable::corpus_path();
        let reader = Reader::new(&sources);

        let mut index = MouseCursorIndex::new();
        for path in &tops {
            let Ok(document) = reader.read(path, &table) else {
                continue;
            };
            index.insert_document(&document);
        }
        println!("{} MouseCursor definition(s)", index.len());
        for name in ["Arrow", "Normal", "Move", "AttackMove", "Select", "Scroll"] {
            println!("  {name}: {:?}", index.get(name));
        }

        let cursors_dir = root.join("data").join("cursors");
        let on_disk = cursor_files_on_disk(&cursors_dir);
        println!("{} file(s) under {}", on_disk.len(), cursors_dir.display());

        let names: Vec<String> = index.names().map(str::to_owned).collect();
        let mut resolved = 0usize;
        let mut unresolved: Vec<String> = Vec::new();
        for name in names {
            let Some(def) = index.get(&name) else {
                continue;
            };
            let directions = def.directions.filter(|&d| d > 1).unwrap_or(1);
            let mut all_ok = def.texture.is_some();
            for direction in 0..directions {
                let file = cursor_file(def, direction);
                if !file.is_some_and(|f| on_disk.contains(&f.to_ascii_lowercase())) {
                    all_ok = false;
                }
            }
            if all_ok {
                resolved = resolved.saturating_add(1);
            } else if unresolved.len() < MAX_LISTED {
                unresolved.push(format!("{name}: texture={:?}", def.texture));
            }
        }
        println!(
            "{resolved} of {} definition(s) resolve every direction to a file on disk",
            index.len()
        );
        for line in &unresolved {
            println!("  unresolved: {line}");
        }
        println!();
    }

    ExitCode::SUCCESS
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

fn ini_sources(archives: &[PathBuf]) -> MemorySources {
    let mut sources = MemorySources::new();
    for path in archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            if !entry.path.as_str().starts_with(INI_ROOT) {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                continue;
            };
            sources.insert(entry.path.as_str(), bytes);
        }
    }
    sources
}

fn top_level_paths(sources: &MemorySources) -> Vec<String> {
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut included: BTreeSet<String> = BTreeSet::new();
    for path in &paths {
        let Some(bytes) = Sources::read(sources, path) else {
            continue;
        };
        for item in mint_sage_ini::lex::lex(&mint_sage_ini::lex::decode_latin1(&bytes)) {
            if let mint_sage_ini::lex::Lexed::Directive {
                directive: mint_sage_ini::lex::Directive::Include(target),
                ..
            } = item
            {
                included.insert(mint_sage_ini::resolve_include(path, target));
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !included.contains(path))
        .collect()
}

fn cursor_files_on_disk(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(listing) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in listing.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            out.insert(name.to_ascii_lowercase());
        }
    }
    out
}
