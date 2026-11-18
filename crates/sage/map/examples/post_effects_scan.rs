use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: post_effects_scan <install dir> [...]");
        return ExitCode::FAILURE;
    }
    let mut maps = 0u64;
    let mut images: BTreeMap<String, u64> = BTreeMap::new();
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !lower.ends_with(".map") {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&raw) else {
                    println!("unparsed {lower}");
                    continue;
                };
                maps += 1;
                if map.global_lighting.unnamed.bloom_enabled != 0 {
                    println!("{lower}	bloom");
                }
                for effect in &map.post_effects {
                    println!(
                        "{lower}	{}	{}	{}",
                        effect.name, effect.blend_factor, effect.lookup_image
                    );
                    *images
                        .entry(effect.lookup_image.to_ascii_lowercase())
                        .or_default() += 1;
                }
            }
        }
    }
    println!("{maps} maps; lookup images: {images:?}");
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
