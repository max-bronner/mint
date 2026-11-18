use crate::math::{Mat4, Vec3};
use crate::mesh::Aabb;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    planes: [[f32; 4]; 6],
}

impl Frustum {
    #[must_use]
    pub fn from_view_proj(m: Mat4) -> Self {
        let row = |i: usize| [m.get(i, 0), m.get(i, 1), m.get(i, 2), m.get(i, 3)];
        let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
        let add = |a: [f32; 4], b: [f32; 4]| [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]];
        let sub = |a: [f32; 4], b: [f32; 4]| [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]];
        Self {
            planes: [
                add(r3, r0),
                sub(r3, r0),
                add(r3, r1),
                sub(r3, r1),
                r2,
                sub(r3, r2),
            ],
        }
    }

    #[must_use]
    pub fn intersects(&self, bounds: &Aabb) -> bool {
        if bounds.is_empty() {
            return true;
        }
        let (min, max) = (bounds.min(), bounds.max());
        for plane in &self.planes {
            let [a, b, c, d] = *plane;
            let x = if a >= 0.0 { max.x } else { min.x };
            let y = if b >= 0.0 { max.y } else { min.y };
            let z = if c >= 0.0 { max.z } else { min.z };
            if a * x + b * y + c * z + d < 0.0 {
                return false;
            }
        }
        true
    }

    #[must_use]
    pub fn sphere_within_sides(&self, centre: Vec3, radius: f32) -> bool {
        for plane in self.planes.iter().take(4) {
            let [a, b, c, d] = *plane;
            let length = (a * a + b * b + c * c).sqrt();
            if length > 0.0 && (a * centre.x + b * centre.y + c * centre.z + d) / length < -radius {
                return false;
            }
        }
        true
    }
}

#[must_use]
pub fn transformed_bounds(bounds: &Aabb, transform: Mat4) -> Aabb {
    if bounds.is_empty() {
        return Aabb::EMPTY;
    }
    let (min, max) = (bounds.min(), bounds.max());
    let center = Vec3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    let extent = Vec3::new(
        (max.x - min.x) * 0.5,
        (max.y - min.y) * 0.5,
        (max.z - min.z) * 0.5,
    );
    let moved = transform.transform_point(center);
    let axis = |r: usize| {
        transform.get(r, 0).abs() * extent.x
            + transform.get(r, 1).abs() * extent.y
            + transform.get(r, 2).abs() * extent.z
    };
    let spread = Vec3::new(axis(0), axis(1), axis(2));
    Aabb::from_corners(
        Vec3::new(moved.x - spread.x, moved.y - spread.y, moved.z - spread.z),
        Vec3::new(moved.x + spread.x, moved.y + spread.y, moved.z + spread.z),
    )
}

const FRUSTUM_FACES: [[usize; 4]; 6] = [
    [0, 1, 3, 2],
    [4, 5, 7, 6],
    [0, 2, 6, 4],
    [1, 3, 7, 5],
    [0, 1, 5, 4],
    [2, 3, 7, 6],
];

const MAX_FACE_VERTICES: usize = 4 + 6;

const MAX_REGION_VERTICES: usize = 6 * MAX_FACE_VERTICES + 8;

#[derive(Debug, Clone, Copy)]
pub(crate) struct VisibleRegion {
    points: [Vec3; MAX_REGION_VERTICES],
    count: usize,
}

impl VisibleRegion {
    pub(crate) fn points(&self) -> &[Vec3] {
        self.points.get(..self.count).unwrap_or(&[])
    }
}

pub(crate) fn visible_region(corners: &[Vec3; 8], bounds: &Aabb) -> Option<VisibleRegion> {
    if bounds.is_empty() {
        return None;
    }
    let (lo, hi) = (bounds.min(), bounds.max());
    if !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    let mut region = VisibleRegion {
        points: [Vec3::ZERO; MAX_REGION_VERTICES],
        count: 0,
    };

    let mut front = [Vec3::ZERO; MAX_FACE_VERTICES];
    let mut back = [Vec3::ZERO; MAX_FACE_VERTICES];
    for face in FRUSTUM_FACES {
        let mut len = 0usize;
        for index in face {
            if let (Some(corner), Some(slot)) = (corners.get(index), front.get_mut(len)) {
                *slot = *corner;
                len = len.saturating_add(1);
            }
        }
        for axis in 0..3usize {
            for (limit, keep_greater) in [(component(lo, axis), true), (component(hi, axis), false)]
            {
                len = clip_face(
                    front.get(..len).unwrap_or(&[]),
                    axis,
                    limit,
                    keep_greater,
                    &mut back,
                );
                std::mem::swap(&mut front, &mut back);
            }
        }
        for point in front.get(..len).unwrap_or(&[]) {
            region.push(*point);
        }
    }

    let mut interior = Vec3::ZERO;
    for corner in corners {
        interior = Vec3::new(
            interior.x + corner.x * 0.125,
            interior.y + corner.y * 0.125,
            interior.z + corner.z * 0.125,
        );
    }
    let planes = FRUSTUM_FACES.map(|face| face_plane(corners, face, interior));
    for index in 0..8usize {
        let corner = Vec3::new(
            if index & 1 == 0 { lo.x } else { hi.x },
            if index & 2 == 0 { lo.y } else { hi.y },
            if index & 4 == 0 { lo.z } else { hi.z },
        );
        let inside = planes
            .iter()
            .flatten()
            .all(|[a, b, c, d]| a * corner.x + b * corner.y + c * corner.z + d >= 0.0);
        if inside {
            region.push(corner);
        }
    }

    if region.count == 0 {
        None
    } else {
        Some(region)
    }
}

impl VisibleRegion {
    fn push(&mut self, point: Vec3) {
        if !point.is_finite() {
            return;
        }
        if let Some(slot) = self.points.get_mut(self.count) {
            *slot = point;
            self.count = self.count.saturating_add(1);
        }
    }
}

fn component(v: Vec3, axis: usize) -> f32 {
    match axis {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

fn clip_face(
    poly: &[Vec3],
    axis: usize,
    limit: f32,
    keep_greater: bool,
    out: &mut [Vec3; MAX_FACE_VERTICES],
) -> usize {
    let inside = |point: Vec3| {
        let value = component(point, axis);
        if keep_greater {
            value >= limit
        } else {
            value <= limit
        }
    };
    let mut count = 0usize;
    let mut write = |point: Vec3, count: &mut usize| {
        if let Some(slot) = out.get_mut(*count) {
            *slot = point;
            *count = count.saturating_add(1);
        }
    };
    let last = poly.len().saturating_sub(1);
    for (index, current) in poly.iter().copied().enumerate() {
        let previous_index = if index == 0 {
            last
        } else {
            index.saturating_sub(1)
        };
        let Some(previous) = poly.get(previous_index).copied() else {
            continue;
        };
        let (here, there) = (inside(current), inside(previous));
        if here != there {
            let (from, to) = (component(previous, axis), component(current, axis));
            let span = to - from;
            if span != 0.0 && span.is_finite() {
                let t = (limit - from) / span;
                write(
                    Vec3::new(
                        previous.x + (current.x - previous.x) * t,
                        previous.y + (current.y - previous.y) * t,
                        previous.z + (current.z - previous.z) * t,
                    ),
                    &mut count,
                );
            }
        }
        if here {
            write(current, &mut count);
        }
    }
    count
}

fn face_plane(corners: &[Vec3; 8], face: [usize; 4], interior: Vec3) -> Option<[f32; 4]> {
    let mut normal = Vec3::ZERO;
    let mut anchor = Vec3::ZERO;
    for (position, index) in face.iter().enumerate() {
        let after = position.saturating_add(1);
        let next = face.get(if after < face.len() { after } else { 0 })?;
        let (a, b) = (corners.get(*index)?, corners.get(*next)?);
        normal = Vec3::new(
            normal.x + (a.y - b.y) * (a.z + b.z),
            normal.y + (a.z - b.z) * (a.x + b.x),
            normal.z + (a.x - b.x) * (a.y + b.y),
        );
        anchor = *a;
    }
    if !normal.is_finite() || normal.length_squared() <= f32::EPSILON {
        return None;
    }
    let towards = Vec3::new(
        interior.x - anchor.x,
        interior.y - anchor.y,
        interior.z - anchor.z,
    );
    let normal = if normal.dot(towards) < 0.0 {
        Vec3::new(-normal.x, -normal.y, -normal.z)
    } else {
        normal
    };
    Some([normal.x, normal.y, normal.z, -normal.dot(anchor)])
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::camera::Camera;
    use crate::depth::DepthDirection;

    fn box_at(center: Vec3, half: f32) -> Aabb {
        Aabb::from_corners(
            Vec3::new(center.x - half, center.y - half, center.z - half),
            Vec3::new(center.x + half, center.y + half, center.z + half),
        )
    }

    fn camera_frustum(direction: DepthDirection) -> Frustum {
        let camera = Camera {
            eye: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            near: 1.0,
            far: 100.0,
            ..Camera::default()
        };
        Frustum::from_view_proj(
            camera
                .projection_with_depth(direction, 1.0)
                .mul_mat4(camera.view()),
        )
    }

    fn corners_of(near: f32, far: f32) -> [Vec3; 8] {
        let camera = Camera {
            eye: Vec3::ZERO,
            target: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            near,
            far,
            ..Camera::default()
        };
        camera.frustum_corners(1.0, near, far)
    }

    fn region_bounds(region: &VisibleRegion) -> Aabb {
        let mut out = Aabb::EMPTY;
        for point in region.points() {
            out.insert(*point);
        }
        out
    }

    #[test]
    fn a_frustum_inside_the_box_is_returned_whole() {
        let corners = corners_of(1.0, 10.0);
        let world = box_at(Vec3::ZERO, 1000.0);
        let region = visible_region(&corners, &world).expect("the two overlap");
        for corner in corners {
            assert!(
                region.points().iter().any(|p| (p.x - corner.x).abs() < 1e-3
                    && (p.y - corner.y).abs() < 1e-3
                    && (p.z - corner.z).abs() < 1e-3),
                "corner {corner:?} was dropped"
            );
        }
    }

    #[test]
    fn a_box_inside_the_frustum_is_returned_as_its_own_corners() {
        let corners = corners_of(1.0, 1000.0);
        let world = Aabb::from_corners(Vec3::new(-2.0, -2.0, -60.0), Vec3::new(2.0, 2.0, -50.0));
        let region = visible_region(&corners, &world).expect("the box is in view");
        let bounds = region_bounds(&region);
        assert!((bounds.min().x - world.min().x).abs() < 1e-3);
        assert!((bounds.max().z - world.max().z).abs() < 1e-3);
        assert!((bounds.min().z - world.min().z).abs() < 1e-3);
    }

    #[test]
    fn a_box_behind_the_camera_gives_nothing() {
        let corners = corners_of(1.0, 100.0);
        let world = Aabb::from_corners(Vec3::new(-10.0, -10.0, 50.0), Vec3::new(10.0, 10.0, 60.0));
        assert!(visible_region(&corners, &world).is_none());
    }

    #[test]
    fn an_empty_box_gives_nothing() {
        let corners = corners_of(1.0, 100.0);
        assert!(visible_region(&corners, &Aabb::EMPTY).is_none());
    }

    #[test]
    fn a_thin_world_slab_bounds_the_region_to_its_own_thickness() {
        let corners = corners_of(1.0, 10_000.0);
        let world = Aabb::from_corners(
            Vec3::new(-4000.0, -100.0, -4000.0),
            Vec3::new(4000.0, 100.0, 4000.0),
        );
        let region = visible_region(&corners, &world).expect("the view crosses the world");
        let bounds = region_bounds(&region);
        for axis in 0..3usize {
            assert!(
                component(bounds.min(), axis) >= component(world.min(), axis) - 1e-3
                    && component(bounds.max(), axis) <= component(world.max(), axis) + 1e-3,
                "axis {axis}: the region left the world, {:?} .. {:?}",
                bounds.min(),
                bounds.max()
            );
        }
        assert!(
            bounds.min().z <= -3999.0,
            "the region stopped short of the far side of the world at {}",
            bounds.min().z
        );
    }

    #[test]
    fn every_point_the_camera_can_see_of_the_world_is_inside_the_fitted_ball() {
        let (near, far) = (1.0, 6000.0);
        let corners = corners_of(near, far);
        let world = Aabb::from_corners(
            Vec3::new(-1500.0, -80.0, -3000.0),
            Vec3::new(1500.0, 220.0, 500.0),
        );
        let region = visible_region(&corners, &world).expect("the view crosses the world");
        let points = region.points();
        let mut centre = Vec3::ZERO;
        let mut count = 0.0f32;
        for point in points {
            centre = Vec3::new(centre.x + point.x, centre.y + point.y, centre.z + point.z);
            count += 1.0;
        }
        centre = Vec3::new(centre.x / count, centre.y / count, centre.z / count);
        let mut radius = 0.0f32;
        for point in points {
            radius = radius.max(
                Vec3::new(point.x - centre.x, point.y - centre.y, point.z - centre.z).length(),
            );
        }

        let mut seen = 0usize;
        for xi in 0..17u8 {
            for yi in 0..9u8 {
                for zi in 0..17u8 {
                    let point = Vec3::new(
                        -1500.0 + 3000.0 * f32::from(xi) / 16.0,
                        -80.0 + 300.0 * f32::from(yi) / 8.0,
                        -3000.0 + 3500.0 * f32::from(zi) / 16.0,
                    );
                    let depth = -point.z;
                    let half = depth * mint_math::f32::tan(Camera::default().fov_y * 0.5);
                    if depth < near || depth > far || point.x.abs() > half || point.y.abs() > half {
                        continue;
                    }
                    seen += 1;
                    let offset =
                        Vec3::new(point.x - centre.x, point.y - centre.y, point.z - centre.z);
                    assert!(
                        offset.length() <= radius + 1e-2,
                        "{point:?} is visible world and outside the fitted ball \
                         (radius {radius}, distance {})",
                        offset.length()
                    );
                }
            }
        }
        assert!(seen > 100, "the sample missed the visible region: {seen}");
    }
    #[test]
    fn a_box_in_front_of_the_camera_survives_both_depth_directions() {
        for direction in [DepthDirection::Standard, DepthDirection::Reversed] {
            let frustum = camera_frustum(direction);
            assert!(frustum.intersects(&box_at(Vec3::new(0.0, 0.0, -10.0), 1.0)));
        }
    }

    #[test]
    fn a_box_behind_the_camera_is_culled_both_depth_directions() {
        for direction in [DepthDirection::Standard, DepthDirection::Reversed] {
            let frustum = camera_frustum(direction);
            assert!(!frustum.intersects(&box_at(Vec3::new(0.0, 0.0, 10.0), 1.0)));
        }
    }

    #[test]
    fn a_box_far_off_to_one_side_is_culled() {
        let frustum = camera_frustum(DepthDirection::Reversed);
        assert!(!frustum.intersects(&box_at(Vec3::new(1000.0, 0.0, -10.0), 1.0)));
    }

    #[test]
    fn a_box_beyond_the_far_plane_is_culled() {
        let frustum = camera_frustum(DepthDirection::Reversed);
        assert!(!frustum.intersects(&box_at(Vec3::new(0.0, 0.0, -1000.0), 1.0)));
    }

    #[test]
    fn a_dome_enclosing_the_camera_survives() {
        let frustum = camera_frustum(DepthDirection::Reversed);
        assert!(frustum.intersects(&box_at(Vec3::ZERO, 7178.0)));
    }

    #[test]
    fn an_empty_box_is_never_culled() {
        let frustum = camera_frustum(DepthDirection::Reversed);
        assert!(frustum.intersects(&Aabb::EMPTY));
    }

    #[test]
    fn a_box_with_a_non_finite_corner_is_kept() {
        let frustum = camera_frustum(DepthDirection::Reversed);
        let unit = Aabb::from_corners(Vec3::ZERO, Vec3::splat(1.0));
        let bounds = transformed_bounds(&unit, Mat4::from_scale(Vec3::splat(f32::NAN)));
        assert!(frustum.intersects(&bounds));
    }

    #[test]
    fn a_translated_box_lands_where_the_translation_puts_it() {
        let unit = Aabb::from_corners(Vec3::splat(-1.0), Vec3::splat(1.0));
        let moved = transformed_bounds(&unit, Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)));
        assert!((moved.min().x - 9.0).abs() < 1e-4);
        assert!((moved.max().x - 11.0).abs() < 1e-4);
    }

    #[test]
    fn a_scaled_box_grows_by_the_scale() {
        let unit = Aabb::from_corners(Vec3::splat(-1.0), Vec3::splat(1.0));
        let big = transformed_bounds(&unit, Mat4::from_scale(Vec3::new(3.0, 1.0, 1.0)));
        assert!((big.max().x - 3.0).abs() < 1e-4);
        assert!((big.max().y - 1.0).abs() < 1e-4);
    }
}
