use std::collections::BTreeMap;

use mint_render::{MeshData, Vertex};
use mint_sage_map::{HeightMapData, StandingWaterArea};

use crate::water_depth::{ground_samples, subdivide, subdivision_levels_with};

pub const WATER_UV_WORLD_SIZE: f32 = 150.0;

pub const WATER_TINT: [f32; 4] = [0.24, 0.42, 0.52, 0.66];

#[must_use]
pub(crate) fn wrap_unit(value: f32) -> f32 {
    let wrapped = value % 1.0;
    if wrapped < 0.0 {
        let lifted = wrapped + 1.0;
        if lifted < 1.0 {
            lifted
        } else {
            0.0
        }
    } else {
        wrapped
    }
}

pub const MAX_POLYGON_POINTS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Winding {
    Counter,
    Clockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WaterSkip {
    TooFewPoints,
    TooManyPoints,
    NonFinite,
    Degenerate,
}

#[derive(Debug, Clone)]
pub struct WaterBody {
    pub unique_id: u32,
    pub name: String,
    pub height: f32,
    pub additive_blend: bool,
    pub bump_texture: String,
    pub sky_texture: String,
    pub fx_shader: String,
    pub depth_colors: String,
    pub uv_scroll_speed: f32,
    pub mesh: MeshData,
    pub points: usize,
    pub ground: Vec<f32>,
    pub subdivision: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaterStats {
    pub areas: usize,
    pub drawn: usize,
    pub skipped: BTreeMap<WaterSkip, usize>,
    pub points: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub convex: usize,
    pub additive: usize,
    pub forced_clips: usize,
    pub dropped_triangles: usize,
    pub splits: usize,
}

impl WaterStats {
    #[must_use]
    pub fn skipped_total(&self) -> usize {
        self.skipped
            .values()
            .fold(0usize, |sum, count| sum.saturating_add(*count))
    }
}

#[derive(Debug, Clone, Default)]
pub struct WaterSurface {
    pub bodies: Vec<WaterBody>,
    pub stats: WaterStats,
}

#[derive(Debug, Clone, Default)]
pub struct Triangulation {
    pub triangles: Vec<[u32; 3]>,
    pub forced: usize,
    pub dropped: usize,
    pub splits: usize,
}

#[must_use]
fn double_signed_area(points: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0f32;
    let last = points.len().saturating_sub(1);
    let mut previous = points.get(last).copied().unwrap_or([0.0, 0.0]);
    for point in points {
        sum += previous[0] * point[1] - point[0] * previous[1];
        previous = *point;
    }
    sum
}

#[must_use]
pub fn is_convex(points: &[[f32; 2]]) -> bool {
    let len = points.len();
    if len < 3 {
        return false;
    }
    let mut sign = 0i8;
    for i in 0..len {
        let (Some(a), Some(b), Some(c)) = (
            points.get(previous_index(i, len)),
            points.get(i),
            points.get(next_index(i, len)),
        ) else {
            return false;
        };
        let turn = cross(*a, *b, *c);
        let this = if turn > 0.0 {
            1
        } else if turn < 0.0 {
            -1
        } else {
            0
        };
        if this != 0 {
            if sign != 0 && this != sign {
                return false;
            }
            sign = this;
        }
    }
    true
}

fn previous_index(i: usize, len: usize) -> usize {
    if i == 0 {
        len.saturating_sub(1)
    } else {
        i.saturating_sub(1)
    }
}

fn next_index(i: usize, len: usize) -> usize {
    let next = i.saturating_add(1);
    if next >= len {
        0
    } else {
        next
    }
}

fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
}

fn inside(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> bool {
    cross(a, b, p) > 0.0 && cross(b, c, p) > 0.0 && cross(c, a, p) > 0.0
}

const MAX_SPLIT_RINGS: usize = 64;

fn pinch_pair(points: &[[f32; 2]], ring: &[u32]) -> Option<(usize, usize)> {
    let len = ring.len();
    let at = |slot: usize| -> Option<[f32; 2]> {
        points.get(usize::try_from(*ring.get(slot)?).ok()?).copied()
    };
    for i in 0..len {
        let a = at(i)?;
        for j in i.saturating_add(2)..len {
            if i == 0 && j.saturating_add(1) >= len {
                continue;
            }
            if at(j)? == a {
                return Some((i, j));
            }
        }
    }
    None
}

#[derive(Debug, Default)]
struct Clipped {
    triangles: Vec<[u32; 3]>,
    dropped: usize,
}

impl Clipped {
    fn push(&mut self, points: &[[f32; 2]], triangle: [u32; 3]) {
        let corner = |slot: usize| -> Option<[f32; 2]> {
            points
                .get(usize::try_from(*triangle.get(slot)?).ok()?)
                .copied()
        };
        let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
            return;
        };
        if cross(a, b, c) > 0.0 {
            self.triangles.push(triangle);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}

fn clip_ring(points: &[[f32; 2]], mut ring: Vec<u32>, allow_force: bool) -> Option<Clipped> {
    let at = |ring: &[u32], slot: usize| -> Option<[f32; 2]> {
        points.get(usize::try_from(*ring.get(slot)?).ok()?).copied()
    };
    let mut out = Clipped::default();
    while ring.len() > 3 {
        let count = ring.len();
        let corner = |ring: &[u32], slot: usize| -> Option<([f32; 2], [f32; 2], [f32; 2])> {
            Some((
                at(ring, previous_index(slot, count))?,
                at(ring, slot)?,
                at(ring, next_index(slot, count))?,
            ))
        };

        let flat = (0..count)
            .find(|slot| corner(&ring, *slot).is_some_and(|(a, b, c)| cross(a, b, c) == 0.0));

        let ear = flat.is_none().then(|| {
            (0..count).find(|slot| {
                let Some((a, b, c)) = corner(&ring, *slot) else {
                    return false;
                };
                if cross(a, b, c) <= 0.0 {
                    return false;
                }
                let previous = previous_index(*slot, count);
                let next = next_index(*slot, count);
                !(0..count)
                    .filter(|other| *other != previous && *other != *slot && *other != next)
                    .filter_map(|other| at(&ring, other))
                    .any(|p| inside(a, b, c, p))
            })
        });

        let (slot, emit) = match (flat, ear.flatten()) {
            (Some(slot), _) => (slot, false),
            (None, Some(slot)) => (slot, true),
            (None, None) if allow_force => (0, true),
            (None, None) => return None,
        };
        if emit {
            let previous = previous_index(slot, count);
            let next = next_index(slot, count);
            if let (Some(a), Some(b), Some(c)) = (
                ring.get(previous).copied(),
                ring.get(slot).copied(),
                ring.get(next).copied(),
            ) {
                out.push(points, [a, b, c]);
            }
        }
        if slot < ring.len() {
            ring.remove(slot);
        } else {
            break;
        }
    }
    if let (Some(a), Some(b), Some(c)) = (
        ring.first().copied(),
        ring.get(1).copied(),
        ring.get(2).copied(),
    ) {
        out.push(points, [a, b, c]);
    }
    Some(out)
}

#[must_use]
pub fn triangulate(points: &[[f32; 2]]) -> Triangulation {
    let mut out = Triangulation::default();
    let len = points.len();
    if len < 3
        || points
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return out;
    }
    let winding = match double_signed_area(points) {
        area if area > 0.0 => Winding::Counter,
        area if area < 0.0 => Winding::Clockwise,
        _ => return out,
    };

    let mut ring: Vec<u32> = (0..len)
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        .collect();
    if winding == Winding::Clockwise {
        ring.reverse();
    }

    let mut pending = vec![ring];
    let mut rings_done = 0usize;
    while let Some(ring) = pending.pop() {
        rings_done = rings_done.saturating_add(1);
        if ring.len() < 3 {
            continue;
        }
        let exhausted = rings_done >= MAX_SPLIT_RINGS;
        if let Some(clipped) = clip_ring(points, ring.clone(), exhausted) {
            if exhausted {
                out.forced = out.forced.saturating_add(1);
            }
            out.triangles.extend(clipped.triangles);
            out.dropped = out.dropped.saturating_add(clipped.dropped);
            continue;
        }
        match pinch_pair(points, &ring) {
            Some((i, j)) => {
                let first: Vec<u32> = ring.get(i..=j).map(<[u32]>::to_vec).unwrap_or_default();
                let mut second: Vec<u32> = ring.get(j..).map(<[u32]>::to_vec).unwrap_or_default();
                second.extend(ring.get(..=i).unwrap_or_default());
                out.splits = out.splits.saturating_add(1);
                pending.push(first);
                pending.push(second);
            }
            None => {
                out.forced = out.forced.saturating_add(1);
                if let Some(clipped) = clip_ring(points, ring, true) {
                    out.triangles.extend(clipped.triangles);
                    out.dropped = out.dropped.saturating_add(clipped.dropped);
                }
            }
        }
    }
    out
}

#[allow(clippy::cast_precision_loss)]
fn height_of(water_height: u32) -> f32 {
    water_height as f32
}

#[allow(clippy::too_many_arguments)]
fn build_body(
    area: &StandingWaterArea,
    uv_world_size: f32,
    height_map: Option<&HeightMapData>,
    target_edge: f32,
    budget: usize,
    forced: &mut usize,
    dropped: &mut usize,
    splits: &mut usize,
) -> Result<(WaterBody, bool), WaterSkip> {
    if area.points.len() < 3 {
        return Err(WaterSkip::TooFewPoints);
    }
    if area.points.len() > MAX_POLYGON_POINTS {
        return Err(WaterSkip::TooManyPoints);
    }
    let height = height_of(area.water_height);
    if !height.is_finite()
        || area
            .points
            .iter()
            .any(|p| !p[0].is_finite() || !p[1].is_finite())
    {
        return Err(WaterSkip::NonFinite);
    }
    let triangulation = triangulate(&area.points);
    if triangulation.triangles.is_empty() {
        return Err(WaterSkip::Degenerate);
    }
    *forced = forced.saturating_add(triangulation.forced);
    *dropped = dropped.saturating_add(triangulation.dropped);
    *splits = splits.saturating_add(triangulation.splits);

    let repeat = if uv_world_size.is_finite() && uv_world_size > 0.0 {
        uv_world_size
    } else {
        WATER_UV_WORLD_SIZE
    };
    let mut points = area.points.clone();
    let mut triangles = triangulation.triangles;
    let mut subdivision = 0;
    if height_map.is_some() {
        let polygon_area = double_signed_area(&points).abs() * 0.5;
        subdivision = subdivision_levels_with(
            polygon_area,
            triangles.len(),
            points.len(),
            target_edge,
            budget,
        );
        triangles = subdivide(&mut points, triangles, subdivision);
    }

    let mut mesh = MeshData {
        vertices: Vec::with_capacity(points.len()),
        indices: Vec::with_capacity(triangles.len().saturating_mul(3)),
    };
    for point in &points {
        mesh.vertices.push(
            Vertex::new(
                [point[0], point[1], height],
                [0.0, 0.0, 1.0],
                [point[0] / repeat, point[1] / repeat],
            )
            .with_tangent([1.0, 0.0, 0.0, 1.0]),
        );
    }
    for [a, b, c] in triangles {
        mesh.indices.extend_from_slice(&[a, b, c]);
    }
    let ground = match height_map {
        Some(height_map) => ground_samples(height_map, &points),
        None => Vec::new(),
    };

    let convex = is_convex(&area.points);
    Ok((
        WaterBody {
            unique_id: area.unique_id,
            name: area.name.clone(),
            height,
            additive_blend: area.additive_blend,
            bump_texture: area.bump_texture.clone(),
            sky_texture: area.sky_texture.clone(),
            fx_shader: area.fx_shader.clone(),
            depth_colors: area.depth_colors.clone(),
            uv_scroll_speed: area.uv_scroll_speed,
            mesh,
            points: area.points.len(),
            ground,
            subdivision,
        },
        convex,
    ))
}

#[must_use]
pub fn build_standing_water(
    areas: &[StandingWaterArea],
    uv_world_size: f32,
    height_map: Option<&HeightMapData>,
) -> WaterSurface {
    build_standing_water_tuned(
        areas,
        uv_world_size,
        height_map,
        crate::water_depth::WATER_DEPTH_EDGE,
        crate::water_depth::MAX_WATER_VERTICES,
    )
}

#[must_use]
pub fn build_standing_water_tuned(
    areas: &[StandingWaterArea],
    uv_world_size: f32,
    height_map: Option<&HeightMapData>,
    target_edge: f32,
    budget: usize,
) -> WaterSurface {
    let mut surface = WaterSurface {
        bodies: Vec::new(),
        stats: WaterStats {
            areas: areas.len(),
            ..WaterStats::default()
        },
    };
    for area in areas {
        let mut forced = 0usize;
        let mut dropped = 0usize;
        let mut splits = 0usize;
        match build_body(
            area,
            uv_world_size,
            height_map,
            target_edge,
            budget,
            &mut forced,
            &mut dropped,
            &mut splits,
        ) {
            Err(reason) => {
                let slot = surface.stats.skipped.entry(reason).or_insert(0);
                *slot = slot.saturating_add(1);
            }
            Ok((body, convex)) => {
                let stats = &mut surface.stats;
                stats.drawn = stats.drawn.saturating_add(1);
                stats.points = stats.points.saturating_add(body.points);
                stats.vertices = stats.vertices.saturating_add(body.mesh.vertices.len());
                stats.triangles = stats.triangles.saturating_add(body.mesh.triangle_count());
                stats.forced_clips = stats.forced_clips.saturating_add(forced);
                stats.dropped_triangles = stats.dropped_triangles.saturating_add(dropped);
                stats.splits = stats.splits.saturating_add(splits);
                if convex {
                    stats.convex = stats.convex.saturating_add(1);
                }
                if body.additive_blend {
                    stats.additive = stats.additive.saturating_add(1);
                }
                surface.bodies.push(body);
            }
        }
    }
    surface
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn area(points: Vec<[f32; 2]>, height: u32) -> StandingWaterArea {
        StandingWaterArea {
            unique_id: 1,
            name: "New Water Area".to_string(),
            layer_name: String::new(),
            uv_scroll_speed: 0.06,
            additive_blend: false,
            bump_texture: "WaterRippleBump.tga".to_string(),
            sky_texture: "SkyEnv.tga".to_string(),
            points,
            water_height: height,
            fx_shader: "Wtr_Moat.W3D".to_string(),
            depth_colors: "LUTDepthTint.tga".to_string(),
        }
    }

    fn double_areas(points: &[[f32; 2]], triangles: &[[u32; 3]]) -> Vec<f32> {
        triangles
            .iter()
            .map(|[a, b, c]| {
                let (a, b, c) = (
                    points[*a as usize],
                    points[*b as usize],
                    points[*c as usize],
                );
                (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
            })
            .collect()
    }

    #[test]
    fn a_non_convex_polygon_is_covered_by_forward_triangles_only() {
        let chevron = vec![[0.0, 0.0], [2.0, 3.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let result = triangulate(&chevron);
        assert_eq!(result.forced, 0);
        assert_eq!(result.triangles.len(), 3, "n - 2 triangles");
        let areas = double_areas(&chevron, &result.triangles);
        assert!(
            areas.iter().all(|a| *a > 0.0),
            "no triangle may be wound backwards: {areas:?}"
        );
        let total: f32 = areas.iter().sum();
        assert!((total - 20.0).abs() < 1e-4, "twice the polygon's area");

        let fan: Vec<[u32; 3]> = (1..4).map(|i| [0, i, i + 1]).collect();
        assert!(double_areas(&chevron, &fan).iter().any(|a| *a < 0.0));
    }

    #[test]
    fn a_clockwise_polygon_is_emitted_counter_clockwise() {
        let clockwise = vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let result = triangulate(&clockwise);
        assert_eq!(result.triangles.len(), 2);
        assert!(double_areas(&clockwise, &result.triangles)
            .iter()
            .all(|a| *a > 0.0));
    }

    #[test]
    fn degenerate_polygons_produce_nothing_and_terminate() {
        assert!(triangulate(&[]).triangles.is_empty());
        assert!(triangulate(&[[0.0, 0.0], [1.0, 1.0]]).triangles.is_empty());
        assert!(triangulate(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]])
            .triangles
            .is_empty());
        assert!(triangulate(&[[3.0, 3.0]; 8]).triangles.is_empty());
        assert!(triangulate(&[[0.0, 0.0], [f32::NAN, 1.0], [1.0, 1.0]])
            .triangles
            .is_empty());

        let crossed = vec![
            [4.0, 0.0],
            [4.0, 4.0],
            [3.0, 0.0],
            [1.0, 0.0],
            [4.0, 6.0],
            [1.0, 2.0],
        ];
        let result = triangulate(&crossed);
        assert_eq!(result.forced, 1);
        assert!(result.triangles.len() <= 4);
    }

    #[test]
    fn a_polygon_that_touches_itself_is_covered_exactly() {
        let pinched = vec![
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 6.0],
            [4.0, 6.0],
            [4.0, 8.0],
            [6.0, 8.0],
            [6.0, 10.0],
            [2.0, 10.0],
            [2.0, 8.0],
            [4.0, 8.0],
            [4.0, 6.0],
            [0.0, 6.0],
        ];
        let result = triangulate(&pinched);
        assert_eq!(result.forced, 0);
        let areas = double_areas(&pinched, &result.triangles);
        assert!(areas.iter().all(|a| *a > 0.0), "{areas:?}");
        let total: f32 = areas.iter().sum();
        assert!((total - 88.0).abs() < 1e-3, "covered {total}, want 88");
    }

    #[test]
    fn the_mesh_sits_at_the_stored_height_with_a_world_planar_uv() {
        let surface = build_standing_water(
            &[area(
                vec![[0.0, 0.0], [300.0, 0.0], [300.0, 300.0], [0.0, 300.0]],
                123,
            )],
            WATER_UV_WORLD_SIZE,
            None,
        );
        assert_eq!(surface.stats.drawn, 1);
        assert_eq!(surface.stats.triangles, 2);
        assert_eq!(surface.stats.convex, 1);
        let body = &surface.bodies[0];
        assert_eq!(body.height, 123.0);
        for vertex in &body.mesh.vertices {
            assert_eq!(vertex.position[2], 123.0, "flat at the stored height");
            assert_eq!(vertex.uv[0], vertex.position[0] / WATER_UV_WORLD_SIZE);
            assert_eq!(vertex.uv[1], vertex.position[1] / WATER_UV_WORLD_SIZE);
        }
        assert_eq!(body.mesh.vertices[2].uv, [2.0, 2.0]);
        assert!(body.mesh.validate().is_none(), "indices must be in range");
    }

    #[test]
    fn skipped_areas_are_counted_by_reason() {
        let surface = build_standing_water(
            &[
                area(vec![[0.0, 0.0], [1.0, 1.0]], 7),
                area(vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 7),
                area(vec![[0.0, 0.0], [f32::INFINITY, 0.0], [1.0, 1.0]], 7),
                area(vec![[0.0, 0.0]; MAX_POLYGON_POINTS + 1], 7),
            ],
            WATER_UV_WORLD_SIZE,
            None,
        );
        assert_eq!(surface.stats.areas, 4);
        assert_eq!(surface.stats.drawn, 0);
        assert_eq!(surface.stats.skipped_total(), 4);
        assert_eq!(surface.stats.skipped[&WaterSkip::TooFewPoints], 1);
        assert_eq!(surface.stats.skipped[&WaterSkip::Degenerate], 1);
        assert_eq!(surface.stats.skipped[&WaterSkip::NonFinite], 1);
        assert_eq!(surface.stats.skipped[&WaterSkip::TooManyPoints], 1);
    }
}
