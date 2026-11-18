use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_render::TextureData;
use mint_sage_big::BigArchive;
use mint_sage_map::{
    cell_to_world, BlendTileData, CliffRecord, HeightMapData, MapFile, MAP_XY_FACTOR,
};
use mint_sage_terrain::{
    cell_blend, cell_cliff, load_class_textures, overlay_cliff_uv, stretch_uv, CellTile,
    TerrainTypes, TileLayout,
};
use mint_vfs::Vfs;

const TERRAIN_INI: &str = "data/ini/terrain.ini";

const DEFAULT_VERTICAL_SCALE: f32 = 0.039_062_5;

const BASELINE_STRIDE: i64 = 4;

const BASELINE_KEPT: usize = 2_000_000;

const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const D: usize = 3;

const FOLD_SCALES: [f32; 6] = [0.01, 0.02, DEFAULT_VERTICAL_SCALE, 0.0625, 0.1, 0.2];

const SPLIT_BC: [[usize; 3]; 2] = [[A, B, C], [B, D, C]];
const SPLIT_AD: [[usize; 3]; 2] = [[A, B, D], [A, D, C]];

struct Order {
    name: &'static str,
    slot_to_corner: [usize; 4],
}

const ORDERS: &[Order] = &[
    Order {
        name: "implemented: A B D C (ccw from (x,y))",
        slot_to_corner: [A, B, D, C],
    },
    Order {
        name: "ccw from (x+1,y)",
        slot_to_corner: [B, D, C, A],
    },
    Order {
        name: "ccw from (x+1,y+1)",
        slot_to_corner: [D, C, A, B],
    },
    Order {
        name: "ccw from (x,y+1)",
        slot_to_corner: [C, A, B, D],
    },
    Order {
        name: "cw from (x,y)",
        slot_to_corner: [A, C, D, B],
    },
    Order {
        name: "cw from (x,y+1)",
        slot_to_corner: [C, D, B, A],
    },
    Order {
        name: "cw from (x+1,y+1)",
        slot_to_corner: [D, B, A, C],
    },
    Order {
        name: "cw from (x+1,y)",
        slot_to_corner: [B, A, C, D],
    },
    Order {
        name: "row-major A B C D (the base-tile order)",
        slot_to_corner: [A, B, C, D],
    },
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_cliff_scan <install dir> [<install dir> ...]");
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

    if totals.failures.is_empty() && totals.order_is_best() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct CliffTriangle {
    xy: [[f32; 2]; 3],
    z: [f32; 3],
    cliff_texels: f64,
    base_texels: f64,
}

#[derive(Default, Clone, Copy)]
struct Mean {
    sum: f64,
    n: u64,
}

impl Mean {
    fn add(&mut self, value: f64) {
        self.sum += value;
        self.n = self.n.saturating_add(1);
    }

    fn get(self) -> f64 {
        if self.n == 0 {
            f64::INFINITY
        } else {
            self.sum / (self.n as f64)
        }
    }
}

#[derive(Default, Clone)]
struct Seam {
    x: Mean,
    y: Mean,
    exact_x: u64,
    exact_y: u64,
}

#[derive(Default)]
struct Totals {
    files: u64,
    parsed: u64,
    failures: Vec<String>,

    records: u64,
    shifted_in_class: u64,
    direct_in_class: u64,
    flip: BTreeMap<u8, u64>,
    second_byte: BTreeMap<u8, u64>,

    record_local_tile: BTreeMap<i32, u64>,
    used_local_tile: BTreeMap<i32, u64>,
    stretch_candidates: u64,
    stretch_applied: u64,
    stretch_before: Vec<f64>,
    stretch_after: Vec<f64>,

    used_u_span: Vec<f32>,
    used_v_span: Vec<f32>,

    cells: u64,
    cliff_cells: u64,
    index_past_records: u64,
    class_agrees: u64,
    cliff_and_blend: u64,
    records_shared: u64,

    seams: Vec<Seam>,
    cross_class_pairs: u64,
    cross_class_exact: u64,
    cross_class_difference: Mean,
    seam_vs_class_source: BTreeMap<(bool, bool, bool), u64>,
    residual: BTreeMap<&'static str, u64>,
    residual_vs_run: BTreeMap<(bool, bool), u64>,
    residual_vs_flip: BTreeMap<(bool, bool), u64>,
    open_seam_match: BTreeMap<&'static str, u64>,
    flip_vs_fold_by_scale: BTreeMap<(u32, u8, bool), u64>,

    baseline: Mean,
    baseline_samples: Vec<f64>,
    baseline_texels: f64,
    baseline_area: f64,
    drop_with_record: Vec<u16>,
    drop_without_record: Vec<u16>,
    files_using_records: u64,

    overlay_flat: Mean,
    overlay_cliff: Mean,
    overlay_floor: Mean,
    overlay_class_mismatch: u64,
    overlay_flat_gated: Mean,
    overlay_cliff_gated: Mean,
    overlay_floor_gated: Mean,

    overlay_layers: u64,
    overlay_gate_pass: u64,
    overlay_stretched: u64,
    overlay_untouched: u64,
    overlay_before: Vec<f64>,
    overlay_after: Vec<f64>,
    overlay_record_before: Vec<f64>,
    overlay_record_after: Vec<f64>,
    overlay_stretch_before: Vec<f64>,
    overlay_stretch_after: Vec<f64>,
    overlays_total: u64,
    pending_exemplars: Vec<(i64, i64, u64)>,
    exemplars: Vec<(u64, String)>,

    edge_level: Mean,
    edge_steep: Mean,
    edge_level_base: Mean,
    cliff_edges: Vec<(f64, f64, f64)>,
    cliff_triangles: Vec<CliffTriangle>,
    density_ratio: Vec<f64>,

    flip_vs_fold: BTreeMap<(u8, bool), u64>,
    flip_vs_height: BTreeMap<(u8, bool), u64>,
    second_vs_shape: BTreeMap<(u8, bool), u64>,
    second_vs_interior: BTreeMap<(u8, bool), u64>,
    second_vs_seam: BTreeMap<(u8, bool), u64>,
    second_vs_repeat: BTreeMap<(u8, bool), u64>,
    second_vs_class: BTreeMap<(u8, bool), u64>,
    second_vs_blend: BTreeMap<(u8, bool), u64>,
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
}

fn mean_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.iter().sum::<f64>() / (values.len() as f64)
}

impl Totals {
    fn seam(&self, index: usize) -> Seam {
        self.seams.get(index).cloned().unwrap_or_default()
    }

    fn order_is_best(&self) -> bool {
        let best = self.seam(0);
        best.x.get().is_finite()
            && best.y.get().is_finite()
            && (1..ORDERS.len()).all(|other| {
                let seam = self.seam(other);
                seam.x.get() >= best.x.get()
                    && seam.y.get() >= best.y.get()
                    && (seam.x.get() > best.x.get() || seam.y.get() > best.y.get())
            })
    }

    fn add_seam(&mut self, index: usize, difference: f64, horizontal: bool) {
        while self.seams.len() <= index {
            self.seams.push(Seam::default());
        }
        let Some(seam) = self.seams.get_mut(index) else {
            return;
        };
        if horizontal {
            seam.x.add(difference);
            if difference < 1e-5 {
                seam.exact_x = seam.exact_x.saturating_add(1);
            }
        } else {
            seam.y.add(difference);
            if difference < 1e-5 {
                seam.exact_y = seam.exact_y.saturating_add(1);
            }
        }
    }

    fn cliff_density(&self, scale: f32) -> (f64, f64, f64, f64) {
        let mut cliff = Vec::with_capacity(self.cliff_triangles.len());
        let mut base = Vec::with_capacity(self.cliff_triangles.len());
        for triangle in &self.cliff_triangles {
            let area = triangle_area(triangle.xy, triangle.z, scale);
            if area <= 1e-6 {
                continue;
            }
            cliff.push(triangle.cliff_texels / area);
            base.push(triangle.base_texels / area);
        }
        let (cliff_mean, base_mean) = (mean_of(&cliff), mean_of(&base));
        (cliff_mean, median(&mut cliff), base_mean, median(&mut base))
    }

    fn cliff_totals(&self, scale: f32) -> (f64, f64, f64) {
        let (mut texels, mut area, mut base) = (0.0f64, 0.0f64, 0.0f64);
        for triangle in &self.cliff_triangles {
            let this = triangle_area(triangle.xy, triangle.z, scale);
            if this <= 1e-6 {
                continue;
            }
            texels += triangle.cliff_texels;
            base += triangle.base_texels;
            area += this;
        }
        (texels, area, base)
    }

    fn edge_rates(&self, scale: f32) -> (f64, f64) {
        let (mut level, mut steep) = (Mean::default(), Mean::default());
        for (texels, flat, drop) in &self.cliff_edges {
            let dz = drop * f64::from(scale);
            let world = (flat * flat + dz * dz).sqrt();
            if world <= 1e-6 {
                continue;
            }
            if *drop < 100.0 {
                level.add(texels / world);
            } else if *drop >= 1000.0 {
                steep.add(texels / world);
            }
        }
        (level.get(), steep.get())
    }

    #[allow(clippy::too_many_lines)]
    fn report(&self, seconds: f64) {
        println!(
            "\n{} .map/.bse files, {} parsed, in {seconds:.1}s",
            self.files, self.parsed
        );
        for failure in &self.failures {
            println!("  FAILED {failure}");
        }

        println!("\n-- records --");
        println!("  cliff records                          {}", self.records);
        println!(
            "  tileIndex >> 2 inside a texture class  {} of {}",
            self.shifted_in_class, self.records
        );
        println!(
            "  tileIndex unshifted, same test         {} of {}",
            self.direct_in_class, self.records
        );
        println!("  flip byte                              {:?}", self.flip);
        println!(
            "  second byte                            {:?}",
            self.second_byte
        );

        println!("\n-- cells --");
        println!("  cells                                  {}", self.cells);
        println!(
            "  cells with a cliff index               {} ({:.4}%)",
            self.cliff_cells,
            100.0 * self.cliff_cells as f64 / (self.cells.max(1) as f64)
        );
        println!(
            "  cliff index past the record list       {}",
            self.index_past_records
        );
        println!(
            "  record's class == the cell's own class {} of {} (the other {} keep their base tile)",
            self.class_agrees,
            self.cliff_cells,
            self.cliff_cells.saturating_sub(self.class_agrees)
        );
        println!(
            "  cliff cells that also carry a blend    {}",
            self.cliff_and_blend
        );
        let top = |hist: &BTreeMap<i32, u64>, total: u64| -> String {
            let mut entries: Vec<(i32, u64)> = hist.iter().map(|(k, v)| (*k, *v)).collect();
            entries.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            entries
                .iter()
                .take(4)
                .map(|(local, count)| {
                    format!(
                        "{local}: {count} ({:.2}%)",
                        100.0 * *count as f64 / total.max(1) as f64
                    )
                })
                .collect::<Vec<String>>()
                .join(", ")
        };
        println!(
            "  record tile minus its class's first_tile, commonest: {}",
            top(&self.record_local_tile, self.records)
        );
        println!(
            "  the same, records a cell references:                 {}",
            top(&self.used_local_tile, self.cliff_cells)
        );
        let span_median = |values: &[f32]| -> f32 {
            let mut sorted = values.to_vec();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            sorted.get(sorted.len() / 2).copied().unwrap_or(f32::NAN)
        };
        println!(
            "  stored span per referenced cell, median: u {:.5}, v {:.5}  (a flat cell's own rectangle is 0.125 on each axis)",
            span_median(&self.used_u_span),
            span_median(&self.used_v_span)
        );
        println!(
            "  records referenced by >1 cell          {}",
            self.records_shared
        );

        println!("\n-- corner order: mean |du| + |dv| at a shared grid corner --");
        let best = self.seam(0);
        println!(
            "  {} pairs across +x and {} across +y; {} skipped as cross-class",
            best.x.n, best.y.n, self.cross_class_pairs
        );
        println!("                                                +x seam   +y seam   exact");
        for (index, order) in ORDERS.iter().enumerate() {
            let seam = self.seam(index);
            let marker = if index == 0 { "->" } else { "  " };
            let exact = seam.exact_x.saturating_add(seam.exact_y);
            let total = seam.x.n.saturating_add(seam.y.n).max(1);
            println!(
                "  {marker} {:<40} {:8.5}  {:8.5}  {:5.1}%",
                order.name,
                seam.x.get(),
                seam.y.get(),
                100.0 * exact as f64 / total as f64
            );
        }
        if self.order_is_best() {
            println!(
                "  the implemented order dominates the other {}: no worse on either axis, \
                 strictly better on one",
                ORDERS.len().saturating_sub(1)
            );
        } else {
            println!("  FAILED: the implemented order does not dominate every alternative");
        }

        println!("\n  where the implemented order's own residual comes from:");
        for (bucket, count) in &self.residual {
            println!("    {bucket:<24} {count}");
        }
        println!(
            "    by (seam exact, record indices adjacent): {:?}",
            self.residual_vs_run
        );
        println!(
            "    by (seam exact, the two flip bytes agree): {:?}",
            self.residual_vs_flip
        );
        println!("    where an open seam's coordinate actually lands:");
        for (name, count) in &self.open_seam_match {
            println!("      {name:<28} {count}");
        }

        println!("\n  adjacent cliff pairs by (same record class, same cell class, exact):");
        println!("    {:?}", self.seam_vs_class_source);

        println!("\n-- the flip byte --");
        println!("  vs the fold diagonal that equalises the two triangles' texture scale");
        println!(
            "    (flip, fold is (x,y)-(x+1,y+1)): {:?}",
            self.flip_vs_fold
        );
        println!(
            "  vs h(x,y)+h(x+1,y+1) >= h(x+1,y)+h(x,y+1): {:?}",
            self.flip_vs_height
        );
        println!("  the fold test across vertical scales, as % of each flip value voting AD:");
        for (slot, scale) in FOLD_SCALES.iter().enumerate() {
            let key = u32::try_from(slot).unwrap_or(0);
            let share = |flip: u8| -> f64 {
                let yes = self
                    .flip_vs_fold_by_scale
                    .get(&(key, flip, true))
                    .copied()
                    .unwrap_or(0);
                let no = self
                    .flip_vs_fold_by_scale
                    .get(&(key, flip, false))
                    .copied()
                    .unwrap_or(0);
                100.0 * yes as f64 / (yes.saturating_add(no)).max(1) as f64
            };
            println!(
                "    scale {scale:<9} flip=0 {:5.1}%   flip=1 {:5.1}%",
                share(0),
                share(1)
            );
        }

        println!("\n-- the second byte, and what failed to separate it --");
        println!(
            "  vs the UV quad being a parallelogram:  {:?}",
            self.second_vs_shape
        );
        println!(
            "  vs all four neighbours being cliffs:   {:?}",
            self.second_vs_interior
        );
        println!(
            "  vs the cell's own seam not closing:    {:?}",
            self.second_vs_seam
        );
        println!(
            "  vs any |u| or |v| outside 0..1:        {:?}",
            self.second_vs_repeat
        );
        println!(
            "  vs the record's class == the cell's:   {:?}",
            self.second_vs_class
        );
        println!(
            "  vs the cell also carrying a blend:     {:?}",
            self.second_vs_blend
        );

        println!("\n-- do a record's UVs belong to the face or to the image? --");
        println!(
            "  adjacent cliff pairs naming different classes   {}",
            self.cross_class_pairs
        );
        println!(
            "  of those, agreeing exactly at both shared corners  {} ({:.1}%), mean |du|+|dv| {:.5}",
            self.cross_class_exact,
            if self.cross_class_pairs == 0 {
                0.0
            } else {
                100.0 * (self.cross_class_exact as f64) / (self.cross_class_pairs as f64)
            },
            self.cross_class_difference.get()
        );

        println!("\n-- the blend overlay on a cliff cell --");
        println!(
            "  shared-edge comparisons                {}",
            self.overlay_flat.n
        );
        println!(
            "  cliff cells with an overlay whose neighbour draws another class  {}",
            self.overlay_class_mismatch
        );
        println!(
            "  floor: the neighbour's texel vs the one beside it   {:9.3}",
            self.overlay_floor.get()
        );
        println!(
            "  implemented: the overlay's own flat tile rectangle  {:9.3}",
            self.overlay_flat.get()
        );
        println!(
            "  candidate:   the cliff record's own coordinates     {:9.3}",
            self.overlay_cliff.get()
        );

        println!("  restricted to the overlays the original's own gate accepts - the record");
        println!("  names the *overlay's* class, which section 10 never asked:");
        println!(
            "    comparisons {}   floor {:9.3}   flat rectangle {:9.3}   record's coordinates {:9.3}",
            self.overlay_flat_gated.n,
            self.overlay_floor_gated.get(),
            self.overlay_flat_gated.get(),
            self.overlay_cliff_gated.get()
        );

        println!("\n-- what a cliff cell's overlay layers draw --");
        println!(
            "  implemented: none. overlay layers suppressed on a cliff cell  {} of {} ({:.4}% of every overlay)",
            self.overlay_layers,
            self.overlays_total,
            100.0 * (self.overlay_layers as f64) / (self.overlays_total.max(1) as f64)
        );
        println!("  what the original would draw on those same layers, per `overlay_cliff_uv`:");
        println!(
            "    record names the overlay's class - it takes the record's coordinates  {}",
            self.overlay_gate_pass
        );
        println!(
            "    it does not - the overlay's own rectangle is stretched               {}",
            self.overlay_stretched
        );
        println!(
            "    not steep enough for the stretch - the plain rectangle stands        {}",
            self.overlay_untouched
        );
        let mut before = self.overlay_before.clone();
        let mut after = self.overlay_after.clone();
        println!("  texels per world unit squared over those triangles, against flat ground's:");
        println!(
            "    before: the plain overlay rectangle    mean {:9.3}  median {:9.3}",
            mean_of(&self.overlay_before),
            median(&mut before)
        );
        println!(
            "    after:  the rule above                 mean {:9.3}  median {:9.3}",
            mean_of(&self.overlay_after),
            median(&mut after)
        );
        for (name, before, after) in [
            (
                "the record's coordinates",
                &self.overlay_record_before,
                &self.overlay_record_after,
            ),
            (
                "the height stretch      ",
                &self.overlay_stretch_before,
                &self.overlay_stretch_after,
            ),
        ] {
            let (mut b, mut a) = (before.clone(), after.clone());
            println!(
                "    {name}  n {:6}  before mean {:9.3} median {:9.3} | after mean {:9.3} median {:9.3}",
                before.len(),
                mean_of(before),
                median(&mut b),
                mean_of(after),
                median(&mut a)
            );
        }

        let mut best = self.exemplars.clone();
        best.sort_by(|a, b| b.0.cmp(&a.0));
        println!("  the steepest suppressed cells, to look at:");
        for (_, line) in best.iter().take(6) {
            println!("    {line}");
        }

        println!("\n-- is a cell without a record simply not steep? --");
        let mut with = self.drop_with_record.clone();
        let mut without = self.drop_without_record.clone();
        with.sort_unstable();
        without.sort_unstable();
        let quantile = |sorted: &[u16], numerator: usize, denominator: usize| -> u32 {
            if sorted.is_empty() || denominator == 0 {
                return 0;
            }
            let index = sorted
                .len()
                .saturating_sub(1)
                .saturating_mul(numerator)
                .checked_div(denominator)
                .unwrap_or(0);
            u32::from(sorted.get(index).copied().unwrap_or(0))
        };
        println!(
            "  cells in the {} file(s) that use a record: {} carry one, {} do not",
            self.files_using_records,
            with.len(),
            without.len()
        );
        println!("  corner height range, raw samples:   median   90th   99th    max");
        println!(
            "    cells with a record              {:8}{:7}{:7}{:7}",
            quantile(&with, 1, 2),
            quantile(&with, 9, 10),
            quantile(&with, 99, 100),
            quantile(&with, 1, 1)
        );
        println!(
            "    cells without one                {:8}{:7}{:7}{:7}",
            quantile(&without, 1, 2),
            quantile(&without, 9, 10),
            quantile(&without, 99, 100),
            quantile(&without, 1, 1)
        );
        let threshold = quantile(&with, 1, 10);
        let steep_without = without
            .iter()
            .filter(|drop| u32::from(**drop) >= threshold)
            .count();
        println!(
            "  cells with no record but a drop of {threshold} or more (the 10th percentile \
             of the cells that have one): {steep_without}"
        );

        println!("\n-- the height-based stretch on cells with no record the gate accepts --");
        println!(
            "  candidate cells {}, stretched {} ({:.2}%)",
            self.stretch_candidates,
            self.stretch_applied,
            100.0 * self.stretch_applied as f64 / (self.stretch_candidates.max(1)) as f64
        );
        let mut before = self.stretch_before.clone();
        let mut after = self.stretch_after.clone();
        println!("  texels per world unit squared over those triangles, against flat ground's:");
        println!(
            "    before: the plain tile rectangle      mean {:9.3}  median {:9.3}",
            mean_of(&self.stretch_before),
            median(&mut before)
        );
        println!(
            "    after:  stretched down the face       mean {:9.3}  median {:9.3}",
            mean_of(&self.stretch_after),
            median(&mut after)
        );

        println!("\n-- texel density: source texels per world unit squared --");
        let mut baseline = self.baseline_samples.clone();
        println!(
            "  flat ground baseline, {} triangles      mean {:9.3}  median {:9.3}",
            self.baseline.n,
            self.baseline.get(),
            median(&mut baseline)
        );
        let (cliff_mean, cliff_median, base_mean, base_median) =
            self.cliff_density(DEFAULT_VERTICAL_SCALE);
        println!(
            "  cliff faces, {} triangles, at scale {DEFAULT_VERTICAL_SCALE}",
            self.cliff_triangles.len()
        );
        println!(
            "    before: the cell's own tile rect      mean {base_mean:9.3}  median {base_median:9.3}"
        );
        println!(
            "    after:  the cliff record's own UVs    mean {cliff_mean:9.3}  median {cliff_median:9.3}"
        );
        let mut ratio = self.density_ratio.clone();
        println!(
            "    after/before over the same polygon    mean {:9.3}  median {:9.3}",
            mean_of(&self.density_ratio),
            median(&mut ratio)
        );

        let (cliff_texels, cliff_area, base_texels) = self.cliff_totals(DEFAULT_VERTICAL_SCALE);
        let weighted = |texels: f64, area: f64| if area > 0.0 { texels / area } else { 0.0 };
        println!("\n  area-weighted (total texels / total world area), the same populations:");
        println!(
            "    flat ground                           {:9.3}",
            weighted(self.baseline_texels, self.baseline_area)
        );
        println!(
            "    cliff faces, before                   {:9.3}",
            weighted(base_texels, cliff_area)
        );
        println!(
            "    cliff faces, after                    {:9.3}   (x{:.3} of flat ground)",
            weighted(cliff_texels, cliff_area),
            weighted(cliff_texels, cliff_area) / weighted(self.baseline_texels, self.baseline_area)
        );

        println!(
            "\n  cliff density by how tall the triangle is (area-weighted, scale {DEFAULT_VERTICAL_SCALE}):"
        );
        println!("    raw height across the triangle   triangles       texels/unit^2");
        for (low, high, label) in [
            (0u32, 1u32, "        0 (flat)"),
            (1, 200, "        1 .. 199"),
            (200, 600, "      200 .. 599"),
            (600, 1200, "     600 .. 1199"),
            (1200, 2000, "    1200 .. 1999"),
            (2000, u32::MAX, "         2000 +"),
        ] {
            let (mut texels, mut area, mut count) = (0.0f64, 0.0f64, 0u64);
            for triangle in &self.cliff_triangles {
                let (min, max) = triangle
                    .z
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(lo, hi), z| (lo.min(*z), hi.max(*z)));
                let drop = (max - min).max(0.0);
                if !(f64::from(drop) >= f64::from(low) && f64::from(drop) < f64::from(high)) {
                    continue;
                }
                let this = triangle_area(triangle.xy, triangle.z, DEFAULT_VERTICAL_SCALE);
                if this <= 1e-6 {
                    continue;
                }
                texels += triangle.cliff_texels;
                area += this;
                count = count.saturating_add(1);
            }
            println!(
                "    {label}   {count:10}   {:17.3}",
                if area > 0.0 { texels / area } else { 0.0 }
            );
        }

        println!("\n  texels per world unit along a quad edge, by how much height it crosses:");
        println!(
            "    flat ground, level edges (the control, 32 texels / 10 units)   {:7.3}  over {}",
            self.edge_level_base.get(),
            self.edge_level_base.n
        );
        println!(
            "    cliff, edges crossing under 100 raw samples                    {:7.3}  over {}",
            self.edge_level.get(),
            self.edge_level.n
        );
        println!(
            "    cliff, edges crossing 1000 or more                             {:7.3}  over {}",
            self.edge_steep.get(),
            self.edge_steep.n
        );

        println!("    the two cliff rates across vertical scales:");
        for scale in [0.02f32, 0.03, DEFAULT_VERTICAL_SCALE, 0.05, 0.0605, 0.08] {
            let (level, steep) = self.edge_rates(scale);
            println!("      scale {scale:<9} level {level:7.3}   steep {steep:7.3}");
        }
        let (mut lo, mut hi) = (0.005f32, 0.5f32);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            let (level, steep) = self.edge_rates(mid);
            if steep > level {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        println!(
            "    the two cliff rates are equal - square texels on the face - at scale {:.4}",
            0.5 * (lo + hi)
        );

        println!("\n  the same across vertical scales (the flat baseline barely moves with it):");
        for scale in [0.02f32, 0.03, DEFAULT_VERTICAL_SCALE, 0.05, 0.0625, 0.08] {
            let (c_mean, c_median, b_mean, b_median) = self.cliff_density(scale);
            println!(
                "    scale {scale:<9} before mean {b_mean:8.3} median {b_median:8.3} \
                 | after mean {c_mean:8.3} median {c_median:8.3}"
            );
        }

        let mut baseline_median = self.baseline_samples.clone();
        let target = median(&mut baseline_median);
        let (mut lo, mut hi) = (0.005f32, 0.5f32);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if self.cliff_density(mid).1 > target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        println!(
            "  the cliff density equals the flat baseline ({target:.3}) at scale {:.4}",
            0.5 * (lo + hi)
        );
    }
}

fn triangle_area(xy: [[f32; 2]; 3], z: [f32; 3], scale: f32) -> f64 {
    let point = |i: usize| -> [f64; 3] {
        let p = xy.get(i).copied().unwrap_or([0.0, 0.0]);
        [
            f64::from(p[0]),
            f64::from(p[1]),
            f64::from(z.get(i).copied().unwrap_or(0.0) * scale),
        ]
    };
    let (p0, p1, p2) = (point(0), point(1), point(2));
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt()
}

fn uv_texels(uv: [[f32; 2]; 3], width: u32, height: u32) -> f64 {
    let scaled = |i: usize| -> [f64; 2] {
        let p = uv.get(i).copied().unwrap_or([0.0, 0.0]);
        [
            f64::from(p[0]) * f64::from(width),
            f64::from(p[1]) * f64::from(height),
        ]
    };
    let (p0, p1, p2) = (scaled(0), scaled(1), scaled(2));
    0.5 * ((p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0])).abs()
}

fn pick<T: Copy + Default>(source: &[T; 4], tri: [usize; 3], i: usize) -> T {
    tri.get(i)
        .and_then(|c| source.get(*c))
        .copied()
        .unwrap_or_default()
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
                    label_exemplars(&name, totals);
                }
                Err(error) => totals.failures.push(format!("{label}: {error}")),
            }
        }
    }
}

fn label_exemplars(name: &str, totals: &mut Totals) {
    let mut found = std::mem::take(&mut totals.pending_exemplars);
    found.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    found.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    for (x, y, relief) in found.into_iter().take(2) {
        totals.exemplars.push((
            relief,
            format!("{name} cell ({x}, {y}), corner height range {relief}"),
        ));
    }
}

fn class_of(blend_tile: &BlendTileData, tile: i32) -> Option<usize> {
    blend_tile.texture_classes.iter().position(|class| {
        tile >= class.first_tile && tile < class.first_tile.saturating_add(class.num_tiles)
    })
}

fn cell_index(blend_tile: &BlendTileData, x: i64, y: i64) -> Option<usize> {
    if x < 0 || y < 0 || x >= i64::from(blend_tile.width) || y >= i64::from(blend_tile.height) {
        return None;
    }
    usize::try_from(y.checked_mul(i64::from(blend_tile.width))?.checked_add(x)?).ok()
}

fn corners(record: &CliffRecord, order: &Order) -> [[f32; 2]; 4] {
    let mut out = [[0.0f32; 2]; 4];
    for (slot, corner) in order.slot_to_corner.iter().enumerate() {
        if let (Some(source), Some(target)) = (record.tex_coords.get(slot), out.get_mut(*corner)) {
            *target = [source.0, source.1];
        }
    }
    out
}

fn record_of(blend_tile: &BlendTileData, index: usize) -> Option<&CliffRecord> {
    let value = blend_tile.cliff_info_index.get(index).copied()?;
    let slot = usize::try_from(value).ok()?.checked_sub(1)?;
    blend_tile.cliff_records.get(slot)
}

fn height_at(height_map: &HeightMapData, x: i64, y: i64) -> f32 {
    let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
        return 0.0;
    };
    f32::from(height_map.sample(x, y).unwrap_or(0))
}

fn quad_heights(height_map: &HeightMapData, x: i64, y: i64) -> [f32; 4] {
    [
        height_at(height_map, x, y),
        height_at(height_map, x.saturating_add(1), y),
        height_at(height_map, x, y.saturating_add(1)),
        height_at(height_map, x.saturating_add(1), y.saturating_add(1)),
    ]
}

fn quad_xy(height_map: &HeightMapData, x: i64, y: i64) -> [[f32; 2]; 4] {
    let world =
        |v: i64| cell_to_world(i32::try_from(v).unwrap_or(i32::MAX), height_map.border_size);
    let (x0, x1) = (world(x), world(x.saturating_add(1)));
    let (y0, y1) = (world(y), world(y.saturating_add(1)));
    [[x0, y0], [x1, y0], [x0, y1], [x1, y1]]
}

fn base_quad_uv(tile: CellTile) -> [[f32; 2]; 4] {
    [
        [tile.uv_xy[0], tile.uv_xy[1]],
        [tile.uv_x1y1[0], tile.uv_xy[1]],
        [tile.uv_xy[0], tile.uv_x1y1[1]],
        [tile.uv_x1y1[0], tile.uv_x1y1[1]],
    ]
}

#[allow(clippy::too_many_arguments)]
fn split_imbalance(
    uv: [[f32; 2]; 4],
    xy: [[f32; 2]; 4],
    z: [f32; 4],
    split: [[usize; 3]; 2],
    width: u32,
    height: u32,
    scale: f32,
) -> Option<f64> {
    let mut scales = [0.0f64; 2];
    for (slot, tri) in split.into_iter().enumerate() {
        let area = triangle_area(
            [pick(&xy, tri, 0), pick(&xy, tri, 1), pick(&xy, tri, 2)],
            [pick(&z, tri, 0), pick(&z, tri, 1), pick(&z, tri, 2)],
            scale,
        );
        let texels = uv_texels(
            [pick(&uv, tri, 0), pick(&uv, tri, 1), pick(&uv, tri, 2)],
            width,
            height,
        );
        if area <= 1e-6 || texels <= 1e-9 {
            return None;
        }
        *scales.get_mut(slot)? = (texels / area).sqrt();
    }
    let (a, b) = (*scales.first()?, *scales.get(1)?);
    Some((a - b).abs() / (a + b))
}

#[allow(clippy::too_many_lines)]
fn check_map(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    vfs: &Vfs,
    types: &TerrainTypes,
    totals: &mut Totals,
) {
    let layout = TileLayout::new(&blend_tile.texture_classes);
    let loaded = load_class_textures(vfs, types, &blend_tile.texture_classes);
    let textures = &loaded.textures;
    totals.cells = totals
        .cells
        .saturating_add(u64::from(blend_tile.cell_count));

    let record_count = u32::try_from(blend_tile.cliff_records.len()).unwrap_or(u32::MAX);
    for record in &blend_tile.cliff_records {
        totals.records = totals.records.saturating_add(1);
        if class_of(blend_tile, record.tile_index >> 2).is_some() {
            totals.shifted_in_class = totals.shifted_in_class.saturating_add(1);
        }
        if class_of(blend_tile, record.tile_index).is_some() {
            totals.direct_in_class = totals.direct_in_class.saturating_add(1);
        }
        *totals.flip.entry(record.flip).or_default() += 1;
        *totals.second_byte.entry(record.unknown).or_default() += 1;
        let tile = record.tile_index >> 2;
        if let Some(class) = class_of(blend_tile, tile) {
            if let Some(entry) = blend_tile.texture_classes.get(class) {
                *totals
                    .record_local_tile
                    .entry(tile.saturating_sub(entry.first_tile))
                    .or_default() += 1;
            }
        }
    }

    let mut usage: BTreeMap<u32, u64> = BTreeMap::new();
    for value in &blend_tile.cliff_info_index {
        if *value != 0 {
            *usage.entry(*value).or_default() += 1;
        }
    }
    totals.records_shared = totals.records_shared.saturating_add(
        u64::try_from(usage.values().filter(|count| **count > 1).count()).unwrap_or(u64::MAX),
    );

    let uses_records = blend_tile.cliff_info_index.iter().any(|value| *value != 0);
    if uses_records {
        totals.files_using_records = totals.files_using_records.saturating_add(1);
    }

    let Some(order) = ORDERS.first() else {
        return;
    };
    let last_x = i64::from(blend_tile.width).saturating_sub(1);
    let last_y = i64::from(blend_tile.height).saturating_sub(1);
    let mut y = 0i64;
    while y < last_y {
        let mut x = 0i64;
        while x < last_x {
            let Some(index) = cell_index(blend_tile, x, y) else {
                x = x.saturating_add(1);
                continue;
            };
            if blend_tile.cliff_info_index.get(index).copied().unwrap_or(0) > record_count {
                totals.index_past_records = totals.index_past_records.saturating_add(1);
            }
            let base = blend_tile
                .tile_index
                .get(index)
                .copied()
                .and_then(|value| layout.cell(value));
            for array in [&blend_tile.blend_index, &blend_tile.extra_blend_index] {
                if array.get(index).copied().unwrap_or(0) != 0 {
                    totals.overlays_total = totals.overlays_total.saturating_add(1);
                }
            }

            if uses_records {
                let sample = |dx: i64, dy: i64| {
                    let (sx, sy) = (x.saturating_add(dx), y.saturating_add(dy));
                    u32::try_from(sx)
                        .ok()
                        .zip(u32::try_from(sy).ok())
                        .and_then(|(sx, sy)| height_map.sample(sx, sy))
                        .unwrap_or(0)
                };
                let corners = [sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1)];
                let low = corners.iter().copied().min().unwrap_or(0);
                let high = corners.iter().copied().max().unwrap_or(0);
                let drop = high.saturating_sub(low);
                if blend_tile.cliff_info_index.get(index).copied().unwrap_or(0) == 0 {
                    totals.drop_without_record.push(drop);
                } else {
                    totals.drop_with_record.push(drop);
                }
            }

            match record_of(blend_tile, index) {
                Some(record) => {
                    totals.cliff_cells = totals.cliff_cells.saturating_add(1);
                    let record_class = class_of(blend_tile, record.tile_index >> 2);
                    if record_class == base.map(|tile| tile.class) {
                        totals.class_agrees = totals.class_agrees.saturating_add(1);
                    }
                    if let Some(entry) =
                        record_class.and_then(|class| blend_tile.texture_classes.get(class))
                    {
                        *totals
                            .used_local_tile
                            .entry((record.tile_index >> 2).saturating_sub(entry.first_tile))
                            .or_default() += 1;
                    }
                    let span = |axis: fn(&(f32, f32)) -> f32| -> f32 {
                        let values = record.tex_coords.iter().map(axis);
                        values.clone().fold(f32::MIN, f32::max) - values.fold(f32::MAX, f32::min)
                    };
                    totals.used_u_span.push(span(|pair| pair.0));
                    totals.used_v_span.push(span(|pair| pair.1));
                    if blend_tile.blend_index.get(index).copied().unwrap_or(0) != 0 {
                        totals.cliff_and_blend = totals.cliff_and_blend.saturating_add(1);
                        measure_cliff_overlay(
                            blend_tile, &layout, textures, order, x, y, index, totals,
                        );
                    }
                    measure_overlay_uv(
                        height_map, blend_tile, &layout, textures, x, y, index, totals,
                    );
                    let h = quad_heights(height_map, x, y);
                    *totals
                        .flip_vs_height
                        .entry((record.flip, h[A] + h[D] >= h[B] + h[C]))
                        .or_default() += 1;
                    second_byte_probes(blend_tile, record, x, y, order, totals);
                    *totals
                        .second_vs_class
                        .entry((record.unknown, record_class == base.map(|tile| tile.class)))
                        .or_default() += 1;
                    *totals
                        .second_vs_blend
                        .entry((
                            record.unknown,
                            blend_tile.blend_index.get(index).copied().unwrap_or(0) != 0,
                        ))
                        .or_default() += 1;

                    let uv = corners(record, order);
                    let xy = quad_xy(height_map, x, y);
                    if let (Some(class), Some(base)) = (record_class, base) {
                        if let Some(Some(texture)) = textures.get(class) {
                            if let Some(level) = texture.levels.first() {
                                for (slot, scale) in FOLD_SCALES.iter().enumerate() {
                                    let bc = split_imbalance(
                                        uv,
                                        xy,
                                        h,
                                        SPLIT_BC,
                                        level.width,
                                        level.height,
                                        *scale,
                                    );
                                    let ad = split_imbalance(
                                        uv,
                                        xy,
                                        h,
                                        SPLIT_AD,
                                        level.width,
                                        level.height,
                                        *scale,
                                    );
                                    let (Some(bc), Some(ad)) = (bc, ad) else {
                                        continue;
                                    };
                                    let key = u32::try_from(slot).unwrap_or(0);
                                    *totals
                                        .flip_vs_fold_by_scale
                                        .entry((key, record.flip, ad < bc))
                                        .or_default() += 1;
                                    if (*scale - DEFAULT_VERTICAL_SCALE).abs() < f32::EPSILON {
                                        *totals
                                            .flip_vs_fold
                                            .entry((record.flip, ad < bc))
                                            .or_default() += 1;
                                    }
                                }
                            }
                            collect_density(totals, texture, uv, base_quad_uv(base), xy, h);
                        }
                    }
                }
                None => {
                    if x % BASELINE_STRIDE == 0 && y % BASELINE_STRIDE == 0 {
                        if let Some(base) = base {
                            if let Some(Some(texture)) = textures.get(base.class) {
                                collect_baseline(
                                    totals,
                                    texture,
                                    base_quad_uv(base),
                                    quad_xy(height_map, x, y),
                                    quad_heights(height_map, x, y),
                                );
                            }
                        }
                    }
                }
            }
            x = x.saturating_add(1);
        }
        y = y.saturating_add(1);
    }

    measure_stretch(height_map, blend_tile, &layout, textures, totals);
    measure_corner_order(blend_tile, totals);
}

fn second_byte_probes(
    blend_tile: &BlendTileData,
    record: &CliffRecord,
    x: i64,
    y: i64,
    order: &Order,
    totals: &mut Totals,
) {
    let p = corners(record, order);
    let parallelogram = match (p.first(), p.get(1), p.get(2), p.get(3)) {
        (Some(a), Some(b), Some(c), Some(d)) => {
            ((b[0] - a[0]) - (d[0] - c[0])).abs() < 1e-4
                && ((b[1] - a[1]) - (d[1] - c[1])).abs() < 1e-4
        }
        _ => false,
    };
    *totals
        .second_vs_shape
        .entry((record.unknown, parallelogram))
        .or_default() += 1;

    let mut interior = true;
    let mut seam_open = false;
    for (dx, dy, mine, theirs) in [
        (1i64, 0i64, B, A),
        (-1, 0, A, B),
        (0, 1, C, A),
        (0, -1, A, C),
    ] {
        let neighbour = cell_index(blend_tile, x.saturating_add(dx), y.saturating_add(dy))
            .and_then(|n| record_of(blend_tile, n));
        match neighbour {
            None => interior = false,
            Some(other) => {
                let q = corners(other, order);
                if let (Some(a), Some(b)) = (p.get(mine), q.get(theirs)) {
                    if (a[0] - b[0]).abs() + (a[1] - b[1]).abs() > 1e-5 {
                        seam_open = true;
                    }
                }
            }
        }
    }
    *totals
        .second_vs_interior
        .entry((record.unknown, interior))
        .or_default() += 1;
    *totals
        .second_vs_seam
        .entry((record.unknown, seam_open))
        .or_default() += 1;

    let outside = record
        .tex_coords
        .iter()
        .any(|(u, v)| !(0.0..=1.0).contains(u) || !(0.0..=1.0).contains(v));
    *totals
        .second_vs_repeat
        .entry((record.unknown, outside))
        .or_default() += 1;
}

fn measure_stretch(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    textures: &[Option<TextureData>],
    totals: &mut Totals,
) {
    let mut y = 0i64;
    while y < i64::from(blend_tile.height) {
        let mut x = 0i64;
        while x < i64::from(blend_tile.width) {
            let index = usize::try_from(
                y.saturating_mul(i64::from(blend_tile.width))
                    .saturating_add(x),
            )
            .unwrap_or(usize::MAX);
            let Some(base) = blend_tile
                .tile_index
                .get(index)
                .copied()
                .and_then(|value| layout.cell(value))
            else {
                x = x.saturating_add(1);
                continue;
            };
            let gated = blend_tile
                .cliff_info_index
                .get(index)
                .copied()
                .and_then(|value| {
                    cell_cliff(layout, &blend_tile.cliff_records, value, Some(base.class))
                })
                .is_none();
            if !gated {
                x = x.saturating_add(1);
                continue;
            }
            totals.stretch_candidates = totals.stretch_candidates.saturating_add(1);
            let h = quad_heights(height_map, x, y);
            let sample = |cx: i64, cy: i64| -> u16 {
                let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy)) else {
                    return 0;
                };
                height_map.sample(cx, cy).unwrap_or(0)
            };
            let (x1, y1) = (x.saturating_add(1), y.saturating_add(1));
            let samples = [sample(x, y), sample(x1, y), sample(x1, y1), sample(x, y1)];
            let Some(stretched) = stretch_uv(
                base.uv_xy,
                base.uv_x1y1,
                samples,
                DEFAULT_VERTICAL_SCALE / MAP_XY_FACTOR,
            ) else {
                x = x.saturating_add(1);
                continue;
            };
            totals.stretch_applied = totals.stretch_applied.saturating_add(1);
            if let Some(Some(texture)) = textures.get(base.class) {
                if let Some(level) = texture.levels.first() {
                    let xy = quad_xy(height_map, x, y);
                    let plain = base_quad_uv(base);
                    for tri in SPLIT_BC {
                        let area = triangle_area(
                            [pick(&xy, tri, 0), pick(&xy, tri, 1), pick(&xy, tri, 2)],
                            [pick(&h, tri, 0), pick(&h, tri, 1), pick(&h, tri, 2)],
                            DEFAULT_VERTICAL_SCALE,
                        );
                        if area <= 1e-6 {
                            continue;
                        }
                        let before = uv_texels(
                            [
                                pick(&plain, tri, 0),
                                pick(&plain, tri, 1),
                                pick(&plain, tri, 2),
                            ],
                            level.width,
                            level.height,
                        );
                        let after = uv_texels(
                            [
                                pick(&stretched, tri, 0),
                                pick(&stretched, tri, 1),
                                pick(&stretched, tri, 2),
                            ],
                            level.width,
                            level.height,
                        );
                        if totals.stretch_before.len() < BASELINE_KEPT {
                            totals.stretch_before.push(before / area);
                            totals.stretch_after.push(after / area);
                        }
                    }
                }
            }
            x = x.saturating_add(1);
        }
        y = y.saturating_add(1);
    }
}

fn collect_density(
    totals: &mut Totals,
    texture: &TextureData,
    cliff_uv: [[f32; 2]; 4],
    base_uv: [[f32; 2]; 4],
    xy: [[f32; 2]; 4],
    z: [f32; 4],
) {
    let Some(level) = texture.levels.first() else {
        return;
    };
    for (p, q) in [(A, B), (C, D), (A, C), (B, D)] {
        let (Some(pu), Some(qu), Some(pxy), Some(qxy), Some(pz), Some(qz)) = (
            cliff_uv.get(p),
            cliff_uv.get(q),
            xy.get(p),
            xy.get(q),
            z.get(p),
            z.get(q),
        ) else {
            continue;
        };
        let texels = {
            let du = f64::from(qu[0] - pu[0]) * f64::from(level.width);
            let dv = f64::from(qu[1] - pu[1]) * f64::from(level.height);
            (du * du + dv * dv).sqrt()
        };
        let drop = f64::from((qz - pz).abs());
        let world = {
            let dx = f64::from(qxy[0] - pxy[0]);
            let dy = f64::from(qxy[1] - pxy[1]);
            let dz = drop * f64::from(DEFAULT_VERTICAL_SCALE);
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        if world <= 1e-6 || texels <= 1e-9 {
            continue;
        }
        if drop < 100.0 {
            totals.edge_level.add(texels / world);
        } else if drop >= 1000.0 {
            totals.edge_steep.add(texels / world);
        }
        let flat_length = {
            let dx = f64::from(qxy[0] - pxy[0]);
            let dy = f64::from(qxy[1] - pxy[1]);
            (dx * dx + dy * dy).sqrt()
        };
        totals.cliff_edges.push((texels, flat_length, drop));
    }
    for tri in SPLIT_BC {
        let cliff = uv_texels(
            [
                pick(&cliff_uv, tri, 0),
                pick(&cliff_uv, tri, 1),
                pick(&cliff_uv, tri, 2),
            ],
            level.width,
            level.height,
        );
        let base = uv_texels(
            [
                pick(&base_uv, tri, 0),
                pick(&base_uv, tri, 1),
                pick(&base_uv, tri, 2),
            ],
            level.width,
            level.height,
        );
        totals.cliff_triangles.push(CliffTriangle {
            xy: [pick(&xy, tri, 0), pick(&xy, tri, 1), pick(&xy, tri, 2)],
            z: [pick(&z, tri, 0), pick(&z, tri, 1), pick(&z, tri, 2)],
            cliff_texels: cliff,
            base_texels: base,
        });
        if base > 1e-9 {
            totals.density_ratio.push(cliff / base);
        }
    }
}

fn collect_baseline(
    totals: &mut Totals,
    texture: &TextureData,
    base_uv: [[f32; 2]; 4],
    xy: [[f32; 2]; 4],
    z: [f32; 4],
) {
    let Some(level) = texture.levels.first() else {
        return;
    };
    for (p, q) in [(A, B), (C, D)] {
        let (Some(pu), Some(qu), Some(pz), Some(qz)) =
            (base_uv.get(p), base_uv.get(q), z.get(p), z.get(q))
        else {
            continue;
        };
        if (qz - pz).abs() >= 100.0 {
            continue;
        }
        let du = f64::from(qu[0] - pu[0]) * f64::from(level.width);
        let dv = f64::from(qu[1] - pu[1]) * f64::from(level.height);
        let texels = (du * du + dv * dv).sqrt();
        let world = {
            let dx = f64::from(qxy_x(xy, q) - qxy_x(xy, p));
            let dy = f64::from(qxy_y(xy, q) - qxy_y(xy, p));
            let dz = f64::from((qz - pz).abs()) * f64::from(DEFAULT_VERTICAL_SCALE);
            (dx * dx + dy * dy + dz * dz).sqrt()
        };
        if world > 1e-6 && texels > 1e-9 {
            totals.edge_level_base.add(texels / world);
        }
    }
    for tri in SPLIT_BC {
        let texels = uv_texels(
            [
                pick(&base_uv, tri, 0),
                pick(&base_uv, tri, 1),
                pick(&base_uv, tri, 2),
            ],
            level.width,
            level.height,
        );
        let area = triangle_area(
            [pick(&xy, tri, 0), pick(&xy, tri, 1), pick(&xy, tri, 2)],
            [pick(&z, tri, 0), pick(&z, tri, 1), pick(&z, tri, 2)],
            DEFAULT_VERTICAL_SCALE,
        );
        if area > 1e-6 {
            let density = texels / area;
            totals.baseline_texels += texels;
            totals.baseline_area += area;
            totals.baseline.add(density);
            if totals.baseline_samples.len() < BASELINE_KEPT {
                totals.baseline_samples.push(density);
            }
        }
    }
}

fn qxy_x(xy: [[f32; 2]; 4], corner: usize) -> f32 {
    xy.get(corner).map_or(0.0, |p| p[0])
}

fn qxy_y(xy: [[f32; 2]; 4], corner: usize) -> f32 {
    xy.get(corner).map_or(0.0, |p| p[1])
}

const SHARED_X: [(usize, usize); 2] = [(B, A), (D, C)];
const SHARED_Y: [(usize, usize); 2] = [(C, A), (D, B)];

fn measure_cliff_overlay(
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    textures: &[Option<TextureData>],
    order: &Order,
    x: i64,
    y: i64,
    index: usize,
    totals: &mut Totals,
) {
    let Some(overlay) = blend_tile
        .blend_index
        .get(index)
        .copied()
        .and_then(|value| cell_blend(layout, &blend_tile.blend_records, value))
    else {
        return;
    };
    let Some(record) = record_of(blend_tile, index) else {
        return;
    };
    let covered = overlay.ramp.map(|value| value >= 1.0);
    let (dx, dy, shared) = match covered {
        [false, true, false, true] => (1i64, 0i64, [(B, A), (D, C)]),
        [true, false, true, false] => (-1, 0, [(A, B), (C, D)]),
        [false, false, true, true] => (0, 1, [(C, A), (D, B)]),
        [true, true, false, false] => (0, -1, [(A, C), (B, D)]),
        _ => return,
    };
    let Some(neighbour) = cell_index(blend_tile, x.saturating_add(dx), y.saturating_add(dy)) else {
        return;
    };

    let (neighbour_class, neighbour_uv) = match record_of(blend_tile, neighbour) {
        Some(other) => (
            class_of(blend_tile, other.tile_index >> 2),
            corners(other, order),
        ),
        None => {
            let Some(tile) = blend_tile
                .tile_index
                .get(neighbour)
                .copied()
                .and_then(|value| layout.cell(value))
            else {
                return;
            };
            (Some(tile.class), base_quad_uv(tile))
        }
    };
    if neighbour_class != Some(overlay.tile.class) {
        totals.overlay_class_mismatch = totals.overlay_class_mismatch.saturating_add(1);
        return;
    }
    let Some(Some(texture)) = textures.get(overlay.tile.class) else {
        return;
    };
    let Some(level) = texture.levels.first() else {
        return;
    };

    let flat = base_quad_uv(overlay.tile);
    let cliff = corners(record, order);
    for (mine, theirs) in shared {
        let (Some(flat_uv), Some(cliff_uv), Some(other_uv)) =
            (flat.get(mine), cliff.get(mine), neighbour_uv.get(theirs))
        else {
            continue;
        };
        let (Some(pa), Some(pb), Some(pc)) = (
            texel_at(level, flat_uv[0], flat_uv[1]),
            texel_at(level, cliff_uv[0], cliff_uv[1]),
            texel_at(level, other_uv[0], other_uv[1]),
        ) else {
            continue;
        };
        let step = 1.0 / f32::from(u16::try_from(level.width.max(1)).unwrap_or(u16::MAX));
        let Some(pd) = texel_at(level, other_uv[0] + step, other_uv[1]) else {
            continue;
        };
        totals.overlay_flat.add(channel_difference(pa, pc));
        totals.overlay_cliff.add(channel_difference(pb, pc));
        totals.overlay_floor.add(channel_difference(pd, pc));
        if class_of(blend_tile, record.tile_index >> 2) == Some(overlay.tile.class) {
            totals.overlay_flat_gated.add(channel_difference(pa, pc));
            totals.overlay_cliff_gated.add(channel_difference(pb, pc));
            totals.overlay_floor_gated.add(channel_difference(pd, pc));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn measure_overlay_uv(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    textures: &[Option<TextureData>],
    x: i64,
    y: i64,
    index: usize,
    totals: &mut Totals,
) {
    let Some(cliff_index) = blend_tile
        .cliff_info_index
        .get(index)
        .copied()
        .filter(|value| *value != 0)
    else {
        return;
    };
    let sample = |cx: i64, cy: i64| -> u16 {
        let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy)) else {
            return 0;
        };
        height_map.sample(cx, cy).unwrap_or(0)
    };
    let (x1, y1) = (x.saturating_add(1), y.saturating_add(1));
    let samples = [sample(x, y), sample(x1, y), sample(x1, y1), sample(x, y1)];
    let xy = quad_xy(height_map, x, y);
    let h = quad_heights(height_map, x, y);

    for array in [&blend_tile.blend_index, &blend_tile.extra_blend_index] {
        let Some(overlay) = array
            .get(index)
            .copied()
            .and_then(|value| cell_blend(layout, &blend_tile.blend_records, value))
        else {
            continue;
        };
        totals.overlay_layers = totals.overlay_layers.saturating_add(1);
        let plain = base_quad_uv(overlay.tile);
        let took_record = cell_cliff(
            layout,
            &blend_tile.cliff_records,
            cliff_index,
            Some(overlay.tile.class),
        )
        .is_some();
        let Some(after) = overlay_cliff_uv(
            layout,
            &blend_tile.cliff_records,
            cliff_index,
            (overlay.tile.uv_xy, overlay.tile.uv_x1y1),
            overlay.tile.class,
            samples,
            DEFAULT_VERTICAL_SCALE / MAP_XY_FACTOR,
        ) else {
            totals.overlay_untouched = totals.overlay_untouched.saturating_add(1);
            continue;
        };
        if took_record {
            totals.overlay_gate_pass = totals.overlay_gate_pass.saturating_add(1);
        } else {
            totals.overlay_stretched = totals.overlay_stretched.saturating_add(1);
            let low = samples.iter().copied().min().unwrap_or(0);
            let high = samples.iter().copied().max().unwrap_or(0);
            totals
                .pending_exemplars
                .push((x, y, u64::from(high.saturating_sub(low))));
        }
        let Some(Some(texture)) = textures.get(overlay.tile.class) else {
            continue;
        };
        let Some(level) = texture.levels.first() else {
            continue;
        };
        for tri in SPLIT_BC {
            let area = triangle_area(
                [pick(&xy, tri, 0), pick(&xy, tri, 1), pick(&xy, tri, 2)],
                [pick(&h, tri, 0), pick(&h, tri, 1), pick(&h, tri, 2)],
                DEFAULT_VERTICAL_SCALE,
            );
            if area <= 1e-6 {
                continue;
            }
            let texels = |uv: &[[f32; 2]; 4]| {
                uv_texels(
                    [pick(uv, tri, 0), pick(uv, tri, 1), pick(uv, tri, 2)],
                    level.width,
                    level.height,
                )
            };
            if totals.overlay_before.len() < BASELINE_KEPT {
                totals.overlay_before.push(texels(&plain) / area);
                totals.overlay_after.push(texels(&after) / area);
                let (before_split, after_split) = if took_record {
                    (
                        &mut totals.overlay_record_before,
                        &mut totals.overlay_record_after,
                    )
                } else {
                    (
                        &mut totals.overlay_stretch_before,
                        &mut totals.overlay_stretch_after,
                    )
                };
                before_split.push(texels(&plain) / area);
                after_split.push(texels(&after) / area);
            }
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn texel_at(level: &mint_render::MipLevel, u: f32, v: f32) -> Option<[u8; 3]> {
    if level.width == 0 || level.height == 0 {
        return None;
    }
    let wrap = |value: f32, size: u32| -> Option<u32> {
        let size_f = f64::from(size);
        let scaled = (f64::from(value) * size_f).floor();
        if !scaled.is_finite() {
            return None;
        }
        let wrapped = scaled.rem_euclid(size_f);
        if !(0.0..size_f).contains(&wrapped) {
            return None;
        }
        Some((wrapped as u32).min(size.saturating_sub(1)))
    };
    let (tx, ty) = (wrap(u, level.width)?, wrap(v, level.height)?);
    let offset = usize::try_from(ty)
        .ok()?
        .checked_mul(usize::try_from(level.width).ok()?)?
        .checked_add(usize::try_from(tx).ok()?)?
        .checked_mul(4)?;
    let bytes = level.data.get(offset..offset.checked_add(3)?)?;
    Some([
        bytes.first().copied()?,
        bytes.get(1).copied()?,
        bytes.get(2).copied()?,
    ])
}

fn channel_difference(a: [u8; 3], b: [u8; 3]) -> f64 {
    let total: u32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| u32::from(x.abs_diff(*y)))
        .sum();
    f64::from(total) / 3.0
}

fn measure_corner_order(blend_tile: &BlendTileData, totals: &mut Totals) {
    for horizontal in [true, false] {
        let (dx, dy) = if horizontal { (1i64, 0i64) } else { (0, 1) };
        let mut y = 0i64;
        while y < i64::from(blend_tile.height) {
            let mut x = 0i64;
            while x < i64::from(blend_tile.width) {
                compare_pair(blend_tile, x, y, dx, dy, horizontal, totals);
                x = x.saturating_add(1);
            }
            y = y.saturating_add(1);
        }
    }
}

fn compare_pair(
    blend_tile: &BlendTileData,
    x: i64,
    y: i64,
    dx: i64,
    dy: i64,
    horizontal: bool,
    totals: &mut Totals,
) {
    let (Some(here), Some(there)) = (
        cell_index(blend_tile, x, y),
        cell_index(blend_tile, x.saturating_add(dx), y.saturating_add(dy)),
    ) else {
        return;
    };
    let (Some(a), Some(b)) = (record_of(blend_tile, here), record_of(blend_tile, there)) else {
        return;
    };
    {
        let same_record_class =
            class_of(blend_tile, a.tile_index >> 2) == class_of(blend_tile, b.tile_index >> 2);
        let cell_class = |at: usize| {
            blend_tile
                .tile_index
                .get(at)
                .map(|value| i32::from(*value) >> 2)
                .and_then(|tile| class_of(blend_tile, tile))
        };
        let same_cell_class = cell_class(here) == cell_class(there);
        if let Some(order) = ORDERS.first() {
            let shared = if horizontal { SHARED_X } else { SHARED_Y };
            let (ca, cb) = (corners(a, order), corners(b, order));
            let mut difference = 0.0f64;
            for (mine, theirs) in shared {
                if let (Some(p), Some(q)) = (ca.get(mine), cb.get(theirs)) {
                    difference += f64::from((p[0] - q[0]).abs()) + f64::from((p[1] - q[1]).abs());
                }
            }
            *totals
                .seam_vs_class_source
                .entry((same_record_class, same_cell_class, difference / 2.0 < 1e-5))
                .or_default() += 1;
        }
    }
    if class_of(blend_tile, a.tile_index >> 2) != class_of(blend_tile, b.tile_index >> 2) {
        totals.cross_class_pairs = totals.cross_class_pairs.saturating_add(1);
        let shared = if horizontal { SHARED_X } else { SHARED_Y };
        let Some(order) = ORDERS.first() else {
            return;
        };
        let (ca, cb) = (corners(a, order), corners(b, order));
        let mut difference = 0.0f64;
        for (mine, theirs) in shared {
            let (Some(p), Some(q)) = (ca.get(mine), cb.get(theirs)) else {
                continue;
            };
            difference += f64::from((p[0] - q[0]).abs()) + f64::from((p[1] - q[1]).abs());
        }
        let difference = difference / 2.0;
        totals.cross_class_difference.add(difference);
        if difference < 1e-5 {
            totals.cross_class_exact = totals.cross_class_exact.saturating_add(1);
        }
        return;
    }
    let shared = if horizontal { SHARED_X } else { SHARED_Y };
    for (index, order) in ORDERS.iter().enumerate() {
        let (ca, cb) = (corners(a, order), corners(b, order));
        let mut difference = 0.0f64;
        for (mine, theirs) in shared {
            let (Some(p), Some(q)) = (ca.get(mine), cb.get(theirs)) else {
                continue;
            };
            difference += f64::from((p[0] - q[0]).abs()) + f64::from((p[1] - q[1]).abs());
        }
        let difference = difference / 2.0;
        totals.add_seam(index, difference, horizontal);
        if index == 0 {
            let bucket = match difference {
                d if d < 1e-5 => "exact (< 1e-5)",
                d if d < 0.001 => "< 0.001",
                d if d < 0.01 => "< 0.01",
                d if d < 0.1 => "< 0.1",
                d if d < 0.5 => "< 0.5",
                _ => ">= 0.5",
            };
            *totals.residual.entry(bucket).or_default() += 1;
            let adjacent = blend_tile
                .cliff_info_index
                .get(here)
                .zip(blend_tile.cliff_info_index.get(there))
                .is_some_and(|(p, q)| p.abs_diff(*q) == 1);
            *totals
                .residual_vs_run
                .entry((difference < 1e-5, adjacent))
                .or_default() += 1;
            *totals
                .residual_vs_flip
                .entry((difference < 1e-5, a.flip == b.flip))
                .or_default() += 1;
            if difference >= 1e-5 {
                for (mine, theirs) in shared {
                    let Some(p) = ca.get(mine) else { continue };
                    let matched = (0..4).find(|corner| {
                        cb.get(*corner)
                            .is_some_and(|q| (p[0] - q[0]).abs() + (p[1] - q[1]).abs() < 1e-5)
                    });
                    let name = match matched {
                        None => "no corner of the neighbour",
                        Some(m) if m == theirs => "the expected corner",
                        Some(0) => "the neighbour's (x,y)",
                        Some(1) => "the neighbour's (x+1,y)",
                        Some(2) => "the neighbour's (x,y+1)",
                        _ => "the neighbour's (x+1,y+1)",
                    };
                    *totals.open_seam_match.entry(name).or_default() += 1;
                }
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

#[allow(dead_code)]
fn library_entry_point(
    layout: &TileLayout,
    records: &[CliffRecord],
    index: u32,
    base_class: Option<usize>,
) -> Option<[[f32; 2]; 4]> {
    cell_cliff(layout, records, index, base_class).map(|cliff| cliff.uv)
}
