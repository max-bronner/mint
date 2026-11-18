use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_asset::{sway_index, Breeze, MAX_SWAY_TYPES};
use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;
use mint_sage_model::{load_object_index, Template};
use mint_sage_w3d::{Model, UvMapperType};
use mint_vfs::Vfs;

const TOP: usize = 15;

struct MapRow {
    name: String,
    objects: usize,
    fire: usize,
    fire_templates: usize,
    vegetation: usize,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut wanted: Option<String> = None;
    if let Some(at) = args.iter().position(|arg| arg == "--where") {
        wanted = args.get(at.saturating_add(1)).cloned();
        args.truncate(at);
    }
    let roots: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: vegetation_fire_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let Some(primary) = roots.first() else {
        eprintln!("usage: vegetation_fire_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    };
    println!("=== maps from {} ===", primary.display());
    for extra in roots.iter().skip(1) {
        println!("=== art also from {} ===", extra.display());
    }
    scan(primary, &roots, wanted.as_deref());
    ExitCode::SUCCESS
}

fn scan(primary: &Path, roots: &[PathBuf], wanted: Option<&str>) {
    let started = Instant::now();
    let mut vfs = Vfs::new();
    for root in roots.iter().skip(1).rev().chain(roots.iter().take(1)) {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    let all_archives: Vec<PathBuf> = roots.iter().flat_map(|root| big_archives(root)).collect();
    let map_archives = big_archives(primary);

    let mut grid_stems: BTreeMap<String, usize> = BTreeMap::new();
    let mut grid_materials = 0usize;
    for path in &all_archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if !lower.ends_with(".w3d") {
                continue;
            }
            let Ok(data) = archive.read_entry(entry) else {
                continue;
            };
            let Ok(model) = Model::parse(&data) else {
                continue;
            };
            let mut found = 0usize;
            for mesh in &model.meshes {
                for vm in &mesh.vertex_materials {
                    let Some(info) = &vm.info else { continue };
                    if info.stage0_mapper() == UvMapperType::Grid
                        || info.stage1_mapper() == UvMapperType::Grid
                    {
                        found += 1;
                    }
                }
            }
            if found > 0 {
                grid_materials += found;
                if let Some(stem) = stem_of(&lower) {
                    let seen = grid_stems.entry(stem).or_default();
                    *seen = (*seen).max(found);
                }
            }
        }
    }
    println!(
        "{} GRID vertex material(s) in {} model stem(s)",
        grid_materials,
        grid_stems.len()
    );

    let (index, report) = load_object_index(&vfs);
    println!(
        "object index: {} top-level definition(s) from {} document(s)",
        report.top_level, report.parsed
    );

    let mut rows: Vec<MapRow> = Vec::new();
    let mut fire_templates: BTreeMap<String, usize> = BTreeMap::new();
    let mut placed_stems: BTreeSet<String> = BTreeSet::new();
    let mut vegetation_templates: BTreeMap<String, usize> = BTreeMap::new();
    let mut both = 0usize;
    let mut maps = 0usize;
    let mut phases = [0usize; MAX_SWAY_TYPES + 1];
    let mut bad_payloads: Vec<String> = Vec::new();
    let tables: Vec<[[f32; 3]; mint_sage_asset::SWAY_OFFSET_COUNT]> = [0.0, 1.25, 2.5, 3.75]
        .iter()
        .map(|t| Breeze::GENERALS.offsets(*t))
        .collect();
    for path in &map_archives {
        let Ok(archive) = BigArchive::open(path) else {
            continue;
        };
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if !lower.ends_with(".map") && !lower.ends_with(".bse") {
                continue;
            }
            let Ok(raw) = archive.read_entry(entry) else {
                continue;
            };
            let Ok(map) = MapFile::parse(&raw) else {
                continue;
            };
            maps += 1;
            let mut row = MapRow {
                name: entry.name.clone(),
                objects: map.objects.len(),
                fire: 0,
                fire_templates: 0,
                vegetation: 0,
            };
            let mut seen_fire: BTreeSet<String> = BTreeSet::new();
            let naming = wanted.is_some_and(|needle| lower.contains(&needle.to_ascii_lowercase()));
            let border = f32::from(u16::try_from(map.height_map.border_size).unwrap_or(0));
            for object in &map.objects {
                let sways = index.sways(&object.template_name);
                let burns = match index.lookup(&object.template_name) {
                    Template::Model(model) => {
                        let key = model.to_ascii_lowercase();
                        if grid_stems.contains_key(&key) {
                            placed_stems.insert(key);
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if burns && naming {
                    println!(
                        "  FIRE  --focus {:.0},{:.0}  {}",
                        object.x / 10.0 + border,
                        object.y / 10.0 + border,
                        object.template_name
                    );
                }
                if sways && naming && row.vegetation < 6 {
                    println!(
                        "  TREE  --focus {:.0},{:.0}  {}",
                        object.x / 10.0 + border,
                        object.y / 10.0 + border,
                        object.template_name
                    );
                }
                if burns {
                    row.fire += 1;
                    seen_fire.insert(object.template_name.to_ascii_lowercase());
                    *fire_templates
                        .entry(object.template_name.to_ascii_lowercase())
                        .or_default() += 1;
                }
                if sways {
                    row.vegetation += 1;
                    *vegetation_templates
                        .entry(object.template_name.to_ascii_lowercase())
                        .or_default() += 1;
                    let index = sway_index(object.x, object.y);
                    match phases.get_mut(index) {
                        Some(slot) => *slot += 1,
                        None if bad_payloads.len() < 20 => bad_payloads.push(format!(
                            "{} :: {} at ({}, {}) hashed to slot {index}",
                            entry.name, object.template_name, object.x, object.y
                        )),
                        None => {}
                    }
                    for table in &tables {
                        let shear = table.get(index).copied().unwrap_or([f32::NAN; 3]);
                        if !shear.iter().all(|v| v.is_finite()) && bad_payloads.len() < 20 {
                            bad_payloads.push(format!(
                                "{} :: {} at ({}, {}) got a non-finite shear {shear:?}",
                                entry.name, object.template_name, object.x, object.y
                            ));
                        }
                    }
                }
                if sways && burns {
                    both += 1;
                }
            }
            row.fire_templates = seen_fire.len();
            rows.push(row);
        }
    }

    let total_fire: usize = rows.iter().map(|r| r.fire).sum();
    let total_veg: usize = rows.iter().map(|r| r.vegetation).sum();
    let total_objects: usize = rows.iter().map(|r| r.objects).sum();
    let maps_with_fire = rows.iter().filter(|r| r.fire > 0).count();
    let maps_with_veg = rows.iter().filter(|r| r.vegetation > 0).count();
    println!(
        "{maps} map(s), {total_objects} placement(s): {total_fire} name a GRID-bearing model on \
         {maps_with_fire} map(s), {total_veg} are drawn by a W3DTreeDraw on {maps_with_veg} map(s)"
    );
    let placed_materials: usize = placed_stems
        .iter()
        .filter_map(|stem| grid_stems.get(stem))
        .sum();
    println!(
        "{} of the {} GRID model stems are placed by at least one map, carrying {} of the {} \
         GRID vertex materials; {} distinct fire template(s), {} distinct vegetation template(s)",
        placed_stems.len(),
        grid_stems.len(),
        placed_materials,
        grid_materials,
        fire_templates.len(),
        vegetation_templates.len()
    );
    println!(
        "cross-check 1: {both} placement(s) are both a W3DTreeDraw and a GRID model (expected 0 \
         or a handful)"
    );

    let payloads: usize = phases.iter().sum();
    println!(
        "cross-check 2: {payloads} sway payload(s) built, {} rejected; slot 0 (the \
         does-not-sway entry) was chosen {} time(s)",
        bad_payloads.len(),
        phases.first().copied().unwrap_or(0)
    );
    for line in bad_payloads.iter().take(20) {
        println!("  BAD: {line}");
    }
    if payloads > 0 {
        let ideal = payloads / MAX_SWAY_TYPES;
        print!("  phases 1..10:");
        for count in phases.iter().skip(1) {
            print!(" {count}");
        }
        println!();
        let worst = phases
            .iter()
            .skip(1)
            .map(|count| {
                let diff = count.abs_diff(ideal);
                #[allow(clippy::cast_precision_loss)]
                let ratio = diff as f64 / ideal.max(1) as f64;
                ratio
            })
            .fold(0.0f64, f64::max);
        println!("  worst phase is {:.1}% off an even tenth", worst * 100.0);
    }

    rows.sort_by(|a, b| b.fire.cmp(&a.fire).then_with(|| a.name.cmp(&b.name)));
    println!("\n-- most GRID placements (map, fire, distinct fire templates, trees, objects) --");
    for row in rows.iter().take(TOP) {
        println!(
            "  {:5} {:3} {:6} {:7}  {}",
            row.fire, row.fire_templates, row.vegetation, row.objects, row.name
        );
    }

    rows.sort_by(|a, b| {
        b.vegetation
            .cmp(&a.vegetation)
            .then_with(|| a.name.cmp(&b.name))
    });
    println!("\n-- most W3DTreeDraw placements (fire, templates, trees, objects) --");
    for row in rows.iter().take(TOP) {
        println!(
            "  {:5} {:3} {:6} {:7}  {}",
            row.fire, row.fire_templates, row.vegetation, row.objects, row.name
        );
    }

    let mut best: Vec<&MapRow> = rows
        .iter()
        .filter(|r| r.fire > 0 && r.vegetation > 0)
        .collect();
    best.sort_by(|a, b| {
        (b.fire.min(b.vegetation / 10))
            .cmp(&(a.fire.min(a.vegetation / 10)))
            .then_with(|| a.name.cmp(&b.name))
    });
    println!("\n-- maps with plenty of both (fire, templates, trees, objects) --");
    for row in best.iter().take(TOP) {
        println!(
            "  {:5} {:3} {:6} {:7}  {}",
            row.fire, row.fire_templates, row.vegetation, row.objects, row.name
        );
    }

    let mut named: Vec<(&String, &usize)> = fire_templates.iter().collect();
    named.sort_by(|a, b| b.1.cmp(a.1));
    println!("\n-- most placed fire templates --");
    for (name, n) in named.iter().take(TOP) {
        println!("  {n:5}  {name}");
    }

    println!("\nscanned in {:.1}s", started.elapsed().as_secs_f64());
}

fn stem_of(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
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
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("big"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
