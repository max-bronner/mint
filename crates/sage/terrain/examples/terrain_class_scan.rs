use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::{BlendTileData, HeightMapData, MapFile};
use mint_sage_terrain::{load_class_textures, TerrainTypes};
use mint_vfs::Vfs;

const TERRAIN_INI: &str = "data/ini/terrain.ini";
const TILE_PIXEL_EXTENT: u32 = 64;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: terrain_class_scan <install dir> [<install dir> ...]");
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

    let Ok(ini) = vfs.read(TERRAIN_INI) else {
        eprintln!("{TERRAIN_INI}: not present");
        return ExitCode::FAILURE;
    };
    let types = TerrainTypes::parse(&String::from_utf8_lossy(&ini));
    println!(
        "{TERRAIN_INI}: {} types, {} redefinition(s)\n",
        types.len(),
        types.redefinitions()
    );

    let mut t = Totals::default();
    let started = Instant::now();
    for root in &roots {
        walk(root, &vfs, &types, &mut t);
    }
    t.report(started.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

#[derive(Default)]
struct Totals {
    files: u64,
    parsed: u64,
    classes: u64,
    edge_classes: u64,

    dup_name_files: Vec<String>,
    dup_name_pairs: u64,
    dup_name_same_shape: u64,
    dup_name_diff_width: u64,
    dup_name_diff_numtiles: u64,

    overlap_files: Vec<String>,
    overlap_pairs: u64,

    gap_files: Vec<String>,
    unsorted_files: Vec<String>,
    sum_mismatch_files: Vec<String>,
    nonsquare: u64,

    unresolved: u64,
    extent_exact: u64,
    extent_other: BTreeMap<(u32, u32, i32, String), (u64, u64)>,
    width_disagrees: u64,
    width_disagrees_files: Vec<String>,
    width_disagrees_cells: u64,

    same_file_pairs: u64,
    same_file_diff_name_pairs: u64,

    cells: u64,
    cells_multi_range: u64,
    cells_no_range: u64,

    cliff_records: u64,
    cliff_local_pred_hit: u64,
    cliff_sub_pred_hit: u64,
    cliff_ctrl_zero: u64,
    cliff_ctrl_last: u64,
    cliff_ctrl_sub_zero: u64,
    cliff_local_hist: BTreeMap<i64, u64>,

    degenerate_first: u64,
    degenerate_count: u64,
    degenerate_width: u64,

    fornost: Option<String>,
}

fn walk(root: &Path, vfs: &Vfs, types: &TerrainTypes, t: &mut Totals) {
    for archive_path in big_archives(root) {
        let Ok(archive) = BigArchive::open(&archive_path) else {
            continue;
        };
        let names: Vec<String> = archive
            .entries()
            .iter()
            .map(|e| e.name.clone())
            .filter(|n| {
                let lower = n.to_ascii_lowercase();
                lower.ends_with(".map") || lower.ends_with(".bse")
            })
            .collect();
        for name in names {
            t.files = t.files.saturating_add(1);
            let Some(entry) = archive.entries().iter().find(|e| e.name == name).cloned() else {
                continue;
            };
            let Ok(raw) = archive.read_entry(&entry) else {
                continue;
            };
            if let Ok(map) = MapFile::parse(&raw) {
                t.parsed = t.parsed.saturating_add(1);
                check(&name, &map.height_map, &map.blend_tile, vfs, types, t);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn check(
    name: &str,
    hm: &HeightMapData,
    bt: &BlendTileData,
    vfs: &Vfs,
    types: &TerrainTypes,
    t: &mut Totals,
) {
    let classes = &bt.texture_classes;
    t.classes = t.classes.saturating_add(classes.len() as u64);
    t.edge_classes = t
        .edge_classes
        .saturating_add(bt.edge_texture_classes.len() as u64);

    let mut dup_here = 0u64;
    for (i, a) in classes.iter().enumerate() {
        for b in classes.iter().skip(i.saturating_add(1)) {
            if a.name.eq_ignore_ascii_case(&b.name) {
                dup_here += 1;
                t.dup_name_pairs = t.dup_name_pairs.saturating_add(1);
                if a.width == b.width && a.num_tiles == b.num_tiles {
                    t.dup_name_same_shape = t.dup_name_same_shape.saturating_add(1);
                }
                if a.width != b.width {
                    t.dup_name_diff_width = t.dup_name_diff_width.saturating_add(1);
                }
                if a.num_tiles != b.num_tiles {
                    t.dup_name_diff_numtiles = t.dup_name_diff_numtiles.saturating_add(1);
                }
            }
        }
    }
    if dup_here > 0 && t.dup_name_files.len() < 40 {
        t.dup_name_files
            .push(format!("{name} ({dup_here} pair(s))"));
    }

    let mut ranges: Vec<(i64, i64, usize)> = Vec::new();
    for (idx, c) in classes.iter().enumerate() {
        if c.first_tile < 0 {
            t.degenerate_first = t.degenerate_first.saturating_add(1);
        }
        if c.width <= 0 {
            t.degenerate_width = t.degenerate_width.saturating_add(1);
        }
        if c.num_tiles <= 0 {
            t.degenerate_count = t.degenerate_count.saturating_add(1);
            continue;
        }
        let first = i64::from(c.first_tile);
        ranges.push((first, first + i64::from(c.num_tiles), idx));
        if c.num_tiles != c.width.saturating_mul(c.width) {
            t.nonsquare = t.nonsquare.saturating_add(1);
        }
    }
    let mut overlaps = 0u64;
    for (i, a) in ranges.iter().enumerate() {
        for b in ranges.iter().skip(i.saturating_add(1)) {
            if a.0 < b.1 && b.0 < a.1 {
                overlaps += 1;
                t.overlap_pairs = t.overlap_pairs.saturating_add(1);
            }
        }
    }
    if overlaps > 0 && t.overlap_files.len() < 40 {
        t.overlap_files.push(format!("{name} ({overlaps} pair(s))"));
    }

    let declared_order_sorted = ranges
        .windows(2)
        .all(|w| w.first().zip(w.get(1)).is_none_or(|(a, b)| a.0 <= b.0));
    if !declared_order_sorted && t.unsorted_files.len() < 40 {
        t.unsorted_files.push(name.to_owned());
    }

    let mut sorted = ranges.clone();
    sorted.sort_by_key(|r| r.0);
    let mut covered_ok = sorted.first().is_none_or(|r| r.0 == 0);
    for w in sorted.windows(2) {
        if let (Some(a), Some(b)) = (w.first(), w.get(1)) {
            if a.1 != b.0 {
                covered_ok = false;
            }
        }
    }
    if let Some(last) = sorted.last() {
        if last.1 != i64::from(bt.num_bitmap_tiles) {
            covered_ok = false;
        }
    }
    if !covered_ok && t.gap_files.len() < 40 {
        t.gap_files.push(name.to_owned());
    }

    let sum: i64 = classes.iter().map(|c| i64::from(c.num_tiles)).sum();
    if sum != i64::from(bt.num_bitmap_tiles) && t.sum_mismatch_files.len() < 40 {
        t.sum_mismatch_files.push(format!(
            "{name}: sum {sum} vs numBitmapTiles {}",
            bt.num_bitmap_tiles
        ));
    }

    for value in &bt.tile_index {
        let tile = i64::from(*value >> 2);
        let hits = ranges.iter().filter(|r| tile >= r.0 && tile < r.1).count();
        t.cells = t.cells.saturating_add(1);
        if hits == 0 {
            t.cells_no_range = t.cells_no_range.saturating_add(1);
        } else if hits > 1 {
            t.cells_multi_range = t.cells_multi_range.saturating_add(1);
        }
    }

    let loaded = load_class_textures(vfs, types, classes);
    let files: Vec<Option<String>> = classes
        .iter()
        .map(|c| types.texture(&c.name).map(str::to_owned))
        .collect();
    for (i, left) in files.iter().enumerate() {
        for (j, right) in files.iter().enumerate().skip(i.saturating_add(1)) {
            let (Some(a), Some(b)) = (left.as_ref(), right.as_ref()) else {
                continue;
            };
            if !a.eq_ignore_ascii_case(b) {
                continue;
            }
            t.same_file_pairs = t.same_file_pairs.saturating_add(1);
            let same_name = classes
                .get(i)
                .zip(classes.get(j))
                .is_some_and(|(x, y)| x.name.eq_ignore_ascii_case(&y.name));
            if !same_name {
                t.same_file_diff_name_pairs = t.same_file_diff_name_pairs.saturating_add(1);
            }
        }
    }
    for (i, class) in classes.iter().enumerate() {
        let Some(Some(tex)) = loaded.textures.get(i) else {
            t.unresolved = t.unresolved.saturating_add(1);
            continue;
        };
        let Some(level) = tex.levels.first() else {
            continue;
        };
        let want = u32::try_from(class.width.max(0))
            .unwrap_or(0)
            .saturating_mul(TILE_PIXEL_EXTENT);
        if level.width == want && level.height == want {
            t.extent_exact = t.extent_exact.saturating_add(1);
        } else {
            let first = i64::from(class.first_tile);
            let end = first.saturating_add(i64::from(class.num_tiles));
            let used = bt
                .tile_index
                .iter()
                .filter(|v| {
                    let tile = i64::from(**v >> 2);
                    tile >= first && tile < end
                })
                .count() as u64;
            let bucket = t
                .extent_other
                .entry((level.width, level.height, class.width, class.name.clone()))
                .or_default();
            bucket.0 = bucket.0.saturating_add(1);
            bucket.1 = bucket.1.saturating_add(used);
            let tiles = (level.width / TILE_PIXEL_EXTENT) * (level.height / TILE_PIXEL_EXTENT);
            let mut implied = 0i32;
            for w in (1..=10i32).rev() {
                if i64::from(tiles) >= i64::from(w) * i64::from(w) {
                    implied = w;
                    break;
                }
            }
            if implied != class.width {
                t.width_disagrees = t.width_disagrees.saturating_add(1);
                t.width_disagrees_cells = t.width_disagrees_cells.saturating_add(used);
                if t.width_disagrees_files.len() < 40 {
                    t.width_disagrees_files.push(format!(
                        "{name}: class '{}' declares width {} numTiles {}, image {}x{} implies width {implied}",
                        class.name, class.width, class.num_tiles, level.width, level.height
                    ));
                    if let Some(last) = t.width_disagrees_files.last_mut() {
                        last.push_str(&format!(", {used} cell(s) use it"));
                    }
                }
            }
        }
    }

    let (mw, mh) = (i64::from(hm.width), i64::from(hm.height));
    for rec in &bt.cliff_records {
        let tile = i64::from(rec.tile_index >> 2);
        let sub = rec.tile_index & 3;
        let Some(r) = ranges.iter().find(|r| tile >= r.0 && tile < r.1) else {
            continue;
        };
        let Some(class) = classes.get(r.2) else {
            continue;
        };
        let width = i64::from(class.width).max(1);
        let local = tile - r.0;
        t.cliff_records = t.cliff_records.saturating_add(1);
        *t.cliff_local_hist.entry(local).or_default() += 1;
        let pred_local = ((mw / 2) % width) + width * ((mh / 2) % width);
        if local == pred_local {
            t.cliff_local_pred_hit = t.cliff_local_pred_hit.saturating_add(1);
        }
        let pred_sub = 2 * (mh & 1) + (mw & 1);
        if i64::from(sub) == pred_sub {
            t.cliff_sub_pred_hit = t.cliff_sub_pred_hit.saturating_add(1);
        }
        if local == 0 {
            t.cliff_ctrl_zero = t.cliff_ctrl_zero.saturating_add(1);
        }
        if sub == 0 {
            t.cliff_ctrl_sub_zero = t.cliff_ctrl_sub_zero.saturating_add(1);
        }
        let last = (((mw - 1) / 2) % width) + width * (((mh - 1) / 2) % width);
        if local == last {
            t.cliff_ctrl_last = t.cliff_ctrl_last.saturating_add(1);
        }
    }

    if name.to_ascii_lowercase().contains("evil fornost") && t.fornost.is_none() {
        dump_class_map(hm, bt, &ranges, classes);
        let mut out = format!(
            "\n=== {name} ===\n  {}x{} border {}, numBitmapTiles {}, {} class(es), {} edge class(es)\n",
            hm.width,
            hm.height,
            hm.border_size,
            bt.num_bitmap_tiles,
            classes.len(),
            bt.edge_texture_classes.len()
        );
        for (i, c) in classes.iter().enumerate() {
            let file = types.texture(&c.name).unwrap_or("<unresolved>");
            let dims = loaded
                .textures
                .get(i)
                .and_then(Option::as_ref)
                .and_then(|tx| tx.levels.first())
                .map_or_else(|| "-".to_owned(), |l| format!("{}x{}", l.width, l.height));
            out.push_str(&format!(
                "  [{i:2}] first {:4} n {:3} w {} {:24} -> {:22} {dims}\n",
                c.first_tile, c.num_tiles, c.width, c.name, file
            ));
        }
        for (i, c) in bt.edge_texture_classes.iter().enumerate() {
            out.push_str(&format!(
                "  edge [{i:2}] first {:4} n {:3} w {} {}\n",
                c.first_tile, c.num_tiles, c.width, c.name
            ));
        }

        let mut pairs: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut cliffy: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        let class_of = |tile: i64| -> Option<usize> {
            ranges
                .iter()
                .find(|r| tile >= r.0 && tile < r.1)
                .map(|r| r.2)
        };
        for (i, value) in bt.tile_index.iter().enumerate() {
            let own = class_of(i64::from(*value >> 2));
            let own_name = own
                .and_then(|c| classes.get(c))
                .map_or("<none>", |c| c.name.as_str())
                .to_owned();
            let index = bt.cliff_info_index.get(i).copied().unwrap_or(0);
            let entry = cliffy.entry(own_name.clone()).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
            if index == 0 {
                continue;
            }
            entry.1 = entry.1.saturating_add(1);
            let Some(rec) = bt.cliff_records.get((index as usize).saturating_sub(1)) else {
                continue;
            };
            let rec_name = class_of(i64::from(rec.tile_index >> 2))
                .and_then(|c| classes.get(c))
                .map_or("<none>", |c| c.name.as_str())
                .to_owned();
            *pairs.entry((own_name, rec_name)).or_default() += 1;
        }
        out.push_str("\n  cells per class, and how many carry a cliff record:\n");
        let mut rows: Vec<(&String, &(u64, u64))> = cliffy.iter().collect();
        rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.1));
        for (name, (all, cliff)) in rows.iter().take(12) {
            out.push_str(&format!(
                "    {name:24} cells {all:7}  with a cliff record {cliff:6}\n"
            ));
        }
        out.push_str("\n  (cell's own class -> the class its cliff record names):\n");
        let mut prows: Vec<((String, String), u64)> = pairs.into_iter().collect();
        prows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for ((a, b), n) in prows.iter().take(14) {
            let mark = if a == b { "  " } else { "<-" };
            out.push_str(&format!("    {a:24} -> {b:24} {n:6} {mark}\n"));
        }
        t.fornost = Some(out);
    }
}

impl Totals {
    fn report(&self, secs: f64) {
        println!("files {} parsed {} in {secs:.1}s", self.files, self.parsed);
        println!(
            "texture classes {} (edge {})",
            self.classes, self.edge_classes
        );
        println!("\n-- duplicate class names within one map --");
        println!(
            "  pairs {} | same width and numTiles {} | differing width {} | differing numTiles {}",
            self.dup_name_pairs,
            self.dup_name_same_shape,
            self.dup_name_diff_width,
            self.dup_name_diff_numtiles
        );
        for f in &self.dup_name_files {
            println!("    {f}");
        }
        println!("\n-- tile ranges --");
        println!("  overlapping pairs {}", self.overlap_pairs);
        for f in &self.overlap_files {
            println!("    {f}");
        }
        println!(
            "  files whose ranges do not tile [0, numBitmapTiles) exactly: {}",
            self.gap_files.len()
        );
        for f in &self.gap_files {
            println!("    {f}");
        }
        println!(
            "  files whose declaration order is not ascending by firstTile: {}",
            self.unsorted_files.len()
        );
        for f in &self.unsorted_files {
            println!("    {f}");
        }
        println!(
            "  files whose numTiles do not sum to numBitmapTiles: {}",
            self.sum_mismatch_files.len()
        );
        for f in &self.sum_mismatch_files {
            println!("    {f}");
        }
        println!(
            "  class records with numTiles != width*width: {}",
            self.nonsquare
        );
        println!("\n-- cells against the ranges --");
        println!(
            "  cells {} | in no range {} | in more than one range {}",
            self.cells, self.cells_no_range, self.cells_multi_range
        );
        println!("\n-- images --");
        println!(
            "  unresolved class refs {} | image exactly width*64 square {}",
            self.unresolved, self.extent_exact
        );
        for ((w, h, dw, name), (n, cells)) in &self.extent_other {
            println!(
                "    {w}x{h} against declared width {dw} ({name}): {n} record(s), {cells} cell(s)"
            );
        }
        println!(
            "  class records whose declared width disagrees with the image's own: {} ({} cells use them)",
            self.width_disagrees, self.width_disagrees_cells
        );
        for f in &self.width_disagrees_files {
            println!("    {f}");
        }
        println!(
            "  pairs of classes in one map resolving to the same image: {} (of which differently named: {})",
            self.same_file_pairs, self.same_file_diff_name_pairs
        );
        println!(
            "\n-- degenerate class records: firstTile<0 {} | numTiles<=0 {} | width<=0 {}",
            self.degenerate_first, self.degenerate_count, self.degenerate_width
        );
        println!("\n-- cliff record tile index, predicted from optimizeTiles' post-loop x/y --");
        println!(
            "  records {} | class-local index predicted {} | sub-tile predicted {}",
            self.cliff_records, self.cliff_local_pred_hit, self.cliff_sub_pred_hit
        );
        println!(
            "  controls: class-local == 0 {} | predicted from (w-1, h-1) {} | sub-tile == 0 {}",
            self.cliff_ctrl_zero, self.cliff_ctrl_last, self.cliff_ctrl_sub_zero
        );
        let mut hist: Vec<(i64, u64)> = self
            .cliff_local_hist
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect();
        hist.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (k, n) in hist.iter().take(6) {
            println!("    class-local {k}: {n}");
        }
        if let Some(f) = &self.fornost {
            println!("{f}");
        }
    }
}

fn dump_class_map(
    hm: &HeightMapData,
    bt: &BlendTileData,
    ranges: &[(i64, i64, usize)],
    classes: &[mint_sage_map::TextureClass],
) {
    let Ok(dir) = std::env::var("MINT_SCRATCH") else {
        return;
    };
    let (w, h) = (hm.width as usize, hm.height as usize);
    let mut counts = vec![0u64; classes.len()];
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h {
        for x in 0..w {
            let value = bt
                .tile_index
                .get(y.saturating_mul(w).saturating_add(x))
                .copied()
                .unwrap_or(0);
            let tile = i64::from(value >> 2);
            let class = ranges
                .iter()
                .find(|r| tile >= r.0 && tile < r.1)
                .map(|r| r.2);
            let rgb = match class {
                Some(c) => {
                    if let Some(n) = counts.get_mut(c) {
                        *n = n.saturating_add(1);
                    }
                    match classes.get(c).map(|k| k.name.as_str()) {
                        Some("CliffMinasMorgul03") => [0, 255, 0],
                        Some("StoneCaradhras03") => [255, 0, 0],
                        Some("StoneFornost06") => [0, 0, 255],
                        _ => {
                            let shade = u8::try_from(c % 256).unwrap_or(0);
                            let g = 40u8.saturating_add(shade.wrapping_mul(7) % 160);
                            [g, g, g]
                        }
                    }
                }
                None => [255, 0, 255],
            };
            ppm.extend_from_slice(&rgb);
        }
    }
    let _ = std::fs::write(Path::new(&dir).join("fornost_classes.ppm"), &ppm);

    let mut mask = format!("P6\n{w} {h}\n255\n").into_bytes();
    for y in 0..h {
        for x in 0..w {
            let i = y.saturating_mul(w).saturating_add(x);
            let value = bt.tile_index.get(i).copied().unwrap_or(0);
            let tile = i64::from(value >> 2);
            let class = ranges
                .iter()
                .find(|r| tile >= r.0 && tile < r.1)
                .map(|r| r.2);
            let name = class
                .and_then(|c| classes.get(c))
                .map(|k| k.name.as_str())
                .unwrap_or("");
            let cliff = bt.cliff_info_index.get(i).copied().unwrap_or(0) != 0;
            mask.extend_from_slice(&[
                if cliff { 255 } else { 0 },
                if name == "CliffMinasMorgul03" { 255 } else { 0 },
                if name == "StoneCaradhras03" { 255 } else { 0 },
            ]);
        }
    }
    let _ = std::fs::write(Path::new(&dir).join("fornost_mask.ppm"), &mask);
    let mut lines = String::new();
    for (i, c) in classes.iter().enumerate() {
        lines.push_str(&format!(
            "{i:2} {:24} {}\n",
            c.name,
            counts.get(i).copied().unwrap_or(0)
        ));
    }
    let _ = std::fs::write(Path::new(&dir).join("fornost_counts.txt"), lines);
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
