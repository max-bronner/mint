use std::ops::{Add, Div, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const X: Self = Self::new(1.0, 0.0, 0.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);
    pub const Z: Self = Self::new(0.0, 0.0, 1.0);

    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub const fn splat(value: f32) -> Self {
        Self::new(value, value, value)
    }

    #[must_use]
    pub const fn from_array(v: [f32; 3]) -> Self {
        Self::new(v[0], v[1], v[2])
    }

    #[must_use]
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    #[must_use]
    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    #[must_use]
    pub fn normalize(self) -> Self {
        let length = self.length();
        if length > 0.0 && length.is_finite() {
            Self::new(self.x / length, self.y / length, self.z / length)
        } else {
            Self::ZERO
        }
    }

    #[must_use]
    pub fn min(self, other: Self) -> Self {
        Self::new(
            self.x.min(other.x),
            self.y.min(other.y),
            self.z.min(other.z),
        )
    }

    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Self::new(
            self.x.max(other.x),
            self.y.max(other.y),
            self.z.max(other.z),
        )
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, scale: f32) -> Self {
        Self::new(self.x * scale, self.y * scale, self.z * scale)
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, divisor: f32) -> Self {
        Self::new(self.x / divisor, self.y / divisor, self.z / divisor)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4 {
    cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    #[must_use]
    pub const fn from_cols(cols: [[f32; 4]; 4]) -> Self {
        Self { cols }
    }

    #[must_use]
    pub const fn to_cols(self) -> [[f32; 4]; 4] {
        self.cols
    }

    #[must_use]
    pub fn to_cols_array(self) -> [f32; 16] {
        let [a, b, c, d] = self.cols;
        [
            a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], c[0], c[1], c[2], c[3], d[0], d[1],
            d[2], d[3],
        ]
    }

    #[must_use]
    pub fn get(self, row: usize, col: usize) -> f32 {
        self.cols
            .get(col)
            .and_then(|c| c.get(row))
            .copied()
            .unwrap_or(0.0)
    }

    #[must_use]
    pub fn from_translation(offset: Vec3) -> Self {
        Self::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [offset.x, offset.y, offset.z, 1.0],
        ])
    }

    #[must_use]
    pub fn from_scale(scale: Vec3) -> Self {
        Self::from_cols([
            [scale.x, 0.0, 0.0, 0.0],
            [0.0, scale.y, 0.0, 0.0],
            [0.0, 0.0, scale.z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    #[must_use]
    pub fn from_rotation_y(radians: f32) -> Self {
        let (sin, cos) = mint_math::f32::sin_cos(radians);
        Self::from_cols([
            [cos, 0.0, -sin, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [sin, 0.0, cos, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    #[must_use]
    pub fn mul_mat4(self, rhs: Self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for (c, column) in out.iter_mut().enumerate() {
            for (r, slot) in column.iter_mut().enumerate() {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += self.get(r, k) * rhs.get(k, c);
                }
                *slot = sum;
            }
        }
        Self::from_cols(out)
    }

    #[must_use]
    pub fn transform_point(self, point: Vec3) -> Vec3 {
        let x = self.get(0, 0) * point.x
            + self.get(0, 1) * point.y
            + self.get(0, 2) * point.z
            + self.get(0, 3);
        let y = self.get(1, 0) * point.x
            + self.get(1, 1) * point.y
            + self.get(1, 2) * point.z
            + self.get(1, 3);
        let z = self.get(2, 0) * point.x
            + self.get(2, 1) * point.y
            + self.get(2, 2) * point.z
            + self.get(2, 3);
        let w = self.get(3, 0) * point.x
            + self.get(3, 1) * point.y
            + self.get(3, 2) * point.z
            + self.get(3, 3);
        if w == 0.0 {
            Vec3::new(x, y, z)
        } else {
            Vec3::new(x / w, y / w, z / w)
        }
    }

    #[must_use]
    pub fn transform_vector(self, vector: Vec3) -> Vec3 {
        Vec3::new(
            self.get(0, 0) * vector.x + self.get(0, 1) * vector.y + self.get(0, 2) * vector.z,
            self.get(1, 0) * vector.x + self.get(1, 1) * vector.y + self.get(1, 2) * vector.z,
            self.get(2, 0) * vector.x + self.get(2, 1) * vector.y + self.get(2, 2) * vector.z,
        )
    }

    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        let back = Vec3::new(eye.x - target.x, eye.y - target.y, eye.z - target.z).normalize();
        let right = up.cross(back).normalize();
        let true_up = back.cross(right);
        Self::from_cols([
            [right.x, true_up.x, back.x, 0.0],
            [right.y, true_up.y, back.y, 0.0],
            [right.z, true_up.z, back.z, 0.0],
            [-right.dot(eye), -true_up.dot(eye), -back.dot(eye), 1.0],
        ])
    }

    #[must_use]
    pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let half = fov_y_radians * 0.5;
        let focal = 1.0 / mint_math::f32::tan(half);
        let depth = near - far;
        Self::from_cols([
            [focal / aspect, 0.0, 0.0, 0.0],
            [0.0, focal, 0.0, 0.0],
            [0.0, 0.0, far / depth, -1.0],
            [0.0, 0.0, near * far / depth, 0.0],
        ])
    }

    #[must_use]
    pub fn perspective_reversed_z(fov_y_radians: f32, aspect: f32, near: f32, far: f32) -> Self {
        let half = fov_y_radians * 0.5;
        let focal = 1.0 / mint_math::f32::tan(half);
        let depth = far - near;
        Self::from_cols([
            [focal / aspect, 0.0, 0.0, 0.0],
            [0.0, focal, 0.0, 0.0],
            [0.0, 0.0, near / depth, -1.0],
            [0.0, 0.0, far * near / depth, 0.0],
        ])
    }

    #[must_use]
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Self {
        let width = right - left;
        let height = top - bottom;
        let depth = near - far;
        Self::from_cols([
            [2.0 / width, 0.0, 0.0, 0.0],
            [0.0, 2.0 / height, 0.0, 0.0],
            [0.0, 0.0, 1.0 / depth, 0.0],
            [
                -(right + left) / width,
                -(top + bottom) / height,
                near / depth,
                1.0,
            ],
        ])
    }

    #[must_use]
    pub fn transpose(self) -> Self {
        let mut out = [[0.0f32; 4]; 4];
        for (c, column) in out.iter_mut().enumerate() {
            for (r, slot) in column.iter_mut().enumerate() {
                *slot = self.get(c, r);
            }
        }
        Self::from_cols(out)
    }

    #[must_use]
    pub fn inverse(self) -> Option<Self> {
        let [m0, m1, m2, m3, m4, m5, m6, m7, m8, m9, m10, m11, m12, m13, m14, m15] =
            self.to_cols_array();
        let i0 = m5 * m10 * m15 - m5 * m11 * m14 - m9 * m6 * m15 + m9 * m7 * m14 + m13 * m6 * m11
            - m13 * m7 * m10;
        let i4 = -m4 * m10 * m15 + m4 * m11 * m14 + m8 * m6 * m15 - m8 * m7 * m14 - m12 * m6 * m11
            + m12 * m7 * m10;
        let i8 = m4 * m9 * m15 - m4 * m11 * m13 - m8 * m5 * m15 + m8 * m7 * m13 + m12 * m5 * m11
            - m12 * m7 * m9;
        let i12 = -m4 * m9 * m14 + m4 * m10 * m13 + m8 * m5 * m14 - m8 * m6 * m13 - m12 * m5 * m10
            + m12 * m6 * m9;
        let i1 = -m1 * m10 * m15 + m1 * m11 * m14 + m9 * m2 * m15 - m9 * m3 * m14 - m13 * m2 * m11
            + m13 * m3 * m10;
        let i5 = m0 * m10 * m15 - m0 * m11 * m14 - m8 * m2 * m15 + m8 * m3 * m14 + m12 * m2 * m11
            - m12 * m3 * m10;
        let i9 = -m0 * m9 * m15 + m0 * m11 * m13 + m8 * m1 * m15 - m8 * m3 * m13 - m12 * m1 * m11
            + m12 * m3 * m9;
        let i13 = m0 * m9 * m14 - m0 * m10 * m13 - m8 * m1 * m14 + m8 * m2 * m13 + m12 * m1 * m10
            - m12 * m2 * m9;
        let i2 = m1 * m6 * m15 - m1 * m7 * m14 - m5 * m2 * m15 + m5 * m3 * m14 + m13 * m2 * m7
            - m13 * m3 * m6;
        let i6 = -m0 * m6 * m15 + m0 * m7 * m14 + m4 * m2 * m15 - m4 * m3 * m14 - m12 * m2 * m7
            + m12 * m3 * m6;
        let i10 = m0 * m5 * m15 - m0 * m7 * m13 - m4 * m1 * m15 + m4 * m3 * m13 + m12 * m1 * m7
            - m12 * m3 * m5;
        let i14 = -m0 * m5 * m14 + m0 * m6 * m13 + m4 * m1 * m14 - m4 * m2 * m13 - m12 * m1 * m6
            + m12 * m2 * m5;
        let i3 = -m1 * m6 * m11 + m1 * m7 * m10 + m5 * m2 * m11 - m5 * m3 * m10 - m9 * m2 * m7
            + m9 * m3 * m6;
        let i7 = m0 * m6 * m11 - m0 * m7 * m10 - m4 * m2 * m11 + m4 * m3 * m10 + m8 * m2 * m7
            - m8 * m3 * m6;
        let i11 = -m0 * m5 * m11 + m0 * m7 * m9 + m4 * m1 * m11 - m4 * m3 * m9 - m8 * m1 * m7
            + m8 * m3 * m5;
        let i15 = m0 * m5 * m10 - m0 * m6 * m9 - m4 * m1 * m10 + m4 * m2 * m9 + m8 * m1 * m6
            - m8 * m2 * m5;
        let determinant = m0 * i0 + m1 * i4 + m2 * i8 + m3 * i12;
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let s = 1.0 / determinant;
        let out = Self::from_cols([
            [i0 * s, i1 * s, i2 * s, i3 * s],
            [i4 * s, i5 * s, i6 * s, i7 * s],
            [i8 * s, i9 * s, i10 * s, i11 * s],
            [i12 * s, i13 * s, i14 * s, i15 * s],
        ]);
        out.to_cols_array()
            .iter()
            .all(|value| value.is_finite())
            .then_some(out)
    }

    #[must_use]
    pub fn normal_matrix(self) -> Self {
        let m = |r: usize, c: usize| self.get(r, c);
        let c00 = m(1, 1) * m(2, 2) - m(1, 2) * m(2, 1);
        let c01 = m(1, 2) * m(2, 0) - m(1, 0) * m(2, 2);
        let c02 = m(1, 0) * m(2, 1) - m(1, 1) * m(2, 0);
        let determinant = m(0, 0) * c00 + m(0, 1) * c01 + m(0, 2) * c02;
        if determinant == 0.0 || !determinant.is_finite() {
            return Self::IDENTITY;
        }
        let c10 = m(0, 2) * m(2, 1) - m(0, 1) * m(2, 2);
        let c11 = m(0, 0) * m(2, 2) - m(0, 2) * m(2, 0);
        let c12 = m(0, 1) * m(2, 0) - m(0, 0) * m(2, 1);
        let c20 = m(0, 1) * m(1, 2) - m(0, 2) * m(1, 1);
        let c21 = m(0, 2) * m(1, 0) - m(0, 0) * m(1, 2);
        let c22 = m(0, 0) * m(1, 1) - m(0, 1) * m(1, 0);
        let s = 1.0 / determinant;
        Self::from_cols([
            [c00 * s, c10 * s, c20 * s, 0.0],
            [c01 * s, c11 * s, c21 * s, 0.0],
            [c02 * s, c12 * s, c22 * s, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    const EPSILON: f32 = 1e-5;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= EPSILON
    }

    fn close_vec(a: Vec3, b: Vec3) -> bool {
        close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z)
    }

    #[test]
    fn a_view_projection_times_its_inverse_is_the_identity() {
        let view = Mat4::look_at(
            Vec3::new(30.0, -40.0, 55.0),
            Vec3::new(2.0, 3.0, 0.0),
            Vec3::Z,
        );
        let matrix = Mat4::perspective_reversed_z(0.8, 1.6, 2.0, 900.0).mul_mat4(view);
        let inverse = matrix.inverse().expect("a view-projection is invertible");
        let product = matrix.mul_mat4(inverse);
        for row in 0..4 {
            for col in 0..4 {
                let expected = if row == col { 1.0 } else { 0.0 };
                assert!(
                    (product.get(row, col) - expected).abs() < 1e-3,
                    "({row}, {col}) is {}",
                    product.get(row, col)
                );
            }
        }
        assert_eq!(Mat4::from_cols([[0.0; 4]; 4]).inverse(), None);
    }

    #[test]
    fn cross_products_are_right_handed() {
        assert!(close_vec(Vec3::X.cross(Vec3::Y), Vec3::Z));
        assert!(close_vec(Vec3::Y.cross(Vec3::Z), Vec3::X));
        assert!(close_vec(Vec3::Z.cross(Vec3::X), Vec3::Y));
    }

    #[test]
    fn normalising_a_zero_vector_yields_zero_rather_than_nan() {
        let n = Vec3::ZERO.normalize();
        assert!(
            n.is_finite(),
            "a degenerate model must not poison the camera"
        );
        assert_eq!(n, Vec3::ZERO);
    }

    #[test]
    fn identity_multiplication_changes_nothing() {
        let m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(m.mul_mat4(Mat4::IDENTITY), m);
        assert_eq!(Mat4::IDENTITY.mul_mat4(m), m);
    }

    #[test]
    fn translation_then_scale_applies_in_the_written_order() {
        let t = Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
        let s = Mat4::from_scale(Vec3::splat(2.0));
        assert!(close_vec(
            s.mul_mat4(t).transform_point(Vec3::ZERO),
            Vec3::new(2.0, 0.0, 0.0)
        ));
        assert!(close_vec(
            t.mul_mat4(s).transform_point(Vec3::ZERO),
            Vec3::new(1.0, 0.0, 0.0)
        ));
    }

    #[test]
    fn look_at_puts_the_eye_at_the_origin_looking_down_negative_z() {
        let eye = Vec3::new(5.0, 0.0, 0.0);
        let view = Mat4::look_at(eye, Vec3::ZERO, Vec3::Z);

        assert!(close_vec(view.transform_point(eye), Vec3::ZERO));
        let target = view.transform_point(Vec3::ZERO);
        assert!(close(target.z, -5.0), "target at {target:?}");
    }

    #[test]
    fn look_at_tolerates_an_up_vector_that_is_not_perpendicular() {
        let skewed = Mat4::look_at(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::new(0.3, 1.0, 0.7),
        );
        let square = Mat4::look_at(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::ZERO,
            Vec3::new(0.3, 1.0, 0.0),
        );
        for i in 0..4 {
            for j in 0..4 {
                assert!(close(skewed.get(i, j), square.get(i, j)));
            }
        }
    }

    #[test]
    fn perspective_maps_near_to_zero_and_far_to_one() {
        let near = 0.5;
        let far = 100.0;
        let proj = Mat4::perspective(60f32.to_radians(), 1.0, near, far);

        let on_near = proj.transform_point(Vec3::new(0.0, 0.0, -near));
        let on_far = proj.transform_point(Vec3::new(0.0, 0.0, -far));
        assert!(close(on_near.z, 0.0), "near mapped to {}", on_near.z);
        assert!(close(on_far.z, 1.0), "far mapped to {}", on_far.z);
    }

    #[test]
    fn reversed_z_maps_near_to_one_and_far_to_zero() {
        let (near, far) = (0.5f32, 900.0f32);
        let proj = Mat4::perspective_reversed_z(60f32.to_radians(), 1.0, near, far);
        let at = |d: f32| {
            let p = proj.transform_point(Vec3::new(0.0, 0.0, -d));
            p.z
        };
        assert!((at(near) - 1.0).abs() < 1e-5, "near gave {}", at(near));
        assert!(at(far).abs() < 1e-5, "far gave {}", at(far));
        assert!(at(10.0) > at(100.0));
    }

    #[test]
    fn reversed_z_resolves_a_small_offset_that_standard_z_cannot() {
        let (near, far, d, lift) = (29.0f32, 58000.0f32, 8700.0f32, 0.078_125f32);
        let ulps = |m: Mat4| {
            let z = |d: f32| m.transform_point(Vec3::new(0.0, 0.0, -d)).z.to_bits() as i64;
            (z(d) - z(d + lift)).abs()
        };
        let standard = ulps(Mat4::perspective(45f32.to_radians(), 1.0, near, far));
        let reversed = ulps(Mat4::perspective_reversed_z(
            45f32.to_radians(),
            1.0,
            near,
            far,
        ));
        assert!(
            standard <= 1,
            "standard z separated the two by {standard} ulps"
        );
        assert!(
            reversed > 100,
            "reversed z separated the two by only {reversed} ulps"
        );
    }

    #[test]
    fn perspective_puts_the_view_axis_at_the_centre_of_the_screen() {
        let proj = Mat4::perspective(60f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        let centre = proj.transform_point(Vec3::new(0.0, 0.0, -10.0));
        assert!(close(centre.x, 0.0));
        assert!(close(centre.y, 0.0));
    }

    #[test]
    fn a_wider_aspect_ratio_compresses_x_and_leaves_y_alone() {
        let square = Mat4::perspective(60f32.to_radians(), 1.0, 0.1, 100.0);
        let wide = Mat4::perspective(60f32.to_radians(), 2.0, 0.1, 100.0);
        let point = Vec3::new(1.0, 1.0, -10.0);
        let a = square.transform_point(point);
        let b = wide.transform_point(point);

        assert!(close(b.x, a.x * 0.5), "{} vs {}", b.x, a.x);
        assert!(close(b.y, a.y));
    }

    #[test]
    fn a_point_on_the_edge_of_the_vertical_field_of_view_lands_on_the_ndc_edge() {
        let fov = 90f32.to_radians();
        let proj = Mat4::perspective(fov, 1.0, 0.1, 100.0);
        let edge = proj.transform_point(Vec3::new(0.0, 10.0, -10.0));
        assert!(close(edge.y, 1.0), "edge y {}", edge.y);
    }

    #[test]
    fn the_normal_matrix_of_a_rigid_transform_is_its_own_rotation() {
        let rigid = Mat4::from_translation(Vec3::new(3.0, -2.0, 1.0));
        let n = rigid.normal_matrix();
        assert!(close_vec(n.transform_vector(Vec3::Y), Vec3::Y));
    }

    #[test]
    fn the_normal_matrix_undoes_a_non_uniform_scale() {
        let squash = Mat4::from_scale(Vec3::new(1.0, 0.25, 1.0));
        let n = squash.normal_matrix();
        let tilted = n
            .transform_vector(Vec3::new(0.0, 1.0, 1.0).normalize())
            .normalize();
        let naive = squash
            .transform_vector(Vec3::new(0.0, 1.0, 1.0).normalize())
            .normalize();

        assert!(
            tilted.y > 0.9,
            "normal should stay mostly up, got {tilted:?}"
        );
        assert!(naive.y < 0.5, "the naive path is wrong, which is the point");
    }

    #[test]
    fn the_normal_matrix_keeps_normals_perpendicular_under_a_turn_and_a_squash() {
        let (s, c) = (0.6f32, 0.8f32);
        let turn = Mat4::from_cols([
            [c, s, 0.0, 0.0],
            [-s, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [4.0, -1.0, 2.0, 1.0],
        ]);
        let turned = turn.normal_matrix().transform_vector(Vec3::X);
        assert!(close_vec(turned, Vec3::new(c, s, 0.0)), "{turned:?}");
        let squashed = turn.mul_mat4(Mat4::from_scale(Vec3::new(1.0, 0.25, 3.0)));
        let normal = Vec3::new(1.0, 2.0, -0.5).normalize();
        let along = [Vec3::new(2.0, -1.0, 0.0), Vec3::new(0.5, 0.0, 1.0)];
        for tangent in along {
            assert!(close(normal.dot(tangent), 0.0));
            let moved = squashed.transform_vector(tangent);
            let n = squashed.normal_matrix().transform_vector(normal);
            assert!(n.dot(moved).abs() < 1e-4, "{n:?} . {moved:?}");
        }
    }

    #[test]
    fn a_singular_transform_yields_the_identity_rather_than_infinities() {
        let flat = Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(flat.normal_matrix(), Mat4::IDENTITY);
    }

    #[test]
    fn transpose_is_its_own_inverse() {
        let m = Mat4::perspective(1.0, 1.5, 0.1, 50.0);
        assert_eq!(m.transpose().transpose(), m);
    }

    #[test]
    fn columns_round_trip_through_the_flat_array() {
        let m = Mat4::look_at(Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO, Vec3::Z);
        let flat = m.to_cols_array();
        for c in 0..4 {
            for r in 0..4 {
                assert_eq!(flat[c * 4 + r], m.get(r, c));
            }
        }
    }

    #[test]
    fn an_out_of_range_element_is_zero_rather_than_a_panic() {
        assert_eq!(Mat4::IDENTITY.get(9, 9), 0.0);
        assert_eq!(Mat4::IDENTITY.get(usize::MAX, 0), 0.0);
    }
}
