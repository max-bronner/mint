use std::collections::BTreeMap;

use mint_sage_map::{HeightMapData, MAP_XY_FACTOR};

use crate::surface::surface_height;

pub const WATER_DEPTH_EDGE: f32 = MAP_XY_FACTOR;

pub const MAX_WATER_SUBDIVISION: u32 = 6;

pub const MAX_WATER_VERTICES: usize = 300_000;

#[must_use]
pub fn subdivision_levels(polygon_area: f32, triangles: usize, vertices: usize) -> u32 {
    subdivision_levels_with(
        polygon_area,
        triangles,
        vertices,
        WATER_DEPTH_EDGE,
        MAX_WATER_VERTICES,
    )
}

#[must_use]
pub fn subdivision_levels_with(
    polygon_area: f32,
    triangles: usize,
    vertices: usize,
    target_edge: f32,
    budget: usize,
) -> u32 {
    if !polygon_area.is_finite() || polygon_area <= 0.0 || triangles == 0 {
        return 0;
    }
    let target_edge = if target_edge.is_finite() && target_edge > 0.0 {
        target_edge
    } else {
        WATER_DEPTH_EDGE
    };
    #[allow(clippy::cast_precision_loss)]
    let mean = polygon_area / triangles as f32;
    let mut edge = (2.0 * mean).sqrt();
    if !edge.is_finite() {
        return 0;
    }
    let mut levels = 0u32;
    let mut count = vertices;
    while levels < MAX_WATER_SUBDIVISION && edge > target_edge {
        let next = count.saturating_mul(4);
        if next > budget {
            break;
        }
        count = next;
        edge *= 0.5;
        levels = levels.saturating_add(1);
    }
    levels
}

#[must_use]
pub fn subdivide(
    points: &mut Vec<[f32; 2]>,
    triangles: Vec<[u32; 3]>,
    levels: u32,
) -> Vec<[u32; 3]> {
    let mut current = triangles;
    for _ in 0..levels {
        let mut midpoints: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut next = Vec::with_capacity(current.len().saturating_mul(4));
        for [a, b, c] in current {
            let (Some(ab), Some(bc), Some(ca)) = (
                midpoint(points, &mut midpoints, a, b),
                midpoint(points, &mut midpoints, b, c),
                midpoint(points, &mut midpoints, c, a),
            ) else {
                next.push([a, b, c]);
                continue;
            };
            next.push([a, ab, ca]);
            next.push([ab, b, bc]);
            next.push([ca, bc, c]);
            next.push([ab, bc, ca]);
        }
        current = next;
    }
    current
}

fn midpoint(
    points: &mut Vec<[f32; 2]>,
    cache: &mut BTreeMap<(u32, u32), u32>,
    a: u32,
    b: u32,
) -> Option<u32> {
    let key = if a <= b { (a, b) } else { (b, a) };
    if let Some(index) = cache.get(&key) {
        return Some(*index);
    }
    let pa = points.get(a as usize).copied()?;
    let pb = points.get(b as usize).copied()?;
    let index = u32::try_from(points.len()).ok()?;
    points.push([(pa[0] + pb[0]) * 0.5, (pa[1] + pb[1]) * 0.5]);
    cache.insert(key, index);
    Some(index)
}

#[must_use]
pub fn ground_samples(height_map: &HeightMapData, points: &[[f32; 2]]) -> Vec<f32> {
    points
        .iter()
        .map(|p| surface_height(height_map, 1.0, p[0], p[1]).unwrap_or(0.0))
        .collect()
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn subdivide_shares_a_shared_edges_midpoint_rather_than_duplicating_it() {
        let mut points = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let triangles = subdivide(&mut points, vec![[0, 1, 2], [0, 2, 3]], 1);
        assert_eq!(triangles.len(), 8, "each triangle splits four ways");
        assert_eq!(points.len(), 4 + 5, "5 unique edges, not 6");

        for (i, a) in points.iter().enumerate() {
            for b in points.iter().skip(i + 1) {
                assert!(a != b, "duplicate point {a:?}");
            }
        }
        let mid = points.iter().position(|p| *p == [5.0, 5.0]).unwrap();
        let mid = u32::try_from(mid).unwrap();
        assert!(triangles[..4].iter().any(|t| t.contains(&mid)));
        assert!(triangles[4..].iter().any(|t| t.contains(&mid)));
    }

    #[test]
    fn every_index_stays_in_range_at_every_level() {
        let mut points = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let triangles = subdivide(&mut points, vec![[0, 1, 2], [0, 2, 3]], 3);
        assert_eq!(triangles.len(), 2 * 4 * 4 * 4);
        let limit = u32::try_from(points.len()).unwrap();
        assert!(triangles.iter().flatten().all(|i| *i < limit));
    }

    #[test]
    fn the_level_count_respects_the_budget_the_cap_and_a_hostile_number() {
        assert_eq!(
            subdivision_levels_with(250_000.0, 1, 3, 10.0, usize::MAX),
            MAX_WATER_SUBDIVISION
        );
        assert_eq!(subdivision_levels_with(250_000.0, 1, 3, 10.0, 100), 2);
        assert_eq!(subdivision_levels_with(50.0, 1, 3, 10.0, usize::MAX), 0);
        for (area, tris, edge) in [
            (f32::NAN, 1usize, 10.0f32),
            (f32::INFINITY, 1, 10.0),
            (-1.0, 1, 10.0),
            (250_000.0, 0, 10.0),
            (250_000.0, 1, 0.0),
            (250_000.0, 1, f32::NAN),
        ] {
            let levels = subdivision_levels_with(area, tris, 3, edge, usize::MAX);
            assert!(levels <= MAX_WATER_SUBDIVISION, "{area} {tris} {edge}");
        }
    }
}
