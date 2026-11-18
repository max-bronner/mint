use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn main() {
    let root = match std::env::args_os().nth(1) {
        Some(root) => PathBuf::from(root),
        None => {
            eprintln!("usage: sfe_scan <install dir>");
            return;
        }
    };

    for path in files_with_extension(&root, "big") {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if !lower.ends_with(".w3d") || !lower.contains("sfe_") {
                continue;
            }
            let Ok(bytes) = archive.read_entry(entry) else {
                continue;
            };
            let Ok(model) = Model::parse(&bytes) else {
                println!("{}: PARSE FAILED", entry.name);
                continue;
            };
            let has_anim = !model.animations.is_empty();
            let mut blend_entries = Vec::new();
            let mut frame_animated_textures = 0usize;
            for mesh in &model.meshes {
                for shader in mesh.shaders.iter().flatten() {
                    blend_entries.push((
                        shader.dest_blend,
                        shader.src_blend,
                        shader.depth_mask,
                        shader.alpha_test,
                    ));
                }
                for texture in &mesh.textures {
                    if texture.info.is_some_and(|info| info.frame_count > 1) {
                        frame_animated_textures += 1;
                    }
                }
            }
            println!(
                "{}: {} mesh(es), embedded animation: {has_anim}, flipbook textures: \
                 {frame_animated_textures}, shaders (dest,src,depth_mask,alpha_test): {:?}",
                entry.name,
                model.meshes.len(),
                blend_entries
            );
        }
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
