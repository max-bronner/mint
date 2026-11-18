use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: property_names <install dir> [<install dir> ...]");
        return;
    }

    let mut names: BTreeMap<String, usize> = BTreeMap::new();
    for root in &roots {
        for path in files_with_extension(root, "big") {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                    continue;
                }
                let Ok(bytes) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(model) = Model::parse(&bytes) else {
                    continue;
                };
                for mesh in &model.meshes {
                    for material in &mesh.shader_materials {
                        for property in &material.properties {
                            *names.entry(property.name.clone()).or_default() += 1;
                            if matches!(
                                property.name.as_str(),
                                "WaveAngle" | "WaveSpeed" | "BumpRotation" | "BumpSpeed"
                            ) && names.get(&property.name).copied().unwrap_or(0) <= 1
                            {
                                println!(
                                    "  {} :: {} shader {:?} property {:?} = {:?}",
                                    entry.name,
                                    mesh.name(),
                                    material.shader_name,
                                    property.name,
                                    property.value
                                );
                            }
                            if property.name.starts_with("TextureAnimation") {
                                println!(
                                    "  {} :: {} shader {:?} property {:?} = {:?}",
                                    entry.name,
                                    mesh.name(),
                                    material.shader_name,
                                    property.name,
                                    property.value
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    for (name, count) in &names {
        println!("{count:>8}  {name}");
    }
    println!("{} distinct property names", names.len());
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
