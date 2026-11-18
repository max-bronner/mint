use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_render::TextureData;
use mint_sage_big::BigArchive;
use mint_sage_map::{BlendTileData, HeightMapData, MapFile, MAP_XY_FACTOR};
use mint_sage_terrain::{load_class_textures, TerrainTypes, TileLayout};
use mint_vfs::Vfs;

const TERRAIN_INI: &str = "data/ini/terrain.ini";

const SEAM_STRIDE: u32 = 4;

const SEAM_SAMPLES: u16 = 4;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_texture_scan <install dir> [<install dir> ...]");
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

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        walk_install(root, &vfs, &types, &mut totals);
    }
    totals.report(started.elapsed().as_secs_f64());

    if totals.failures.is_empty() && totals.layout_is_best() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

type PlaceRect = fn(f32, f32, u16, f32, f32) -> ([f32; 2], [f32; 2]);

const CANDIDATES: [(&str, PlaceRect); 8] = [
    (
        "implemented: rows from the bottom, bit 1 = upper half",
        |c, r, s, w, h| up(c + half(s & 1), r + half(s >> 1), 0.5, 0.5, w, h),
    ),
    (
        "the same, mirrored in v (mint before 2026-08-29)",
        |c, r, s, w, h| down(c + half(s & 1), r + half(s >> 1), 0.5, 0.5, w, h),
    ),
    ("whole tile per cell", |c, r, _s, w, h| {
        up(c, r, 1.0, 1.0, w, h)
    }),
    ("bit 1 = lower half", |c, r, s, w, h| {
        up(c + half(s & 1), r + half(1 - (s >> 1)), 0.5, 0.5, w, h)
    }),
    ("tile rows counted from the top", |c, r, s, w, h| {
        up(c + half(s & 1), h - 1.0 - r + half(s >> 1), 0.5, 0.5, w, h)
    }),
    ("sub-tile bits swapped", |c, r, s, w, h| {
        up(c + half(s >> 1), r + half(s & 1), 0.5, 0.5, w, h)
    }),
    ("tile grid transposed", |c, r, s, w, h| {
        up(r + half(s & 1), c + half(s >> 1), 0.5, 0.5, w, h)
    }),
    ("sub-tile column inverted", |c, r, s, w, h| {
        up(c + half(1 - (s & 1)), r + half(s >> 1), 0.5, 0.5, w, h)
    }),
];

const MIRROR_TWIN: usize = 1;

const MIRROR_TOLERANCE: f64 = 0.05;

fn half(bit: u16) -> f32 {
    if bit == 0 {
        0.0
    } else {
        0.5
    }
}

fn up(u: f32, v: f32, du: f32, dv: f32, width: f32, rows: f32) -> ([f32; 2], [f32; 2]) {
    (
        [u / width, 1.0 - v / rows],
        [(u + du) / width, 1.0 - (v + dv) / rows],
    )
}

fn down(u: f32, v: f32, du: f32, dv: f32, width: f32, rows: f32) -> ([f32; 2], [f32; 2]) {
    ([u / width, v / rows], [(u + du) / width, (v + dv) / rows])
}

fn mean(sum: Option<&f64>, n: Option<&u64>) -> f64 {
    let (Some(&sum), Some(&n)) = (sum, n) else {
        return f64::INFINITY;
    };
    if n == 0 {
        return f64::INFINITY;
    }
    sum / (n as f64)
}

#[derive(Default)]
struct Totals {
    files: u64,
    parsed: u64,
    failures: Vec<String>,

    classes: u64,
    classes_resolved: u64,
    unresolved: BTreeMap<String, u64>,

    cells: u64,
    cells_out_of_range: u64,
    out_of_range_files: Vec<String>,

    parity_cells: u64,
    parity_files: u64,
    parity_files_checked: u64,

    aligned: BTreeMap<i32, (u64, u64, u64)>,

    objects: u64,
    objects_inside: u64,
    objects_inside_transposed: u64,
    files_favouring_axes: u64,
    files_favouring_transposed: Vec<String>,

    square_classes: u64,
    shapes: BTreeMap<(i32, i32), u64>,

    seam_sum: [f64; CANDIDATES.len()],
    seam_n: [u64; CANDIDATES.len()],
    reference_sum: f64,
    reference_n: u64,

    differing_sum: [f64; CANDIDATES.len()],
    differing_n: [u64; CANDIDATES.len()],
    differing_reference_sum: f64,
    differing_reference_n: u64,

    orientation_groups: u64,
    orientation_cells: u64,
    orientation_r_implemented: f64,
    orientation_r_mirrored: f64,
}

impl Totals {
    fn layout_is_best(&self) -> bool {
        let all = self.seam_mean(0);
        let differing = self.differing_mean(0);
        let separable = (1..CANDIDATES.len())
            .filter(|other| *other != MIRROR_TWIN)
            .all(|other| self.seam_mean(other) > all && self.differing_mean(other) > differing);
        separable && self.mirror_is_inert()
    }

    fn mirror_is_inert(&self) -> bool {
        (self.seam_mean(MIRROR_TWIN) - self.seam_mean(0)).abs() < MIRROR_TOLERANCE
            && (self.differing_mean(MIRROR_TWIN) - self.differing_mean(0)).abs() < MIRROR_TOLERANCE
    }

    fn seam_mean(&self, candidate: usize) -> f64 {
        mean(self.seam_sum.get(candidate), self.seam_n.get(candidate))
    }

    fn differing_mean(&self, candidate: usize) -> f64 {
        mean(
            self.differing_sum.get(candidate),
            self.differing_n.get(candidate),
        )
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
            "texture classes: {} of {} resolved to a decodable texture",
            self.classes_resolved, self.classes
        );
        if self.unresolved.is_empty() {
            println!("  no unresolved classes");
        } else {
            for (reason, count) in &self.unresolved {
                println!("  {count} x {reason}");
            }
        }
        println!(
            "class tile grids: {} of {} have num_tiles == width * width; shapes {:?}",
            self.square_classes, self.classes, self.shapes
        );
        println!(
            "cells at their class's commonest grid alignment, by tile-grid width \
             (implemented, then with the tile grid transposed).\n\
             This is a fact about the file's encoding and is *blind* to which \
             way round the image is read: mirroring v leaves every number below \
             unchanged."
        );
        for (width, (cells, aligned, transposed)) in &self.aligned {
            println!(
                "  width {width:2}: {aligned} of {cells} ({:.1}%)   transposed {transposed} ({:.1}%)",
                100.0 * *aligned as f64 / (*cells).max(1) as f64,
                100.0 * *transposed as f64 / (*cells).max(1) as f64
            );
        }
        println!(
            "placed objects inside the heightmap: {} of {} as parsed, {} with the axes \
             transposed; files favouring each: {} against {}",
            self.objects_inside,
            self.objects,
            self.objects_inside_transposed,
            self.files_favouring_axes,
            self.files_favouring_transposed.len()
        );
        for file in &self.files_favouring_transposed {
            println!("  favours the transposed axes: {file}");
        }
        println!(
            "low 2 bits == cell parity: {} of {} cells, and every cell in {} of {} files",
            self.parity_cells, self.cells, self.parity_files, self.parity_files_checked
        );
        println!(
            "cells: {} total, {} with a tile index past every class ({} file(s))",
            self.cells,
            self.cells_out_of_range,
            self.out_of_range_files.len()
        );
        for file in &self.out_of_range_files {
            println!("  {file}");
        }

        let reference = mean(Some(&self.reference_sum), Some(&self.reference_n));
        let differing_reference = mean(
            Some(&self.differing_reference_sum),
            Some(&self.differing_reference_n),
        );
        println!(
            "\nseam cross-check: {} sampled texel pairs per candidate, {} of them \
             across a boundary whose two cells name different tiles",
            self.seam_n.first().copied().unwrap_or(0),
            self.differing_n.first().copied().unwrap_or(0)
        );
        println!("                   all seams   differing tiles");
        println!(
            "  floor           {reference:9.3}   {differing_reference:9.3}   (texture noise floor)"
        );
        for (index, (name, _)) in CANDIDATES.iter().enumerate() {
            let all = self.seam_mean(index);
            let differing = self.differing_mean(index);
            let marker = if index == 0 { "->" } else { "  " };
            println!("  {marker}              {all:9.3}   {differing:9.3}   {name}");
        }
        println!(
            "  mirror twin: {:+.3} / {:+.3} against the implemented layout - a \
             whole-class\n  mirror is continuity-preserving, so these must \
             agree; the seam figure cannot\n  choose between them and does not \
             claim to. See the orientation cross-check.",
            self.seam_mean(MIRROR_TWIN) - self.seam_mean(0),
            self.differing_mean(MIRROR_TWIN) - self.differing_mean(0)
        );
        if self.layout_is_best() {
            println!(
                "  the implemented layout is the lowest of the {} it can be \
                 separated from",
                CANDIDATES.len().saturating_sub(2)
            );
        } else if !self.mirror_is_inert() {
            println!(
                "  FAILED: the mirror twin does not measure the same - the v \
                 mirror is applied to only one of the tile row and the sub-tile bit"
            );
        } else {
            println!("  FAILED: the implemented layout is not the lowest");
        }

        self.report_orientation();
    }

    fn report_orientation(&self) {
        println!(
            "\norientation cross-check: texture brightness against heightmap relief, \
             over\n{} cells of {} authored classes (tile grid 8 or wider), \
             cell-weighted mean |r|:",
            self.orientation_cells, self.orientation_groups
        );
        let implemented = mean_f64(self.orientation_r_implemented, self.orientation_cells);
        let mirrored = mean_f64(self.orientation_r_mirrored, self.orientation_cells);
        println!("  implemented (rows from the bottom)   {implemented:.4}");
        println!("  mirrored in v (mint before 2026-08-29) {mirrored:.4}");
        if self.orientation_cells == 0 {
            println!("  no class large enough to measure - nothing is claimed");
        } else if implemented > mirrored {
            println!("  the implemented layout agrees better with the relief");
        } else {
            println!("  the mirror agrees better with the relief - INVESTIGATE");
        }
    }
}

fn mean_f64(sum: f64, n: u64) -> f64 {
    if n == 0 {
        return 0.0;
    }
    sum / (n as f64)
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
                    measure_object_axes(&label, &map, totals);
                    check_map(&label, &map.blend_tile, &map.height_map, vfs, types, totals);
                }
                Err(error) => totals.failures.push(format!("{label}: {error}")),
            }
        }
    }
}

fn measure_object_axes(name: &str, map: &MapFile, totals: &mut Totals) {
    let (width, height) = (map.height_map.width, map.height_map.height);
    let border = f32::from(u16::try_from(map.height_map.border_size).unwrap_or(u16::MAX));
    let inside = |wx: f32, wy: f32| -> bool {
        let cx = wx / MAP_XY_FACTOR + border;
        let cy = wy / MAP_XY_FACTOR + border;
        cx >= 0.0 && cy >= 0.0 && cx < width as f32 && cy < height as f32
    };
    let (mut direct, mut swapped) = (0u64, 0u64);
    for object in &map.objects {
        if inside(object.x, object.y) {
            direct = direct.saturating_add(1);
        }
        if inside(object.y, object.x) {
            swapped = swapped.saturating_add(1);
        }
    }
    totals.objects = totals.objects.saturating_add(map.objects.len() as u64);
    totals.objects_inside = totals.objects_inside.saturating_add(direct);
    totals.objects_inside_transposed = totals.objects_inside_transposed.saturating_add(swapped);
    if direct > swapped {
        totals.files_favouring_axes = totals.files_favouring_axes.saturating_add(1);
    } else if swapped > direct {
        totals.files_favouring_transposed.push(format!(
            "{name}: {direct} against {swapped} of {}",
            map.objects.len()
        ));
    }
}

fn check_map(
    name: &str,
    blend_tile: &BlendTileData,
    height_map: &HeightMapData,
    vfs: &Vfs,
    types: &TerrainTypes,
    totals: &mut Totals,
) {
    let loaded = load_class_textures(vfs, types, &blend_tile.texture_classes);
    totals.classes = totals
        .classes
        .saturating_add(blend_tile.texture_classes.len() as u64);
    for texture in loaded.textures.iter().flatten() {
        totals.classes_resolved = totals.classes_resolved.saturating_add(1);
        let _ = texture;
    }
    for (index, error) in &loaded.failures {
        let class = blend_tile
            .texture_classes
            .get(*index)
            .map_or("?", |c| c.name.as_str());
        *totals
            .unresolved
            .entry(format!("{class}: {error}"))
            .or_default() += 1;
    }

    for class in &blend_tile.texture_classes {
        *totals
            .shapes
            .entry((class.num_tiles, class.width))
            .or_default() += 1;
        if class.width > 0 && class.num_tiles == class.width.saturating_mul(class.width) {
            totals.square_classes = totals.square_classes.saturating_add(1);
        }
    }

    totals.cells = totals
        .cells
        .saturating_add(u64::from(blend_tile.cell_count));

    let mut parity_ok = 0u64;
    for y in 0..blend_tile.height {
        for x in 0..blend_tile.width {
            let Some(index) = y
                .checked_mul(blend_tile.width)
                .and_then(|row| row.checked_add(x))
                .and_then(|index| usize::try_from(index).ok())
            else {
                continue;
            };
            let Some(&value) = blend_tile.tile_index.get(index) else {
                continue;
            };
            let expected = u16::try_from((x & 1) | ((y & 1) << 1)).unwrap_or(u16::MAX);
            if value & 3 == expected {
                parity_ok = parity_ok.saturating_add(1);
            }
        }
    }
    measure_alignment(blend_tile, totals);
    totals.parity_cells = totals.parity_cells.saturating_add(parity_ok);
    totals.parity_files_checked = totals.parity_files_checked.saturating_add(1);
    if parity_ok == u64::from(blend_tile.cell_count) {
        totals.parity_files = totals.parity_files.saturating_add(1);
    }
    let out_of_range = blend_tile
        .tile_index
        .iter()
        .filter(|&&value| i32::from(value >> 2) >= blend_tile.num_bitmap_tiles)
        .count() as u64;
    if out_of_range > 0 {
        totals.cells_out_of_range = totals.cells_out_of_range.saturating_add(out_of_range);
        totals
            .out_of_range_files
            .push(format!("{name}: {out_of_range} cell(s)"));
    }

    measure_seams(blend_tile, &loaded.textures, totals);
    measure_orientation(blend_tile, height_map, &loaded.textures, totals);
}

const AUTHORED_WIDTH: i32 = 8;

const ORIENTATION_MIN_CELLS: usize = 64;

fn measure_orientation(
    blend_tile: &BlendTileData,
    height_map: &HeightMapData,
    textures: &[Option<TextureData>],
    totals: &mut Totals,
) {
    let layout = TileLayout::new(&blend_tile.texture_classes);
    for (class_index, class) in blend_tile.texture_classes.iter().enumerate() {
        if class.width < AUTHORED_WIDTH {
            continue;
        }
        let Some(Some(texture)) = textures.get(class_index) else {
            continue;
        };
        let mut samples: Vec<(f64, f64, f64)> = Vec::new();
        for y in 0..blend_tile.height {
            for x in 0..blend_tile.width {
                let Some(placed) = place(blend_tile, &layout, x, y) else {
                    continue;
                };
                if placed.class != class_index {
                    continue;
                }
                let Some(height) = height_map.sample(x, y) else {
                    continue;
                };
                let (Some(direct), Some(mirrored)) = (
                    patch_brightness(texture, &placed, false),
                    patch_brightness(texture, &placed, true),
                ) else {
                    continue;
                };
                samples.push((f64::from(height), direct, mirrored));
            }
        }
        if samples.len() < ORIENTATION_MIN_CELLS {
            continue;
        }
        let heights: Vec<f64> = samples.iter().map(|s| s.0).collect();
        let direct: Vec<f64> = samples.iter().map(|s| s.1).collect();
        let mirrored: Vec<f64> = samples.iter().map(|s| s.2).collect();
        let (Some(r_direct), Some(r_mirrored)) = (
            correlation(&heights, &direct),
            correlation(&heights, &mirrored),
        ) else {
            continue;
        };
        let cells = samples.len() as u64;
        totals.orientation_groups = totals.orientation_groups.saturating_add(1);
        totals.orientation_cells = totals.orientation_cells.saturating_add(cells);
        totals.orientation_r_implemented += r_direct.abs() * cells as f64;
        totals.orientation_r_mirrored += r_mirrored.abs() * cells as f64;
    }
}

fn patch_brightness(texture: &TextureData, placed: &Placed, mirror: bool) -> Option<f64> {
    let index = if mirror { MIRROR_TWIN } else { 0 };
    let place_rect = CANDIDATES.get(index)?.1;
    let (xy, x1y1) = place_rect(
        placed.column,
        placed.row,
        placed.sub,
        placed.width,
        placed.rows,
    );
    let mut total = 0.0f64;
    let mut count = 0u32;
    for iy in 0..ORIENTATION_PATCH {
        for ix in 0..ORIENTATION_PATCH {
            let t = |i: u16, lo: f32, hi: f32| {
                let step = (f32::from(i) + 0.5) / f32::from(ORIENTATION_PATCH);
                lo + (hi - lo) * step
            };
            let Some(rgb) = texel(texture, t(ix, xy[0], x1y1[0]), t(iy, xy[1], x1y1[1])) else {
                continue;
            };
            total += f64::from(u16::from(rgb[0]) + u16::from(rgb[1]) + u16::from(rgb[2]));
            count = count.saturating_add(1);
        }
    }
    if count == 0 {
        return None;
    }
    Some(total / f64::from(count))
}

const ORIENTATION_PATCH: u16 = 4;

fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len();
    if n != b.len() || n < 2 {
        return None;
    }
    let count = n as f64;
    let mean_a = a.iter().sum::<f64>() / count;
    let mean_b = b.iter().sum::<f64>() / count;
    let mut cov = 0.0;
    let mut var_a = 0.0;
    let mut var_b = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        let (dx, dy) = (x - mean_a, y - mean_b);
        cov += dx * dy;
        var_a += dx * dx;
        var_b += dy * dy;
    }
    if var_a <= 0.0 || var_b <= 0.0 {
        return None;
    }
    Some(cov / (var_a * var_b).sqrt())
}

fn measure_alignment(blend_tile: &BlendTileData, totals: &mut Totals) {
    for (class_index, class) in blend_tile.texture_classes.iter().enumerate() {
        if class.width <= 0 {
            continue;
        }
        let span = i64::from(class.width).saturating_mul(2);
        let mut offsets: BTreeMap<(i64, i64), u64> = BTreeMap::new();
        let mut transposed: BTreeMap<(i64, i64), u64> = BTreeMap::new();
        let mut cells = 0u64;
        for y in 0..blend_tile.height {
            for x in 0..blend_tile.width {
                let Some(index) = y
                    .checked_mul(blend_tile.width)
                    .and_then(|row| row.checked_add(x))
                    .and_then(|index| usize::try_from(index).ok())
                else {
                    continue;
                };
                let Some(&value) = blend_tile.tile_index.get(index) else {
                    continue;
                };
                let tile = i64::from(value >> 2);
                if tile < i64::from(class.first_tile)
                    || tile
                        >= i64::from(class.first_tile).saturating_add(i64::from(class.num_tiles))
                {
                    continue;
                }
                let local = tile.saturating_sub(i64::from(class.first_tile));
                let sub = i64::from(value & 3);
                let half_column = local
                    .checked_rem(i64::from(class.width))
                    .unwrap_or(0)
                    .saturating_mul(2)
                    .saturating_add(sub & 1);
                let half_row = local
                    .checked_div(i64::from(class.width))
                    .unwrap_or(0)
                    .saturating_mul(2)
                    .saturating_add(sub >> 1);
                let du = half_column.saturating_sub(i64::from(x)).rem_euclid(span);
                let dv = half_row.saturating_sub(i64::from(y)).rem_euclid(span);
                *offsets.entry((du, dv)).or_default() += 1;
                let tu = half_row.saturating_sub(i64::from(x)).rem_euclid(span);
                let tv = half_column.saturating_sub(i64::from(y)).rem_euclid(span);
                *transposed.entry((tu, tv)).or_default() += 1;
                cells = cells.saturating_add(1);
            }
        }
        let _ = class_index;
        let entry = totals.aligned.entry(class.width).or_default();
        entry.0 = entry.0.saturating_add(cells);
        entry.1 = entry
            .1
            .saturating_add(offsets.values().copied().max().unwrap_or(0));
        entry.2 = entry
            .2
            .saturating_add(transposed.values().copied().max().unwrap_or(0));
    }
}

struct Placed {
    class: usize,
    local: u16,
    column: f32,
    row: f32,
    sub: u16,
    width: f32,
    rows: f32,
}

fn place(blend_tile: &BlendTileData, layout: &TileLayout, x: u32, y: u32) -> Option<Placed> {
    let index = usize::try_from(y.checked_mul(blend_tile.width)?.checked_add(x)?).ok()?;
    let value = blend_tile.tile_index.get(index).copied()?;
    let cell = layout.cell(value)?;
    let class = blend_tile.texture_classes.get(cell.class)?;
    let width = f32::from(u16::try_from(class.width).ok()?);
    let rows = (f32::from(u16::try_from(class.num_tiles).ok()?) / width).ceil();
    let tile = i64::from(value >> 2).checked_sub(i64::from(class.first_tile))?;
    let local = u16::try_from(tile).ok()?;
    let columns = u16::try_from(class.width).ok()?;
    if columns == 0 {
        return None;
    }
    Some(Placed {
        class: cell.class,
        local,
        column: f32::from(local % columns),
        row: f32::from(local / columns),
        sub: value & 3,
        width,
        rows,
    })
}

fn measure_seams(
    blend_tile: &BlendTileData,
    textures: &[Option<TextureData>],
    totals: &mut Totals,
) {
    let layout = TileLayout::new(&blend_tile.texture_classes);
    if blend_tile.width < 2 || blend_tile.height < 2 {
        return;
    }

    for y in (0..blend_tile.height).step_by(SEAM_STRIDE as usize) {
        for x in (0..blend_tile.width.saturating_sub(1)).step_by(SEAM_STRIDE as usize) {
            let (Some(a), Some(b)) = (
                place(blend_tile, &layout, x, y),
                place(blend_tile, &layout, x.saturating_add(1), y),
            ) else {
                continue;
            };
            if a.class != b.class {
                continue;
            }
            let Some(Some(texture)) = textures.get(a.class) else {
                continue;
            };
            compare(texture, &a, &b, Axis::Horizontal, totals);
        }
    }
    for y in (0..blend_tile.height.saturating_sub(1)).step_by(SEAM_STRIDE as usize) {
        for x in (0..blend_tile.width).step_by(SEAM_STRIDE as usize) {
            let (Some(a), Some(b)) = (
                place(blend_tile, &layout, x, y),
                place(blend_tile, &layout, x, y.saturating_add(1)),
            ) else {
                continue;
            };
            if a.class != b.class {
                continue;
            }
            let Some(Some(texture)) = textures.get(a.class) else {
                continue;
            };
            compare(texture, &a, &b, Axis::Vertical, totals);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
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

fn texel(texture: &TextureData, u: f32, v: f32) -> Option<[u8; 3]> {
    let level = texture.levels.first()?;
    let (w, h) = (level.width, level.height);
    if w == 0 || h == 0 {
        return None;
    }
    let px = i64::from(clamp_to_u32(u * w as f32, w));
    let py = i64::from(clamp_to_u32(v * h as f32, h));
    let offset = usize::try_from(
        py.checked_mul(i64::from(w))?
            .checked_add(px)?
            .checked_mul(4)?,
    )
    .ok()?;
    let slice = level.data.get(offset..offset.checked_add(3)?)?;
    Some([*slice.first()?, *slice.get(1)?, *slice.get(2)?])
}

fn compare(texture: &TextureData, a: &Placed, b: &Placed, axis: Axis, totals: &mut Totals) {
    let Some(level) = texture.levels.first() else {
        return;
    };
    if level.width == 0 || level.height == 0 {
        return;
    }
    let half_u = 0.5 / level.width as f32;
    let half_v = 0.5 / level.height as f32;

    let inward = |edge: f32, other: f32, amount: f32| edge + amount.copysign(other - edge);
    let along = |from: f32, to: f32, t: f32, amount: f32| {
        let span = to - from;
        from + t * span + amount.copysign(span)
    };

    for (index, (_, place_rect)) in CANDIDATES.iter().enumerate() {
        let (a_xy, a_x1y1) = place_rect(a.column, a.row, a.sub, a.width, a.rows);
        let (b_xy, b_x1y1) = place_rect(b.column, b.row, b.sub, b.width, b.rows);

        for step in 0..SEAM_SAMPLES {
            let t = (f32::from(step) + 0.5) / f32::from(SEAM_SAMPLES);
            let (a_uv, b_uv, floor_uv) = match axis {
                Axis::Horizontal => {
                    let au = [
                        inward(a_x1y1[0], a_xy[0], half_u),
                        along(a_xy[1], a_x1y1[1], t, half_v),
                    ];
                    (
                        au,
                        [
                            inward(b_xy[0], b_x1y1[0], half_u),
                            along(b_xy[1], b_x1y1[1], t, half_v),
                        ],
                        [inward(au[0], a_xy[0], 2.0 * half_u), au[1]],
                    )
                }
                Axis::Vertical => {
                    let av = [
                        along(a_xy[0], a_x1y1[0], t, half_u),
                        inward(a_x1y1[1], a_xy[1], half_v),
                    ];
                    (
                        av,
                        [
                            along(b_xy[0], b_x1y1[0], t, half_u),
                            inward(b_xy[1], b_x1y1[1], half_v),
                        ],
                        [av[0], inward(av[1], a_xy[1], 2.0 * half_v)],
                    )
                }
            };

            let (Some(pa), Some(pb)) = (
                texel(texture, a_uv[0], a_uv[1]),
                texel(texture, b_uv[0], b_uv[1]),
            ) else {
                continue;
            };
            let difference = mean_abs_difference(pa, pb);
            if let (Some(sum), Some(n)) =
                (totals.seam_sum.get_mut(index), totals.seam_n.get_mut(index))
            {
                *sum += difference;
                *n = n.saturating_add(1);
            }
            if a.local != b.local {
                if let (Some(sum), Some(n)) = (
                    totals.differing_sum.get_mut(index),
                    totals.differing_n.get_mut(index),
                ) {
                    *sum += difference;
                    *n = n.saturating_add(1);
                }
            }

            if index == 0 {
                if let Some(pc) = texel(texture, floor_uv[0], floor_uv[1]) {
                    let floor = mean_abs_difference(pa, pc);
                    totals.reference_sum += floor;
                    totals.reference_n = totals.reference_n.saturating_add(1);
                    if a.local != b.local {
                        totals.differing_reference_sum += floor;
                        totals.differing_reference_n =
                            totals.differing_reference_n.saturating_add(1);
                    }
                }
            }
        }
    }
}

fn mean_abs_difference(a: [u8; 3], b: [u8; 3]) -> f64 {
    let total: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| f64::from(*x).abs_diff_f64(f64::from(*y)))
        .sum();
    total / 3.0
}

trait AbsDiff {
    fn abs_diff_f64(self, other: Self) -> Self;
}

impl AbsDiff for f64 {
    fn abs_diff_f64(self, other: Self) -> Self {
        (self - other).abs()
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
