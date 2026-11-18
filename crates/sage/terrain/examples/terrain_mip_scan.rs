use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_render::{MipLevel, TextureData};
use mint_sage_big::BigArchive;
use mint_sage_map::{BlendTileData, MapFile, TextureClass};
use mint_sage_terrain::{class_mip_levels, load_class_textures, TerrainTypes, TileLayout};
use mint_vfs::Vfs;

const TERRAIN_INI: &str = "data/ini/terrain.ini";

const PROBE_LEVELS: usize = 11;

const ROUNDING_BOUND: f64 = 2.5;

const EDGE_EPSILON: f64 = 1e-6;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_mip_scan <install dir> [<install dir> ...]");
        eprintln!("  installations are mounted in order; later ones shadow earlier ones");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        let mut archives = 0usize;
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
                archives = archives.saturating_add(1);
            }
        }
        println!("mounted {archives} archive(s) from {}", root.display());
    }
    println!("{} entries visible", vfs.len());

    let Ok(ini) = vfs.read(TERRAIN_INI) else {
        eprintln!("{TERRAIN_INI}: not present in the mounted archives");
        return ExitCode::FAILURE;
    };
    let types = TerrainTypes::parse(&String::from_utf8_lossy(&ini));
    println!(
        "{TERRAIN_INI}: {} terrain type(s) with a texture, {} redefinition(s)\n",
        types.len(),
        types.redefinitions()
    );

    let started = Instant::now();
    let mut walk = Walk::default();
    for root in &roots {
        walk_install(root, &mut walk);
    }
    println!(
        "{} .map/.bse files, {} parsed, {} class reference(s), {} cell(s), in {:.1}s",
        walk.files,
        walk.parsed,
        walk.references,
        walk.cells,
        started.elapsed().as_secs_f64()
    );
    for failure in &walk.failures {
        println!("  FAILED {failure}");
    }

    let measured = Instant::now();
    let report = measure_all(&vfs, &types, &walk);
    report.print(&walk, measured.elapsed().as_secs_f64());

    if walk.failures.is_empty() && report.is_clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

type Shape = (String, i32);

#[derive(Default)]
struct Walk {
    files: u64,
    parsed: u64,
    failures: Vec<String>,
    references: u64,
    cells: u64,
    shapes: BTreeMap<Shape, (u64, u64)>,
}

fn walk_install(root: &Path, walk: &mut Walk) {
    for archive_path in big_archives(root) {
        let Ok(archive) = BigArchive::open(&archive_path) else {
            continue;
        };
        let entries: Vec<_> = archive
            .entries()
            .iter()
            .filter(|entry| {
                let lower = entry.name.to_ascii_lowercase();
                lower.ends_with(".map") || lower.ends_with(".bse")
            })
            .cloned()
            .collect();
        for entry in entries {
            walk.files = walk.files.saturating_add(1);
            let label = format!("{} :: {}", archive_path.display(), entry.name);
            let raw = match archive.read_entry(&entry) {
                Ok(raw) => raw,
                Err(error) => {
                    walk.failures.push(format!("{label}: {error}"));
                    continue;
                }
            };
            match MapFile::parse(&raw) {
                Ok(map) => {
                    walk.parsed = walk.parsed.saturating_add(1);
                    collect(&map.blend_tile, walk);
                }
                Err(error) => walk.failures.push(format!("{label}: {error}")),
            }
        }
    }
}

fn collect(blend_tile: &BlendTileData, walk: &mut Walk) {
    walk.cells = walk.cells.saturating_add(u64::from(blend_tile.cell_count));
    walk.references = walk
        .references
        .saturating_add(blend_tile.texture_classes.len() as u64);

    let layout = TileLayout::new(&blend_tile.texture_classes);
    let mut per_class = vec![0u64; blend_tile.texture_classes.len()];
    for value in &blend_tile.tile_index {
        if let Some(cell) = layout.cell(*value) {
            if let Some(slot) = per_class.get_mut(cell.class) {
                *slot = slot.saturating_add(1);
            }
        }
    }
    for (index, class) in blend_tile.texture_classes.iter().enumerate() {
        let slot = walk
            .shapes
            .entry((class.name.clone(), class.width))
            .or_default();
        slot.0 = slot.0.saturating_add(1);
        slot.1 = slot
            .1
            .saturating_add(per_class.get(index).copied().unwrap_or(0));
    }
}

#[derive(Default, Clone, Copy)]
struct LevelStats {
    shapes: u64,
    rectangles: u64,
    texels: u64,
    uncontained: u64,
    out_of_range: u64,
    excess_sum: f64,
    excess_max: f64,
    drift_sum: f64,
    drift_max: f64,
}

#[derive(Default)]
struct Report {
    caps: BTreeMap<usize, (u64, u64, u64)>,
    single: Vec<String>,
    missing: u64,
    levels: Vec<LevelStats>,
    kept: LevelStats,
    kept_unaligned: LevelStats,
    first_dropped: LevelStats,
    misplaced: Vec<String>,
    bytes_level_zero: u64,
    bytes_with_chain: u64,
}

impl Report {
    fn is_clean(&self) -> bool {
        self.misplaced.is_empty()
            && self.kept.uncontained == 0
            && self.kept.out_of_range == 0
            && self.kept.drift_max < ROUNDING_BOUND
            && self.first_dropped.texels > 0
            && self.first_dropped.uncontained == self.first_dropped.texels
    }

    fn print(&self, walk: &Walk, seconds: f64) {
        println!(
            "\n{} distinct (terrain type, tile grid) shape(s); {} did not resolve to a texture",
            walk.shapes.len(),
            self.missing
        );
        println!("\nlevels given, by shape:");
        println!("  levels   shapes   class refs        cells");
        for (levels, (shapes, references, cells)) in &self.caps {
            println!("  {levels:6}   {shapes:6}   {references:10}   {cells:11}");
        }
        for name in &self.single {
            println!("  single level: {name}");
        }

        println!(
            "\nbleed by level, over every quarter-tile rectangle of every shape ({seconds:.1}s):"
        );
        println!(
            "  level  shapes   rectangles       texels   uncontained   mean drift      max   out of range   max"
        );
        for (level, stats) in self.levels.iter().enumerate() {
            if stats.texels == 0 {
                continue;
            }
            print_row(&format!("  {level:5}  {:6}", stats.shapes), stats);
        }
        print_row("  kept, aligned ", &self.kept);
        print_row("  kept, unaligned", &self.kept_unaligned);
        print_row("  first dropped ", &self.first_dropped);

        let before = self.bytes_level_zero as f64;
        let after = self.bytes_with_chain as f64;
        println!(
            "\ntexture bytes over all shapes: {:.1} MiB at one level, {:.1} MiB with the chain (x{:.4})",
            before / (1024.0 * 1024.0),
            after / (1024.0 * 1024.0),
            if before > 0.0 { after / before } else { 0.0 }
        );

        if self.misplaced.is_empty() {
            println!("chain length agrees with the rule on every shape");
        } else {
            for name in &self.misplaced {
                println!("  MISPLACED {name}");
            }
        }
        if self.is_clean() {
            println!(
                "the cut is the deepest bleed-free level: nothing kept bleeds, the next one does"
            );
        } else {
            println!("FAILED: see the columns above");
        }
    }
}

fn print_row(label: &str, stats: &LevelStats) {
    println!(
        "{label}   {:10}   {:10}   {:11}   {:10.3}   {:6.2}   {:12}   {:3.0}",
        stats.rectangles,
        stats.texels,
        stats.uncontained,
        stats.mean_drift(),
        stats.drift_max,
        stats.out_of_range,
        stats.excess_max
    );
}

impl LevelStats {
    fn mean_drift(&self) -> f64 {
        if self.texels == 0 {
            0.0
        } else {
            self.drift_sum / (self.texels as f64)
        }
    }

    fn add(&mut self, other: &Self) {
        self.rectangles = self.rectangles.saturating_add(other.rectangles);
        self.texels = self.texels.saturating_add(other.texels);
        self.uncontained = self.uncontained.saturating_add(other.uncontained);
        self.out_of_range = self.out_of_range.saturating_add(other.out_of_range);
        self.excess_sum += other.excess_sum;
        self.excess_max = self.excess_max.max(other.excess_max);
        self.drift_sum += other.drift_sum;
        self.drift_max = self.drift_max.max(other.drift_max);
    }
}

fn measure_all(vfs: &Vfs, types: &TerrainTypes, walk: &Walk) -> Report {
    let mut report = Report {
        levels: vec![LevelStats::default(); PROBE_LEVELS],
        ..Report::default()
    };

    for ((name, tiles), (references, cells)) in &walk.shapes {
        let class = TextureClass {
            first_tile: 0,
            num_tiles: tiles.saturating_mul(*tiles),
            width: *tiles,
            name: name.clone(),
        };
        let loaded = load_class_textures(vfs, types, std::slice::from_ref(&class));
        let Some(Some(texture)) = loaded.textures.first() else {
            report.missing = report.missing.saturating_add(1);
            continue;
        };
        let kept = texture.levels.len();
        let slot = report.caps.entry(kept).or_default();
        slot.0 = slot.0.saturating_add(1);
        slot.1 = slot.1.saturating_add(*references);
        slot.2 = slot.2.saturating_add(*cells);
        if kept == 1 {
            report.single.push(format!(
                "{name} ({}x{}, {tiles} tiles across): {references} reference(s), {cells} cell(s)",
                texture.width(),
                texture.height()
            ));
        }
        if kept != class_mip_levels(texture.width(), texture.height(), *tiles) {
            report.misplaced.push(format!(
                "{name}: loader kept {kept} level(s), the rule says {}",
                class_mip_levels(texture.width(), texture.height(), *tiles)
            ));
        }

        for (index, level) in texture.levels.iter().enumerate() {
            let bytes = level.data.len() as u64;
            report.bytes_with_chain = report.bytes_with_chain.saturating_add(bytes);
            if index == 0 {
                report.bytes_level_zero = report.bytes_level_zero.saturating_add(bytes);
            }
        }

        let Some(base) = texture.levels.first() else {
            continue;
        };
        let mut full =
            TextureData::single(texture.format, base.width, base.height, base.data.clone());
        full.generate_mipmaps(PROBE_LEVELS);

        let aligned = rectangles_are_whole_texels(texture.width(), texture.height(), *tiles);
        let per_level = measure_shape(&full, *tiles);
        for (index, stats) in per_level.iter().enumerate() {
            if stats.texels == 0 {
                continue;
            }
            if let Some(slot) = report.levels.get_mut(index) {
                slot.add(stats);
                slot.shapes = slot.shapes.saturating_add(1);
            }
            if index < kept {
                if aligned {
                    report.kept.add(stats);
                } else {
                    report.kept_unaligned.add(stats);
                }
            } else if index == kept && aligned {
                report.first_dropped.add(stats);
            }
        }
        if aligned {
            report.kept.shapes = report.kept.shapes.saturating_add(1);
        } else {
            report.kept_unaligned.shapes = report.kept_unaligned.shapes.saturating_add(1);
        }
    }
    report
}

fn rectangles_are_whole_texels(width: u32, height: u32, tiles: i32) -> bool {
    let Ok(tiles) = u32::try_from(tiles) else {
        return false;
    };
    let Some(across) = tiles.checked_mul(2).filter(|n| *n > 0) else {
        return false;
    };
    width.checked_rem(across) == Some(0) && height.checked_rem(across) == Some(0)
}

fn measure_shape(texture: &TextureData, tiles: i32) -> Vec<LevelStats> {
    let mut out = vec![LevelStats::default(); texture.levels.len()];
    let Ok(tiles_u) = u32::try_from(tiles) else {
        return out;
    };
    let Some(across) = tiles_u.checked_mul(2).filter(|n| *n > 0) else {
        return out;
    };
    let (width, height) = (texture.width(), texture.height());
    let Some(base) = texture.levels.first() else {
        return out;
    };
    if width == 0 || height == 0 {
        return out;
    }

    for ry in 0..across {
        for rx in 0..across {
            let bounds = (
                f64::from(rx) * f64::from(width) / f64::from(across),
                f64::from(rx.saturating_add(1)) * f64::from(width) / f64::from(across),
                f64::from(ry) * f64::from(height) / f64::from(across),
                f64::from(ry.saturating_add(1)) * f64::from(height) / f64::from(across),
            );
            let Some(range) = own_range(base, bounds) else {
                continue;
            };
            for (index, level) in texture.levels.iter().enumerate() {
                let Some(stats) = out.get_mut(index) else {
                    break;
                };
                stats.rectangles = stats.rectangles.saturating_add(1);
                accumulate(level, base, width, height, bounds, range, stats);
            }
        }
    }
    out
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn own_range(base: &MipLevel, bounds: (f64, f64, f64, f64)) -> Option<([u8; 4], [u8; 4])> {
    let (x0, x1, y0, y1) = bounds;
    let (x0, x1) = (x0.floor() as u32, x1.ceil() as u32);
    let (y0, y1) = (y0.floor() as u32, y1.ceil() as u32);
    let mut min = [u8::MAX; 4];
    let mut max = [0u8; 4];
    let mut seen = false;
    for y in y0..y1.min(base.height) {
        for x in x0..x1.min(base.width) {
            let Some(texel) = texel_at(base, x, y) else {
                continue;
            };
            seen = true;
            for (channel, value) in texel.iter().enumerate() {
                if let Some(slot) = min.get_mut(channel) {
                    *slot = (*slot).min(*value);
                }
                if let Some(slot) = max.get_mut(channel) {
                    *slot = (*slot).max(*value);
                }
            }
        }
    }
    seen.then_some((min, max))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn region_mean(base: &MipLevel, bounds: (f64, f64, f64, f64)) -> Option<[f64; 3]> {
    let (x0, x1, y0, y1) = bounds;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut sums = [0.0f64; 3];
    let mut count = 0u64;
    for y in (y0.floor().max(0.0) as u32)..(y1.ceil() as u32).min(base.height) {
        for x in (x0.floor().max(0.0) as u32)..(x1.ceil() as u32).min(base.width) {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            if cx < x0 || cx >= x1 || cy < y0 || cy >= y1 {
                continue;
            }
            let Some(texel) = texel_at(base, x, y) else {
                continue;
            };
            for (sum, value) in sums.iter_mut().zip(texel) {
                *sum += f64::from(*value);
            }
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        return None;
    }
    let n = count as f64;
    Some([sums[0] / n, sums[1] / n, sums[2] / n])
}

fn texel_at(level: &MipLevel, x: u32, y: u32) -> Option<&[u8]> {
    let index = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(level.width).ok()?)?
        .checked_add(usize::try_from(x).ok()?)?
        .checked_mul(4)?;
    level.data.get(index..index.checked_add(4)?)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn accumulate(
    level: &MipLevel,
    base: &MipLevel,
    width: u32,
    height: u32,
    bounds: (f64, f64, f64, f64),
    range: ([u8; 4], [u8; 4]),
    stats: &mut LevelStats,
) {
    let (rx0, rx1, ry0, ry1) = bounds;
    if level.width == 0 || level.height == 0 {
        return;
    }
    let step_x = f64::from(width) / f64::from(level.width);
    let step_y = f64::from(height) / f64::from(level.height);
    let first_x = (rx0 / step_x).floor().max(0.0) as u32;
    let first_y = (ry0 / step_y).floor().max(0.0) as u32;
    let last_x = ((rx1 / step_x).ceil() as u32).min(level.width);
    let last_y = ((ry1 / step_y).ceil() as u32).min(level.height);

    for ty in first_y..last_y {
        for tx in first_x..last_x {
            let (fx0, fx1) = (
                f64::from(tx) * step_x,
                f64::from(tx.saturating_add(1)) * step_x,
            );
            let (fy0, fy1) = (
                f64::from(ty) * step_y,
                f64::from(ty.saturating_add(1)) * step_y,
            );
            if fx1 <= rx0 + EDGE_EPSILON
                || fx0 >= rx1 - EDGE_EPSILON
                || fy1 <= ry0 + EDGE_EPSILON
                || fy0 >= ry1 - EDGE_EPSILON
            {
                continue;
            }
            stats.texels = stats.texels.saturating_add(1);
            if !(fx0 >= rx0 - EDGE_EPSILON
                && fx1 <= rx1 + EDGE_EPSILON
                && fy0 >= ry0 - EDGE_EPSILON
                && fy1 <= ry1 + EDGE_EPSILON)
            {
                stats.uncontained = stats.uncontained.saturating_add(1);
            }
            let Some(texel) = texel_at(level, tx, ty) else {
                continue;
            };
            if let Some(ideal) = region_mean(
                base,
                (fx0.max(rx0), fx1.min(rx1), fy0.max(ry0), fy1.min(ry1)),
            ) {
                let mut drift = 0.0f64;
                for channel in 0..3usize {
                    let (Some(value), Some(want)) =
                        (texel.get(channel).copied(), ideal.get(channel).copied())
                    else {
                        continue;
                    };
                    drift += (f64::from(value) - want).abs();
                }
                drift /= 3.0;
                stats.drift_sum += drift;
                stats.drift_max = stats.drift_max.max(drift);
            }
            let mut excess = 0.0f64;
            for channel in 0..3usize {
                let (Some(value), Some(low), Some(high)) = (
                    texel.get(channel).copied(),
                    range.0.get(channel).copied(),
                    range.1.get(channel).copied(),
                ) else {
                    continue;
                };
                let over = if value > high {
                    f64::from(value) - f64::from(high)
                } else if value < low {
                    f64::from(low) - f64::from(value)
                } else {
                    0.0
                };
                excess = excess.max(over);
            }
            if excess > 0.0 {
                stats.out_of_range = stats.out_of_range.saturating_add(1);
                stats.excess_sum += excess;
                stats.excess_max = stats.excess_max.max(excess);
            }
        }
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
