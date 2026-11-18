use super::*;
use crate::testutil::{chunk, concat, mesh_header};
use crate::{Chunks, UvMapperType, VertexMaterialInfo};

#[test]
fn the_header_needs_all_116_bytes() {
    assert!(MeshHeader3::parse(&[0u8; 115]).is_err());
    let h = MeshHeader3::parse(&[0u8; 116]).expect("exact size parses");
    assert_eq!(h.num_vertices, 0);
}

#[test]
fn header_field_offsets_match_the_c_struct() {
    let mut buf = [0u8; MeshHeader3::SIZE];
    let put = |buf: &mut [u8; MeshHeader3::SIZE], off: usize, v: u32| {
        if let Some(slot) = buf.get_mut(off..off + 4) {
            slot.copy_from_slice(&v.to_le_bytes());
        }
    };
    put(&mut buf, 0, 0x0004_0001);
    put(&mut buf, 4, MeshHeader3::GEOMETRY_TYPE_SKIN);
    buf[8..13].copy_from_slice(b"TORSO");
    buf[24..30].copy_from_slice(b"ARAGRN");
    put(&mut buf, 40, 708);
    put(&mut buf, 44, 937);
    put(&mut buf, 68, 0x0000_0003);
    put(&mut buf, 72, 0x0000_0001);
    put(&mut buf, 112, 0x4080_0000);

    let h = MeshHeader3::parse(&buf).expect("valid");
    assert_eq!(h.version, 0x0004_0001);
    assert_eq!(h.mesh_name, "TORSO");
    assert_eq!(h.container_name, "ARAGRN");
    assert_eq!(h.num_tris, 708);
    assert_eq!(h.num_vertices, 937);
    assert_eq!(h.vertex_channels, 3);
    assert_eq!(h.face_channels, 1);
    assert_eq!(h.bound_sphere_radius, 4.0);
    assert!(h.is_skin());
}

#[test]
fn influences_read_as_two_bones_and_two_percentage_weights() {
    let raw = [0x01, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00];
    let inf = VertexInfluence::read(&mut Reader::new(&raw)).expect("8 bytes");
    assert_eq!(inf.bones, [1, 0]);
    assert_eq!(inf.weights, [100, 0]);
    assert_eq!(inf.weights_fraction(), [1.0, 0.0]);
}

#[test]
fn a_genuine_two_bone_split_keeps_both_weights() {
    let raw = [0x03, 0x00, 0x07, 0x00, 0x3C, 0x00, 0x28, 0x00];
    let inf = VertexInfluence::read(&mut Reader::new(&raw)).expect("8 bytes");
    assert_eq!(inf.bones, [3, 7]);
    assert_eq!(inf.weights_percent(), [60, 40]);
    assert_eq!(inf.weights_fraction(), [0.6, 0.4]);
}

#[test]
fn both_weights_zero_means_all_of_bone_zero() {
    let inf = VertexInfluence {
        bones: [5, 0],
        weights: [0, 0],
    };
    assert_eq!(inf.weights, [0, 0], "the stored bytes are left alone");
    assert_eq!(inf.weights_percent(), [100, 0]);
    assert_eq!(inf.weights_fraction(), [1.0, 0.0]);
}

#[test]
fn counts_that_disagree_are_reported_not_rejected() {
    let body = concat(&[
        chunk(
            ChunkType::MESH_HEADER3,
            false,
            &mesh_header("BOX", 1, 99, 0),
        ),
        chunk(ChunkType::VERTICES, false, &[0u8; 24]),
        chunk(ChunkType::TRIANGLES, false, &[0u8; 32]),
    ]);
    let file = chunk(ChunkType::MESH, true, &body);
    let top = Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    let mesh = Mesh::parse(&top).expect("a stale count is still parseable");
    assert_eq!(mesh.vertex_count(), 2, "sized from bytes, not the header");
    assert_eq!(mesh.header.num_vertices, 99);
    assert!(!mesh.counts_agree());
}

#[test]
fn a_vertex_count_of_four_billion_allocates_nothing() {
    let body = concat(&[
        chunk(
            ChunkType::MESH_HEADER3,
            false,
            &mesh_header("BOX", u32::MAX, u32::MAX, 0),
        ),
        chunk(ChunkType::VERTICES, false, &[0u8; 24]),
    ]);
    let file = chunk(ChunkType::MESH, true, &body);
    let top = Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    let mesh = Mesh::parse(&top).expect("valid chunk structure");
    assert_eq!(mesh.vertex_count(), 2);
    assert_eq!(mesh.positions.to_vec().map(|v| v.len()), Ok(2));
    assert!(!mesh.counts_agree());
}

#[test]
fn a_ragged_vertex_array_is_an_error() {
    let body = concat(&[
        chunk(ChunkType::MESH_HEADER3, false, &mesh_header("BOX", 0, 0, 0)),
        chunk(ChunkType::VERTICES, false, &[0u8; 25]),
    ]);
    let file = chunk(ChunkType::MESH, true, &body);
    let top = Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    assert!(matches!(
        Mesh::parse(&top),
        Err(crate::Error::RaggedArray {
            what: "VERTICES",
            ..
        })
    ));
}

#[test]
fn a_mesh_with_no_children_at_all_parses_to_defaults() {
    let file = chunk(ChunkType::MESH, true, &[]);
    let top = Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    let mesh = Mesh::parse(&top).expect("an empty mesh is structurally valid");
    assert_eq!(mesh.vertex_count(), 0);
    assert_eq!(mesh.texture_name(), None);
    assert!(mesh.texcoords().is_empty());
    assert!(mesh.counts_agree(), "0 == 0");
}

fn texcoord_payload() -> Vec<u8> {
    let mut buf = Vec::new();
    for (u, v) in [(0.25f32, 0.5f32), (0.75, 1.0)] {
        buf.extend_from_slice(&u.to_le_bytes());
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

fn parse_mesh(payload: &[u8]) -> Mesh<'_> {
    let top = Chunks::new(payload)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    Mesh::parse(&top).expect("structurally valid")
}

#[test]
fn texcoords_are_found_inside_a_texture_stage_the_generals_era_way() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &chunk(
                ChunkType::TEXTURE_STAGE,
                true,
                &chunk(ChunkType::STAGE_TEXCOORDS, false, &texcoord_payload()),
            ),
        ),
    );
    let mesh = parse_mesh(&file);
    assert_eq!(mesh.texcoords().len(), 2);
    assert_eq!(
        mesh.texcoords().get(0).and_then(Result::ok),
        Some(Vec2 { u: 0.25, v: 0.5 })
    );
}

#[test]
fn texcoords_are_also_found_directly_under_a_material_pass() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &concat(&[
                chunk(ChunkType::SHADER_MATERIAL_ID, false, &0u32.to_le_bytes()),
                chunk(ChunkType::STAGE_TEXCOORDS, false, &texcoord_payload()),
            ]),
        ),
    );
    let mesh = parse_mesh(&file);
    assert!(
        mesh.passes
            .first()
            .is_some_and(|pass| pass.texture_stages.is_empty()),
        "this shape has no TEXTURE_STAGE at all"
    );
    assert_eq!(mesh.texcoords().len(), 2);
    assert_eq!(
        mesh.texcoords().get(1).and_then(Result::ok),
        Some(Vec2 { u: 0.75, v: 1.0 })
    );
}

#[test]
fn a_texture_stage_wins_over_a_pass_level_array_if_a_file_carries_both() {
    let mut nested = Vec::new();
    nested.extend_from_slice(&1.0f32.to_le_bytes());
    nested.extend_from_slice(&1.0f32.to_le_bytes());

    let file = chunk(
        ChunkType::MESH,
        true,
        &chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &concat(&[
                chunk(
                    ChunkType::TEXTURE_STAGE,
                    true,
                    &chunk(ChunkType::STAGE_TEXCOORDS, false, &nested),
                ),
                chunk(ChunkType::STAGE_TEXCOORDS, false, &texcoord_payload()),
            ]),
        ),
    );
    let mesh = parse_mesh(&file);
    assert_eq!(mesh.texcoords().len(), 1);
    assert_eq!(
        mesh.texcoords().get(0).and_then(Result::ok),
        Some(Vec2 { u: 1.0, v: 1.0 })
    );
}

#[test]
fn an_empty_texture_stage_falls_through_to_the_pass_level_array() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &concat(&[
                chunk(ChunkType::TEXTURE_STAGE, true, &[]),
                chunk(ChunkType::STAGE_TEXCOORDS, false, &texcoord_payload()),
            ]),
        ),
    );
    assert_eq!(parse_mesh(&file).texcoords().len(), 2);
}

fn texture_property(name: &str, value: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_le_bytes());
    let name_len = u32::try_from(name.len().saturating_add(1)).expect("test names are short");
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    let value_len = u32::try_from(value.len().saturating_add(1)).expect("test names are short");
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(value.as_bytes());
    out.push(0);
    out
}

fn mesh_with_textures(name: &str, properties: &[(&str, &str)]) -> Vec<u8> {
    let mut header = vec![1u8];
    header.resize(33, 0);
    header.extend_from_slice(&2u32.to_le_bytes());

    let mut body = chunk(ChunkType::SHADER_MATERIAL_HEADER, false, &header);
    for (property, value) in properties {
        body.extend_from_slice(&chunk(
            ChunkType::SHADER_MATERIAL_PROPERTY,
            false,
            &texture_property(property, value),
        ));
    }
    concat(&[
        chunk(ChunkType::MESH_HEADER3, false, &mesh_header(name, 0, 0, 0)),
        chunk(
            ChunkType::SHADER_MATERIALS,
            true,
            &chunk(ChunkType::SHADER_MATERIAL, true, &body),
        ),
    ])
}

#[test]
fn a_mesh_named_pick_is_a_picking_proxy_and_not_geometry() {
    for name in ["PICK", "PICK_BOX05", "PICK_BOX06", "pick", "Pick_Box05"] {
        let file = chunk(
            ChunkType::MESH,
            true,
            &chunk(
                ChunkType::MESH_HEADER3,
                false,
                &mesh_header(name, 2, 4, 0x0000_1000),
            ),
        );
        assert!(
            parse_mesh(&file).is_pick_proxy(),
            "{name} should be recognised as a picking proxy"
        );
    }
}

#[test]
fn ordinary_geometry_is_not_mistaken_for_a_picking_proxy() {
    for name in ["GBFARM_04", "PIECES_00", "PIC", "SPICK", "P", ""] {
        let file = chunk(
            ChunkType::MESH,
            true,
            &chunk(ChunkType::MESH_HEADER3, false, &mesh_header(name, 2, 4, 0)),
        );
        assert!(
            !parse_mesh(&file).is_pick_proxy(),
            "{name} is renderable geometry"
        );
    }
}

#[test]
fn textures_are_selected_by_property_name_and_not_by_position() {
    for order in [
        [
            ("DiffuseTexture", "gbfarm.tga"),
            ("NormalMap", "gbfarm_nrm.tga"),
        ],
        [
            ("NormalMap", "gbfarm_nrm.tga"),
            ("DiffuseTexture", "gbfarm.tga"),
        ],
    ] {
        let file = chunk(
            ChunkType::MESH,
            true,
            &mesh_with_textures("GBFARM_08", &order),
        );
        let mesh = parse_mesh(&file);
        assert_eq!(mesh.texture_name(), Some("gbfarm.tga"), "{order:?}");
        assert_eq!(mesh.normal_map_name(), Some("gbfarm_nrm.tga"), "{order:?}");
    }
}

#[test]
fn a_mesh_with_no_normal_map_says_so_rather_than_borrowing_the_diffuse() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &mesh_with_textures("GBFARM_08", &[("DiffuseTexture", "gbfarm.tga")]),
    );
    let mesh = parse_mesh(&file);
    assert_eq!(mesh.texture_name(), Some("gbfarm.tga"));
    assert_eq!(mesh.normal_map_name(), None);
}

#[test]
fn the_tangent_accessors_cross_the_two_chunks_over() {
    let vector = |x: f32, y: f32, z: f32| {
        let mut out = Vec::new();
        for value in [x, y, z] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    };
    let body = concat(&[
        chunk(ChunkType::MESH_HEADER3, false, &mesh_header("M", 0, 1, 0)),
        chunk(ChunkType::TANGENTS, false, &vector(0.0, 1.0, 0.0)),
        chunk(ChunkType::BITANGENTS, false, &vector(1.0, 0.0, 0.0)),
    ]);
    let file = chunk(ChunkType::MESH, true, &body);
    let mesh = parse_mesh(&file);

    let tangents = mesh.uv_tangents().to_vec().expect("one Vec3");
    let bitangents = mesh.uv_bitangents().to_vec().expect("one Vec3");
    assert_eq!(tangents.first().map(|v| v.x), Some(1.0), "u runs along +X");
    assert_eq!(
        bitangents.first().map(|v| v.y),
        Some(1.0),
        "v runs along +Y"
    );
}

fn vertex_materials_chunk(attributes: u32) -> Vec<u8> {
    let mut info = vec![0u8; VertexMaterialInfo::SIZE];
    info[0..4].copy_from_slice(&attributes.to_le_bytes());
    let entry = concat(&[
        chunk(ChunkType::VERTEX_MATERIAL_NAME, false, b"Mat0\0"),
        chunk(ChunkType::VERTEX_MATERIAL_INFO, false, &info),
    ]);
    chunk(
        ChunkType::VERTEX_MATERIALS,
        true,
        &chunk(ChunkType::VERTEX_MATERIAL, true, &entry),
    )
}

#[test]
fn vertex_material_resolves_through_pass_zeros_shared_index() {
    let body = concat(&[
        vertex_materials_chunk(0x0004_0800),
        chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &chunk(ChunkType::VERTEX_MATERIAL_IDS, false, &0u32.to_le_bytes()),
        ),
    ]);
    let file = chunk(ChunkType::MESH, true, &body);
    let mesh = parse_mesh(&file);

    let vm = mesh.vertex_material().expect("index 0 resolves");
    assert_eq!(vm.name, "Mat0");
    let info = vm.info.expect("VERTEX_MATERIAL_INFO was written");
    assert_eq!(info.stage0_mapper(), UvMapperType::LinearOffset);
    assert_eq!(info.stage1_mapper(), UvMapperType::Rotate);
}

#[test]
fn vertex_material_is_none_with_no_material_pass_at_all() {
    let file = chunk(ChunkType::MESH, true, &vertex_materials_chunk(0));
    let mesh = parse_mesh(&file);
    assert!(mesh.vertex_material().is_none());
}

#[test]
fn a_ragged_pass_level_texcoord_array_is_refused_rather_than_truncated() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &chunk(ChunkType::STAGE_TEXCOORDS, false, &[0u8; 12]),
        ),
    );
    let top = Chunks::new(&file)
        .next()
        .and_then(Result::ok)
        .expect("one chunk");
    assert!(matches!(
        Mesh::parse(&top),
        Err(crate::Error::RaggedArray { stride: 8, .. })
    ));
}

fn shaders_chunk(dest_blends: &[u8]) -> Vec<u8> {
    let mut table = Vec::new();
    for dest_blend in dest_blends {
        let mut raw = [0u8; Shader::SIZE];
        raw[3] = *dest_blend;
        table.extend_from_slice(&raw);
    }
    chunk(ChunkType::SHADERS, false, &table)
}

fn pass_naming_shader(id: u32) -> Vec<u8> {
    chunk(
        ChunkType::MATERIAL_PASS,
        true,
        &chunk(ChunkType::SHADER_IDS, false, &id.to_le_bytes()),
    )
}

#[test]
fn drawn_shaders_reads_only_the_first_passs_entry() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &concat(&[
            shaders_chunk(&[0, 1]),
            pass_naming_shader(0),
            pass_naming_shader(1),
        ]),
    );
    let mesh = parse_mesh(&file);
    assert_eq!(mesh.shaders.len(), 2, "both entries are in the table");
    let drawn = mesh.drawn_shaders();
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn.first().map(|s| s.dest_blend), Some(0));

    let file = chunk(
        ChunkType::MESH,
        true,
        &concat(&[
            shaders_chunk(&[0, 1]),
            pass_naming_shader(1),
            pass_naming_shader(0),
        ]),
    );
    assert_eq!(
        parse_mesh(&file)
            .drawn_shaders()
            .first()
            .map(|s| s.dest_blend),
        Some(1)
    );
}

#[test]
fn drawn_shaders_falls_back_to_the_whole_table_with_no_shader_ids() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &concat(&[
            shaders_chunk(&[0, 1, 5]),
            chunk(ChunkType::MATERIAL_PASS, true, &[]),
        ]),
    );
    assert_eq!(parse_mesh(&file).drawn_shaders().len(), 3);

    let file = chunk(ChunkType::MESH, true, &shaders_chunk(&[0, 1, 5]));
    assert_eq!(parse_mesh(&file).drawn_shaders().len(), 3);
}

#[test]
fn drawn_shaders_falls_back_when_the_index_is_out_of_range() {
    let file = chunk(
        ChunkType::MESH,
        true,
        &concat(&[shaders_chunk(&[5]), pass_naming_shader(7)]),
    );
    let drawn = parse_mesh(&file).drawn_shaders();
    assert_eq!(drawn.len(), 1);
    assert_eq!(drawn.first().map(|s| s.dest_blend), Some(5));
}

#[test]
fn drawn_shaders_deduplicates_a_per_triangle_table() {
    let ids = concat(&[
        1u32.to_le_bytes().to_vec(),
        1u32.to_le_bytes().to_vec(),
        u32::MAX.to_le_bytes().to_vec(),
        0u32.to_le_bytes().to_vec(),
    ]);
    let file = chunk(
        ChunkType::MESH,
        true,
        &concat(&[
            shaders_chunk(&[0, 5]),
            chunk(
                ChunkType::MATERIAL_PASS,
                true,
                &chunk(ChunkType::SHADER_IDS, false, &ids),
            ),
        ]),
    );
    let drawn = parse_mesh(&file).drawn_shaders();
    assert_eq!(
        drawn.iter().map(|s| s.dest_blend).collect::<Vec<_>>(),
        vec![5, 0]
    );
}
