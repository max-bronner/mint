use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_render::Blend;
use mint_sage_big::BigArchive;
use mint_sage_model::convert::{convert_model, Part};
use mint_sage_w3d::Model;

const NAMES_PER_CATEGORY: usize = 6;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: model_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut scan = Scan::default();
    let started = Instant::now();
    let mut bytes = 0u64;
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.path.as_str().ends_with(".w3d") {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    scan.unreadable = scan.unreadable.saturating_add(1);
                    continue;
                };
                bytes = bytes.saturating_add(raw.len() as u64);
                scan.visit(entry.path.as_str(), &raw);
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    scan.report(elapsed, bytes);
    if scan.non_finite > 0 || scan.invalid_meshes > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[derive(Default)]
struct Category {
    parts: usize,
    files: Vec<String>,
}

impl Category {
    fn hit(&mut self, file: &str) {
        self.parts = self.parts.saturating_add(1);
        if self.files.len() < NAMES_PER_CATEGORY
            && self.files.last().map(String::as_str) != Some(file)
        {
            self.files.push(file.to_owned());
        }
    }
}

#[derive(Default)]
struct Scan {
    files: usize,
    unreadable: usize,
    parsed: usize,
    converted: usize,
    no_meshes: usize,
    parse_failures: Vec<String>,
    convert_failures: Vec<String>,
    parts: usize,
    vertices: usize,
    triangles: usize,
    dropped_triangles: usize,
    invalid_meshes: usize,
    non_finite: usize,
    categories: BTreeMap<&'static str, Category>,
    shaders: ShaderFacts,
}

#[derive(Default)]
struct ShaderFacts {
    entries: usize,
    multi_pass_meshes: usize,
    multi_pass_with_shader_ids: usize,
    pass_selection_differs: usize,
    cutout_meshes: usize,
    cutout_entries: usize,
    depth_writing_translucent_entries: usize,
}

impl Scan {
    fn visit(&mut self, path: &str, raw: &[u8]) {
        self.files = self.files.saturating_add(1);
        let model = match Model::parse(raw) {
            Ok(model) => model,
            Err(error) => {
                if self.parse_failures.len() < 20 {
                    self.parse_failures.push(format!("{path}: {error}"));
                }
                return;
            }
        };
        self.parsed = self.parsed.saturating_add(1);
        if model.meshes.is_empty() {
            self.no_meshes = self.no_meshes.saturating_add(1);
            return;
        }
        let converted = match convert_model(&model, model.hierarchy.as_ref()) {
            Ok(converted) => converted,
            Err(error) => {
                if self.convert_failures.len() < 20 {
                    self.convert_failures.push(format!("{path}: {error}"));
                }
                return;
            }
        };
        self.converted = self.converted.saturating_add(1);
        for part in &converted.parts {
            self.visit_part(path, part);
        }
        for mesh in &model.meshes {
            self.visit_shaders(mesh);
        }
    }

    fn visit_shaders(&mut self, mesh: &mint_sage_w3d::Mesh<'_>) {
        let facts = &mut self.shaders;
        facts.entries = facts.entries.saturating_add(mesh.shaders.len());
        let drawn = mesh.drawn_shaders();

        if mesh.passes.len() > 1 {
            facts.multi_pass_meshes = facts.multi_pass_meshes.saturating_add(1);
            if mesh
                .passes
                .first()
                .is_some_and(|pass| pass.shader_ids.is_some())
            {
                facts.multi_pass_with_shader_ids =
                    facts.multi_pass_with_shader_ids.saturating_add(1);
                let mut selected: Vec<u8> = drawn.iter().map(|s| s.dest_blend).collect();
                let mut all: Vec<u8> = mesh
                    .shaders
                    .iter()
                    .flatten()
                    .map(|s| s.dest_blend)
                    .collect();
                selected.sort_unstable();
                selected.dedup();
                all.sort_unstable();
                all.dedup();
                if selected != all {
                    facts.pass_selection_differs = facts.pass_selection_differs.saturating_add(1);
                }
            }
        }

        let mut mesh_has_cutout = false;
        for shader in &drawn {
            if shader.dest_blend < 2 || shader.depth_mask == 0 {
                continue;
            }
            if shader.alpha_test == 0 {
                facts.depth_writing_translucent_entries =
                    facts.depth_writing_translucent_entries.saturating_add(1);
                continue;
            }
            facts.cutout_entries = facts.cutout_entries.saturating_add(1);
            mesh_has_cutout = true;
        }
        if mesh_has_cutout {
            facts.cutout_meshes = facts.cutout_meshes.saturating_add(1);
        }
    }

    fn visit_part(&mut self, path: &str, part: &Part) {
        self.parts = self.parts.saturating_add(1);
        self.vertices = self.vertices.saturating_add(part.mesh.vertices.len());
        self.triangles = self.triangles.saturating_add(part.mesh.triangle_count());
        self.dropped_triangles = self
            .dropped_triangles
            .saturating_add(part.dropped_triangles);
        if part.mesh.validate().is_some() {
            self.invalid_meshes = self.invalid_meshes.saturating_add(1);
        }
        if !part
            .transform
            .to_cols_array()
            .into_iter()
            .all(f32::is_finite)
        {
            self.non_finite = self.non_finite.saturating_add(1);
        }
        let mut hit = |name: &'static str| {
            self.categories.entry(name).or_default().hit(path);
        };
        if part.texture2.is_some() {
            hit("second texture stage");
        }
        if part.env_map0.is_some() || part.env_map1.is_some() {
            hit("environment mapper");
        }
        if part.blend == Blend::Additive {
            hit("additive blend");
        }
        if part.blend == Blend::AlphaBlend {
            hit("alpha blend");
        }
        if part.sort_level.is_some() {
            hit("explicit sort level");
        }
        if part.alpha_test {
            hit("alpha test");
        }
        if part.normal_map.is_some() {
            hit("normal map");
        }
        if part.uv_mapper.is_some() {
            hit("animated uv mapper");
        }
        if part.skinned {
            hit("skinned");
        }
        if part.unlit {
            hit("unlit");
        }
    }

    fn report(&self, elapsed: f64, bytes: u64) {
        println!("== files ==");
        println!(
            "{} .w3d entries, {} unreadable, {} parsed, {} held no mesh, {} converted",
            self.files, self.unreadable, self.parsed, self.no_meshes, self.converted
        );
        for line in &self.parse_failures {
            println!("  PARSE {line}");
        }
        for line in &self.convert_failures {
            println!("  CONVERT {line}");
        }

        println!("\n== geometry ==");
        println!(
            "{} part(s), {} vertices, {} triangles",
            self.parts, self.vertices, self.triangles
        );
        println!(
            "{} triangle(s) dropped for an out-of-range index; {} part(s) failed \
             MeshData::validate; {} part(s) carry a non-finite transform",
            self.dropped_triangles, self.invalid_meshes, self.non_finite
        );

        println!("\n== SHADERS table, read from the file ==");
        let facts = &self.shaders;
        println!(
            "{} entr(ies); {} mesh(es) carry more than one MATERIAL_PASS, {} of those name \
             their shaders on pass 0, and {} of *those* hold a dest_blend outside pass 0's \
             own selection",
            facts.entries,
            facts.multi_pass_meshes,
            facts.multi_pass_with_shader_ids,
            facts.pass_selection_differs
        );
        println!(
            "drawn entries that are alpha-blended, write depth and alpha-test (read as a \
             cutout): {} entr(ies) over {} mesh(es); {} more write depth without an alpha \
             test and stay blended",
            facts.cutout_entries, facts.cutout_meshes, facts.depth_writing_translucent_entries
        );

        println!("\n== material paths, and files that exercise each ==");
        for (name, category) in &self.categories {
            println!("{:>8} part(s)  {name}", category.parts);
            for file in &category.files {
                println!("             {file}");
            }
        }

        println!("\n== throughput ==");
        println!(
            "{:.2} s, {:.1} MiB, {:.1} MiB/s",
            elapsed,
            bytes as f64 / (1024.0 * 1024.0),
            bytes as f64 / (1024.0 * 1024.0) / elapsed.max(1e-9)
        );
    }
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
                .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
