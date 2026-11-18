use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::{MeshHeader3, Model};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: skybox_model_dump <name filter> <install dir> [...]");
        return;
    }
    let filter = args.remove(0).to_ascii_lowercase();
    let roots: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();

    let mut found: BTreeMap<String, String> = BTreeMap::new();

    for root in &roots {
        for path in big_archives(root) {
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
                let mut out = String::new();
                out.push_str(&format!(
                    "{} ({} bytes, {} mesh(es), {} hlod(s))\n",
                    entry.name,
                    bytes.len(),
                    model.meshes.len(),
                    model.hlods.len()
                ));
                for mesh in &model.meshes {
                    let h: &MeshHeader3 = &mesh.header;
                    let two_sided = h.attributes & 0x0000_1000 != 0;
                    let hidden = h.attributes & 0x0000_2000 != 0;
                    let names: Vec<&str> = mesh.textures.iter().map(|t| t.name.as_str()).collect();
                    out.push_str(&format!(
                        "  {:<18} {:>6} vert {:>6} tri  bbox ({:.0},{:.0},{:.0})..({:.0},{:.0},{:.0})  \
                         radius {:.0}{}{}  textures {:?}\n",
                        h.mesh_name,
                        h.num_vertices,
                        h.num_tris,
                        h.bound_box_min.x,
                        h.bound_box_min.y,
                        h.bound_box_min.z,
                        h.bound_box_max.x,
                        h.bound_box_max.y,
                        h.bound_box_max.z,
                        h.bound_sphere_radius,
                        if two_sided { " two-sided" } else { "" },
                        if hidden { " HIDDEN" } else { "" },
                        names
                    ));
                    for material in &mesh.shader_materials {
                        out.push_str(&format!(
                            "      shader material {:?}, {} property(ies)\n",
                            material.shader_name,
                            material.properties.len()
                        ));
                        for property in &material.properties {
                            out.push_str(&format!(
                                "        {property:?}
"
                            ));
                        }
                    }
                }
                found.insert(entry.name.clone(), out);
            }
        }
    }

    println!("{} file(s) matching {filter:?}", found.len());
    for text in found.values() {
        print!("{text}");
    }
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
