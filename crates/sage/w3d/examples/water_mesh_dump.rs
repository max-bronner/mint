use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::Model;

const SAMPLES: usize = 4;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: water_mesh_dump <name filter> <install dir> [...]");
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
                dumps.insert(lower, describe(&model));
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

fn describe(model: &Model<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    for mesh in &model.meshes {
        lines.push(format!(
            "  mesh {:?}  {} vertices, {} triangles, {} normals, {} pass(es)",
            mesh.header.mesh_name,
            mesh.positions.len(),
            mesh.triangles.len(),
            mesh.normals.len(),
            mesh.passes.len(),
        ));
        lines.push(range_line("    POSITION ", mesh.positions.iter().map(v3)));
        lines.push(range_line("    NORMAL   ", mesh.normals.iter().map(v3)));
        for (index, pass) in mesh.passes.iter().enumerate() {
            let mut sets: Vec<Vec<[f32; 2]>> = Vec::new();
            if !pass.texcoords.is_empty() {
                sets.push(
                    pass.texcoords
                        .iter()
                        .flatten()
                        .map(|uv| [uv.u, uv.v])
                        .collect(),
                );
            }
            for stage in &pass.texture_stages {
                if !stage.texcoords.is_empty() {
                    sets.push(
                        stage
                            .texcoords
                            .iter()
                            .flatten()
                            .map(|uv| [uv.u, uv.v])
                            .collect(),
                    );
                }
            }
            for (set, uvs) in sets.iter().enumerate() {
                lines.push(range_line(
                    &format!("    pass {index} TEXCOORD{set}"),
                    uvs.iter().map(|uv| [uv[0], uv[1], 0.0]),
                ));
                for uv in uvs.iter().take(SAMPLES) {
                    lines.push(format!("      {:>12.4} {:>12.4}", uv[0], uv[1]));
                }
            }
        }
        for (index, (p, n)) in mesh
            .positions
            .iter()
            .flatten()
            .zip(mesh.normals.iter().flatten())
            .take(SAMPLES)
            .enumerate()
        {
            lines.push(format!(
                "    v{index}  pos {:>10.3} {:>10.3} {:>10.3}   normal {:>10.4} {:>10.4} {:>10.4}  |n| {:.4}",
                p.x,
                p.y,
                p.z,
                n.x,
                n.y,
                n.z,
                (n.x * n.x + n.y * n.y + n.z * n.z).sqrt(),
            ));
        }
    }
    lines
}

fn v3(v: Result<mint_sage_w3d::Vec3, mint_sage_w3d::Error>) -> [f32; 3] {
    v.map_or([0.0; 3], |v| [v.x, v.y, v.z])
}

fn range_line(label: &str, values: impl Iterator<Item = [f32; 3]>) -> String {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut count = 0usize;
    for value in values {
        for axis in 0..3 {
            let Some(v) = value.get(axis) else { continue };
            if let Some(slot) = min.get_mut(axis) {
                *slot = slot.min(*v);
            }
            if let Some(slot) = max.get_mut(axis) {
                *slot = slot.max(*v);
            }
        }
        count = count.saturating_add(1);
    }
    if count == 0 {
        return format!("{label} (empty)");
    }
    format!(
        "{label} {count:>6} values, x {:>10.4}..{:<10.4} y {:>10.4}..{:<10.4} z {:>10.4}..{:<10.4}",
        min[0], max[0], min[1], max[1], min[2], max[2]
    )
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
