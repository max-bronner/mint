use mint_sage_w3d::{ChunkType, Error, Model, MAX_CHUNK_DEPTH};

const HAS_SUBCHUNKS: u32 = 0x8000_0000;

fn chunk(kind: ChunkType, children: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&kind.0.to_le_bytes());
    let mut size = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    if children {
        size |= HAS_SUBCHUNKS;
    }
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.iter().flatten().copied().collect()
}

fn put(buf: &mut [u8], at: usize, bytes: &[u8]) {
    if let Some(slot) = buf.get_mut(at..at.saturating_add(bytes.len())) {
        slot.copy_from_slice(bytes);
    }
}

fn rich_model() -> Vec<u8> {
    let mut header = vec![0u8; 116];
    put(&mut header, 0, &0x0004_0001u32.to_le_bytes());
    put(&mut header, 4, &0x0002_0000u32.to_le_bytes());
    put(&mut header, 8, b"ARM\0");
    put(&mut header, 24, b"HEROES");
    put(&mut header, 40, &1u32.to_le_bytes());
    put(&mut header, 44, &2u32.to_le_bytes());

    let mut influences = Vec::new();
    for _ in 0..2 {
        influences.extend_from_slice(&3u16.to_le_bytes());
        influences.extend_from_slice(&7u16.to_le_bytes());
        influences.extend_from_slice(&60u16.to_le_bytes());
        influences.extend_from_slice(&40u16.to_le_bytes());
    }

    let mut sm_header = vec![1u8];
    sm_header.extend_from_slice(b"NormalMapped.fx");
    sm_header.resize(33, 0);
    sm_header.extend_from_slice(&2u32.to_le_bytes());

    let mut diffuse = Vec::new();
    diffuse.extend_from_slice(&1u32.to_le_bytes());
    diffuse.extend_from_slice(&15u32.to_le_bytes());
    diffuse.extend_from_slice(b"DiffuseTexture\0");
    diffuse.extend_from_slice(&8u32.to_le_bytes());
    diffuse.extend_from_slice(b"arm.tga\0");

    let mesh = chunk(
        ChunkType::MESH,
        true,
        &concat(&[
            chunk(ChunkType::MESH_HEADER3, false, &header),
            chunk(ChunkType::MESH_USER_TEXT, false, b"exported by hand\0"),
            chunk(ChunkType::VERTICES, false, &[0u8; 24]),
            chunk(ChunkType::VERTEX_NORMALS, false, &[0u8; 24]),
            chunk(ChunkType::TRIANGLES, false, &[0u8; 32]),
            chunk(ChunkType::VERTEX_INFLUENCES, false, &influences),
            chunk(ChunkType::VERTEX_SHADE_INDICES, false, &[0u8; 8]),
            chunk(ChunkType::TANGENTS, false, &[0u8; 24]),
            chunk(ChunkType::BITANGENTS, false, &[0u8; 24]),
            chunk(ChunkType::VERTICES_2, false, &[0u8; 24]),
            chunk(ChunkType::VERTEX_NORMALS_2, false, &[0u8; 24]),
            chunk(ChunkType::MATERIAL_INFO, false, &[0u8; 16]),
            chunk(ChunkType::SHADERS, false, &[0u8; 16]),
            chunk(
                ChunkType::SHADER_MATERIALS,
                true,
                &chunk(
                    ChunkType::SHADER_MATERIAL,
                    true,
                    &concat(&[
                        chunk(ChunkType::SHADER_MATERIAL_HEADER, false, &sm_header),
                        chunk(ChunkType::SHADER_MATERIAL_PROPERTY, false, &diffuse),
                    ]),
                ),
            ),
            chunk(
                ChunkType::MATERIAL_PASS,
                true,
                &concat(&[
                    chunk(ChunkType::SHADER_MATERIAL_ID, false, &0u32.to_le_bytes()),
                    chunk(
                        ChunkType::TEXTURE_STAGE,
                        true,
                        &chunk(ChunkType::STAGE_TEXCOORDS, false, &[0u8; 16]),
                    ),
                ]),
            ),
        ]),
    );

    let mut hier_header = vec![0u8; 36];
    put(&mut hier_header, 4, b"HEROSKL");
    put(&mut hier_header, 20, &2u32.to_le_bytes());
    let mut pivots = vec![0u8; 120];
    put(&mut pivots, 0, b"ROOT");
    put(&mut pivots, 16, &u32::MAX.to_le_bytes());
    put(&mut pivots, 60, b"TORSO");
    put(&mut pivots, 76, &0u32.to_le_bytes());

    let hierarchy = chunk(
        ChunkType::HIERARCHY,
        true,
        &concat(&[
            chunk(ChunkType::HIERARCHY_HEADER, false, &hier_header),
            chunk(ChunkType::PIVOTS, false, &pivots),
        ]),
    );

    let mut hlod_header = vec![0u8; 40];
    put(&mut hlod_header, 4, &1u32.to_le_bytes());
    put(&mut hlod_header, 8, b"ARM");
    put(&mut hlod_header, 24, b"HEROSKL");
    let mut sub_object = vec![0u8; 36];
    put(&mut sub_object, 4, b"ARM.ARM");
    let mut array_header = vec![0u8; 8];
    put(&mut array_header, 0, &1u32.to_le_bytes());
    put(&mut array_header, 4, &f32::MAX.to_le_bytes());

    let hlod = chunk(
        ChunkType::HLOD,
        true,
        &concat(&[
            chunk(ChunkType::HLOD_HEADER, false, &hlod_header),
            chunk(
                ChunkType::HLOD_LOD_ARRAY,
                true,
                &concat(&[
                    chunk(
                        ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER,
                        false,
                        &array_header,
                    ),
                    chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub_object),
                ]),
            ),
        ]),
    );

    concat(&[mesh, hierarchy, hlod])
}

#[test]
fn the_reference_model_parses_and_every_part_arrives() {
    let bytes = rich_model();
    let model = Model::parse(&bytes).expect("the reference model is well formed");

    let mesh = model.mesh("ARM").expect("one mesh named ARM");
    assert!(mesh.header.is_skin());
    assert!(mesh.counts_agree());
    assert_eq!(mesh.vertex_count(), 2);
    assert_eq!(mesh.triangle_count(), 1);
    assert_eq!(mesh.influences.len(), 2);
    assert_eq!(mesh.tangents.len(), 2);
    assert_eq!(mesh.bitangents.len(), 2);
    assert_eq!(mesh.positions2.len(), 2);
    assert_eq!(mesh.normals2.len(), 2);
    assert_eq!(mesh.user_text.as_deref(), Some("exported by hand"));
    assert_eq!(mesh.texture_name(), Some("arm.tga"));

    let influence = mesh
        .influences
        .get(0)
        .and_then(Result::ok)
        .expect("first influence");
    assert_eq!(influence.bones, [3, 7]);
    assert_eq!(influence.weights_percent(), [60, 40]);

    let hierarchy = model.hierarchy.as_ref().expect("a hierarchy");
    assert_eq!(hierarchy.len(), 2);
    assert!(hierarchy.counts_agree());
    assert!(hierarchy.unusable_parents().is_empty());
    assert!(hierarchy.parents_precede_children());

    assert_eq!(model.hlods.len(), 1);
    assert_eq!(model.external_hierarchy(), None, "the skeleton is in-file");
}

#[test]
fn truncating_at_every_length_returns_rather_than_panicking() {
    let bytes = rich_model();
    for cut in 0..bytes.len() {
        let prefix = bytes.get(..cut).unwrap_or_default();
        let _ = Model::parse(prefix);
    }
}

#[test]
fn flipping_every_bit_of_every_byte_returns_rather_than_panicking() {
    let bytes = rich_model();
    for i in 0..bytes.len() {
        for bit in 0..8 {
            let mut corrupt = bytes.clone();
            if let Some(b) = corrupt.get_mut(i) {
                *b ^= 1u8 << bit;
            }
            let _ = Model::parse(&corrupt);
        }
    }
}

#[test]
fn a_chunk_claiming_a_size_past_eof_is_a_typed_error() {
    let mut file = Vec::new();
    file.extend_from_slice(&ChunkType::MESH.0.to_le_bytes());
    file.extend_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
    file.extend_from_slice(&[0u8; 32]);
    assert!(matches!(
        Model::parse(&file),
        Err(Error::ChunkOverrun {
            declared: 0x7FFF_FFFF,
            available: 32,
            ..
        })
    ));
}

#[test]
fn a_truncated_chunk_header_is_a_typed_error() {
    for len in 1..8usize {
        assert!(
            matches!(
                Model::parse(&vec![0u8; len]),
                Err(Error::UnexpectedEof { .. })
            ),
            "{len} trailing bytes should not be read as a chunk"
        );
    }
}

#[test]
fn a_subchunk_that_outsizes_its_parent_is_a_typed_error() {
    let mut inner = Vec::new();
    inner.extend_from_slice(&ChunkType::VERTICES.0.to_le_bytes());
    inner.extend_from_slice(&1024u32.to_le_bytes());
    inner.extend_from_slice(&[0u8; 12]);
    let file = chunk(ChunkType::MESH, true, &inner);
    assert!(matches!(
        Model::parse(&file),
        Err(Error::ChunkOverrun { declared: 1024, .. })
    ));
}

#[test]
fn bit_31_set_on_a_leaf_payload_is_a_typed_error() {
    let file = chunk(ChunkType::MESH, true, &{
        let mut v = chunk(ChunkType::VERTICES, true, &[0xAB; 30]);
        v.extend_from_slice(&[]);
        v
    });
    assert!(Model::parse(&file).is_err());
}

#[test]
fn a_depth_bomb_hits_the_cap_instead_of_the_stack() {
    let mut payload = Vec::new();
    for _ in 0..MAX_CHUNK_DEPTH.saturating_mul(64) {
        payload = chunk(ChunkType::MESH, true, &payload);
    }
    assert!(matches!(
        Model::parse(&payload),
        Err(Error::TooDeep {
            max: MAX_CHUNK_DEPTH,
            ..
        })
    ));
}

#[test]
fn a_header_count_of_four_billion_allocates_nothing() {
    let mut header = vec![0u8; 116];
    put(&mut header, 40, &u32::MAX.to_le_bytes());
    put(&mut header, 44, &u32::MAX.to_le_bytes());

    let mut hier_header = vec![0u8; 36];
    put(&mut hier_header, 20, &u32::MAX.to_le_bytes());

    let file = concat(&[
        chunk(
            ChunkType::MESH,
            true,
            &concat(&[
                chunk(ChunkType::MESH_HEADER3, false, &header),
                chunk(ChunkType::VERTICES, false, &[0u8; 12]),
                chunk(ChunkType::TRIANGLES, false, &[0u8; 32]),
            ]),
        ),
        chunk(
            ChunkType::HIERARCHY,
            true,
            &concat(&[
                chunk(ChunkType::HIERARCHY_HEADER, false, &hier_header),
                chunk(ChunkType::PIVOTS, false, &[0u8; 60]),
            ]),
        ),
    ]);

    let model = Model::parse(&file).expect("structurally valid, semantically a lie");
    assert_eq!(model.vertex_count(), 1);
    assert_eq!(model.triangle_count(), 1);
    assert_eq!(model.hierarchy.map(|h| h.len()), Some(1));
    assert!(!model.meshes.first().is_some_and(|m| m.counts_agree()));
}

#[test]
fn zero_length_payloads_are_accepted_wherever_they_are_legal() {
    let file = concat(&[
        chunk(
            ChunkType::MESH,
            true,
            &concat(&[
                chunk(ChunkType::MESH_HEADER3, false, &[0u8; 116]),
                chunk(ChunkType::VERTICES, false, &[]),
                chunk(ChunkType::TRIANGLES, false, &[]),
                chunk(ChunkType::MESH_USER_TEXT, false, &[]),
                chunk(ChunkType::TEXTURES, true, &[]),
                chunk(ChunkType::MATERIAL_PASS, true, &[]),
            ]),
        ),
        chunk(ChunkType::HLOD, true, &[]),
        chunk(ChunkType::HIERARCHY, true, &[]),
    ]);
    let model = Model::parse(&file).expect("empty is not malformed");
    assert_eq!(model.vertex_count(), 0);
    assert_eq!(model.meshes.len(), 1);
    assert_eq!(model.hlods.len(), 1);
    assert_eq!(model.hierarchy.map(|h| h.len()), Some(0));
}

#[test]
fn a_file_of_random_bytes_returns_rather_than_panicking() {
    let mut state: u32 = 0x1234_5678;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        u8::try_from(state >> 24).unwrap_or(0)
    };
    for _ in 0..256 {
        let bytes: Vec<u8> = (0..512).map(|_| next()).collect();
        let _ = Model::parse(&bytes);
    }
}
