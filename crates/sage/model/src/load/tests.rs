use super::*;
use std::sync::Arc;

struct FakeMount(Vec<VfsPath>);

impl mint_vfs::Mount for FakeMount {
    fn label(&self) -> &str {
        "fake"
    }
    fn entries(&self) -> Vec<VfsPath> {
        self.0.clone()
    }
    fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
        Err(mint_vfs::Error::NotFound { path: path.clone() })
    }
}

fn vfs_with(paths: &[&str]) -> Vfs {
    let mut vfs = Vfs::new();
    vfs.mount(Arc::new(FakeMount(
        paths.iter().map(|p| VfsPath::new(p)).collect(),
    )));
    vfs
}

#[test]
fn model_search_matches_on_the_filename_and_ignores_other_extensions() {
    let vfs = vfs_with(&[
        r"art\w3d\gb\gbfarm.w3d",
        r"art\w3d\gb\gbfarm_skl.w3d",
        r"art\w3d\mu\muhovel.w3d",
        r"art\compiledtextures\gb\gbfarm.dds",
    ]);

    assert_eq!(
        find_models(&vfs, "farm"),
        vec!["gbfarm.w3d".to_owned(), "gbfarm_skl.w3d".to_owned()]
    );
    assert_eq!(find_models(&vfs, "hovel"), vec!["muhovel.w3d".to_owned()]);
    assert_eq!(find_models(&vfs, "").len(), 3, "the .dds is not a model");
}

#[test]
fn model_search_is_case_insensitive_because_vfs_paths_are_normalised() {
    let vfs = vfs_with(&[r"Art\W3D\GB\GBFarm.W3D"]);
    assert_eq!(find_models(&vfs, "GBFARM"), vec!["gbfarm.w3d".to_owned()]);
}

#[test]
fn a_name_without_an_extension_is_looked_up_as_a_w3d() {
    let vfs = Vfs::new();
    let error = load_model(&vfs, &Resolver::new(), "gbfarm").expect_err("nothing is mounted");
    match error {
        Error::Unresolved { name, .. } => assert_eq!(name, "gbfarm.w3d"),
        other => panic!("expected Unresolved, got {other:?}"),
    }
}

#[test]
fn an_extension_that_is_already_present_is_not_doubled() {
    let vfs = Vfs::new();
    let error = load_model(&vfs, &Resolver::new(), "GBFarm.W3D").expect_err("nothing is mounted");
    match error {
        Error::Unresolved { name, .. } => assert_eq!(name, "GBFarm.W3D"),
        other => panic!("expected Unresolved, got {other:?}"),
    }
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

fn vfs_with_files(entries: &[(&str, Vec<u8>)]) -> Vfs {
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
    if let Some(slot) = buf.get_mut(..n) {
        slot.copy_from_slice(&bytes[..n]);
    }
}

fn synthetic_hierarchy_file(name: &str) -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut header = vec![0u8; 36];
    header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut header[4..20], name);
    header[20..24].copy_from_slice(&1u32.to_le_bytes());

    let mut pivot = vec![0u8; 60];
    write_name(&mut pivot[0..16], "ROOT");
    pivot[16..20].copy_from_slice(&u32::MAX.to_le_bytes());

    let body = [
        w3d_chunk(ChunkType::HIERARCHY_HEADER, false, &header),
        w3d_chunk(ChunkType::PIVOTS, false, &pivot),
    ]
    .concat();
    w3d_chunk(ChunkType::HIERARCHY, true, &body)
}

fn synthetic_animation_file(name: &str, hierarchy_name: &str, num_frames: u32) -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut header = vec![0u8; 44];
    header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut header[4..20], name);
    write_name(&mut header[20..36], hierarchy_name);
    header[36..40].copy_from_slice(&num_frames.to_le_bytes());
    header[40..44].copy_from_slice(&30u32.to_le_bytes());

    w3d_chunk(
        ChunkType::ANIMATION,
        true,
        &w3d_chunk(ChunkType::ANIMATION_HEADER, false, &header),
    )
}

#[test]
fn load_animation_resolves_its_hierarchy_from_a_separate_skl_file() {
    let vfs = vfs_with_files(&[
        (
            r"art\w3d\mu\muwalk.w3d",
            synthetic_animation_file("WALK", "MUSKEL", 40),
        ),
        (
            r"art\w3d\mu\muskel_skl.w3d",
            synthetic_hierarchy_file("MUSKEL"),
        ),
    ]);
    let loaded =
        load_animation(&vfs, &Resolver::new(), "muwalk").expect("should resolve and parse");
    assert_eq!(loaded.animation.name, "WALK");
    assert_eq!(loaded.animation.num_frames, 40);
    assert_eq!(loaded.hierarchy.len(), 1);
    assert_eq!(
        loaded.hierarchy_path.as_str(),
        "art/w3d/mu/muskel_skl.w3d",
        "the _skl spelling is what the file actually mounted under"
    );
}

#[test]
fn load_animation_fails_and_names_what_it_looked_for_when_no_skeleton_is_mounted() {
    let vfs = vfs_with_files(&[(
        r"art\w3d\mu\muwalk.w3d",
        synthetic_animation_file("WALK", "MUSKEL", 40),
    )]);
    let error =
        load_animation(&vfs, &Resolver::new(), "muwalk").expect_err("no file supplies MUSKEL");
    match error {
        Error::SkeletonNotFound {
            hierarchy, tried, ..
        } => {
            assert_eq!(hierarchy, "MUSKEL");
            assert_eq!(
                tried,
                vec!["muskel.w3d".to_owned(), "muskel_skl.w3d".to_owned()]
            );
        }
        other => panic!("expected SkeletonNotFound, got {other:?}"),
    }
}

#[test]
fn load_animation_on_a_file_with_no_animation_is_reported_as_such() {
    let vfs = vfs_with_files(&[(r"art\w3d\mu\muskel.w3d", synthetic_hierarchy_file("MUSKEL"))]);
    let error = load_animation(&vfs, &Resolver::new(), "muskel").expect_err("no animation here");
    match error {
        Error::NoAnimation { name, found } => {
            assert_eq!(name, "muskel.w3d");
            assert_eq!(
                found, "HIERARCHY",
                "the message should name what was found instead of an animation"
            );
        }
        other => panic!("expected NoAnimation, got {other:?}"),
    }
}

#[test]
fn load_model_keeps_the_hierarchy_it_resolved_for_later_posing() {
    let vfs = vfs_with_files(&[
        (r"art\w3d\bo\box.w3d", tiny_model_bytes()),
        (
            r"art\w3d\bo\boxskel_skl.w3d",
            synthetic_hierarchy_file("BOXSKEL"),
        ),
    ]);
    let loaded = load_model(&vfs, &Resolver::new(), "box").expect("should load");
    let hierarchy = loaded.hierarchy.expect("hierarchy resolved");
    assert_eq!(hierarchy.header.name, "BOXSKEL");
}

#[test]
fn a_same_file_skeleton_still_resolves_and_binds_with_no_second_read() {
    let bytes = [tiny_model_bytes(), synthetic_hierarchy_file("BOXSKEL")].concat();
    let vfs = vfs_with_files(&[(r"art\w3d\bo\box.w3d", bytes)]);

    let loaded = load_model(&vfs, &Resolver::new(), "box").expect("should load");
    assert_eq!(
        loaded.skeleton, None,
        "an in-file hierarchy needs no second file"
    );
    let hierarchy = loaded
        .hierarchy
        .expect("hierarchy resolved from the same file");
    assert_eq!(hierarchy.header.name, "BOXSKEL");

    let part = loaded.model.parts.first().expect("the box mesh converted");
    assert_eq!(
        part.pivot,
        Some(0),
        "tiny_model_bytes's sub-object names bone_index 0, and that binding \
             must hold whether the hierarchy came from this file or another one"
    );
}

#[test]
fn a_model_with_no_skeleton_anywhere_still_renders_statically() {
    let vfs = vfs_with_files(&[(r"art\w3d\bo\box.w3d", tiny_model_bytes())]);

    let loaded = load_model(&vfs, &Resolver::new(), "box").expect("should still load");
    assert_eq!(loaded.hierarchy, None);
    assert_eq!(loaded.skeleton, None);

    let part = loaded.model.parts.first().expect("the box mesh converted");
    assert_eq!(
        part.transform,
        mint_render::Mat4::IDENTITY,
        "with nothing to place it, the part sits at the origin rather than \
             refusing to convert at all"
    );
    assert!(
        loaded
            .model
            .notes
            .iter()
            .any(|note| note.contains("BOXSKEL") && note.contains("no mounted file")),
        "the gap must be visible, not silent: {:?}",
        loaded.model.notes
    );
}

fn tiny_model_bytes() -> Vec<u8> {
    use mint_sage_w3d::ChunkType;
    let mut mesh_header = vec![0u8; 116];
    mesh_header[0..4].copy_from_slice(&0x0004_0001u32.to_le_bytes());
    write_name(&mut mesh_header[8..24], "BOX");
    mesh_header[40..44].copy_from_slice(&1u32.to_le_bytes());
    mesh_header[44..48].copy_from_slice(&2u32.to_le_bytes());

    let mesh_body = [
        w3d_chunk(ChunkType::MESH_HEADER3, false, &mesh_header),
        w3d_chunk(ChunkType::VERTICES, false, &[0u8; 24]),
        w3d_chunk(ChunkType::VERTEX_NORMALS, false, &[0u8; 24]),
        w3d_chunk(ChunkType::TRIANGLES, false, &[0u8; 32]),
    ]
    .concat();

    let mut hlod_header = vec![0u8; 40];
    hlod_header[4..8].copy_from_slice(&1u32.to_le_bytes());
    write_name(&mut hlod_header[8..24], "BOX");
    write_name(&mut hlod_header[24..40], "BOXSKEL");

    let mut sub_object = vec![0u8; 36];
    write_name(&mut sub_object[4..11], "BOX.BOX");

    let mut lod_array_header = vec![0u8; 8];
    lod_array_header[0..4].copy_from_slice(&1u32.to_le_bytes());
    lod_array_header[4..8].copy_from_slice(&f32::MAX.to_le_bytes());

    let hlod_body = [
        w3d_chunk(ChunkType::HLOD_HEADER, false, &hlod_header),
        w3d_chunk(
            ChunkType::HLOD_LOD_ARRAY,
            true,
            &[
                w3d_chunk(
                    ChunkType::HLOD_SUB_OBJECT_ARRAY_HEADER,
                    false,
                    &lod_array_header,
                ),
                w3d_chunk(ChunkType::HLOD_SUB_OBJECT, false, &sub_object),
            ]
            .concat(),
        ),
    ]
    .concat();

    [
        w3d_chunk(ChunkType::MESH, true, &mesh_body),
        w3d_chunk(ChunkType::HLOD, true, &hlod_body),
    ]
    .concat()
}
