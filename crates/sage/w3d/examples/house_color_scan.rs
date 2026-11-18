use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

fn bare_mesh_name(name: &str) -> &str {
    name.split_once('.').map_or(name, |(_, rest)| rest)
}

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: house_color_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut meshes_total = 0usize;
    let mut models_total = 0usize;
    let mut by_name: BTreeSet<String> = BTreeSet::new();
    let mut by_name_files: BTreeSet<String> = BTreeSet::new();
    let mut by_name_textures: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_name_detail_func: BTreeMap<u8, usize> = BTreeMap::new();
    let mut zhc_textures: BTreeMap<String, usize> = BTreeMap::new();
    let mut hc_textures: BTreeMap<String, usize> = BTreeMap::new();
    let mut modalphaadd_meshes = 0usize;
    let mut modalphaadd_with_two_stages = 0usize;
    let mut modalphaadd_examples: Vec<String> = Vec::new();
    let mut modalphaadd_textures: BTreeMap<String, usize> = BTreeMap::new();

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
    println!("{} .w3d file(s) collected", jobs.len());

    for (file, bytes) in &jobs {
        let Ok(model) = Model::parse(bytes) else {
            continue;
        };
        models_total += 1;
        for mesh in &model.meshes {
            meshes_total += 1;
            let bare = bare_mesh_name(mesh.name());
            let texture = mesh.texture_name().unwrap_or_default().to_owned();

            if bare.len() >= 10
                && bare
                    .get(..10)
                    .is_some_and(|p| p.eq_ignore_ascii_case("HOUSECOLOR"))
            {
                by_name.insert(bare.to_ascii_uppercase());
                by_name_files.insert(file.to_ascii_lowercase());
                *by_name_textures
                    .entry(texture.to_ascii_uppercase())
                    .or_default() += 1;
                let func = mesh
                    .shaders
                    .iter()
                    .flatten()
                    .next()
                    .map_or(255, |s| s.detail_color_func);
                *by_name_detail_func.entry(func).or_default() += 1;
            }

            for name in [mesh.texture_name(), mesh.stage2_texture_name()]
                .into_iter()
                .flatten()
            {
                let upper = name.to_ascii_uppercase();
                if upper.starts_with("ZHC") {
                    *zhc_textures.entry(upper.clone()).or_default() += 1;
                }
                if upper.starts_with("HC_") {
                    *hc_textures.entry(upper).or_default() += 1;
                }
            }

            let Some(first) = mesh.shaders.iter().flatten().next() else {
                println!(
                    "TEXFUNC	{}	none	{file}	{}",
                    texture.to_ascii_uppercase(),
                    mesh.name()
                );
                continue;
            };
            println!(
                "TEXFUNC	{}	{}	{file}	{}",
                texture.to_ascii_uppercase(),
                first.detail_color_func,
                mesh.name()
            );
            if first.detail_color_func == 12 {
                modalphaadd_meshes += 1;
                *modalphaadd_textures
                    .entry(texture.to_ascii_uppercase())
                    .or_default() += 1;
                if mesh.stage2_texture_name().is_some() {
                    modalphaadd_with_two_stages += 1;
                }
                if modalphaadd_examples.len() < 4000 {
                    modalphaadd_examples.push(format!(
                        "{file} :: {} texture={texture:?} stage2={:?}",
                        mesh.name(),
                        mesh.stage2_texture_name()
                    ));
                }
            }
        }
    }

    println!("{models_total} model(s) parsed, {meshes_total} mesh(es)\n");

    println!(
        "Marker 1 - mesh name begins HOUSECOLOR (W3DAssetManager.cpp:929): \
         {} distinct name(s) in {} file(s)",
        by_name.len(),
        by_name_files.len()
    );
    for name in by_name.iter().take(40) {
        println!("    {name}");
    }
    println!("  their stage-0 textures:");
    for (texture, count) in by_name_textures.iter().take(40) {
        println!("    {count:>5}  {texture}");
    }
    println!("  their detail_color_func (255 = no SHADERS entry): {by_name_detail_func:?}");

    println!(
        "\nMarker 2 - texture name begins ZHC (W3DAssetManager.cpp:941): {} distinct",
        zhc_textures.len()
    );
    for (texture, count) in zhc_textures.iter().take(30) {
        println!("    {count:>5}  {texture}");
    }

    println!(
        "\nMarker 3a - a texture named HC_* referenced directly by a mesh: {} distinct",
        hc_textures.len()
    );
    for (texture, count) in hc_textures.iter().take(30) {
        println!("    {count:>5}  {texture}");
    }
    println!(
        "Marker 3b - detail_color_func = 12 (ModAlphaAdd): {modalphaadd_meshes} mesh(es), \
         {modalphaadd_with_two_stages} of them with a second texture stage in the file"
    );
    for line in &modalphaadd_examples {
        println!("    {line}");
    }
    println!(
        "
ModAlphaAdd stage-0 textures, distinct:"
    );
    for (texture, count) in &modalphaadd_textures {
        println!("    {count:>5}  {texture}");
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
