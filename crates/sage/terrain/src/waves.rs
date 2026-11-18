use mint_render::{MeshData, Vertex};
use mint_sage_map::{HeightMapData, StandingWaterArea, StandingWaveArea};

use crate::surface::surface_height;

pub const MAX_WAVE_TILES: usize = 1024;

const MAX_WAVE_TILES_F: f32 = 1024.0;

pub const WAVE_TILE_OVERLAP: f32 = 0.75;

pub const MIN_WAVE_SPACING: f32 = 2.0;

pub const MAX_WAVE_EXTENT: f32 = 4096.0;

pub const WAVE_LIFT: f32 = 0.25;

pub const WAVE_VELOCITY_UNIT: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveScale {
    #[default]
    Record,
    Cells,
}

impl WaveScale {
    #[must_use]
    pub fn world_units(self) -> f32 {
        match self {
            Self::Record => 1.0,
            Self::Cells => mint_sage_map::MAP_XY_FACTOR,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Record => Self::Cells,
            Self::Cells => Self::Record,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Cells => "cells",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "record" => Some(Self::Record),
            "cells" => Some(Self::Cells),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveParams {
    pub final_width: f32,
    pub final_height: f32,
    pub initial_width_fraction: f32,
    pub initial_height_fraction: f32,
    pub initial_velocity: f32,
    pub time_to_fade: f32,
    pub time_to_compress: f32,
    pub time_offset_second_wave: f32,
    pub distance_from_shore: f32,
    pub pca: bool,
}

impl WaveParams {
    fn from_record(area: &StandingWaveArea) -> Self {
        let extent = |raw: u32| (raw as f32).clamp(0.0, MAX_WAVE_EXTENT);
        let fraction = |raw: u32| (raw as f32 / 100.0).clamp(0.0, 4.0);
        let seconds = |raw: u32| (raw as f32 / 1000.0).clamp(0.001, 600.0);
        Self {
            final_width: extent(area.final_width),
            final_height: extent(area.final_height),
            initial_width_fraction: fraction(area.initial_width_fraction),
            initial_height_fraction: fraction(area.initial_height_fraction),
            initial_velocity: extent(area.initial_velocity),
            time_to_fade: seconds(area.time_to_fade),
            time_to_compress: seconds(area.time_to_compress),
            time_offset_second_wave: seconds(area.time_offset_second_wave),
            distance_from_shore: extent(area.distance_from_shore),
            pca: area.enable_pca_wave.unwrap_or(0) != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaveSkip {
    NoPoints,
    NoShoreAxis,
    NonFinite,
    Degenerate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveLine {
    pub start: [f32; 2],
    pub along: [f32; 2],
    pub shore: [f32; 2],
    pub length: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveArea {
    pub unique_id: u32,
    pub name: String,
    pub texture: String,
    pub params: WaveParams,
    pub height: f32,
    pub body: Option<usize>,
    pub mesh: MeshData,
    pub tiles: usize,
    pub length: f32,
    pub centre: [f32; 2],
    pub lines: Vec<WaveLine>,
    pub phase: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaveStats {
    pub areas: usize,
    pub drawn: usize,
    pub skipped: std::collections::BTreeMap<WaveSkip, usize>,
    pub single_point: usize,
    pub tiles: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub capped: usize,
    pub pca: usize,
    pub no_pca_field: usize,
}

impl WaveStats {
    #[must_use]
    pub fn skipped_total(&self) -> usize {
        self.skipped.values().copied().sum()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaveField {
    pub areas: Vec<WaveArea>,
    pub stats: WaveStats,
}

#[must_use]
pub fn build_standing_waves(
    waves: &[StandingWaveArea],
    water: &[StandingWaterArea],
    height_map: Option<&HeightMapData>,
    tuning: &crate::water_tuning::WaterTuning,
) -> WaveField {
    let mut field = WaveField {
        areas: Vec::new(),
        stats: WaveStats {
            areas: waves.len(),
            ..WaveStats::default()
        },
    };
    for area in waves {
        let params = WaveParams::from_record(area);
        match build_area(area, params, water, height_map, tuning, &mut field.stats) {
            Err(reason) => {
                let slot = field.stats.skipped.entry(reason).or_insert(0);
                *slot = slot.saturating_add(1);
            }
            Ok(built) => {
                let stats = &mut field.stats;
                stats.drawn = stats.drawn.saturating_add(1);
                stats.tiles = stats.tiles.saturating_add(built.tiles);
                stats.vertices = stats.vertices.saturating_add(built.mesh.vertices.len());
                stats.triangles = stats.triangles.saturating_add(built.mesh.triangle_count());
                if area.enable_pca_wave.is_none() {
                    stats.no_pca_field = stats.no_pca_field.saturating_add(1);
                } else if params.pca {
                    stats.pca = stats.pca.saturating_add(1);
                }
                field.areas.push(built);
            }
        }
    }
    field
}

fn polyline(
    area: &StandingWaveArea,
    params: &WaveParams,
    water: &[StandingWaterArea],
) -> Result<Vec<[f32; 2]>, WaveSkip> {
    if area.points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(WaveSkip::NonFinite);
    }
    match area.points.len() {
        0 => Err(WaveSkip::NoPoints),
        1 => {
            let point = *area.points.first().ok_or(WaveSkip::NoPoints)?;
            let axis = nearest_outline_axis(point, water).ok_or(WaveSkip::NoShoreAxis)?;
            let half = (params.final_width * 0.5).max(MIN_WAVE_SPACING);
            Ok(vec![
                [point[0] - axis[0] * half, point[1] - axis[1] * half],
                [point[0] + axis[0] * half, point[1] + axis[1] * half],
            ])
        }
        _ => Ok(area.points.clone()),
    }
}

fn build_area(
    area: &StandingWaveArea,
    params: WaveParams,
    water: &[StandingWaterArea],
    height_map: Option<&HeightMapData>,
    tuning: &crate::water_tuning::WaterTuning,
    stats: &mut WaveStats,
) -> Result<WaveArea, WaveSkip> {
    let points = polyline(area, &params, water)?;
    if area.points.len() == 1 {
        stats.single_point = stats.single_point.saturating_add(1);
    }
    let tuned_width =
        params.final_width * finite(tuning.wave_unit, 1.0) * finite(tuning.wave_final_width, 1.0);
    let spacing = (tuned_width * WAVE_TILE_OVERLAP).max(MIN_WAVE_SPACING);

    let mut length = 0.0f32;
    for (a, b) in segments(&points) {
        length += distance(a, b);
    }
    if length <= MIN_WAVE_SPACING || !length.is_finite() {
        return Err(WaveSkip::Degenerate);
    }
    let wanted = (length / spacing).ceil().max(1.0);
    let tiles_wanted = if wanted.is_finite() && wanted < MAX_WAVE_TILES_F {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            wanted as usize
        }
    } else {
        stats.capped = stats.capped.saturating_add(1);
        MAX_WAVE_TILES
    };
    #[allow(clippy::cast_precision_loss)]
    let step = length / tiles_wanted as f32;

    let centre = point_at(&points, length * 0.5);
    let body = body_for(centre, water);
    let height = body
        .and_then(|slot| water.get(slot))
        .map_or(0.0, |area| area.water_height as f32)
        + WAVE_LIFT;

    let mut mesh = MeshData {
        vertices: Vec::with_capacity(tiles_wanted.saturating_mul(4)),
        indices: Vec::with_capacity(tiles_wanted.saturating_mul(6)),
    };
    let phase = fract(f32::from(u16::try_from(area.unique_id % 1024).unwrap_or(0)) * 0.618_034);

    let mut lines = Vec::new();
    for (a, b) in segments(&points) {
        let span = distance(a, b);
        let Some(along) = normalize([b[0] - a[0], b[1] - a[1]]) else {
            continue;
        };
        let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let left = [-along[1], along[0]];
        let sign = shoreward_sign(mid, left, &params, water, height_map);
        lines.push(WaveLine {
            start: a,
            along,
            shore: [left[0] * sign, left[1] * sign],
            length: span,
        });
    }

    let mut tiles = 0usize;
    for index in 0..tiles_wanted {
        #[allow(clippy::cast_precision_loss)]
        let along = (index as f32 + 0.5) * step;
        let at = point_at(&points, along);
        let Some(tangent) = normalize(tangent_at(&points, along)) else {
            continue;
        };
        let left = [-tangent[1], tangent[0]];
        let sign = shoreward_sign(at, left, &params, water, height_map);
        let shore = [left[0] * sign, left[1] * sign];
        let mirror = index % 2 == 1;
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        if base > u32::MAX.saturating_sub(4) {
            break;
        }
        for (corner_x, corner_y) in [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            let mut u = corner_x + 0.5;
            if mirror {
                u = 1.0 - u;
            }
            let v = corner_y + 0.5;
            mesh.vertices.push(
                Vertex::new([at[0], at[1], height], [shore[0], shore[1], 0.0], [u, v])
                    .with_tangent([tangent[0], tangent[1], 0.0, 1.0])
                    .with_uv2([corner_x, corner_y])
                    .with_color([phase, 0.0, 0.0, 1.0]),
            );
        }
        mesh.indices.extend_from_slice(&[
            base,
            base.saturating_add(1),
            base.saturating_add(2),
            base,
            base.saturating_add(2),
            base.saturating_add(3),
        ]);
        tiles = tiles.saturating_add(1);
    }
    if tiles == 0 {
        return Err(WaveSkip::Degenerate);
    }
    Ok(WaveArea {
        unique_id: area.unique_id,
        name: area.name.clone(),
        texture: area.wave_texture.clone(),
        params,
        height,
        body,
        mesh,
        tiles,
        length,
        centre,
        lines,
        phase,
    })
}

fn shoreward_sign(
    point: [f32; 2],
    left: [f32; 2],
    params: &WaveParams,
    water: &[StandingWaterArea],
    height_map: Option<&HeightMapData>,
) -> f32 {
    let span = params.distance_from_shore.max(MIN_WAVE_SPACING);
    if let Some(height_map) = height_map {
        let mut delta = 0.0f32;
        let mut sampled = 0u32;
        for step in [1.0f32, 2.0, 4.0] {
            let reach = span * step;
            let plus = surface_height(
                height_map,
                1.0,
                point[0] + left[0] * reach,
                point[1] + left[1] * reach,
            );
            let minus = surface_height(
                height_map,
                1.0,
                point[0] - left[0] * reach,
                point[1] - left[1] * reach,
            );
            if let (Some(plus), Some(minus)) = (plus, minus) {
                delta += plus - minus;
                sampled = sampled.saturating_add(1);
            }
        }
        if sampled > 0 && delta != 0.0 {
            return if delta > 0.0 { 1.0 } else { -1.0 };
        }
    }
    if let Some(inward) = nearest_outline_inward(point, water) {
        let dot = left[0] * inward[0] + left[1] * inward[1];
        if dot != 0.0 {
            return if dot < 0.0 { 1.0 } else { -1.0 };
        }
    }
    1.0
}

fn body_for(point: [f32; 2], water: &[StandingWaterArea]) -> Option<usize> {
    let mut nearest: Option<(f32, usize)> = None;
    for (slot, area) in water.iter().enumerate() {
        if contains(&area.points, point) {
            return Some(slot);
        }
        let distance = outline_distance(&area.points, point);
        if distance.is_finite() && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, slot));
        }
    }
    nearest.map(|(_, slot)| slot)
}

fn nearest_outline_axis(point: [f32; 2], water: &[StandingWaterArea]) -> Option<[f32; 2]> {
    nearest_edge(point, water).and_then(|(edge, _)| normalize(edge))
}

fn nearest_outline_inward(point: [f32; 2], water: &[StandingWaterArea]) -> Option<[f32; 2]> {
    let (edge, counter_clockwise) = nearest_edge(point, water)?;
    let inward = if counter_clockwise {
        [-edge[1], edge[0]]
    } else {
        [edge[1], -edge[0]]
    };
    normalize(inward)
}

fn nearest_edge(point: [f32; 2], water: &[StandingWaterArea]) -> Option<([f32; 2], bool)> {
    let mut best = f32::INFINITY;
    let mut found: Option<([f32; 2], bool)> = None;
    for area in water {
        let count = area.points.len();
        if count < 2 {
            continue;
        }
        let counter_clockwise = double_signed_area(&area.points) >= 0.0;
        for index in 0..count {
            let Some(a) = area.points.get(index) else {
                continue;
            };
            let next = index.saturating_add(1);
            let Some(b) = area.points.get(if next >= count { 0 } else { next }) else {
                continue;
            };
            let distance = segment_distance(*a, *b, point);
            if distance < best {
                best = distance;
                found = Some(([b[0] - a[0], b[1] - a[1]], counter_clockwise));
            }
        }
    }
    found
}

fn outline_distance(points: &[[f32; 2]], point: [f32; 2]) -> f32 {
    let count = points.len();
    if count < 2 {
        return f32::INFINITY;
    }
    let mut best = f32::INFINITY;
    for index in 0..count {
        let Some(a) = points.get(index) else { continue };
        let next = index.saturating_add(1);
        let Some(b) = points.get(if next >= count { 0 } else { next }) else {
            continue;
        };
        best = best.min(segment_distance(*a, *b, point));
    }
    best
}

fn contains(points: &[[f32; 2]], point: [f32; 2]) -> bool {
    let count = points.len();
    if count < 3 {
        return false;
    }
    let mut inside = false;
    for index in 0..count {
        let Some(a) = points.get(index) else { continue };
        let next = index.saturating_add(1);
        let Some(b) = points.get(if next >= count { 0 } else { next }) else {
            continue;
        };
        if (a[1] > point[1]) == (b[1] > point[1]) {
            continue;
        }
        let denominator = b[1] - a[1];
        if denominator == 0.0 {
            continue;
        }
        let x = a[0] + (point[1] - a[1]) / denominator * (b[0] - a[0]);
        if point[0] < x {
            inside = !inside;
        }
    }
    inside
}

fn double_signed_area(points: &[[f32; 2]]) -> f32 {
    let count = points.len();
    let mut total = 0.0f32;
    for index in 0..count {
        let Some(a) = points.get(index) else { continue };
        let next = index.saturating_add(1);
        let Some(b) = points.get(if next >= count { 0 } else { next }) else {
            continue;
        };
        total += a[0] * b[1] - b[0] * a[1];
    }
    total
}

fn segment_distance(a: [f32; 2], b: [f32; 2], point: [f32; 2]) -> f32 {
    let edge = [b[0] - a[0], b[1] - a[1]];
    let length2 = edge[0] * edge[0] + edge[1] * edge[1];
    let t = if length2 > 1e-9 {
        (((point[0] - a[0]) * edge[0] + (point[1] - a[1]) * edge[1]) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    distance([a[0] + edge[0] * t, a[1] + edge[1] * t], point)
}

fn point_at(points: &[[f32; 2]], along: f32) -> [f32; 2] {
    let mut walked = 0.0f32;
    let mut last = points.first().copied().unwrap_or([0.0, 0.0]);
    for (a, b) in segments(points) {
        let step = distance(a, b);
        if step > 0.0 && walked + step >= along {
            let t = ((along - walked) / step).clamp(0.0, 1.0);
            return [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        }
        walked += step;
        last = b;
    }
    last
}

fn tangent_at(points: &[[f32; 2]], along: f32) -> [f32; 2] {
    let mut walked = 0.0f32;
    let mut last = [1.0f32, 0.0];
    for (a, b) in segments(points) {
        let step = distance(a, b);
        if step <= 0.0 {
            continue;
        }
        last = [b[0] - a[0], b[1] - a[1]];
        if walked + step >= along {
            return last;
        }
        walked += step;
    }
    last
}

fn segments(points: &[[f32; 2]]) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    points
        .iter()
        .zip(points.iter().skip(1))
        .map(|(a, b)| (*a, *b))
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

fn normalize(v: [f32; 2]) -> Option<[f32; 2]> {
    let length = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if length.is_finite() && length > 1e-6 {
        Some([v[0] / length, v[1] / length])
    } else {
        None
    }
}

fn fract(v: f32) -> f32 {
    v - v.floor()
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::water_tuning::WaterTuning;

    fn wave(points: Vec<[f32; 2]>) -> StandingWaveArea {
        StandingWaveArea {
            unique_id: 1,
            name: "New Wave Area".to_string(),
            layer_name: String::new(),
            uv_scroll_speed: 0.06,
            additive_blend: false,
            points,
            reserved: 0,
            final_width: 28,
            final_height: 18,
            initial_width_fraction: 50,
            initial_height_fraction: 100,
            initial_velocity: 5,
            time_to_fade: 2000,
            time_to_compress: 1000,
            time_offset_second_wave: 3000,
            distance_from_shore: 40,
            wave_texture: "wave256.tga".to_string(),
            enable_pca_wave: Some(1),
        }
    }

    fn sea() -> StandingWaterArea {
        StandingWaterArea {
            unique_id: 2,
            name: "New Water Area".to_string(),
            layer_name: String::new(),
            uv_scroll_speed: 0.06,
            additive_blend: false,
            bump_texture: "WaterRippleBump.tga".to_string(),
            sky_texture: "SkyEnv.tga".to_string(),
            points: vec![[0.0, 0.0], [600.0, 0.0], [600.0, 300.0], [0.0, 300.0]],
            water_height: 50,
            fx_shader: String::new(),
            depth_colors: String::new(),
        }
    }

    fn slope_north() -> HeightMapData {
        let (w, h) = (64u32, 64u32);
        let mut samples = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for _ in 0..w {
                samples.push(u16::try_from(y * 100).unwrap());
            }
        }
        HeightMapData {
            width: w,
            height: h,
            border_size: 0,
            boundaries: Vec::new(),
            samples,
        }
    }

    #[test]
    fn the_shoreward_direction_points_at_the_land() {
        let field = build_standing_waves(
            &[wave(vec![[100.0, 300.0], [500.0, 300.0]])],
            &[sea()],
            Some(&slope_north()),
            &WaterTuning::default(),
        );
        assert_eq!(field.stats.drawn, 1);
        let area = &field.areas[0];
        assert!(!area.lines.is_empty());
        for line in &area.lines {
            assert!(
                line.shore[1] > 0.9,
                "shore {:?} is not northward",
                line.shore
            );
            assert!(line.shore[0].abs() < 0.1, "shore {:?}", line.shore);
        }
        for vertex in &area.mesh.vertices {
            assert!(vertex.normal[1] > 0.9, "normal {:?}", vertex.normal);
        }
        let mut downhill = slope_north();
        downhill.samples.reverse();
        let flipped = build_standing_waves(
            &[wave(vec![[100.0, 300.0], [500.0, 300.0]])],
            &[sea()],
            Some(&downhill),
            &WaterTuning::default(),
        );
        for line in &flipped.areas[0].lines {
            assert!(line.shore[1] < -0.9, "shore {:?}", line.shore);
        }
    }

    #[test]
    fn a_capped_area_still_spans_its_whole_polyline() {
        let length = 40_000.0f32;
        let field = build_standing_waves(
            &[wave(vec![[0.0, 300.0], [length, 300.0]])],
            &[sea()],
            None,
            &WaterTuning::default(),
        );
        assert_eq!(field.stats.capped, 1);
        assert_eq!(field.stats.tiles, MAX_WAVE_TILES);
        let area = &field.areas[0];
        assert!((area.length - length).abs() < 1e-3);
        let step = length / MAX_WAVE_TILES as f32;
        let last_x = area
            .mesh
            .vertices
            .iter()
            .fold(f32::MIN, |acc, v| acc.max(v.position[0]));
        assert!(
            last_x > length - step,
            "last tile ends at {last_x}, line ends at {length}"
        );
    }

    #[test]
    fn degenerate_wave_areas_are_skipped_by_reason_and_never_panic() {
        let water = [sea()];
        let cases = [
            (wave(Vec::new()), &water[..], WaveSkip::NoPoints),
            (wave(vec![[0.0, 0.0]]), &[][..], WaveSkip::NoShoreAxis),
            (
                wave(vec![[0.0, f32::NAN], [1.0, 0.0]]),
                &water[..],
                WaveSkip::NonFinite,
            ),
            (
                wave(vec![[100.0, 300.0], [100.5, 300.0]]),
                &water[..],
                WaveSkip::Degenerate,
            ),
        ];
        for (area, bodies, reason) in cases {
            let field = build_standing_waves(&[area], bodies, None, &WaterTuning::default());
            assert_eq!(field.stats.drawn, 0, "{reason:?}");
            assert_eq!(field.stats.skipped.get(&reason), Some(&1), "{reason:?}");
        }
    }
}
