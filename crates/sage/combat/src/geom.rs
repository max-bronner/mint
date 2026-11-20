use mint_sim::StateHasher;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3 {
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub fn distance_squared_2d(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    #[must_use]
    pub fn distance_squared_3d(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
            z: self.z + (other.z - self.z) * t,
        }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_f32(self.x);
        hasher.write_f32(self.y);
        hasher.write_f32(self.z);
    }
}

#[must_use]
pub fn surface_distance_squared_2d(a: Point3, a_radius: f32, b: Point3, b_radius: f32) -> f32 {
    let centre_squared = a.distance_squared_2d(b);
    if !centre_squared.is_finite() {
        return f32::INFINITY;
    }
    let radii = a_radius.max(0.0) + b_radius.max(0.0);
    let surface = centre_squared.sqrt() - radii;
    if surface > 0.0 {
        surface * surface
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxVolume {
    pub major: f32,
    pub minor: f32,
    pub cos: f32,
    pub sin: f32,
}

impl BoxVolume {
    #[must_use]
    pub fn distance_squared_to(&self, centre: (f32, f32), at: (f32, f32), at_radius: f32) -> f32 {
        let dx = at.0 - centre.0;
        let dy = at.1 - centre.1;
        let a = (dx * self.cos + dy * self.sin).abs();
        let b = (dy * self.cos - dx * self.sin).abs();
        let raw = if a < self.major && b < self.minor {
            0.0
        } else if a < self.major {
            b - self.minor
        } else if b < self.minor {
            a - self.major
        } else {
            let length = (dx * dx + dy * dy).sqrt();
            if length > 0.0 {
                length - (self.major * a + self.minor * b) / length
            } else {
                0.0
            }
        };
        let border = raw - at_radius.max(0.0);
        if border.is_finite() && border > 0.0 {
            border * border
        } else if border.is_finite() {
            0.0
        } else {
            f32::INFINITY
        }
    }
}

#[must_use]
pub fn border_distance_squared(
    a: (f32, f32),
    a_radius: f32,
    a_box: Option<&BoxVolume>,
    b: (f32, f32),
    b_radius: f32,
    b_box: Option<&BoxVolume>,
) -> f32 {
    match (a_box, b_box) {
        (Some(shape), None) => shape.distance_squared_to(a, b, b_radius),
        (None, Some(shape)) => shape.distance_squared_to(b, a, a_radius),
        _ => surface_distance_squared_2d(
            Point3::new(a.0, a.1, 0.0),
            a_radius,
            Point3::new(b.0, b.1, 0.0),
            b_radius,
        ),
    }
}

#[must_use]
pub fn within_attack_range(distance_squared: f32, min_range: f32, max_range: f32) -> bool {
    if !distance_squared.is_finite() || !min_range.is_finite() || !max_range.is_finite() {
        return false;
    }
    let min_squared = min_range.max(0.0) * min_range.max(0.0);
    let max_squared = max_range.max(0.0) * max_range.max(0.0);
    if !min_squared.is_finite() || !max_squared.is_finite() {
        return false;
    }
    distance_squared >= min_squared && distance_squared <= max_squared
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_box_victim_is_reached_by_its_sides_not_its_bounding_circle() {
        let shape = BoxVolume {
            major: 50.0,
            minor: 10.0,
            cos: 1.0,
            sin: 0.0,
        };
        let from = |at: (f32, f32), shape: &BoxVolume| {
            border_distance_squared(at, 5.0, None, (0.0, 0.0), 0.0, Some(shape)).sqrt()
        };
        assert!((from((0.0, 60.0), &shape) - 45.0).abs() < 1e-3);
        assert!((from((60.0, 0.0), &shape) - 5.0).abs() < 1e-3);
        let turned = BoxVolume {
            cos: 0.0,
            sin: 1.0,
            ..shape
        };
        assert!((from((0.0, 60.0), &turned) - 5.0).abs() < 1e-3);
        let both = border_distance_squared(
            (0.0, 60.0),
            5.0,
            Some(&shape),
            (0.0, 0.0),
            0.0,
            Some(&shape),
        );
        assert!((both.sqrt() - (60.0 - 5.0)).abs() < 1e-3);
    }

    #[test]
    fn surface_distance_subtracts_both_radii_and_clamps_at_zero() {
        let a = Point3::new(0.0, 0.0, 0.0);
        let b = Point3::new(100.0, 0.0, 50.0);
        let d = surface_distance_squared_2d(a, 10.0, b, 15.0);
        assert!((d.sqrt() - 75.0).abs() < 1e-3, "{}", d.sqrt());
        let close = surface_distance_squared_2d(a, 60.0, b, 60.0);
        assert_eq!(close, 0.0);
    }

    #[test]
    fn the_range_test_is_inclusive_at_both_ends() {
        assert!(within_attack_range(100.0 * 100.0, 0.0, 100.0));
        assert!(!within_attack_range(100.01 * 100.01, 0.0, 100.0));
        assert!(within_attack_range(20.0 * 20.0, 20.0, 100.0));
        assert!(!within_attack_range(19.99 * 19.99, 20.0, 100.0));
    }

    #[test]
    fn a_non_finite_or_negative_range_refuses_rather_than_matching_everything() {
        assert!(!within_attack_range(f32::NAN, 0.0, 100.0));
        assert!(!within_attack_range(100.0, f32::NAN, 100.0));
        assert!(!within_attack_range(100.0, 0.0, f32::NAN));
        assert!(!within_attack_range(100.0, 0.0, f32::INFINITY));
        assert!(!within_attack_range(1.0, 0.0, -50.0));
        assert!(within_attack_range(0.0, 0.0, -50.0));
        let nan = Point3::new(f32::NAN, 0.0, 0.0);
        let here = Point3::new(0.0, 0.0, 0.0);
        assert!(!surface_distance_squared_2d(nan, 5.0, here, 5.0).is_finite());
        assert!(!within_attack_range(
            surface_distance_squared_2d(nan, 5.0, here, 5.0),
            0.0,
            1000.0
        ));
    }

    #[test]
    fn lerp_hits_both_ends_exactly() {
        let a = Point3::new(1.0, 2.0, 3.0);
        let b = Point3::new(5.0, 6.0, 7.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        let mid = a.lerp(b, 0.5);
        assert!((mid.x - 3.0).abs() < 1e-6 && (mid.z - 5.0).abs() < 1e-6);
    }
}
