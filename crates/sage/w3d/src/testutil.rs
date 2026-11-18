use crate::ChunkType;

const HAS_SUBCHUNKS: u32 = 0x8000_0000;

pub(crate) fn chunk(kind: ChunkType, children: bool, payload: &[u8]) -> Vec<u8> {
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

pub(crate) fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

pub(crate) fn mesh_header(name: &str, tris: u32, vertices: u32, attributes: u32) -> Vec<u8> {
    let mut buf = vec![0u8; 116];
    buf[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    buf[4..8].copy_from_slice(&attributes.to_le_bytes());
    let name = name.as_bytes();
    let n = name.len().min(15);
    buf[8..8usize.saturating_add(n)].copy_from_slice(&name[..n]);
    buf[40..44].copy_from_slice(&tris.to_le_bytes());
    buf[44..48].copy_from_slice(&vertices.to_le_bytes());
    buf
}

pub(crate) fn pivot(name: &str, parent: u32) -> Vec<u8> {
    let mut buf = vec![0u8; 60];
    let name = name.as_bytes();
    let n = name.len().min(15);
    buf[0..n].copy_from_slice(&name[..n]);
    buf[16..20].copy_from_slice(&parent.to_le_bytes());
    buf
}

pub(crate) fn hierarchy(name: &str, pivots: &[(&str, u32)]) -> Vec<u8> {
    let mut header = vec![0u8; 36];
    header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    let bytes = name.as_bytes();
    let n = bytes.len().min(15);
    header[4..4usize.saturating_add(n)].copy_from_slice(&bytes[..n]);
    header[20..24].copy_from_slice(
        &u32::try_from(pivots.len())
            .expect("test hierarchies are small")
            .to_le_bytes(),
    );

    let mut table = Vec::new();
    for (pivot_name, parent) in pivots {
        table.extend_from_slice(&pivot(pivot_name, *parent));
    }

    let body = concat(&[
        chunk(ChunkType::HIERARCHY_HEADER, false, &header),
        chunk(ChunkType::PIVOTS, false, &table),
    ]);
    chunk(ChunkType::HIERARCHY, true, &body)
}

pub(crate) fn tiny_model() -> Vec<u8> {
    let mesh_body = concat(&[
        chunk(ChunkType::MESH_HEADER3, false, &mesh_header("BOX", 1, 2, 0)),
        chunk(ChunkType::VERTICES, false, &[0u8; 24]),
        chunk(ChunkType::VERTEX_NORMALS, false, &[0u8; 24]),
        chunk(ChunkType::TRIANGLES, false, &[0u8; 32]),
        chunk(
            ChunkType::TEXTURES,
            true,
            &chunk(
                ChunkType::TEXTURE,
                true,
                &chunk(ChunkType::TEXTURE_NAME, false, b"box.tga\0"),
            ),
        ),
        chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &chunk(
                ChunkType::TEXTURE_STAGE,
                true,
                &concat(&[
                    chunk(ChunkType::TEXTURE_IDS, false, &0u32.to_le_bytes()),
                    chunk(ChunkType::STAGE_TEXCOORDS, false, &[0u8; 16]),
                ]),
            ),
        ),
    ]);

    let mut hlod_header = vec![0u8; 40];
    hlod_header[4..8].copy_from_slice(&1u32.to_le_bytes());
    hlod_header[8..11].copy_from_slice(b"BOX");
    hlod_header[24..32].copy_from_slice(b"BOXSKEL\0");

    let mut sub_object = vec![0u8; 36];
    sub_object[0..4].copy_from_slice(&0u32.to_le_bytes());
    sub_object[4..11].copy_from_slice(b"BOX.BOX");

    let mut lod_array_header = vec![0u8; 8];
    lod_array_header[0..4].copy_from_slice(&1u32.to_le_bytes());
    lod_array_header[4..8].copy_from_slice(&f32::MAX.to_le_bytes());

    let hlod_body = concat(&[
        chunk(ChunkType::HLOD_HEADER, false, &hlod_header),
        chunk(
            ChunkType::HLOD_LOD_ARRAY,
            true,
            &concat(&[
                chunk(
                    ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER,
                    false,
                    &lod_array_header,
                ),
                chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub_object),
            ]),
        ),
    ]);

    concat(&[
        chunk(ChunkType::MESH, true, &mesh_body),
        chunk(ChunkType::HLOD, true, &hlod_body),
    ])
}

pub(crate) fn animated_model() -> Vec<u8> {
    let mut plain_header = vec![0u8; 44];
    plain_header[4..8].copy_from_slice(b"RUN\0");
    plain_header[36..40].copy_from_slice(&2u32.to_le_bytes());
    plain_header[40..44].copy_from_slice(&30u32.to_le_bytes());

    let mut dense = vec![0u8; 12];
    dense.extend_from_slice(&1.5f32.to_le_bytes());
    let mut bits = vec![0u8; 10];
    bits[8] = 1;

    let plain = chunk(
        ChunkType::ANIMATION,
        true,
        &concat(&[
            chunk(ChunkType::ANIMATION_HEADER, false, &plain_header),
            chunk(ChunkType::ANIMATION_CHANNEL, false, &dense),
            chunk(ChunkType::BIT_CHANNEL, false, &bits),
        ]),
    );

    let mut compressed_header = vec![0u8; 44];
    compressed_header[4..8].copy_from_slice(b"WALK");
    compressed_header[36..40].copy_from_slice(&40u32.to_le_bytes());
    compressed_header[40..42].copy_from_slice(&30u16.to_le_bytes());
    compressed_header[42..44].copy_from_slice(&0u16.to_le_bytes());

    let mut time_coded = Vec::new();
    time_coded.extend_from_slice(&1u32.to_le_bytes());
    time_coded.extend_from_slice(&2u16.to_le_bytes());
    time_coded.push(1);
    time_coded.push(0);
    time_coded.extend_from_slice(&5u32.to_le_bytes());
    time_coded.extend_from_slice(&3.0f32.to_le_bytes());

    let mut motion = vec![0u8, 1, 1, 0];
    motion.extend_from_slice(&17u16.to_le_bytes());
    motion.extend_from_slice(&1u16.to_le_bytes());
    motion.extend_from_slice(&1.0f32.to_le_bytes());
    motion.extend_from_slice(&0.0f32.to_le_bytes());
    motion.push(8);
    motion.extend_from_slice(&[0u8; 8]);

    let compressed = chunk(
        ChunkType::COMPRESSED_ANIMATION,
        true,
        &concat(&[
            chunk(
                ChunkType::COMPRESSED_ANIMATION_HEADER,
                false,
                &compressed_header,
            ),
            chunk(ChunkType::COMPRESSED_ANIMATION_CHANNEL, false, &time_coded),
            chunk(
                ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
                false,
                &motion,
            ),
        ]),
    );

    concat(&[plain, compressed])
}
