use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;

const MAX_DEPTH: u32 = 16;

const BLEND_RECORD_MARKER: u32 = 0x7ADA_0000;

const BLEND_RECORD_LEN: usize = 18;
const CLIFF_RECORD_LEN: usize = 38;

const BLEND_LAYOUT: &[(u16, usize, usize)] = &[
    (8, 8, 1),
    (9, 8, 1),
    (11, 8, 3),
    (14, 14, 4),
    (15, 14, 5),
    (16, 15, 5),
    (17, 15, 6),
    (18, 15, 6),
];

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: map_scan <install dir> [<install dir> ...]");
        return ExitCode::FAILURE;
    }

    let mut all = Totals::default();
    for root in &roots {
        let started = Instant::now();
        let mut totals = Totals::default();
        walk_install(root, &mut totals);
        println!("== {} ==", root.display());
        totals.report(started.elapsed().as_secs_f64());
        all.merge(totals);
        println!();
    }

    if roots.len() > 1 {
        println!("== all installations ==");
        all.report(0.0);
    }

    if all.cross_checks_held() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().ok()?;
        Some(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }

    fn ascii(&mut self) -> Option<String> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len)?;
        Some(bytes.iter().map(|&b| char::from(b)).collect())
    }

    fn skip_unicode(&mut self) -> Option<()> {
        let len = usize::from(self.u16()?);
        self.take(len.checked_mul(2)?)?;
        Some(())
    }

    fn dict(&mut self) -> Option<Vec<u32>> {
        let count = usize::from(self.u16()?);
        let mut keys = Vec::new();
        for _ in 0..count {
            let packed = self.u32()?;
            let kind = packed & 0xff;
            keys.push(packed >> 8);
            match kind {
                0 => {
                    self.u8()?;
                }
                1 | 2 => {
                    self.u32()?;
                }
                3 => {
                    self.ascii()?;
                }
                4 => {
                    self.skip_unicode()?;
                }
                _ => return None,
            }
        }
        Some(keys)
    }
}

struct NameTable {
    names: BTreeMap<u32, String>,
    end: usize,
    indices_are_dense: bool,
}

impl NameTable {
    fn name_of(&self, index: u32) -> String {
        self.names
            .get(&index)
            .cloned()
            .unwrap_or_else(|| format!("<index {index}>"))
    }
}

fn read_name_table(body: &[u8]) -> Option<NameTable> {
    let mut cur = Cursor::new(body, 0);
    if cur.take(4)? != b"CkMp" {
        return None;
    }
    let count = cur.u32()?;
    let mut names = BTreeMap::new();
    for _ in 0..count {
        let len = usize::from(cur.u8()?);
        let bytes = cur.take(len)?;
        let name: String = bytes.iter().map(|&b| char::from(b)).collect();
        names.insert(cur.u32()?, name);
    }
    let dense = names.len() as u64 == u64::from(count) && names.keys().copied().eq(1..=count);
    Some(NameTable {
        names,
        end: cur.pos,
        indices_are_dense: dense,
    })
}

struct ChunkHeader {
    name: String,
    version: u16,
    body: usize,
    len: usize,
}

impl ChunkHeader {
    fn end(&self) -> Option<usize> {
        self.body.checked_add(self.len)
    }
}

fn chunks(data: &[u8], start: usize, end: usize, table: &NameTable) -> Option<Vec<ChunkHeader>> {
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        let mut cur = Cursor::new(data, at);
        let index = cur.u32()?;
        let version = cur.u16()?;
        let len = cur.u32()? as usize;
        let name = table.names.get(&index)?.clone();
        let header = ChunkHeader {
            name,
            version,
            body: cur.pos,
            len,
        };
        let next = header.end()?;
        if next > end {
            return None;
        }
        at = next;
        out.push(header);
    }
    Some(out)
}

#[derive(Default)]
struct ChunkStat {
    count: u64,
    bytes: u64,
    files: u64,
    seen_in_this_file: bool,
    versions: BTreeMap<u16, u64>,
    depths: BTreeMap<u32, u64>,
}

#[derive(Default)]
struct Totals {
    files: u64,
    bodies_bytes: u64,
    walked_to_end: u64,
    dense_indices: u64,
    catalogue: BTreeMap<String, ChunkStat>,

    heightmaps: u64,
    heightmap_count_matches_area: u64,
    heightmap_payload_two_bytes: u64,
    heightmap_dims: BTreeMap<(u32, u32), u64>,
    bordered_maps: u64,
    flat_maps: u64,
    border_ring_constant: u64,
    border_ring_constant_not_flat: u64,
    border_ring_values: BTreeMap<u16, u64>,
    border_ring_values_not_flat: BTreeMap<u16, u64>,
    outer_ring_constant: u64,
    outer_ring_values: BTreeMap<u16, u64>,
    height_min: u16,
    height_max: u16,

    blendtiles: u64,
    blend_len_matches_area: u64,
    blend_tail_exact: u64,
    blend_tail_by_search: Vec<String>,
    blend_unknown_version: BTreeMap<u16, u64>,
    blend_class_sum_ok: u64,
    blend_marker_ok: u64,
    blend_record_tile_ok: u64,
    blend_cliff_tile_ok: u64,
    blend_cell_tile_ok: u64,
    blend_records_checked: u64,
    blend_cliff_records_checked: u64,
    blend_texture_classes: u64,
    blend_cells_out_of_range: Vec<String>,

    objects: u64,
    objects_exact: u64,
    objects_named: u64,
    object_keys: BTreeMap<String, u64>,

    worldinfos: u64,
    worldinfo_keys: BTreeMap<String, u64>,

    failures: Vec<String>,
}

impl Totals {
    fn note_chunk(&mut self, header: &ChunkHeader, depth: u32) {
        let stat = self.catalogue.entry(header.name.clone()).or_default();
        stat.count = stat.count.saturating_add(1);
        stat.bytes = stat.bytes.saturating_add(header.len as u64);
        *stat.versions.entry(header.version).or_default() += 1;
        *stat.depths.entry(depth).or_default() += 1;
        if !stat.seen_in_this_file {
            stat.seen_in_this_file = true;
            stat.files = stat.files.saturating_add(1);
        }
    }

    fn end_of_file(&mut self) {
        for stat in self.catalogue.values_mut() {
            stat.seen_in_this_file = false;
        }
    }

    fn merge(&mut self, other: Totals) {
        self.files += other.files;
        self.bodies_bytes += other.bodies_bytes;
        self.walked_to_end += other.walked_to_end;
        self.dense_indices += other.dense_indices;
        self.heightmaps += other.heightmaps;
        self.heightmap_count_matches_area += other.heightmap_count_matches_area;
        self.heightmap_payload_two_bytes += other.heightmap_payload_two_bytes;
        self.bordered_maps += other.bordered_maps;
        self.flat_maps += other.flat_maps;
        self.border_ring_constant += other.border_ring_constant;
        self.border_ring_constant_not_flat += other.border_ring_constant_not_flat;
        merge_counts(
            &mut self.border_ring_values_not_flat,
            other.border_ring_values_not_flat,
        );
        self.outer_ring_constant += other.outer_ring_constant;
        self.height_max = self.height_max.max(other.height_max);
        self.height_min = if self.heightmaps == 0 {
            other.height_min
        } else {
            self.height_min.min(other.height_min)
        };
        merge_counts(&mut self.border_ring_values, other.border_ring_values);
        merge_counts(&mut self.outer_ring_values, other.outer_ring_values);
        self.blendtiles += other.blendtiles;
        self.blend_len_matches_area += other.blend_len_matches_area;
        self.blend_tail_exact += other.blend_tail_exact;
        self.blend_tail_by_search.extend(other.blend_tail_by_search);
        self.blend_class_sum_ok += other.blend_class_sum_ok;
        self.blend_marker_ok += other.blend_marker_ok;
        self.blend_record_tile_ok += other.blend_record_tile_ok;
        self.blend_cliff_tile_ok += other.blend_cliff_tile_ok;
        self.blend_cell_tile_ok += other.blend_cell_tile_ok;
        self.blend_records_checked += other.blend_records_checked;
        self.blend_cliff_records_checked += other.blend_cliff_records_checked;
        self.blend_texture_classes += other.blend_texture_classes;
        self.blend_cells_out_of_range
            .extend(other.blend_cells_out_of_range);
        self.objects += other.objects;
        self.objects_exact += other.objects_exact;
        self.objects_named += other.objects_named;
        self.worldinfos += other.worldinfos;
        self.failures.extend(other.failures);
        merge_counts(&mut self.heightmap_dims, other.heightmap_dims);
        merge_counts(&mut self.blend_unknown_version, other.blend_unknown_version);
        merge_counts(&mut self.object_keys, other.object_keys);
        merge_counts(&mut self.worldinfo_keys, other.worldinfo_keys);
        for (name, stat) in other.catalogue {
            let mine = self.catalogue.entry(name).or_default();
            mine.count += stat.count;
            mine.bytes += stat.bytes;
            mine.files += stat.files;
            merge_counts(&mut mine.versions, stat.versions);
            merge_counts(&mut mine.depths, stat.depths);
        }
    }

    fn cross_checks_held(&self) -> bool {
        self.failures.is_empty()
            && self.walked_to_end == self.files
            && self.dense_indices == self.files
            && self.heightmap_count_matches_area == self.heightmaps
            && self.heightmap_payload_two_bytes == self.heightmaps
            && self.blend_len_matches_area == self.blendtiles
            && self.blend_tail_exact == self.blendtiles
            && self.blend_class_sum_ok == self.blendtiles
            && self.blend_marker_ok == self.blendtiles
            && self.blend_record_tile_ok == self.blendtiles
            && self.blend_cliff_tile_ok == self.blendtiles
            && self.objects_exact == self.objects
    }

    fn report(&self, seconds: f64) {
        println!(
            "  {} .map/.bse bodies, {:.1} MiB decompressed",
            self.files,
            mib(self.bodies_bytes)
        );
        if seconds > 0.0 {
            println!(
                "  {seconds:.1}s, {:.1} MiB/s decompressed and walked",
                mib(self.bodies_bytes) / seconds
            );
        }
        println!();
        println!("  chunk catalogue");
        println!(
            "    {:<24} {:>8} {:>14} {:>6} {:>7}  versions",
            "name", "count", "bytes", "files", "depths"
        );
        let mut rows: Vec<_> = self.catalogue.iter().collect();
        rows.sort_by_key(|(name, stat)| (u64::MAX - stat.bytes, (*name).clone()));
        for (name, stat) in rows {
            let versions = stat
                .versions
                .iter()
                .map(|(v, c)| format!("{v}x{c}"))
                .collect::<Vec<_>>()
                .join(",");
            let depths = stat
                .depths
                .keys()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("/");
            println!(
                "    {name:<24} {:>8} {:>14} {:>6} {depths:>7}  {versions}",
                stat.count, stat.bytes, stat.files
            );
        }
        println!();
        println!("  cross-checks");
        line(
            "chunk walk lands exactly on the body's last byte",
            self.walked_to_end,
            self.files,
        );
        line(
            "name-table indices are exactly 1..=count",
            self.dense_indices,
            self.files,
        );
        line(
            "HeightMapData sample count == width * height",
            self.heightmap_count_matches_area,
            self.heightmaps,
        );
        line(
            "HeightMapData payload == 2 bytes per sample",
            self.heightmap_payload_two_bytes,
            self.heightmaps,
        );
        line(
            "BlendTileData length field == width * height",
            self.blend_len_matches_area,
            self.blendtiles,
        );
        line(
            "BlendTileData scalar section ends on the chunk's last byte",
            self.blend_tail_exact,
            self.blendtiles,
        );
        line(
            "texture-class tile counts sum to numBitmapTiles",
            self.blend_class_sum_ok,
            self.blendtiles,
        );
        line(
            "every blend record carries the 0x7ADA0000 marker",
            self.blend_marker_ok,
            self.blendtiles,
        );
        line(
            "blend-record tile index >> 2 in range",
            self.blend_record_tile_ok,
            self.blendtiles,
        );
        line(
            "cliff-record tile index >> 2 in range",
            self.blend_cliff_tile_ok,
            self.blendtiles,
        );
        line(
            "every cell's tile index >> 2 in range",
            self.blend_cell_tile_ok,
            self.blendtiles,
        );
        line(
            "Object fields consume exactly the chunk",
            self.objects_exact,
            self.objects,
        );
        println!(
            "    {} blend records and {} cliff records checked, across {} texture classes",
            self.blend_records_checked,
            self.blend_cliff_records_checked,
            self.blend_texture_classes
        );
        println!(
            "    {} objects, {} with a non-empty name",
            self.objects, self.objects_named
        );
        println!("    {} distinct heightmap sizes", self.heightmap_dims.len());
        println!(
            "    height samples span {}..={} across the corpus",
            self.height_min, self.height_max
        );
        println!(
            "    {} of {} bordered maps are flat everywhere (min == max)",
            self.flat_maps, self.bordered_maps
        );
        println!(
            "    border ring holds a single sample value in {} of {} bordered maps; of those, {} are not flat everywhere",
            self.border_ring_constant, self.bordered_maps, self.border_ring_constant_not_flat
        );
        print_ring("border ring", &self.border_ring_values);
        println!("    the same, counting only maps that are not flat everywhere:");
        print_ring("border ring", &self.border_ring_values_not_flat);
        println!(
            "    outermost ring holds a single sample value in {} of {} bordered maps",
            self.outer_ring_constant, self.bordered_maps
        );
        print_ring("outermost ring", &self.outer_ring_values);

        if !self.blend_cells_out_of_range.is_empty() {
            println!("  maps with out-of-range cell tile indices:");
            for note in &self.blend_cells_out_of_range {
                println!("    {note}");
            }
        }
        if !self.blend_tail_by_search.is_empty() {
            println!("  BlendTileData layouts found by search rather than the table:");
            for note in self.blend_tail_by_search.iter().take(20) {
                println!("    {note}");
            }
        }
        if !self.blend_unknown_version.is_empty() {
            println!(
                "  BlendTileData versions absent from the layout table: {:?}",
                self.blend_unknown_version
            );
        }
        if !self.failures.is_empty() {
            println!("  {} failures:", self.failures.len());
            for failure in self.failures.iter().take(40) {
                println!("    {failure}");
            }
        }
        println!();
        println!("  WorldInfo dictionary keys ({} chunks)", self.worldinfos);
        print_keys(&self.worldinfo_keys);
        println!("  Object dictionary keys ({} objects)", self.objects);
        print_keys(&self.object_keys);
    }
}

fn merge_counts<K: Ord>(into: &mut BTreeMap<K, u64>, from: BTreeMap<K, u64>) {
    for (key, count) in from {
        *into.entry(key).or_default() += count;
    }
}

fn print_keys(keys: &BTreeMap<String, u64>) {
    let mut rows: Vec<_> = keys.iter().collect();
    rows.sort_by_key(|(name, count)| (u64::MAX - **count, (*name).clone()));
    for (name, count) in rows {
        println!("    {name:<40} {count}");
    }
}

fn print_ring(label: &str, values: &BTreeMap<u16, u64>) {
    let mut rows: Vec<_> = values.iter().collect();
    rows.sort_by_key(|(value, count)| (u64::MAX - **count, **value));
    for (value, count) in rows.into_iter().take(12) {
        let implied = if *value == 0 {
            String::from("n/a")
        } else {
            format!("{:.9}", 100.0 / f64::from(*value))
        };
        println!("      {label} = {value} in {count} maps; 100.0/{value} = {implied}");
    }
}

fn line(label: &str, held: u64, of: u64) {
    let mark = if held == of { "ok " } else { "!! " };
    println!("    {mark}{label}: {held} of {of}");
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
            match mint_sage_refpack::decompress(stream) {
                Ok(body) => scan_body(&name, &body, totals),
                Err(error) => totals.failures.push(format!("{name}: {error}")),
            }
            totals.end_of_file();
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

fn scan_body(name: &str, body: &[u8], totals: &mut Totals) {
    totals.files = totals.files.saturating_add(1);
    totals.bodies_bytes = totals.bodies_bytes.saturating_add(body.len() as u64);

    let Some(table) = read_name_table(body) else {
        totals.failures.push(format!("{name}: no CkMp name table"));
        return;
    };
    if table.indices_are_dense {
        totals.dense_indices += 1;
    }
    let Some(top) = chunks(body, table.end, body.len(), &table) else {
        totals.failures.push(format!(
            "{name}: chunk walk did not reach the end of the body"
        ));
        return;
    };
    totals.walked_to_end += 1;

    catalogue(body, &top, 0, &table, totals);

    let mut dims: Option<(usize, usize)> = None;
    for header in &top {
        match header.name.as_str() {
            "HeightMapData" => match read_heightmap(body, header) {
                Some(hm) => {
                    totals.heightmaps += 1;
                    dims = Some((hm.width, hm.height));
                    let key = (
                        u32::try_from(hm.width).unwrap_or(u32::MAX),
                        u32::try_from(hm.height).unwrap_or(u32::MAX),
                    );
                    *totals.heightmap_dims.entry(key).or_default() += 1;
                    if Some(hm.count) == hm.width.checked_mul(hm.height) {
                        totals.heightmap_count_matches_area += 1;
                    }
                    if Some(hm.payload_after_count) == hm.count.checked_mul(2) {
                        totals.heightmap_payload_two_bytes += 1;
                    }
                    if hm.width > 2 && hm.height > 2 {
                        if totals.heightmaps == 1 {
                            totals.height_min = hm.min;
                        }
                        totals.height_min = totals.height_min.min(hm.min);
                        totals.height_max = totals.height_max.max(hm.max);
                        if hm.border > 0 {
                            totals.bordered_maps += 1;
                            let flat = hm.min == hm.max;
                            if flat {
                                totals.flat_maps += 1;
                            }
                            if hm.border_ring.is_constant() {
                                totals.border_ring_constant += 1;
                                *totals
                                    .border_ring_values
                                    .entry(hm.border_ring.first)
                                    .or_default() += 1;
                                if !flat {
                                    totals.border_ring_constant_not_flat += 1;
                                    *totals
                                        .border_ring_values_not_flat
                                        .entry(hm.border_ring.first)
                                        .or_default() += 1;
                                }
                            }
                            if hm.outer_ring.is_constant() {
                                totals.outer_ring_constant += 1;
                                *totals
                                    .outer_ring_values
                                    .entry(hm.outer_ring.first)
                                    .or_default() += 1;
                            }
                        }
                    }
                }
                None => totals
                    .failures
                    .push(format!("{name}: HeightMapData truncated")),
            },
            "BlendTileData" => read_blendtile(name, body, header, dims, totals),
            "WorldInfo" => {
                totals.worldinfos += 1;
                let mut cur = Cursor::new(body, header.body);
                match cur.dict() {
                    Some(keys) => {
                        for key in keys {
                            *totals.worldinfo_keys.entry(table.name_of(key)).or_default() += 1;
                        }
                    }
                    None => totals
                        .failures
                        .push(format!("{name}: WorldInfo dictionary truncated")),
                }
            }
            "ObjectsList" => read_objects(name, body, header, &table, totals),
            _ => {}
        }
    }
}

fn catalogue(
    body: &[u8],
    headers: &[ChunkHeader],
    depth: u32,
    table: &NameTable,
    totals: &mut Totals,
) {
    for header in headers {
        totals.note_chunk(header, depth);
        if depth >= MAX_DEPTH || header.len == 0 {
            continue;
        }
        let Some(end) = header.end() else { continue };
        if let Some(children) = chunks(body, header.body, end, table) {
            if !children.is_empty() {
                catalogue(body, &children, depth.saturating_add(1), table, totals);
            }
        }
    }
}

struct HeightMap {
    width: usize,
    height: usize,
    border: usize,
    count: usize,
    payload_after_count: usize,
    border_ring: RingSummary,
    outer_ring: RingSummary,
    min: u16,
    max: u16,
}

#[derive(Default, Clone, Copy)]
struct RingSummary {
    cells: u64,
    distinct: usize,
    first: u16,
}

impl RingSummary {
    fn is_constant(&self) -> bool {
        self.cells > 0 && self.distinct == 1
    }
}

const RING_DISTINCT_CAP: usize = 8;

fn summarise_ring(values: impl Iterator<Item = u16>) -> RingSummary {
    let mut seen: Vec<u16> = Vec::new();
    let mut cells = 0u64;
    for value in values {
        cells = cells.saturating_add(1);
        if seen.len() < RING_DISTINCT_CAP && !seen.contains(&value) {
            seen.push(value);
        }
    }
    RingSummary {
        cells,
        distinct: seen.len(),
        first: seen.first().copied().unwrap_or(0),
    }
}

fn read_heightmap(body: &[u8], header: &ChunkHeader) -> Option<HeightMap> {
    let mut cur = Cursor::new(body, header.body);
    let width = cur.u32()? as usize;
    let height = cur.u32()? as usize;
    let border = cur.u32()? as usize;
    let boundaries = cur.u32()? as usize;
    for _ in 0..boundaries {
        cur.u32()?;
        cur.u32()?;
    }
    let count = cur.u32()? as usize;
    let payload_after_count = header.end()?.checked_sub(cur.pos)?;

    let samples_at = cur.pos;
    let sample = |x: usize, y: usize| -> Option<u16> {
        let index = y.checked_mul(width)?.checked_add(x)?;
        let at = samples_at.checked_add(index.checked_mul(2)?)?;
        Cursor::new(body, at).u16()
    };
    let have_samples = payload_after_count == count.checked_mul(2)? && count == width * height;

    let mut border_ring = RingSummary::default();
    let mut outer_ring = RingSummary::default();
    let mut min = u16::MAX;
    let mut max = 0u16;
    if have_samples && width > 0 && height > 0 {
        let in_border = |x: usize, y: usize| {
            x < border || y < border || x + border >= width || y + border >= height
        };
        let is_outer = |x: usize, y: usize| x == 0 || y == 0 || x + 1 == width || y + 1 == height;
        border_ring = summarise_ring((0..height).flat_map(|y| {
            (0..width).filter_map(move |x| if in_border(x, y) { sample(x, y) } else { None })
        }));
        outer_ring = summarise_ring((0..height).flat_map(|y| {
            (0..width).filter_map(move |x| if is_outer(x, y) { sample(x, y) } else { None })
        }));
        for y in 0..height {
            for x in 0..width {
                if let Some(value) = sample(x, y) {
                    min = min.min(value);
                    max = max.max(value);
                }
            }
        }
    }

    Some(HeightMap {
        width,
        height,
        border,
        count,
        payload_after_count,
        border_ring,
        outer_ring,
        min,
        max,
    })
}

struct BlendTail {
    scalar_at: usize,
    bitmap_tiles: usize,
    blended_tiles: usize,
    cliff_infos: usize,
    class_tile_sum: usize,
    classes: usize,
    blends_at: usize,
    cliffs_at: usize,
}

fn parse_blend_tail(body: &[u8], at: usize, end: usize) -> Option<BlendTail> {
    let mut cur = Cursor::new(body, at);
    let bitmap_tiles = cur.i32()?;
    let blended_tiles = cur.i32()?;
    let cliff_infos = cur.i32()?;
    let classes = cur.i32()?;
    if bitmap_tiles <= 0 || blended_tiles <= 0 || cliff_infos < 0 || classes <= 0 {
        return None;
    }
    if classes > 4096 || blended_tiles > 1 << 22 || cliff_infos > 1 << 22 {
        return None;
    }
    let mut class_tile_sum: usize = 0;
    for _ in 0..classes {
        let _first_tile = cur.i32()?;
        let tiles = cur.i32()?;
        let _width = cur.i32()?;
        let _legacy = cur.i32()?;
        let class_name = cur.ascii()?;
        if class_name.len() > 128 || cur.pos > end {
            return None;
        }
        class_tile_sum = class_tile_sum.saturating_add(usize::try_from(tiles).unwrap_or(0));
    }
    let _edge_tiles = cur.i32()?;
    let edge_classes = cur.i32()?;
    if !(0..4096).contains(&edge_classes) {
        return None;
    }
    for _ in 0..edge_classes {
        let _first_tile = cur.i32()?;
        let _tiles = cur.i32()?;
        let _width = cur.i32()?;
        let class_name = cur.ascii()?;
        if class_name.len() > 128 || cur.pos > end {
            return None;
        }
    }
    let blends_at = cur.pos;
    let blend_records = usize::try_from(blended_tiles)
        .unwrap_or(0)
        .saturating_sub(1);
    let cliff_records = usize::try_from(cliff_infos).unwrap_or(0).saturating_sub(1);
    cur.take(blend_records.checked_mul(BLEND_RECORD_LEN)?)?;
    let cliffs_at = cur.pos;
    cur.take(cliff_records.checked_mul(CLIFF_RECORD_LEN)?)?;
    if cur.pos != end {
        return None;
    }
    Some(BlendTail {
        scalar_at: at,
        bitmap_tiles: usize::try_from(bitmap_tiles).unwrap_or(0),
        blended_tiles: usize::try_from(blended_tiles).unwrap_or(0),
        cliff_infos: usize::try_from(cliff_infos).unwrap_or(0),
        class_tile_sum,
        classes: usize::try_from(classes).unwrap_or(0),
        blends_at,
        cliffs_at,
    })
}

fn read_blendtile(
    name: &str,
    body: &[u8],
    header: &ChunkHeader,
    dims: Option<(usize, usize)>,
    totals: &mut Totals,
) {
    totals.blendtiles += 1;
    let Some((width, height)) = dims else {
        totals.failures.push(format!(
            "{name}: BlendTileData with no preceding HeightMapData"
        ));
        return;
    };
    let Some(area) = width.checked_mul(height) else {
        return;
    };
    let Some(end) = header.end() else { return };
    let mut cur = Cursor::new(body, header.body);
    let Some(len) = cur.u32() else {
        totals
            .failures
            .push(format!("{name}: BlendTileData truncated"));
        return;
    };
    if len as usize == area {
        totals.blend_len_matches_area += 1;
    }

    let plane_bytes = width
        .checked_add(7)
        .and_then(|w| w.checked_div(8))
        .and_then(|stride| stride.checked_mul(height))
        .unwrap_or(0);

    let known = BLEND_LAYOUT
        .iter()
        .find(|(version, _, _)| *version == header.version)
        .map(|(_, bytes, planes)| (*bytes, *planes));
    if known.is_none() {
        *totals
            .blend_unknown_version
            .entry(header.version)
            .or_default() += 1;
    }

    let mut tail = None;
    if let Some((bytes_per_cell, planes)) = known {
        if let Some(at) = array_end(header.body, area, bytes_per_cell, planes, plane_bytes) {
            tail = parse_blend_tail(body, at, end);
        }
    }
    if tail.is_none() {
        'search: for bytes_per_cell in 0..=24usize {
            for planes in 0..=14usize {
                let Some(at) = array_end(header.body, area, bytes_per_cell, planes, plane_bytes)
                else {
                    continue;
                };
                if at >= end {
                    continue;
                }
                if let Some(found) = parse_blend_tail(body, at, end) {
                    totals.blend_tail_by_search.push(format!(
                        "{name}: version {} -> {bytes_per_cell} bytes/cell, {planes} bit planes",
                        header.version
                    ));
                    tail = Some(found);
                    break 'search;
                }
            }
        }
    }
    let Some(tail) = tail else {
        totals
            .failures
            .push(format!("{name}: BlendTileData scalar section not located"));
        return;
    };
    totals.blend_tail_exact += 1;
    if tail.class_tile_sum == tail.bitmap_tiles {
        totals.blend_class_sum_ok += 1;
    }
    totals.blend_texture_classes = totals
        .blend_texture_classes
        .saturating_add(u64::try_from(tail.classes).unwrap_or(0));

    let mut markers_ok = true;
    let mut record_tiles_ok = true;
    let mut at = tail.blends_at;
    for _ in 0..tail.blended_tiles.saturating_sub(1) {
        let mut record = Cursor::new(body, at);
        match record.i32() {
            Some(tile) if in_tile_range(tile, tail.bitmap_tiles) => {}
            _ => record_tiles_ok = false,
        }
        let mut marker = Cursor::new(body, at.saturating_add(14));
        if marker.u32() != Some(BLEND_RECORD_MARKER) {
            markers_ok = false;
        }
        at = at.saturating_add(BLEND_RECORD_LEN);
        totals.blend_records_checked = totals.blend_records_checked.saturating_add(1);
    }
    if markers_ok {
        totals.blend_marker_ok += 1;
    }
    if record_tiles_ok {
        totals.blend_record_tile_ok += 1;
    }

    let mut cliff_tiles_ok = true;
    let mut at = tail.cliffs_at;
    for _ in 0..tail.cliff_infos.saturating_sub(1) {
        let mut record = Cursor::new(body, at);
        match record.i32() {
            Some(tile) if in_tile_range(tile, tail.bitmap_tiles) => {}
            _ => cliff_tiles_ok = false,
        }
        at = at.saturating_add(CLIFF_RECORD_LEN);
        totals.blend_cliff_records_checked = totals.blend_cliff_records_checked.saturating_add(1);
    }
    if cliff_tiles_ok {
        totals.blend_cliff_tile_ok += 1;
    }

    let mut out_of_range = 0u64;
    let mut cells = Cursor::new(body, header.body.saturating_add(4));
    for _ in 0..area {
        let Some(index) = cells.u16() else { break };
        if usize::from(index >> 2) >= tail.bitmap_tiles {
            out_of_range += 1;
        }
    }
    if out_of_range == 0 {
        totals.blend_cell_tile_ok += 1;
    } else {
        totals.blend_cells_out_of_range.push(format!(
            "{name}: {out_of_range} of {area} cells index past numBitmapTiles ({}); \
             scalar section at chunk offset {}",
            tail.bitmap_tiles,
            tail.scalar_at.saturating_sub(header.body)
        ));
    }
}

fn in_tile_range(tile: i32, bitmap_tiles: usize) -> bool {
    usize::try_from(tile).is_ok_and(|tile| tile >> 2 < bitmap_tiles)
}

fn array_end(
    body_start: usize,
    area: usize,
    bytes_per_cell: usize,
    planes: usize,
    plane_bytes: usize,
) -> Option<usize> {
    body_start
        .checked_add(4)?
        .checked_add(area.checked_mul(bytes_per_cell)?)?
        .checked_add(planes.checked_mul(plane_bytes)?)
}

fn read_objects(
    name: &str,
    body: &[u8],
    header: &ChunkHeader,
    table: &NameTable,
    totals: &mut Totals,
) {
    let Some(end) = header.end() else { return };
    let Some(children) = chunks(body, header.body, end, table) else {
        totals.failures.push(format!(
            "{name}: ObjectsList children do not tile the payload"
        ));
        return;
    };
    for child in &children {
        if child.name != "Object" {
            totals
                .failures
                .push(format!("{name}: ObjectsList child named {}", child.name));
            continue;
        }
        totals.objects += 1;
        let mut cur = Cursor::new(body, child.body);
        let parsed = (|| -> Option<()> {
            let _x = cur.f32()?;
            let _y = cur.f32()?;
            let _z = cur.f32()?;
            let _angle = cur.f32()?;
            let _flags = cur.i32()?;
            let template = cur.ascii()?;
            if !template.is_empty() {
                totals.objects_named += 1;
            }
            for key in cur.dict()? {
                *totals.object_keys.entry(table.name_of(key)).or_default() += 1;
            }
            Some(())
        })();
        match (parsed, child.end()) {
            (Some(()), Some(child_end)) if cur.pos == child_end => totals.objects_exact += 1,
            (Some(()), Some(child_end)) => totals.failures.push(format!(
                "{name}: Object left {} bytes unread",
                child_end.saturating_sub(cur.pos)
            )),
            _ => totals
                .failures
                .push(format!("{name}: Object fields ran past the chunk")),
        }
    }
}
