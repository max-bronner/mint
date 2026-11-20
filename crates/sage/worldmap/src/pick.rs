use std::sync::Arc;

use mint_render::{Mat4, Ray, Vec3};
use mint_sage_livingworld::{ArmyId, RegionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapPick {
    Army { army: ArmyId, region: RegionId },
    Region(RegionId),
}

#[derive(Debug, Clone)]
pub struct IconPick {
    pub army: ArmyId,
    pub region: RegionId,
    pub triangles: Arc<[[Vec3; 3]]>,
    pub centre: Vec3,
    pub placement: Mat4,
}

impl IconPick {
    #[must_use]
    pub fn new(
        army: ArmyId,
        region: RegionId,
        triangles: Arc<[[Vec3; 3]]>,
        placement: Mat4,
    ) -> Self {
        let corners = triangles.iter().flatten();
        let (lo, hi) = corners.fold(
            (
                Vec3::new(f32::MAX, f32::MAX, f32::MAX),
                Vec3::new(f32::MIN, f32::MIN, f32::MIN),
            ),
            |(lo, hi), p| {
                (
                    Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
                    Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
                )
            },
        );
        let centre = if triangles.is_empty() {
            Vec3::new(0.0, 0.0, 0.0)
        } else {
            Vec3::new(
                (lo.x + hi.x) * 0.5,
                (lo.y + hi.y) * 0.5,
                (lo.z + hi.z) * 0.5,
            )
        };
        Self {
            army,
            region,
            triangles,
            centre,
            placement,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RegionPicker {
    triangles: Vec<(RegionId, [Vec3; 3])>,
}

impl RegionPicker {
    pub fn add_mesh(&mut self, region: RegionId, positions: &[Vec3], indices: &[u32]) {
        for tri in indices.chunks_exact(3) {
            let corner = |i: u32| positions.get(usize::try_from(i).ok()?).copied();
            if let [a, b, c] = tri {
                if let (Some(a), Some(b), Some(c)) = (corner(*a), corner(*b), corner(*c)) {
                    self.triangles.push((region, [a, b, c]));
                }
            }
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.triangles.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty()
    }

    #[must_use]
    pub fn pick(&self, ray: Ray) -> Option<(RegionId, f32)> {
        let mut best: Option<(RegionId, f32)> = None;
        for (region, tri) in &self.triangles {
            if let Some(t) = hit(ray, tri) {
                if best.is_none_or(|(_, b)| t < b) {
                    best = Some((*region, t));
                }
            }
        }
        best
    }

    #[must_use]
    pub fn pick_map(&self, ray: Ray, icons: &[IconPick]) -> Option<MapPick> {
        let icon = icons
            .iter()
            .filter_map(|b| hit_icon(ray, b).map(|t| (b, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let region = self.pick(ray);
        match (icon, region) {
            (Some((b, t)), Some((_, rt))) if t <= rt => Some(MapPick::Army {
                army: b.army,
                region: b.region,
            }),
            (Some((b, _)), None) => Some(MapPick::Army {
                army: b.army,
                region: b.region,
            }),
            (_, Some((r, _))) => Some(MapPick::Region(r)),
            (None, None) => None,
        }
    }

    #[must_use]
    pub fn ground(&self, x: f32, y: f32) -> Option<f32> {
        let top = 100_000.0;
        let ray = Ray {
            origin: Vec3::new(x, y, top),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        self.pick(ray).map(|(_, t)| top - t)
    }

    #[must_use]
    pub fn ground_in(&self, region: RegionId, x: f32, y: f32) -> Option<f32> {
        let top = 100_000.0;
        let ray = Ray {
            origin: Vec3::new(x, y, top),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        self.triangles
            .iter()
            .filter(|(r, _)| *r == region)
            .filter_map(|(_, tri)| hit(ray, tri))
            .min_by(f32::total_cmp)
            .map(|t| top - t)
    }

    #[must_use]
    pub fn drape(
        &self,
        points: &[[f32; 2]],
        start: Option<RegionId>,
        end: Option<RegionId>,
        bias: f32,
    ) -> Vec<[f32; 3]> {
        points
            .iter()
            .map(|p| {
                let ground = start
                    .and_then(|r| self.ground_in(r, p[0], p[1]))
                    .or_else(|| end.and_then(|r| self.ground_in(r, p[0], p[1])))
                    .or_else(|| self.ground(p[0], p[1]))
                    .unwrap_or(0.0);
                [p[0], p[1], ground + bias]
            })
            .collect()
    }
}

fn hit_icon(ray: Ray, icon: &IconPick) -> Option<f32> {
    let inverse = icon.placement.inverse()?;
    let local = Ray {
        origin: inverse.transform_point(ray.origin),
        direction: inverse.transform_vector(ray.direction),
    };
    icon.triangles
        .iter()
        .filter_map(|tri| hit(local, tri))
        .min_by(f32::total_cmp)
}

fn hit(ray: Ray, [a, b, c]: &[Vec3; 3]) -> Option<f32> {
    let e1 = *b - *a;
    let e2 = *c - *a;
    let p = ray.direction.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1.0e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = ray.origin - *a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = ray.direction.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(picker: &mut RegionPicker, region: u16, x0: f32, z: f32) {
        let p = [
            Vec3::new(x0, 0.0, z),
            Vec3::new(x0 + 10.0, 0.0, z),
            Vec3::new(x0 + 10.0, 10.0, z),
            Vec3::new(x0, 10.0, z),
        ];
        picker.add_mesh(RegionId(region), &p, &[0, 1, 2, 0, 2, 3, 0, 9, 1]);
    }

    #[test]
    fn a_ray_picks_the_region_under_it_and_the_nearer_of_two() {
        let mut picker = RegionPicker::default();
        square(&mut picker, 1, 0.0, 0.0);
        square(&mut picker, 2, 10.0, 0.0);
        square(&mut picker, 3, 10.0, 5.0);
        assert_eq!(picker.len(), 6);
        let down = |x: f32| Ray {
            origin: Vec3::new(x, 5.0, 100.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(picker.pick(down(3.0)).map(|h| h.0), Some(RegionId(1)));
        assert_eq!(picker.pick(down(13.0)).map(|h| h.0), Some(RegionId(3)));
        assert_eq!(picker.pick(down(30.0)), None);
        assert_eq!(picker.ground(13.0, 5.0), Some(5.0));
    }

    fn pick_box() -> Vec<[Vec3; 3]> {
        let v = |x, y, z| Vec3::new(x, y, z);
        vec![
            [v(-1.0, -1.0, 4.0), v(1.0, -1.0, 4.0), v(1.0, 1.0, 4.0)],
            [v(-1.0, -1.0, 4.0), v(1.0, 1.0, 4.0), v(-1.0, 1.0, 4.0)],
        ]
    }

    fn icon_of(army: u32, at: Vec3, triangles: Vec<[Vec3; 3]>) -> IconPick {
        IconPick {
            army: ArmyId(army),
            region: RegionId(1),
            triangles: triangles.into(),
            centre: Vec3::new(0.0, 0.0, 2.0),
            placement: Mat4::from_translation(at),
        }
    }

    fn icon(army: u32, at: Vec3) -> IconPick {
        icon_of(army, at, pick_box())
    }

    #[test]
    fn an_icon_is_cast_against_its_pick_model_not_the_bounds_around_it() {
        let v = |x, y, z| Vec3::new(x, y, z);
        let sliver = vec![[v(-1.0, -1.0, 4.0), v(1.0, -1.0, 4.0), v(-1.0, 1.0, 4.0)]];
        let icons = [icon_of(3, v(5.0, 5.0, 0.0), sliver)];
        let picker = RegionPicker::default();
        assert!(
            matches!(
                picker.pick_map(down(4.5), &icons),
                Some(MapPick::Army {
                    army: ArmyId(3),
                    ..
                })
            ),
            "inside the triangle"
        );
        assert_eq!(
            picker.pick_map(down(5.8), &icons),
            None,
            "inside the model's bounds but off every triangle"
        );
    }

    #[test]
    fn a_larger_pick_model_takes_a_click_the_visible_model_would_miss() {
        let v = |x, y, z| Vec3::new(x, y, z);
        let big = [
            [v(-3.0, -3.0, 4.0), v(3.0, -3.0, 4.0), v(3.0, 3.0, 4.0)],
            [v(-3.0, -3.0, 4.0), v(3.0, 3.0, 4.0), v(-3.0, 3.0, 4.0)],
        ];
        let at = v(5.0, 5.0, 0.0);
        let picker = RegionPicker::default();
        let small = [icon(4, at)];
        let large = [icon_of(4, at, big.to_vec())];
        assert_eq!(picker.pick_map(down(7.5), &small), None);
        assert!(picker.pick_map(down(7.5), &large).is_some());
    }

    #[test]
    fn an_icon_scaled_and_turned_by_its_placement_casts_in_its_own_space() {
        let spun = IconPick {
            placement: Mat4::from_cols([
                [0.0, 2.0, 0.0, 0.0],
                [-2.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 2.0, 0.0],
                [5.0, 5.0, 0.0, 1.0],
            ]),
            ..icon(5, Vec3::new(0.0, 0.0, 0.0))
        };
        let picker = RegionPicker::default();
        assert!(picker
            .pick_map(down(6.9), std::slice::from_ref(&spun))
            .is_some());
        assert_eq!(picker.pick_map(down(7.1), &[spun]), None);
    }

    fn down(x: f32) -> Ray {
        Ray {
            origin: Vec3::new(x, 5.0, 100.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        }
    }

    #[test]
    fn an_icon_in_front_of_a_region_wins_and_a_ray_beside_it_picks_the_region() {
        let mut picker = RegionPicker::default();
        square(&mut picker, 1, 0.0, 0.0);
        let icons = [icon(7, Vec3::new(5.0, 5.0, 0.0))];
        assert_eq!(
            picker.pick_map(down(5.5), &icons),
            Some(MapPick::Army {
                army: ArmyId(7),
                region: RegionId(1)
            })
        );
        assert_eq!(
            picker.pick_map(down(8.0), &icons),
            Some(MapPick::Region(RegionId(1)))
        );
    }

    #[test]
    fn the_nearer_of_two_overlapping_icons_wins() {
        let mut picker = RegionPicker::default();
        square(&mut picker, 1, 0.0, 0.0);
        let icons = [
            icon(1, Vec3::new(5.0, 5.0, 0.0)),
            icon(2, Vec3::new(5.0, 5.0, 3.0)),
        ];
        let army = |p| match p {
            Some(MapPick::Army { army, .. }) => Some(army.0),
            _ => None,
        };
        assert_eq!(army(picker.pick_map(down(5.0), &icons)), Some(2));
        assert_eq!(
            army(picker.pick_map(down(5.0), &[icons[1].clone(), icons[0].clone()])),
            Some(2)
        );
    }

    #[test]
    fn a_draped_point_takes_the_start_regions_ground_then_the_ends_plus_the_bias() {
        let mut picker = RegionPicker::default();
        square(&mut picker, 1, 0.0, 0.0);
        square(&mut picker, 2, 0.0, 7.0);
        let one = [[5.0, 5.0]];
        assert_eq!(
            picker.drape(&one, Some(RegionId(1)), Some(RegionId(2)), 0.5),
            [[5.0, 5.0, 0.5]]
        );
        assert_eq!(
            picker.drape(&one, Some(RegionId(3)), Some(RegionId(2)), 0.5),
            [[5.0, 5.0, 7.5]]
        );
        assert_eq!(
            picker.drape(&[[50.0, 5.0]], None, None, 0.5),
            [[50.0, 5.0, 0.5]]
        );
    }
}
