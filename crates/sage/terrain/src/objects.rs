use mint_render::{Mat4, MeshData, Vec3, Vertex};
use mint_sage_map::{HeightMapData, Object};

use crate::surface::{surface_height, surface_normal};

pub const PROXY_SIZE: f32 = 9.0;

#[must_use]
pub fn forward(angle: f32) -> Vec3 {
    if !angle.is_finite() {
        return Vec3::X;
    }
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    Vec3::new(cos, sin, 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectProxy {
    pub index: usize,
    pub position: Vec3,
    pub ground: Option<f32>,
    pub angle: f32,
    pub align_to_terrain: bool,
    pub waypoint: bool,
    pub placement: Mat4,
}

impl ObjectProxy {
    #[must_use]
    pub fn marker_transform(&self) -> Mat4 {
        scaled(self.placement, PROXY_SIZE)
    }
}

#[must_use]
fn scaled(transform: Mat4, size: f32) -> Mat4 {
    let column = |index: usize| {
        [
            transform.get(0, index) * size,
            transform.get(1, index) * size,
            transform.get(2, index) * size,
            transform.get(3, index),
        ]
    };
    Mat4::from_cols([
        column(0),
        column(1),
        column(2),
        [
            transform.get(0, 3),
            transform.get(1, 3),
            transform.get(2, 3),
            transform.get(3, 3),
        ],
    ])
}

pub const ALIGN_KEY: &str = "alignToTerrain";

pub const PROTOTYPE_SCALE_KEY: &str = "objectPrototypeScale";

#[must_use]
pub fn prototype_scale(object: &Object) -> f32 {
    match object.properties.get(PROTOTYPE_SCALE_KEY) {
        Some(mint_sage_map::Value::Float(scale)) if scale.is_finite() && *scale > 0.0 => *scale,
        _ => 1.0,
    }
}

pub const WAYPOINT_KEY: &str = "waypointID";

#[must_use]
pub fn wants_terrain_alignment(object: &Object) -> bool {
    matches!(
        object.properties.get(ALIGN_KEY),
        Some(mint_sage_map::Value::Bool(true))
    )
}

#[must_use]
pub fn is_waypoint(object: &Object) -> bool {
    object.properties.get(WAYPOINT_KEY).is_some()
}

#[must_use]
pub fn place_objects(
    height_map: &HeightMapData,
    vertical_scale: f32,
    objects: &[Object],
    anchors: &[[f32; 2]],
) -> Vec<ObjectProxy> {
    let mut out = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        let heading = forward(object.angle);
        let (x, y) = match anchors.get(index) {
            Some(&[ax, ay]) if ax.is_finite() && ay.is_finite() => (
                object.x + ax * heading.x - ay * heading.y,
                object.y + ax * heading.y + ay * heading.x,
            ),
            _ => (object.x, object.y),
        };
        let ground = surface_height(height_map, vertical_scale, x, y);
        let offset = if object.z.is_finite() { object.z } else { 0.0 };
        let position = Vec3::new(x, y, ground.unwrap_or(0.0) + offset);
        let align_to_terrain = wants_terrain_alignment(object);
        let up = if align_to_terrain {
            surface_normal(height_map, vertical_scale, x, y).unwrap_or(Vec3::Z)
        } else {
            Vec3::Z
        };
        out.push(ObjectProxy {
            index,
            position,
            ground,
            angle: object.angle,
            align_to_terrain,
            waypoint: is_waypoint(object),
            placement: scaled(
                proxy_transform(position, heading, up),
                prototype_scale(object),
            ),
        });
    }
    out
}

#[must_use]
pub fn proxy_transform(position: Vec3, forward: Vec3, up: Vec3) -> Mat4 {
    let up = {
        let n = up.normalize();
        if n.length_squared() > 0.0 {
            n
        } else {
            Vec3::Z
        }
    };
    let projected = {
        let along = up.dot(forward);
        Vec3::new(
            forward.x - up.x * along,
            forward.y - up.y * along,
            forward.z - up.z * along,
        )
    };
    let front = {
        let n = projected.normalize();
        if n.length_squared() > 0.0 {
            n
        } else {
            let fallback = if up.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
            let along = up.dot(fallback);
            Vec3::new(
                fallback.x - up.x * along,
                fallback.y - up.y * along,
                fallback.z - up.z * along,
            )
            .normalize()
        }
    };
    let left = up.cross(front);
    Mat4::from_cols([
        [front.x, front.y, front.z, 0.0],
        [left.x, left.y, left.z, 0.0],
        [up.x, up.y, up.z, 0.0],
        [position.x, position.y, position.z, 1.0],
    ])
}

const PROXY_CORNERS: [[f32; 3]; 4] = [
    [0.5, 0.0, 0.15],
    [-0.4, 0.35, 0.0],
    [-0.4, -0.35, 0.0],
    [-0.15, 0.0, 0.9],
];

const PROXY_COLORS: [[f32; 4]; 4] = [
    [1.0, 0.42, 0.12, 1.0],
    [0.10, 0.30, 0.85, 1.0],
    [0.10, 0.30, 0.85, 1.0],
    [0.98, 0.92, 0.30, 1.0],
];

const PROXY_FACES: [[usize; 3]; 4] = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];

#[must_use]
pub fn proxy_mesh() -> MeshData {
    let mut vertices = Vec::with_capacity(12);
    let mut indices = Vec::with_capacity(12);
    for face in PROXY_FACES {
        let corner = |slot: usize| -> Vec3 {
            face.get(slot)
                .and_then(|index| PROXY_CORNERS.get(*index))
                .map_or(Vec3::ZERO, |p| Vec3::from_array(*p))
        };
        let (a, b, c) = (corner(0), corner(1), corner(2));
        let edge_ab = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let edge_ac = Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
        let normal = edge_ab.cross(edge_ac).normalize().to_array();
        for slot in 0..3 {
            let index = face.get(slot).copied().unwrap_or(0);
            let position = PROXY_CORNERS.get(index).copied().unwrap_or([0.0; 3]);
            let color = PROXY_COLORS.get(index).copied().unwrap_or([1.0; 4]);
            let next = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
            vertices.push(Vertex::new(position, normal, [0.0, 0.0]).with_color(color));
            indices.push(next);
        }
    }
    MeshData { vertices, indices }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_map::Dictionary;

    fn flat_map(width: u32, height: u32, sample: u16) -> HeightMapData {
        HeightMapData {
            width,
            height,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![sample; (width * height) as usize],
        }
    }

    fn object(x: f32, y: f32, z: f32, angle: f32) -> Object {
        Object {
            x,
            y,
            z,
            angle,
            flags: 0,
            template_name: "t".to_owned(),
            properties: Dictionary::default(),
        }
    }

    #[test]
    fn every_proxy_face_points_outward() {
        let mesh = proxy_mesh();
        assert_eq!(mesh.triangle_count(), 4);
        assert_eq!(mesh.validate(), None);
        let mut centroid = Vec3::ZERO;
        for corner in PROXY_CORNERS {
            let c = Vec3::from_array(corner);
            centroid = Vec3::new(
                centroid.x + c.x / 4.0,
                centroid.y + c.y / 4.0,
                centroid.z + c.z / 4.0,
            );
        }
        for face in mesh.vertices.chunks_exact(3) {
            let mut middle = Vec3::ZERO;
            for vertex in face {
                let p = Vec3::from_array(vertex.position);
                middle = Vec3::new(
                    middle.x + p.x / 3.0,
                    middle.y + p.y / 3.0,
                    middle.z + p.z / 3.0,
                );
            }
            let outward = Vec3::new(
                middle.x - centroid.x,
                middle.y - centroid.y,
                middle.z - centroid.z,
            );
            let normal = Vec3::from_array(face[0].normal);
            assert!(
                normal.dot(outward) > 0.0,
                "face normal {normal:?} points into the solid"
            );
        }
    }

    #[test]
    fn a_rising_angle_turns_toward_plus_y() {
        let zero = forward(0.0);
        assert!((zero.x - 1.0).abs() < 1e-6);
        assert!(zero.y.abs() < 1e-6);

        let quarter = forward(core::f32::consts::FRAC_PI_2);
        assert!(quarter.x.abs() < 1e-6);
        assert!((quarter.y - 1.0).abs() < 1e-6, "a quarter turn reaches +y");

        let transform = proxy_transform(Vec3::ZERO, quarter, Vec3::Z);
        let nose = transform.transform_point(Vec3::from_array(PROXY_CORNERS[0]));
        assert!(
            nose.y > 0.4,
            "the nose should have swung to +y, got {nose:?}"
        );
        assert!(nose.x.abs() < 1e-5);
        let apex = transform.transform_point(Vec3::from_array(PROXY_CORNERS[3]));
        assert!(apex.z > 0.8);
    }

    #[test]
    fn a_placement_stands_on_the_terrain_plus_its_own_z() {
        let hm = flat_map(8, 8, 1000);
        let objects = vec![object(25.0, 25.0, -3.0, 0.0)];
        let placed = place_objects(&hm, 0.04, &objects, &[]);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].ground, Some(40.0));
        assert!((placed[0].position.z - 37.0).abs() < 1e-4);
        assert!(!placed[0].align_to_terrain);
        assert!(!placed[0].waypoint);
    }

    #[test]
    fn a_placement_stands_at_its_anchor_turned_by_its_angle() {
        let mut hm = flat_map(8, 8, 1000);
        for row in 0..8usize {
            hm.samples[row * 8 + 5] = 2000;
        }
        let objects = vec![object(15.0, 15.0, 0.0, std::f32::consts::FRAC_PI_2)];
        let placed = place_objects(&hm, 0.04, &objects, &[[35.0, 0.0]]);
        assert!((placed[0].position.x - 15.0).abs() < 1e-3);
        assert!((placed[0].position.y - 50.0).abs() < 1e-3);
        assert_eq!(placed[0].ground, Some(40.0));

        let objects = vec![object(15.0, 15.0, 0.0, 0.0)];
        let placed = place_objects(&hm, 0.04, &objects, &[[35.0, 0.0]]);
        assert!((placed[0].position.x - 50.0).abs() < 1e-3);
        assert!((placed[0].position.y - 15.0).abs() < 1e-3);
        assert_eq!(placed[0].ground, Some(80.0));
    }

    #[test]
    fn a_prototype_scale_resizes_the_placement_and_leaves_its_position() {
        let hm = flat_map(8, 8, 0);
        let mut scaled_object = object(25.0, 35.0, 0.0, 0.0);
        scaled_object.properties = mint_sage_map::Dictionary::from_pairs(vec![(
            PROTOTYPE_SCALE_KEY.to_owned(),
            mint_sage_map::Value::Float(1.5),
        )]);
        let placed = place_objects(
            &hm,
            0.04,
            &[scaled_object, object(25.0, 35.0, 0.0, 0.0)],
            &[],
        );
        let [scaled_x, _, scaled_z, scaled_at] = placed[0].placement.to_cols();
        assert!((scaled_x[0] - 1.5).abs() < 1e-5);
        assert!((scaled_z[2] - 1.5).abs() < 1e-5);
        assert_eq!(scaled_at, placed[1].placement.to_cols()[3]);
        assert!((placed[1].placement.to_cols()[0][0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn an_object_outside_the_grid_is_kept_and_reported() {
        let hm = flat_map(4, 4, 500);
        let objects = vec![object(-500.0, 0.0, 0.0, 0.0)];
        let placed = place_objects(&hm, 0.04, &objects, &[]);
        assert_eq!(placed[0].ground, None);
        assert_eq!(placed[0].position.x, -500.0);
    }

    #[test]
    fn the_marker_scale_reaches_the_basis_and_not_the_translation() {
        let hm = flat_map(8, 8, 0);
        let objects = vec![object(25.0, 35.0, 0.0, 0.0)];
        let placed = place_objects(&hm, 0.04, &objects, &[]);
        let placement = placed[0].placement;
        let marker = placed[0].marker_transform();

        let front = Vec3::new(
            placement.get(0, 0),
            placement.get(1, 0),
            placement.get(2, 0),
        );
        assert!((front.length() - 1.0).abs() < 1e-6, "{front:?}");

        for row in 0..3 {
            for col in 0..3 {
                assert!(
                    (marker.get(row, col) - placement.get(row, col) * PROXY_SIZE).abs() < 1e-5,
                    "basis element ({row},{col})"
                );
            }
        }
        for row in 0..4 {
            assert_eq!(
                marker.get(row, 3),
                placement.get(row, 3),
                "column 3 row {row}"
            );
        }
        assert_eq!(placement.transform_point(Vec3::ZERO), placed[0].position);
    }

    #[test]
    fn a_non_finite_angle_or_z_does_not_produce_a_non_finite_transform() {
        let hm = flat_map(4, 4, 500);
        let objects = vec![object(5.0, 5.0, f32::NAN, f32::NAN)];
        let placed = place_objects(&hm, 0.04, &objects, &[]);
        for value in placed[0].marker_transform().to_cols_array() {
            assert!(value.is_finite(), "transform holds {value}");
        }
    }
}
