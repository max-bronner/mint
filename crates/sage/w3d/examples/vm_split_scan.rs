use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::{IndexTable, Model};

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let mut jobs: Vec<(String, Vec<u8>)> = Vec::new();
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
                jobs.push((
                    format!("{}::{}", path.display(), entry.name),
                    bytes.to_vec(),
                ));
            }
        }
    }
    println!("{} .w3d file(s) collected", jobs.len());

    let mut mesh_count = 0usize;
    let mut file_set = std::collections::BTreeSet::new();
    for (file, bytes) in &jobs {
        let Ok(model) = Model::parse(bytes) else {
            continue;
        };
        for mesh in &model.meshes {
            let Some(pass) = mesh.passes.first() else {
                continue;
            };
            let Some(IndexTable::PerElement(ids)) = &pass.vertex_material_ids else {
                continue;
            };
            let mut distinct = std::collections::BTreeSet::new();
            for id in ids.iter().flatten() {
                if id != u32::MAX {
                    distinct.insert(id);
                }
            }
            if distinct.len() > 1 {
                mesh_count += 1;
                file_set.insert(file.clone());
            }
        }
    }
    println!("{mesh_count} mesh(es) in {} file(s)", file_set.len());
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
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(want))
            {
                found.push(path);
            }
        }
    }
    found
}
