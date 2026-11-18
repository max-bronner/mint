use mint_render::{Ray, Vec3};
use mint_sage_map::{HeightMapData, MAP_XY_FACTOR};

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn floor_index(value: f32, limit: u32) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let floored = value.floor();
    if floored >= f32::from(u16::MAX) {
        return None;
    }
    let index = floored as u32;
    (index < limit).then_some(index)
}

fn finite_scale(vertical_scale: f32) -> f32 {
    if vertical_scale.is_finite() {
        vertical_scale
    } else {
        0.0
    }
}

const PICK_STEP: f32 = MAP_XY_FACTOR / 4.0;

const PICK_STEPS: u32 = 24_000;

const PICK_BISECTIONS: u32 = 20;

#[must_use]
pub fn pick_surface(height_map: &HeightMapData, vertical_scale: f32, ray: Ray) -> Option<Vec3> {
    if !ray.origin.is_finite() || !ray.direction.is_finite() {
        return None;
    }
    let clearance = |point: Vec3| {
        surface_height(height_map, vertical_scale, point.x, point.y).map(|ground| point.z - ground)
    };

    let mut previous: Option<(f32, f32)> = None;
    for step in 0..PICK_STEPS {
        let t =
            f32::from(u16::try_from(step.min(u32::from(u16::MAX))).unwrap_or(u16::MAX)) * PICK_STEP;
        let Some(gap) = clearance(ray.at(t)) else {
            previous = None;
            continue;
        };
        if let Some((last_t, last_gap)) = previous {
            if last_gap > 0.0 && gap <= 0.0 {
                let (mut lo, mut hi) = (last_t, t);
                for _ in 0..PICK_BISECTIONS {
                    let mid = (lo + hi) * 0.5;
                    match clearance(ray.at(mid)) {
                        Some(g) if g > 0.0 => lo = mid,
                        Some(_) => hi = mid,
                        None => break,
                    }
                }
                return Some(ray.at((lo + hi) * 0.5));
            }
        }
        previous = Some((t, gap));
    }
    None
}

#[must_use]
pub fn surface_height(
    height_map: &HeightMapData,
    vertical_scale: f32,
    wx: f32,
    wy: f32,
) -> Option<f32> {
    let border = f32::from(u16::try_from(height_map.border_size).ok()?);
    let gx = wx / MAP_XY_FACTOR + border;
    let gy = wy / MAP_XY_FACTOR + border;
    if !gx.is_finite() || !gy.is_finite() || gx < 0.0 || gy < 0.0 {
        return None;
    }
    let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
    let x0 = floor_index(gx, height_map.width)?;
    let y0 = floor_index(gy, height_map.height)?;
    let sample = |dx: u32, dy: u32| {
        height_map
            .sample(
                x0.saturating_add(dx)
                    .min(height_map.width.saturating_sub(1)),
                y0.saturating_add(dy)
                    .min(height_map.height.saturating_sub(1)),
            )
            .map(f32::from)
    };
    let (h00, h10, h01, h11) = (sample(0, 0)?, sample(1, 0)?, sample(0, 1)?, sample(1, 1)?);
    let top = h00 + (h10 - h00) * fx;
    let bottom = h01 + (h11 - h01) * fx;
    Some((top + (bottom - top) * fy) * finite_scale(vertical_scale))
}

#[must_use]
pub fn drawn_surface_height(
    height_map: &HeightMapData,
    vertical_scale: f32,
    wx: f32,
    wy: f32,
) -> Option<f32> {
    let border = f32::from(u16::try_from(height_map.border_size).ok()?);
    let gx = wx / MAP_XY_FACTOR + border;
    let gy = wy / MAP_XY_FACTOR + border;
    if !gx.is_finite() || !gy.is_finite() || gx < 0.0 || gy < 0.0 {
        return None;
    }
    let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
    let x0 = floor_index(gx, height_map.width)?;
    let y0 = floor_index(gy, height_map.height)?;
    let sample = |dx: u32, dy: u32| {
        height_map
            .sample(
                x0.saturating_add(dx)
                    .min(height_map.width.saturating_sub(1)),
                y0.saturating_add(dy)
                    .min(height_map.height.saturating_sub(1)),
            )
            .map(f32::from)
    };
    let (a, b, c, d) = (sample(0, 0)?, sample(1, 0)?, sample(0, 1)?, sample(1, 1)?);
    let height = if crate::quad_folds_along_ad(height_map, x0, y0) {
        if fy <= fx {
            a + (b - a) * fx + (d - b) * fy
        } else {
            a + (d - c) * fx + (c - a) * fy
        }
    } else if fx + fy <= 1.0 {
        a + (b - a) * fx + (c - a) * fy
    } else {
        b + c - d + (d - c) * fx + (d - b) * fy
    };
    Some(height * finite_scale(vertical_scale))
}

#[must_use]
pub fn max_cell_height(
    height_map: &HeightMapData,
    vertical_scale: f32,
    wx: f32,
    wy: f32,
) -> Option<f32> {
    let border = f32::from(u16::try_from(height_map.border_size).ok()?);
    let gx = wx / MAP_XY_FACTOR + border;
    let gy = wy / MAP_XY_FACTOR + border;
    if !gx.is_finite() || !gy.is_finite() || gx < 0.0 || gy < 0.0 {
        return None;
    }
    let x0 = floor_index(gx, height_map.width)?;
    let y0 = floor_index(gy, height_map.height)?;
    let sample = |dx: u32, dy: u32| {
        height_map
            .sample(
                x0.saturating_add(dx)
                    .min(height_map.width.saturating_sub(1)),
                y0.saturating_add(dy)
                    .min(height_map.height.saturating_sub(1)),
            )
            .map(f32::from)
    };
    let (a, b, c, d) = (sample(0, 0)?, sample(1, 0)?, sample(0, 1)?, sample(1, 1)?);
    Some(a.max(b).max(c).max(d) * finite_scale(vertical_scale))
}

const NORMAL_PROBE_CELLS: f32 = 0.5;

#[must_use]
pub fn surface_normal(
    height_map: &HeightMapData,
    vertical_scale: f32,
    wx: f32,
    wy: f32,
) -> Option<Vec3> {
    let centre = surface_height(height_map, vertical_scale, wx, wy)?;
    let span = MAP_XY_FACTOR * NORMAL_PROBE_CELLS;
    let probe = |dx: f32, dy: f32| {
        surface_height(height_map, vertical_scale, wx + dx, wy + dy).unwrap_or(centre)
    };
    let dz_dx = (probe(span, 0.0) - probe(-span, 0.0)) / (span + span);
    let dz_dy = (probe(0.0, span) - probe(0.0, -span)) / (span + span);
    let normal = Vec3::new(-dz_dx, -dz_dy, 1.0).normalize();
    normal.is_finite().then_some(normal)
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn grid(width: u32, height: u32, border: u32, f: impl Fn(u32, u32) -> u16) -> HeightMapData {
        let mut samples = Vec::new();
        for y in 0..height {
            for x in 0..width {
                samples.push(f(x, y));
            }
        }
        HeightMapData {
            width,
            height,
            border_size: border,
            boundaries: Vec::new(),
            samples,
        }
    }

    #[test]
    fn a_ray_from_above_lands_on_the_ground_it_passed_through() {
        let hm = grid(16, 16, 0, |_, _| 1000);
        let ray = Ray {
            origin: Vec3::new(55.0, 55.0, 500.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        let hit = pick_surface(&hm, 0.04, ray).unwrap();
        assert!((hit.x - 55.0).abs() < 1e-3 && (hit.y - 55.0).abs() < 1e-3);
        assert!((hit.z - 40.0).abs() < 1e-3, "{hit:?}");
    }

    #[test]
    fn an_oblique_ray_hits_a_point_that_is_on_both_the_ray_and_the_surface() {
        let hm = grid(24, 24, 0, |x, _| u16::try_from(x * 200).unwrap());
        let ray = Ray {
            origin: Vec3::new(10.0, 60.0, 200.0),
            direction: Vec3::new(0.6, 0.0, -0.8),
        };
        let hit = pick_surface(&hm, 0.05, ray).unwrap();
        let ground = surface_height(&hm, 0.05, hit.x, hit.y).unwrap();
        assert!((hit.z - ground).abs() < 1e-2, "{hit:?} vs ground {ground}");
        let t = (hit.x - ray.origin.x) / ray.direction.x;
        assert!((ray.at(t).z - hit.z).abs() < 1e-2, "{hit:?} is off the ray");
    }

    #[test]
    fn a_ray_into_the_sky_hits_nothing() {
        let hm = grid(16, 16, 0, |_, _| 1000);
        let ray = Ray {
            origin: Vec3::new(55.0, 55.0, 500.0),
            direction: Vec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(pick_surface(&hm, 0.04, ray), None);
    }

    #[test]
    fn a_ray_that_leaves_the_grid_sideways_hits_nothing() {
        let hm = grid(16, 16, 0, |_, _| 1000);
        let ray = Ray {
            origin: Vec3::new(55.0, 55.0, 500.0),
            direction: Vec3::new(1.0, 0.0, -0.02),
        };
        assert_eq!(pick_surface(&hm, 0.04, ray), None);
    }

    #[test]
    fn a_non_finite_ray_is_refused_rather_than_marched() {
        let hm = grid(8, 8, 0, |_, _| 1000);
        let ray = Ray {
            origin: Vec3::new(f32::NAN, 5.0, 500.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(pick_surface(&hm, 0.04, ray), None);
    }

    #[test]
    fn the_cell_maximum_is_never_below_the_drawn_surface_inside_the_cell() {
        let hm = HeightMapData {
            width: 3,
            height: 3,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![10, 90, 30, 20, 40, 80, 60, 15, 70],
        };
        let step = MAP_XY_FACTOR / 16.0;
        let mut probes = 0usize;
        for i in 0..=32 {
            for j in 0..=32 {
                let (x, y) = (i as f32 * step, j as f32 * step);
                let (Some(top), Some(drawn)) = (
                    max_cell_height(&hm, 1.0, x, y),
                    drawn_surface_height(&hm, 1.0, x, y),
                ) else {
                    continue;
                };
                probes += 1;
                assert!(top >= drawn - 1e-3, "at ({x}, {y}): {top} < {drawn}");
            }
        }
        assert!(
            probes > 900,
            "the sweep should cover the grid, got {probes}"
        );
    }

    #[test]
    fn a_point_mid_cell_interpolates_between_its_corners() {
        let hm = grid(4, 4, 0, |x, _| u16::try_from(x * 100).unwrap());
        let at = |wx: f32| surface_height(&hm, 1.0, wx, 0.0).unwrap();
        assert_eq!(at(0.0), 0.0);
        assert_eq!(at(10.0), 100.0);
        assert!((at(12.5) - 125.0).abs() < 1e-3);
        assert!((at(15.0) - 150.0).abs() < 1e-3);
    }

    #[test]
    fn the_drawn_height_follows_the_fold_where_the_bilinear_one_cuts_across_it() {
        let hm = grid(2, 2, 0, |x, y| match (x, y) {
            (0, 0) => 50,
            (1, 0) => 0,
            (0, 1) => 60,
            _ => 50,
        });
        assert!(crate::quad_folds_along_ad(&hm, 0, 0));
        let drawn = drawn_surface_height(&hm, 1.0, 5.0, 5.0).unwrap();
        let bilinear = surface_height(&hm, 1.0, 5.0, 5.0).unwrap();
        assert!((drawn - 50.0).abs() < 1e-3, "{drawn}");
        assert!((bilinear - 40.0).abs() < 1e-3, "{bilinear}");

        for (wx, wy, want) in [(0.0, 0.0, 50.0), (10.0, 0.0, 0.0), (0.0, 10.0, 60.0)] {
            let drawn = drawn_surface_height(&hm, 1.0, wx, wy).unwrap();
            assert!((drawn - want).abs() < 1e-3, "{wx},{wy} gave {drawn}");
        }

        assert_eq!(drawn_surface_height(&hm, 1.0, -1.0, 0.0), None);
        assert_eq!(drawn_surface_height(&hm, 1.0, f32::NAN, 0.0), None);
    }

    #[test]
    fn the_border_offsets_the_world_origin() {
        let hm = grid(5, 5, 2, |x, _| u16::try_from(x * 10).unwrap());
        assert_eq!(surface_height(&hm, 1.0, 0.0, 0.0), Some(20.0));
        assert_eq!(surface_height(&hm, 1.0, -20.0, 0.0), Some(0.0));
    }

    #[test]
    fn a_point_off_the_grid_is_none_rather_than_clamped() {
        let hm = grid(4, 4, 0, |_, _| 7);
        assert_eq!(surface_height(&hm, 1.0, -0.1, 0.0), None);
        assert_eq!(surface_height(&hm, 1.0, 0.0, -0.1), None);
        assert_eq!(surface_height(&hm, 1.0, 40.0, 0.0), None);
        assert_eq!(surface_normal(&hm, 1.0, 40.0, 0.0), None);
    }

    #[test]
    fn level_ground_has_a_straight_up_normal() {
        let hm = grid(6, 6, 0, |_, _| 1234);
        let n = surface_normal(&hm, 0.04, 25.0, 25.0).unwrap();
        assert!(n.x.abs() < 1e-6);
        assert!(n.y.abs() < 1e-6);
        assert!((n.z - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_slope_tilts_the_normal_downhill() {
        let hm = grid(6, 6, 0, |x, _| u16::try_from(x * 100).unwrap());
        let n = surface_normal(&hm, 0.1, 25.0, 25.0).unwrap();
        assert!(n.x < 0.0, "uphill is +x, so the normal leans -x");
        assert!((n.x + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!((n.z - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!(n.y.abs() < 1e-6);
    }

    #[test]
    fn a_non_finite_scale_does_not_poison_the_height() {
        let hm = grid(4, 4, 0, |_, _| 100);
        assert_eq!(surface_height(&hm, f32::NAN, 5.0, 5.0), Some(0.0));
    }
}
