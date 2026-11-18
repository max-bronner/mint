use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::MapFile;

const VERTICAL_SCALE: f32 = 5.0 / 128.0;

const THRESHOLD_SWEEP: &[f32] = &[
    1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 15.0, 20.0, 30.0, 40.0,
];

fn main() -> std::process::ExitCode {
    let mut installs: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--install" => match args.next() {
                Some(path) => installs.push(PathBuf::from(path)),
                None => {
                    eprintln!("--install wants a path");
                    return std::process::ExitCode::FAILURE;
                }
            },
            other => installs.push(PathBuf::from(other)),
        }
    }
    if installs.is_empty() {
        eprintln!(
            "usage: cliff_plane_corpus <install dir> [--install <dir>]\n\
             scores every BlendTileData bit plane against is_cliff() and cliff_info_index"
        );
        return std::process::ExitCode::FAILURE;
    }

    let mut by_version: BTreeMap<u16, VersionTotals> = BTreeMap::new();
    let mut parse_failures = Vec::new();
    let mut sample_maps: BTreeMap<u16, String> = BTreeMap::new();
    let started = Instant::now();
    let mut files_seen = 0u64;

    for install in &installs {
        for archive_path in mint_sage_big::archives_under(install) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            let archive_label = archive_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let name = format!("{archive_label}::{}", entry.name);
                files_seen += 1;
                let Ok(bytes) = archive.read_entry(entry) else {
                    parse_failures.push(format!("{name}: unreadable BIG entry"));
                    continue;
                };
                let map = match MapFile::parse(&bytes) {
                    Ok(map) => map,
                    Err(error) => {
                        parse_failures.push(format!("{name}: {error}"));
                        continue;
                    }
                };
                if map.height_map.width < 2 || map.height_map.height < 2 {
                    continue;
                }
                sample_maps
                    .entry(map.blend_tile.version)
                    .or_insert_with(|| name.clone());
                tally(
                    &name,
                    &map,
                    by_version.entry(map.blend_tile.version).or_default(),
                );
            }
        }
    }

    println!(
        "{files_seen} .map/.bse archive entr(ies) seen, {} parse failure(s), in {:.1} s",
        parse_failures.len(),
        started.elapsed().as_secs_f64()
    );
    if !parse_failures.is_empty() {
        println!("\n{} parse failure(s):", parse_failures.len());
        for f in &parse_failures {
            println!("  {f}");
        }
    }

    for (version, totals) in &by_version {
        println!(
            "\n=== version {version} ({} plane(s), {} cell(s) compared) ===",
            totals.plane_count, totals.cells
        );
        println!(
            "  is_cliff() baseline: {} of {} cells are cliffs at the 9.8 threshold ({:.2}%); \
             {} of {} cells are textured as cliff ({:.3}%)",
            totals.is_cliff_count,
            totals.cells,
            percent(totals.is_cliff_count, totals.cells),
            totals.textured_count,
            totals.cells,
            percent(totals.textured_count, totals.cells),
        );
        for plane in &totals.planes {
            for order in [BitOrder::Lsb, BitOrder::Msb] {
                let m = plane.for_order(order);
                println!(
                    "  plane {:>2} {:<4}: set {:>9} ({:>5.2}%) | vs is_cliff: acc {:>5.2}% \
                     prec {:>5.2}% rec {:>5.2}% | vs textured: acc {:>5.2}% prec {:>5.2}% \
                     rec {:>5.2}%",
                    plane.index,
                    order.label(),
                    m.set_count,
                    percent(m.set_count, totals.cells),
                    percent(m.tp_cliff + m.tn_cliff, totals.cells),
                    precision(m.tp_cliff, m.fp_cliff),
                    recall(m.tp_cliff, m.fn_cliff),
                    percent(m.tp_tex + m.tn_tex, totals.cells),
                    precision(m.tp_tex, m.fp_tex),
                    recall(m.tp_tex, m.fn_tex),
                );
            }
        }
        if totals
            .plane0_sweep
            .iter()
            .any(|c| c.tp_cliff + c.fp_cliff + c.fn_cliff > 0)
        {
            println!("  plane 0 lsb, swept against is_cliff() at other thresholds:");
            for (multiplier, c) in THRESHOLD_SWEEP.iter().zip(&totals.plane0_sweep) {
                let acc = percent(c.tp_cliff + c.tn_cliff, totals.cells);
                let prec = precision(c.tp_cliff, c.fp_cliff);
                let rec = recall(c.tp_cliff, c.fn_cliff);
                let f1 = if prec + rec > 0.0 {
                    2.0 * prec * rec / (prec + rec)
                } else {
                    0.0
                };
                println!(
                    "    threshold {:>6.1} ({:>5.2}x): acc {:>5.2}% prec {:>5.2}% rec {:>5.2}% f1 {:>5.2}",
                    mint_sage_passability::CLIFF_SLOPE_LIMIT * multiplier,
                    multiplier,
                    acc,
                    prec,
                    rec,
                    f1,
                );
            }
        }
    }

    println!("\nsample map per version (for the bit-order/stride proof dump):");
    for (version, name) in &sample_maps {
        println!("  version {version}: {name}");
    }

    std::process::ExitCode::SUCCESS
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BitOrder {
    Lsb,
    Msb,
}

impl BitOrder {
    fn label(self) -> &'static str {
        match self {
            BitOrder::Lsb => "lsb",
            BitOrder::Msb => "msb",
        }
    }

    fn bit(self, byte: u8, x: u32) -> bool {
        #[allow(clippy::cast_possible_truncation)]
        let shift = (x & 7) as u8;
        match self {
            BitOrder::Lsb => (byte >> shift) & 1 != 0,
            BitOrder::Msb => (byte >> (7 - shift)) & 1 != 0,
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Confusion {
    set_count: u64,
    tp_cliff: u64,
    fp_cliff: u64,
    fn_cliff: u64,
    tn_cliff: u64,
    tp_tex: u64,
    fp_tex: u64,
    fn_tex: u64,
    tn_tex: u64,
}

struct PlaneTotals {
    index: usize,
    lsb: Confusion,
    msb: Confusion,
}

impl PlaneTotals {
    fn for_order(&self, order: BitOrder) -> &Confusion {
        match order {
            BitOrder::Lsb => &self.lsb,
            BitOrder::Msb => &self.msb,
        }
    }
}

#[derive(Default)]
struct VersionTotals {
    cells: u64,
    is_cliff_count: u64,
    textured_count: u64,
    plane_count: usize,
    planes: Vec<PlaneTotals>,
    plane0_sweep: Vec<Confusion>,
}

fn tally(_name: &str, map: &MapFile, totals: &mut VersionTotals) {
    let bt = &map.blend_tile;
    let width = bt.width;
    let height = bt.height;
    if width == 0 || height == 0 {
        return;
    }
    if totals.planes.is_empty() {
        totals.plane_count = bt.bit_planes.len();
        totals.planes = (0..bt.bit_planes.len())
            .map(|index| PlaneTotals {
                index,
                lsb: Confusion::default(),
                msb: Confusion::default(),
            })
            .collect();
    }
    if totals.plane0_sweep.is_empty() {
        totals.plane0_sweep = vec![Confusion::default(); THRESHOLD_SWEEP.len()];
    }
    let row_stride = width.div_ceil(8);
    let cells_x = width.saturating_sub(1);
    let cells_y = height.saturating_sub(1);
    for y in 0..cells_y {
        for x in 0..cells_x {
            let Some(index) = usize::try_from(y)
                .ok()
                .zip(usize::try_from(width).ok())
                .and_then(|(y, width)| y.checked_mul(width))
                .zip(usize::try_from(x).ok())
                .and_then(|(row_start, x)| row_start.checked_add(x))
            else {
                continue;
            };
            let is_cliff = mint_sage_passability::is_cliff(&map.height_map, VERTICAL_SCALE, x, y);
            let textured = bt
                .cliff_info_index
                .get(index)
                .is_some_and(|value| *value != 0);
            totals.cells = totals.cells.saturating_add(1);
            if is_cliff {
                totals.is_cliff_count = totals.is_cliff_count.saturating_add(1);
            }
            if textured {
                totals.textured_count = totals.textured_count.saturating_add(1);
            }

            for (plane_index, plane_bytes) in bt.bit_planes.iter().enumerate() {
                let Some(row_start) = usize::try_from(y)
                    .ok()
                    .and_then(|y| y.checked_mul(usize::try_from(row_stride).ok()?))
                else {
                    continue;
                };
                let Some(byte_index) = usize::try_from(x / 8)
                    .ok()
                    .and_then(|b| row_start.checked_add(b))
                else {
                    continue;
                };
                let Some(byte) = plane_bytes.get(byte_index).copied() else {
                    continue;
                };
                let Some(slot) = totals.planes.get_mut(plane_index) else {
                    continue;
                };
                for order in [BitOrder::Lsb, BitOrder::Msb] {
                    let bit = order.bit(byte, x);
                    let confusion = match order {
                        BitOrder::Lsb => &mut slot.lsb,
                        BitOrder::Msb => &mut slot.msb,
                    };
                    if bit {
                        confusion.set_count = confusion.set_count.saturating_add(1);
                    }
                    match (bit, is_cliff) {
                        (true, true) => confusion.tp_cliff = confusion.tp_cliff.saturating_add(1),
                        (true, false) => confusion.fp_cliff = confusion.fp_cliff.saturating_add(1),
                        (false, true) => confusion.fn_cliff = confusion.fn_cliff.saturating_add(1),
                        (false, false) => confusion.tn_cliff = confusion.tn_cliff.saturating_add(1),
                    }
                    match (bit, textured) {
                        (true, true) => confusion.tp_tex = confusion.tp_tex.saturating_add(1),
                        (true, false) => confusion.fp_tex = confusion.fp_tex.saturating_add(1),
                        (false, true) => confusion.fn_tex = confusion.fn_tex.saturating_add(1),
                        (false, false) => confusion.tn_tex = confusion.tn_tex.saturating_add(1),
                    }
                    if plane_index == 0 && order == BitOrder::Lsb {
                        for (slot_index, multiplier) in THRESHOLD_SWEEP.iter().enumerate() {
                            let swept = mint_sage_passability::is_cliff(
                                &map.height_map,
                                VERTICAL_SCALE / multiplier,
                                x,
                                y,
                            );
                            let Some(c) = totals.plane0_sweep.get_mut(slot_index) else {
                                continue;
                            };
                            match (bit, swept) {
                                (true, true) => c.tp_cliff = c.tp_cliff.saturating_add(1),
                                (true, false) => c.fp_cliff = c.fp_cliff.saturating_add(1),
                                (false, true) => c.fn_cliff = c.fn_cliff.saturating_add(1),
                                (false, false) => c.tn_cliff = c.tn_cliff.saturating_add(1),
                            }
                        }
                    }
                }
            }
        }
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        100.0 * part as f64 / whole as f64
    }
}

fn precision(tp: u64, fp: u64) -> f64 {
    percent(tp, tp.saturating_add(fp))
}

fn recall(tp: u64, fnn: u64) -> f64 {
    percent(tp, tp.saturating_add(fnn))
}
