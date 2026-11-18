use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mint_sage_big::BigArchive;
use mint_sage_w3d::{MapperArgs, Model, UvMapperType};

const GRID: UvMapperType = UvMapperType::Grid;

const CELL_TOLERANCE: f32 = 0.001;

struct Counts {
    files: BTreeSet<String>,
    materials_stage0: usize,
    materials_stage1: usize,
    materials_either: usize,
    meshes: usize,
    arg_sets: BTreeMap<String, usize>,
    keys: BTreeMap<String, usize>,
    log2_widths: BTreeMap<u32, usize>,
    blends: BTreeMap<(u8, u8, u8, u8, u8), usize>,
    textures: BTreeMap<String, usize>,
    stage_counts: BTreeMap<usize, usize>,
    stage1_with_second_stage: usize,
    stage1_without_second_stage: usize,
    uv_inside_cell: usize,
    uv_outside_cell: usize,
    uv_absent: usize,
    outside_examples: Vec<String>,
}

impl Counts {
    fn new() -> Self {
        Self {
            files: BTreeSet::new(),
            materials_stage0: 0,
            materials_stage1: 0,
            materials_either: 0,
            meshes: 0,
            arg_sets: BTreeMap::new(),
            keys: BTreeMap::new(),
            log2_widths: BTreeMap::new(),
            blends: BTreeMap::new(),
            textures: BTreeMap::new(),
            stage_counts: BTreeMap::new(),
            stage1_with_second_stage: 0,
            stage1_without_second_stage: 0,
            uv_inside_cell: 0,
            uv_outside_cell: 0,
            uv_absent: 0,
            outside_examples: Vec::new(),
        }
    }
}

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: grid_mapper_scan <install dir> [<install dir> ...]");
        return;
    }

    let mut counts = Counts::new();
    let mut files_walked = 0usize;
    let mut files_refused = 0usize;
    let mut bytes = 0u64;
    let started = std::time::Instant::now();

    for root in &roots {
        for path in files_with_extension(root, "big") {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                if !entry.name.to_ascii_lowercase().ends_with(".w3d") {
                    continue;
                }
                let Ok(data) = archive.read_entry(entry) else {
                    continue;
                };
                files_walked += 1;
                bytes += data.len() as u64;
                let Ok(model) = Model::parse(&data) else {
                    files_refused += 1;
                    continue;
                };
                scan_model(&model, &entry.name, &mut counts);
            }
        }
    }

    let elapsed = started.elapsed().as_secs_f64();
    println!("=== GRID (mapper 7) census ===");
    println!(
        "{files_walked} .w3d entries walked, {files_refused} refused to parse, \
         {:.1} MB in {elapsed:.2}s ({:.1} MB/s)",
        bytes as f64 / 1e6,
        bytes as f64 / 1e6 / elapsed.max(1e-9)
    );
    println!(
        "GRID vertex materials: {} on stage 0, {} on stage 1, {} on either, in {} file(s)",
        counts.materials_stage0,
        counts.materials_stage1,
        counts.materials_either,
        counts.files.len()
    );
    println!("meshes carrying at least one: {}", counts.meshes);

    println!("\n-- Log2Width --");
    for (width, n) in &counts.log2_widths {
        println!(
            "  Log2Width={width} ({}x{} cells): {n}",
            1 << width,
            1 << width
        );
    }

    println!("\n-- keys present in GRID mapper arguments --");
    for (key, n) in &counts.keys {
        println!("  {key}: {n}");
    }

    println!(
        "\n-- argument sets, deduplicated (top 20 of {}) --",
        counts.arg_sets.len()
    );
    let mut sets: Vec<(&String, &usize)> = counts.arg_sets.iter().collect();
    sets.sort_by(|a, b| b.1.cmp(a.1));
    for (text, n) in sets.iter().take(20) {
        println!("  {n:5}  {}", text.replace('\n', " | "));
    }

    println!(
        "\n-- blend state of GRID meshes (depth_write, src, dest, pri_gradient, alpha_test) --"
    );
    let mut blends: Vec<(&(u8, u8, u8, u8, u8), &usize)> = counts.blends.iter().collect();
    blends.sort_by(|a, b| b.1.cmp(a.1));
    for (state, n) in blends {
        println!("  {n:5}  {state:?}");
    }
    let additive: usize = counts
        .blends
        .iter()
        .filter(|((depth, src, dest, _, _), _)| *depth == 0 && *src == 1 && *dest == 1)
        .map(|(_, n)| *n)
        .sum();
    println!("  additive with depth writes off: {additive}");

    println!("\n-- texture stages on GRID meshes --");
    for (stages, n) in &counts.stage_counts {
        println!("  {stages} stage(s): {n} mesh(es)");
    }
    println!(
        "  stage-1 GRID declarations: {} on a mesh that has a second stage, {} on one that does \
         not (F3)",
        counts.stage1_with_second_stage, counts.stage1_without_second_stage
    );

    println!("\n-- stage 0 textures (top 12) --");
    let mut textures: Vec<(&String, &usize)> = counts.textures.iter().collect();
    textures.sort_by(|a, b| b.1.cmp(a.1));
    for (name, n) in textures.iter().take(12) {
        println!("  {n:5}  {name}");
    }

    println!("\n=== the cross-check: do the stored UVs fit inside one cell? ===");
    let judged = counts.uv_inside_cell + counts.uv_outside_cell;
    println!(
        "{} of {judged} GRID meshes have every triangle inside one \
         1/gridWidth cell in both u and v ({} do not); {} carry no coordinates at all",
        counts.uv_inside_cell, counts.uv_outside_cell, counts.uv_absent
    );
    for line in counts.outside_examples.iter().take(20) {
        println!("  OUTSIDE: {line}");
    }
}

fn scan_model(model: &Model<'_>, name: &str, counts: &mut Counts) {
    for mesh in &model.meshes {
        let mut mesh_has_grid = false;
        let mut mesh_log2: Option<u32> = None;
        let mut mesh_stage1_grid = false;
        for vm in &mesh.vertex_materials {
            let Some(info) = &vm.info else { continue };
            let s0 = info.stage0_mapper() == GRID;
            let s1 = info.stage1_mapper() == GRID;
            if !s0 && !s1 {
                continue;
            }
            mesh_has_grid = true;
            counts.files.insert(name.to_ascii_lowercase());
            counts.materials_either += 1;
            if s0 {
                counts.materials_stage0 += 1;
            }
            if s1 {
                counts.materials_stage1 += 1;
                mesh_stage1_grid = true;
            }
            let text = if s0 {
                vm.mapper_args0.clone()
            } else {
                vm.mapper_args1.clone()
            };
            let text = text.unwrap_or_default();
            let args = MapperArgs::parse(&text);
            *counts.arg_sets.entry(normalise(&text)).or_default() += 1;
            for line in text.split('\n') {
                if let Some((key, _)) = line.trim_end_matches('\r').split_once('=') {
                    *counts.keys.entry(key.trim().to_owned()).or_default() += 1;
                }
            }
            if s0 {
                *counts.log2_widths.entry(args.log2_width).or_default() += 1;
                mesh_log2 = Some(args.log2_width);
            }
        }
        if !mesh_has_grid {
            continue;
        }
        counts.meshes += 1;
        let stages = mesh
            .passes
            .first()
            .map_or(0, |pass| pass.texture_stages.len());
        *counts.stage_counts.entry(stages).or_default() += 1;
        if mesh_stage1_grid {
            if stages >= 2 {
                counts.stage1_with_second_stage += 1;
            } else {
                counts.stage1_without_second_stage += 1;
            }
        }
        if let Some(shader) = mesh.drawn_shaders().first() {
            *counts
                .blends
                .entry((
                    shader.depth_mask,
                    shader.src_blend,
                    shader.dest_blend,
                    shader.pri_gradient,
                    shader.alpha_test,
                ))
                .or_default() += 1;
        }
        if let Some(texture) = mesh.texture_name() {
            *counts
                .textures
                .entry(texture.to_ascii_lowercase())
                .or_default() += 1;
        }
        if let Some(log2) = mesh_log2 {
            check_cell_fit(mesh, name, log2, counts);
        }
    }
}

fn check_cell_fit(mesh: &mint_sage_w3d::Mesh<'_>, file: &str, log2: u32, counts: &mut Counts) {
    let coords: Vec<mint_sage_w3d::Vec2> = mesh.texcoords().iter().flatten().collect();
    if coords.is_empty() {
        counts.uv_absent += 1;
        return;
    }
    let cell = 1.0 / (1u32 << log2.min(8)) as f32;
    let mut worst = (0.0f32, 0.0f32);
    let mut fits = true;
    for triangle in mesh.triangles.iter().flatten() {
        let mut min = (f32::MAX, f32::MAX);
        let mut max = (f32::MIN, f32::MIN);
        let mut seen = 0;
        for index in triangle.indices {
            let Some(uv) = usize::try_from(index).ok().and_then(|i| coords.get(i)) else {
                continue;
            };
            seen += 1;
            min.0 = min.0.min(uv.u);
            min.1 = min.1.min(uv.v);
            max.0 = max.0.max(uv.u);
            max.1 = max.1.max(uv.v);
        }
        if seen == 0 {
            continue;
        }
        let span = (max.0 - min.0, max.1 - min.1);
        worst = (worst.0.max(span.0), worst.1.max(span.1));
        if span.0 > cell + CELL_TOLERANCE || span.1 > cell + CELL_TOLERANCE {
            fits = false;
        }
    }
    if fits {
        counts.uv_inside_cell += 1;
    } else {
        counts.uv_outside_cell += 1;
        if counts.outside_examples.len() < 20 {
            counts.outside_examples.push(format!(
                "{file} :: {} :: Log2Width={log2} cell={cell:.4} worst triangle                  span=({:.4}, {:.4})",
                mesh.name(),
                worst.0,
                worst.1
            ));
        }
    }
}

fn normalise(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r').trim();
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
