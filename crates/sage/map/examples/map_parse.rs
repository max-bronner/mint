use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, TimeOfDay};

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: map_parse <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        walk_install(root, &mut totals);
    }
    totals.report(started.elapsed().as_secs_f64());

    if totals.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct Totals {
    files: u64,
    parsed: u64,
    decompressed_bytes: u64,
    failures: Vec<String>,

    objects: u64,
    heightmap_dims: BTreeMap<(u32, u32), u64>,
    boundary_matches_formula: u64,
    boundary_checked: u64,

    lighting_versions: BTreeMap<u16, u64>,
    lighting_exact_length: u64,
    lighting_length_checked: u64,
    unit_directions_1e2: u64,
    unit_directions_1e3: u64,
    lights_seen: u64,
    accent_ambients: u64,
    accents_seen: u64,
    unnamed_flags: BTreeMap<u32, u64>,
    unnamed_off_quantum: u64,
    unnamed_components: u64,
    unnamed_half: u64,
    unnamed_alphas: BTreeMap<u8, u64>,
    ambient_range: (f32, f32),
    color_range: (f32, f32),
    time_of_day: BTreeMap<String, u64>,
    suns_pointing_down: u64,
    suns_seen: u64,

    blend_versions: BTreeMap<u16, u64>,
    out_of_range_tile_files: Vec<String>,
    out_of_range_tile_cells: u64,
    total_cells: u64,
}

impl Totals {
    fn report(&self, seconds: f64) {
        println!(
            "{} .map/.bse files, {} parsed, {:.1} MiB decompressed in {seconds:.1}s ({:.1} MiB/s)",
            self.files,
            self.parsed,
            mib(self.decompressed_bytes),
            mib(self.decompressed_bytes) / seconds.max(f64::EPSILON),
        );
        println!("{} object placements across the corpus", self.objects);
        println!("{} distinct HeightMapData sizes", self.heightmap_dims.len());
        println!(
            "boundary[0] matches the (width - 2*border, height - 2*border) formula in {} of {} non-trivial maps",
            self.boundary_matches_formula, self.boundary_checked
        );
        println!("GlobalLighting versions seen:");
        for (version, count) in &self.lighting_versions {
            println!("  version {version:>2}: {count} files");
        }
        println!(
            "{} of {} GlobalLighting payloads are exactly the length their version calls for",
            self.lighting_exact_length, self.lighting_length_checked
        );
        println!(
            "{} of {} light directions are unit length within 1e-2, {} within 1e-3",
            self.unit_directions_1e2, self.lights_seen, self.unit_directions_1e3
        );
        println!(
            "{} of {} terrain accent lights carry a nonzero ambient (expected 0)",
            self.accent_ambients, self.accents_seen
        );
        println!(
            "unnamed block: flag values {:?}, packed-colour alphas {:?}",
            self.unnamed_flags, self.unnamed_alphas
        );
        println!(
            "  {} of {} unnamed colour components are exactly 0.5; of the rest, {} are not an exact n/255",
            self.unnamed_half,
            self.unnamed_components,
            self.unnamed_off_quantum
        );
        println!(
            "ambient components span {:.3} to {:.3}; colour components span {:.3} to {:.3}",
            self.ambient_range.0, self.ambient_range.1, self.color_range.0, self.color_range.1
        );
        println!("time of day: {:?}", self.time_of_day);
        println!(
            "{} of {} terrain suns travel downward (direction z < 0)",
            self.suns_pointing_down, self.suns_seen
        );
        println!("BlendTileData versions seen:");
        for (version, count) in &self.blend_versions {
            println!("  version {version:>2}: {count} files");
        }
        println!(
            "{} of {} BlendTileData cells have an out-of-range tile_index (>> 2 >= numBitmapTiles)",
            self.out_of_range_tile_cells, self.total_cells
        );
        if !self.out_of_range_tile_files.is_empty() {
            println!("  in these files (expected: exactly `map wor rhun`, both installs):");
            for f in &self.out_of_range_tile_files {
                println!("    {f}");
            }
        }
        if self.failures.is_empty() {
            println!("0 failures");
        } else {
            println!("{} failures:", self.failures.len());
            for f in &self.failures {
                println!("  {f}");
            }
        }
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn walk_install(root: &Path, totals: &mut Totals) {
    for path in big_archives(root) {
        let Ok(archive) = BigArchive::open(&path) else {
            continue;
        };
        let label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for entry in archive.entries() {
            let lower = entry.name.to_ascii_lowercase();
            if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                continue;
            }
            totals.files = totals.files.saturating_add(1);
            let name = format!("{label}::{}", entry.name);
            let Ok(raw) = archive.read_entry(entry) else {
                totals
                    .failures
                    .push(format!("{name}: unreadable BIG entry"));
                continue;
            };

            let Some(stream) = mint_sage_refpack::strip_ear_wrapper(&raw) else {
                totals.failures.push(format!("{name}: no EAR\\0 wrapper"));
                continue;
            };
            let body = match mint_sage_refpack::decompress(stream) {
                Ok(body) => body,
                Err(e) => {
                    totals.failures.push(format!("{name}: decompress: {e}"));
                    continue;
                }
            };
            totals.decompressed_bytes = totals.decompressed_bytes.saturating_add(body.len() as u64);

            match MapFile::parse_body(body) {
                Ok(map) => {
                    totals.parsed = totals.parsed.saturating_add(1);
                    tally(&name, &map, totals);
                }
                Err(e) => totals.failures.push(format!("{name}: {e}")),
            }
        }
    }
}

impl Default for Totals {
    fn default() -> Self {
        Self {
            files: 0,
            parsed: 0,
            decompressed_bytes: 0,
            failures: Vec::new(),
            objects: 0,
            heightmap_dims: BTreeMap::new(),
            boundary_matches_formula: 0,
            boundary_checked: 0,
            lighting_versions: BTreeMap::new(),
            lighting_exact_length: 0,
            lighting_length_checked: 0,
            unit_directions_1e2: 0,
            unit_directions_1e3: 0,
            lights_seen: 0,
            accent_ambients: 0,
            accents_seen: 0,
            unnamed_flags: BTreeMap::new(),
            unnamed_off_quantum: 0,
            unnamed_components: 0,
            unnamed_half: 0,
            unnamed_alphas: BTreeMap::new(),
            ambient_range: (f32::INFINITY, f32::NEG_INFINITY),
            color_range: (f32::INFINITY, f32::NEG_INFINITY),
            time_of_day: BTreeMap::new(),
            suns_pointing_down: 0,
            suns_seen: 0,
            blend_versions: BTreeMap::new(),
            out_of_range_tile_files: Vec::new(),
            out_of_range_tile_cells: 0,
            total_cells: 0,
        }
    }
}

fn expected_lighting_len(version: u16) -> Option<usize> {
    match version {
        7 => Some(1348),
        8 => Some(1360),
        _ => None,
    }
}

fn tally_lighting(map: &MapFile, totals: &mut Totals) {
    let lighting = &map.global_lighting;
    *totals
        .lighting_versions
        .entry(lighting.version)
        .or_default() += 1;

    if let Some(info) = map.chunks.iter().find(|c| c.name == "GlobalLighting") {
        if let Some(expected) = expected_lighting_len(lighting.version) {
            totals.lighting_length_checked = totals.lighting_length_checked.saturating_add(1);
            if info.len == expected {
                totals.lighting_exact_length = totals.lighting_exact_length.saturating_add(1);
            }
        }
    }

    *totals
        .unnamed_flags
        .entry(lighting.unnamed.bloom_enabled)
        .or_default() += 1;
    let alpha = u8::try_from(lighting.unnamed.packed_color >> 24).unwrap_or(u8::MAX);
    *totals.unnamed_alphas.entry(alpha).or_default() += 1;
    for color in lighting.unnamed.colors {
        for component in color {
            totals.unnamed_components = totals.unnamed_components.saturating_add(1);
            if component == 0.5 {
                totals.unnamed_half = totals.unnamed_half.saturating_add(1);
                continue;
            }
            let scaled = component * 255.0;
            if (scaled - scaled.round()).abs() > 1e-3 {
                totals.unnamed_off_quantum = totals.unnamed_off_quantum.saturating_add(1);
            }
        }
    }

    let label = match lighting.time_of_day {
        Some(time) => time.name().to_owned(),
        None => format!("out of range ({})", lighting.time_of_day_code),
    };
    *totals.time_of_day.entry(label).or_default() += 1;

    for time in TimeOfDay::ALL {
        let config = lighting.config(time);
        for set in [config.terrain, config.object, config.infantry] {
            for light in set {
                totals.lights_seen = totals.lights_seen.saturating_add(1);
                let [x, y, z] = light.direction;
                let length = (x * x + y * y + z * z).sqrt();
                if (length - 1.0).abs() <= 1e-2 {
                    totals.unit_directions_1e2 = totals.unit_directions_1e2.saturating_add(1);
                }
                if (length - 1.0).abs() <= 1e-3 {
                    totals.unit_directions_1e3 = totals.unit_directions_1e3.saturating_add(1);
                }
                for component in light.ambient {
                    totals.ambient_range.0 = totals.ambient_range.0.min(component);
                    totals.ambient_range.1 = totals.ambient_range.1.max(component);
                }
                for component in light.color {
                    totals.color_range.0 = totals.color_range.0.min(component);
                    totals.color_range.1 = totals.color_range.1.max(component);
                }
            }
        }
        for accent in config.terrain.iter().skip(1) {
            totals.accents_seen = totals.accents_seen.saturating_add(1);
            if accent.ambient.iter().any(|component| *component != 0.0) {
                totals.accent_ambients = totals.accent_ambients.saturating_add(1);
            }
        }
        if let Some(sun) = config.terrain.first() {
            totals.suns_seen = totals.suns_seen.saturating_add(1);
            if sun.direction[2] < 0.0 {
                totals.suns_pointing_down = totals.suns_pointing_down.saturating_add(1);
            }
        }
    }
}

fn tally(name: &str, map: &MapFile, totals: &mut Totals) {
    tally_lighting(map, totals);

    totals.objects = totals.objects.saturating_add(map.objects.len() as u64);

    let hm = &map.height_map;
    let key = (hm.width, hm.height);
    *totals.heightmap_dims.entry(key).or_default() += 1;
    if hm.width > 1 && hm.height > 1 {
        if let Some(first) = hm.boundaries.first() {
            totals.boundary_checked = totals.boundary_checked.saturating_add(1);
            let expected_x = i64::from(hm.width) - 2 * i64::from(hm.border_size);
            let expected_y = i64::from(hm.height) - 2 * i64::from(hm.border_size);
            if i64::from(first.x) == expected_x && i64::from(first.y) == expected_y {
                totals.boundary_matches_formula = totals.boundary_matches_formula.saturating_add(1);
            }
        }
    }

    let bt = &map.blend_tile;
    *totals.blend_versions.entry(bt.version).or_default() += 1;

    let bitmap_tiles = u16::try_from(bt.num_bitmap_tiles.max(0)).unwrap_or(u16::MAX);
    let mut out_of_range = 0u64;
    for &index in &bt.tile_index {
        totals.total_cells = totals.total_cells.saturating_add(1);
        if index >> 2 >= bitmap_tiles {
            out_of_range = out_of_range.saturating_add(1);
        }
    }
    if out_of_range > 0 {
        totals.out_of_range_tile_cells =
            totals.out_of_range_tile_cells.saturating_add(out_of_range);
        totals.out_of_range_tile_files.push(format!(
            "{name}: {out_of_range} of {} cells",
            bt.tile_index.len()
        ));
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
