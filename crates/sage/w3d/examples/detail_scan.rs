use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: detail_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut color_func_histogram: BTreeMap<u8, usize> = BTreeMap::new();
    let mut extension_blend: BTreeMap<u8, (usize, usize, Vec<(u8, u8)>)> = BTreeMap::new();
    let mut extension_alpha_func: BTreeMap<u8, BTreeMap<u8, usize>> = BTreeMap::new();
    let mut extension_has_two_stages: BTreeMap<u8, (usize, usize)> = BTreeMap::new();
    let mut non_additive_alpha_test: BTreeMap<(u8, bool), usize> = BTreeMap::new();

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
                jobs.push((entry.name.to_string(), bytes.to_vec()));
            }
        }
        for path in files_with_extension(root, "w3d") {
            if let Ok(bytes) = std::fs::read(&path) {
                jobs.push((path.display().to_string(), bytes));
            }
        }
    }

    for (name, bytes) in &jobs {
        let Ok(model) = Model::parse(bytes) else {
            continue;
        };
        for mesh in &model.meshes {
            for shader in mesh.shaders.iter().flatten() {
                *color_func_histogram
                    .entry(shader.detail_color_func)
                    .or_default() += 1;
            }

            let Some(first_shader) = mesh.shaders.iter().flatten().next() else {
                continue;
            };
            let color_func = first_shader.detail_color_func;
            if color_func <= 8 {
                continue;
            }

            let has_two_stages = mesh.stage2_texture_name().is_some();
            let slot = extension_has_two_stages.entry(color_func).or_insert((0, 0));
            if has_two_stages {
                slot.0 += 1;
            } else {
                slot.1 += 1;
            }

            if !has_two_stages {
                continue;
            }

            let entry_slot = extension_blend
                .entry(color_func)
                .or_insert((0, 0, Vec::new()));
            if first_shader.src_blend == 1 && first_shader.dest_blend == 1 {
                entry_slot.0 += 1;
            } else {
                entry_slot.1 += 1;
                if entry_slot.2.len() < 10 {
                    entry_slot
                        .2
                        .push((first_shader.src_blend, first_shader.dest_blend));
                }
            }

            *extension_alpha_func
                .entry(color_func)
                .or_default()
                .entry(first_shader.detail_alpha_func)
                .or_default() += 1;

            if !(first_shader.src_blend == 1 && first_shader.dest_blend == 1) {
                let key = (color_func, first_shader.alpha_test != 0);
                let count = non_additive_alpha_test.entry(key).or_insert(0usize);
                *count += 1;
                if *count <= 8 {
                    println!(
                        "NON-ADDITIVE two-stage color_func={color_func}: {name} :: mesh \
                         {:?} src={} dest={} alpha_test={} stage2={:?}",
                        mesh.name(),
                        first_shader.src_blend,
                        first_shader.dest_blend,
                        first_shader.alpha_test,
                        mesh.stage2_texture_name(),
                    );
                }
            }

            let file_lower = name.to_ascii_lowercase();
            let mesh_lower = mesh.name().to_ascii_lowercase();
            if file_lower.contains("sfe_menucredits") || file_lower.contains("sfe_menubkgrnd") {
                println!(
                    "{name} :: mesh {mesh_lower:?} detail_color_func={} detail_alpha_func={} \
                     post_detail_color_func={} post_detail_alpha_func={} src_blend={} \
                     dest_blend={} alpha_test={} stage2_texture={:?}",
                    first_shader.detail_color_func,
                    first_shader.detail_alpha_func,
                    first_shader.post_detail_color_func,
                    first_shader.post_detail_alpha_func,
                    first_shader.src_blend,
                    first_shader.dest_blend,
                    first_shader.alpha_test,
                    mesh.stage2_texture_name(),
                );
            }
        }
    }

    println!("\ndetail_color_func histogram, over every SHADERS entry:");
    for (value, count) in &color_func_histogram {
        println!("  {count:>8}  {value}");
    }

    println!(
        "\nFor color_func values above 8 (believed undocumented when this tool was written; \
         GeneralsMD/WW3D2/shader.h documents through 12 - see docs/research/w3d-shading.md \
         section 3.2): does the mesh's first SHADERS entry blend additively \
         (src_blend=1, dest_blend=1)?"
    );
    for (value, (additive, other, examples)) in &extension_blend {
        println!(
            "  color_func={value}: {additive} additive, {other} not (examples of non-additive \
             (src,dest): {examples:?})"
        );
    }

    println!("\nFor the same values, detail_alpha_func distribution:");
    for (value, by_alpha) in &extension_alpha_func {
        println!("  color_func={value}: {by_alpha:?}");
    }

    println!("\nFor the same values, does the mesh actually carry a second texture stage?");
    for (value, (yes, no)) in &extension_has_two_stages {
        println!("  color_func={value}: {yes} with a second stage, {no} without");
    }

    println!("\nNon-additive, two-stage survivors by (color_func, alpha_test != 0):");
    for ((value, alpha_test), count) in &non_additive_alpha_test {
        println!("  color_func={value} alpha_test={alpha_test}: {count}");
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
