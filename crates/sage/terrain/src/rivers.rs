use std::collections::BTreeMap;

use mint_render::{MeshData, Vertex};
use mint_sage_map::RiverArea;

use crate::water::wrap_unit;

pub const MAX_RIVER_SECTIONS: usize = 65_536;

pub const FALLBACK_RIVER_WIDTH: f32 = 96.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiverSkip {
    TooFewSections,
    TooManySections,
    NonFinite,
    Degenerate,
}

#[derive(Debug, Clone)]
pub struct RiverRibbon {
    pub unique_id: u32,
    pub name: String,
    pub height: f32,
    pub additive_blend: bool,
    pub river_texture: String,
    pub noise_texture: String,
    pub alpha_edge_texture: String,
    pub sparkle_texture: String,
    pub tint: [f32; 4],
    pub uv_scroll_speed: f32,
    pub minimum_water_lod: String,
    pub mesh: MeshData,
    pub sections: usize,
    pub length: f32,
    pub width: f32,
    pub repeats: f32,
    pub flow: [f32; 2],
    pub centre: [f32; 2],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RiverStats {
    pub areas: usize,
    pub drawn: usize,
    pub skipped: BTreeMap<RiverSkip, usize>,
    pub sections: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub additive: usize,
    pub flipped: usize,
    pub backwards_triangles: usize,
    pub zero_width_sections: usize,
    pub fallback_width: usize,
}

impl RiverStats {
    #[must_use]
    pub fn skipped_total(&self) -> usize {
        self.skipped
            .values()
            .fold(0usize, |sum, count| sum.saturating_add(*count))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RiverNetwork {
    pub ribbons: Vec<RiverRibbon>,
    pub stats: RiverStats,
}

#[must_use]
pub fn river_scroll_offset(speed: f32, seconds: f32) -> [f32; 2] {
    let distance = speed * seconds;
    if !distance.is_finite() || distance == 0.0 {
        return [0.0, 0.0];
    }
    [0.0, wrap_unit(-distance)]
}

#[allow(clippy::cast_precision_loss)]
fn height_of(water_height: u32) -> f32 {
    water_height as f32
}

fn quad_double_area(a0: [f32; 2], b0: [f32; 2], a1: [f32; 2], b1: [f32; 2]) -> f32 {
    let mut sum = 0.0f32;
    let mut previous = a1;
    for point in [a0, b0, b1, a1] {
        sum += previous[0] * point[1] - point[0] * previous[1];
        previous = point;
    }
    sum
}

fn triangle_double_area([a, b, c]: [[f32; 2]; 3]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
}

fn bank_at(area: &RiverArea, index: usize, flip: bool) -> Option<([f32; 2], [f32; 2])> {
    let line = area.lines.get(index)?;
    Some(if flip {
        (line.b, line.a)
    } else {
        (line.a, line.b)
    })
}

fn station_flow(mids: &[[f32; 2]], index: usize) -> [f32; 2] {
    let before = index.saturating_sub(1);
    let after = index.saturating_add(1).min(mids.len().saturating_sub(1));
    let (Some(from), Some(to)) = (mids.get(before), mids.get(after)) else {
        return [0.0, 0.0];
    };
    let step = [to[0] - from[0], to[1] - from[1]];
    let span = mint_math::f32::hypot(step[0], step[1]);
    if !span.is_finite() || span <= 0.0 {
        return [0.0, 0.0];
    }
    [step[0] / span, step[1] / span]
}

fn build_ribbon(area: &RiverArea, stats: &mut RiverStats) -> Result<RiverRibbon, RiverSkip> {
    let sections = area.lines.len();
    if sections < 2 {
        return Err(RiverSkip::TooFewSections);
    }
    if sections > MAX_RIVER_SECTIONS {
        return Err(RiverSkip::TooManySections);
    }
    let height = height_of(area.water_height);
    if !height.is_finite()
        || area.lines.iter().any(|line| {
            !line.a[0].is_finite()
                || !line.a[1].is_finite()
                || !line.b[0].is_finite()
                || !line.b[1].is_finite()
        })
    {
        return Err(RiverSkip::NonFinite);
    }

    let mut ribbon_area = 0.0f32;
    for pair in area.lines.windows(2) {
        let (Some(first), Some(second)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        ribbon_area += quad_double_area(first.a, first.b, second.a, second.b);
    }
    let flip = ribbon_area < 0.0;
    if flip {
        stats.flipped = stats.flipped.saturating_add(1);
    }

    let mut along = Vec::with_capacity(sections);
    let mut mids = Vec::with_capacity(sections);
    let mut total = 0.0f32;
    let mut width_sum = 0.0f32;
    let mut first_width = 0.0f32;
    let mut previous: Option<[f32; 2]> = None;
    for index in 0..sections {
        let Some((left, right)) = bank_at(area, index, flip) else {
            continue;
        };
        let width = mint_math::f32::hypot(right[0] - left[0], right[1] - left[1]);
        if width == 0.0 {
            stats.zero_width_sections = stats.zero_width_sections.saturating_add(1);
        }
        if index == 0 {
            first_width = width;
        }
        width_sum += width;
        let mid = [(left[0] + right[0]) * 0.5, (left[1] + right[1]) * 0.5];
        if let Some(last) = previous {
            total += mint_math::f32::hypot(mid[0] - last[0], mid[1] - last[1]);
        }
        previous = Some(mid);
        along.push(total);
        mids.push(mid);
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(RiverSkip::Degenerate);
    }

    #[allow(clippy::cast_precision_loss)]
    let mean_width = width_sum / sections as f32;
    let width = if first_width.is_finite() && first_width > 0.0 {
        first_width
    } else if mean_width.is_finite() && mean_width > 0.0 {
        mean_width
    } else {
        stats.fallback_width = stats.fallback_width.saturating_add(1);
        FALLBACK_RIVER_WIDTH
    };
    let repeats = total / width;

    let mut mesh = MeshData {
        vertices: Vec::with_capacity(sections.saturating_mul(2)),
        indices: Vec::with_capacity(sections.saturating_sub(1).saturating_mul(6)),
    };
    for index in 0..sections {
        let (Some((left, right)), Some(distance)) =
            (bank_at(area, index, flip), along.get(index).copied())
        else {
            continue;
        };
        let v = distance / width;
        let mut tangent = [right[0] - left[0], right[1] - left[1]];
        let across = mint_math::f32::hypot(tangent[0], tangent[1]);
        if across > 0.0 {
            tangent = [tangent[0] / across, tangent[1] / across];
        } else {
            tangent = [1.0, 0.0];
        }
        let station_flow = station_flow(&mids, index);
        for (point, u) in [(left, 0.0f32), (right, 1.0f32)] {
            mesh.vertices.push(
                Vertex::new([point[0], point[1], height], [0.0, 0.0, 1.0], [u, v])
                    .with_uv2(station_flow)
                    .with_tangent([tangent[0], tangent[1], 0.0, 1.0]),
            );
        }
    }

    for index in 0..sections.saturating_sub(1) {
        let base = u32::try_from(index.saturating_mul(2)).unwrap_or(u32::MAX);
        let (Some(l0), Some(r0), Some(l1), Some(r1)) = (
            Some(base),
            base.checked_add(1),
            base.checked_add(2),
            base.checked_add(3),
        ) else {
            continue;
        };
        mesh.indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
        if let (Some((a0, b0)), Some((a1, b1))) = (
            bank_at(area, index, flip),
            bank_at(area, index.saturating_add(1), flip),
        ) {
            for corners in [[a0, b0, b1], [a0, b1, a1]] {
                if triangle_double_area(corners) < 0.0 {
                    stats.backwards_triangles = stats.backwards_triangles.saturating_add(1);
                }
            }
        }
    }

    let (Some(first), Some(last)) = (
        bank_at(area, 0, flip),
        bank_at(area, sections.saturating_sub(1), flip),
    ) else {
        return Err(RiverSkip::Degenerate);
    };
    let start = [
        (first.0[0] + first.1[0]) * 0.5,
        (first.0[1] + first.1[1]) * 0.5,
    ];
    let end = [(last.0[0] + last.1[0]) * 0.5, (last.0[1] + last.1[1]) * 0.5];
    let span = mint_math::f32::hypot(end[0] - start[0], end[1] - start[1]);
    let flow = if span > 0.0 {
        [(end[0] - start[0]) / span, (end[1] - start[1]) / span]
    } else {
        [0.0, 0.0]
    };

    Ok(RiverRibbon {
        unique_id: area.unique_id,
        name: area.name.clone(),
        height,
        additive_blend: area.additive_blend,
        river_texture: area.river_texture.clone(),
        noise_texture: area.noise_texture.clone(),
        alpha_edge_texture: area.alpha_edge_texture.clone(),
        sparkle_texture: area.sparkle_texture.clone(),
        tint: [
            f32::from(area.color[0]) / 255.0,
            f32::from(area.color[1]) / 255.0,
            f32::from(area.color[2]) / 255.0,
            if area.alpha.is_finite() {
                area.alpha.clamp(0.0, 1.0)
            } else {
                1.0
            },
        ],
        uv_scroll_speed: area.uv_scroll_speed,
        minimum_water_lod: area.minimum_water_lod.clone(),
        mesh,
        sections,
        length: total,
        width,
        repeats,
        flow,
        centre: mids
            .get(mids.len() / 2)
            .copied()
            .unwrap_or([(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5]),
    })
}

#[must_use]
pub fn build_rivers(areas: &[RiverArea]) -> RiverNetwork {
    let mut network = RiverNetwork {
        ribbons: Vec::new(),
        stats: RiverStats {
            areas: areas.len(),
            ..RiverStats::default()
        },
    };
    for area in areas {
        let mut found = RiverStats::default();
        match build_ribbon(area, &mut found) {
            Err(reason) => {
                let slot = network.stats.skipped.entry(reason).or_insert(0);
                *slot = slot.saturating_add(1);
            }
            Ok(ribbon) => {
                let totals = &mut network.stats;
                totals.drawn = totals.drawn.saturating_add(1);
                totals.sections = totals.sections.saturating_add(ribbon.sections);
                totals.vertices = totals.vertices.saturating_add(ribbon.mesh.vertices.len());
                totals.triangles = totals
                    .triangles
                    .saturating_add(ribbon.mesh.triangle_count());
                totals.flipped = totals.flipped.saturating_add(found.flipped);
                totals.backwards_triangles = totals
                    .backwards_triangles
                    .saturating_add(found.backwards_triangles);
                totals.zero_width_sections = totals
                    .zero_width_sections
                    .saturating_add(found.zero_width_sections);
                totals.fallback_width = totals.fallback_width.saturating_add(found.fallback_width);
                if ribbon.additive_blend {
                    totals.additive = totals.additive.saturating_add(1);
                }
                network.ribbons.push(ribbon);
            }
        }
    }
    network
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_map::RiverLine;

    fn river(lines: Vec<RiverLine>, height: u32) -> RiverArea {
        RiverArea {
            unique_id: 1,
            name: "New River".to_string(),
            layer_name: String::new(),
            uv_scroll_speed: 0.06,
            additive_blend: true,
            river_texture: "TWWater01.tga".to_string(),
            noise_texture: "Noise0000.tga".to_string(),
            alpha_edge_texture: "TWAlphaEdge.tga".to_string(),
            sparkle_texture: "WaterSurfaceBubbles.tga".to_string(),
            color: [255, 255, 255],
            reserved: 0,
            alpha: 1.0,
            water_height: height,
            minimum_water_lod: "Medium".to_string(),
            lines,
        }
    }

    fn straight() -> Vec<RiverLine> {
        (0i16..5)
            .map(|i| {
                let x = f32::from(i) * 100.0;
                RiverLine {
                    a: [x, 50.0],
                    b: [x, -50.0],
                }
            })
            .collect()
    }

    fn double_areas(mesh: &MeshData) -> Vec<f32> {
        mesh.indices
            .chunks_exact(3)
            .map(|t| {
                let p = |slot: usize| {
                    let v = &mesh.vertices[t[slot] as usize];
                    [v.position[0], v.position[1]]
                };
                let (a, b, c) = (p(0), p(1), p(2));
                (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
            })
            .collect()
    }

    #[test]
    fn a_straight_river_is_a_forward_wound_strip_with_u_across_and_v_along() {
        let network = build_rivers(&[river(straight(), 42)]);
        assert_eq!(network.stats.drawn, 1);
        assert_eq!(network.stats.sections, 5);
        assert_eq!(network.stats.vertices, 10);
        assert_eq!(network.stats.triangles, 8, "2 per quad, 4 quads");
        assert_eq!(network.stats.backwards_triangles, 0);
        assert_eq!(network.stats.additive, 1);

        let ribbon = &network.ribbons[0];
        assert_eq!(ribbon.height, 42.0);
        assert!((ribbon.length - 400.0).abs() < 1e-3);
        assert!((ribbon.width - 100.0).abs() < 1e-3);
        assert!((ribbon.repeats - 4.0).abs() < 1e-4);
        assert!((ribbon.flow[0] - 1.0).abs() < 1e-5 && ribbon.flow[1].abs() < 1e-5);

        let areas = double_areas(&ribbon.mesh);
        assert!(areas.iter().all(|a| *a > 0.0), "{areas:?}");
        let covered: f32 = areas.iter().sum::<f32>() / 2.0;
        assert!((covered - 400.0 * 100.0).abs() < 1.0, "covered {covered}");

        for vertex in &ribbon.mesh.vertices {
            assert_eq!(vertex.position[2], 42.0, "flat at the stored height");
            assert!(vertex.uv[0] == 0.0 || vertex.uv[0] == 1.0, "u spans 0..1");
        }
        assert!((ribbon.mesh.vertices[8].uv[1] - 4.0).abs() < 1e-4);
        assert!(ribbon.mesh.validate().is_none(), "indices must be in range");
    }

    #[test]
    fn a_river_stored_bank_first_is_flipped_rather_than_wound_backwards() {
        let mirrored: Vec<RiverLine> = straight()
            .into_iter()
            .map(|line| RiverLine {
                a: line.b,
                b: line.a,
            })
            .collect();
        let network = build_rivers(&[river(mirrored, 7)]);
        assert_eq!(network.stats.flipped, 1);
        let areas = double_areas(&network.ribbons[0].mesh);
        assert!(areas.iter().all(|a| *a > 0.0), "{areas:?}");
    }

    #[test]
    fn degenerate_rivers_are_skipped_by_reason_and_never_panic() {
        let single = vec![RiverLine {
            a: [0.0, 0.0],
            b: [1.0, 0.0],
        }];
        let nan = vec![
            RiverLine {
                a: [0.0, f32::NAN],
                b: [1.0, 0.0],
            },
            RiverLine {
                a: [0.0, 1.0],
                b: [1.0, 1.0],
            },
        ];
        let stuck = vec![
            RiverLine {
                a: [5.0, 0.0],
                b: [5.0, 2.0],
            };
            4
        ];
        let network = build_rivers(&[
            river(Vec::new(), 1),
            river(single, 1),
            river(nan, 1),
            river(stuck, 1),
            river(
                vec![
                    RiverLine {
                        a: [0.0, 0.0],
                        b: [0.0, 0.0]
                    };
                    MAX_RIVER_SECTIONS + 1
                ],
                1,
            ),
        ]);
        assert_eq!(network.stats.areas, 5);
        assert_eq!(network.stats.drawn, 0);
        assert_eq!(network.stats.skipped[&RiverSkip::TooFewSections], 2);
        assert_eq!(network.stats.skipped[&RiverSkip::NonFinite], 1);
        assert_eq!(network.stats.skipped[&RiverSkip::Degenerate], 1);
        assert_eq!(network.stats.skipped[&RiverSkip::TooManySections], 1);
    }

    #[test]
    fn a_pinched_station_is_counted_and_does_not_divide_by_zero() {
        let mut lines = straight();
        lines[0] = RiverLine {
            a: [0.0, 0.0],
            b: [0.0, 0.0],
        };
        let network = build_rivers(&[river(lines, 3)]);
        assert_eq!(network.stats.drawn, 1);
        assert_eq!(network.stats.zero_width_sections, 1);
        let ribbon = &network.ribbons[0];
        assert!(ribbon.width > 0.0 && ribbon.width.is_finite());
        assert!(ribbon.repeats.is_finite() && ribbon.repeats > 0.0);
        assert!(ribbon
            .mesh
            .vertices
            .iter()
            .all(|v| v.uv[1].is_finite() && v.uv[0].is_finite()));
    }

    #[test]
    fn the_scroll_runs_along_v_only_and_wraps_at_one_repeat() {
        let quarter = river_scroll_offset(0.06, 5.0);
        assert_eq!(quarter[0], 0.0, "u never moves across a ribbon");
        assert!((quarter[1] - 0.7).abs() < 1e-5, "{quarter:?}");

        let whole = river_scroll_offset(0.06, 1.0 / 0.06);
        assert!(whole[1] < 1e-4, "{whole:?}");

        assert_eq!(river_scroll_offset(0.0, 1e6), [0.0, 0.0]);
        for speed in [f32::NAN, f32::INFINITY, -f32::INFINITY, 1e30] {
            assert_eq!(river_scroll_offset(speed, 1e30), [0.0, 0.0]);
        }
        for seconds in [1.0, 1e6, 1e12] {
            let offset = river_scroll_offset(0.06, seconds);
            assert!((0.0..1.0).contains(&offset[1]), "{offset:?} at {seconds}");
        }
    }
}
