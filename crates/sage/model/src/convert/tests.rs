use super::*;
use mint_sage_w3d::{ChunkType, HierarchyHeader, Pivot, Vec3 as W3dVec3};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-5
}

#[test]
fn an_identity_quaternion_is_the_identity_matrix() {
    assert_eq!(rotation_matrix(Quat::default()), Mat4::IDENTITY);
}

#[test]
fn a_quarter_turn_about_z_maps_x_onto_y() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let q = Quat {
        x: 0.0,
        y: 0.0,
        z: half,
        w: half,
    };
    let rotated = rotation_matrix(q).transform_vector(Vec3::X);
    assert!(
        close(rotated.x, 0.0) && close(rotated.y, 1.0) && close(rotated.z, 0.0),
        "got {rotated:?}"
    );
}

#[test]
fn an_unnormalised_quaternion_does_not_scale_the_mesh() {
    let q = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 4.0,
    };
    let scaled = rotation_matrix(q).transform_vector(Vec3::X);
    assert!(close(scaled.length(), 1.0), "length {}", scaled.length());
}

#[test]
fn a_zero_quaternion_yields_the_identity_rather_than_nan() {
    let q = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    assert_eq!(rotation_matrix(q), Mat4::IDENTITY);
}

fn pivot(name: &str, parent: Option<u32>, translation: W3dVec3) -> Pivot {
    Pivot {
        name: name.to_owned(),
        parent,
        translation,
        euler_angles: W3dVec3::default(),
        rotation: Quat::default(),
    }
}

#[test]
fn child_pivots_compose_their_parents_transforms() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot(
                "ROOT",
                None,
                W3dVec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                },
            ),
            pivot(
                "A",
                Some(0),
                W3dVec3 {
                    x: 0.0,
                    y: 2.0,
                    z: 0.0,
                },
            ),
            pivot(
                "B",
                Some(1),
                W3dVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 3.0,
                },
            ),
        ],
    };
    let world = pivot_world_transforms(&hierarchy);
    assert_eq!(world.len(), 3);

    let placed = world[2].transform_point(Vec3::ZERO);
    assert!(
        close(placed.x, 1.0) && close(placed.y, 2.0) && close(placed.z, 3.0),
        "leaf landed at {placed:?}"
    );
}

#[test]
fn a_forward_parent_reference_falls_back_rather_than_looping() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![pivot(
            "ODD",
            Some(5),
            W3dVec3 {
                x: 7.0,
                y: 0.0,
                z: 0.0,
            },
        )],
    };
    let world = pivot_world_transforms(&hierarchy);
    let placed = world[0].transform_point(Vec3::ZERO);
    assert!(close(placed.x, 7.0), "got {placed:?}");
}

#[test]
fn a_pivot_that_is_its_own_parent_does_not_recurse() {
    let hierarchy = Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![pivot("SELF", Some(0), W3dVec3::default())],
    };
    assert_eq!(pivot_world_transforms(&hierarchy).len(), 1);
}

#[test]
fn an_empty_hierarchy_yields_no_transforms() {
    assert!(pivot_world_transforms(&Hierarchy::default()).is_empty());
}

fn one_vertex_mesh(position: [f32; 3]) -> MeshData {
    MeshData {
        vertices: vec![Vertex::new(position, [0.0, 0.0, 1.0], [0.0, 0.0])],
        indices: vec![0, 0, 0],
    }
}

fn influence(bone: u16) -> VertexInfluence {
    VertexInfluence {
        bones: [bone, 0],
        weights: [100, 0],
    }
}

#[test]
fn skin_in_place_moves_a_fully_weighted_vertex_by_its_bones_matrix() {
    let mut data = one_vertex_mesh([1.0, 0.0, 0.0]);
    let bones = vec![Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0))];
    let influences = vec![influence(0)];
    assert!(skin_in_place(&mut data, &influences, &[], &bones));
    assert_eq!(data.vertices[0].position, [1.0, 5.0, 0.0]);
}

#[test]
fn skin_in_place_gives_the_second_bone_its_own_copy_of_the_vertex() {
    let mut data = one_vertex_mesh([1.0, 0.0, 0.0]);
    let bones = vec![
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)),
    ];
    let influences = vec![VertexInfluence {
        bones: [0, 1],
        weights: [50, 50],
    }];
    let second = vec![SecondBoneVertex {
        position: [1.0, -10.0, 0.0],
        normal: [0.0, 0.0, 1.0],
    }];
    assert!(skin_in_place(&mut data, &influences, &second, &bones));
    assert_eq!(data.vertices[0].position, [1.0, 0.0, 0.0]);
}

#[test]
fn skin_in_place_falls_back_to_the_first_copy_when_there_is_no_second_array() {
    let mut data = one_vertex_mesh([1.0, 0.0, 0.0]);
    let bones = vec![
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)),
    ];
    let influences = vec![VertexInfluence {
        bones: [0, 1],
        weights: [50, 50],
    }];
    assert!(skin_in_place(&mut data, &influences, &[], &bones));
    assert_eq!(data.vertices[0].position, [1.0, 5.0, 0.0]);
}

#[test]
fn skin_in_place_does_nothing_when_there_is_no_usable_skeleton() {
    let mut data = one_vertex_mesh([1.0, 0.0, 0.0]);
    let influences = vec![influence(0)];
    assert!(!skin_in_place(&mut data, &influences, &[], &[]));
    assert_eq!(data.vertices[0].position, [1.0, 0.0, 0.0]);
}

#[test]
fn skin_in_place_does_nothing_when_influences_do_not_match_the_vertex_count() {
    let mut data = one_vertex_mesh([1.0, 0.0, 0.0]);
    let bones = vec![Mat4::from_translation(Vec3::new(0.0, 5.0, 0.0))];
    assert!(!skin_in_place(&mut data, &[], &[], &bones));
    assert_eq!(data.vertices[0].position, [1.0, 0.0, 0.0]);
}

#[test]
fn skin_mesh_reposes_the_same_bone_space_source_for_two_different_poses() {
    let source = SkinSource {
        bone_space: one_vertex_mesh([1.0, 0.0, 0.0]),
        second_bone_space: Vec::new(),
        influences: vec![influence(0)],
    };
    let bind = skin_mesh(&source, &[Mat4::IDENTITY]);
    assert_eq!(bind.vertices[0].position, [1.0, 0.0, 0.0]);

    let moved = skin_mesh(
        &source,
        &[Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0))],
    );
    assert_eq!(moved.vertices[0].position, [11.0, 0.0, 0.0]);

    assert_eq!(source.bone_space.vertices[0].position, [1.0, 0.0, 0.0]);
}

#[test]
fn pose_to_matrices_matches_the_bind_pose_construction() {
    let translation = W3dVec3 {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let rotation = {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        Quat {
            x: 0.0,
            y: 0.0,
            z: half,
            w: half,
        }
    };
    let world = vec![PoseTransform::new(translation, rotation)];
    let matrices = pose_to_matrices(&world);
    assert_eq!(matrices.len(), 1);

    let expected =
        Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)).mul_mat4(rotation_matrix(rotation));
    assert_eq!(matrices[0], expected);
}

const HAS_SUBCHUNKS: u32 = 0x8000_0000;

fn chunk(kind: ChunkType, children: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&kind.0.to_le_bytes());
    let mut size = u32::try_from(payload.len()).expect("test payloads are small");
    if children {
        size |= HAS_SUBCHUNKS;
    }
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn floats(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::new();
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn mesh_header(name: &str, tris: u32, vertices: u32) -> Vec<u8> {
    let mut buf = vec![0u8; 116];
    buf[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    let name = name.as_bytes();
    let n = name.len().min(15);
    buf[8..8 + n].copy_from_slice(&name[..n]);
    buf[40..44].copy_from_slice(&tris.to_le_bytes());
    buf[44..48].copy_from_slice(&vertices.to_le_bytes());
    buf
}

fn triangle(a: u32, b: u32, c: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for index in [a, b, c] {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 20]);
    out
}

fn shader_material(property: &str, value: &str) -> Vec<u8> {
    let mut header = vec![1u8];
    header.resize(33, 0);
    header.extend_from_slice(&2u32.to_le_bytes());

    let with_nul =
        |text: &str| u32::try_from(text.len().saturating_add(1)).expect("test names are short");
    let mut field = Vec::new();
    field.extend_from_slice(&1u32.to_le_bytes());
    field.extend_from_slice(&with_nul(property).to_le_bytes());
    field.extend_from_slice(property.as_bytes());
    field.push(0);
    field.extend_from_slice(&with_nul(value).to_le_bytes());
    field.extend_from_slice(value.as_bytes());
    field.push(0);

    let mut body = chunk(ChunkType::SHADER_MATERIAL_HEADER, false, &header);
    body.extend_from_slice(&chunk(ChunkType::SHADER_MATERIAL_PROPERTY, false, &field));
    chunk(
        ChunkType::SHADER_MATERIALS,
        true,
        &chunk(ChunkType::SHADER_MATERIAL, true, &body),
    )
}

#[derive(Default)]
struct MeshSpec<'a> {
    name: &'a str,
    positions: Vec<f32>,
    normals: Vec<f32>,
    texcoords: Vec<f32>,
    triangles: Vec<[u32; 3]>,
    tangents: Vec<f32>,
    bitangents: Vec<f32>,
    normal_map: Option<&'a str>,
    dest_blends: Vec<u8>,
    shader_flags: Vec<(u8, u8)>,
    src_blends: Vec<u8>,
    detail_color_funcs: Vec<u8>,
    pass_shader_ids: Vec<u32>,
    vertex_material: Option<TestVertexMaterial>,
    extra_vertex_materials: Vec<TestVertexMaterial>,
    vertex_material_ids: Vec<u32>,
    textures: Vec<&'a str>,
    face_texture_ids: Vec<u32>,
    face_shader_ids: Vec<u32>,
    face_texture2_ids: Vec<u32>,
}

struct TestVertexMaterial {
    attributes: u32,
    ambient: [u8; 3],
    diffuse: [u8; 3],
    emissive: [u8; 3],
    opacity: f32,
    args0: &'static str,
    args1: &'static str,
}

fn shader_bytes(
    dest_blend: u8,
    depth_mask: u8,
    alpha_test: u8,
    src_blend: u8,
    detail_color_func: u8,
) -> [u8; 16] {
    let mut raw = [0u8; 16];
    if let Some(slot) = raw.get_mut(7) {
        *slot = src_blend;
    }
    if let Some(slot) = raw.get_mut(1) {
        *slot = depth_mask;
    }
    if let Some(slot) = raw.get_mut(3) {
        *slot = dest_blend;
    }
    if let Some(slot) = raw.get_mut(9) {
        *slot = detail_color_func;
    }
    if let Some(slot) = raw.get_mut(12) {
        *slot = alpha_test;
    }
    raw
}

fn vertex_material_entry(vm: &TestVertexMaterial) -> Vec<u8> {
    let mut info = vec![0u8; 32];
    info[0..4].copy_from_slice(&vm.attributes.to_le_bytes());
    info[4..7].copy_from_slice(&vm.ambient);
    info[8..11].copy_from_slice(&vm.diffuse);
    info[16..19].copy_from_slice(&vm.emissive);
    info[24..28].copy_from_slice(&vm.opacity.to_le_bytes());
    let mut entry = chunk(ChunkType::VERTEX_MATERIAL_NAME, false, b"TestMat\0");
    entry.extend_from_slice(&chunk(ChunkType::VERTEX_MATERIAL_INFO, false, &info));
    entry.extend_from_slice(&chunk(
        ChunkType::VERTEX_MAPPER_ARGS0,
        false,
        vm.args0.as_bytes(),
    ));
    entry.extend_from_slice(&chunk(
        ChunkType::VERTEX_MAPPER_ARGS1,
        false,
        vm.args1.as_bytes(),
    ));
    chunk(ChunkType::VERTEX_MATERIAL, true, &entry)
}

fn build_model(spec: &MeshSpec<'_>) -> Vec<u8> {
    let vertices = u32::try_from(spec.positions.len() / 3).unwrap_or(0);
    let mut body = chunk(
        ChunkType::MESH_HEADER3,
        false,
        &mesh_header(
            spec.name,
            u32::try_from(spec.triangles.len()).unwrap_or(0),
            vertices,
        ),
    );
    body.extend_from_slice(&chunk(ChunkType::VERTICES, false, &floats(&spec.positions)));
    if !spec.normals.is_empty() {
        body.extend_from_slice(&chunk(
            ChunkType::VERTEX_NORMALS,
            false,
            &floats(&spec.normals),
        ));
    }
    let mut triangles = Vec::new();
    for [a, b, c] in &spec.triangles {
        triangles.extend_from_slice(&triangle(*a, *b, *c));
    }
    body.extend_from_slice(&chunk(ChunkType::TRIANGLES, false, &triangles));
    if let Some(vm) = &spec.vertex_material {
        let mut table = vertex_material_entry(vm);
        for extra in &spec.extra_vertex_materials {
            table.extend_from_slice(&vertex_material_entry(extra));
        }
        body.extend_from_slice(&chunk(ChunkType::VERTEX_MATERIALS, true, &table));
    }
    if !spec.textures.is_empty() {
        let mut table = Vec::new();
        for name in &spec.textures {
            let mut entry = name.as_bytes().to_vec();
            entry.push(0);
            table.extend_from_slice(&chunk(
                ChunkType::TEXTURE,
                true,
                &chunk(ChunkType::TEXTURE_NAME, false, &entry),
            ));
        }
        body.extend_from_slice(&chunk(ChunkType::TEXTURES, true, &table));
    }
    let wants_pass = !spec.texcoords.is_empty()
        || spec.vertex_material.is_some()
        || !spec.pass_shader_ids.is_empty()
        || !spec.face_texture_ids.is_empty()
        || !spec.face_shader_ids.is_empty()
        || !spec.vertex_material_ids.is_empty();
    if wants_pass {
        let shader_ids: Vec<Option<u32>> = if spec.pass_shader_ids.is_empty() {
            vec![None]
        } else {
            spec.pass_shader_ids.iter().copied().map(Some).collect()
        };
        for (index, shader_id) in shader_ids.iter().enumerate() {
            let mut pass = Vec::new();
            if index == 0 && !spec.vertex_material_ids.is_empty() {
                let mut ids = Vec::new();
                for id in &spec.vertex_material_ids {
                    ids.extend_from_slice(&id.to_le_bytes());
                }
                pass.extend_from_slice(&chunk(ChunkType::VERTEX_MATERIAL_IDS, false, &ids));
            } else if index == 0 && spec.vertex_material.is_some() {
                pass.extend_from_slice(&chunk(
                    ChunkType::VERTEX_MATERIAL_IDS,
                    false,
                    &0u32.to_le_bytes(),
                ));
            }
            if index == 0 && !spec.face_shader_ids.is_empty() {
                let mut ids = Vec::new();
                for id in &spec.face_shader_ids {
                    ids.extend_from_slice(&id.to_le_bytes());
                }
                pass.extend_from_slice(&chunk(ChunkType::SHADER_IDS, false, &ids));
            } else if let Some(id) = shader_id {
                pass.extend_from_slice(&chunk(ChunkType::SHADER_IDS, false, &id.to_le_bytes()));
            }
            if index == 0 && !spec.texcoords.is_empty() {
                pass.extend_from_slice(&chunk(
                    ChunkType::STAGE_TEXCOORDS,
                    false,
                    &floats(&spec.texcoords),
                ));
            }
            if index == 0 && !spec.face_texture_ids.is_empty() {
                let mut ids = Vec::new();
                for id in &spec.face_texture_ids {
                    ids.extend_from_slice(&id.to_le_bytes());
                }
                pass.extend_from_slice(&chunk(
                    ChunkType::TEXTURE_STAGE,
                    true,
                    &chunk(ChunkType::TEXTURE_IDS, false, &ids),
                ));
            }
            if index == 0 && !spec.face_texture2_ids.is_empty() {
                let mut ids = Vec::new();
                for id in &spec.face_texture2_ids {
                    ids.extend_from_slice(&id.to_le_bytes());
                }
                pass.extend_from_slice(&chunk(
                    ChunkType::TEXTURE_STAGE,
                    true,
                    &chunk(ChunkType::TEXTURE_IDS, false, &ids),
                ));
            }
            body.extend_from_slice(&chunk(ChunkType::MATERIAL_PASS, true, &pass));
        }
    }
    if !spec.tangents.is_empty() {
        body.extend_from_slice(&chunk(ChunkType::TANGENTS, false, &floats(&spec.tangents)));
    }
    if !spec.bitangents.is_empty() {
        body.extend_from_slice(&chunk(
            ChunkType::BITANGENTS,
            false,
            &floats(&spec.bitangents),
        ));
    }
    if let Some(map) = spec.normal_map {
        body.extend_from_slice(&shader_material("NormalMap", map));
    }
    if !spec.dest_blends.is_empty() {
        let mut table = Vec::new();
        for (index, dest_blend) in spec.dest_blends.iter().enumerate() {
            let (depth_mask, alpha_test) = spec.shader_flags.get(index).copied().unwrap_or((0, 0));
            let src_blend = spec.src_blends.get(index).copied().unwrap_or(0);
            let detail_color_func = spec.detail_color_funcs.get(index).copied().unwrap_or(0);
            table.extend_from_slice(&shader_bytes(
                *dest_blend,
                depth_mask,
                alpha_test,
                src_blend,
                detail_color_func,
            ));
        }
        body.extend_from_slice(&chunk(ChunkType::SHADERS, false, &table));
    }
    chunk(ChunkType::MESH, true, &body)
}

fn unit_triangle() -> MeshSpec<'static> {
    MeshSpec {
        name: "TRI",
        positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        texcoords: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        triangles: vec![[0, 1, 2]],
        ..MeshSpec::default()
    }
}

fn shipped_basis_triangle() -> MeshSpec<'static> {
    MeshSpec {
        tangents: vec![0.0, -1.0, 0.0, 0.0, -1.0, 0.0, 0.0, -1.0, 0.0],
        bitangents: vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        ..unit_triangle()
    }
}

fn convert_one(spec: &MeshSpec<'_>) -> ConvertedModel {
    let bytes = build_model(spec);
    let model = Model::parse(&bytes).expect("synthetic model parses");
    convert_model(&model, None).expect("synthetic model converts")
}

#[test]
fn a_mesh_with_per_face_texture_ids_draws_once_per_texture() {
    let spec = MeshSpec {
        name: "TWO",
        positions: vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 2.0, 1.0,
            0.0,
        ],
        triangles: vec![[0, 1, 2], [3, 4, 5], [0, 1, 2]],
        textures: vec!["first.tga", "second.tga"],
        face_texture_ids: vec![1, 0, 1],
        ..MeshSpec::default()
    };
    let parts = convert_one(&spec).parts;
    let drawn: Vec<(Option<&str>, usize)> = parts
        .iter()
        .map(|part| (part.texture.as_deref(), part.mesh.triangle_count()))
        .collect();
    assert_eq!(drawn, vec![(Some("second.tga"), 2), (Some("first.tga"), 1)]);
}

#[test]
fn a_mesh_with_per_face_shader_ids_draws_once_per_shader() {
    let spec = MeshSpec {
        name: "TWO",
        positions: vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 2.0, 1.0,
            0.0,
        ],
        triangles: vec![[0, 1, 2], [3, 4, 5], [0, 1, 2]],
        dest_blends: vec![0, 5],
        shader_flags: vec![(1, 0), (0, 0)],
        face_shader_ids: vec![1, 0, 1],
        ..MeshSpec::default()
    };
    let parts = convert_one(&spec).parts;
    let drawn: Vec<(Blend, usize)> = parts
        .iter()
        .map(|part| (part.blend, part.mesh.triangle_count()))
        .collect();
    assert_eq!(drawn, vec![(Blend::AlphaBlend, 2), (Blend::Opaque, 1)]);
}

#[test]
fn a_mesh_with_per_vertex_materials_draws_once_per_first_vertex_material() {
    let spec = MeshSpec {
        name: "FIREPLANE",
        positions: vec![
            0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 3.0, 0.0, 0.0, 2.0, 1.0,
            0.0,
        ],
        triangles: vec![[0, 1, 2], [3, 4, 5]],
        textures: vec!["card.tga"],
        face_texture_ids: vec![0, 0],
        face_texture2_ids: vec![0, 0],
        vertex_material: Some(TestVertexMaterial {
            attributes: 0,
            ambient: [255, 255, 255],
            diffuse: [255, 255, 255],
            emissive: [255, 0, 0],
            opacity: 1.0,
            args0: "",
            args1: "",
        }),
        extra_vertex_materials: vec![TestVertexMaterial {
            attributes: 0,
            ambient: [255, 255, 255],
            diffuse: [255, 255, 255],
            emissive: [0, 255, 0],
            opacity: 1.0,
            args0: "",
            args1: "",
        }],
        vertex_material_ids: vec![0, 0, 0, 1, 1, 1],
        dest_blends: vec![0, 0],
        detail_color_funcs: vec![1, 2],
        face_shader_ids: vec![0, 1],
        ..MeshSpec::default()
    };
    let parts = convert_one(&spec).parts;
    let drawn: Vec<([f32; 3], DetailCombine)> = parts
        .iter()
        .map(|part| (part.material_emissive, part.detail_combine))
        .collect();
    assert_eq!(
        drawn,
        vec![
            ([1.0, 0.0, 0.0], DetailCombine::Detail),
            ([0.0, 1.0, 0.0], DetailCombine::Scale),
        ]
    );
}

#[test]
fn a_mesh_with_no_vertex_material_gets_the_neutral_colours_and_no_uv_mapper() {
    let converted = convert_one(&unit_triangle());
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.material_ambient, [1.0, 1.0, 1.0]);
    assert_eq!(part.material_diffuse, [1.0, 1.0, 1.0]);
    assert_eq!(part.material_emissive, [0.0, 0.0, 0.0]);
    assert_eq!(part.material_opacity, 1.0);
    assert!(part.uv_mapper.is_none());
}

#[test]
fn vertex_material_colours_are_read_and_normalised_to_0_1() {
    let spec = MeshSpec {
        vertex_material: Some(TestVertexMaterial {
            attributes: 0,
            ambient: [255, 255, 255],
            diffuse: [0, 0, 0],
            emissive: [50, 73, 50],
            opacity: 1.0,
            args0: "",
            args1: "",
        }),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.material_ambient, [1.0, 1.0, 1.0]);
    assert_eq!(part.material_diffuse, [0.0, 0.0, 0.0]);
    let emissive = part.material_emissive;
    assert!((emissive[0] - 50.0 / 255.0).abs() < 1e-6, "{emissive:?}");
    assert!((emissive[1] - 73.0 / 255.0).abs() < 1e-6, "{emissive:?}");
    assert!((emissive[2] - 50.0 / 255.0).abs() < 1e-6, "{emissive:?}");
    assert!(part.uv_mapper.is_none());
}

#[test]
fn a_non_uv_mapper_on_either_stage_produces_an_animated_uv_mapper() {
    let spec = MeshSpec {
        vertex_material: Some(TestVertexMaterial {
            attributes: 0x0004_0800,
            ambient: [255, 255, 255],
            diffuse: [255, 255, 255],
            emissive: [0, 0, 0],
            opacity: 1.0,
            args0: "UPerSec=0.3\r\nVPerSec=0.3\r\nUScale=.001\r\nVScale=.001",
            args1: "Speed=0.03\r\nUCenter=0.5\r\nVCenter=0.5\r\nUScale=1.0\r\nVScale=1.0",
        }),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    let state = part.uv_mapper.as_ref().expect("a non-UV mapper animates");
    assert_eq!(state.info.stage0_mapper(), UvMapperType::LinearOffset);
    assert_eq!(state.info.stage1_mapper(), UvMapperType::Rotate);

    let (stage0_at0, stage1_at0, _) = sample_uv_transforms_for(state, 0.0);
    let (stage0_at1, stage1_at1, _) = sample_uv_transforms_for(state, 1.0);
    assert_ne!(
        stage0_at0, stage0_at1,
        "stage 0's LinearOffset must move over time"
    );
    assert_ne!(
        stage1_at0, stage1_at1,
        "stage 1's Rotate must move over time, independently of stage 0"
    );
}

#[test]
fn a_non_unit_opacity_is_read_from_the_vertex_material() {
    let spec = MeshSpec {
        vertex_material: Some(TestVertexMaterial {
            attributes: 0,
            ambient: [0, 0, 0],
            diffuse: [0, 0, 0],
            emissive: [60, 58, 30],
            opacity: 0.0,
            args0: "",
            args1: "",
        }),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.material_opacity, 0.0);
}

#[test]
fn the_v_axis_is_flipped_on_the_way_in_and_must_stay_that_way() {
    let spec = MeshSpec {
        texcoords: vec![0.0, 0.0, 0.25, 0.25, 1.0, 1.0],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    let got: Vec<[f32; 2]> = part.mesh.vertices.iter().map(|v| v.uv).collect();

    assert_eq!(
        got,
        vec![[0.0, 1.0], [0.25, 0.75], [1.0, 0.0]],
        "\n\nW3D texture coordinates have a BOTTOM-LEFT origin and this API has a \
             TOP-LEFT one, so `v = 1 - v` is applied during conversion. It is not \
             decoration: without it gbfarm_a.w3d puts its door on the roof and its \
             flowers up the walls, which was confirmed against three models in both \
             installations. If this test is failing because someone simplified the flip \
             away, put it back. See docs/research/w3d-format.md and convert::flip_v.\n"
    );
}

#[test]
fn the_flip_is_its_own_inverse_and_leaves_the_midpoint_alone() {
    for v in [0.0f32, 0.25, 0.5, 1.0] {
        let once = flip_v(Vec2 { u: 0.3, v });
        let twice = flip_v(Vec2 {
            u: once[0],
            v: once[1],
        });
        assert!((twice[1] - v).abs() < 1e-6, "flipping twice must return v");
        assert_eq!(once[0], 0.3, "u is never touched");
    }
    assert_eq!(flip_v(Vec2 { u: 0.0, v: 0.5 })[1], 0.5);
}

#[test]
fn a_shipped_tangent_basis_is_read_with_the_two_chunks_crossed_over() {
    let converted = convert_one(&shipped_basis_triangle());
    let part = converted.parts.first().expect("one part");

    for vertex in &part.mesh.vertices {
        assert_eq!(
            [vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]],
            [1.0, 0.0, 0.0],
            "the tangent must come from BITANGENTS, which holds dP/du"
        );
        assert_eq!(
            vertex.tangent[3], -1.0,
            "handedness follows the bitangent the file gives"
        );
    }
}

#[test]
fn the_shipped_basis_and_the_derived_one_agree_on_the_same_triangle() {
    let shipped = convert_one(&shipped_basis_triangle());
    let derived = convert_one(&MeshSpec {
        normal_map: Some("gbfarm_nrm.tga"),
        ..unit_triangle()
    });

    let basis = |model: &ConvertedModel| -> Vec<[f32; 4]> {
        model
            .parts
            .first()
            .map(|part| part.mesh.vertices.iter().map(|v| v.tangent).collect())
            .unwrap_or_default()
    };
    let (shipped, derived) = (basis(&shipped), basis(&derived));
    assert_eq!(shipped.len(), 3);
    for (from_file, computed) in shipped.iter().zip(&derived) {
        for (a, b) in from_file.iter().zip(computed) {
            assert!(
                (a - b).abs() < 1e-5,
                "shipped {from_file:?} against derived {computed:?}"
            );
        }
    }
}

#[test]
fn a_derived_tangent_points_the_way_u_increases() {
    let spec = MeshSpec {
        normal_map: Some("gbfarm_nrm.tga"),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.normal_map.as_deref(), Some("gbfarm_nrm.tga"));

    for vertex in &part.mesh.vertices {
        let [x, y, z, w] = vertex.tangent;
        assert!(
            (x - 1.0).abs() < 1e-5 && y.abs() < 1e-5 && z.abs() < 1e-5,
            "derived tangent should run along +X, got {:?}",
            [x, y, z]
        );
        assert_eq!(w, -1.0, "handedness");
    }
}

#[test]
fn a_derived_tangent_follows_the_texture_and_not_the_geometry() {
    let spec = MeshSpec {
        texcoords: vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        normal_map: Some("gbfarm_nrm.tga"),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");

    for vertex in &part.mesh.vertices {
        let [x, y, z, _] = vertex.tangent;
        assert!(
            x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5 && z.abs() < 1e-5,
            "tangent should follow the UVs onto +Y, got {:?}",
            [x, y, z]
        );
    }
}

#[test]
fn a_mesh_with_no_normal_map_gets_no_tangent_basis_at_all() {
    let converted = convert_one(&unit_triangle());
    let part = converted.parts.first().expect("one part");
    assert!(part.normal_map.is_none());
    for vertex in &part.mesh.vertices {
        assert_eq!(vertex.tangent, [0.0; 4]);
    }
}

#[test]
fn a_degenerate_uv_triangle_derives_nothing_rather_than_an_infinity() {
    let spec = MeshSpec {
        texcoords: vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
        normal_map: Some("gbfarm_nrm.tga"),
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    for vertex in &part.mesh.vertices {
        assert_eq!(vertex.tangent, [0.0; 4]);
        assert!(vertex.uv.iter().all(|value| value.is_finite()));
    }
}

#[test]
fn a_pick_mesh_is_converted_flagged_and_left_out_of_the_bounds() {
    let mut spec = unit_triangle();
    spec.name = "PICK";
    spec.positions = vec![-9.0, -9.0, 0.0, 9.0, -9.0, 0.0, 0.0, 9.0, 0.0];
    let converted = convert_one(&spec);

    let part = converted
        .parts
        .first()
        .expect("the proxy is still converted");
    assert!(part.pick_proxy);
    assert_eq!(part.mesh.vertices.len(), 3, "its geometry is intact");
    assert!(
        converted.bounds.is_empty(),
        "a model with nothing but a proxy has no drawable bounds"
    );
    assert!(
        converted.notes.iter().any(|note| note.contains("picking")),
        "the skip should be visible to whoever is looking: {:?}",
        converted.notes
    );
}

#[test]
fn ordinary_geometry_is_drawn_and_does_bound_the_model() {
    let converted = convert_one(&unit_triangle());
    let part = converted.parts.first().expect("one part");
    assert!(!part.pick_proxy);
    assert!(!converted.bounds.is_empty());
}

#[test]
fn a_mesh_with_no_shaders_chunk_is_opaque() {
    let converted = convert_one(&unit_triangle());
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.blend, Blend::Opaque);
}

#[test]
fn dest_blend_zero_is_opaque() {
    let spec = MeshSpec {
        dest_blends: vec![0],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Opaque
    );
}

#[test]
fn dest_blend_one_is_additive() {
    let spec = MeshSpec {
        dest_blends: vec![1],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Additive
    );
}

#[test]
fn dest_blend_one_minus_src_alpha_is_alpha_blend() {
    let spec = MeshSpec {
        dest_blends: vec![5],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::AlphaBlend
    );
}

#[test]
fn ea_blend_pair_table_is_applied() {
    let expected = [
        ((1u8, 0u8), Blend::Opaque),
        ((1, 1), Blend::Additive),
        ((2, 5), Blend::AlphaBlend),
        ((1, 4), Blend::AddScaledByAlpha),
        ((1, 2), Blend::AddScaledByColor),
        ((1, 3), Blend::AddInverseScaledByColor),
        ((0, 2), Blend::Multiply),
    ];
    for ((src_blend, dest_blend), want) in expected {
        let shader = Shader {
            src_blend,
            dest_blend,
            ..Shader::default()
        };
        assert_eq!(
            shader_blend(&shader),
            want,
            "(src_blend {src_blend}, dest_blend {dest_blend})"
        );
    }
}

#[test]
fn a_pair_outside_the_table_falls_back_rather_than_panicking() {
    let shader = Shader {
        src_blend: 3,
        dest_blend: 3,
        ..Shader::default()
    };
    assert_eq!(shader_blend(&shader), Blend::AlphaBlend);
}

#[test]
fn a_mesh_with_several_shaders_takes_the_most_transparent() {
    let spec = MeshSpec {
        dest_blends: vec![0, 1],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Additive
    );

    for dest_blends in [vec![5, 1], vec![1, 5]] {
        let spec = MeshSpec {
            dest_blends,
            ..unit_triangle()
        };
        let converted = convert_one(&spec);
        assert_eq!(
            converted.parts.first().expect("one part").blend,
            Blend::Additive
        );
    }
}

#[test]
fn every_blend_pair_survives_the_mesh_level_fold() {
    for ((src_blend, dest_blend), want) in [
        ((1u8, 4u8), Blend::AddScaledByAlpha),
        ((1, 2), Blend::AddScaledByColor),
        ((1, 3), Blend::AddInverseScaledByColor),
        ((0, 2), Blend::Multiply),
    ] {
        for (src_blends, dest_blends) in [
            (vec![src_blend], vec![dest_blend]),
            (vec![1, src_blend], vec![0, dest_blend]),
        ] {
            let spec = MeshSpec {
                src_blends,
                dest_blends,
                ..unit_triangle()
            };
            let converted = convert_one(&spec);
            assert_eq!(
                converted.parts.first().expect("one part").blend,
                want,
                "(src_blend {src_blend}, dest_blend {dest_blend})"
            );
        }
    }
}

#[test]
fn a_second_material_pass_does_not_decide_the_first_pass_blend() {
    let spec = MeshSpec {
        dest_blends: vec![0, 1],
        pass_shader_ids: vec![0, 1],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Opaque
    );

    let spec = MeshSpec {
        dest_blends: vec![0, 1],
        pass_shader_ids: vec![1, 0],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Additive
    );
}

#[test]
fn a_second_material_pass_does_not_decide_the_first_pass_alpha_test() {
    let spec = MeshSpec {
        dest_blends: vec![0, 0],
        shader_flags: vec![(1, 0), (1, 1)],
        pass_shader_ids: vec![0, 1],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert!(!converted.parts.first().expect("one part").alpha_test);

    let spec = MeshSpec {
        dest_blends: vec![0, 0],
        shader_flags: vec![(1, 0), (1, 1)],
        pass_shader_ids: vec![1, 0],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert!(converted.parts.first().expect("one part").alpha_test);
}

#[test]
fn an_alpha_blended_shader_that_writes_depth_and_alpha_tests_is_a_cutout() {
    let spec = MeshSpec {
        dest_blends: vec![5],
        shader_flags: vec![(1, 1)],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.blend, Blend::Opaque);
    assert!(part.alpha_test);
}

#[test]
fn the_cutout_reading_needs_both_the_depth_write_and_the_alpha_test() {
    for flags in [(1u8, 0u8), (0, 1), (0, 0)] {
        let spec = MeshSpec {
            dest_blends: vec![5],
            shader_flags: vec![flags],
            ..unit_triangle()
        };
        let converted = convert_one(&spec);
        assert_eq!(
            converted.parts.first().expect("one part").blend,
            Blend::AlphaBlend,
            "depth_mask {}, alpha_test {}",
            flags.0,
            flags.1
        );
    }
}

#[test]
fn an_alpha_tested_additive_shader_is_still_additive() {
    let spec = MeshSpec {
        dest_blends: vec![1],
        shader_flags: vec![(1, 1)],
        ..unit_triangle()
    };
    let converted = convert_one(&spec);
    assert_eq!(
        converted.parts.first().expect("one part").blend,
        Blend::Additive
    );
}

fn hlod_with_one_sub_object(mesh_name: &str, bone_index: u32) -> Vec<u8> {
    let mut header = vec![0u8; 40];
    header[4..8].copy_from_slice(&1u32.to_le_bytes());
    write_name(&mut header[8..24], "RIG");
    write_name(&mut header[24..40], "RIGSKEL");

    let mut sub_object = vec![0u8; 36];
    sub_object[0..4].copy_from_slice(&bone_index.to_le_bytes());
    write_name(&mut sub_object[4..], &format!("RIG.{mesh_name}"));

    let mut array_header = vec![0u8; 8];
    array_header[4..8].copy_from_slice(&f32::MAX.to_le_bytes());

    chunk(
        ChunkType::HLOD,
        true,
        &[
            chunk(ChunkType::HLOD_HEADER, false, &header),
            chunk(
                ChunkType::HLOD_LOD_ARRAY,
                true,
                &[
                    chunk(
                        ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER,
                        false,
                        &array_header,
                    ),
                    chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub_object),
                ]
                .concat(),
            ),
        ]
        .concat(),
    )
}

fn write_name(buf: &mut [u8], name: &str) {
    let bytes = name.as_bytes();
    let n = bytes.len().min(buf.len());
    if let Some(slot) = buf.get_mut(..n) {
        slot.copy_from_slice(&bytes[..n]);
    }
}

fn two_pivot_hierarchy() -> Hierarchy {
    Hierarchy {
        header: HierarchyHeader::default(),
        pivots: vec![
            pivot("ROOT", None, W3dVec3::default()),
            pivot(
                "BONE",
                Some(0),
                W3dVec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 5.0,
                },
            ),
        ],
    }
}

#[test]
fn a_rigid_mesh_bound_by_hlod_carries_its_pivot_index_forward() {
    let mesh_bytes = build_model(&unit_triangle());
    let bytes = [mesh_bytes, hlod_with_one_sub_object("TRI", 1)].concat();
    let model = Model::parse(&bytes).expect("mesh + HLOD parses");
    let hierarchy = two_pivot_hierarchy();

    let converted = convert_model(&model, Some(&hierarchy)).expect("converts");
    let part = converted.parts.first().expect("one part");

    assert_eq!(
        part.pivot,
        Some(1),
        "the sub-object named bone_index 1; the part must carry that index \
             forward so an animation player can re-transform it every frame"
    );
    assert!(!part.skinned, "this mesh has no VERTEX_INFLUENCES");
    assert!(part.skin.is_none(), "a rigid part has no SkinSource");

    let placed = part.transform.transform_point(Vec3::ZERO);
    assert!(close(placed.z, 5.0), "got {placed:?}");
}

#[test]
fn a_mesh_with_no_hlod_binding_has_no_pivot_and_sits_at_the_origin() {
    let converted = convert_one(&unit_triangle());
    let part = converted.parts.first().expect("one part");
    assert_eq!(part.pivot, None);
    assert_eq!(part.transform, Mat4::IDENTITY);
}

#[test]
fn an_hlod_binding_with_no_hierarchy_at_all_still_converts_without_a_pivot_transform() {
    let mesh_bytes = build_model(&unit_triangle());
    let bytes = [mesh_bytes, hlod_with_one_sub_object("TRI", 1)].concat();
    let model = Model::parse(&bytes).expect("mesh + HLOD parses");

    let converted = convert_model(&model, None).expect("converts with no hierarchy");
    let part = converted.parts.first().expect("one part");
    assert_eq!(
        part.pivot,
        Some(1),
        "the file's own binding is preserved even with no bones to place it"
    );
    assert_eq!(
        part.transform,
        Mat4::IDENTITY,
        "with no hierarchy there is nothing to place it with"
    );
}

const ZERO: [f32; 3] = [0.0, 0.0, 0.0];
const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

#[test]
fn no_dcg_array_is_the_baseline_regardless_of_material() {
    let sources = dcg_sources(false, false, RED, GREEN, BLUE);
    assert_eq!(sources, DcgSources::NO_DCG);
}

#[test]
fn diffuse_only_premultiplies_by_diffuse_and_keeps_ambient_on_the_material() {
    let sources = dcg_sources(true, false, ZERO, GREEN, ZERO);
    assert!(!sources.ambient_from_vertex);
    assert!(sources.diffuse_from_vertex);
    assert!(!sources.lighting_disabled);
    assert_eq!(sources.premultiply, Some(GREEN));
    assert!(!sources.dcg_multiplies_result);
}

#[test]
fn diffuse_and_ambient_premultiplies_by_diffuse_and_moves_ambient_to_the_vertex_too() {
    let sources = dcg_sources(true, false, RED, GREEN, ZERO);
    assert!(sources.ambient_from_vertex);
    assert!(sources.diffuse_from_vertex);
    assert!(!sources.lighting_disabled);
    assert_eq!(sources.premultiply, Some(GREEN));
    assert!(!sources.dcg_multiplies_result);
}

#[test]
fn ambient_only_premultiplies_by_ambient_and_reverts_diffuse_to_the_material() {
    let sources = dcg_sources(true, false, RED, ZERO, ZERO);
    assert!(sources.ambient_from_vertex);
    assert!(
        !sources.diffuse_from_vertex,
        "the one branch that undoes COLOR1"
    );
    assert!(!sources.lighting_disabled);
    assert_eq!(sources.premultiply, Some(RED));
    assert!(!sources.dcg_multiplies_result);
}

#[test]
fn emissive_only_premultiplies_by_emissive_and_disables_lighting() {
    let sources = dcg_sources(true, false, ZERO, ZERO, BLUE);
    assert!(!sources.ambient_from_vertex);
    assert!(sources.diffuse_from_vertex);
    assert!(sources.lighting_disabled);
    assert_eq!(sources.premultiply, Some(BLUE));
    assert!(!sources.dcg_multiplies_result);
}

#[test]
fn every_other_combination_falls_through_with_no_premultiply() {
    for (a, d, e) in [
        (ZERO, ZERO, ZERO),
        (RED, ZERO, BLUE),
        (ZERO, GREEN, BLUE),
        (RED, GREEN, BLUE),
    ] {
        let sources = dcg_sources(true, false, a, d, e);
        assert_eq!(sources.premultiply, None, "a={a:?} d={d:?} e={e:?}");
        assert!(!sources.ambient_from_vertex);
        assert!(sources.diffuse_from_vertex);
        assert!(!sources.lighting_disabled);
        assert!(!sources.dcg_multiplies_result);
    }
}

fn mesh_with_shader_and_tables(
    shader: mint_sage_w3d::Shader,
    pass_count: usize,
    vertex_material_count: usize,
    stage1_without_stage0: bool,
) -> Mesh<'static> {
    let raw: [u8; 16] = [
        shader.depth_compare,
        shader.depth_mask,
        shader.color_mask,
        shader.dest_blend,
        shader.fog_func,
        shader.pri_gradient,
        shader.sec_gradient,
        shader.src_blend,
        shader.texturing,
        shader.detail_color_func,
        shader.detail_alpha_func,
        shader.shader_preset,
        shader.alpha_test,
        shader.post_detail_color_func,
        shader.post_detail_alpha_func,
        0,
    ];
    let bytes: &'static [u8] = Box::leak(Box::new(raw));
    let shaders = mint_sage_w3d::Array::new("SHADERS", bytes).expect("16 bytes is one Shader");

    let mut first_pass = mint_sage_w3d::MaterialPass::default();
    let mut textures = Vec::new();
    if stage1_without_stage0 {
        textures.push(mint_sage_w3d::Texture {
            name: "stage1.tga".to_owned(),
            info: None,
        });
        first_pass.texture_stages = vec![
            mint_sage_w3d::TextureStage::default(),
            mint_sage_w3d::TextureStage {
                texture_ids: Some(mint_sage_w3d::IndexTable::Shared(0)),
                ..mint_sage_w3d::TextureStage::default()
            },
        ];
    }
    let mut passes = Vec::new();
    if pass_count > 0 {
        passes.push(first_pass);
        for _ in 1..pass_count {
            passes.push(mint_sage_w3d::MaterialPass::default());
        }
    }

    Mesh {
        shaders,
        passes,
        vertex_materials: vec![mint_sage_w3d::VertexMaterial::default(); vertex_material_count],
        textures,
        ..Mesh::default()
    }
}

#[test]
fn the_effect_path_wins_over_every_fixed_function_branch_when_it_applies() {
    for (a, d, e) in [
        (ZERO, ZERO, ZERO),
        (ZERO, GREEN, ZERO),
        (RED, GREEN, ZERO),
        (RED, ZERO, ZERO),
        (ZERO, ZERO, BLUE),
    ] {
        let sources = dcg_sources(true, true, a, d, e);
        assert_eq!(sources, DcgSources::EFFECT_DCG, "a={a:?} d={d:?} e={e:?}");
        assert!(!sources.ambient_from_vertex);
        assert!(!sources.diffuse_from_vertex);
        assert!(!sources.lighting_disabled);
        assert_eq!(sources.premultiply, None);
        assert!(sources.dcg_multiplies_result);
    }
}

#[test]
fn effect_path_is_moot_with_no_dcg_array_at_all() {
    let sources = dcg_sources(false, true, RED, GREEN, BLUE);
    assert_eq!(sources, DcgSources::NO_DCG);
}

fn passing_shader() -> mint_sage_w3d::Shader {
    mint_sage_w3d::Shader {
        src_blend: 2,
        dest_blend: 5,
        detail_alpha_func: 0,
        sec_gradient: 0,
        pri_gradient: 1,
        depth_compare: 3,
        ..mint_sage_w3d::Shader::default()
    }
}

#[test]
fn a_single_pass_legacy_mesh_asks_for_house_colour_whatever_its_detail_function() {
    for func in 0..=14u8 {
        let shader = mint_sage_w3d::Shader {
            detail_color_func: func,
            ..passing_shader()
        };
        let one = mesh_with_shader_and_tables(shader, 1, 1, false);
        assert!(wants_house_color(&one), "detail_color_func {func}");
        let two = mesh_with_shader_and_tables(shader, 2, 1, false);
        assert!(!wants_house_color(&two), "two passes, func {func}");
    }
}

#[test]
fn only_the_default_effect_without_a_normal_map_asks_for_house_colour() {
    let material = |effect: &str, with_normal_map: bool| {
        let mut properties = vec![mint_sage_w3d::ShaderMaterialProperty {
            name: "Texture_0".to_owned(),
            value: mint_sage_w3d::ShaderMaterialValue::Texture("a.tga".to_owned()),
        }];
        if with_normal_map {
            properties.push(mint_sage_w3d::ShaderMaterialProperty {
                name: "NormalMap".to_owned(),
                value: mint_sage_w3d::ShaderMaterialValue::Texture("n.tga".to_owned()),
            });
        }
        mint_sage_w3d::ShaderMaterial {
            shader_name: effect.to_owned(),
            properties,
            ..mint_sage_w3d::ShaderMaterial::default()
        }
    };
    for (effect, normal_map, expected) in [
        ("defaultw3d.fx", false, true),
        ("DefaultW3D.fx", true, false),
        ("NormalMapped.fx", false, false),
        ("Simple.fx", false, false),
    ] {
        let mut mesh = mesh_with_shader_and_tables(passing_shader(), 1, 1, false);
        mesh.shader_materials = vec![material(effect, normal_map)];
        assert_eq!(
            wants_house_color(&mesh),
            expected,
            "{effect} normal map {normal_map}"
        );
    }
}

#[test]
fn wants_effect_path_accepts_every_documented_blend_pair() {
    for (src, dest) in [(1u8, 0u8), (2, 5), (1, 1)] {
        let shader = mint_sage_w3d::Shader {
            src_blend: src,
            dest_blend: dest,
            ..passing_shader()
        };
        let mesh = mesh_with_shader_and_tables(shader, 1, 1, false);
        assert!(
            wants_effect_path(&mesh),
            "src={src} dest={dest} should be accepted"
        );
    }
}

#[test]
fn wants_effect_path_refuses_an_unlisted_blend_pair() {
    let shader = mint_sage_w3d::Shader {
        src_blend: 3,
        dest_blend: 3,
        ..passing_shader()
    };
    let mesh = mesh_with_shader_and_tables(shader, 1, 1, false);
    assert!(!wants_effect_path(&mesh));
}

#[test]
fn wants_effect_path_refuses_a_non_disabled_detail_alpha() {
    let shader = mint_sage_w3d::Shader {
        detail_alpha_func: 1,
        ..passing_shader()
    };
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        shader, 1, 1, false
    )));
}

#[test]
fn wants_effect_path_refuses_a_non_disabled_secondary_gradient() {
    let shader = mint_sage_w3d::Shader {
        sec_gradient: 1,
        ..passing_shader()
    };
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        shader, 1, 1, false
    )));
}

#[test]
fn wants_effect_path_accepts_pri_gradient_disable_as_well_as_modulate() {
    let shader = mint_sage_w3d::Shader {
        pri_gradient: 0,
        ..passing_shader()
    };
    assert!(wants_effect_path(&mesh_with_shader_and_tables(
        shader, 1, 1, false
    )));
}

#[test]
fn wants_effect_path_refuses_a_pri_gradient_outside_disable_or_modulate() {
    let shader = mint_sage_w3d::Shader {
        pri_gradient: 2,
        ..passing_shader()
    };
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        shader, 1, 1, false
    )));
}

#[test]
fn wants_effect_path_refuses_a_depth_compare_other_than_less_equal() {
    let shader = mint_sage_w3d::Shader {
        depth_compare: 7,
        ..passing_shader()
    };
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        shader, 1, 1, false
    )));
}

#[test]
fn wants_effect_path_refuses_more_than_one_material_pass() {
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        passing_shader(),
        2,
        1,
        false
    )));
}

#[test]
fn wants_effect_path_refuses_more_than_one_vertex_material() {
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        passing_shader(),
        1,
        2,
        false
    )));
}

#[test]
fn wants_effect_path_refuses_a_texture_stage_1_with_no_stage_0() {
    assert!(!wants_effect_path(&mesh_with_shader_and_tables(
        passing_shader(),
        1,
        1,
        true
    )));
}

#[test]
fn wants_effect_path_refuses_a_mesh_with_no_shaders_array_at_all() {
    let mesh = Mesh {
        passes: vec![mint_sage_w3d::MaterialPass::default()],
        vertex_materials: vec![mint_sage_w3d::VertexMaterial::default()],
        ..Mesh::default()
    };
    assert!(!wants_effect_path(&mesh));
}

fn textured(mut mesh: Mesh<'static>) -> Mesh<'static> {
    mesh.textures = vec![mint_sage_w3d::Texture {
        name: "stage0.tga".to_owned(),
        info: None,
    }];
    mesh.passes[0].texture_stages = vec![mint_sage_w3d::TextureStage {
        texture_ids: Some(mint_sage_w3d::IndexTable::Shared(0)),
        ..mint_sage_w3d::TextureStage::default()
    }];
    mesh
}

#[test]
fn stage0_modulate2x_follows_the_gradient_only_off_the_effect_path() {
    let rows = [
        (5u8, 0u8, true, Modulate2x::On),
        (5, 0, false, Modulate2x::Off),
        (1, 3, true, Modulate2x::WithScene),
        (1, 0, true, Modulate2x::Off),
        (2, 0, true, Modulate2x::Off),
    ];
    for (pri_gradient, detail_color_func, has_texture, expected) in rows {
        let shader = mint_sage_w3d::Shader {
            pri_gradient,
            detail_color_func,
            ..passing_shader()
        };
        let mut mesh = mesh_with_shader_and_tables(shader, 1, 1, false);
        if has_texture {
            mesh = textured(mesh);
        }
        assert_eq!(
            wants_modulate2x(&mesh),
            expected,
            "gradient {pri_gradient} detail {detail_color_func} textured {has_texture}"
        );
    }
}

#[test]
fn wants_effect_path_refuses_a_detail_color_outside_the_supported_set() {
    for func in 0..=14u8 {
        let shader = mint_sage_w3d::Shader {
            detail_color_func: func,
            ..passing_shader()
        };
        let accepted = wants_effect_path(&mesh_with_shader_and_tables(shader, 1, 1, false));
        assert_eq!(accepted, matches!(func, 0 | 2 | 8 | 11 | 12), "func {func}");
    }
}
