use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_ini::{BlockTable, Document, MemorySources, Reader};

const INI_ROOT: &str = "data/ini/";

pub(crate) fn load(roots: &[PathBuf]) -> Result<Vec<Document>, String> {
    let mut sources = MemorySources::new();
    for root in roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
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
    }
    if sources.is_empty() {
        return Err(format!(
            "no {INI_ROOT} entries found under any given install - wrong path?"
        ));
    }

    let table = BlockTable::corpus_path();
    let reader = Reader::new(&sources);
    let paths: Vec<String> = sources.paths().map(str::to_owned).collect();
    let mut documents = Vec::new();
    for path in &paths {
        if let Ok(doc) = reader.read(path, &table) {
            documents.push(doc);
        }
    }
    mint_sage_ini::expand_macros(&mut documents);
    Ok(documents)
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
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
                    {
                        found.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    found
}
