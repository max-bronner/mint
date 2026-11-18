use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: water_material_dump <name filter> <install dir> [...]");
        return;
    }
    let filter = args.remove(0).to_ascii_lowercase();
    let roots: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();

    let mut dumps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for root in &roots {
        for path in files_with_extension(root, "big") {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !lower.ends_with(".w3d") || !lower.contains(&filter) {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(model) = Model::parse(&bytes) else {
                    continue;
                };
                let mut lines = Vec::new();
                for mesh in &model.meshes {
                    for material in &mesh.shader_materials {
                        lines.push(format!("  shader {:?}", material.shader_name));
                        for property in &material.properties {
                            lines.push(format!("    {:<28} {:?}", property.name, property.value));
                        }
                    }
                }
                dumps.insert(lower, lines);
            }
        }
    }

    for (name, lines) in &dumps {
        println!("{name}");
        for line in lines {
            println!("{line}");
        }
    }
    println!("{} file(s)", dumps.len());
}

fn files_with_extension(root: &Path, want: &str) -> Vec<PathBuf> {
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
                    let matches = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case(want));
                    if matches {
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
