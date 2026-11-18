use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: wall_spacing_scan <install dir> [...]");
        return ExitCode::FAILURE;
    }
    let mut spacings: BTreeMap<String, BTreeMap<i64, u64>> = BTreeMap::new();
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut files = 0u64;
    for root in &roots {
        for path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    continue;
                };
                let Ok(map) = MapFile::parse(&raw) else {
                    continue;
                };
                files = files.saturating_add(1);
                scan(&map, &mut spacings, &mut counts);
            }
        }
        for path in loose_maps(root) {
            let Ok(raw) = std::fs::read(&path) else {
                continue;
            };
            let Ok(map) = MapFile::parse(&raw) else {
                continue;
            };
            files = files.saturating_add(1);
            scan(&map, &mut spacings, &mut counts);
        }
    }
    println!("files {files}");
    for (template, total) in &counts {
        println!("{template}: {total} placed");
        if let Some(hist) = spacings.get(template) {
            let mut rows: Vec<(i64, u64)> = hist.iter().map(|(k, v)| (*k, *v)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1));
            for (step, n) in rows.iter().take(6) {
                println!("    nearest-neighbour {:.2} x{n}", *step as f64 / 100.0);
            }
        }
    }
    ExitCode::SUCCESS
}

fn scan(
    map: &MapFile,
    spacings: &mut BTreeMap<String, BTreeMap<i64, u64>>,
    counts: &mut BTreeMap<String, u64>,
) {
    let mut by_template: BTreeMap<String, Vec<(f32, f32)>> = BTreeMap::new();
    for object in &map.objects {
        let lower = object.template_name.to_ascii_lowercase();
        if !(lower.contains("wallsegment") || lower.contains("wallhub") || lower.contains("wall")) {
            continue;
        }
        by_template
            .entry(object.template_name.clone())
            .or_default()
            .push((object.x, object.y));
    }
    for (template, points) in by_template {
        *counts.entry(template.clone()).or_default() += points.len() as u64;
        if points.len() < 2 {
            continue;
        }
        for (i, a) in points.iter().enumerate() {
            let mut best = f32::MAX;
            for (j, b) in points.iter().enumerate() {
                if i == j {
                    continue;
                }
                let (dx, dy) = (a.0 - b.0, a.1 - b.1);
                let d = (dx * dx + dy * dy).sqrt();
                if d < best {
                    best = d;
                }
            }
            if best.is_finite() {
                #[allow(clippy::cast_possible_truncation)]
                let bucket = (best * 100.0).round() as i64;
                *spacings
                    .entry(template.clone())
                    .or_default()
                    .entry(bucket)
                    .or_default() += 1;
            }
        }
    }
}

fn loose_maps(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("map") || ext.eq_ignore_ascii_case("bse")
            }) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn big_archives(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("big"))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
