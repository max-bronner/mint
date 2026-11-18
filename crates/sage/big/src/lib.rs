#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod archive;
#[cfg(test)]
mod synth;

pub use archive::{BigArchive, BigEntry, BigFormat};

use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is {len} bytes, too short for a 16-byte BIG header")]
    HeaderTruncated { path: PathBuf, len: u64 },

    #[error("{path} is not a BIG archive: magic is {magic:02x?}, not \"BIG4\" or \"BIGF\"")]
    NotArchive { path: PathBuf, magic: [u8; 4] },

    #[error(
        "{path} claims {count} entries, which needs at least {needed} bytes \
         but only {available} follow the header"
    )]
    EntryCountTooLarge {
        path: PathBuf,
        count: u32,
        needed: u64,
        available: u64,
    },

    #[error("{path}: entry table ends early at byte {offset}, in entry {entry} of {count}")]
    EntryTableTruncated {
        path: PathBuf,
        entry: u32,
        count: u32,
        offset: u64,
    },

    #[error("{path}: entry {entry} has an unterminated name at byte {offset} (limit {limit})")]
    UnterminatedName {
        path: PathBuf,
        entry: u32,
        offset: u64,
        limit: usize,
    },

    #[error("{path}: entry {entry} has an empty name")]
    EmptyName { path: PathBuf, entry: u32 },

    #[error("{path}: entry \"{name}\" spans {offset}..{end} but the file is {file_len} bytes")]
    EntryOutOfBounds {
        path: PathBuf,
        name: String,
        offset: u32,
        end: u64,
        file_len: u64,
    },

    #[error("{path} contains no entry named {name}")]
    NotFound { path: PathBuf, name: String },
}

#[must_use]
pub fn archives_under(root: &Path) -> Vec<PathBuf> {
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
    found.sort_by_cached_key(|path| std::cmp::Reverse(retail_order_key(root, path)));
    found
}

fn retail_order_key(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut key = String::new();
    for (index, part) in relative.components().enumerate() {
        if index > 0 {
            key.push('\\');
        }
        key.push_str(&part.as_os_str().to_string_lossy().to_ascii_lowercase());
    }
    key
}

#[cfg(test)]
mod discovery_tests {
    use super::archives_under;

    #[test]
    fn finds_big_files_at_the_top_level_and_nested_and_sorts_them() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "mint-big-discovery-test-{}-{nanos}",
            std::process::id()
        ));
        let nested = dir.join("Data").join("English");
        std::fs::create_dir_all(&nested).expect("create temp tree");
        for (rel, name) in [(&dir, "z.big"), (&dir, "a.big"), (&nested, "m.big")] {
            std::fs::write(rel.join(name), b"not a real archive").expect("write stub file");
        }
        std::fs::write(dir.join("readme.txt"), b"ignored").expect("write non-.big file");

        let found = archives_under(&dir);
        let expected = vec![dir.join("z.big"), nested.join("m.big"), dir.join("a.big")];
        assert_eq!(found, expected);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn underscored_archives_take_priority_in_retail_order() {
        let root = std::path::Path::new("install");
        let mut keys: Vec<String> = ["INI.big", "_test.big", "___test-wave.big", "data2.big"]
            .iter()
            .map(|name| super::retail_order_key(root, &root.join(name)))
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            ["___test-wave.big", "_test.big", "data2.big", "ini.big"]
        );
    }

    #[test]
    fn a_root_that_does_not_exist_yields_no_archives_rather_than_panicking() {
        let missing = std::env::temp_dir().join("mint-big-discovery-test-does-not-exist");
        assert!(archives_under(&missing).is_empty());
    }
}
