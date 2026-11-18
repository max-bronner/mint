use super::*;
use mint_render::{skin_point, SkinVertex, Vertex};

fn vertex(position: [f32; 3]) -> Vertex {
    Vertex::new(position, [0.0, 0.0, 1.0], [0.0, 0.0])
}

#[test]
fn the_posed_box_contains_every_posed_vertex() {
    let vertices = [
        vertex([0.0, 0.0, 0.0]),
        vertex([1.0, 2.0, 3.0]),
        vertex([-4.0, 0.5, 1.0]),
        vertex([2.0, -1.0, 0.0]),
    ];
    let skin = [
        SkinVertex {
            bones: [0, 1],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        },
        SkinVertex {
            bones: [0, 1],
            weights: [0.25, 0.75],
            position2: [5.0, 0.0, -2.0],
            normal2: [0.0, 0.0, 1.0],
        },
        SkinVertex {
            bones: [1, 0],
            weights: [0.6, 0.4],
            position2: [0.0, 3.0, 0.0],
            normal2: [0.0, 0.0, 1.0],
        },
        SkinVertex {
            bones: [1, 0],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        },
    ];
    let c = core::f32::consts::FRAC_1_SQRT_2;
    let bones = [
        Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        Mat4::from_cols([
            [c, c, 0.0, 0.0],
            [-c, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 7.0, 1.0],
        ]),
    ];
    let boxes = bone_boxes(&vertices, &skin);
    let posed = posed_bounds(&boxes, &bones);
    for (v, s) in vertices.iter().zip(&skin) {
        let (p, _, _) = skin_point(
            &bones,
            Vec3::from_array(v.position),
            Vec3::from_array(v.normal),
            Vec3::ZERO,
            s,
        );
        let (min, max) = (posed.min(), posed.max());
        let eps = 1e-4;
        assert!(
            p.x >= min.x - eps
                && p.y >= min.y - eps
                && p.z >= min.z - eps
                && p.x <= max.x + eps
                && p.y <= max.y + eps
                && p.z <= max.z + eps,
            "{p:?} outside {posed:?}"
        );
    }
}

#[test]
fn the_shaders_inputs_reproduce_the_cpu_skin_exactly() {
    use crate::convert::{skin_mesh, SecondBoneVertex, SkinSource};
    use mint_render::MeshData;
    use mint_sage_w3d::VertexInfluence;
    let v = |p: [f32; 3], n: [f32; 3]| {
        Vertex::new(p, n, [0.0, 0.0]).with_tangent([0.0, 1.0, 0.0, -1.0])
    };
    let influence = |bones: [u16; 2], weights: [u16; 2]| VertexInfluence { bones, weights };
    let source = SkinSource {
        bone_space: MeshData {
            vertices: vec![
                v([1.0, 2.0, 3.0], [0.0, 0.0, 1.0]),
                v([-1.0, 0.5, 2.0], [0.6, 0.0, 0.8]),
                v([0.25, -3.0, 1.0], [0.0, 1.0, 0.0]),
                v([2.0, 2.0, -1.0], [1.0, 0.0, 0.0]),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
        },
        second_bone_space: vec![
            SecondBoneVertex {
                position: [4.0, -1.0, 0.5],
                normal: [0.0, 0.6, 0.8],
            },
            SecondBoneVertex {
                position: [0.0, 0.0, 7.0],
                normal: [1.0, 0.0, 0.0],
            },
            SecondBoneVertex {
                position: [-2.0, 1.0, 1.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        influences: vec![
            influence([0, 1], [60, 30]),
            influence([2, 1], [55, 45]),
            influence([1, 9], [50, 50]),
            influence([2, 0], [0, 0]),
        ],
    };
    let c = core::f32::consts::FRAC_1_SQRT_2;
    let bones = [
        Mat4::from_translation(Vec3::new(1.0, -2.0, 0.5)),
        Mat4::from_cols([
            [c, c, 0.0, 0.0],
            [-c, c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 3.0, 1.0, 1.0],
        ]),
        Mat4::from_cols([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [-4.0, 0.0, 2.0, 1.0],
        ]),
    ];
    let cpu = skin_mesh(&source, &bones);
    let skin = skin_vertices(&source, bones.len());
    assert_eq!(skin.len(), 4);
    for ((stored, posed), entry) in source
        .bone_space
        .vertices
        .iter()
        .zip(&cpu.vertices)
        .zip(&skin)
    {
        let (p, n, t) = skin_point(
            &bones,
            Vec3::from_array(stored.position),
            Vec3::from_array(stored.normal),
            Vec3::new(stored.tangent[0], stored.tangent[1], stored.tangent[2]),
            entry,
        );
        assert_eq!(p.to_array(), posed.position);
        assert_eq!(n.to_array(), posed.normal);
        assert_eq!([t.x, t.y, t.z, stored.tangent[3]], posed.tangent);
        assert!(entry.bone_span() <= 3, "{entry:?} reads past the skeleton");
    }
}

#[test]
fn every_posed_vertex_stays_inside_the_clip_radius() {
    use mint_sage_asset::{pivot_reach, sample_pose};
    use mint_sage_w3d::{
        Animation, Channel, ChannelData, ChannelKind, Hierarchy, HierarchyHeader, Pivot, Quat,
        Value, Vec3 as W3dVec3,
    };
    let q = |z: f32, w: f32| Quat {
        x: 0.0,
        y: 0.0,
        z,
        w,
    };
    let pivot = |parent, x, y, z, rotation| Pivot {
        name: String::new(),
        parent,
        translation: W3dVec3 { x, y, z },
        euler_angles: W3dVec3::default(),
        rotation,
    };
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot(None, 0.0, 0.0, 0.5, q(0.0, 1.0)),
            pivot(Some(0), 0.0, 1.5, 0.0, q(0.0, 1.0)),
            pivot(Some(1), 2.0, 0.0, 0.0, q(0.382_683, 0.923_880)),
        ],
    };
    let channel = |pivot, kind, values: Vec<Value>| Channel {
        pivot,
        kind,
        declared_vector_len: None,
        data: ChannelData::Dense {
            first_frame: 0,
            values,
        },
    };
    let spin: Vec<Value> = [
        (0.0, 1.0),
        (0.5, 0.866),
        (0.866, 0.5),
        (1.0, 0.0),
        (0.707, -0.707),
    ]
    .into_iter()
    .map(|(z, w)| Value::Rotation(q(z, w)))
    .collect();
    let animation = Animation {
        name: String::new(),
        hierarchy_name: String::new(),
        num_frames: 5,
        frame_rate: 30,
        compressed: false,
        flavor: None,
        channel_chunks: 3,
        channels: vec![
            channel(1, ChannelKind::Rotation, spin.clone()),
            channel(2, ChannelKind::Rotation, spin),
            channel(
                2,
                ChannelKind::TranslationX,
                [0.0, 1.0, -0.5, 2.5, 0.0]
                    .into_iter()
                    .map(Value::Scalar)
                    .collect(),
            ),
        ],
    };
    let vertices = [
        vertex([0.5, 0.2, 0.0]),
        vertex([1.0, -0.4, 0.3]),
        vertex([-0.3, 0.9, -0.2]),
    ];
    let skin = [
        SkinVertex {
            bones: [1, 2],
            weights: [0.5, 0.5],
            position2: [0.8, 0.0, 0.1],
            normal2: [0.0, 0.0, 1.0],
        },
        SkinVertex {
            bones: [2, 0],
            weights: [1.0, 0.0],
            ..SkinVertex::default()
        },
        SkinVertex {
            bones: [0, 2],
            weights: [0.3, 0.7],
            position2: [-1.2, 0.4, 0.0],
            normal2: [0.0, 0.0, 1.0],
        },
    ];
    let reach = pivot_reach(&hierarchy, &animation);
    let boxes = bone_boxes(&vertices, &skin);
    let radius = radius_of(&reach, [PartReach::Skinned(&boxes)]);
    assert!(radius.is_finite());
    let mut furthest = 0.0f32;
    for step in -10..=60 {
        let frame = step as f32 * 0.1;
        let pose = sample_pose(&hierarchy, &animation, frame);
        let bones = crate::convert::pose_to_matrices(&pose.world);
        for (v, s) in vertices.iter().zip(&skin) {
            let (p, _, _) = skin_point(
                &bones,
                Vec3::from_array(v.position),
                Vec3::from_array(v.normal),
                Vec3::ZERO,
                s,
            );
            furthest = furthest.max(p.length());
            assert!(
                p.length() <= radius * (1.0 + 1e-4),
                "frame {frame}: {p:?} is {} from the origin, radius {radius}",
                p.length()
            );
        }
    }
    assert!(furthest > radius * 0.5, "the radius is never approached");
}

#[test]
fn an_unboundable_part_makes_the_radius_infinite() {
    let unit = Aabb::from_corners(Vec3::splat(-1.0), Vec3::splat(1.0));
    assert!(radius_of(&[1.0], [PartReach::Unbounded]).is_infinite());
    assert!(radius_of(&[1.0], [PartReach::Driven(4, unit)]).is_infinite());
    let boxes = [(3usize, unit)];
    assert!(radius_of(&[1.0], [PartReach::Skinned(&boxes)]).is_infinite());
    let fixed = radius_of(&[], [PartReach::Fixed(unit)]);
    assert!((fixed - 3f32.sqrt()).abs() < 1e-6);
    let driven = radius_of(
        &[0.0, 2.0],
        [PartReach::Driven(1, unit), PartReach::Fixed(unit)],
    );
    assert!((driven - (2.0 + 3f32.sqrt())).abs() < 1e-6);
}

#[test]
fn only_weighted_bones_contribute_a_box() {
    let boxes = bone_boxes(
        &[vertex([1.0, 1.0, 1.0])],
        &[SkinVertex {
            bones: [2, 5],
            weights: [1.0, 0.0],
            position2: [99.0, 99.0, 99.0],
            ..SkinVertex::default()
        }],
    );
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].0, 2);
}
