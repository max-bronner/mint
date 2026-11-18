#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use std::sync::Arc;

use mint_render::{Error as RenderError, Renderer};
use mint_sage_model::{ModelCache, Resolver};
use mint_vfs::{Vfs, VfsPath};

fn renderer() -> Option<Renderer> {
    match Renderer::offscreen(64, 64) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::NoAdapter) => {
            eprintln!(
                "SKIPPED: no graphics adapter on this machine, so nothing in \
                 tests/cache.rs was actually verified"
            );
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

#[test]
fn one_model_reached_through_two_spellings_is_uploaded_once() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[("art/w3d/bo/boxmodel.w3d", tiny_model_bytes())]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();

    let first = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "BoxModel")
        .expect("the synthetic model loads");
    let after_first = cache.stats();
    let second = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "bOXmODEL")
        .expect("the same model, spelled differently");

    assert!(
        Arc::ptr_eq(&first, &second),
        "the second spelling must return the same uploaded model, not a copy"
    );
    assert_eq!(cache.stats().loaded, 1, "loaded once");
    assert_eq!(
        cache.stats().meshes,
        after_first.meshes,
        "the second lookup must upload no mesh"
    );
    assert_eq!(cache.stats().requests, 2, "both lookups are counted");
    assert_eq!(cache.len(), 1, "one entry, not two");
}

#[test]
fn a_name_that_does_not_resolve_fails_once_and_is_remembered() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[("art/w3d/bo/boxmodel.w3d", tiny_model_bytes())]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();

    for _ in 0..5 {
        assert!(
            cache
                .get_or_load(&mut renderer, &vfs, &resolver, "NotHere")
                .is_none(),
            "a name nothing supplies must draw nothing"
        );
    }
    assert_eq!(cache.stats().failed, 1, "one failure, not five");
    assert_eq!(cache.failures().len(), 1, "one report line, not five");
    assert_eq!(cache.failures()[0].0, "NotHere");
    assert!(
        cache.failures()[0].1.contains("NotHere"),
        "the reason must name the file: {:?}",
        cache.failures()[0].1
    );
    assert_eq!(cache.stats().requests, 5);
    assert_eq!(cache.stats().meshes, 0);
}

#[test]
fn a_file_with_no_mesh_fails_with_a_reason_that_says_so() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[("art/w3d/em/emitteronly.w3d", hierarchy_only_bytes())]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();

    assert!(cache
        .get_or_load(&mut renderer, &vfs, &resolver, "EmitterOnly")
        .is_none());
    let reason = &cache.failures()[0].1;
    assert!(
        reason.contains("no meshes"),
        "the reason must distinguish an empty file from a missing one: {reason:?}"
    );
}

struct BytesMount(Vec<(VfsPath, Vec<u8>)>);

impl mint_vfs::Mount for BytesMount {
    fn label(&self) -> &str {
        "bytes"
    }
    fn entries(&self) -> Vec<VfsPath> {
        self.0.iter().map(|(path, _)| path.clone()).collect()
    }
    fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
        self.0
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })
    }
}

fn vfs_with(entries: &[(&str, Vec<u8>)]) -> Vfs {
    let mut vfs = Vfs::new();
    vfs.mount(Arc::new(BytesMount(
        entries
            .iter()
            .map(|(path, bytes)| (VfsPath::new(path), bytes.clone()))
            .collect(),
    )));
    vfs
}

fn w3d_chunk(kind: mint_sage_w3d::ChunkType, children: bool, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&kind.0.to_le_bytes());
    let mut size = u32::try_from(payload.len()).unwrap_or(0);
    if children {
        size |= 0x8000_0000;
    }
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn write_name(buf: &mut [u8], name: &str) {
    let bytes = name.as_bytes();
    let n = bytes.len().min(buf.len());
    buf[..n].copy_from_slice(&bytes[..n]);
}

fn tiny_model_bytes() -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut mesh_header = vec![0u8; 116];
    mesh_header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut mesh_header[8..24], "BOX");
    mesh_header[40..44].copy_from_slice(&1u32.to_le_bytes());
    mesh_header[44..48].copy_from_slice(&3u32.to_le_bytes());

    let mut vertices = Vec::new();
    for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in point {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut triangle = vec![0u8; 32];
    triangle[0..4].copy_from_slice(&0u32.to_le_bytes());
    triangle[4..8].copy_from_slice(&1u32.to_le_bytes());
    triangle[8..12].copy_from_slice(&2u32.to_le_bytes());

    let mesh_body = [
        w3d_chunk(ChunkType::MESH_HEADER3, false, &mesh_header),
        w3d_chunk(ChunkType::VERTICES, false, &vertices),
        w3d_chunk(ChunkType::VERTEX_NORMALS, false, &normals),
        w3d_chunk(ChunkType::TRIANGLES, false, &triangle),
    ]
    .concat();

    w3d_chunk(ChunkType::MESH, true, &mesh_body)
}

fn hierarchy_only_bytes() -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut header = vec![0u8; 36];
    header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut header[4..20], "EMPTY");
    header[20..24].copy_from_slice(&0u32.to_le_bytes());
    w3d_chunk(
        ChunkType::HIERARCHY,
        true,
        &w3d_chunk(ChunkType::HIERARCHY_HEADER, false, &header),
    )
}

fn mesh_bytes(name: &str, u_per_sec: Option<f32>) -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut mesh_header = vec![0u8; 116];
    mesh_header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut mesh_header[8..24], name);
    mesh_header[40..44].copy_from_slice(&1u32.to_le_bytes());
    mesh_header[44..48].copy_from_slice(&3u32.to_le_bytes());

    let mut vertices = Vec::new();
    for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in point {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut triangle = vec![0u8; 32];
    triangle[0..4].copy_from_slice(&0u32.to_le_bytes());
    triangle[4..8].copy_from_slice(&1u32.to_le_bytes());
    triangle[8..12].copy_from_slice(&2u32.to_le_bytes());

    let mut body = [
        w3d_chunk(ChunkType::MESH_HEADER3, false, &mesh_header),
        w3d_chunk(ChunkType::VERTICES, false, &vertices),
        w3d_chunk(ChunkType::VERTEX_NORMALS, false, &normals),
        w3d_chunk(ChunkType::TRIANGLES, false, &triangle),
    ]
    .concat();

    if let Some(rate) = u_per_sec {
        let mut info = vec![0u8; 32];
        info[0..4].copy_from_slice(&0x0004_0000u32.to_le_bytes());
        info[24..28].copy_from_slice(&1.0f32.to_le_bytes());
        let args = format!("UPerSec={rate}\r\nVPerSec=0.0\r\nUScale=1.0\r\nVScale=1.0\0");
        let material = [
            w3d_chunk(ChunkType::VERTEX_MATERIAL_NAME, false, b"scroll\0"),
            w3d_chunk(ChunkType::VERTEX_MATERIAL_INFO, false, &info),
            w3d_chunk(ChunkType::VERTEX_MAPPER_ARGS0, false, args.as_bytes()),
        ]
        .concat();
        body.extend_from_slice(&w3d_chunk(
            ChunkType::VERTEX_MATERIALS,
            true,
            &w3d_chunk(ChunkType::VERTEX_MATERIAL, true, &material),
        ));
        body.extend_from_slice(&w3d_chunk(
            ChunkType::MATERIAL_PASS,
            true,
            &w3d_chunk(ChunkType::VERTEX_MATERIAL_IDS, false, &0u32.to_le_bytes()),
        ));
    }

    w3d_chunk(ChunkType::MESH, true, &body)
}

fn still_then_scrolling_model_bytes() -> Vec<u8> {
    [mesh_bytes("STILL", None), mesh_bytes("SCROLL", Some(0.5))].concat()
}

#[test]
fn a_cached_part_keeps_the_uv_mapper_its_material_names() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[
        (
            "art/w3d/sc/scroller.w3d",
            still_then_scrolling_model_bytes(),
        ),
        ("art/w3d/bo/boxmodel.w3d", tiny_model_bytes()),
    ]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();

    let scroller = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "Scroller")
        .expect("the synthetic model loads");
    assert_eq!(scroller.parts.len(), 2);
    assert!(
        scroller.parts[0].uv_mapper.is_none(),
        "the first mesh names no mapper"
    );
    assert!(
        scroller.parts[1].uv_mapper.is_some(),
        "the second mesh's LinearOffset must survive the cache"
    );
    assert!(scroller.has_uv_mappers());

    let still = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "BoxModel")
        .expect("the static model loads");
    assert!(
        !still.has_uv_mappers(),
        "a model with no mapper must not ask to be re-sampled every frame"
    );
}

#[test]
fn the_uv_transform_lands_on_the_scrolling_parts_instances_only() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[(
        "art/w3d/sc/scroller.w3d",
        still_then_scrolling_model_bytes(),
    )]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();
    let model = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "Scroller")
        .expect("the synthetic model loads");

    let placements = [mint_render::Mat4::IDENTITY, mint_render::Mat4::IDENTITY];
    let mut instances = Vec::new();
    model.place_many(&placements, &mut instances);
    assert_eq!(instances.len(), 4, "two parts times two placements");
    let before: Vec<_> = instances.iter().map(|i| i.uv_offset).collect();

    assert!(
        model.apply_uv_mappers(placements.len(), &mut instances, 1.0),
        "a model with a mapper reports that it wrote something"
    );
    assert_eq!(instances[0].uv_offset, before[0], "part 0, placement 0");
    assert_eq!(instances[1].uv_offset, before[1], "part 0, placement 1");
    assert_ne!(instances[2].uv_offset, before[2], "part 1, placement 0");
    assert_ne!(instances[3].uv_offset, before[3], "part 1, placement 1");
    assert_eq!(
        instances[2].uv_offset, instances[3].uv_offset,
        "one sample per part, shared by every placement"
    );
}

#[test]
fn the_uv_transform_moves_with_the_clock_and_only_with_it() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let vfs = vfs_with(&[(
        "art/w3d/sc/scroller.w3d",
        still_then_scrolling_model_bytes(),
    )]);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();
    let model = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "Scroller")
        .expect("the synthetic model loads");

    let at = |seconds: f32| {
        let mut instances = Vec::new();
        model.place_many(&[mint_render::Mat4::IDENTITY], &mut instances);
        model.apply_uv_mappers(1, &mut instances, seconds);
        instances[1].uv_offset
    };
    assert_ne!(at(0.0), at(0.5), "half a second of scrolling must show");
    assert_ne!(at(0.5), at(1.0), "and it must keep moving");
    assert_eq!(at(0.0), at(2.0), "two seconds is exactly one repeat");
    assert_eq!(at(0.0), [0.0, 0.0], "t = 0 is the unscrolled sample point");
    assert_eq!(at(1.0), [0.5, 0.0], "half a repeat, along U only");
}

fn tiny_tga(bgra: [u8; 4]) -> Vec<u8> {
    let mut bytes = vec![0u8; 18];
    bytes[2] = 2;
    bytes[12] = 1;
    bytes[14] = 1;
    bytes[16] = 32;
    bytes[17] = 0x28;
    bytes.extend_from_slice(&bgra);
    bytes
}

fn house_colour_mesh_bytes(
    texture: &str,
    detail_color_func: u8,
    own_stage2: Option<&str>,
) -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut mesh_header = vec![0u8; 116];
    mesh_header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut mesh_header[8..24], "HCBOX");
    mesh_header[40..44].copy_from_slice(&1u32.to_le_bytes());
    mesh_header[44..48].copy_from_slice(&3u32.to_le_bytes());

    let mut vertices = Vec::new();
    for point in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in point {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut normals = Vec::new();
    for _ in 0..3 {
        for value in [0.0f32, 0.0, 1.0] {
            normals.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut triangle = vec![0u8; 32];
    triangle[4..8].copy_from_slice(&1u32.to_le_bytes());
    triangle[8..12].copy_from_slice(&2u32.to_le_bytes());

    let mut shader = vec![0u8; 16];
    shader[9] = detail_color_func;

    let texture_entry = |texture: &str| {
        let mut name = texture.as_bytes().to_vec();
        name.push(0);
        w3d_chunk(
            ChunkType::TEXTURE,
            true,
            &w3d_chunk(ChunkType::TEXTURE_NAME, false, &name),
        )
    };
    let mut textures = texture_entry(texture);
    let mut stages = w3d_chunk(
        ChunkType::TEXTURE_STAGE,
        true,
        &w3d_chunk(ChunkType::TEXTURE_IDS, false, &0u32.to_le_bytes()),
    );
    if let Some(second) = own_stage2 {
        textures.extend(texture_entry(second));
        stages.extend(w3d_chunk(
            ChunkType::TEXTURE_STAGE,
            true,
            &w3d_chunk(ChunkType::TEXTURE_IDS, false, &1u32.to_le_bytes()),
        ));
    }

    [
        w3d_chunk(ChunkType::MESH_HEADER3, false, &mesh_header),
        w3d_chunk(ChunkType::VERTICES, false, &vertices),
        w3d_chunk(ChunkType::VERTEX_NORMALS, false, &normals),
        w3d_chunk(ChunkType::TRIANGLES, false, &triangle),
        w3d_chunk(ChunkType::TEXTURES, true, &textures),
        w3d_chunk(ChunkType::SHADERS, false, &shader),
        w3d_chunk(ChunkType::MATERIAL_PASS, true, &stages),
    ]
    .concat()
    .pipe_mesh()
}

trait PipeMesh {
    fn pipe_mesh(self) -> Vec<u8>;
}
impl PipeMesh for Vec<u8> {
    fn pipe_mesh(self) -> Vec<u8> {
        w3d_chunk(mint_sage_w3d::ChunkType::MESH, true, &self)
    }
}

#[test]
fn a_part_gets_a_house_colour_mask_from_its_listed_texture_alone() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let table = |text: &str| {
        let document = mint_sage_ini::Document::parse(
            "housecolor.ini",
            text,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .expect("the synthetic table parses");
        let mut table = mint_sage_ini::HouseColorTable::new();
        table.insert_document(&document);
        Arc::new(table)
    };
    let mapped = table(
        "HouseColor
  BaseTexture = BaseTex.tga
  HouseTexture = HC_BaseTex.tga
End
",
    );
    let unmapped = table(
        "HouseColor
  BaseTexture = Elsewhere.tga
  HouseTexture = HC_Elsewhere.tga
End
",
    );

    let files = |func: u8, own_stage2: Option<&str>| {
        vec![
            (
                "art/w3d/hc/hcmodel.w3d",
                house_colour_mesh_bytes("BaseTex.tga", func, own_stage2),
            ),
            (
                "art/compiledtextures/ba/basetex.tga",
                tiny_tga([10, 20, 30, 255]),
            ),
            (
                "art/compiledtextures/hc/hc_basetex.tga",
                tiny_tga([255, 255, 255, 255]),
            ),
            (
                "art/compiledtextures/ow/owndetail.tga",
                tiny_tga([1, 2, 3, 255]),
            ),
        ]
    };
    let resolver = Resolver::new();
    let load = |renderer: &mut Renderer, files: Vec<(&str, Vec<u8>)>, table| {
        let vfs = vfs_with(&files);
        let mut cache = ModelCache::new();
        if let Some(table) = table {
            cache.set_house_colors(table);
        }
        let model = cache
            .get_or_load(renderer, &vfs, &resolver, "HcModel")
            .expect("the synthetic model loads");
        model.parts[0].clone()
    };

    for func in [0u8, 11, 12] {
        let part = load(&mut renderer, files(func, None), Some(Arc::clone(&mapped)));
        assert!(part.house_mask.is_some(), "detail_color_func {func}");
        assert!(part.instance.texture2.is_none());
    }

    let part = load(
        &mut renderer,
        files(0, Some("OwnDetail.tga")),
        Some(Arc::clone(&mapped)),
    );
    assert!(
        part.house_mask.is_some(),
        "own second stage no longer exempts"
    );
    assert!(
        part.instance.texture2.is_some(),
        "the file's own stage is cached"
    );

    let part = load(&mut renderer, files(12, None), Some(unmapped));
    assert!(
        part.house_mask.is_none(),
        "an unlisted texture takes no colour"
    );

    let part = load(&mut renderer, files(12, None), None);
    assert!(part.house_mask.is_none(), "no table, no mask");
}

#[test]
fn textures_upload_in_part_order_whatever_order_they_decode_in() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let names: Vec<String> = (0..16u8)
        .map(|n| match n {
            9 => "Tex03.tga".to_owned(),
            12 => "Absent.tga".to_owned(),
            n => format!("Tex{n:02}.tga"),
        })
        .collect();
    let model: Vec<u8> = names
        .iter()
        .flat_map(|name| house_colour_mesh_bytes(name, 0, None))
        .collect();
    let mut files: Vec<(String, Vec<u8>)> = vec![("art/w3d/or/ordered.w3d".to_owned(), model)];
    for n in 0..16u8 {
        files.push((
            format!("art/compiledtextures/te/tex{n:02}.tga"),
            tiny_tga([n, n, n, 255]),
        ));
    }
    let entries: Vec<(&str, Vec<u8>)> = files
        .iter()
        .map(|(path, bytes)| (path.as_str(), bytes.clone()))
        .collect();
    let vfs = vfs_with(&entries);
    let resolver = Resolver::new();
    let mut cache = ModelCache::new();
    let loaded = cache
        .get_or_load(&mut renderer, &vfs, &resolver, "Ordered")
        .expect("the synthetic model loads");
    let handles: Vec<_> = loaded
        .parts
        .iter()
        .map(|part| part.instance.texture.expect("every part binds a texture"))
        .collect();
    assert_eq!(handles.len(), names.len());
    assert_eq!(
        handles[9], handles[3],
        "a reused name keeps its first upload"
    );
    let fresh: Vec<_> = handles
        .iter()
        .enumerate()
        .filter(|(at, _)| *at != 9)
        .map(|(_, handle)| *handle)
        .collect();
    assert!(
        fresh.windows(2).all(|pair| pair[0] < pair[1]),
        "uploads follow the parts' order: {handles:?}"
    );
    assert_eq!(cache.stats().textures, 15);
}
