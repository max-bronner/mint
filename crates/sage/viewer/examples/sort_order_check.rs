use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_render::Blend;
use mint_sage_big::BigArchive;
use mint_sage_model::convert::convert_model;
use mint_sage_w3d::Model;

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: sort_order_check <install dir> [<install dir> ...]");
        return;
    }

    let mut total_meshes = 0usize;
    let mut non_opaque_meshes = 0usize;
    let mut non_opaque_with_explicit_sort_level = 0usize;
    let mut level_distribution: BTreeMap<i32, usize> = BTreeMap::new();
    let mut pri_gradient_distribution: BTreeMap<u8, usize> = BTreeMap::new();

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
                let Ok(converted) = convert_model(&model, model.hierarchy.as_ref()) else {
                    continue;
                };
                let parts_by_name: BTreeMap<&str, &mint_sage_model::Part> = converted
                    .parts
                    .iter()
                    .map(|part| (part.name.as_str(), part))
                    .collect();
                for mesh in &model.meshes {
                    total_meshes += 1;
                    let Some(part) = parts_by_name.get(mesh.name()) else {
                        continue;
                    };
                    if part.blend == Blend::Opaque {
                        continue;
                    }
                    non_opaque_meshes += 1;
                    let level = mesh.header.sort_level;
                    *level_distribution.entry(level).or_default() += 1;
                    if (1..=32).contains(&level) {
                        non_opaque_with_explicit_sort_level += 1;
                    }
                    if let Some(shader) = mesh.shaders.iter().flatten().next() {
                        *pri_gradient_distribution
                            .entry(shader.pri_gradient)
                            .or_default() += 1;
                    }
                }
            }
        }
    }

    println!("{total_meshes} meshes total, {non_opaque_meshes} non-opaque (blended or additive)");
    println!(
        "{non_opaque_with_explicit_sort_level} of {non_opaque_meshes} non-opaque meshes carry an \
         explicit sort_level (1..=32) - see mint_render::Instance::sort_level for what mint does \
         with it"
    );
    println!("sort_level distribution across non-opaque meshes:");
    for (level, count) in &level_distribution {
        println!("  {count:>8}  sort_level={level}");
    }
    println!("pri_gradient distribution across non-opaque meshes (0=DISABLE, 1=MODULATE default):");
    for (gradient, count) in &pri_gradient_distribution {
        println!("  {count:>8}  pri_gradient={gradient}");
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
