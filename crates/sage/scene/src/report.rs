use std::collections::BTreeMap;
use std::time::Duration;

use mint_render::TextureData;
use mint_sage_map::{BlendTileData, HeightMapData, MapFile};
use mint_sage_terrain::{
    ObjectProxy, RiverNetwork, RiverSkip, Terrain, TerrainTypes, TileLayout, WaterSkip,
    WaterSurface, WaveField, WaveLook, WaveSkip, WaveSurface, MAX_SURFACE_WAVES, WAVE_ALPHA_GAIN,
    WAVE_BEACH_BAND, WAVE_CREST_HEIGHT, WAVE_VELOCITY_UNIT,
};

#[derive(Debug, Clone)]
pub struct ClassNote {
    pub name: String,
    pub file: Option<String>,
    pub loaded: bool,
}

pub(crate) fn report(
    name: &str,
    height_map: &HeightMapData,
    terrain: &Terrain,
    scale: f32,
    build: Duration,
) {
    let range = height_map
        .samples
        .iter()
        .copied()
        .fold(None, |range: Option<(u16, u16)>, s| {
            Some(range.map_or((s, s), |(lo, hi)| (lo.min(s), hi.max(s))))
        });

    println!("{name}");
    println!(
        "  {}x{} cells, border {}, {} boundary rectangle(s)",
        height_map.width,
        height_map.height,
        height_map.border_size,
        height_map.boundaries.len()
    );
    println!(
        "  {} chunk(s), {} part(s) (one draw call each), {} triangle(s)",
        terrain.chunks.len(),
        terrain.part_count(),
        terrain.triangle_count()
    );
    match range {
        Some((min, max)) => println!("  raw sample range: {min} .. {max}"),
        None => println!("  no samples (this file has no terrain)"),
    }
    println!("  starting vertical scale: {scale}");
    println!("  built in {:.0} ms", build.as_secs_f64() * 1000.0);
}

pub(crate) fn report_cliffs(height_map: &HeightMapData, blend_tile: &BlendTileData) {
    let used = blend_tile
        .cliff_info_index
        .iter()
        .filter(|value| **value != 0)
        .count();
    println!(
        "  {} cliff record(s), {used} cell(s) using one",
        blend_tile.cliff_records.len()
    );
    let mut tallest: Vec<(u16, u32, u32)> = Vec::new();
    for (index, value) in blend_tile.cliff_info_index.iter().enumerate() {
        if *value == 0 || blend_tile.width == 0 {
            continue;
        }
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let (x, y) = (index % blend_tile.width, index / blend_tile.width);
        let mut range: Option<(u16, u16)> = None;
        for (dx, dy) in [(0u32, 0u32), (1, 0), (0, 1), (1, 1)] {
            let sample = height_map
                .sample(x.saturating_add(dx), y.saturating_add(dy))
                .unwrap_or(0);
            range = Some(range.map_or((sample, sample), |(lo, hi)| {
                (lo.min(sample), hi.max(sample))
            }));
        }
        if let Some((lo, hi)) = range {
            tallest.push((hi.saturating_sub(lo), x, y));
        }
    }
    tallest.sort_by(|a, b| b.cmp(a));
    for (drop, x, y) in tallest.iter().take(3) {
        println!("    tallest cliff cell ({x},{y}), {drop} raw units across the quad");
    }
    let block = 16u32;
    let mut blocks: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (index, value) in blend_tile.cliff_info_index.iter().enumerate() {
        if *value == 0 || blend_tile.width == 0 {
            continue;
        }
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let (x, y) = (index % blend_tile.width, index / blend_tile.width);
        *blocks.entry((x / block, y / block)).or_default() += 1;
    }
    let mut densest: Vec<((u32, u32), u32)> = blocks.into_iter().collect();
    densest.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for ((bx, by), count) in densest.iter().take(3) {
        println!(
            "    densest {block}x{block} block at ({},{}) holds {count} cliff cell(s)",
            bx * block + block / 2,
            by * block + block / 2
        );
    }

    let mut with_overlay: Vec<(u16, u32, u32)> = Vec::new();
    let mut overlay_blocks: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (index, value) in blend_tile.cliff_info_index.iter().enumerate() {
        if *value == 0 || blend_tile.width == 0 {
            continue;
        }
        let blended = [&blend_tile.blend_index, &blend_tile.extra_blend_index]
            .into_iter()
            .any(|array| array.get(index).is_some_and(|value| *value != 0));
        if !blended {
            continue;
        }
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let (x, y) = (index % blend_tile.width, index / blend_tile.width);
        let mut range: Option<(u16, u16)> = None;
        for (dx, dy) in [(0u32, 0u32), (1, 0), (0, 1), (1, 1)] {
            let sample = height_map
                .sample(x.saturating_add(dx), y.saturating_add(dy))
                .unwrap_or(0);
            range = Some(range.map_or((sample, sample), |(lo, hi)| {
                (lo.min(sample), hi.max(sample))
            }));
        }
        if let Some((lo, hi)) = range {
            with_overlay.push((hi.saturating_sub(lo), x, y));
        }
        *overlay_blocks.entry((x / block, y / block)).or_default() += 1;
    }
    println!(
        "    {} of those cell(s) also carry a blend - suppressed where the record \
         applies, placed where the gate rejects it",
        with_overlay.len()
    );
    with_overlay.sort_by(|a, b| b.cmp(a));
    for (drop, x, y) in with_overlay.iter().take(3) {
        println!("      tallest at ({x},{y}), {drop} raw units across the quad");
    }
    let mut densest_overlay: Vec<((u32, u32), u32)> = overlay_blocks.into_iter().collect();
    densest_overlay.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for ((bx, by), count) in densest_overlay.iter().take(3) {
        println!(
            "      densest {block}x{block} block at ({},{}) holds {count} of them",
            bx * block + block / 2,
            by * block + block / 2
        );
    }
}

pub(crate) fn print_class_census(
    blend_tile: &BlendTileData,
    types: &TerrainTypes,
    loaded: &mint_sage_terrain::ClassTextures,
) {
    let layout = TileLayout::new(&blend_tile.texture_classes);
    let mut cells = vec![0u64; blend_tile.texture_classes.len()];
    let mut first: Vec<Option<(u32, u32)>> = vec![None; blend_tile.texture_classes.len()];
    let mut unclassed = 0u64;
    let mut first_unclassed: Option<(u32, u32)> = None;
    for (index, value) in blend_tile.tile_index.iter().enumerate() {
        let at = u32::try_from(index).unwrap_or(u32::MAX);
        let cell = (at % blend_tile.width.max(1), at / blend_tile.width.max(1));
        match layout.cell(*value).map(|tile| tile.class) {
            Some(class) => {
                if let Some(count) = cells.get_mut(class) {
                    *count = count.saturating_add(1);
                }
                if let Some(slot) = first.get_mut(class) {
                    slot.get_or_insert(cell);
                }
            }
            None => {
                unclassed = unclassed.saturating_add(1);
                first_unclassed.get_or_insert(cell);
            }
        }
    }
    println!(
        "edge tiles: {}, edge texture classes: {} {:?}",
        blend_tile.num_edge_tiles,
        blend_tile.edge_texture_classes.len(),
        blend_tile
            .edge_texture_classes
            .iter()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>()
    );
    println!("texture classes, by cells drawing them:");
    let mut order: Vec<usize> = (0..blend_tile.texture_classes.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(cells.get(*index).copied().unwrap_or(0)));
    for index in order {
        let Some(class) = blend_tile.texture_classes.get(index) else {
            continue;
        };
        let texture = loaded.textures.get(index).and_then(Option::as_ref);
        println!(
            "  class {index:>2}  tiles {:>5}..{:<5} w{:<3} {:>8} cell(s)  first {:>12}  mean rgb {}  {:?} -> {}",
            class.first_tile,
            class.first_tile.saturating_add(class.num_tiles),
            class.width,
            cells.get(index).copied().unwrap_or(0),
            first
                .get(index)
                .copied()
                .flatten()
                .map_or_else(|| "-".to_owned(), |(x, y)| format!("{x},{y}")),
            texture.map_or_else(|| "!! TEXTURE DID NOT LOAD !!".to_owned(), mean_rgb),
            class.name,
            types.texture(&class.name).unwrap_or("!! not in terrain.ini !!")
        );
    }
    report_boundary_coverage(blend_tile, &layout);
    if unclassed > 0 {
        println!(
            "  !! {unclassed} cell(s) name no class at all and draw UNTEXTURED !!  first {}",
            first_unclassed.map_or_else(|| "-".to_owned(), |(x, y)| format!("{x},{y}"))
        );
    }
}

fn report_boundary_coverage(blend_tile: &BlendTileData, layout: &TileLayout) {
    let class_at = |x: u32, y: u32| -> Option<usize> {
        let index =
            usize::try_from(u64::from(y) * u64::from(blend_tile.width) + u64::from(x)).ok()?;
        layout
            .cell(blend_tile.tile_index.get(index).copied()?)
            .map(|tile| tile.class)
    };
    let blended = |x: u32, y: u32| -> bool {
        usize::try_from(u64::from(y) * u64::from(blend_tile.width) + u64::from(x))
            .ok()
            .is_some_and(|index| {
                blend_tile.blend_index.get(index).copied().unwrap_or(0) != 0
                    || blend_tile
                        .extra_blend_index
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        != 0
            })
    };
    let (mut boundaries, mut covered) = (0u64, 0u64);
    let mut bare: BTreeMap<usize, u64> = BTreeMap::new();
    for y in 0..blend_tile.height {
        for x in 0..blend_tile.width {
            for (dx, dy) in [(1u32, 0u32), (0, 1)] {
                let (nx, ny) = (x.saturating_add(dx), y.saturating_add(dy));
                if nx >= blend_tile.width || ny >= blend_tile.height {
                    continue;
                }
                if class_at(x, y) == class_at(nx, ny) {
                    continue;
                }
                boundaries = boundaries.saturating_add(1);
                if blended(x, y) || blended(nx, ny) {
                    covered = covered.saturating_add(1);
                } else {
                    for class in [class_at(x, y), class_at(nx, ny)].into_iter().flatten() {
                        *bare.entry(class).or_default() += 1;
                    }
                }
            }
        }
    }
    let share = if boundaries == 0 {
        0.0
    } else {
        (covered as f64) * 100.0 / (boundaries as f64)
    };
    println!(
        "class boundaries: {boundaries} adjacent pairs of different classes, {covered} ({share:.1}%) with a blend overlay on at least one side"
    );
    let mut worst: Vec<(usize, u64)> = bare.into_iter().collect();
    worst.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let top: Vec<String> = worst
        .into_iter()
        .take(6)
        .map(|(class, count)| format!("class {class} x{count}"))
        .collect();
    println!("  bare boundaries lean on: {}", top.join(", "));
}

fn mean_rgb(texture: &TextureData) -> String {
    let Some(level) = texture.levels.first() else {
        return "no levels".to_owned();
    };
    let mut sums = [0u64; 3];
    let mut pixels = 0u64;
    for chunk in level.data.chunks_exact(4) {
        for (sum, byte) in sums.iter_mut().zip(chunk.iter()) {
            *sum = sum.saturating_add(u64::from(*byte));
        }
        pixels = pixels.saturating_add(1);
    }
    if pixels == 0 {
        return "empty".to_owned();
    }
    format!(
        "{:>3},{:>3},{:>3}",
        sums[0] / pixels,
        sums[1] / pixels,
        sums[2] / pixels
    )
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn object_cell(world: f32, border: u32) -> u32 {
    let grid = world / mint_sage_map::MAP_XY_FACTOR + f32::from(u16::try_from(border).unwrap_or(0));
    if grid.is_finite() && grid >= 0.0 && grid < f32::from(u16::MAX) {
        grid as u32
    } else {
        0
    }
}

pub(crate) fn report_objects(
    proxies: &[ObjectProxy],
    drawn: bool,
    markers: bool,
    animation_label: &str,
    border: u32,
    height_map: &HeightMapData,
    scale: f32,
) {
    let off_map = proxies.iter().filter(|p| p.ground.is_none()).count();
    let aligned = proxies.iter().filter(|p| p.align_to_terrain).count();
    let waypoints = proxies.iter().filter(|p| p.waypoint).count();
    println!();
    println!("objects: {} placement(s)", proxies.len());
    println!(
        "  {off_map} off the heightmap, {aligned} asking for alignToTerrain, {waypoints} waypoint(s)"
    );
    println!(
        "  objects {}, drawn as {} (p), animation = {} (a, --object-animation)",
        if drawn { "on (o)" } else { "off (o)" },
        if markers { "proxy markers" } else { "models" },
        animation_label
    );

    let mut steepest: Vec<(f32, u32, u32)> = proxies
        .iter()
        .filter(|proxy| proxy.align_to_terrain)
        .filter_map(|proxy| {
            let normal = mint_sage_terrain::surface_normal(
                height_map,
                scale,
                proxy.position.x,
                proxy.position.y,
            )?;
            Some((
                mint_math::f32::acos(normal.z.clamp(-1.0, 1.0)).to_degrees(),
                object_cell(proxy.position.x, border),
                object_cell(proxy.position.y, border),
            ))
        })
        .collect();
    steepest.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (degrees, x, y) in steepest.iter().take(3) {
        println!("  steepest ground under an aligned object: {degrees:.1} deg at cell ({x},{y})");
    }

    let block = 16u32;
    let mut blocks: BTreeMap<(u32, u32), (u32, f32)> = BTreeMap::new();
    for proxy in proxies {
        let key = (
            object_cell(proxy.position.x, border) / block,
            object_cell(proxy.position.y, border) / block,
        );
        let slot = blocks.entry(key).or_insert((0, 0.0));
        slot.0 = slot.0.saturating_add(1);
        slot.1 += proxy.angle;
    }
    let mut densest: Vec<((u32, u32), (u32, f32))> = blocks.into_iter().collect();
    densest.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
    for ((bx, by), (count, angle_sum)) in densest.iter().take(4) {
        println!(
            "  densest {block}x{block} block at cell ({},{}) holds {count}, mean angle {:.3} rad",
            bx.saturating_mul(block).saturating_add(block / 2),
            by.saturating_mul(block).saturating_add(block / 2),
            angle_sum / (*count).max(1) as f32
        );
    }
}

pub(crate) fn report_water(
    map: &MapFile,
    surface: &WaterSurface,
    mode_label: &str,
    mode_on: bool,
    elapsed: Duration,
) {
    let present = |chunk: Option<usize>| match chunk {
        None => "absent".to_string(),
        Some(count) => format!("{count} area(s)"),
    };
    println!();
    println!(
        "water: StandingWaterAreas {}, RiverAreas {}, StandingWaveAreas {}",
        present(map.water.standing.as_ref().map(|c| c.areas.len())),
        present(map.water.rivers.as_ref().map(|c| c.areas.len())),
        present(map.water.waves.as_ref().map(|c| c.areas.len())),
    );
    println!("  all three chunks are drawn; the waves have their own report below");
    let stats = &surface.stats;
    println!(
        "  {} standing area(s), {} drawn, {} skipped, in {:.1} ms",
        stats.areas,
        stats.drawn,
        stats.skipped_total(),
        elapsed.as_secs_f64() * 1000.0
    );
    for (reason, count) in &stats.skipped {
        let why = match reason {
            WaterSkip::TooFewPoints => "fewer than three points",
            WaterSkip::TooManyPoints => "more points than the triangulator will take",
            WaterSkip::NonFinite => "a coordinate or the height is not finite",
            WaterSkip::Degenerate => "the polygon encloses no area",
        };
        println!("  skipped {count:>5}  {why}");
    }
    if stats.drawn > 0 {
        println!(
            "  {} point(s) -> {} vertices, {} triangles; {} convex, {} additive, \
             {} split, {} sliver(s) dropped, {} forced",
            stats.points,
            stats.vertices,
            stats.triangles,
            stats.convex,
            stats.additive,
            stats.splits,
            stats.dropped_triangles,
            stats.forced_clips
        );
        let mut heights: Vec<String> = surface
            .bodies
            .iter()
            .map(|body| format!("{:.0}", body.height))
            .collect();
        heights.sort();
        heights.dedup();
        println!("  height(s), world units: {}", heights.join(", "));
        for body in &surface.bodies {
            println!(
                "    #{:<4} {:<20} z {:>5.0}  scroll {:<6} {:<24} {:<20} {}{}",
                body.unique_id,
                body.name,
                body.height,
                body.uv_scroll_speed,
                body.sky_texture,
                body.bump_texture,
                if body.fx_shader.is_empty() {
                    "(no fxShader)"
                } else {
                    &body.fx_shader
                },
                if body.additive_blend {
                    "  additive"
                } else {
                    ""
                },
            );
        }
    }
    println!(
        "  water = {mode_label} ({})",
        if mode_on { "drawn" } else { "not drawn" }
    );
}

pub(crate) fn report_rivers(
    network: &RiverNetwork,
    mode_label: &str,
    mode_on: bool,
    elapsed: Duration,
) {
    let stats = &network.stats;
    println!();
    println!(
        "  {} river area(s), {} drawn, {} skipped, in {:.1} ms",
        stats.areas,
        stats.drawn,
        stats.skipped_total(),
        elapsed.as_secs_f64() * 1000.0
    );
    for (reason, count) in &stats.skipped {
        let why = match reason {
            RiverSkip::TooFewSections => "fewer than two cross-sections",
            RiverSkip::TooManySections => "more cross-sections than the builder will take",
            RiverSkip::NonFinite => "a coordinate or the height is not finite",
            RiverSkip::Degenerate => "the ribbon has no length (a zero-extent editor accident)",
        };
        println!("  skipped {count:>5}  {why}");
    }
    if stats.drawn > 0 {
        println!(
            "  {} cross-section(s) -> {} vertices, {} triangles; {} additive, \
             {} bank order(s) swapped, {} backwards triangle(s)",
            stats.sections,
            stats.vertices,
            stats.triangles,
            stats.additive,
            stats.flipped,
            stats.backwards_triangles
        );
        for ribbon in &network.ribbons {
            println!(
                "    #{:<4} {:<20} z {:>5.0}  scroll {:<6} {:<24} {:>6.0} long / {:>5.0} wide \
                 = {:>5.1} repeat(s), flow ({:+.2},{:+.2}) at ({:.0},{:.0}){}",
                ribbon.unique_id,
                ribbon.name,
                ribbon.height,
                ribbon.uv_scroll_speed,
                ribbon.river_texture,
                ribbon.length,
                ribbon.width,
                ribbon.repeats,
                ribbon.flow[0],
                ribbon.flow[1],
                ribbon.centre[0],
                ribbon.centre[1],
                if ribbon.additive_blend {
                    "  additive"
                } else {
                    ""
                },
            );
        }
    }
    println!(
        "  rivers = {mode_label} ({})",
        if mode_on { "drawn" } else { "not drawn" }
    );
}

pub(crate) fn report_waves(
    field: &WaveField,
    surface: &WaveSurface,
    look: WaveLook,
    elapsed: Duration,
) {
    let stats = &field.stats;
    println!();
    println!(
        "  {} standing-wave area(s), {} drawn, {} skipped, in {:.1} ms",
        stats.areas,
        stats.drawn,
        stats.skipped_total(),
        elapsed.as_secs_f64() * 1000.0
    );
    if stats.areas == 0 {
        println!("  this map places no waves, so it has none - design W1/W3");
        return;
    }
    for (reason, count) in &stats.skipped {
        let why = match reason {
            WaveSkip::NoPoints => "the area carries no points (an unfinished editor object)",
            WaveSkip::NoShoreAxis => "one point, and no standing water to take an axis from",
            WaveSkip::NonFinite => "a coordinate is not finite",
            WaveSkip::Degenerate => "the authored line has no length",
        };
        println!("  skipped {count:>5}  {why}");
    }
    if stats.drawn == 0 {
        return;
    }
    println!(
        "  {} tile(s) -> {} vertices, {} triangles; {} area(s) built from a single point, \
         {} capped at the tile budget",
        stats.tiles, stats.vertices, stats.triangles, stats.single_point, stats.capped
    );
    println!(
        "  waves = {} (--waves, mode.waves), pca = {} (--wave-pca, mode.wave_pca), \
         scale = {} (--wave-scale, mode.wave_scale)",
        look.draw.label(),
        look.pca.label(),
        look.scale.label()
    );
    println!(
        "  enablePcaWave: {} of {} drawn area(s) set it, {} are chunk version 1 and carry \
         no such field; on the crest rung it lifts the sprite {WAVE_CREST_HEIGHT} world units",
        stats.pca, stats.drawn, stats.no_pca_field
    );
    println!(
        "  beach dissolve {WAVE_BEACH_BAND} world units, sprite alpha x{WAVE_ALPHA_GAIN}, \
         velocity x{WAVE_VELOCITY_UNIT} world units/s (all ours, all in the tuning file)"
    );
    println!(
        "  surface displacement: {} swell(s) over {} PCA area(s), {} segment(s) past the \
         {} the uniform block holds",
        surface.waves.len(),
        surface.areas,
        surface.dropped,
        MAX_SURFACE_WAVES,
    );
    let units = look.scale.world_units();
    for area in &field.areas {
        let params = &area.params;
        println!(
            "    #{:<4} {:<16} z {:>5.0}  line {:>6.0} long -> {:>4} tile(s) at ({:.0},{:.0})  \
             {:>5.0}x{:<5.0} from {:>3.0}%x{:<3.0}%, {:>4.0} out at {:>5.1}/s, \
             life {:.1}s compress {:.1}s cycle {:.1}s{}  {}",
            area.unique_id,
            area.name,
            area.height,
            area.length,
            area.tiles,
            area.centre[0],
            area.centre[1],
            params.final_width * units,
            params.final_height * units,
            params.initial_width_fraction * 100.0,
            params.initial_height_fraction * 100.0,
            params.distance_from_shore * units,
            params.initial_velocity * units * WAVE_VELOCITY_UNIT,
            params.time_to_fade,
            params.time_to_compress,
            params.time_offset_second_wave,
            if params.pca { " pca" } else { "" },
            area.texture,
        );
    }
}
