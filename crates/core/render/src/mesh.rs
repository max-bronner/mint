use crate::math::Vec3;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
    pub uv2: [f32; 2],
    pub color: [f32; 4],
}

impl Vertex {
    pub const SIZE: usize = 72;

    #[must_use]
    pub const fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            tangent: [0.0; 4],
            uv2: [0.0; 2],
            color: [1.0; 4],
        }
    }

    #[must_use]
    pub const fn with_tangent(mut self, tangent: [f32; 4]) -> Self {
        self.tangent = tangent;
        self
    }

    #[must_use]
    pub const fn with_uv2(mut self, uv2: [f32; 2]) -> Self {
        self.uv2 = uv2;
        self
    }

    #[must_use]
    pub const fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    min: Vec3,
    max: Vec3,
    empty: bool,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    pub const EMPTY: Self = Self {
        min: Vec3::ZERO,
        max: Vec3::ZERO,
        empty: true,
    };

    #[must_use]
    pub fn from_corners(a: Vec3, b: Vec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
            empty: false,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.empty
    }

    pub fn insert(&mut self, point: Vec3) {
        if !point.is_finite() {
            return;
        }
        if self.empty {
            self.min = point;
            self.max = point;
            self.empty = false;
        } else {
            self.min = self.min.min(point);
            self.max = self.max.max(point);
        }
    }

    pub fn union(&mut self, other: &Self) {
        if other.empty {
            return;
        }
        self.insert(other.min);
        self.insert(other.max);
    }

    #[must_use]
    pub fn min(&self) -> Vec3 {
        self.min
    }

    #[must_use]
    pub fn max(&self) -> Vec3 {
        self.max
    }

    #[must_use]
    pub fn center(&self) -> Vec3 {
        if self.empty {
            Vec3::ZERO
        } else {
            Vec3::new(
                (self.min.x + self.max.x) * 0.5,
                (self.min.y + self.max.y) * 0.5,
                (self.min.z + self.max.z) * 0.5,
            )
        }
    }

    #[must_use]
    pub fn radius(&self) -> f32 {
        if self.empty {
            0.0
        } else {
            Vec3::new(
                self.max.x - self.min.x,
                self.max.y - self.min.y,
                self.max.z - self.min.z,
            )
            .length()
                * 0.5
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len().checked_div(3).unwrap_or(0)
    }

    #[must_use]
    pub fn bounds(&self) -> Aabb {
        let mut bounds = Aabb::EMPTY;
        for vertex in &self.vertices {
            bounds.insert(Vec3::from_array(vertex.position));
        }
        bounds
    }

    #[must_use]
    pub fn validate(&self) -> Option<usize> {
        if self.indices.len().checked_rem(3) != Some(0) {
            return Some(self.indices.len());
        }
        let count = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX);
        self.indices.iter().position(|&index| index >= count)
    }

    #[must_use]
    pub fn vertex_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.vertices.len().saturating_mul(Vertex::SIZE));
        for vertex in &self.vertices {
            for value in vertex.position {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in vertex.normal {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in vertex.uv {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in vertex.tangent {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in vertex.uv2 {
                out.extend_from_slice(&value.to_le_bytes());
            }
            for value in vertex.color {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }

    #[must_use]
    pub fn index_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.indices.len().saturating_mul(4));
        for index in &self.indices {
            out.extend_from_slice(&index.to_le_bytes());
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_vertex_stride_matches_the_declared_layout() {
        assert_eq!(
            std::mem::size_of::<Vertex>(),
            Vertex::SIZE,
            "the shader's attribute offsets are derived from this constant"
        );
        assert_eq!(std::mem::align_of::<Vertex>(), 4);
    }

    #[test]
    fn vertex_bytes_are_the_stride_times_the_count() {
        let mesh = MeshData {
            vertices: vec![Vertex::default(); 7],
            indices: Vec::new(),
        };
        assert_eq!(mesh.vertex_bytes().len(), 7 * Vertex::SIZE);
    }

    #[test]
    fn vertex_bytes_are_little_endian_in_declaration_order() {
        let mesh = MeshData {
            vertices: vec![Vertex::new([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [0.25, 0.75])
                .with_tangent([1.0, 0.0, 0.0, -1.0])],
            indices: Vec::new(),
        };
        let bytes = mesh.vertex_bytes();
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[12..16], &0.0f32.to_le_bytes());
        assert_eq!(&bytes[20..24], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[24..28], &0.25f32.to_le_bytes());
        assert_eq!(&bytes[28..32], &0.75f32.to_le_bytes());
        assert_eq!(&bytes[32..36], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[44..48], &(-1.0f32).to_le_bytes());
    }

    #[test]
    fn a_vertex_has_no_tangent_basis_unless_one_is_given() {
        let plain = Vertex::new([0.0; 3], [0.0, 0.0, 1.0], [0.0; 2]);
        assert_eq!(plain.tangent, [0.0; 4]);
        assert_eq!(
            plain.with_tangent([1.0, 0.0, 0.0, 1.0]).tangent,
            [1.0, 0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn an_empty_box_is_distinguishable_from_a_box_at_the_origin() {
        let empty = Aabb::EMPTY;
        assert!(empty.is_empty());
        assert_eq!(empty.radius(), 0.0);

        let mut point = Aabb::EMPTY;
        point.insert(Vec3::ZERO);
        assert!(!point.is_empty());
        assert_eq!(point.radius(), 0.0);
    }

    #[test]
    fn a_box_grows_to_contain_every_point() {
        let mut bounds = Aabb::EMPTY;
        bounds.insert(Vec3::new(-1.0, 2.0, 0.0));
        bounds.insert(Vec3::new(3.0, -4.0, 5.0));

        assert_eq!(bounds.min(), Vec3::new(-1.0, -4.0, 0.0));
        assert_eq!(bounds.max(), Vec3::new(3.0, 2.0, 5.0));
        assert_eq!(bounds.center(), Vec3::new(1.0, -1.0, 2.5));
    }

    #[test]
    fn a_non_finite_vertex_does_not_poison_the_bounds() {
        let mut bounds = Aabb::EMPTY;
        bounds.insert(Vec3::new(0.0, 0.0, 0.0));
        bounds.insert(Vec3::new(f32::NAN, 1.0, 1.0));
        bounds.insert(Vec3::new(f32::INFINITY, 0.0, 0.0));
        bounds.insert(Vec3::new(2.0, 2.0, 2.0));

        assert_eq!(bounds.min(), Vec3::ZERO);
        assert_eq!(bounds.max(), Vec3::splat(2.0));
        assert!(bounds.radius().is_finite());
    }

    #[test]
    fn union_with_an_empty_box_changes_nothing() {
        let mut a = Aabb::from_corners(Vec3::ZERO, Vec3::ONE);
        let before = a;
        a.union(&Aabb::EMPTY);
        assert_eq!(a, before);
    }

    #[test]
    fn the_radius_of_a_unit_cube_is_half_its_diagonal() {
        let cube = Aabb::from_corners(Vec3::ZERO, Vec3::ONE);
        assert!((cube.radius() - 3f32.sqrt() / 2.0).abs() < 1e-6);
    }

    #[test]
    fn an_index_past_the_last_vertex_is_reported() {
        let mesh = MeshData {
            vertices: vec![Vertex::default(); 3],
            indices: vec![0, 1, 3],
        };
        assert_eq!(mesh.validate(), Some(2));
    }

    #[test]
    fn a_partial_triangle_is_reported() {
        let mesh = MeshData {
            vertices: vec![Vertex::default(); 3],
            indices: vec![0, 1],
        };
        assert_eq!(mesh.validate(), Some(2));
    }

    #[test]
    fn a_well_formed_mesh_validates() {
        let mesh = MeshData {
            vertices: vec![Vertex::default(); 3],
            indices: vec![0, 1, 2],
        };
        assert_eq!(mesh.validate(), None);
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn an_empty_mesh_validates_and_has_no_triangles() {
        let mesh = MeshData::new();
        assert_eq!(mesh.validate(), None);
        assert_eq!(mesh.triangle_count(), 0);
        assert!(mesh.bounds().is_empty());
    }
}
