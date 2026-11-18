use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: gradient_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut pri_gradient: BTreeMap<u8, usize> = BTreeMap::new();
    let mut shader_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut property_names: BTreeMap<String, usize> = BTreeMap::new();
    let mut bright_hits: Vec<(String, String)> = Vec::new();

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
                    for shader in mesh.shaders.iter().flatten() {
                        *pri_gradient.entry(shader.pri_gradient).or_default() += 1;
                    }
                    for material in &mesh.shader_materials {
                        *shader_names
                            .entry(material.shader_name.clone())
                            .or_default() += 1;
                        for property in &material.properties {
                            *property_names.entry(property.name.clone()).or_default() += 1;
                            let lower = property.name.to_ascii_lowercase();
                            if lower.contains("bright")
                                || lower.contains("glow")
                                || lower.contains("hdr")
                            {
                                bright_hits.push((entry.name.clone(), property.name.clone()));
                            }
                        }
                    }
                }
            }
        }
    }

    println!("pri_gradient histogram over every SHADERS entry:");
    for (value, count) in &pri_gradient {
        println!("  {value:>3}  {count}");
    }

    println!("\nshader_name histogram (SHADER_MATERIAL.shader_name):");
    for (name, count) in &shader_names {
        println!("  {count:>6}  {name}");
    }

    println!(
        "\nSHADER_MATERIAL property names containing 'bright'/'glow'/'hdr' \
         (case-insensitive): {} hit(s)",
        bright_hits.len()
    );
    for (mesh, prop) in bright_hits.iter().take(20) {
        println!("  {mesh}: {prop}");
    }

    println!(
        "\nAll distinct SHADER_MATERIAL property names ({}):",
        property_names.len()
    );
    for (name, count) in &property_names {
        println!("  {count:>6}  {name}");
    }
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
