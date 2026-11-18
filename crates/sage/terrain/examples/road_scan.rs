use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::{HeightMapData, MapFile, MAP_XY_FACTOR};
use mint_sage_model::{load_object_index, load_texture, ObjectIndex, Resolver};
use mint_sage_terrain::{
    build_roads, drawn_surface_height, surface_height, RoadChainOrder, RoadHeight, RoadMesh,
    RoadOptions, RoadSkip, RoadStyle, RoadTessellation, StepsPerCell, ROAD_END_FLAG,
    ROAD_LIFT_HEIGHT_STEP_EIGHTH, ROAD_LIFT_XY_128, ROAD_START_FLAG, STRAIGHT_BAND_V,
    TEXTURE_REPEAT_WIDTHS,
};
use mint_vfs::Vfs;

const VERTICAL_SCALE: f32 = 0.039_062_5;

const KNOWN_FLAG_BITS: i32 = 0x1FF;

const INTERIOR_LATTICE: f32 = MAP_XY_FACTOR / 2.0;

const INTERIOR_LATTICE_LIMIT: i64 = 4096;

const BAND_TOLERANCE: f32 = 0.001;

const WEDGE_CEILING: f64 = 5.0;

const SEAM_RUNGS: [f32; 4] = [BAND_TOLERANCE, 0.01, 0.1, 0.5];

const LIFT_RUNGS: [f32; 5] = [
    ROAD_LIFT_HEIGHT_STEP_EIGHTH,
    ROAD_LIFT_XY_128,
    0.312_5,
    1.25,
    5.0,
];

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut options = RoadOptions::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tessellation" => {
                let Some(value) = args.next() else {
                    eprintln!("--tessellation wants ribbon, 1, 2 or 4");
                    return ExitCode::FAILURE;
                };
                options.tessellation = match value.as_str() {
                    "ribbon" => RoadTessellation::Ribbon,
                    "1" => RoadTessellation::Grid(StepsPerCell::One),
                    "2" => RoadTessellation::Grid(StepsPerCell::Two),
                    "4" => RoadTessellation::Grid(StepsPerCell::Four),
                    other => {
                        eprintln!("--tessellation wants ribbon, 1, 2 or 4, not {other:?}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            "--chain-order" => {
                let Some(value) = args.next() else {
                    eprintln!("--chain-order wants file, chained or reversed");
                    return ExitCode::FAILURE;
                };
                options.chain_order = match value.as_str() {
                    "file" => RoadChainOrder::AsFile,
                    "chained" => RoadChainOrder::Chained,
                    "reversed" => RoadChainOrder::Reversed,
                    other => {
                        eprintln!("--chain-order wants file, chained or reversed, not {other:?}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            "--road-height" => {
                options.height = match args.next().as_deref() {
                    Some("surface") => RoadHeight::Surface,
                    Some("max") => RoadHeight::StationMax,
                    Some("cell") => RoadHeight::CellMax,
                    other => {
                        eprintln!("--road-height wants surface, max or cell, not {other:?}");
                        return ExitCode::FAILURE;
                    }
                };
            }
            "--lift" => {
                let Some(value) = args.next().and_then(|v| v.parse::<f32>().ok()) else {
                    eprintln!("--lift wants a number");
                    return ExitCode::FAILURE;
                };
                options.lift = value;
            }
            other => roots.push(PathBuf::from(other)),
        }
    }
    if roots.is_empty() {
        eprintln!(
            "usage: road_scan [--tessellation ribbon|1|2|4] [--chain-order file|chained|reversed] [--lift <n>] <install dir> [<install dir> ...]"
        );
        return ExitCode::FAILURE;
    }
    println!(
        "tessellation = {}, height = {}, chain order = {}, lift = {} world units",
        options.tessellation.label(),
        options.height.label(),
        options.chain_order.label(),
        options.lift
    );

    let mut vfs = Vfs::new();
    for root in &roots {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    let started = Instant::now();
    let (index, report) = load_object_index(&vfs);
    println!(
        "{} INI document(s), {} parsed; {} road declaration(s) in {:.1} s",
        report.entries,
        report.parsed,
        index.roads(),
        started.elapsed().as_secs_f64()
    );

    report_declarations(&vfs, &index);

    let mut totals = Totals::default();
    let started = Instant::now();
    for root in &roots {
        for archive_path in big_archives(root) {
            let Ok(archive) = BigArchive::open(&archive_path) else {
                continue;
            };
            for entry in archive.entries() {
                let lower = entry.name.to_ascii_lowercase();
                if !(lower.ends_with(".map") || lower.ends_with(".bse")) {
                    continue;
                }
                let Ok(raw) = archive.read_entry(entry) else {
                    totals.unreadable.push(entry.name.clone());
                    continue;
                };
                match MapFile::parse(&raw) {
                    Ok(map) => totals.add(&lower, &map, &index, &options),
                    Err(error) => totals.failures.push(format!("{}: {error}", entry.name)),
                }
            }
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let ok = totals.report(seconds);
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn report_declarations(vfs: &Vfs, index: &ObjectIndex) {
    let resolver = Resolver::new();
    let (mut resolved, mut decoded, mut widths) = (0usize, 0usize, BTreeSet::new());
    let mut failures: Vec<String> = Vec::new();
    let mut in_texture: BTreeMap<String, usize> = BTreeMap::new();
    let mut incomplete = 0usize;
    for (name, road) in index.road_definitions() {
        if road.texture.is_none() || road.width.is_none() || road.width_in_texture.is_none() {
            incomplete = incomplete.saturating_add(1);
        }
        if let Some(width) = road.width {
            widths.insert(width.to_bits());
        }
        if let Some(value) = road.width_in_texture {
            *in_texture.entry(format!("{value}")).or_default() += 1;
        }
        let Some(texture) = road.texture.as_deref() else {
            continue;
        };
        match load_texture(vfs, &resolver, texture, true) {
            Ok(loaded) => {
                resolved = resolved.saturating_add(1);
                decoded = decoded.saturating_add(1);
                let _ = loaded;
            }
            Err(error) => {
                if resolver.resolve(vfs, texture).is_ok() {
                    resolved = resolved.saturating_add(1);
                }
                failures.push(format!("{name} -> {texture}: {error}"));
            }
        }
    }
    println!(
        "road declarations: {} with all three fields, {incomplete} missing one; \
         {} distinct RoadWidth value(s), RoadWidthInTexture {in_texture:?}",
        index.roads().saturating_sub(incomplete),
        widths.len()
    );
    println!("  textures: {resolved} resolve to a mounted file, {decoded} decode");
    for line in &failures {
        println!("  TEXTURE {line}");
    }
}

#[derive(Default)]
struct Totals {
    files: u64,
    files_with_roads: u64,
    objects: u64,
    road_flagged: u64,
    starts: u64,
    ends: u64,
    both_bits: u64,
    unknown_flag_bits: u64,
    segments: u64,
    drawn: u64,
    skipped: BTreeMap<String, u64>,
    degenerate: u64,
    off_map: u64,
    name_mismatch: u64,
    endpoint_classes: [u64; 5],
    endpoint_ends: u64,
    mitred: u64,
    mitre_capped: u64,
    opposed_joins: u64,
    reoriented: u64,
    orientation_conflicts: u64,
    duplicate_spans: u64,
    end_caps: u64,
    end_caps_drawn: u64,
    chains: u64,
    chains_open: u64,
    chains_one_capped_end: u64,
    tees: u64,
    crossroads: u64,
    junction_y: u64,
    junction_h: u64,
    junction_degenerate: u64,
    buried: u64,
    buried_past_lift: u64,
    deepest_bury: f64,
    interior_samples: u64,
    interior_under: u64,
    interior_under_lift: u64,
    deepest_interior: f64,
    interior_over_rung: [u64; LIFT_RUNGS.len()],
    worst_interior: BTreeMap<String, (f64, f32, f32)>,
    wedges: BTreeMap<String, (u64, f64, f32, f32)>,
    band_vertices: u64,
    band_outside: u64,
    band_worst: f32,
    band_worst_at: (String, f32, f32),
    seam_points: u64,
    seam_disagreeing: u64,
    seam_rungs: [u64; SEAM_RUNGS.len()],
    seam_worst: f32,
    seam_worst_at: (String, f32, f32),
    road_vertices: u64,
    vertices: u64,
    triangles: u64,
    non_finite_vertices: u64,
    undeclared: BTreeMap<String, BTreeSet<String>>,
    per_map: Vec<(u64, String)>,
    junctions_per_map: Vec<(u64, String)>,
    failures: Vec<String>,
    unreadable: Vec<String>,
}

impl Totals {
    fn add(&mut self, name: &str, map: &MapFile, index: &ObjectIndex, options: &RoadOptions) {
        self.files = self.files.saturating_add(1);
        for object in &map.objects {
            self.objects = self.objects.saturating_add(1);
            if object.flags & !KNOWN_FLAG_BITS != 0 {
                self.unknown_flag_bits = self.unknown_flag_bits.saturating_add(1);
            }
            let start = object.flags & ROAD_START_FLAG != 0;
            let end = object.flags & ROAD_END_FLAG != 0;
            if start {
                self.starts = self.starts.saturating_add(1);
            }
            if end {
                self.ends = self.ends.saturating_add(1);
            }
            if start && end {
                self.both_bits = self.both_bits.saturating_add(1);
            }
        }

        let built = build_roads(
            &map.height_map,
            VERTICAL_SCALE,
            &map.objects,
            options,
            |road| style_for(index, road),
        );
        let stats = &built.stats;
        if stats.segments == 0 {
            return;
        }
        self.files_with_roads = self.files_with_roads.saturating_add(1);
        let count = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
        self.road_flagged = self.road_flagged.saturating_add(count(stats.placements));
        self.segments = self.segments.saturating_add(count(stats.segments));
        self.drawn = self.drawn.saturating_add(count(stats.drawn));
        self.degenerate = self.degenerate.saturating_add(count(stats.degenerate));
        self.off_map = self.off_map.saturating_add(count(stats.off_map));
        self.name_mismatch = self
            .name_mismatch
            .saturating_add(count(stats.name_mismatch));
        self.mitred = self.mitred.saturating_add(count(stats.mitred));
        self.mitre_capped = self.mitre_capped.saturating_add(count(stats.mitre_capped));
        self.opposed_joins = self
            .opposed_joins
            .saturating_add(count(stats.opposed_joins));
        self.reoriented = self.reoriented.saturating_add(count(stats.reoriented));
        self.orientation_conflicts = self
            .orientation_conflicts
            .saturating_add(count(stats.orientation_conflicts));
        self.duplicate_spans = self
            .duplicate_spans
            .saturating_add(count(stats.duplicate_spans));
        self.end_caps = self.end_caps.saturating_add(count(stats.end_caps));
        self.end_caps_drawn = self
            .end_caps_drawn
            .saturating_add(count(stats.end_caps_drawn));
        self.chains = self.chains.saturating_add(count(stats.chains));
        self.chains_open = self.chains_open.saturating_add(count(stats.chains_open));
        self.chains_one_capped_end = self
            .chains_one_capped_end
            .saturating_add(count(stats.chains_one_capped_end));
        self.tees = self.tees.saturating_add(count(stats.tees));
        self.crossroads = self.crossroads.saturating_add(count(stats.crossroads));
        self.junction_y = self.junction_y.saturating_add(count(stats.junction_y));
        self.junction_h = self.junction_h.saturating_add(count(stats.junction_h));
        self.junction_degenerate = self
            .junction_degenerate
            .saturating_add(count(stats.junction_degenerate));
        self.vertices = self.vertices.saturating_add(count(stats.vertices));
        self.triangles = self.triangles.saturating_add(count(stats.triangles));
        for (reason, number) in &stats.skipped {
            let key = match reason {
                RoadSkip::UndeclaredType => "type not declared",
                RoadSkip::UnusableWidth => "declared width unusable",
                RoadSkip::NonFinite => "endpoint not finite",
            };
            *self.skipped.entry(key.to_owned()).or_default() += count(*number);
        }
        self.endpoint_ends = self
            .endpoint_ends
            .saturating_add(count(stats.endpoint_ends));
        for (slot, value) in stats.endpoint_classes.iter().enumerate() {
            if let Some(total) = self.endpoint_classes.get_mut(slot) {
                *total = total.saturating_add(count(*value));
            }
        }
        for road in &built.meshes {
            for vertex in &road.mesh.vertices {
                if !vertex.position.into_iter().all(f32::is_finite) {
                    self.non_finite_vertices = self.non_finite_vertices.saturating_add(1);
                }
                self.road_vertices = self.road_vertices.saturating_add(1);
                let [x, y, _] = vertex.position;
                let bilinear = surface_height(&map.height_map, VERTICAL_SCALE, x, y);
                let drawn = drawn_surface_height(&map.height_map, VERTICAL_SCALE, x, y);
                if let (Some(bilinear), Some(drawn)) = (bilinear, drawn) {
                    let gap = f64::from(drawn - bilinear);
                    if gap > 0.0 {
                        self.buried = self.buried.saturating_add(1);
                        if gap > f64::from(ROAD_LIFT_XY_128) {
                            self.buried_past_lift = self.buried_past_lift.saturating_add(1);
                        }
                        if gap > self.deepest_bury {
                            self.deepest_bury = gap;
                        }
                    }
                }
            }
            self.measure_band(name, road);
            self.measure_interior(name, &map.height_map, road, options.lift);
        }
        for road in built.undeclared {
            self.undeclared
                .entry(road)
                .or_default()
                .insert(name.to_owned());
        }
        self.per_map.push((count(stats.segments), name.to_owned()));
        let junctions = stats
            .endpoint_classes
            .iter()
            .skip(2)
            .fold(0usize, |sum, value| sum.saturating_add(*value));
        self.junctions_per_map
            .push((count(junctions), name.to_owned()));
    }

    fn measure_band(&mut self, name: &str, road: &RoadMesh) {
        let Some(half_width) = road.style.half_width() else {
            return;
        };
        let repeat = TEXTURE_REPEAT_WIDTHS * road.style.width;
        if !(repeat.is_finite() && repeat > 0.0) {
            return;
        }
        let half_band = half_width / repeat;
        if !(half_band.is_finite() && half_band > 0.0) {
            return;
        }
        let ribbon = road
            .mesh
            .vertices
            .get(..road.segment_vertices)
            .unwrap_or(&road.mesh.vertices);
        let mut seams: BTreeMap<(u32, u32), (f32, f32, u32)> = BTreeMap::new();
        for vertex in ribbon {
            let v = vertex.uv[1];
            if !v.is_finite() {
                continue;
            }
            let [x, y, _] = vertex.position;
            let across = v - STRAIGHT_BAND_V;
            if x.is_finite() && y.is_finite() {
                let slot = seams
                    .entry((x.to_bits(), y.to_bits()))
                    .or_insert((across, across, 0u32));
                slot.0 = slot.0.min(across);
                slot.1 = slot.1.max(across);
                slot.2 = slot.2.saturating_add(1);
            }
            self.band_vertices = self.band_vertices.saturating_add(1);
            let over = ((v - STRAIGHT_BAND_V).abs() - half_band) / half_band;
            if over > BAND_TOLERANCE {
                self.band_outside = self.band_outside.saturating_add(1);
                if over > self.band_worst {
                    self.band_worst = over;
                    self.band_worst_at = (name.to_owned(), vertex.position[0], vertex.position[1]);
                }
            }
        }
        for ((xb, yb), (low, high, held)) in seams {
            if held < 2 {
                continue;
            }
            self.seam_points = self.seam_points.saturating_add(1);
            let spread = (high - low) / half_band;
            if spread > BAND_TOLERANCE {
                self.seam_disagreeing = self.seam_disagreeing.saturating_add(1);
                for (slot, rung) in SEAM_RUNGS.iter().enumerate() {
                    if spread > *rung {
                        if let Some(count) = self.seam_rungs.get_mut(slot) {
                            *count = count.saturating_add(1);
                        }
                    }
                }
                if spread > self.seam_worst {
                    self.seam_worst = spread;
                    self.seam_worst_at = (name.to_owned(), f32::from_bits(xb), f32::from_bits(yb));
                }
            }
        }
    }

    fn measure_interior(
        &mut self,
        name: &str,
        height_map: &HeightMapData,
        road: &RoadMesh,
        lift: f32,
    ) {
        let mut worst = (0.0f64, 0.0f32, 0.0f32);
        let mut wedge = (0u64, 0.0f64, 0.0f32, 0.0f32);
        for tri in road.mesh.indices.chunks_exact(3) {
            let corner = |slot: usize| -> Option<[f32; 3]> {
                let index = usize::try_from(*tri.get(slot)?).ok()?;
                let vertex = road.mesh.vertices.get(index)?;
                vertex
                    .position
                    .into_iter()
                    .all(f32::is_finite)
                    .then_some(vertex.position)
            };
            let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
                continue;
            };
            let area2 = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
            if !area2.is_finite() || area2.abs() < f32::EPSILON {
                continue;
            }
            let mut probe = |x: f32, y: f32| {
                let w_a = ((b[0] - x) * (c[1] - y) - (c[0] - x) * (b[1] - y)) / area2;
                let w_b = ((c[0] - x) * (a[1] - y) - (a[0] - x) * (c[1] - y)) / area2;
                let w_c = 1.0 - w_a - w_b;
                if w_a < 0.0 || w_b < 0.0 || w_c < 0.0 {
                    return;
                }
                let road_z = w_a * a[2] + w_b * b[2] + w_c * c[2] - lift;
                let Some(ground) = drawn_surface_height(height_map, VERTICAL_SCALE, x, y) else {
                    return;
                };
                self.interior_samples = self.interior_samples.saturating_add(1);
                let under = f64::from(ground - road_z);
                if under > 0.0 {
                    self.interior_under = self.interior_under.saturating_add(1);
                    if under > f64::from(ROAD_LIFT_XY_128) {
                        self.interior_under_lift = self.interior_under_lift.saturating_add(1);
                    }
                    if under > self.deepest_interior {
                        self.deepest_interior = under;
                    }
                    if under > worst.0 {
                        worst = (under, x, y);
                    }
                    if under > f64::from(ROAD_LIFT_XY_128) {
                        wedge.0 = wedge.0.saturating_add(1);
                        if under > wedge.1 && under < WEDGE_CEILING {
                            wedge = (wedge.0, under, x, y);
                        }
                    }
                    for (slot, rung) in LIFT_RUNGS.iter().enumerate() {
                        if under > f64::from(*rung) {
                            if let Some(count) = self.interior_over_rung.get_mut(slot) {
                                *count = count.saturating_add(1);
                            }
                        }
                    }
                }
            };

            let third = 1.0 / 3.0;
            probe((a[0] + b[0] + c[0]) * third, (a[1] + b[1] + c[1]) * third);
            for (p, q) in [(a, b), (b, c), (c, a)] {
                probe((p[0] + q[0]) * 0.5, (p[1] + q[1]) * 0.5);
            }

            let lo_x = a[0].min(b[0]).min(c[0]);
            let hi_x = a[0].max(b[0]).max(c[0]);
            let lo_y = a[1].min(b[1]).min(c[1]);
            let hi_y = a[1].max(b[1]).max(c[1]);
            let steps = |lo: f32, hi: f32| -> Option<(i64, i64)> {
                let first = (lo / INTERIOR_LATTICE).ceil();
                let last = (hi / INTERIOR_LATTICE).floor();
                if !(first.is_finite() && last.is_finite() && last >= first) {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation)]
                Some((first as i64, last as i64))
            };
            let (Some((first_x, last_x)), Some((first_y, last_y))) =
                (steps(lo_x, hi_x), steps(lo_y, hi_y))
            else {
                continue;
            };
            if last_x.saturating_sub(first_x) > INTERIOR_LATTICE_LIMIT
                || last_y.saturating_sub(first_y) > INTERIOR_LATTICE_LIMIT
            {
                continue;
            }
            for ix in first_x..=last_x {
                for iy in first_y..=last_y {
                    probe(ix as f32 * INTERIOR_LATTICE, iy as f32 * INTERIOR_LATTICE);
                }
            }
        }
        if wedge.0 > 0 {
            let slot = self
                .wedges
                .entry(name.to_owned())
                .or_insert((0, 0.0, 0.0, 0.0));
            slot.0 = slot.0.saturating_add(wedge.0);
            if wedge.1 > slot.1 {
                *slot = (slot.0, wedge.1, wedge.2, wedge.3);
            }
        }
        if worst.0 > 0.0 {
            let slot = self
                .worst_interior
                .entry(name.to_owned())
                .or_insert((0.0, 0.0, 0.0));
            if worst.0 > slot.0 {
                *slot = worst;
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn report(&self, seconds: f64) -> bool {
        let share = |part: u64, whole: u64| -> f64 {
            if whole == 0 {
                0.0
            } else {
                (part as f64) * 100.0 / (whole as f64)
            }
        };
        println!();
        println!(
            "{} map file(s) in {seconds:.1} s ({:.0} files/s), {} carry roads",
            self.files,
            (self.files as f64) / seconds.max(1e-9),
            self.files_with_roads
        );
        println!("{} object(s), {} carrying a road bit", self.objects, {
            self.starts.saturating_add(self.ends)
        });
        println!(
            "  starts {} == ends {} == segments {}: {}",
            self.starts,
            self.ends,
            self.segments,
            yes_no(self.starts == self.ends && self.starts == self.segments)
        );
        println!(
            "  objects carrying both bits: {} ({})",
            self.both_bits,
            yes_no(self.both_bits == 0)
        );
        println!(
            "  objects setting a flag bit outside the decoded nine: {} ({})",
            self.unknown_flag_bits,
            yes_no(self.unknown_flag_bits == 0)
        );

        println!(
            "segments: {} built, {} drawn ({:.2}%), {} skipped",
            self.segments,
            self.drawn,
            share(self.drawn, self.segments),
            self.segments.saturating_sub(self.drawn)
        );
        for (reason, count) in &self.skipped {
            println!("  skipped {count:>6}  {reason}");
        }
        println!(
            "  {} zero-length (nudged), {} touching ground off the heightmap, {} name mismatches",
            self.degenerate, self.off_map, self.name_mismatch
        );
        println!(
            "  {} vertices, {} triangles, {} non-finite vertex position(s) ({})",
            self.vertices,
            self.triangles,
            self.non_finite_vertices,
            yes_no(self.non_finite_vertices == 0)
        );

        let endpoints: u64 = self.endpoint_classes.iter().copied().sum();
        println!("endpoints: {endpoints} distinct location(s)");
        for (slot, label) in [
            "free end",
            "two-way join",
            "tee / Y (three)",
            "crossroads (four)",
            "five or more",
        ]
        .into_iter()
        .enumerate()
        {
            let count = self.endpoint_classes.get(slot).copied().unwrap_or(0);
            println!("  {label:<18} {count:>7}  {:.3}%", share(count, endpoints));
        }
        let doubled = self.drawn.saturating_mul(2);
        println!(
            "  ends bucketed {} vs 2 x drawn segments {doubled}: {}",
            self.endpoint_ends,
            yes_no(self.endpoint_ends == doubled)
        );
        println!(
            "joins: {} mitred, {} left square by the mitre cap ({:.2}% of two-way joins)",
            self.mitred,
            self.mitre_capped,
            share(
                self.mitre_capped,
                self.mitred.saturating_add(self.mitre_capped)
            )
        );
        println!(
            "chains: {} two-way join(s) stored nose to nose or tail to tail ({:.2}% of them), {} segment(s) turned round, {} conflict(s)",
            self.opposed_joins,
            share(
                self.opposed_joins,
                self.endpoint_classes.get(1).copied().unwrap_or(0)
            ),
            self.reoriented,
            self.orientation_conflicts
        );
        println!(
            "  {} segment(s) lie on top of another of the same type, and are made to run with it rather than against it",
            self.duplicate_spans
        );
        println!(
            "  {} chain(s), {} of them open at both ends; {} of those carry the alpha end-cap flag at exactly one end ({:.2}%)",
            self.chains,
            self.chains_open,
            self.chains_one_capped_end,
            share(self.chains_one_capped_end, self.chains_open)
        );
        println!(
            "  {} free end(s) carry the alpha end-cap flag, {} given a cap piece",
            self.end_caps, self.end_caps_drawn
        );
        let three = self
            .tees
            .saturating_add(self.junction_y)
            .saturating_add(self.junction_h);
        println!(
            "junctions: {} tee of {three} three-way ({:.2}%), {} crossroads of {} four-way",
            self.tees,
            share(self.tees, three),
            self.crossroads,
            self.endpoint_classes.get(3).copied().unwrap_or(0)
        );
        println!(
            "  left without a piece: {} Y, {} slanted H, {} with no usable frame",
            self.junction_y, self.junction_h, self.junction_degenerate
        );
        println!(
            "ground: {} of {} road vertices sit where bilinear is below the drawn terrain",
            self.buried, self.road_vertices
        );
        println!(
            "  {} of them by more than the default lift {ROAD_LIFT_XY_128}; deepest {:.2} world units",
            self.buried_past_lift, self.deepest_bury
        );
        println!(
            "seams: {} of {} shared ribbon point(s) have the two segments disagreeing on the band ({})",
            self.seam_disagreeing,
            self.seam_points,
            yes_no(self.seam_disagreeing == 0)
        );
        if self.seam_disagreeing > 0 {
            let (map, x, y) = &self.seam_worst_at;
            println!(
                "  worst {:.4} of a half-width apart, at ({x:.0}, {y:.0})  {map}",
                self.seam_worst
            );
            for (slot, rung) in SEAM_RUNGS.iter().enumerate() {
                println!(
                    "    more than {rung:>6} of a half-width apart: {:>6}",
                    self.seam_rungs.get(slot).copied().unwrap_or(0)
                );
            }
        }
        println!(
            "atlas band: {} of {} ribbon vertices sit outside the straight band ({})",
            self.band_outside,
            self.band_vertices,
            yes_no(self.band_outside == 0)
        );
        if self.band_outside > 0 {
            let (map, x, y) = &self.band_worst_at;
            println!(
                "  worst {:.4} of a half-width off, at ({x:.0}, {y:.0})  {map}",
                self.band_worst
            );
        }
        println!(
            "clipping: {} of {} points sampled inside road triangles are under the drawn terrain",
            self.interior_under, self.interior_samples
        );
        println!(
            "  {} of them by more than the lift {}; deepest {:.4} world units ({})",
            self.interior_under_lift,
            ROAD_LIFT_XY_128,
            self.deepest_interior,
            yes_no(self.interior_under_lift == 0)
        );
        let mut worst: Vec<(&f64, &str, f32, f32)> = self
            .worst_interior
            .iter()
            .map(|(map, (depth, x, y))| (depth, map.as_str(), *x, *y))
            .collect();
        worst.sort_by(|a, b| b.0.total_cmp(a.0).then(a.1.cmp(b.1)));
        println!("  how much of the ribbon each rung of the lift ladder leaves buried:");
        for (slot, rung) in LIFT_RUNGS.iter().enumerate() {
            let over = self.interior_over_rung.get(slot).copied().unwrap_or(0);
            println!(
                "    lift {rung:<10} {over:>9} sample(s) still under, {:.4}% of the ribbon",
                share(over, self.interior_samples)
            );
        }
        let mut wedges: Vec<(&u64, &str, f64, f32, f32)> = self
            .wedges
            .iter()
            .map(|(map, (count, depth, x, y))| (count, map.as_str(), *depth, *x, *y))
            .collect();
        wedges.sort_by(|a, b| b.0.cmp(a.0).then(a.1.cmp(b.1)));
        println!("  most widespread, with a world point to fly to:");
        for (count, map, depth, x, y) in wedges.iter().take(8) {
            println!("    {count:>7} sample(s), {depth:>6.2} deep at ({x:.0}, {y:.0})  {map}");
        }
        println!("  worst map(s), with the world point to fly to:");
        for (depth, map, x, y) in worst.iter().take(8) {
            println!("    {depth:>8.2}  ({x:.0}, {y:.0})  {map}");
        }

        if self.undeclared.is_empty() {
            println!("every placed road type is declared");
        } else {
            println!("road types placed but not declared:");
            for (name, maps) in &self.undeclared {
                let listed: Vec<&str> = maps.iter().take(3).map(String::as_str).collect();
                println!("  {name} on {} map(s): {}", maps.len(), listed.join(", "));
            }
        }

        let mut busiest = self.per_map.clone();
        busiest.sort_by(|a, b| b.cmp(a));
        println!("busiest maps by road segment:");
        for (count, name) in busiest.iter().take(10) {
            println!("  {count:>6}  {name}");
        }
        let mut junctions = self.junctions_per_map.clone();
        junctions.sort_by(|a, b| b.cmp(a));
        println!("maps with the most junctions of three or more (what this round leaves out):");
        for (count, name) in junctions.iter().take(6) {
            println!("  {count:>6}  {name}");
        }

        for failure in &self.failures {
            println!("PARSE FAILURE {failure}");
        }
        for name in &self.unreadable {
            println!("UNREADABLE {name}");
        }

        self.failures.is_empty()
            && self.both_bits == 0
            && self.non_finite_vertices == 0
            && self.endpoint_ends == doubled
            && self.starts == self.ends
            && self.starts == self.segments
    }
}

fn style_for(index: &ObjectIndex, name: &str) -> Option<RoadStyle> {
    let road = index.road(name)?;
    Some(RoadStyle {
        width: road.width.unwrap_or(0.0),
        width_in_texture: road.width_in_texture.unwrap_or(0.0),
    })
}

fn yes_no(ok: bool) -> &'static str {
    if ok {
        "ok"
    } else {
        "!! MISMATCH !!"
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
