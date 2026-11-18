use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_render::{Mat4, Vec3};
use mint_sage_big::BigArchive;
use mint_sage_model::convert::pivot_world_transforms;
use mint_sage_w3d::{Mesh, Model};
use mint_vfs::Vfs;

const TOLERANCE: f32 = 0.01;

const BANDS: [f32; 3] = [0.25, 1.0, 4.0];

const NAMES: usize = 12;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: skin_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    println!("mounted {} path(s)", vfs.len());

    let started = Instant::now();
    let models: Vec<String> = vfs
        .paths()
        .filter(|path| path.as_str().ends_with(".w3d"))
        .map(|path| path.as_str().to_owned())
        .collect();

    let mut skeletons: BTreeMap<String, String> = BTreeMap::new();
    for path in &models {
        let Ok(bytes) = vfs.read(path) else { continue };
        let Ok(parsed) = Model::parse(&bytes) else {
            continue;
        };
        if let Some(hierarchy) = &parsed.hierarchy {
            skeletons
                .entry(hierarchy.header.name.to_ascii_uppercase())
                .or_insert_with(|| path.clone());
        }
    }
    println!("{} hierarchy name(s) supplied by a file", skeletons.len());

    let mut scan = Scan::default();
    for path in &models {
        let Ok(bytes) = vfs.read(path) else { continue };
        let Ok(parsed) = Model::parse(&bytes) else {
            continue;
        };
        if !parsed.meshes.iter().any(|mesh| mesh.header.is_skin()) {
            continue;
        }
        let own = parsed.hierarchy.clone();
        let external = match (&own, parsed.external_hierarchy()) {
            (Some(_), _) => None,
            (None, Some(name)) => skeletons
                .get(&name.to_ascii_uppercase())
                .and_then(|file| vfs.read(file).ok())
                .and_then(|bytes| Model::parse(&bytes).ok().and_then(|m| m.hierarchy)),
            (None, None) => None,
        };
        let Some(hierarchy) = own.or(external) else {
            scan.no_skeleton = scan.no_skeleton.saturating_add(1);
            continue;
        };
        let bones = pivot_world_transforms(&hierarchy);
        for mesh in &parsed.meshes {
            if mesh.header.is_skin() {
                scan.visit(path, mesh, &bones);
            }
        }
    }

    scan.report(started.elapsed().as_secs_f64());
    if scan.disagreeing_vertices > scan.agreeing_vertices / 100 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Default)]
struct Scan {
    skinned_meshes: usize,
    no_skeleton: usize,
    with_second_array: usize,
    with_two_bone: usize,
    orphan_two_bone_meshes: usize,
    vertices: usize,
    two_bone_vertices: usize,
    agreeing_vertices: usize,
    disagreeing_vertices: usize,
    banded: [usize; BANDS.len()],
    worst_new: f32,
    worst_old: f32,
    total_old: f64,
    disagreeing: Vec<String>,
    worst_models: Vec<(f32, String)>,
}

impl Scan {
    fn visit(&mut self, file: &str, mesh: &Mesh<'_>, bones: &[Mat4]) {
        self.skinned_meshes = self.skinned_meshes.saturating_add(1);
        let (Ok(positions), Ok(influences)) = (mesh.positions.to_vec(), mesh.influences.to_vec())
        else {
            return;
        };
        if influences.len() != positions.len() {
            return;
        }
        self.vertices = self.vertices.saturating_add(positions.len());
        let second = if mesh.positions2.len() == positions.len() {
            self.with_second_array = self.with_second_array.saturating_add(1);
            mesh.positions2.to_vec().ok()
        } else {
            None
        };

        let mut two_bone = 0usize;
        let mut bad = 0usize;
        let mut worst_here = 0.0f32;
        for (index, influence) in influences.iter().enumerate() {
            let [_, weight] = influence.weights_percent();
            if weight == 0 {
                continue;
            }
            two_bone = two_bone.saturating_add(1);
            let (Some(first), Some(second_bone)) = (
                bones.get(usize::from(influence.bones[0])),
                bones.get(usize::from(influence.bones[1])),
            ) else {
                continue;
            };
            let Some(position) = positions.get(index) else {
                continue;
            };
            let here = Vec3::new(position.x, position.y, position.z);
            let anchor = first.transform_point(here);
            let old = second_bone.transform_point(here);
            let old_gap = distance(anchor, old);
            self.total_old += f64::from(old_gap);
            if old_gap > self.worst_old {
                self.worst_old = old_gap;
            }
            if old_gap > worst_here {
                worst_here = old_gap;
            }
            let Some(other) = second.as_ref().and_then(|array| array.get(index)) else {
                continue;
            };
            let there = Vec3::new(other.x, other.y, other.z);
            let gap = distance(anchor, second_bone.transform_point(there));
            if gap > self.worst_new {
                self.worst_new = gap;
            }
            for (band, count) in BANDS.iter().zip(self.banded.iter_mut()) {
                if gap <= *band {
                    *count = count.saturating_add(1);
                }
            }
            if gap <= TOLERANCE {
                self.agreeing_vertices = self.agreeing_vertices.saturating_add(1);
            } else {
                self.disagreeing_vertices = self.disagreeing_vertices.saturating_add(1);
                bad = bad.saturating_add(1);
            }
        }

        self.two_bone_vertices = self.two_bone_vertices.saturating_add(two_bone);
        if two_bone > 0 {
            self.with_two_bone = self.with_two_bone.saturating_add(1);
            if second.is_none() {
                self.orphan_two_bone_meshes = self.orphan_two_bone_meshes.saturating_add(1);
            }
            self.worst_models.push((worst_here, file.to_owned()));
            self.worst_models
                .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            self.worst_models.truncate(NAMES);
        }
        if bad > 0 && self.disagreeing.len() < NAMES {
            self.disagreeing
                .push(format!("{file} ({}): {bad} vertex/vertices", mesh.name()));
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn report(&self, seconds: f64) {
        println!("\n{} skinned mesh(es)", self.skinned_meshes);
        println!(
            "  {} whose skeleton no mounted file supplies",
            self.no_skeleton
        );
        println!(
            "  {} carrying a full-length VERTICES_2",
            self.with_second_array
        );
        println!("  {} with at least one two-bone vertex", self.with_two_bone);
        println!(
            "  {} of those with no VERTICES_2 to serve it",
            self.orphan_two_bone_meshes
        );
        println!(
            "\n{} vertices, {} of them two-bone ({:.2}%)",
            self.vertices,
            self.two_bone_vertices,
            100.0 * self.two_bone_vertices as f64 / (self.vertices.max(1)) as f64
        );
        let checked = self.agreeing_vertices + self.disagreeing_vertices;
        println!(
            "cross-check: rest[b0]*VERTICES and rest[b1]*VERTICES_2 agree to {TOLERANCE} for \
             {} of {} ({:.4}%), worst {:.4}",
            self.agreeing_vertices,
            checked,
            100.0 * self.agreeing_vertices as f64 / (checked.max(1)) as f64,
            self.worst_new
        );
        for (band, count) in BANDS.iter().zip(self.banded.iter()) {
            println!(
                "            within {band:>4}: {count} ({:.4}%)",
                100.0 * *count as f64 / (checked.max(1)) as f64
            );
        }
        println!(
            "the old reading (bone 1 given bone 0's vertex) displaced those vertices by \
             {:.4} on average and {:.4} at worst",
            self.total_old / (self.two_bone_vertices.max(1)) as f64,
            self.worst_old
        );
        if !self.disagreeing.is_empty() {
            println!("\nmeshes with a disagreeing vertex:");
            for line in &self.disagreeing {
                println!("  {line}");
            }
        }
        println!("\nmodels the old reading displaced furthest:");
        for (gap, file) in &self.worst_models {
            println!("  {gap:8.3}  {file}");
        }
        println!("\n{seconds:.2}s");
    }
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    let (dx, dy, dz) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("big"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
