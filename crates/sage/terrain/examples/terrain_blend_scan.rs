use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_render::TextureData;
use mint_sage_big::BigArchive;
use mint_sage_map::{BlendRecord, BlendTileData, HeightMapData, MapFile};
use mint_sage_terrain::{
    corner_mask, corner_ramp, load_class_textures, quad_folds_along_ad, ramp_alpha, CellTile,
    TerrainTypes, TileLayout,
};
use mint_vfs::Vfs;

const TERRAIN_INI: &str = "data/ini/terrain.ini";

const SEAM_STRIDE: usize = 2;

const SEAM_SAMPLES: u16 = 4;

struct Candidate {
    name: &'static str,
    alpha: Option<fn(&BlendRecord) -> [f32; 4]>,
    swap_layers: bool,
    wallpaper: bool,
}

fn mask_alpha(record: &BlendRecord) -> [f32; 4] {
    corner_mask(record).map(|covered| if covered { 1.0 } else { 0.0 })
}

fn inverted_alpha(record: &BlendRecord) -> [f32; 4] {
    mask_alpha(record).map(|a| 1.0 - a)
}

fn mirrored_alpha(record: &BlendRecord) -> [f32; 4] {
    let a = mask_alpha(record);
    [
        a.get(3).copied().unwrap_or(0.0),
        a.get(2).copied().unwrap_or(0.0),
        a.get(1).copied().unwrap_or(0.0),
        a.first().copied().unwrap_or(0.0),
    ]
}

fn without_inverted(record: &BlendRecord) -> [f32; 4] {
    let mut record = *record;
    record.inverted = false;
    mask_alpha(&record)
}

fn always_inverted(record: &BlendRecord) -> [f32; 4] {
    let mut record = *record;
    record.inverted = true;
    mask_alpha(&record)
}

fn without_long_diagonal(record: &BlendRecord) -> [f32; 4] {
    let mut record = *record;
    record.long_diagonal = 0;
    mask_alpha(&record)
}

const CANDIDATES: &[Candidate] = &[
    Candidate {
        name: "implemented: corner mask, blend then extra",
        alpha: Some(mask_alpha),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "no blending at all (base tiles only)",
        alpha: None,
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "alpha inverted",
        alpha: Some(inverted_alpha),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "each ramp mirrored",
        alpha: Some(mirrored_alpha),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "inverted flag ignored",
        alpha: Some(without_inverted),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "inverted flag always applied",
        alpha: Some(always_inverted),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "long-diagonal byte ignored",
        alpha: Some(without_long_diagonal),
        swap_layers: false,
        wallpaper: false,
    },
    Candidate {
        name: "layer order swapped: extra then blend",
        alpha: Some(mask_alpha),
        swap_layers: true,
        wallpaper: false,
    },
    Candidate {
        name: "overlay at the cell's wallpaper position",
        alpha: Some(mask_alpha),
        swap_layers: false,
        wallpaper: true,
    },
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_blend_scan <install dir> [<install dir> ...]");
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
        "{TERRAIN_INI}: {} terrain type(s) with a texture\n",
        types.len()
    );

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        walk_install(root, &vfs, &types, &mut totals);
    }
    totals.report(started.elapsed().as_secs_f64());

    if totals.failures.is_empty() && totals.model_is_best() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct Totals {
    files: u64,
    parsed: u64,
    failures: Vec<String>,

    cells: u64,
    blended_cells: u64,
    extra_cells: u64,
    extra_outside_blend: u64,
    index_past_records: u64,
    direction_agrees: u64,
    direction_checked: u64,
    per_flag: BTreeMap<&'static str, (u64, u64)>,

    sum: Vec<f64>,
    n: Vec<u64>,
    blended_sum: Vec<f64>,
    blended_n: Vec<u64>,

    noise_sum: f64,
    noise_n: u64,
    same_class_sum: f64,
    same_class_n: u64,

    class_textures: u64,
    class_textures_alpha: u64,
    overlays_on_alpha_class: u64,
    overlays_total: u64,
    wallpaper_agrees: u64,
    wallpaper_checked: u64,

    interior_buckets: BTreeMap<(usize, bool), u64>,
}

const INTERIOR_STEPS: usize = 16;

const ALPHA_TILES: [(&str, bool, u8); 12] = [
    ("horizontal", false, 0),
    ("horizontal inverted", true, 0),
    ("vertical", false, 0),
    ("vertical inverted", true, 0),
    ("right diagonal", false, 0),
    ("right diagonal inverted", true, 0),
    ("right diagonal long", false, 1),
    ("right diag inv long", true, 1),
    ("left diagonal", false, 0),
    ("left diagonal inverted", true, 0),
    ("left diagonal long", false, 1),
    ("left diag inv long", true, 1),
];

fn alpha_tile_record(slot: usize) -> Option<BlendRecord> {
    let (name, inverted, long) = ALPHA_TILES.get(slot).copied()?;
    Some(BlendRecord {
        tile_index: 0,
        horizontal: name.starts_with("horizontal"),
        vertical: name.starts_with("vertical"),
        right_diagonal: name.starts_with("right"),
        left_diagonal: name.starts_with("left"),
        inverted,
        long_diagonal: long,
        custom_blend_edge_class: -1,
    })
}

fn alpha_tile_slot(record: &BlendRecord) -> Option<usize> {
    let long = record.long_diagonal != 0;
    let base: usize = if record.horizontal {
        0
    } else if record.vertical {
        2
    } else if record.right_diagonal {
        if long {
            6
        } else {
            4
        }
    } else if record.left_diagonal {
        if long {
            10
        } else {
            8
        }
    } else {
        return None;
    };
    Some(base.saturating_add(usize::from(record.inverted)))
}

fn corner_model_alpha(mask: [f32; 4], folds_ad: bool, s: f32, t: f32) -> f32 {
    let m = |slot: usize| mask.get(slot).copied().unwrap_or(0.0);
    if folds_ad {
        if t <= s {
            m(0) * (1.0 - s) + m(1) * (s - t) + m(3) * t
        } else {
            m(0) * (1.0 - t) + m(3) * s + m(2) * (t - s)
        }
    } else {
        if s + t <= 1.0 {
            m(0) * (1.0 - s - t) + m(1) * s + m(2) * t
        } else {
            m(1) * (1.0 - t) + m(3) * (s + t - 1.0) + m(2) * (1.0 - s)
        }
    }
}

fn interior_difference(slot: usize, folds_ad: bool) -> (f64, f64) {
    let Some(record) = alpha_tile_record(slot) else {
        return (0.0, 0.0);
    };
    let ramp = corner_ramp(&record);
    let mask = corner_mask(&record).map(|covered| if covered { 1.0f32 } else { 0.0 });
    let mut sum = 0.0f64;
    let mut worst = 0.0f64;
    let mut n = 0u64;
    for j in 0..INTERIOR_STEPS {
        for i in 0..INTERIOR_STEPS {
            let s = (i as f32 + 0.5) / INTERIOR_STEPS as f32;
            let t = (j as f32 + 0.5) / INTERIOR_STEPS as f32;
            let difference = f64::from(
                (ramp_alpha(ramp, s, t) - corner_model_alpha(mask, folds_ad, s, t)).abs(),
            );
            sum += difference;
            worst = worst.max(difference);
            n = n.saturating_add(1);
        }
    }
    (sum / (n.max(1) as f64), worst)
}

impl Totals {
    fn mean(sum: Option<&f64>, n: Option<&u64>) -> f64 {
        let (Some(&sum), Some(&n)) = (sum, n) else {
            return f64::INFINITY;
        };
        if n == 0 {
            return f64::INFINITY;
        }
        sum / (n as f64)
    }

    fn all(&self, index: usize) -> f64 {
        Self::mean(self.sum.get(index), self.n.get(index))
    }

    fn blended(&self, index: usize) -> f64 {
        Self::mean(self.blended_sum.get(index), self.blended_n.get(index))
    }

    fn model_is_best(&self) -> bool {
        let best = self.blended(0);
        best.is_finite() && (1..CANDIDATES.len()).all(|other| self.blended(other) > best)
    }

    fn add(&mut self, index: usize, difference: f64, blended: bool) {
        while self.sum.len() <= index {
            self.sum.push(0.0);
            self.n.push(0);
            self.blended_sum.push(0.0);
            self.blended_n.push(0);
        }
        if let (Some(sum), Some(n)) = (self.sum.get_mut(index), self.n.get_mut(index)) {
            *sum += difference;
            *n = n.saturating_add(1);
        }
        if blended {
            if let (Some(sum), Some(n)) = (
                self.blended_sum.get_mut(index),
                self.blended_n.get_mut(index),
            ) {
                *sum += difference;
                *n = n.saturating_add(1);
            }
        }
    }

    fn report_interior(&self) {
        let overlays: u64 = self.interior_buckets.values().copied().sum();
        let mut weighted = 0.0f64;
        let mut differing = 0u64;
        let mut rows: Vec<(f64, f64, u64, usize, bool)> = Vec::new();
        for slot in 0..ALPHA_TILES.len() {
            for folds_ad in [false, true] {
                let count = self
                    .interior_buckets
                    .get(&(slot, folds_ad))
                    .copied()
                    .unwrap_or(0);
                let (mean, worst) = interior_difference(slot, folds_ad);
                weighted += mean * count as f64;
                if worst > 1e-6 {
                    differing = differing.saturating_add(count);
                }
                rows.push((mean, worst, count, slot, folds_ad));
            }
        }
        println!(
            "
the alpha inside a cell, against the original's prepared alpha tiles, {} interior samples per cell:",
            INTERIOR_STEPS.saturating_mul(INTERIOR_STEPS)
        );
        println!(
            "  implemented: the ramp carried unclamped, saturated per fragment      mean 0.000  (exact, either fold)"
        );
        println!(
            "  previous:    four clamped corners interpolated over the quad's fold  mean {:.3}",
            weighted / overlays.max(1) as f64
        );
        println!(
            "  overlays whose interior the previous model drew differently: {} of {} ({:.1}%)",
            differing,
            overlays,
            100.0 * differing as f64 / overlays.max(1) as f64
        );
        rows.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("  worst prepared tile / fold pairs:");
        for (mean, worst, count, slot, folds_ad) in rows.iter().take(6) {
            let name = ALPHA_TILES.get(*slot).map_or("?", |entry| entry.0);
            println!(
                "    {name:<24} fold {}  mean {mean:.3}  worst {worst:.3}  on {count:>9} overlay(s)",
                if *folds_ad { "A-D" } else { "B-C" }
            );
        }
    }

    fn report(&self, seconds: f64) {
        println!(
            "\n{} .map/.bse files, {} parsed, in {seconds:.1}s",
            self.files, self.parsed
        );
        for failure in &self.failures {
            println!("  FAILED {failure}");
        }
        println!(
            "cells: {} total, {} with a blend, {} with an extra blend ({} of those outside a blend)",
            self.cells, self.blended_cells, self.extra_cells, self.extra_outside_blend
        );
        println!(
            "blend/extra indices past the record list: {}",
            self.index_past_records
        );
        println!(
            "class textures carrying a non-opaque alpha channel: {} of {} class reference(s);              {} of {} overlays ({:.1}%) draw one",
            self.class_textures_alpha,
            self.class_textures,
            self.overlays_on_alpha_class,
            self.overlays_total,
            100.0 * self.overlays_on_alpha_class as f64 / self.overlays_total.max(1) as f64
        );
        println!(
            "  the channel is discarded at load; this seam measurement cannot see it either way"
        );
        println!(
            "
the record's tile is the cell's own wallpaper position in the overlay's class:              {} of {} ({:.3}%)",
            self.wallpaper_agrees,
            self.wallpaper_checked,
            100.0 * self.wallpaper_agrees as f64 / self.wallpaper_checked.max(1) as f64
        );
        println!(
            "\nthe record names the class of the neighbour its flags point at: {} of {} ({:.1}%)",
            self.direction_agrees,
            self.direction_checked,
            100.0 * self.direction_agrees as f64 / self.direction_checked.max(1) as f64
        );
        for (flag, (ok, total)) in &self.per_flag {
            println!(
                "  {flag:<22} {ok:>9} of {total:>9}  {:.1}%",
                100.0 * *ok as f64 / (*total).max(1) as f64
            );
        }

        self.report_interior();

        println!(
            "\nclass-boundary seam: {} sampled texel pairs per candidate, {} of them where \
             at least one side carries a blend",
            self.n.first().copied().unwrap_or(0),
            self.blended_n.first().copied().unwrap_or(0)
        );
        println!("                                                 all   blended");
        println!(
            "  texture noise floor                      {:9.3} {:9.3}   (unreachable: the two \
             sides are different images)",
            Self::mean(Some(&self.noise_sum), Some(&self.noise_n)),
            Self::mean(Some(&self.noise_sum), Some(&self.noise_n))
        );
        println!(
            "  same-class floor                                   {:9.3}   (overlay tile vs the \
             neighbour's own tile)",
            Self::mean(Some(&self.same_class_sum), Some(&self.same_class_n))
        );
        for (index, candidate) in CANDIDATES.iter().enumerate() {
            let marker = if index == 0 { "->" } else { "  " };
            println!(
                "  {marker} {:<38} {:9.3} {:9.3}",
                candidate.name,
                self.all(index),
                self.blended(index)
            );
        }
        if self.model_is_best() {
            println!(
                "  the implemented model is the lowest of the {}",
                CANDIDATES.len()
            );
        } else {
            println!("  FAILED: the implemented model is not the lowest");
        }
    }
}

fn walk_install(root: &Path, vfs: &Vfs, types: &TerrainTypes, totals: &mut Totals) {
    for archive_path in big_archives(root) {
        let Ok(archive) = BigArchive::open(&archive_path) else {
            continue;
        };
        let names: Vec<String> = archive
            .entries()
            .iter()
            .map(|entry| entry.name.clone())
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                lower.ends_with(".map") || lower.ends_with(".bse")
            })
            .collect();
        for name in names {
            totals.files = totals.files.saturating_add(1);
            let label = format!("{} :: {name}", archive_path.display());
            let Some(entry) = archive.entries().iter().find(|e| e.name == name).cloned() else {
                continue;
            };
            let raw = match archive.read_entry(&entry) {
                Ok(raw) => raw,
                Err(error) => {
                    totals.failures.push(format!("{label}: {error}"));
                    continue;
                }
            };
            match MapFile::parse(&raw) {
                Ok(map) => {
                    totals.parsed = totals.parsed.saturating_add(1);
                    check_map(&map.height_map, &map.blend_tile, vfs, types, totals);
                }
                Err(error) => totals.failures.push(format!("{label}: {error}")),
            }
        }
    }
}

fn class_of(blend_tile: &BlendTileData, tile: i32) -> Option<usize> {
    blend_tile.texture_classes.iter().position(|class| {
        tile >= class.first_tile && tile < class.first_tile.saturating_add(class.num_tiles)
    })
}

fn direction(record: &BlendRecord) -> Option<(i64, i64)> {
    let dy: i64 = if record.inverted { -1 } else { 1 };
    if record.horizontal {
        Some((dy, 0))
    } else if record.vertical {
        Some((0, dy))
    } else if record.right_diagonal {
        Some((1, dy))
    } else if record.left_diagonal {
        Some((-1, dy))
    } else {
        None
    }
}

fn flag_name(record: &BlendRecord) -> &'static str {
    let long = record.long_diagonal != 0;
    match (
        record.horizontal,
        record.vertical,
        record.right_diagonal,
        record.inverted,
        long,
    ) {
        (true, _, _, false, _) => "horizontal",
        (true, _, _, true, _) => "horizontal inverted",
        (_, true, _, false, _) => "vertical",
        (_, true, _, true, _) => "vertical inverted",
        (_, _, true, false, false) => "right diagonal",
        (_, _, true, true, false) => "right diag inverted",
        (_, _, true, false, true) => "right diag long",
        (_, _, true, true, true) => "right diag inv long",
        (_, _, false, false, false) => "left diagonal",
        (_, _, false, true, false) => "left diag inverted",
        (_, _, false, false, true) => "left diag long",
        (_, _, false, true, true) => "left diag inv long",
    }
}

fn cell_index(blend_tile: &BlendTileData, x: i64, y: i64) -> Option<usize> {
    if x < 0 || y < 0 || x >= i64::from(blend_tile.width) || y >= i64::from(blend_tile.height) {
        return None;
    }
    usize::try_from(y.checked_mul(i64::from(blend_tile.width))?.checked_add(x)?).ok()
}

fn base_class(blend_tile: &BlendTileData, x: i64, y: i64) -> Option<usize> {
    let index = cell_index(blend_tile, x, y)?;
    let value = blend_tile.tile_index.get(index).copied()?;
    class_of(blend_tile, i32::from(value >> 2))
}

fn check_map(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    vfs: &Vfs,
    types: &TerrainTypes,
    totals: &mut Totals,
) {
    let loaded = load_class_textures(vfs, types, &blend_tile.texture_classes);
    let layout = TileLayout::new(&blend_tile.texture_classes);
    let offsets = wallpaper_offsets(blend_tile);
    totals.class_textures = totals
        .class_textures
        .saturating_add(u64::try_from(loaded.textures.iter().flatten().count()).unwrap_or(0));
    totals.class_textures_alpha = totals
        .class_textures_alpha
        .saturating_add(u64::try_from(loaded.alpha_discarded.len()).unwrap_or(0));
    totals.cells = totals
        .cells
        .saturating_add(u64::from(blend_tile.cell_count));
    let records = &blend_tile.blend_records;
    let record_count = u32::try_from(records.len()).unwrap_or(u32::MAX);

    for y in 0..i64::from(blend_tile.height) {
        for x in 0..i64::from(blend_tile.width) {
            let Some(index) = cell_index(blend_tile, x, y) else {
                continue;
            };
            let blend = blend_tile.blend_index.get(index).copied().unwrap_or(0);
            let extra = blend_tile
                .extra_blend_index
                .get(index)
                .copied()
                .unwrap_or(0);
            if blend != 0 {
                totals.blended_cells = totals.blended_cells.saturating_add(1);
            }
            if extra != 0 {
                totals.extra_cells = totals.extra_cells.saturating_add(1);
                if blend == 0 {
                    totals.extra_outside_blend = totals.extra_outside_blend.saturating_add(1);
                }
            }
            if blend > record_count || extra > record_count {
                totals.index_past_records = totals.index_past_records.saturating_add(1);
                continue;
            }
            for value in [blend, extra] {
                let Some(record) = usize::try_from(value)
                    .ok()
                    .and_then(|slot| slot.checked_sub(1))
                    .and_then(|slot| records.get(slot))
                else {
                    continue;
                };
                let Some(tile) = u16::try_from(record.tile_index)
                    .ok()
                    .and_then(|raw| layout.cell(raw))
                else {
                    continue;
                };
                totals.overlays_total = totals.overlays_total.saturating_add(1);
                if let (Some(slot), Ok(cx), Ok(cy)) =
                    (alpha_tile_slot(record), u32::try_from(x), u32::try_from(y))
                {
                    let folds_ad = quad_folds_along_ad(height_map, cx, cy);
                    let bucket = totals.interior_buckets.entry((slot, folds_ad)).or_insert(0);
                    *bucket = bucket.saturating_add(1);
                }
                if loaded.alpha_discarded.contains(&tile.class) {
                    totals.overlays_on_alpha_class =
                        totals.overlays_on_alpha_class.saturating_add(1);
                }
                totals.wallpaper_checked = totals.wallpaper_checked.saturating_add(1);
                if wallpaper_tile(blend_tile, &layout, &offsets, tile.class, x, y)
                    .is_some_and(|wallpaper| wallpaper == tile)
                {
                    totals.wallpaper_agrees = totals.wallpaper_agrees.saturating_add(1);
                }
            }

            let Some(record) = usize::try_from(blend)
                .ok()
                .and_then(|slot| slot.checked_sub(1))
                .and_then(|slot| records.get(slot))
            else {
                continue;
            };
            let (Some((dx, dy)), Some(overlay_class)) = (
                direction(record),
                class_of(blend_tile, record.tile_index >> 2),
            ) else {
                continue;
            };
            totals.direction_checked = totals.direction_checked.saturating_add(1);
            let entry = totals.per_flag.entry(flag_name(record)).or_default();
            entry.1 = entry.1.saturating_add(1);
            if base_class(blend_tile, x.saturating_add(dx), y.saturating_add(dy))
                == Some(overlay_class)
            {
                totals.direction_agrees = totals.direction_agrees.saturating_add(1);
                entry.0 = entry.0.saturating_add(1);
            }
        }
    }

    measure_boundaries(blend_tile, &layout, &loaded.textures, &offsets, totals);
}

#[derive(Clone, Copy)]
struct Overlay {
    record_tile: CellTile,
    wallpaper_tile: CellTile,
    record: BlendRecord,
}

struct Cell {
    base: CellTile,
    blend: Option<Overlay>,
    extra: Option<Overlay>,
}

fn wallpaper_offsets(blend_tile: &BlendTileData) -> Vec<(i64, i64)> {
    let mut histograms: Vec<(Vec<u64>, Vec<u64>)> = blend_tile
        .texture_classes
        .iter()
        .map(|class| {
            let slots = usize::try_from(i64::from(class.width).saturating_mul(2)).unwrap_or(0);
            (vec![0u64; slots], vec![0u64; slots])
        })
        .collect();

    for y in 0..i64::from(blend_tile.height) {
        for x in 0..i64::from(blend_tile.width) {
            let Some(raw) =
                cell_index(blend_tile, x, y).and_then(|at| blend_tile.tile_index.get(at).copied())
            else {
                continue;
            };
            let Some(index) = class_of(blend_tile, i32::from(raw >> 2)) else {
                continue;
            };
            let (Some(class), Some((columns, rows))) = (
                blend_tile.texture_classes.get(index),
                histograms.get_mut(index),
            ) else {
                continue;
            };
            let width = i64::from(class.width);
            let period = width.saturating_mul(2);
            if period <= 0 {
                continue;
            }
            let local = i64::from(raw >> 2).saturating_sub(i64::from(class.first_tile));
            let half_x = local
                .rem_euclid(width)
                .saturating_mul(2)
                .saturating_add(i64::from(raw & 1));
            let half_y = local
                .div_euclid(width)
                .saturating_mul(2)
                .saturating_add(i64::from((raw >> 1) & 1));
            if let Some(slot) = columns
                .get_mut(usize::try_from(x.saturating_sub(half_x).rem_euclid(period)).unwrap_or(0))
            {
                *slot = slot.saturating_add(1);
            }
            if let Some(slot) = rows
                .get_mut(usize::try_from(y.saturating_sub(half_y).rem_euclid(period)).unwrap_or(0))
            {
                *slot = slot.saturating_add(1);
            }
        }
    }

    let best = |counts: &[u64]| -> i64 {
        counts
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| **count)
            .map_or(0, |(at, _)| i64::try_from(at).unwrap_or(0))
    };
    histograms
        .iter()
        .map(|(columns, rows)| (best(columns), best(rows)))
        .collect()
}

fn wallpaper_tile(
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    offsets: &[(i64, i64)],
    class: usize,
    x: i64,
    y: i64,
) -> Option<CellTile> {
    let entry = blend_tile.texture_classes.get(class)?;
    let width = i64::from(entry.width);
    let period = width.checked_mul(2)?;
    if period <= 0 {
        return None;
    }
    let (offset_x, offset_y) = offsets.get(class).copied()?;
    let half_x = x.saturating_sub(offset_x).rem_euclid(period);
    let half_y = y.saturating_sub(offset_y).rem_euclid(period);
    let local = half_y
        .div_euclid(2)
        .checked_mul(width)?
        .checked_add(half_x.div_euclid(2))?;
    let tile = i64::from(entry.first_tile).checked_add(local)?;
    let sub = half_x.rem_euclid(2) | half_y.rem_euclid(2).checked_mul(2)?;
    let raw = u16::try_from(tile.checked_mul(4)?.checked_add(sub)?).ok()?;
    layout.cell(raw)
}

fn resolve(
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    offsets: &[(i64, i64)],
    x: i64,
    y: i64,
) -> Option<Cell> {
    let index = cell_index(blend_tile, x, y)?;
    let base = layout.cell(blend_tile.tile_index.get(index).copied()?)?;
    let overlay = |value: u32| -> Option<Overlay> {
        let slot = usize::try_from(value).ok()?.checked_sub(1)?;
        let record = *blend_tile.blend_records.get(slot)?;
        let record_tile = layout.cell(u16::try_from(record.tile_index).ok()?)?;
        let wallpaper_tile = wallpaper_tile(blend_tile, layout, offsets, record_tile.class, x, y)
            .unwrap_or(record_tile);
        Some(Overlay {
            record_tile,
            wallpaper_tile,
            record,
        })
    };
    Some(Cell {
        base,
        blend: blend_tile.blend_index.get(index).copied().and_then(overlay),
        extra: blend_tile
            .extra_blend_index
            .get(index)
            .copied()
            .and_then(overlay),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

fn measure_boundaries(
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    textures: &[Option<TextureData>],
    offsets: &[(i64, i64)],
    totals: &mut Totals,
) {
    if blend_tile.width < 2 || blend_tile.height < 2 {
        return;
    }
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let (dx, dy) = match axis {
            Axis::Horizontal => (1i64, 0i64),
            Axis::Vertical => (0, 1),
        };
        let limit_x = i64::from(blend_tile.width).saturating_sub(dx);
        let limit_y = i64::from(blend_tile.height).saturating_sub(dy);
        let mut y = 0i64;
        while y < limit_y {
            let mut x = 0i64;
            while x < limit_x {
                compare_pair(blend_tile, layout, textures, offsets, x, y, axis, totals);
                x = x.saturating_add(SEAM_STRIDE as i64);
            }
            y = y.saturating_add(SEAM_STRIDE as i64);
        }
    }
}

fn edge_alpha(alpha: [f32; 4], axis: Axis, far_side: bool, t: f32) -> f32 {
    let (a, b) = match (axis, far_side) {
        (Axis::Horizontal, true) => (alpha.get(1), alpha.get(3)),
        (Axis::Horizontal, false) => (alpha.first(), alpha.get(2)),
        (Axis::Vertical, true) => (alpha.get(2), alpha.get(3)),
        (Axis::Vertical, false) => (alpha.first(), alpha.get(1)),
    };
    let (a, b) = (a.copied().unwrap_or(0.0), b.copied().unwrap_or(0.0));
    a + (b - a) * t
}

fn edge_uv(tile: &CellTile, axis: Axis, far_side: bool, t: f32, inset: [f32; 2]) -> [f32; 2] {
    let span = [
        tile.uv_x1y1[0] - tile.uv_xy[0],
        tile.uv_x1y1[1] - tile.uv_xy[1],
    ];
    let step = [inset[0].copysign(span[0]), inset[1].copysign(span[1])];
    match axis {
        Axis::Horizontal => {
            let u = if far_side {
                tile.uv_x1y1[0] - step[0]
            } else {
                tile.uv_xy[0] + step[0]
            };
            [u, tile.uv_xy[1] + t * span[1]]
        }
        Axis::Vertical => {
            let v = if far_side {
                tile.uv_x1y1[1] - step[1]
            } else {
                tile.uv_xy[1] + step[1]
            };
            [tile.uv_xy[0] + t * span[0], v]
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_pair(
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    textures: &[Option<TextureData>],
    offsets: &[(i64, i64)],
    x: i64,
    y: i64,
    axis: Axis,
    totals: &mut Totals,
) {
    let (dx, dy) = match axis {
        Axis::Horizontal => (1i64, 0i64),
        Axis::Vertical => (0, 1),
    };
    let (Some(a), Some(b)) = (
        resolve(blend_tile, layout, offsets, x, y),
        resolve(
            blend_tile,
            layout,
            offsets,
            x.saturating_add(dx),
            y.saturating_add(dy),
        ),
    ) else {
        return;
    };
    if a.base.class == b.base.class {
        return;
    }
    let blended = a.blend.is_some() || b.blend.is_some();

    for step in 0..SEAM_SAMPLES {
        let t = (f32::from(step) + 0.5) / f32::from(SEAM_SAMPLES);
        for (index, candidate) in CANDIDATES.iter().enumerate() {
            let (Some(ca), Some(cb)) = (
                composite(textures, &a, candidate, axis, true, t),
                composite(textures, &b, candidate, axis, false, t),
            ) else {
                continue;
            };
            totals.add(index, difference(ca, cb), blended);
        }

        let Some(base_b) = sample(textures, b.base.class, &b.base, axis, false, t, 0.0) else {
            continue;
        };
        if let Some(neighbour) = sample(textures, b.base.class, &b.base, axis, false, t, 1.0) {
            totals.noise_sum += difference(base_b, neighbour);
            totals.noise_n = totals.noise_n.saturating_add(1);
        }
        if let Some(overlay_layer) = a
            .blend
            .filter(|layer| layer.record_tile.class == b.base.class)
        {
            let tile = overlay_layer.record_tile;
            if let Some(overlay) = sample(textures, tile.class, &tile, axis, true, t, 0.0) {
                totals.same_class_sum += difference(overlay, base_b);
                totals.same_class_n = totals.same_class_n.saturating_add(1);
            }
        }
    }
}

fn composite(
    textures: &[Option<TextureData>],
    cell: &Cell,
    candidate: &Candidate,
    axis: Axis,
    far_side: bool,
    t: f32,
) -> Option<[f32; 3]> {
    let mut colour = sample(
        textures,
        cell.base.class,
        &cell.base,
        axis,
        far_side,
        t,
        0.0,
    )?;
    let Some(alpha_of) = candidate.alpha else {
        return Some(colour);
    };
    let layers = if candidate.swap_layers {
        [cell.extra, cell.blend]
    } else {
        [cell.blend, cell.extra]
    };
    for layer in layers.into_iter().flatten() {
        let tile = if candidate.wallpaper {
            layer.wallpaper_tile
        } else {
            layer.record_tile
        };
        let Some(over) = sample(textures, tile.class, &tile, axis, far_side, t, 0.0) else {
            continue;
        };
        let a = edge_alpha(alpha_of(&layer.record), axis, far_side, t).clamp(0.0, 1.0);
        for channel in 0..3 {
            let (Some(dst), Some(src)) = (colour.get(channel).copied(), over.get(channel).copied())
            else {
                continue;
            };
            if let Some(slot) = colour.get_mut(channel) {
                *slot = dst + (src - dst) * a;
            }
        }
    }
    Some(colour)
}

fn sample(
    textures: &[Option<TextureData>],
    class: usize,
    tile: &CellTile,
    axis: Axis,
    far_side: bool,
    t: f32,
    steps_inward: f32,
) -> Option<[f32; 3]> {
    let Some(Some(texture)) = textures.get(class) else {
        return None;
    };
    let level = texture.levels.first()?;
    if level.width == 0 || level.height == 0 {
        return None;
    }
    let inset = [
        (0.5 + steps_inward) / level.width as f32,
        (0.5 + steps_inward) / level.height as f32,
    ];
    let uv = edge_uv(tile, axis, far_side, t, inset);
    texel(texture, uv[0], uv[1])
}

fn clamp_to_u32(value: f32, extent: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let limit = extent.saturating_sub(1);
    if value >= limit as f32 {
        limit
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = value as u32;
        index
    }
}

fn texel(texture: &TextureData, u: f32, v: f32) -> Option<[f32; 3]> {
    let level = texture.levels.first()?;
    let (w, h) = (level.width, level.height);
    let px = i64::from(clamp_to_u32(u * w as f32, w));
    let py = i64::from(clamp_to_u32(v * h as f32, h));
    let offset = usize::try_from(
        py.checked_mul(i64::from(w))?
            .checked_add(px)?
            .checked_mul(4)?,
    )
    .ok()?;
    let slice = level.data.get(offset..offset.checked_add(3)?)?;
    Some([
        f32::from(*slice.first()?),
        f32::from(*slice.get(1)?),
        f32::from(*slice.get(2)?),
    ])
}

fn difference(a: [f32; 3], b: [f32; 3]) -> f64 {
    let total: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from((x - y).abs()))
        .sum();
    total / 3.0
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
