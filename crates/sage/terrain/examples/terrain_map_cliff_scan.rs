use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mint_sage_big::BigArchive;
use mint_sage_map::{BlendTileData, MapFile};
use mint_sage_terrain::{map_cliff_class, TileLayout};

const STATED: [(&str, &str); 2] = [
    ("map evil fornost", "ClifFornost01"),
    ("map wor minas morgul", "CliffMinasMorgul01"),
];

const CONTROLS: [(&str, Option<&str>); 4] = [
    ("nocliff", None),
    ("cliff1", Some("CliffHarlindon05")),
    ("cliff2", Some("CliffMinasMorgul01")),
    ("cliff3", Some("AsphaltType1")),
];

const SAMPLES: usize = 6;

struct MapResult {
    name: String,
    cliff_texture: Option<String>,
    most_referenced: Option<(String, u64)>,
    agrees: bool,
    gated_cells: u64,
    already_map_texture: u64,
    changing: Vec<(u32, u32, String)>,
}

#[derive(Default)]
struct Seen {
    files: usize,
    parsed: usize,
    no_cliffs: usize,
    empty: Vec<String>,
    results: Vec<MapResult>,
    failures: Vec<String>,
}

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_map_cliff_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut seen = Seen::default();
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            let entries: Vec<_> = archive
                .entries()
                .iter()
                .filter(|entry| is_map(&entry.name))
                .cloned()
                .collect();
            for entry in entries {
                seen.files = seen.files.saturating_add(1);
                match archive.read_entry(&entry) {
                    Ok(raw) => take(&entry.name, &raw, &mut seen),
                    Err(error) => seen.failures.push(format!("{}: {error}", entry.name)),
                }
            }
        }
        for path in loose_maps(root) {
            seen.files = seen.files.saturating_add(1);
            let label = path.display().to_string();
            match std::fs::read(&path) {
                Ok(raw) => take(&label, &raw, &mut seen),
                Err(error) => seen.failures.push(format!("{label}: {error}")),
            }
        }
    }

    report(&seen)
}

fn take(label: &str, raw: &[u8], seen: &mut Seen) {
    match MapFile::parse(raw) {
        Ok(map) => {
            seen.parsed = seen.parsed.saturating_add(1);
            match measure(label, &map.blend_tile) {
                Some(result) => seen.results.push(result),
                None => {
                    seen.no_cliffs = seen.no_cliffs.saturating_add(1);
                    seen.empty.push(label.to_owned());
                }
            }
        }
        Err(error) => seen.failures.push(format!("{label}: {error}")),
    }
}

fn is_map(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".map") || lower.ends_with(".bse")
}

fn measure(name: &str, blend_tile: &BlendTileData) -> Option<MapResult> {
    if blend_tile.cliff_records.is_empty() {
        return None;
    }
    let layout = TileLayout::new(&blend_tile.texture_classes);
    let class_name = |class: usize| -> String {
        blend_tile
            .texture_classes
            .get(class)
            .map_or_else(|| format!("class {class} (unnamed)"), |c| c.name.clone())
    };
    let map_class = map_cliff_class(&layout, &blend_tile.cliff_records);

    let mut cells_per_class: BTreeMap<usize, u64> = BTreeMap::new();
    let mut gated_cells = 0u64;
    let mut already = 0u64;
    let mut gated: Vec<(usize, usize)> = Vec::new();
    for (index, value) in blend_tile.cliff_info_index.iter().enumerate() {
        let Some(class) = record_class(&layout, blend_tile, *value) else {
            continue;
        };
        let counter = cells_per_class.entry(class).or_insert(0);
        *counter = counter.saturating_add(1);
        let base = blend_tile
            .tile_index
            .get(index)
            .and_then(|tile| layout.cell(*tile))
            .map(|tile| tile.class);
        if base == Some(class) {
            gated_cells = gated_cells.saturating_add(1);
            if base == map_class {
                already = already.saturating_add(1);
            }
            gated.push((index, class));
        }
    }
    let mut ranked: Vec<(usize, u64)> = cells_per_class.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let most_referenced = ranked
        .first()
        .map(|(class, count)| (class_name(*class), *count));

    let width = u64::from(blend_tile.width).max(1);
    let mut changing = Vec::new();
    let stride = (gated.len() / SAMPLES.max(1)).max(1);
    for (index, class) in gated.iter().step_by(stride) {
        if Some(*class) == map_class {
            continue;
        }
        let at = u64::try_from(*index).unwrap_or(0);
        changing.push((
            u32::try_from(at % width).unwrap_or(0),
            u32::try_from(at / width).unwrap_or(0),
            class_name(*class),
        ));
        if changing.len() >= SAMPLES {
            break;
        }
    }

    Some(MapResult {
        name: name.to_owned(),
        cliff_texture: map_class.map(class_name),
        agrees: map_class == ranked.first().map(|(class, _)| *class),
        most_referenced,
        gated_cells,
        already_map_texture: already,
        changing,
    })
}

fn record_class(layout: &TileLayout, blend_tile: &BlendTileData, index: u32) -> Option<usize> {
    let slot = usize::try_from(index).ok()?.checked_sub(1)?;
    let record = blend_tile.cliff_records.get(slot)?;
    let value = u16::try_from(record.tile_index).ok()?;
    Some(layout.cell(value)?.class)
}

#[allow(clippy::too_many_lines)]
fn report(seen: &Seen) -> ExitCode {
    println!("\n=== the map's cliff texture, from cliff record 0 ===\n");
    println!(
        "{} .map/.bse file(s), {} parsed, {} failed",
        seen.files,
        seen.parsed,
        seen.failures.len()
    );
    println!(
        "{} with an empty cliff record list - no cliff texture",
        seen.no_cliffs
    );
    println!("{} with at least one record\n", seen.results.len());

    let mut ok = true;

    println!("the authored control maps:");
    for (needle, expected) in CONTROLS {
        let found = seen
            .results
            .iter()
            .find(|r| control_matches(&r.name, needle));
        match (found, expected) {
            (Some(result), Some(want)) => {
                let got = result.cliff_texture.as_deref().unwrap_or("(none)");
                let good = got.eq_ignore_ascii_case(want);
                ok &= good;
                println!(
                    "  {} -> {got}{}",
                    result.name,
                    if good {
                        String::new()
                    } else {
                        format!("  !! authored as {want} !!")
                    }
                );
                if let Some((name, count)) = &result.most_referenced {
                    println!("      most-referenced class would say {name} ({count} cells)");
                }
                println!(
                    "      {} cell(s) pass the interim gate; {} already draw it, so {} change",
                    result.gated_cells,
                    result.already_map_texture,
                    result
                        .gated_cells
                        .saturating_sub(result.already_map_texture)
                );
            }
            (Some(result), None) => {
                ok = false;
                println!(
                    "  {} -> {} !! this map was authored with no cliff texture and should \
                     carry no record !!",
                    result.name,
                    result.cliff_texture.as_deref().unwrap_or("(none)")
                );
            }
            (None, None) => match seen.empty.iter().find(|name| control_matches(name, needle)) {
                Some(name) => println!("  {name} -> no cliff record at all - correct, the control"),
                None => println!("  {needle}: not present as a loose .map - skipped"),
            },
            (None, Some(_)) => println!("  {needle}: not present as a loose .map - skipped"),
        }
    }

    println!("\nthe two shipped maps the owner named:");
    for (needle, expected) in STATED {
        match seen
            .results
            .iter()
            .find(|r| r.name.to_ascii_lowercase().contains(needle))
        {
            Some(result) => {
                let got = result.cliff_texture.as_deref().unwrap_or("(none)");
                let good = got.eq_ignore_ascii_case(expected);
                ok &= good;
                println!(
                    "  {} -> {got}{}",
                    result.name,
                    if good {
                        String::new()
                    } else {
                        format!("  !! he says {expected} !!")
                    }
                );
                println!(
                    "      {} cell(s) pass the interim gate; {} already draw it, so {} change",
                    result.gated_cells,
                    result.already_map_texture,
                    result
                        .gated_cells
                        .saturating_sub(result.already_map_texture)
                );
                for (x, y, class) in &result.changing {
                    println!("      changes: cell ({x}, {y}), base class {class}");
                }
            }
            None => {
                ok = false;
                println!("  {needle}: !! not found in the mounted archives !!");
            }
        }
    }

    let disagree: Vec<&MapResult> = seen.results.iter().filter(|r| !r.agrees).collect();
    println!(
        "\nrecord 0's class differs from the most-referenced class on {} of {} map(s)",
        disagree.len(),
        seen.results.len()
    );
    for result in disagree.iter().take(20) {
        let (name, count) = result
            .most_referenced
            .clone()
            .unwrap_or_else(|| ("(none)".to_owned(), 0));
        println!(
            "  {:<52} record 0 {} vs most-referenced {name} ({count} cells)",
            result.name,
            result.cliff_texture.as_deref().unwrap_or("(none)")
        );
    }

    let (mut gated, mut changing) = (0u64, 0u64);
    for result in &seen.results {
        gated = gated.saturating_add(result.gated_cells);
        changing = changing.saturating_add(
            result
                .gated_cells
                .saturating_sub(result.already_map_texture),
        );
    }
    println!(
        "\nacross every map with a cliff record: {gated} cell(s) pass the gate, \
         {changing} of them change texture under --map-cliff-texture"
    );

    let unresolved = seen
        .results
        .iter()
        .filter(|r| r.cliff_texture.is_none())
        .count();
    println!("{unresolved} map(s) whose record 0 names no declared texture class");

    for failure in seen.failures.iter().take(10) {
        println!("  failed: {failure}");
    }

    if ok && seen.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn control_matches(name: &str, needle: &str) -> bool {
    Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(needle))
}

fn loose_maps(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in top.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("maps")
        {
            continue;
        }
        let Ok(listing) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for map_dir in listing.flatten() {
            let path = map_dir.path();
            if path.is_file() {
                if is_map(&path.to_string_lossy()) {
                    found.push(path);
                }
                continue;
            }
            let Ok(inner) = std::fs::read_dir(&path) else {
                continue;
            };
            for file in inner.flatten() {
                let file = file.path();
                if file.is_file() && is_map(&file.to_string_lossy()) {
                    found.push(file);
                }
            }
        }
    }
    found.sort();
    found
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
