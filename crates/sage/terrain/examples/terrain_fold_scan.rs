use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::{BlendTileData, HeightMapData, MapFile};
use mint_vfs::Vfs;

const DECISIVE_RAW: i64 = 100;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Candidate {
    AlwaysBc,
    AlwaysAd,
    Oracle,
    Parity,
    PlaneLsb(usize),
    PlaneMsb(usize),
    CellByte,
    CliffFlip,
    Orientation,
}

impl Candidate {
    fn label(self) -> String {
        match self {
            Self::AlwaysBc => "always B-C (what mint draws)".to_owned(),
            Self::AlwaysAd => "always A-D".to_owned(),
            Self::Oracle => "floor: the better fold per cell".to_owned(),
            Self::Parity => "control: cell parity (x+y)&1".to_owned(),
            Self::PlaneLsb(n) => format!("bit plane {n}, LSB-first"),
            Self::PlaneMsb(n) => format!("bit plane {n}, MSB-first"),
            Self::CellByte => "the trailing per-cell u8, nonzero".to_owned(),
            Self::CliffFlip => "the cliff record's flip byte".to_owned(),
            Self::Orientation => "derived: the orientation rule".to_owned(),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Score {
    error: f64,
    cells: u64,
    agree: u64,
    said_ad: u64,
}

impl Score {
    fn add(&mut self, error: f64, agree: bool, said_ad: bool) {
        self.error += error;
        self.cells = self.cells.saturating_add(1);
        if agree {
            self.agree = self.agree.saturating_add(1);
        }
        if said_ad {
            self.said_ad = self.said_ad.saturating_add(1);
        }
    }

    fn mean(&self) -> f64 {
        if self.cells == 0 {
            f64::INFINITY
        } else {
            self.error / (self.cells as f64)
        }
    }

    fn percent(count: u64, of: u64) -> f64 {
        if of == 0 {
            0.0
        } else {
            (count as f64) * 100.0 / (of as f64)
        }
    }
}

#[derive(Default)]
struct Totals {
    files: u64,
    parsed: u64,
    failures: Vec<String>,
    cells: u64,
    indifferent: u64,
    decisive: u64,
    scores: BTreeMap<Candidate, Score>,
    cliff_scores: BTreeMap<Candidate, Score>,
    flip_vs_reference: BTreeMap<(u8, bool), u64>,
    flip_vs_orientation: BTreeMap<(u8, bool), u64>,
    flip_all: BTreeMap<u8, u64>,
    reference_vs_orientation: BTreeMap<(bool, bool), u64>,
    plane_set: BTreeMap<(usize, bool), u64>,
    plane_counts: BTreeMap<usize, u64>,
    flip_rules: BTreeMap<&'static str, [u64; 3]>,
    flip_residual: BTreeMap<&'static str, u64>,
    flip_on_tie: BTreeMap<(u8, bool), u64>,
    flip_disagree_by_file: BTreeMap<String, u64>,
    head_to_head: BTreeMap<&'static str, Score>,
}

struct FlipRule {
    name: &'static str,
    decide: fn(i64, i64, i64, i64) -> Option<bool>,
}

fn order(a: i64, b: i64) -> Option<bool> {
    match a.cmp(&b) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(true),
        std::cmp::Ordering::Less => Some(false),
    }
}

const FLIP_RULES: &[FlipRule] = &[
    FlipRule {
        name: "the diagonal whose ends differ least",
        decide: |a, b, c, d| order((b - c).abs(), (a - d).abs()),
    },
    FlipRule {
        name: "the diagonal whose ends differ most",
        decide: |a, b, c, d| order((a - d).abs(), (b - c).abs()),
    },
    FlipRule {
        name: "the higher pair (ridge-preserving)",
        decide: |a, b, c, d| order(a + d, b + c),
    },
    FlipRule {
        name: "the lower pair (valley-preserving)",
        decide: |a, b, c, d| order(b + c, a + d),
    },
    FlipRule {
        name: "the pair with the higher maximum",
        decide: |a, b, c, d| order(a.max(d), b.max(c)),
    },
    FlipRule {
        name: "the pair with the higher minimum",
        decide: |a, b, c, d| order(a.min(d), b.min(c)),
    },
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_fold_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }
    let mut vfs = Vfs::new();
    for root in &roots {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    println!("{} entries visible", vfs.len());

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

impl Totals {
    fn report(&self, seconds: f64) {
        println!(
            "\n{} files, {} parsed, in {seconds:.1}s",
            self.files, self.parsed
        );
        for failure in &self.failures {
            println!("  FAIL {failure}");
        }
        println!("bit-plane counts per file: {:?}", self.plane_counts);
        println!(
            "cells: {} total, {} indifferent (the two folds agree exactly), {} kept (>= {DECISIVE_RAW} raw apart)",
            self.cells, self.indifferent, self.decisive
        );
        println!("\nplane set rates over {} cells:", self.cells);
        for ((plane, msb), count) in &self.plane_set {
            let order = if *msb { "MSB" } else { "LSB" };
            let share = Score::percent(*count, self.cells);
            println!("  plane {plane} {order}-first: {count} set ({share:.2}%)");
        }

        println!("\n-- every kept cell in the corpus --");
        print_scores(&self.scores);
        println!("\n-- kept cliff cells where the byte and the rule disagree --");
        for (name, score) in &self.head_to_head {
            println!(
                "  {name:<8} mean error {:>10.3}   the reference prefers it {:>6.2}%   cells {}",
                score.mean(),
                Score::percent(score.agree, score.cells),
                score.cells
            );
        }
        println!("\n-- kept cells carrying a cliff record --");
        print_scores(&self.cliff_scores);

        println!("\n-- the flip byte against a rule computed from the heights alone --");
        println!("  every cliff cell in the corpus; flip = 0 read as \"fold along A-D\"");
        let cliff_cells: u64 = self.flip_all.values().copied().sum();
        println!(
            "  {:<40} {:>12} {:>12} {:>10} {:>9}",
            "rule", "agrees", "disagrees", "ties", "of decided"
        );
        for (name, counts) in &self.flip_rules {
            let (agree, disagree, tie) = (counts[0], counts[1], counts[2]);
            let decided = agree.saturating_add(disagree);
            println!(
                "  {:<40} {agree:>12} {disagree:>12} {tie:>10} {:>8.2}%",
                name,
                Score::percent(agree, decided)
            );
        }
        println!(
            "  cliff cells: {cliff_cells}, flip histogram {:?}",
            self.flip_all
        );
        println!(
            "  where the leading rule disagrees, its own margin: {:?}",
            self.flip_residual
        );
        println!(
            "  where it ties, (flip, the two folds still differ): {:?}",
            self.flip_on_tie
        );
        let mut worst: Vec<(&String, &u64)> = self.flip_disagree_by_file.iter().collect();
        worst.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        println!("  disagreements by file, worst five:");
        for (file, count) in worst.into_iter().take(5) {
            println!("    {count:>6}  {file}");
        }

        println!(
            "flip vs the reference's preference (cliff cells, kept): {:?}",
            self.flip_vs_reference
        );
        println!(
            "flip vs the orientation rule (cliff cells, kept):      {:?}",
            self.flip_vs_orientation
        );
        println!(
            "reference vs orientation, all kept cells:              {:?}",
            self.reference_vs_orientation
        );
    }
}

fn print_scores(scores: &BTreeMap<Candidate, Score>) {
    let mut rows: Vec<(&Candidate, &Score)> = scores.iter().collect();
    rows.sort_by(|a, b| a.1.mean().total_cmp(&b.1.mean()));
    println!(
        "  {:<34} {:>12} {:>10} {:>11} {:>12}",
        "candidate", "mean error", "agree %", "said A-D %", "cells"
    );
    for (candidate, score) in rows {
        println!(
            "  {:<34} {:>12.3} {:>9.1}% {:>10.1}% {:>12}",
            candidate.label(),
            score.mean(),
            Score::percent(score.agree, score.cells),
            Score::percent(score.said_ad, score.cells),
            score.cells
        );
    }
}

fn walk_install(root: &Path, totals: &mut Totals) {
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
                    check_map(&map.height_map, &map.blend_tile, &name, totals);
                }
                Err(error) => totals.failures.push(format!("{label}: {error}")),
            }
        }
    }
}

fn plane_bit(blend_tile: &BlendTileData, plane: usize, x: i64, y: i64, msb: bool) -> Option<bool> {
    let bytes = blend_tile.bit_planes.get(plane)?;
    let stride = i64::from(blend_tile.width.checked_add(7)? / 8);
    let byte = bytes.get(usize::try_from(y.checked_mul(stride)?.checked_add(x / 8)?).ok()?)?;
    let within = u32::try_from(x % 8).ok()?;
    let shift = if msb {
        7u32.checked_sub(within)?
    } else {
        within
    };
    Some(byte.checked_shr(shift)? & 1 == 1)
}

const CENTRE_WEIGHT: [f64; 4] = [-1.0 / 16.0, 9.0 / 16.0, 9.0 / 16.0, -1.0 / 16.0];

fn centre_reference(height_map: &HeightMapData, x: i64, y: i64) -> f64 {
    let max_x = i64::from(height_map.width.saturating_sub(1));
    let max_y = i64::from(height_map.height.saturating_sub(1));
    let mut sum = 0.0f64;
    for (j, wy) in CENTRE_WEIGHT.iter().enumerate() {
        let sy = y
            .saturating_add(i64::try_from(j).unwrap_or(0))
            .saturating_sub(1)
            .clamp(0, max_y);
        for (i, wx) in CENTRE_WEIGHT.iter().enumerate() {
            let sx = x
                .saturating_add(i64::try_from(i).unwrap_or(0))
                .saturating_sub(1)
                .clamp(0, max_x);
            let sample = u32::try_from(sx)
                .ok()
                .zip(u32::try_from(sy).ok())
                .and_then(|(sx, sy)| height_map.sample(sx, sy))
                .unwrap_or(0);
            sum += wx * wy * f64::from(sample);
        }
    }
    sum
}

fn check_map(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    file: &str,
    totals: &mut Totals,
) {
    *totals
        .plane_counts
        .entry(blend_tile.bit_planes.len())
        .or_default() += 1;
    let last_x = i64::from(height_map.width).saturating_sub(1);
    let last_y = i64::from(height_map.height).saturating_sub(1);
    let mut y = 0i64;
    while y < last_y {
        let mut x = 0i64;
        while x < last_x {
            check_cell(height_map, blend_tile, file, x, y, totals);
            x = x.saturating_add(1);
        }
        y = y.saturating_add(1);
    }
}

fn check_cell(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    file: &str,
    x: i64,
    y: i64,
    totals: &mut Totals,
) {
    totals.cells = totals.cells.saturating_add(1);
    for plane in 0..blend_tile.bit_planes.len() {
        for msb in [false, true] {
            if plane_bit(blend_tile, plane, x, y, msb) == Some(true) {
                *totals.plane_set.entry((plane, msb)).or_default() += 1;
            }
        }
    }

    let index = cell_index(blend_tile, x, y);
    let cliff_flip = index
        .and_then(|index| blend_tile.cliff_info_index.get(index).copied())
        .filter(|value| *value != 0)
        .and_then(|value| usize::try_from(value).ok()?.checked_sub(1))
        .and_then(|slot| blend_tile.cliff_records.get(slot))
        .map(|record| record.flip);
    let sample = |dx: i64, dy: i64| -> i64 {
        u32::try_from(x.saturating_add(dx))
            .ok()
            .zip(u32::try_from(y.saturating_add(dy)).ok())
            .and_then(|(sx, sy)| height_map.sample(sx, sy))
            .map_or(0i64, i64::from)
    };
    let (ha, hb, hc, hd) = (sample(0, 0), sample(1, 0), sample(0, 1), sample(1, 1));

    if let Some(flip) = cliff_flip {
        *totals.flip_all.entry(flip).or_default() += 1;
        for rule in FLIP_RULES {
            let says_ad = (rule.decide)(ha, hb, hc, hd);
            let slot = match says_ad {
                None => 2,
                Some(ad) => usize::from(ad != (flip == 0)),
            };
            if let Some(cell) = totals
                .flip_rules
                .entry(rule.name)
                .or_insert([0; 3])
                .get_mut(slot)
            {
                *cell = cell.saturating_add(1);
            }
            if slot == 1 && rule.name == "the diagonal whose ends differ least" {
                let margin = (ha - hd).abs().abs_diff((hb - hc).abs());
                let bucket = match margin {
                    0 => "0 (a tie the rule broke arbitrarily)",
                    1..=9 => "1..9",
                    10..=99 => "10..99",
                    100..=999 => "100..999",
                    _ => "1000+",
                };
                *totals.flip_residual.entry(bucket).or_default() += 1;
                *totals
                    .flip_disagree_by_file
                    .entry(file.to_owned())
                    .or_default() += 1;
            }
            if slot == 2 && rule.name == "the diagonal whose ends differ least" {
                let folds_differ = ha + hd != hb + hc;
                *totals.flip_on_tie.entry((flip, folds_differ)).or_default() += 1;
            }
        }
    }
    let gap = (ha.saturating_add(hd)).saturating_sub(hb.saturating_add(hc));
    if gap == 0 {
        totals.indifferent = totals.indifferent.saturating_add(1);
        return;
    }
    if gap.abs() < DECISIVE_RAW {
        return;
    }
    totals.decisive = totals.decisive.saturating_add(1);

    let centre_ad = ((ha.saturating_add(hd)) as f64) / 2.0;
    let centre_bc = ((hb.saturating_add(hc)) as f64) / 2.0;
    let reference = centre_reference(height_map, x, y);
    let error_ad = (centre_ad - reference).abs();
    let error_bc = (centre_bc - reference).abs();
    let reference_says_ad = error_ad < error_bc;

    let gx = (hb.saturating_add(hd)).saturating_sub(ha.saturating_add(hc));
    let gy = (hc.saturating_add(hd)).saturating_sub(ha.saturating_add(hb));
    let orientation_says_ad = gx.saturating_mul(gy) < 0;
    *totals
        .reference_vs_orientation
        .entry((reference_says_ad, orientation_says_ad))
        .or_default() += 1;

    if let Some(flip) = cliff_flip {
        let byte_says_ad = flip == 0;
        let rule_says_ad = (ha - hd).abs() < (hb - hc).abs();
        if byte_says_ad != rule_says_ad {
            for (name, says_ad) in [("byte", byte_says_ad), ("rule", rule_says_ad)] {
                let error = if says_ad { error_ad } else { error_bc };
                totals.head_to_head.entry(name).or_default().add(
                    error,
                    says_ad == reference_says_ad,
                    says_ad,
                );
            }
        }
        *totals
            .flip_vs_reference
            .entry((flip, reference_says_ad))
            .or_default() += 1;
        *totals
            .flip_vs_orientation
            .entry((flip, orientation_says_ad))
            .or_default() += 1;
    }
    let cell_byte = index
        .and_then(|index| blend_tile.unknown_cell_byte.as_ref()?.get(index).copied())
        .is_some_and(|byte| byte != 0);

    let mut candidates: Vec<(Candidate, bool)> = vec![
        (Candidate::AlwaysBc, false),
        (Candidate::AlwaysAd, true),
        (Candidate::Oracle, reference_says_ad),
        (Candidate::Parity, (x.saturating_add(y)) % 2 != 0),
        (Candidate::CellByte, cell_byte),
        (Candidate::CliffFlip, cliff_flip == Some(1)),
        (Candidate::Orientation, orientation_says_ad),
    ];
    for plane in 0..blend_tile.bit_planes.len() {
        for msb in [false, true] {
            let set = plane_bit(blend_tile, plane, x, y, msb) == Some(true);
            let name = if msb {
                Candidate::PlaneMsb(plane)
            } else {
                Candidate::PlaneLsb(plane)
            };
            candidates.push((name, set));
        }
    }
    for (candidate, says_ad) in candidates {
        let error = if says_ad { error_ad } else { error_bc };
        totals.scores.entry(candidate).or_default().add(
            error,
            says_ad == reference_says_ad,
            says_ad,
        );
        if cliff_flip.is_some() {
            totals.cliff_scores.entry(candidate).or_default().add(
                error,
                says_ad == reference_says_ad,
                says_ad,
            );
        }
    }
}

fn cell_index(blend_tile: &BlendTileData, x: i64, y: i64) -> Option<usize> {
    if x < 0 || y < 0 || x >= i64::from(blend_tile.width) || y >= i64::from(blend_tile.height) {
        return None;
    }
    usize::try_from(y.checked_mul(i64::from(blend_tile.width))?.checked_add(x)?).ok()
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
