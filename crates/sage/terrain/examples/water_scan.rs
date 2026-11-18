use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use mint_sage_big::BigArchive;
use mint_sage_map::{MapFile, RiverArea, StandingWaterArea};
use mint_sage_model::Resolver;
use mint_sage_terrain::{
    build_rivers, build_standing_water, surface_height, RiverSkip, WaterSkip, WATER_UV_WORLD_SIZE,
};
use mint_vfs::Vfs;

const VERTICAL_SCALE: f32 = 0.039_062_5;

const PROBE_STEPS: u32 = 32;

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        if let Some(corpus) = std::env::var_os("MINT_CORPUS") {
            roots.push(PathBuf::from(corpus));
        }
    }
    if roots.is_empty() {
        eprintln!("usage: water_scan <install dir> [<install dir> ...]  (or set MINT_CORPUS)");
        return ExitCode::FAILURE;
    }

    let mut vfs = Vfs::new();
    for root in roots.iter().skip(1).rev().chain(roots.first()) {
        for path in big_archives(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(std::sync::Arc::new(archive));
            }
        }
    }
    println!(
        "{} entries visible across {} install(s)",
        vfs.len(),
        roots.len()
    );

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
                    Ok(map) => totals.add(&lower, &map),
                    Err(error) => totals.failures.push(format!("{}: {error}", entry.name)),
                }
            }
        }
    }
    totals.report(started.elapsed().as_secs_f64());
    totals.report_texture_resolution(&vfs);
    totals.report_shadow_lightness(&vfs);

    let clean = totals.failures.is_empty()
        && totals.backwards_triangles == 0
        && totals.forced_clips == 0
        && totals.skipped.is_empty()
        && totals
            .river_skipped
            .keys()
            .all(|reason| *reason == RiverSkip::Degenerate);
    if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Default)]
struct HeightVerdict {
    world: u64,
    samples: u64,
    unsampled: u64,
}

#[derive(Default)]
struct Totals {
    files: u64,
    with_standing_chunk: u64,
    with_river_chunk: u64,
    with_wave_chunk: u64,
    standing_chunk_empty: u64,
    files_with_water: u64,
    standing_areas: u64,
    river_areas: u64,
    wave_areas: u64,
    river_lines: u64,
    points: u64,
    triangles: u64,
    vertices: u64,
    convex: u64,
    additive: u64,
    versions: BTreeMap<&'static str, BTreeMap<u16, u64>>,
    heights_per_file: BTreeMap<usize, u64>,
    height_range: Option<(u32, u32)>,
    non_empty_layer: u64,
    non_zero_reserved: u64,
    off_map_points: u64,
    worst_off_map: Option<(f32, String)>,
    backwards_triangles: u64,
    forced_clips: u64,
    dropped_triangles: u64,
    splits: u64,
    polygon_area: f64,
    triangle_area: f64,
    worst_area_error: Option<(f64, String)>,
    height_judged: u64,
    skipped: BTreeMap<WaterSkip, u64>,
    height_verdict: HeightVerdict,

    files_with_rivers: u64,
    river_drawn: u64,
    river_vertices: u64,
    river_triangles: u64,
    river_additive: u64,
    river_flipped: u64,
    river_backwards_triangles: u64,
    river_zero_width: u64,
    river_fallback_width: u64,
    river_skipped: BTreeMap<RiverSkip, u64>,
    river_section_range: Option<(usize, usize)>,
    river_triangle_area: f64,
    river_outline_area: f64,
    river_worst_area_error: Option<(f64, String)>,
    river_backwards_quads: u64,
    river_joins: u64,
    river_chain_joins: u64,
    river_perpendicular_joins: u64,
    river_widths: Vec<f32>,
    river_steps: Vec<f32>,
    river_first_over_mean: Vec<f32>,
    river_height_judged: u64,
    river_height_verdict: HeightVerdict,
    river_above_median: Vec<f32>,
    river_height_range: Option<(u32, u32)>,
    river_texture_uses: BTreeMap<String, (u64, u64)>,
    rivers_per_map: Vec<(u64, String)>,
    river_downhill: u64,
    river_uphill: u64,
    river_level: u64,
    river_drops: Vec<f32>,
    named: BTreeMap<&'static str, BTreeSet<String>>,
    fx_uses: BTreeMap<String, u64>,
    sky_uses: BTreeMap<String, u64>,
    scroll_speeds: BTreeMap<u32, u64>,
    per_map: Vec<(u64, String)>,
    failures: Vec<String>,
    unreadable: Vec<String>,
}

impl Totals {
    #[allow(clippy::too_many_lines)]
    fn add(&mut self, name: &str, map: &MapFile) {
        self.files = self.files.saturating_add(1);

        if let Some(chunk) = &map.water.standing {
            self.with_standing_chunk = self.with_standing_chunk.saturating_add(1);
            self.note_version("StandingWaterAreas", chunk.version);
            if chunk.areas.is_empty() {
                self.standing_chunk_empty = self.standing_chunk_empty.saturating_add(1);
            }
        }
        if let Some(chunk) = &map.water.rivers {
            self.with_river_chunk = self.with_river_chunk.saturating_add(1);
            self.note_version("RiverAreas", chunk.version);
        }
        if let Some(chunk) = &map.water.waves {
            self.with_wave_chunk = self.with_wave_chunk.saturating_add(1);
            self.note_version("StandingWaveAreas", chunk.version);
        }

        for river in map.water.river_areas() {
            self.river_areas = self.river_areas.saturating_add(1);
            for (slot, name) in [
                ("riverTexture", &river.river_texture),
                ("noiseTexture", &river.noise_texture),
                ("alphaEdgeTexture", &river.alpha_edge_texture),
                ("sparkleTexture", &river.sparkle_texture),
            ] {
                if !name.is_empty() {
                    self.named.entry(slot).or_default().insert(name.clone());
                }
            }
            self.river_lines = self
                .river_lines
                .saturating_add(u64::try_from(river.lines.len()).unwrap_or(0));
            if !river.layer_name.is_empty() {
                self.non_empty_layer = self.non_empty_layer.saturating_add(1);
            }
            if river.reserved != 0 {
                self.non_zero_reserved = self.non_zero_reserved.saturating_add(1);
            }
            let uses = self
                .river_texture_uses
                .entry(river.river_texture.clone())
                .or_insert((0, 0));
            uses.0 = uses.0.saturating_add(1);
            if river.additive_blend {
                uses.1 = uses.1.saturating_add(1);
            }
            self.note_river_height_range(river);
            self.note_river_geometry(river);
            self.note_river_height(map, river);
            self.note_river_flow(map, river);
        }
        self.build_rivers_for(name, map);
        for wave in map.water.wave_areas() {
            self.wave_areas = self.wave_areas.saturating_add(1);
            if !wave.wave_texture.is_empty() {
                self.named
                    .entry("waveTexture")
                    .or_default()
                    .insert(wave.wave_texture.clone());
            }
            if !wave.layer_name.is_empty() {
                self.non_empty_layer = self.non_empty_layer.saturating_add(1);
            }
            if wave.reserved != 0 {
                self.non_zero_reserved = self.non_zero_reserved.saturating_add(1);
            }
        }

        let areas = map.water.standing_areas();
        if areas.is_empty() {
            return;
        }
        self.files_with_water = self.files_with_water.saturating_add(1);
        self.per_map
            .push((u64::try_from(areas.len()).unwrap_or(0), name.to_owned()));

        let mut heights = BTreeSet::new();
        for area in areas {
            self.standing_areas = self.standing_areas.saturating_add(1);
            heights.insert(area.water_height);
            self.height_range = Some(match self.height_range {
                None => (area.water_height, area.water_height),
                Some((lo, hi)) => (lo.min(area.water_height), hi.max(area.water_height)),
            });
            if !area.layer_name.is_empty() {
                self.non_empty_layer = self.non_empty_layer.saturating_add(1);
            }
            for (slot, name) in [
                ("bumpTexture", &area.bump_texture),
                ("skyTexture", &area.sky_texture),
                ("fxShader", &area.fx_shader),
                ("depthColors", &area.depth_colors),
            ] {
                if !name.is_empty() {
                    let slot = self.named.entry(slot).or_default();
                    slot.insert(name.clone());
                }
            }
            let speed = self
                .scroll_speeds
                .entry(area.uv_scroll_speed.to_bits())
                .or_insert(0);
            *speed = speed.saturating_add(1);
            if !area.sky_texture.is_empty() {
                let uses = self.sky_uses.entry(area.sky_texture.clone()).or_insert(0);
                *uses = uses.saturating_add(1);
            }
            if !area.fx_shader.is_empty() {
                let uses = self.fx_uses.entry(area.fx_shader.clone()).or_insert(0);
                *uses = uses.saturating_add(1);
            }
            self.note_off_map(name, map, area);
            self.note_height(map, area);
        }
        *self.heights_per_file.entry(heights.len()).or_insert(0) += 1;

        let surface = build_standing_water(areas, WATER_UV_WORLD_SIZE, None);
        let stats = &surface.stats;
        self.points = self
            .points
            .saturating_add(u64::try_from(stats.points).unwrap_or(0));
        self.triangles = self
            .triangles
            .saturating_add(u64::try_from(stats.triangles).unwrap_or(0));
        self.vertices = self
            .vertices
            .saturating_add(u64::try_from(stats.vertices).unwrap_or(0));
        self.convex = self
            .convex
            .saturating_add(u64::try_from(stats.convex).unwrap_or(0));
        self.additive = self
            .additive
            .saturating_add(u64::try_from(stats.additive).unwrap_or(0));
        self.forced_clips = self
            .forced_clips
            .saturating_add(u64::try_from(stats.forced_clips).unwrap_or(0));
        self.dropped_triangles = self
            .dropped_triangles
            .saturating_add(u64::try_from(stats.dropped_triangles).unwrap_or(0));
        self.splits = self
            .splits
            .saturating_add(u64::try_from(stats.splits).unwrap_or(0));
        for (reason, count) in &stats.skipped {
            let slot = self.skipped.entry(*reason).or_insert(0);
            *slot = slot.saturating_add(u64::try_from(*count).unwrap_or(0));
        }

        for (body, area) in surface.bodies.iter().zip(areas) {
            let polygon = f64::from(shoelace(&area.points)) / 2.0;
            let mut covered = 0.0f64;
            for triangle in body.mesh.indices.chunks_exact(3) {
                let corner = |slot: usize| {
                    triangle
                        .get(slot)
                        .and_then(|index| usize::try_from(*index).ok())
                        .and_then(|index| body.mesh.vertices.get(index))
                        .map(|vertex| [vertex.position[0], vertex.position[1]])
                };
                if let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) {
                    let doubled = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
                    if doubled <= 0.0 {
                        self.backwards_triangles = self.backwards_triangles.saturating_add(1);
                    }
                    covered += f64::from(doubled) / 2.0;
                }
            }
            self.polygon_area += polygon.abs();
            self.triangle_area += covered;
            let error = (covered - polygon.abs()) / polygon.abs().max(1.0);
            if self
                .worst_area_error
                .as_ref()
                .is_none_or(|(worst, _)| error.abs() > worst.abs())
            {
                self.worst_area_error = Some((error, name.to_owned()));
            }
        }
    }

    fn report_texture_resolution(&self, vfs: &Vfs) {
        let resolver = Resolver::new();
        println!();
        println!("names the chunks carry, and whether they resolve to shipped bytes:");
        for (slot, names) in &self.named {
            let mut missing: Vec<&String> = Vec::new();
            let mut resolved = 0usize;
            for name in names {
                if resolver.resolve(vfs, name).is_ok() {
                    resolved = resolved.saturating_add(1);
                } else {
                    missing.push(name);
                }
            }
            println!("  {slot:<17} {resolved:>2} of {:>2} resolve", names.len());
            for name in missing {
                let uses = self.sky_uses.get(name).copied().unwrap_or(0);
                let used = if uses > 0 {
                    format!(", named by {uses} area(s)")
                } else {
                    String::new()
                };
                println!("    MISSING {name}{used}");
            }
        }
    }

    fn report_shadow_lightness(&self, vfs: &Vfs) {
        let resolver = Resolver::new();
        let mut values: BTreeMap<u32, (u64, u64, String)> = BTreeMap::new();
        let (mut files, mut without, mut unreadable) = (0u64, 0u64, 0u64);
        for (name, uses) in &self.fx_uses {
            files = files.saturating_add(1);
            let Some(bytes) = resolver
                .resolve(vfs, name)
                .ok()
                .and_then(|path| vfs.read_path(&path).ok())
            else {
                unreadable = unreadable.saturating_add(1);
                continue;
            };
            let Ok(model) = mint_sage_w3d::Model::parse(&bytes) else {
                unreadable = unreadable.saturating_add(1);
                continue;
            };
            let found = model
                .meshes
                .iter()
                .flat_map(|mesh| mesh.shader_materials.iter())
                .flat_map(|material| material.properties.iter())
                .find_map(|property| match (&property.value, property.name.as_str()) {
                    (mint_sage_w3d::ShaderMaterialValue::Float(value), n)
                        if n.eq_ignore_ascii_case("ShadowLightness") =>
                    {
                        Some(*value)
                    }
                    _ => None,
                });
            match found {
                Some(value) => {
                    let slot = values
                        .entry(value.to_bits())
                        .or_insert_with(|| (0, 0, name.clone()));
                    slot.0 = slot.0.saturating_add(1);
                    slot.1 = slot.1.saturating_add(*uses);
                }
                None => without = without.saturating_add(1),
            }
        }
        println!();
        println!(
            "ShadowLightness, over {files} distinct fxShader .w3d file(s):              {} author one, {without} do not, {unreadable} did not parse",
            files.saturating_sub(without).saturating_sub(unreadable),
        );
        for (bits, (count, uses, first)) in &values {
            let value = f32::from_bits(*bits);
            let range = if (0.0..=1.0).contains(&value) {
                ""
            } else {
                "   OUTSIDE [0, 1]"
            };
            println!("  {value:>6.3}   {count:>3} file(s), {uses:>4} area(s), e.g. {first}{range}");
        }
    }

    fn note_river_height_range(&mut self, river: &RiverArea) {
        self.river_height_range = Some(match self.river_height_range {
            None => (river.water_height, river.water_height),
            Some((lo, hi)) => (lo.min(river.water_height), hi.max(river.water_height)),
        });
    }

    #[allow(clippy::cast_precision_loss)]
    fn note_river_geometry(&mut self, river: &RiverArea) {
        for line in &river.lines {
            let width = mint_math::f32::hypot(line.b[0] - line.a[0], line.b[1] - line.a[1]);
            if width.is_finite() {
                self.river_widths.push(width);
            }
        }
        let mean = if river.lines.is_empty() {
            0.0
        } else {
            let sum: f32 = river
                .lines
                .iter()
                .map(|l| mint_math::f32::hypot(l.b[0] - l.a[0], l.b[1] - l.a[1]))
                .sum();
            sum / river.lines.len() as f32
        };
        if let Some(first) = river.lines.first() {
            let width = mint_math::f32::hypot(first.b[0] - first.a[0], first.b[1] - first.a[1]);
            if mean > 0.0 && width.is_finite() {
                self.river_first_over_mean.push(width / mean);
            }
        }
        for pair in river.lines.windows(2) {
            let (Some(first), Some(second)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            self.river_joins = self.river_joins.saturating_add(1);
            if first.b == second.a {
                self.river_chain_joins = self.river_chain_joins.saturating_add(1);
            }
            let mid0 = [
                (first.a[0] + first.b[0]) * 0.5,
                (first.a[1] + first.b[1]) * 0.5,
            ];
            let mid1 = [
                (second.a[0] + second.b[0]) * 0.5,
                (second.a[1] + second.b[1]) * 0.5,
            ];
            let step = [mid1[0] - mid0[0], mid1[1] - mid0[1]];
            let length = mint_math::f32::hypot(step[0], step[1]);
            if length.is_finite() {
                self.river_steps.push(length);
            }
            let across = [first.b[0] - first.a[0], first.b[1] - first.a[1]];
            let width = mint_math::f32::hypot(across[0], across[1]);
            if length <= 0.0 || width <= 0.0 {
                continue;
            }
            let along = (across[0] * step[0] + across[1] * step[1]) / (length * width);
            let perpendicular = (across[0] * step[1] - across[1] * step[0]) / (length * width);
            if perpendicular.abs() > along.abs() {
                self.river_perpendicular_joins = self.river_perpendicular_joins.saturating_add(1);
            }
        }
    }

    fn note_river_height(&mut self, map: &MapFile, river: &RiverArea) {
        if river.lines.len() < 2 {
            return;
        }
        let mut heights: Vec<f32> = Vec::new();
        for line in &river.lines {
            for step in 0..=4u16 {
                let t = f32::from(step) / 4.0;
                let x = line.a[0] + (line.b[0] - line.a[0]) * t;
                let y = line.a[1] + (line.b[1] - line.a[1]) * t;
                if let Some(h) = surface_height(&map.height_map, VERTICAL_SCALE, x, y) {
                    heights.push(h);
                }
            }
        }
        if heights.is_empty() {
            self.river_height_verdict.unsampled =
                self.river_height_verdict.unsampled.saturating_add(1);
            return;
        }
        heights.sort_by(f32::total_cmp);
        let lo = heights.first().copied().unwrap_or(0.0);
        let hi = heights.last().copied().unwrap_or(0.0);
        let median = heights.get(heights.len() / 2).copied().unwrap_or(0.0);
        self.river_height_judged = self.river_height_judged.saturating_add(1);
        let world = height_as_f32(river.water_height);
        let raw = world * VERTICAL_SCALE;
        if (lo..=hi).contains(&world) {
            self.river_height_verdict.world = self.river_height_verdict.world.saturating_add(1);
        }
        if (lo..=hi).contains(&raw) {
            self.river_height_verdict.samples = self.river_height_verdict.samples.saturating_add(1);
        }
        self.river_above_median.push(world - median);
    }

    fn note_river_flow(&mut self, map: &MapFile, river: &RiverArea) {
        let (Some(first), Some(last)) = (river.lines.first(), river.lines.last()) else {
            return;
        };
        let mid = |line: &mint_sage_map::RiverLine| {
            [(line.a[0] + line.b[0]) * 0.5, (line.a[1] + line.b[1]) * 0.5]
        };
        let (start, end) = (mid(first), mid(last));
        let (Some(top), Some(bottom)) = (
            surface_height(&map.height_map, VERTICAL_SCALE, start[0], start[1]),
            surface_height(&map.height_map, VERTICAL_SCALE, end[0], end[1]),
        ) else {
            return;
        };
        let drop = top - bottom;
        if drop.abs() < mint_sage_map::MAP_XY_FACTOR {
            self.river_level = self.river_level.saturating_add(1);
            return;
        }
        self.river_drops.push(drop);
        if drop > 0.0 {
            self.river_downhill = self.river_downhill.saturating_add(1);
        } else {
            self.river_uphill = self.river_uphill.saturating_add(1);
        }
    }

    fn build_rivers_for(&mut self, name: &str, map: &MapFile) {
        let areas = map.water.river_areas();
        if areas.is_empty() {
            return;
        }
        self.files_with_rivers = self.files_with_rivers.saturating_add(1);
        self.rivers_per_map
            .push((u64::try_from(areas.len()).unwrap_or(0), name.to_owned()));
        for area in areas {
            let count = area.lines.len();
            self.river_section_range = Some(match self.river_section_range {
                None => (count, count),
                Some((lo, hi)) => (lo.min(count), hi.max(count)),
            });
        }
        let network = build_rivers(areas);
        let stats = &network.stats;
        self.river_drawn = self
            .river_drawn
            .saturating_add(u64::try_from(stats.drawn).unwrap_or(0));
        self.river_vertices = self
            .river_vertices
            .saturating_add(u64::try_from(stats.vertices).unwrap_or(0));
        self.river_triangles = self
            .river_triangles
            .saturating_add(u64::try_from(stats.triangles).unwrap_or(0));
        self.river_additive = self
            .river_additive
            .saturating_add(u64::try_from(stats.additive).unwrap_or(0));
        self.river_flipped = self
            .river_flipped
            .saturating_add(u64::try_from(stats.flipped).unwrap_or(0));
        self.river_backwards_triangles = self
            .river_backwards_triangles
            .saturating_add(u64::try_from(stats.backwards_triangles).unwrap_or(0));
        self.river_zero_width = self
            .river_zero_width
            .saturating_add(u64::try_from(stats.zero_width_sections).unwrap_or(0));
        self.river_fallback_width = self
            .river_fallback_width
            .saturating_add(u64::try_from(stats.fallback_width).unwrap_or(0));
        for (reason, count) in &stats.skipped {
            let slot = self.river_skipped.entry(*reason).or_insert(0);
            *slot = slot.saturating_add(u64::try_from(*count).unwrap_or(0));
        }

        for area in areas {
            let single = build_rivers(std::slice::from_ref(area));
            let Some(ribbon) = single.ribbons.first() else {
                continue;
            };
            let mut outline: Vec<[f32; 2]> = area.lines.iter().map(|line| line.a).collect();
            outline.extend(area.lines.iter().rev().map(|line| line.b));
            let expected = f64::from(shoelace(&outline)) / 2.0;
            let mut covered = 0.0f64;
            for triangle in ribbon.mesh.indices.chunks_exact(3) {
                let corner = |slot: usize| {
                    triangle
                        .get(slot)
                        .and_then(|index| usize::try_from(*index).ok())
                        .and_then(|index| ribbon.mesh.vertices.get(index))
                        .map(|vertex| [vertex.position[0], vertex.position[1]])
                };
                if let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) {
                    let doubled = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
                    if doubled < 0.0 {
                        self.river_backwards_quads = self.river_backwards_quads.saturating_add(1);
                    }
                    covered += f64::from(doubled) / 2.0;
                }
            }
            self.river_outline_area += expected.abs();
            self.river_triangle_area += covered;
            let error = (covered - expected.abs()) / expected.abs().max(1.0);
            if self
                .river_worst_area_error
                .as_ref()
                .is_none_or(|(worst, _)| error.abs() > worst.abs())
            {
                self.river_worst_area_error = Some((error, name.to_owned()));
            }
        }
    }

    fn note_version(&mut self, chunk: &'static str, version: u16) {
        let slot = self
            .versions
            .entry(chunk)
            .or_default()
            .entry(version)
            .or_insert(0);
        *slot = slot.saturating_add(1);
    }

    fn note_off_map(&mut self, name: &str, map: &MapFile, area: &StandingWaterArea) {
        let width = f32::from(u16::try_from(map.height_map.width).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(map.height_map.height).unwrap_or(u16::MAX));
        let border = f32::from(u16::try_from(map.height_map.border_size).unwrap_or(0));
        let factor = mint_sage_map::MAP_XY_FACTOR;
        let (x0, x1) = (-border * factor, (width - 1.0 - border) * factor);
        let (y0, y1) = (-border * factor, (height - 1.0 - border) * factor);
        for point in &area.points {
            let out = (x0 - point[0])
                .max(point[0] - x1)
                .max(y0 - point[1])
                .max(point[1] - y1);
            if out > 0.0 {
                self.off_map_points = self.off_map_points.saturating_add(1);
                if self
                    .worst_off_map
                    .as_ref()
                    .is_none_or(|(worst, _)| out > *worst)
                {
                    self.worst_off_map = Some((out, name.to_owned()));
                }
            }
        }
    }

    fn note_height(&mut self, map: &MapFile, area: &StandingWaterArea) {
        if area.name.eq_ignore_ascii_case("Default Water") {
            return;
        }
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (
            extreme(&area.points, 0, false),
            extreme(&area.points, 0, true),
            extreme(&area.points, 1, false),
            extreme(&area.points, 1, true),
        ) else {
            self.height_verdict.unsampled = self.height_verdict.unsampled.saturating_add(1);
            return;
        };
        for iy in 0..PROBE_STEPS {
            for ix in 0..PROBE_STEPS {
                let fx = f32::from(u16::try_from(ix).unwrap_or(0))
                    / f32::from(u16::try_from(PROBE_STEPS.saturating_sub(1)).unwrap_or(1));
                let fy = f32::from(u16::try_from(iy).unwrap_or(0))
                    / f32::from(u16::try_from(PROBE_STEPS.saturating_sub(1)).unwrap_or(1));
                let x = min_x + (max_x - min_x) * fx;
                let y = min_y + (max_y - min_y) * fy;
                if !point_in_polygon(&area.points, x, y) {
                    continue;
                }
                if let Some(h) = surface_height(&map.height_map, VERTICAL_SCALE, x, y) {
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
        }
        if !lo.is_finite() || !hi.is_finite() {
            self.height_verdict.unsampled = self.height_verdict.unsampled.saturating_add(1);
            return;
        }
        self.height_judged = self.height_judged.saturating_add(1);
        let world = height_as_f32(area.water_height);
        let raw = world * VERTICAL_SCALE;
        if (lo..=hi).contains(&world) {
            self.height_verdict.world = self.height_verdict.world.saturating_add(1);
        }
        if (lo..=hi).contains(&raw) {
            self.height_verdict.samples = self.height_verdict.samples.saturating_add(1);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn report_rivers(&self) {
        println!();
        println!(
            "rivers: {} area(s) in {} file(s), {} drawn, {} skipped",
            self.river_areas,
            self.files_with_rivers,
            self.river_drawn,
            self.river_skipped.values().sum::<u64>()
        );
        for (reason, count) in &self.river_skipped {
            println!("  skipped {count:>5}  {reason:?}");
        }
        if let Some((lo, hi)) = self.river_section_range {
            println!(
                "  {} cross-section(s), {lo} to {hi} per area -> {} vertices, {} triangles",
                self.river_lines, self.river_vertices, self.river_triangles
            );
        }
        println!(
            "  {} additive of {} drawn - the majority, unlike standing water",
            self.river_additive, self.river_drawn
        );
        println!(
            "  {} area(s) had their bank order swapped to wind the strip forwards; \
             {} backwards triangle(s), {} zero-width station(s), {} width fallback(s)",
            self.river_flipped,
            self.river_backwards_triangles,
            self.river_zero_width,
            self.river_fallback_width
        );
        let mut textures: Vec<(&String, &(u64, u64))> = self.river_texture_uses.iter().collect();
        textures.sort_by(|a, b| b.1.cmp(a.1));
        println!("  riverTexture, and how many of each are additive:");
        for (name, (count, additive)) in textures {
            println!(
                "    {name:<24} {count:>4} area(s), {additive:>4} additive, \
                 {:>4} alpha",
                count.saturating_sub(*additive)
            );
        }

        println!();
        println!("river cross-checks:");
        println!(
            "  triangles wound forwards        {} backwards",
            self.river_backwards_quads
        );
        let error = (self.river_triangle_area - self.river_outline_area).abs()
            / self.river_outline_area.max(1.0)
            * 100.0;
        println!(
            "  strip covers the banks outline  {:.0} of {:.0} world units squared ({error:.4}% out)",
            self.river_triangle_area, self.river_outline_area
        );
        if let Some((worst, map)) = &self.river_worst_area_error {
            println!(
                "    worst single area {:+.4}% (positive covers more than the outline), on {map}",
                worst * 100.0
            );
        }
        println!(
            "  cross-sections, not a chain     chain reading {} of {} join(s), \
             cross-section reading {} of {}",
            self.river_chain_joins,
            self.river_joins,
            self.river_perpendicular_joins,
            self.river_joins
        );
        let verdict = &self.river_height_verdict;
        println!(
            "  waterHeight as WORLD units      {} of {} river(s) land inside their own terrain range",
            verdict.world, self.river_height_judged
        );
        println!(
            "  waterHeight as raw samples      {} of {} - the reading this refutes",
            verdict.samples, self.river_height_judged
        );
        if verdict.unsampled > 0 {
            println!("  ({} river(s) could not be sampled)", verdict.unsampled);
        }
        if let Some((lo, hi)) = self.river_height_range {
            println!("  river heights: {lo} to {hi} world units");
        }
        let sloped = self.river_downhill.saturating_add(self.river_uphill);
        println!(
            "  station order runs downhill     {} of {} sloped river(s) ({} level, \
             within one terrain cell)",
            self.river_downhill, sloped, self.river_level
        );
        if let Some(drop) = median_of(&self.river_drops) {
            println!("    median end-to-end drop {drop:+.0} world units");
        }
        if let Some(median) = median_of(&self.river_above_median) {
            println!("  surface minus the median ground under it: median {median:+.1} world units");
        }
        if let (Some(width), Some(step)) =
            (median_of(&self.river_widths), median_of(&self.river_steps))
        {
            println!(
                "  median cross-section width {width:.1}, median station step {step:.1} world units"
            );
        }
        if let Some(ratio) = median_of(&self.river_first_over_mean) {
            let spread = self
                .river_first_over_mean
                .iter()
                .filter(|r| (0.5..=2.0).contains(*r))
                .count();
            println!(
                "  first cross-section width over the area mean: median {ratio:.2}, \
                 {spread} of {} area(s) within a factor of two",
                self.river_first_over_mean.len()
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    fn report(&self, seconds: f64) {
        println!();
        println!(
            "{} map file(s) walked in {seconds:.1} s, {} parse failure(s)",
            self.files,
            self.failures.len()
        );
        println!();
        println!("chunks present:");
        println!(
            "  StandingWaterAreas  {:>4} file(s), {:>4} of them holding a count of zero",
            self.with_standing_chunk, self.standing_chunk_empty
        );
        println!("  RiverAreas          {:>4} file(s)", self.with_river_chunk);
        println!("  StandingWaveAreas   {:>4} file(s)", self.with_wave_chunk);
        for (chunk, versions) in &self.versions {
            let listed: Vec<String> = versions
                .iter()
                .map(|(version, count)| format!("v{version} x{count}"))
                .collect();
            println!("  {chunk:<20}{}", listed.join(", "));
        }

        println!();
        println!(
            "areas: {} standing (in {} file(s)), {} river ({} cross-section(s)), {} wave - \
             the last one parsed, not drawn",
            self.standing_areas,
            self.files_with_water,
            self.river_areas,
            self.river_lines,
            self.wave_areas
        );
        println!(
            "geometry: {} point(s) -> {} vertices, {} triangles; {} convex polygon(s), \
             {} additive",
            self.points, self.vertices, self.triangles, self.convex, self.additive
        );
        if let Some((lo, hi)) = self.height_range {
            println!("water heights: {lo} to {hi} world units");
        }
        let listed: Vec<String> = self
            .heights_per_file
            .iter()
            .map(|(distinct, files)| format!("{files} file(s) at {distinct}"))
            .collect();
        println!("distinct heights per file: {}", listed.join(", "));

        let mut speeds: Vec<(f32, u64)> = self
            .scroll_speeds
            .iter()
            .map(|(bits, count)| (f32::from_bits(*bits), *count))
            .collect();
        speeds.sort_by(|a, b| a.0.total_cmp(&b.0));
        let still: u64 = speeds
            .iter()
            .filter(|(speed, _)| *speed == 0.0)
            .map(|(_, count)| *count)
            .sum();
        let listed: Vec<String> = speeds
            .iter()
            .map(|(speed, count)| format!("{speed}x{count}"))
            .collect();
        println!(
            "uvScrollSpeed: {} distinct value(s) - {}; {still} area(s) stand still",
            speeds.len(),
            listed.join(", ")
        );

        println!();
        println!("cross-checks (each of these would fail if a field were misread):");
        println!(
            "  layerName empty                 {} area(s) disagree",
            self.non_empty_layer
        );
        println!(
            "  reserved fields zero            {} area(s) disagree",
            self.non_zero_reserved
        );
        println!(
            "  triangles wound forwards        {} backwards or degenerate, {} polygon(s) not \
             cleanly clipped, {} sliver(s) dropped, {} pinch split(s)",
            self.backwards_triangles, self.forced_clips, self.dropped_triangles, self.splits
        );
        let error =
            (self.triangle_area - self.polygon_area).abs() / self.polygon_area.max(1.0) * 100.0;
        println!(
            "  triangles cover the polygon      {:.0} of {:.0} world units squared ({error:.4}% out)",
            self.triangle_area, self.polygon_area
        );
        if let Some((worst, map)) = &self.worst_area_error {
            println!(
                "    worst single area {:+.4}% (positive covers more than the polygon), on {map}",
                worst * 100.0
            );
        }
        let verdict = &self.height_verdict;
        println!(
            "  waterHeight as WORLD units      {} of {} area(s) land inside their own terrain range",
            verdict.world, self.height_judged
        );
        println!(
            "  waterHeight as raw samples      {} of {} - the reading this refutes",
            verdict.samples, self.height_judged
        );
        if verdict.unsampled > 0 {
            println!(
                "  ({} area(s) could not be sampled at all)",
                verdict.unsampled
            );
        }

        if self.off_map_points > 0 {
            print!(
                "\n{} polygon vertex/vertices lie outside their map's world rectangle",
                self.off_map_points
            );
            match &self.worst_off_map {
                Some((worst, name)) => println!(" (worst {worst:.0} units, on {name})"),
                None => println!(),
            }
        }

        if !self.skipped.is_empty() {
            println!();
            for (reason, count) in &self.skipped {
                println!("SKIPPED {count} area(s): {reason:?}");
            }
        }

        self.report_rivers();

        let mut busiest = self.per_map.clone();
        busiest.sort_by(|a, b| b.cmp(a));
        println!();
        println!("maps with the most standing-water areas:");
        for (count, name) in busiest.iter().take(8) {
            println!("  {count:>4}  {name}");
        }
        let mut busiest = self.rivers_per_map.clone();
        busiest.sort_by(|a, b| b.cmp(a));
        println!();
        println!("maps with the most river areas:");
        for (count, name) in busiest.iter().take(8) {
            println!("  {count:>4}  {name}");
        }

        for failure in &self.failures {
            println!("PARSE FAILURE {failure}");
        }
        for name in &self.unreadable {
            println!("UNREADABLE {name}");
        }
    }
}

fn median_of(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted.get(sorted.len() / 2).copied()
}

fn shoelace(points: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0f32;
    let last = points.len().saturating_sub(1);
    let mut previous = points.get(last).copied().unwrap_or([0.0, 0.0]);
    for point in points {
        sum += previous[0] * point[1] - point[0] * previous[1];
        previous = *point;
    }
    sum
}

#[allow(clippy::cast_precision_loss)]
fn height_as_f32(water_height: u32) -> f32 {
    water_height as f32
}

fn extreme(points: &[[f32; 2]], axis: usize, largest: bool) -> Option<f32> {
    let mut best: Option<f32> = None;
    for point in points {
        let value = *point.get(axis)?;
        if !value.is_finite() {
            return None;
        }
        best = Some(match best {
            None => value,
            Some(current) if largest => current.max(value),
            Some(current) => current.min(value),
        });
    }
    best
}

fn point_in_polygon(points: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut inside = false;
    let last = points.len().saturating_sub(1);
    let mut previous = match points.get(last) {
        Some(point) => *point,
        None => return false,
    };
    for point in points {
        if (point[1] > y) != (previous[1] > y) {
            let span = previous[1] - point[1];
            if span != 0.0 {
                let cut = point[0] + (y - point[1]) / span * (previous[0] - point[0]);
                if x < cut {
                    inside = !inside;
                }
            }
        }
        previous = *point;
    }
    inside
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
