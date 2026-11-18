#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use mint_render::{
    directional_light_view_proj, Aabb, Blend, Camera, ColorLut, ContactShadows,
    CustomMaterialSource, CustomShader, DirectionalLight, DistanceFog, Error, Glow, GlowTap,
    Instance, LightCookie, LightSet, MaskShape, Mat4, MeshData, MipLevel, PipelineCacheState,
    PixelFormat, PostEffects, Renderer, Scene, ScreenDraw, ShadowCascade, ShadowEdge, ShadowSample,
    TextureData, UiGeometry, UiQuad, Vec3, Vertex, VisibilityMask,
};

const SIZE: u32 = 64;

fn renderer(size: u32) -> Option<Renderer> {
    match Renderer::offscreen(size, size) {
        Ok(renderer) => Some(renderer),
        Err(Error::NoAdapter) => {
            eprintln!(
                "SKIPPED: no graphics adapter on this machine, so nothing in \
                 tests/headless.rs was actually verified"
            );
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * size + x) * 4) as usize;
    [
        rgba[index],
        rgba[index + 1],
        rgba[index + 2],
        rgba[index + 3],
    ]
}

fn quad() -> MeshData {
    MeshData {
        vertices: vec![
            Vertex::new([-1.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            Vertex::new([1.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0]),
            Vertex::new([1.0, -1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0]),
            Vertex::new([-1.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn upright_quad() -> MeshData {
    MeshData {
        vertices: vec![
            Vertex::new([-1.0, 0.0, 2.0], [0.0, -1.0, 0.0], [0.0, 0.0]),
            Vertex::new([1.0, 0.0, 2.0], [0.0, -1.0, 0.0], [1.0, 0.0]),
            Vertex::new([1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [1.0, 1.0]),
            Vertex::new([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 1.0]),
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

fn upright_camera() -> Camera {
    Camera {
        eye: Vec3::new(0.0, -3.0, 1.0),
        target: Vec3::new(0.0, 0.0, 1.0),
        up: Vec3::new(0.0, 0.0, 1.0),
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.05,
        far: 20.0,
    }
}

fn unit_camera() -> Camera {
    Camera {
        eye: Vec3::new(0.0, 0.0, 1.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.05,
        far: 10.0,
    }
}

fn flat_scene(camera: Camera) -> Scene<'static> {
    let mut scene = Scene::new(camera);
    scene.lights = [DirectionalLight::default(); 3];
    scene.ambient_color = [1.0, 1.0, 1.0];
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene
}

fn solid(rgba: [u8; 4]) -> TextureData {
    TextureData::rgba8(1, 1, rgba.to_vec()).expect("1x1 RGBA is four bytes")
}

fn solid_linear(rgba: [u8; 4]) -> TextureData {
    TextureData::single(PixelFormat::Rgba8Unorm, 1, 1, rgba.to_vec())
}

#[test]
fn a_triangle_covers_the_centre_and_leaves_the_corners_clear() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer
        .upload_mesh(&MeshData {
            vertices: vec![
                Vertex::new([-0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.0, 0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
        })
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "the triangle should cover the centre of the frame"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 1, 1),
        [0, 0, 0, 255],
        "the corner should still be the clear colour"
    );
}

fn draws_a_triangle(renderer: &mut Renderer) -> bool {
    let mesh = renderer
        .upload_mesh(&MeshData {
            vertices: vec![
                Vertex::new([-0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.0, 0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
        })
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    pixel(&rgba, SIZE, SIZE / 2, SIZE / 2) == [255, 255, 255, 255]
}

#[test]
fn a_damaged_pipeline_cache_file_is_ignored_and_the_renderer_still_draws() {
    let dir =
        std::env::temp_dir().join(format!("mint-render-pipeline-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let open = || match Renderer::offscreen_with_pipeline_cache(SIZE, SIZE, Some(&dir)) {
        Ok(renderer) => Some(renderer),
        Err(Error::NoAdapter) => None,
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    };
    let Some(mut first) = open() else {
        eprintln!("SKIPPED: no graphics adapter");
        return;
    };
    if first.pipeline_cache_state() == PipelineCacheState::Off {
        eprintln!("SKIPPED: this backend keeps no pipeline cache");
        return;
    }
    assert_eq!(first.pipeline_cache_state(), PipelineCacheState::Empty);
    assert!(
        first.save_pipeline_cache().expect("save"),
        "a new cache is written"
    );
    assert!(
        !first.save_pipeline_cache().expect("save"),
        "an unchanged one is not"
    );
    let file = std::fs::read_dir(&dir)
        .expect("cache dir")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .expect("a cache file");
    let sealed = std::fs::read(&file).expect("read");

    let mut warm = open().expect("renderer");
    assert!(
        matches!(warm.pipeline_cache_state(), PipelineCacheState::Loaded { bytes } if bytes > 0),
        "{:?}",
        warm.pipeline_cache_state()
    );
    assert!(draws_a_triangle(&mut warm));

    std::fs::write(&file, &sealed[..sealed.len() / 2]).expect("truncate");
    let mut truncated = open().expect("renderer");
    assert!(
        matches!(
            truncated.pipeline_cache_state(),
            PipelineCacheState::Ignored { .. }
        ),
        "{:?}",
        truncated.pipeline_cache_state()
    );
    assert!(draws_a_triangle(&mut truncated));

    let mut foreign = sealed.clone();
    foreign[16 + 17] ^= 0xff;
    let checksum = foreign[16..]
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    foreign[8..16].copy_from_slice(&checksum.to_le_bytes());
    std::fs::write(&file, &foreign).expect("write");
    let mut refused = open().expect("renderer");
    assert!(
        matches!(
            refused.pipeline_cache_state(),
            PipelineCacheState::Ignored { .. }
        ),
        "{:?}",
        refused.pipeline_cache_state()
    );
    assert!(draws_a_triangle(&mut refused));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_invisible_instance_is_not_drawn_but_reappears_when_flipped_back() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");
    let mut instances = [Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    }];

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let visible_frame = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&visible_frame, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "the quad should be drawn by default"
    );

    instances[0].visible = false;
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let hidden_frame = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&hidden_frame, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "an invisible instance must not draw at all, opaque or not"
    );

    instances[0].visible = true;
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let reappeared_frame = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&reappeared_frame, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "flipping visible back on must draw the same instance again"
    );
}

#[test]
fn update_mesh_vertices_moves_a_triangle_uploaded_once() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer
        .upload_mesh(&MeshData {
            vertices: vec![
                Vertex::new([-0.9, -0.9, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([-0.7, -0.9, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([-0.8, -0.7, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
        })
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    }];

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let before = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&before, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "the triangle starts in the corner, not the centre"
    );

    renderer
        .update_mesh_vertices(
            mesh,
            &MeshData {
                vertices: vec![
                    Vertex::new([-0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                    Vertex::new([0.5, -0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                    Vertex::new([0.0, 0.5, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                ],
                indices: vec![0, 1, 2],
            },
        )
        .expect("update");
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let after = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&after, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "the same handle should now draw the moved triangle over the centre"
    );
}

#[test]
fn update_mesh_vertices_refuses_a_different_vertex_count() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let fewer = MeshData {
        vertices: vec![Vertex::default(); 3],
        indices: vec![0, 1, 2],
    };
    let error = renderer
        .update_mesh_vertices(mesh, &fewer)
        .expect_err("quad has 4 vertices, not 3");
    assert!(matches!(
        error,
        Error::VertexCountMismatch {
            original: 4,
            supplied: 3
        }
    ));
}

#[test]
fn update_mesh_vertices_on_an_unknown_handle_is_an_error_not_a_wild_write() {
    let Some(mut first) = renderer(SIZE) else {
        return;
    };
    let Some(mut second) = renderer(SIZE) else {
        return;
    };
    let _ = first.upload_mesh(&quad()).expect("upload");
    let stranger = first.upload_mesh(&quad()).expect("upload");

    let error = second
        .update_mesh_vertices(stranger, &quad())
        .expect_err("handle came from a different renderer");
    assert!(matches!(error, Error::UnknownHandle));
}

#[test]
fn each_instance_gets_its_own_transform_and_its_own_texture() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");
    let green = renderer
        .upload_texture(&solid([0, 255, 0, 255]))
        .expect("upload");
    let blue = renderer
        .upload_texture(&solid([0, 0, 255, 255]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let place = |x: f32, y: f32| {
        Mat4::from_translation(Vec3::new(x, y, 0.0)).mul_mat4(Mat4::from_scale(Vec3::splat(0.5)))
    };
    let instances = [
        Instance {
            texture: Some(red),
            transform: place(-0.5, 0.5),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(green),
            transform: place(0.5, 0.5),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(blue),
            transform: place(-0.5, -0.5),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(white),
            transform: place(0.5, -0.5),
            ..Instance::new(mesh)
        },
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let quarter = SIZE / 4;
    let three_quarters = SIZE - quarter;
    assert_eq!(
        pixel(&rgba, SIZE, quarter, quarter),
        [255, 0, 0, 255],
        "world (-0.5, +0.5) must land top-left; +Y is up on screen"
    );
    assert_eq!(
        pixel(&rgba, SIZE, three_quarters, quarter),
        [0, 255, 0, 255]
    );
    assert_eq!(
        pixel(&rgba, SIZE, quarter, three_quarters),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(&rgba, SIZE, three_quarters, three_quarters),
        [255, 255, 255, 255]
    );
}

#[test]
fn the_nearer_surface_wins_whichever_order_it_is_drawn_in() {
    for near_first in [false, true] {
        let Some(mut renderer) = renderer(SIZE) else {
            return;
        };
        let mesh = renderer.upload_mesh(&quad()).expect("upload");
        let red = renderer
            .upload_texture(&solid([255, 0, 0, 255]))
            .expect("upload");
        let green = renderer
            .upload_texture(&solid([0, 255, 0, 255]))
            .expect("upload");

        let near = Instance {
            texture: Some(red),
            transform: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.25)),
            ..Instance::new(mesh)
        };
        let far = Instance {
            texture: Some(green),
            transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -0.25)),
            ..Instance::new(mesh)
        };
        let instances = if near_first { [near, far] } else { [far, near] };

        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");

        assert_eq!(
            pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
            [255, 0, 0, 255],
            "the nearer quad must win (near drawn first: {near_first})"
        );
    }
}

#[test]
fn backface_culling_discards_exactly_the_triangles_wound_away_from_the_camera() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let front = renderer
        .upload_mesh(&MeshData {
            vertices: vec![
                Vertex::new([-0.8, -0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.8, -0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.0, 0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 1, 2],
        })
        .expect("upload");
    let back = renderer
        .upload_mesh(&MeshData {
            vertices: vec![
                Vertex::new([-0.8, -0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.8, -0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
                Vertex::new([0.0, 0.8, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0]),
            ],
            indices: vec![0, 2, 1],
        })
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let centre = (SIZE / 2, SIZE / 2);
    let draw = |renderer: &mut Renderer, mesh| {
        let instances = [Instance {
            texture: Some(white),
            ..Instance::new(mesh)
        }];
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, centre.0, centre.1)
    };

    assert!(!renderer.backface_culling(), "culling is off by default");
    assert_eq!(draw(&mut renderer, back), [255, 255, 255, 255]);

    renderer.set_backface_culling(true);
    assert_eq!(
        draw(&mut renderer, front),
        [255, 255, 255, 255],
        "a counter-clockwise triangle faces the camera"
    );
    assert_eq!(
        draw(&mut renderer, back),
        [0, 0, 0, 255],
        "a clockwise triangle must be culled"
    );
}

#[test]
fn texture_coordinates_have_their_origin_at_the_top_left() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(
            &TextureData::rgba8(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            )
            .expect("2x2 RGBA"),
        )
        .expect("upload");

    let instances = [Instance {
        texture: Some(texture),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let quarter = SIZE / 4;
    let three_quarters = SIZE - quarter;
    let dominant = |p: [u8; 4]| -> usize {
        let mut best = 0;
        for i in 1..3 {
            if p[i] > p[best] {
                best = i;
            }
        }
        best
    };

    assert_eq!(
        dominant(pixel(&rgba, SIZE, quarter, quarter)),
        0,
        "uv (0,0) belongs at the top-left of the frame"
    );
    assert_eq!(dominant(pixel(&rgba, SIZE, three_quarters, quarter)), 1);
    assert_eq!(dominant(pixel(&rgba, SIZE, quarter, three_quarters)), 2);
    let white = pixel(&rgba, SIZE, three_quarters, three_quarters);
    assert!(
        white[0] > 180 && white[1] > 180 && white[2] > 180,
        "uv (1,1) belongs at the bottom-right, got {white:?}"
    );
}

#[test]
fn rows_updated_in_place_draw_the_same_as_the_whole_texture_uploaded_fresh() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let draw = |renderer: &mut Renderer, texture| {
        let instances = [Instance {
            texture: Some(texture),
            ..Instance::new(mesh)
        }];
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        renderer.read_pixels().expect("readback")
    };
    let first = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
    let updated = renderer
        .upload_texture(&TextureData::rgba8(2, 2, first).expect("2x2 RGBA"))
        .expect("upload");
    let second_row = [0, 0, 255, 255, 255, 255, 255, 255];
    renderer
        .update_texture_rows(updated, 1, &second_row)
        .expect("one whole row inside the texture");
    let in_place = draw(&mut renderer, updated);

    let whole = renderer
        .upload_texture(
            &TextureData::rgba8(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            )
            .expect("2x2 RGBA"),
        )
        .expect("upload");
    assert_eq!(in_place, draw(&mut renderer, whole));

    for (first_row, rows) in [
        (1, &second_row[..4]),
        (2, &second_row[..]),
        (0, &[0u8; 24][..]),
    ] {
        assert!(
            matches!(
                renderer.update_texture_rows(updated, first_row, rows),
                Err(Error::TextureShapeMismatch { .. })
            ),
            "{} byte(s) at row {first_row} are not whole rows inside 2x2",
            rows.len()
        );
    }
}

#[test]
fn a_zeroed_texture_with_one_band_written_draws_as_zeros_elsewhere() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let draw = |renderer: &mut Renderer, texture| {
        let instances = [Instance {
            texture: Some(texture),
            ..Instance::new(mesh)
        }];
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        renderer.read_pixels().expect("readback")
    };
    let band = renderer
        .zeroed_texture(2, 2, PixelFormat::Rgba8Unorm)
        .expect("a 2x2 page");
    renderer
        .update_texture_rows(band, 1, &[0, 0, 255, 255, 255, 255, 255, 255])
        .expect("row 1");
    let banded = draw(&mut renderer, band);
    let mut whole = vec![0; 8];
    whole.extend([0, 0, 255, 255, 255, 255, 255, 255]);
    let whole = renderer
        .upload_texture(&TextureData::rgba8(2, 2, whole).expect("2x2 RGBA"))
        .expect("upload");
    assert_eq!(banded, draw(&mut renderer, whole));
    assert!(matches!(
        renderer.zeroed_texture(4, 4, PixelFormat::Bc1Unorm),
        Err(Error::BlockCompressionUnsupported)
    ));
}

#[test]
fn a_bc1_block_is_uploaded_compressed_and_decoded_by_the_gpu() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    if !renderer.supports_block_compression() {
        eprintln!(
            "SKIPPED: this adapter reports no BC support, so the compressed path was not exercised"
        );
        return;
    }

    let mut block = Vec::with_capacity(8);
    block.extend_from_slice(&0xF800u16.to_le_bytes());
    block.extend_from_slice(&0x0000u16.to_le_bytes());
    block.extend_from_slice(&[0u8; 4]);

    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(&TextureData::single(PixelFormat::Bc1Srgb, 4, 4, block))
        .expect("BC1 upload");

    let instances = [Instance {
        texture: Some(texture),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 0, 0, 255],
        "the GPU should decode the block to pure red"
    );
}

#[test]
fn a_bc3_block_is_uploaded_compressed_and_decoded_by_the_gpu() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    if !renderer.supports_block_compression() {
        eprintln!("SKIPPED: this adapter reports no BC support");
        return;
    }

    let mut block = vec![255u8, 0];
    block.extend_from_slice(&[0u8; 6]);
    block.extend_from_slice(&0xF800u16.to_le_bytes());
    block.extend_from_slice(&0x0000u16.to_le_bytes());
    block.extend_from_slice(&[0u8; 4]);
    assert_eq!(block.len(), 16, "a BC3 block is sixteen bytes");

    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(&TextureData::single(PixelFormat::Bc3Srgb, 4, 4, block))
        .expect("BC3 upload");

    let instances = [Instance {
        texture: Some(texture),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 0, 0, 255],
        "the GPU should decode the BC3 block to opaque pure red"
    );
}

#[test]
fn a_mip_chain_is_uploaded_level_by_level_and_the_top_one_is_what_is_sampled() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let level = |w: u32, h: u32, colour: [u8; 4]| MipLevel {
        width: w,
        height: h,
        data: colour.repeat((w * h) as usize),
    };
    let texture = renderer
        .upload_texture(&TextureData {
            format: PixelFormat::Rgba8Srgb,
            levels: vec![
                level(4, 4, [255, 0, 0, 255]),
                level(2, 2, [0, 255, 0, 255]),
                level(1, 1, [0, 0, 255, 255]),
            ],
            address_u: mint_render::AddressMode::Repeat,
            address_v: mint_render::AddressMode::Repeat,
        })
        .expect("upload");
    let mesh = renderer.upload_mesh(&quad()).expect("upload");

    let instances = [Instance {
        texture: Some(texture),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [255, 0, 0, 255]);
}

#[test]
fn a_fragment_below_the_alpha_cutoff_is_discarded() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let transparent = renderer
        .upload_texture(&solid([255, 255, 255, 0]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(transparent),
        alpha_cutoff: 0.5,
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "a fully transparent texel must not be painted"
    );

    let opaque_pass = [Instance {
        texture: Some(transparent),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&opaque_pass))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let painted = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    assert_eq!(
        painted,
        [255, 255, 255, 255],
        "with the cutout off, the same texel is painted - and opaquely, because          a texture's alpha is not the frame's"
    );
}

#[test]
fn a_merged_outline_is_hidden_wherever_any_merge_mask_covers_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");
    let faint = renderer
        .upload_texture(&solid([255, 255, 255, 40]))
        .expect("upload");
    let half = |x: f32| {
        Mat4::from_translation(Vec3::new(x, 0.0, 0.0))
            .mul_mat4(Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0)))
    };
    let instances = [
        Instance {
            texture: Some(red),
            blend: Blend::MergeAlphaBlend,
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(white),
            blend: Blend::MergeMask,
            alpha_cutoff: 0.375,
            transform: half(0.5),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(faint),
            blend: Blend::MergeMask,
            alpha_cutoff: 0.375,
            transform: half(-0.5),
            ..Instance::new(mesh)
        },
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 4, SIZE / 2),
        [255, 0, 0, 255],
        "outside every mask the outline draws"
    );
    assert_eq!(
        pixel(&rgba, SIZE, SIZE * 3 / 4, SIZE / 2),
        [0, 0, 0, 255],
        "under a mask nothing draws, and the mask itself writes no colour"
    );
}

#[test]
fn every_rendered_pixel_is_opaque_whatever_the_textures_alpha_says() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let faint = renderer
        .upload_texture(&solid_linear([200, 30, 30, 3]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(faint),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    for (x, y) in [(1, 1), (SIZE / 2, SIZE / 2), (SIZE - 2, SIZE - 2)] {
        assert_eq!(pixel(&rgba, SIZE, x, y)[3], 255, "pixel ({x}, {y})");
    }
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [200, 30, 30, 255],
        "and the colour is the texture's, undimmed by its alpha"
    );
}

#[test]
fn an_alpha_blended_instance_composites_with_the_background_rather_than_replacing_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        tint: [1.0, 1.0, 1.0, 0.5],
        blend: Blend::AlphaBlend,
        ..Instance::new(mesh)
    }];
    let mut scene = flat_scene(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    renderer
        .render(&scene.with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let [r, g, b, a] = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);

    for (channel, value) in [("r", r), ("g", g), ("b", b)] {
        assert!(
            value > 40 && value < 220,
            "channel {channel} = {value}, expected something between black and \
             full white, not either extreme"
        );
    }
    assert_eq!(a, 255, "over an opaque background");
    let mut clear = flat_scene(unit_camera());
    clear.clear_color = [0.0, 0.0, 0.0, 0.0];
    renderer
        .render(&clear.with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let a = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)[3];
    assert!(a.abs_diff(128) <= 2, "over a transparent one: {a}");
}

#[test]
fn an_additive_instance_brightens_the_background_rather_than_composing_over_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        tint: [1.0, 1.0, 1.0, 0.2],
        blend: Blend::Additive,
        ..Instance::new(mesh)
    }];
    let mut scene = flat_scene(unit_camera());
    scene.clear_color = [0.3, 0.3, 0.3, 1.0];
    renderer
        .render(&scene.with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let [r, g, b, _] = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);

    for (channel, value) in [("r", r), ("g", g), ("b", b)] {
        assert!(
            value > 160,
            "channel {channel} = {value}, expected brighter than the ~77 the \
             unlit 0.3-grey background alone would now store as"
        );
    }
}

#[test]
fn additive_glows_brighter_than_alpha_blend_would_for_the_same_translucent_source() {
    let mut totals = [0u32; 2];
    for (index, blend) in [Blend::AlphaBlend, Blend::Additive].into_iter().enumerate() {
        let Some(mut renderer) = renderer(SIZE) else {
            return;
        };
        let mesh = renderer.upload_mesh(&quad()).expect("upload");
        let white = renderer
            .upload_texture(&solid([255, 255, 255, 255]))
            .expect("upload");
        let instances = [Instance {
            texture: Some(white),
            tint: [1.0, 1.0, 1.0, 0.3],
            blend,
            ..Instance::new(mesh)
        }];
        let mut scene = flat_scene(unit_camera());
        scene.clear_color = [0.1, 0.1, 0.1, 1.0];
        renderer
            .render(&scene.with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        let [r, g, b, _] = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
        totals[index] = u32::from(r) + u32::from(g) + u32::from(b);
    }
    assert!(
        totals[1] > totals[0],
        "additive brightness {} should exceed alpha-blend brightness {}",
        totals[1],
        totals[0]
    );
}

#[test]
fn overlapping_blended_instances_composite_the_same_regardless_of_submission_order() {
    let mut results = Vec::new();
    for reordered in [false, true] {
        let Some(mut renderer) = renderer(SIZE) else {
            return;
        };
        let mesh = renderer.upload_mesh(&quad()).expect("upload");
        let red = renderer
            .upload_texture(&solid([255, 0, 0, 255]))
            .expect("upload");
        let green = renderer
            .upload_texture(&solid([0, 255, 0, 255]))
            .expect("upload");

        let near = Instance {
            texture: Some(red),
            transform: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.25)),
            tint: [1.0, 1.0, 1.0, 0.5],
            blend: Blend::AlphaBlend,
            ..Instance::new(mesh)
        };
        let far = Instance {
            texture: Some(green),
            transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -0.25)),
            tint: [1.0, 1.0, 1.0, 0.5],
            blend: Blend::AlphaBlend,
            ..Instance::new(mesh)
        };
        let instances = if reordered { [near, far] } else { [far, near] };

        let mut scene = flat_scene(unit_camera());
        scene.clear_color = [0.0, 0.0, 0.0, 1.0];
        renderer
            .render(&scene.with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        results.push(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2));
    }
    assert_eq!(
        results[0], results[1],
        "submission order must not change the composited result: {results:?}"
    );
    assert_ne!(results[0][0..3], [255, 0, 0]);
    assert_ne!(results[0][0..3], [0, 255, 0]);
}

#[test]
fn a_blended_instance_does_not_occlude_opaque_geometry_behind_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");
    let blue = renderer
        .upload_texture(&solid([0, 0, 255, 255]))
        .expect("upload");

    let near_blended = Instance {
        texture: Some(red),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.25)),
        tint: [1.0, 1.0, 1.0, 0.5],
        blend: Blend::AlphaBlend,
        ..Instance::new(mesh)
    };
    let far_opaque = Instance {
        texture: Some(blue),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -0.25)),
        ..Instance::new(mesh)
    };
    let instances = [near_blended, far_opaque];
    let mut scene = flat_scene(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    renderer
        .render(&scene.with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let [r, _, b, _] = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);

    assert!(b > 40, "blue channel = {b}, the far quad must show through");
    assert!(
        r > 40,
        "red channel = {r}, the near quad must still tint the result"
    );
}

#[test]
fn resizing_changes_the_readback_dimensions() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    renderer.resize(32, 16);
    assert_eq!(renderer.size(), (32, 16));
    renderer.render(&flat_scene(unit_camera())).expect("render");
    assert_eq!(renderer.read_pixels().expect("readback").len(), 32 * 16 * 4);

    renderer.resize(0, 0);
    assert_eq!(renderer.size(), (1, 1));
    renderer.render(&flat_scene(unit_camera())).expect("render");
    assert_eq!(renderer.read_pixels().expect("readback").len(), 4);
}

#[test]
fn a_mesh_with_an_out_of_range_index_is_refused_rather_than_uploaded() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let bad = MeshData {
        vertices: vec![Vertex::default(); 3],
        indices: vec![0, 1, 9],
    };
    assert!(matches!(
        renderer.upload_mesh(&bad),
        Err(Error::BadIndex { at: 2, vertices: 3 })
    ));
    assert!(matches!(
        renderer.upload_mesh(&MeshData::default()),
        Err(Error::EmptyMesh)
    ));
}

#[test]
fn a_handle_from_another_renderer_is_an_error_and_not_a_wild_read() {
    let Some(mut first) = renderer(SIZE) else {
        return;
    };
    let Some(mut second) = renderer(SIZE) else {
        return;
    };
    let _ = first.upload_mesh(&quad()).expect("upload");
    let stranger = first.upload_mesh(&quad()).expect("upload");

    let instances = [Instance::new(stranger)];
    assert!(matches!(
        second.render(&flat_scene(unit_camera()).with_instances(&instances)),
        Err(Error::UnknownHandle)
    ));
}

#[test]
fn an_offscreen_renderer_reads_back_before_anything_has_been_drawn() {
    let Some(renderer) = renderer(SIZE) else {
        return;
    };
    assert!(renderer.read_pixels().is_ok());
}

fn tangent_quad(w: f32) -> MeshData {
    let mut mesh = quad();
    for vertex in &mut mesh.vertices {
        vertex.tangent = [1.0, 0.0, 0.0, w];
    }
    mesh.indices = vec![0, 2, 1, 0, 3, 2];
    mesh
}

const UNLIT: u8 = 20;

fn normal_texel(normal: [f32; 3]) -> TextureData {
    let encode = |x: f32| -> u8 {
        match x {
            x if x >= 1.0 => 255,
            x if x <= -1.0 => 0,
            _ => 128,
        }
    };
    TextureData::single(
        PixelFormat::Rgba8Unorm,
        1,
        1,
        vec![encode(normal[0]), encode(normal[1]), encode(normal[2]), 255],
    )
}

fn lit_scene(camera: Camera, light_travels: Vec3) -> Scene<'static> {
    let mut scene = Scene::new(camera);
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [
        DirectionalLight {
            direction: light_travels,
            color: [1.0, 1.0, 1.0],
        },
        DirectionalLight::default(),
        DirectionalLight::default(),
    ];
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene
}

#[test]
fn a_normal_map_turns_the_surface_towards_the_light() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&tangent_quad(1.0)).expect("upload");
    let map = renderer
        .upload_texture(&normal_texel([1.0, 0.0, 0.0]))
        .expect("upload");
    let scene = lit_scene(unit_camera(), Vec3::new(-1.0, 0.0, 0.0));

    let unmapped = [Instance::new(mesh)];
    renderer
        .render(&scene.clone().with_instances(&unmapped))
        .expect("render");
    let dark = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);

    let mapped = [Instance {
        normal_map: Some(map),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&scene.with_instances(&mapped))
        .expect("render");
    let bright = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);

    assert!(
        dark[0] < UNLIT,
        "a surface edge-on to the light must be unlit, got {dark:?}"
    );
    assert!(
        bright[0] > 240,
        "the normal map should have turned the surface into the light, got {bright:?}"
    );
}

#[test]
fn a_mesh_with_no_tangent_basis_ignores_its_normal_map() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let untangented = renderer.upload_mesh(&quad()).expect("upload");
    let tangented = renderer.upload_mesh(&tangent_quad(1.0)).expect("upload");
    let map = renderer
        .upload_texture(&normal_texel([1.0, 0.0, 0.0]))
        .expect("upload");
    let scene = lit_scene(unit_camera(), Vec3::new(-1.0, 0.0, 0.0));

    for (mesh, expected_bright) in [(untangented, false), (tangented, true)] {
        let instances = [Instance {
            normal_map: Some(map),
            ..Instance::new(mesh)
        }];
        renderer
            .render(&scene.clone().with_instances(&instances))
            .expect("render");
        let got = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);
        assert_eq!(
            got[0] > 240,
            expected_bright,
            "with a tangent basis: {expected_bright}, pixel {got:?}"
        );
    }
}

#[test]
fn handedness_decides_which_way_the_bitangent_points() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let map = renderer
        .upload_texture(&normal_texel([0.0, 1.0, 0.0]))
        .expect("upload");
    let scene = lit_scene(unit_camera(), Vec3::new(0.0, -1.0, 0.0));

    let mut lit = Vec::new();
    for w in [1.0f32, -1.0] {
        let mesh = renderer.upload_mesh(&tangent_quad(w)).expect("upload");
        let instances = [Instance {
            normal_map: Some(map),
            ..Instance::new(mesh)
        }];
        renderer
            .render(&scene.clone().with_instances(&instances))
            .expect("render");
        lit.push(pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32)[0]);
    }
    assert!(
        lit[0] > 240 && lit[1] < UNLIT,
        "handedness made no difference: {lit:?}"
    );
}

#[test]
fn a_normal_map_is_bound_per_instance_and_not_globally() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&tangent_quad(1.0)).expect("upload");
    let map = renderer
        .upload_texture(&normal_texel([1.0, 0.0, 0.0]))
        .expect("upload");

    let half = Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.0));
    let other = Mat4::from_translation(Vec3::new(0.5, 0.0, 0.0));
    let instances = [
        Instance {
            normal_map: Some(map),
            transform: half.mul_mat4(Mat4::from_scale(Vec3::splat(0.4))),
            ..Instance::new(mesh)
        },
        Instance {
            transform: other.mul_mat4(Mat4::from_scale(Vec3::splat(0.4))),
            ..Instance::new(mesh)
        },
    ];
    renderer
        .render(&lit_scene(unit_camera(), Vec3::new(-1.0, 0.0, 0.0)).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let mapped = pixel(&rgba, SIZE, SIZE / 4, SIZE / 2);
    let plain = pixel(&rgba, SIZE, 3 * SIZE / 4, SIZE / 2);
    assert!(
        mapped[0] > 240,
        "the mapped instance should be lit, got {mapped:?}"
    );
    assert!(
        plain[0] < UNLIT,
        "the unmapped instance in the same frame should not be, got {plain:?}"
    );
}

#[test]
fn a_uv_offset_moves_which_texel_is_sampled() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(
            &TextureData::rgba8(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            )
            .expect("2x2 RGBA"),
        )
        .expect("upload");

    let dominant = |p: [u8; 4]| -> usize {
        let mut best = 0;
        for i in 1..3 {
            if p[i] > p[best] {
                best = i;
            }
        }
        best
    };
    let quarter = SIZE / 4;

    let draw = |renderer: &mut Renderer, uv_offset: [f32; 2]| {
        let instances = [Instance {
            texture: Some(texture),
            uv_offset,
            ..Instance::new(mesh)
        }];
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, quarter, quarter)
    };

    let at_t0 = draw(&mut renderer, [0.0, 0.0]);
    assert_eq!(
        dominant(at_t0),
        0,
        "unshifted: red should dominate, got {at_t0:?}"
    );

    let at_t1 = draw(&mut renderer, [0.5, 0.0]);
    assert_eq!(
        dominant(at_t1),
        1,
        "shifted: green should dominate, got {at_t1:?}"
    );
    assert_ne!(
        at_t0, at_t1,
        "the same screen pixel must sample differently"
    );
}

#[test]
fn a_uv_linear_part_rotates_which_texel_is_sampled() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(
            &TextureData::rgba8(
                2,
                2,
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ],
            )
            .expect("2x2 RGBA"),
        )
        .expect("upload");
    let dominant = |p: [u8; 4]| -> usize {
        let mut best = 0;
        for i in 1..3 {
            if p[i] > p[best] {
                best = i;
            }
        }
        best
    };
    let three_quarters = SIZE - SIZE / 4;

    let quarter = SIZE / 4;
    let instances_plain = [Instance {
        texture: Some(texture),
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances_plain))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let unswapped = pixel(&rgba, SIZE, three_quarters, quarter);
    assert_eq!(
        dominant(unswapped),
        1,
        "top-right, unswapped: green, got {unswapped:?}"
    );

    let instances_swapped = [Instance {
        texture: Some(texture),
        uv_linear: [[0.0, 1.0], [1.0, 0.0]],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances_swapped))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let swapped = pixel(&rgba, SIZE, three_quarters, quarter);
    assert_eq!(
        dominant(swapped),
        2,
        "top-right, swapped: blue, got {swapped:?}"
    );
}

#[test]
fn material_emissive_shows_through_even_when_diffuse_and_ambient_are_zero() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        material_diffuse: [0.0, 0.0, 0.0],
        material_ambient: [0.0, 0.0, 0.0],
        material_emissive: [0.0, 1.0, 0.0],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 255, 0, 255],
        "zero diffuse and zero ambient must not erase a real emissive colour"
    );
}

#[test]
fn an_emissive_material_still_shows_its_texture() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let half_red = renderer
        .upload_texture(&solid_linear([128, 0, 0, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(half_red),
        material_diffuse: [0.0, 0.0, 0.0],
        material_ambient: [0.0, 0.0, 0.0],
        material_emissive: [1.0, 1.0, 1.0],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let got = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);

    assert!(
        (100..=160).contains(&got[0]) && got[1] < 40 && got[2] < 40,
        "an emissive material must modulate its texture, not replace it; got {got:?}"
    );
}

#[test]
fn zero_diffuse_and_ambient_with_no_emissive_is_black_not_the_bare_texture() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        material_diffuse: [0.0, 0.0, 0.0],
        material_ambient: [0.0, 0.0, 0.0],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [0, 0, 0, 255]);
}

#[test]
fn material_ambient_is_independent_of_material_diffuse() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        material_ambient: [1.0, 0.0, 0.0],
        material_diffuse: [0.0, 1.0, 0.0],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 0, 0, 255],
        "under full scene ambient, only material_ambient's red should show, not \
         material_diffuse's green"
    );
}

fn quad_with_color(color: [f32; 4]) -> MeshData {
    let mut mesh = quad();
    for vertex in &mut mesh.vertices {
        vertex.color = color;
    }
    mesh
}

#[test]
fn dcg_multiplies_result_scales_the_lit_colour_rather_than_replacing_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer
        .upload_mesh(&quad_with_color([1.0, 0.0, 1.0, 1.0]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        material_diffuse: [1.0, 1.0, 1.0],
        material_ambient: [0.0, 0.0, 0.0],
        material_emissive: [0.0, 0.0, 0.0],
        dcg_multiplies_result: true,
        ..Instance::new(mesh)
    }];
    renderer
        .render(&lit_scene(unit_camera(), Vec3::new(0.0, 0.0, 1.0)).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 0, 255, 255],
        "the fully-lit white material should be scaled by the vertex colour, \
         not replaced by it or ignored"
    );
}

#[test]
fn without_dcg_multiplies_result_the_vertex_colour_is_ignored_entirely() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer
        .upload_mesh(&quad_with_color([1.0, 0.0, 1.0, 1.0]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let instances = [Instance {
        texture: Some(white),
        material_diffuse: [1.0, 1.0, 1.0],
        material_ambient: [0.0, 0.0, 0.0],
        material_emissive: [0.0, 0.0, 0.0],
        ..Instance::new(mesh)
    }];
    renderer
        .render(&lit_scene(unit_camera(), Vec3::new(0.0, 0.0, 1.0)).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "with dcg_multiplies_result unset the vertex colour must not leak in"
    );
}

#[test]
fn dcg_multiplies_result_also_carries_vertex_alpha_into_the_cutout_test() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer
        .upload_mesh(&quad_with_color([1.0, 1.0, 1.0, 0.0]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let with_flag = [Instance {
        texture: Some(white),
        alpha_cutoff: 0.5,
        dcg_multiplies_result: true,
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&with_flag))
        .expect("render");
    let discarded = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&discarded, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "zero vertex alpha must fail the cutout when dcg_multiplies_result is set"
    );

    let without_flag = [Instance {
        texture: Some(white),
        alpha_cutoff: 0.5,
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&without_flag))
        .expect("render");
    let kept = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&kept, SIZE, SIZE / 2, SIZE / 2),
        [255, 255, 255, 255],
        "without the flag, vertex alpha must not reach the cutout test at all"
    );
}

fn summed_lights_scene(camera: Camera, count: usize) -> Scene<'static> {
    let mut scene = Scene::new(camera);
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let on = DirectionalLight {
        direction: Vec3::new(0.0, 0.0, 1.0),
        color: [1.0, 1.0, 1.0],
    };
    for (slot, light) in scene.lights.iter_mut().enumerate() {
        *light = if slot < count {
            on
        } else {
            DirectionalLight::default()
        };
    }
    scene
}

#[test]
fn the_lit_term_sums_past_1_0_and_is_clamped_at_lit_ceiling() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let grey = renderer
        .upload_texture(&solid_linear([102, 102, 102, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(grey),
        ..Instance::new(mesh)
    }];

    let render_with = |renderer: &mut Renderer, count: usize| -> u8 {
        renderer
            .render(&summed_lights_scene(unit_camera(), count).with_instances(&instances))
            .expect("render");
        pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32)[0]
    };

    let one = render_with(&mut renderer, 1);
    let two = render_with(&mut renderer, 2);
    let three = render_with(&mut renderer, 3);

    assert!(
        (98..=106).contains(&one),
        "one light, lit = 1.0: 0.4 * 1.0 -> ~102, got {one}"
    );
    assert!(
        (198..=210).contains(&two),
        "two lights, lit = 2.0 (not clamped yet): 0.4 * 2.0 -> ~204, got {two}"
    );
    assert_eq!(
        three, two,
        "a third light must not brighten the result any further: lightsum = 3.0 \
         is clamped to lit_ceiling = 2.0, same as the two-light case - got \
         two={two}, three={three}"
    );
}

#[test]
fn a_light_with_a_zero_direction_contributes_nothing() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    }];

    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.lights = [
        DirectionalLight {
            direction: Vec3::ZERO,
            color: [1.0, 1.0, 1.0],
        },
        DirectionalLight::default(),
        DirectionalLight::default(),
    ];
    renderer
        .render(&scene.with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "a zero-direction light must contribute nothing, whatever its colour"
    );
}

#[test]
fn vertex_alpha_outside_the_unit_range_saturates_instead_of_amplifying() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let handle = renderer
        .upload_mesh(&quad_with_color([1.0, 1.0, 1.0, -1.0]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("upload");

    let grey_scene = || {
        let mut scene = flat_scene(unit_camera());
        scene.clear_color = [0.25, 0.25, 0.25, 1.0];
        scene
    };
    let instances = [Instance {
        texture: Some(white),
        blend: Blend::AlphaBlend,
        vertex_alpha: true,
        ..Instance::new(handle)
    }];
    renderer
        .render(&grey_scene().with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let covered = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    let clear = pixel(&rgba, SIZE, 0, 0);
    assert_eq!(
        covered, clear,
        "a coverage of -1.0 must leave the destination alone, not weight it by two"
    );

    let opaque_ramp = renderer
        .upload_mesh(&quad_with_color([1.0, 1.0, 1.0, 2.0]))
        .expect("upload");
    let full = renderer
        .upload_mesh(&quad_with_color([1.0, 1.0, 1.0, 1.0]))
        .expect("upload");
    let mut rendered = Vec::new();
    for mesh in [opaque_ramp, full] {
        let instances = [Instance {
            texture: Some(white),
            blend: Blend::AlphaBlend,
            vertex_alpha: true,
            ..Instance::new(mesh)
        }];
        renderer
            .render(&grey_scene().with_instances(&instances))
            .expect("render");
        rendered.push(renderer.read_pixels().expect("readback"));
    }
    assert_eq!(
        rendered
            .first()
            .map(|rgba| pixel(rgba, SIZE, SIZE / 2, SIZE / 2)),
        rendered
            .get(1)
            .map(|rgba| pixel(rgba, SIZE, SIZE / 2, SIZE / 2)),
        "a coverage of 2.0 must draw exactly what 1.0 draws"
    );
}

#[test]
fn four_instances_of_one_mesh_and_material_draw_in_one_call_and_keep_their_own_slots() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");

    let place = |x: f32, y: f32| {
        Mat4::from_translation(Vec3::new(x, y, 0.0)).mul_mat4(Mat4::from_scale(Vec3::splat(0.5)))
    };
    let tinted = |x: f32, y: f32, tint: [f32; 4]| Instance {
        transform: place(x, y),
        tint,
        ..Instance::new(mesh)
    };
    let instances = [
        tinted(-0.5, 0.5, [1.0, 0.0, 0.0, 1.0]),
        tinted(0.5, 0.5, [0.0, 1.0, 0.0, 1.0]),
        tinted(-0.5, -0.5, [0.0, 0.0, 1.0, 1.0]),
        tinted(0.5, -0.5, [1.0, 1.0, 1.0, 1.0]),
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(renderer.draw_calls(), 1, "all four share mesh and material");
    assert_eq!(renderer.drawn_instances(), 4);

    let quarter = SIZE / 4;
    let three_quarters = SIZE - quarter;
    assert_eq!(pixel(&rgba, SIZE, quarter, quarter), [255, 0, 0, 255]);
    assert_eq!(
        pixel(&rgba, SIZE, three_quarters, quarter),
        [0, 255, 0, 255]
    );
    assert_eq!(
        pixel(&rgba, SIZE, quarter, three_quarters),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(&rgba, SIZE, three_quarters, three_quarters),
        [255, 255, 255, 255]
    );
}

#[test]
fn a_material_change_between_two_instances_is_a_second_draw_call() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let green = renderer
        .upload_texture(&solid_linear([0, 255, 0, 255]))
        .expect("upload");

    let place = |x: f32| {
        Mat4::from_translation(Vec3::new(x, 0.0, 0.0)).mul_mat4(Mat4::from_scale(Vec3::splat(0.5)))
    };
    let instances = [
        Instance {
            texture: Some(red),
            transform: place(-0.5),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(green),
            transform: place(0.5),
            ..Instance::new(mesh)
        },
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        renderer.draw_calls(),
        2,
        "two materials cannot share a call"
    );
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 4, SIZE / 2),
        [255, 0, 0, 255],
        "the left quad keeps its own texture"
    );
    assert_eq!(
        pixel(&rgba, SIZE, SIZE - SIZE / 4, SIZE / 2),
        [0, 255, 0, 255],
        "the right quad keeps its own texture"
    );
}

#[test]
fn batching_does_not_let_a_blended_instance_join_the_opaque_run() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");

    let at = |z: f32, blend: Blend, tint: [f32; 4]| Instance {
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, z))
            .mul_mat4(Mat4::from_scale(Vec3::splat(0.5))),
        blend,
        tint,
        ..Instance::new(mesh)
    };
    let instances = [
        at(-1.0, Blend::Opaque, [0.0, 0.0, 1.0, 1.0]),
        at(-0.5, Blend::AlphaBlend, [1.0, 0.0, 0.0, 0.5]),
        at(-2.0, Blend::Opaque, [0.0, 1.0, 0.0, 1.0]),
        at(-0.2, Blend::AlphaBlend, [1.0, 1.0, 1.0, 0.0]),
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        renderer.draw_calls(),
        2,
        "one call for the opaque run, one for the blended run"
    );
    assert_eq!(renderer.drawn_instances(), 4);
    let centre = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    assert!(
        centre[0] > centre[1] && centre[2] > 0,
        "the half-transparent red quad must still composite over the blue \
         opaque one behind it, giving a red-dominant purple: got {centre:?}"
    );
}

const TINT_SHADER: &str = "
struct Tint { color: vec4<f32> };
@group(1) @binding(0) var<uniform> tint: Tint;
@group(1) @binding(1) var albedo: texture_2d<f32>;
@group(1) @binding(2) var albedo_sampler: sampler;

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    out.instance = index;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return textureSample(albedo, albedo_sampler, in.uv) * tint.color;
}
";

fn tint_shader(writes_alpha: bool) -> CustomShader<'static> {
    CustomShader {
        label: "test tint",
        source: TINT_SHADER,
        texture_slots: 1,
        uniform_size: 16,
        writes_alpha,
        ..CustomShader::default()
    }
}

const SCENE_SHADER: &str = "
@group(1) @binding(0) var albedo: texture_2d<f32>;
@group(1) @binding(1) var albedo_sampler: sampler;

struct SceneBlock { color: vec4<f32> };
@group(3) @binding(0) var<uniform> scene: SceneBlock;
@group(3) @binding(1) var shared_texture: texture_2d<f32>;
@group(3) @binding(2) var shared_sampler: sampler;

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return textureSample(albedo, albedo_sampler, in.uv)
         * textureSample(shared_texture, shared_sampler, in.uv)
         * scene.color;
}
";

fn scene_shader() -> CustomShader<'static> {
    CustomShader {
        label: "test scene binding",
        source: SCENE_SHADER,
        material: CustomMaterialSource::Builtin,
        scene_uniform_size: 16,
        scene_texture_slots: 1,
        ..CustomShader::default()
    }
}

#[test]
fn a_shader_taking_mints_material_draws_per_instance_textures_off_one_scene_block() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let green = renderer
        .upload_texture(&solid_linear([0, 255, 0, 255]))
        .expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&scene_shader())
        .expect("the test shader compiles");
    let mut block = Vec::new();
    for value in [1.0f32, 1.0, 1.0, 1.0] {
        block.extend_from_slice(&value.to_le_bytes());
    }
    renderer
        .set_custom_scene(shader, &[Some(white)], &block)
        .expect("scene binding");

    for (texture, expected) in [(red, [255, 0, 0, 255]), (green, [0, 255, 0, 255])] {
        let instance = Instance {
            custom_shader: Some(shader),
            texture: Some(texture),
            ..Instance::new(mesh)
        };
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&[instance]))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), expected);
    }

    let mut block = Vec::new();
    for value in [0.0f32, 1.0, 1.0, 1.0] {
        block.extend_from_slice(&value.to_le_bytes());
    }
    renderer
        .update_custom_scene(shader, &block)
        .expect("update");
    let instance = Instance {
        custom_shader: Some(shader),
        texture: Some(red),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [0, 0, 0, 255]);

    renderer
        .set_custom_scene(shader, &[Some(green)], &block)
        .expect("scene binding");
    let instance = Instance {
        custom_shader: Some(shader),
        texture: Some(white),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [0, 255, 0, 255]);
}

#[test]
fn the_two_material_sources_cannot_be_mixed_up() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let scene = renderer
        .register_custom_shader(&scene_shader())
        .expect("the test shader compiles");
    let tint = renderer
        .register_custom_shader(&tint_shader(true))
        .expect("the test shader compiles");
    let material = renderer
        .create_custom_material(tint, &[None], &[0u8; 16])
        .expect("material");

    assert!(matches!(
        renderer.create_custom_material(scene, &[], &[]),
        Err(Error::CustomMaterialSourceMismatch { .. })
    ));
    let stray = Instance {
        custom_shader: Some(tint),
        ..Instance::new(mesh)
    };
    assert!(matches!(
        renderer.render(&flat_scene(unit_camera()).with_instances(&[stray])),
        Err(Error::CustomMaterialSourceMismatch { .. })
    ));
    let both = Instance {
        custom_shader: Some(scene),
        custom_material: Some(material),
        ..Instance::new(mesh)
    };
    assert!(matches!(
        renderer.render(&flat_scene(unit_camera()).with_instances(&[both])),
        Err(Error::CustomMaterialSourceMismatch { .. })
    ));
    let confused = CustomShader {
        label: "confused",
        source: SCENE_SHADER,
        material: CustomMaterialSource::Builtin,
        texture_slots: 1,
        ..CustomShader::default()
    };
    assert!(matches!(
        renderer.register_custom_shader(&confused),
        Err(Error::CustomMaterialSourceMismatch { .. })
    ));
}

#[test]
fn a_scene_binding_with_the_wrong_texture_or_uniform_count_is_refused() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let shader = renderer
        .register_custom_shader(&scene_shader())
        .expect("the test shader compiles");
    assert!(matches!(
        renderer.set_custom_scene(shader, &[], &[0u8; 16]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
    assert!(matches!(
        renderer.set_custom_scene(shader, &[None], &[0u8; 12]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
    assert!(matches!(
        renderer.update_custom_scene(shader, &[0u8; 20]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
}

#[test]
fn a_scene_binding_is_bindable_before_the_caller_has_filled_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let shader = renderer
        .register_custom_shader(&scene_shader())
        .expect("the test shader compiles");
    let instance = Instance {
        custom_shader: Some(shader),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [0, 0, 0, 0]);
}

#[test]
fn a_registered_shader_draws_with_its_own_uniform_block_and_texture() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&tint_shader(true))
        .expect("the test shader compiles");
    let half = 0.5f32.to_le_bytes();
    let one = 1.0f32.to_le_bytes();
    let mut block = Vec::new();
    for value in [half, one, half, one] {
        block.extend_from_slice(&value);
    }
    let material = renderer
        .create_custom_material(shader, &[Some(texture)], &block)
        .expect("material");

    let instance = Instance {
        custom_material: Some(material),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let centre = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    assert!(centre[0] > 100 && centre[0] < 155, "{centre:?}");
    assert_eq!(centre[1], 255, "{centre:?}");
    assert!(centre[2] > 100 && centre[2] < 155, "{centre:?}");

    let mut block = Vec::new();
    for value in [one, [0u8; 4], [0u8; 4], one] {
        block.extend_from_slice(&value);
    }
    renderer
        .update_custom_material(material, &block)
        .expect("update");
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [255, 0, 0, 255]);
}

#[test]
fn a_shader_that_does_not_write_alpha_leaves_the_targets_alpha_alone() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let texture = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&tint_shader(false))
        .expect("the test shader compiles");
    let mut block = Vec::new();
    for value in [1.0f32, 1.0, 1.0, 0.5] {
        block.extend_from_slice(&value.to_le_bytes());
    }
    let material = renderer
        .create_custom_material(shader, &[Some(texture)], &block)
        .expect("material");

    let instance = Instance {
        custom_material: Some(material),
        blend: Blend::AlphaBlend,
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let centre = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    assert_eq!(centre[3], 255, "alpha was written: {centre:?}");
    assert!(centre[0] > 100 && centre[0] < 155, "{centre:?}");
}

#[test]
fn a_custom_material_with_the_wrong_texture_or_uniform_count_is_refused() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let shader = renderer
        .register_custom_shader(&tint_shader(true))
        .expect("the test shader compiles");
    assert!(matches!(
        renderer.create_custom_material(shader, &[], &[0u8; 16]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
    assert!(matches!(
        renderer.create_custom_material(shader, &[None], &[0u8; 12]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
    let material = renderer
        .create_custom_material(shader, &[None], &[0u8; 16])
        .expect("material");
    assert!(matches!(
        renderer.update_custom_material(material, &[0u8; 20]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
}

#[test]
fn a_shader_that_does_not_compile_is_an_error_rather_than_a_panic() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let broken = CustomShader {
        label: "broken",
        source: "@fragment fn fs_main() -> @location(0) vec4<f32> { return nonsense; }",
        ..CustomShader::default()
    };
    assert!(matches!(
        renderer.register_custom_shader(&broken),
        Err(Error::CustomShaderRejected { .. })
    ));
}

fn box_at(centre: Vec3, half: f32) -> Aabb {
    Aabb::from_corners(
        Vec3::new(centre.x - half, centre.y - half, centre.z - half),
        Vec3::new(centre.x + half, centre.y + half, centre.z + half),
    )
}

#[test]
fn each_pass_culls_against_its_own_matrix_and_not_the_frames() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    assert!(
        renderer.frustum_culling(),
        "culling is on by default; the rest of this test assumes it"
    );
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(64).expect("shadow map");
    let target = renderer.create_render_target(SIZE, SIZE).expect("target");

    let at = |x: f32, z: f32| Instance {
        transform: Mat4::from_translation(Vec3::new(x, 0.0, z)),
        bounds: Some(box_at(Vec3::ZERO, 0.3)),
        ..Instance::new(mesh)
    };
    let instances = [at(0.0, 0.0), at(-0.6, 5.0), at(0.0, 5.0), at(0.6, 5.0)];
    let scene = flat_scene(unit_camera()).with_instances(&instances);

    let light = directional_light_view_proj(
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(0.0, 0.0, 5.0),
        1.5,
        20.0,
    );
    let mirrored = Camera {
        eye: Vec3::new(0.0, 0.0, 9.0),
        target: Vec3::new(0.0, 0.0, 5.0),
        up: Vec3::Y,
        fov_y: std::f32::consts::FRAC_PI_2,
        near: 0.05,
        far: 6.0,
    };
    let target_scene = flat_scene(mirrored).with_instances(&instances);

    renderer.render(&scene).expect("render");
    assert_eq!(
        (renderer.drawn_instances(), renderer.culled_instances()),
        (1, 3),
        "the frame sees only the instance in front of the camera"
    );

    renderer
        .render_shadow_map(map, light, &scene)
        .expect("shadow pass");
    assert_eq!(
        (
            renderer.shadow_drawn_instances(),
            renderer.shadow_culled_instances()
        ),
        (3, 1),
        "the depth pass must cull against the light: a caster behind the \
         camera still shadows what the camera can see"
    );

    renderer
        .render_to_target(target, &target_scene)
        .expect("target pass");
    assert_eq!(
        (
            renderer.target_drawn_instances(),
            renderer.target_culled_instances()
        ),
        (3, 1),
        "a render target culls against the camera it is drawn with, not the \
         frame's - a planar reflection must draw what the mirror sees"
    );

    renderer.set_frustum_culling(false);
    renderer.render(&scene).expect("render");
    renderer
        .render_shadow_map(map, light, &scene)
        .expect("shadow pass");
    renderer
        .render_to_target(target, &target_scene)
        .expect("target pass");
    assert_eq!(
        (
            renderer.drawn_instances(),
            renderer.shadow_drawn_instances(),
            renderer.target_drawn_instances()
        ),
        (4, 4, 4)
    );
    assert_eq!(
        (
            renderer.culled_instances(),
            renderer.shadow_culled_instances(),
            renderer.target_culled_instances()
        ),
        (0, 0, 0)
    );
}

#[test]
fn an_instance_with_no_bounds_is_never_culled_by_any_pass() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(64).expect("shadow map");
    let light = directional_light_view_proj(Vec3::new(0.0, -1.0, 0.0), Vec3::ZERO, 2.0, 20.0);

    let far_away = Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0));

    let bounded = Instance {
        transform: far_away,
        bounds: Some(box_at(Vec3::ZERO, 1.0)),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[bounded]))
        .expect("render");
    assert_eq!(
        (renderer.drawn_instances(), renderer.culled_instances()),
        (0, 1)
    );
    renderer
        .render_shadow_map(
            map,
            light,
            &flat_scene(unit_camera()).with_instances(&[bounded]),
        )
        .expect("shadow pass");
    assert_eq!(
        (
            renderer.shadow_drawn_instances(),
            renderer.shadow_culled_instances()
        ),
        (0, 1)
    );

    let unbounded = Instance {
        bounds: None,
        ..bounded
    };
    assert!(
        Instance::new(mesh).bounds.is_none(),
        "and `None` is the default, so a caller opts in rather than out"
    );
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[unbounded]))
        .expect("render");
    assert_eq!(
        (renderer.drawn_instances(), renderer.culled_instances()),
        (1, 0)
    );
    renderer
        .render_shadow_map(
            map,
            light,
            &flat_scene(unit_camera()).with_instances(&[unbounded]),
        )
        .expect("shadow pass");
    assert_eq!(
        (
            renderer.shadow_drawn_instances(),
            renderer.shadow_culled_instances()
        ),
        (1, 0)
    );
}

const TEXTURE_ONLY_SHADER: &str = "
@group(1) @binding(1) var albedo: texture_2d<f32>;
@group(1) @binding(2) var albedo_sampler: sampler;

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return textureSample(albedo, albedo_sampler, in.uv);
}
";

#[test]
fn a_shader_that_declares_no_uniform_block_of_its_own_is_still_drawable() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&CustomShader {
            label: "no block of its own",
            source: TEXTURE_ONLY_SHADER,
            texture_slots: 1,
            uniform_size: 0,
            ..CustomShader::default()
        })
        .expect("a shader with no uniform block must compile and link");
    let material = renderer
        .create_custom_material(shader, &[Some(red)], &[])
        .expect("and must take a material with an empty block");

    let instance = Instance {
        custom_material: Some(material),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[instance]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [255, 0, 0, 255]);

    assert!(matches!(
        renderer.create_custom_material(shader, &[Some(red)], &[0u8; 16]),
        Err(Error::CustomMaterialMismatch { .. })
    ));
}

const SNAPSHOT_SHADER: &str = "
struct SceneBlock { color: vec4<f32> };
@group(3) @binding(0) var<uniform> scene_block: SceneBlock;

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    out.instance = index;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let tint = instances[in.instance].tint;
    if (tint.a > 0.5) {
        return vec4<f32>(tint.rgb * scene_block.color.rgb, 1.0);
    }
    let snapshot = textureLoad(
        mint_scene_color,
        vec2<i32>(i32(in.clip_position.x), i32(in.clip_position.y)),
        0
    );
    return vec4<f32>(snapshot.rgb, 1.0);
}
";

fn snapshot_shader() -> CustomShader<'static> {
    CustomShader {
        label: "test snapshot",
        source: SNAPSHOT_SHADER,
        material: CustomMaterialSource::Builtin,
        scene_color: true,
        scene_uniform_size: 16,
        ..CustomShader::default()
    }
}

const DEPTH_READER_SHADER: &str = "
struct Out { @builtin(position) clip_position: vec4<f32> };

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    let d = textureLoad(
        mint_scene_depth,
        vec2<i32>(i32(in.clip_position.x), i32(in.clip_position.y)),
        0
    );
    return vec4<f32>(d, 0.0, 0.0, 1.0);
}
";

fn plate(mesh: mint_render::MeshHandle, z: f32, scale: f32, blend: Blend) -> Instance {
    Instance {
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, z))
            .mul_mat4(Mat4::from_scale(Vec3::splat(scale))),
        blend,
        ..Instance::new(mesh)
    }
}

#[test]
fn a_surface_reading_the_snapshot_cannot_see_another_surface_reading_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&snapshot_shader())
        .expect("the test shader compiles");
    let mut block = Vec::new();
    for value in [0.0f32, 1.0, 0.0, 1.0] {
        block.extend_from_slice(&value.to_le_bytes());
    }
    renderer
        .set_custom_scene(shader, &[], &block)
        .expect("scene binding");

    let ground = Instance {
        texture: Some(red),
        ..plate(mesh, -0.9, 1.0, Blend::Opaque)
    };
    let painter = Instance {
        custom_material: None,
        custom_shader: Some(shader),
        tint: [1.0, 1.0, 1.0, 1.0],
        ..plate(mesh, -0.4, 0.5, Blend::AlphaBlend)
    };
    let reader = Instance {
        custom_shader: Some(shader),
        tint: [1.0, 1.0, 1.0, 0.0],
        ..plate(mesh, 0.4, 0.5, Blend::AlphaBlend)
    };

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[ground, painter]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 255, 0, 255],
        "the painter draws green, and its scene block at group 3 is what \
         makes it green even though group 2 is also bound"
    );

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[ground, painter, reader]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [255, 0, 0, 255],
        "the reader must see the opaque half - red - and not the green the \
         other consumer laid down before it in the same pass"
    );
}

#[test]
fn a_blended_surface_that_reads_nothing_is_inside_the_snapshot() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let green = renderer
        .upload_texture(&solid_linear([0, 255, 0, 255]))
        .expect("upload");
    let shader = renderer
        .register_custom_shader(&snapshot_shader())
        .expect("the test shader compiles");
    renderer
        .set_custom_scene(shader, &[], &[0u8; 16])
        .expect("scene binding");

    let ground = Instance {
        texture: Some(red),
        ..plate(mesh, -0.9, 1.0, Blend::Opaque)
    };
    let decal = Instance {
        texture: Some(green),
        ..plate(mesh, -0.4, 0.5, Blend::AlphaBlend)
    };
    let reader = Instance {
        custom_shader: Some(shader),
        tint: [1.0, 1.0, 1.0, 0.0],
        ..plate(mesh, 0.4, 0.5, Blend::AlphaBlend)
    };

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[ground, decal, reader]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 255, 0, 255],
        "the snapshot must contain the decal drawn over the opaque half"
    );

    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[ground, decal]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, SIZE / 2, SIZE / 2), [0, 255, 0, 255]);
}

#[test]
fn a_shader_drawn_after_the_opaque_pass_reads_the_depth_that_pass_wrote() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let shader = renderer
        .register_custom_shader(&CustomShader {
            label: "test depth reader",
            source: DEPTH_READER_SHADER,
            material: CustomMaterialSource::Builtin,
            scene_depth: true,
            ..CustomShader::default()
        })
        .expect("the test shader compiles");

    let ground = Instance {
        transform: Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.0))
            .mul_mat4(Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0))),
        ..Instance::new(mesh)
    };
    let reader = Instance {
        custom_shader: Some(shader),
        ..plate(mesh, 0.5, 1.0, Blend::AlphaBlend)
    };
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&[ground, reader]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let covered = pixel(&rgba, SIZE, SIZE / 4, SIZE / 2);
    let empty = pixel(&rgba, SIZE, SIZE - SIZE / 4, SIZE / 2);
    assert_eq!(
        empty[0], 0,
        "where the opaque pass drew nothing the depth is still the clear \
         value: got {empty:?}"
    );
    assert!(
        covered[0] > 0,
        "where it drew, the reader must see that depth rather than the clear \
         value: got {covered:?}"
    );
}

#[test]
fn a_shader_reading_the_frame_is_refused_where_it_cannot_be_drawn() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let target = renderer.create_render_target(SIZE, SIZE).expect("target");
    let colour = renderer
        .register_custom_shader(&snapshot_shader())
        .expect("the test shader compiles");
    renderer
        .set_custom_scene(colour, &[], &[0u8; 16])
        .expect("scene binding");
    let depth = renderer
        .register_custom_shader(&CustomShader {
            label: "test depth reader",
            source: DEPTH_READER_SHADER,
            material: CustomMaterialSource::Builtin,
            scene_depth: true,
            ..CustomShader::default()
        })
        .expect("the test shader compiles");

    for (shader, label) in [(colour, "test snapshot"), (depth, "test depth reader")] {
        let opaque = Instance {
            custom_shader: Some(shader),
            tint: [1.0, 1.0, 1.0, 0.0],
            ..plate(mesh, 0.0, 1.0, Blend::Opaque)
        };
        match renderer.render(&flat_scene(unit_camera()).with_instances(&[opaque])) {
            Err(Error::SceneDepthWritesDepth { label: named }) => {
                assert_eq!(named, label, "the error must name the shader");
            }
            other => panic!("expected SceneDepthWritesDepth for {label}, got {other:?}"),
        }

        let blended = Instance {
            custom_shader: Some(shader),
            tint: [1.0, 1.0, 1.0, 0.0],
            ..plate(mesh, 0.0, 1.0, Blend::AlphaBlend)
        };
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&[blended]))
            .expect("blended is the pass it belongs in");

        match renderer.render_to_target(
            target,
            &flat_scene(unit_camera()).with_instances(&[blended]),
        ) {
            Err(Error::FrameInputsOffTarget { label: named }) => {
                assert_eq!(named, label, "the error must name the shader");
            }
            other => panic!("expected FrameInputsOffTarget for {label}, got {other:?}"),
        }
    }
}

#[test]
fn an_alpha_tested_caster_casts_its_cutout_and_not_its_whole_quad() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let cutout = renderer
        .upload_texture(
            &TextureData::rgba8(2, 1, vec![255, 255, 255, 0, 255, 255, 255, 255])
                .expect("2x1 RGBA is eight bytes"),
        )
        .expect("texture");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");

    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 2.0, 10.0);

    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let caster = |cutoff: f32, u_offset: f32| Instance {
        texture: Some(cutout),
        alpha_cutoff: cutoff,
        uv_offset: [u_offset, 0.0],
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };

    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: towards_the_eye,
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.shadow = Some(ShadowSample {
        lightness: 0.0,
        edge: ShadowEdge::Taps(1.0),
        ..ShadowSample::single(map, light, 0.002)
    });
    let receivers = [receiver];
    let frame = scene.with_instances(&receivers);

    let read = |renderer: &mut Renderer, cutoff: f32, u_offset: f32| -> ([u8; 4], [u8; 4]) {
        let casters = [caster(cutoff, u_offset)];
        renderer
            .render_shadow_map(
                map,
                light,
                &Scene::new(unit_camera()).with_instances(&casters),
            )
            .expect("shadow pass");
        renderer.render(&frame).expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        (
            pixel(&rgba, SIZE, 26, SIZE / 2),
            pixel(&rgba, SIZE, 37, SIZE / 2),
        )
    };

    let (opaque_hole, opaque_solid) = read(&mut renderer, 0.0, 0.0);
    assert_eq!(
        renderer.shadow_alpha_instances(),
        (0, 0),
        "a caster with no cutoff must stay on the material-free path"
    );
    assert!(
        opaque_hole[0] < 32 && opaque_solid[0] < 32,
        "with no alpha test the whole quad shadows: {opaque_hole:?} {opaque_solid:?}"
    );

    let (cut_hole, cut_solid) = read(&mut renderer, 0.5, 0.0);
    assert_eq!(
        renderer.shadow_alpha_instances(),
        (1, 1),
        "a caster with a cutoff and a texture goes through the alpha pipeline"
    );
    assert!(
        cut_hole[0] > 200,
        "the transparent half of the caster must not shadow: {cut_hole:?}"
    );
    assert!(
        cut_solid[0] < 32,
        "the opaque half of the caster still must: {cut_solid:?}"
    );

    let (rolled_hole, rolled_solid) = read(&mut renderer, 0.5, 0.5);
    assert!(
        rolled_hole[0] < 32,
        "half a repeat should put the opaque texel here: {rolled_hole:?}"
    );
    assert!(
        rolled_solid[0] > 200,
        "and the transparent one here: {rolled_solid:?}"
    );
}

#[test]
fn the_alpha_half_of_the_shadow_pass_reads_its_own_transforms() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let cutout = renderer
        .upload_texture(
            &TextureData::rgba8(2, 1, vec![255, 255, 255, 0, 255, 255, 255, 255])
                .expect("2x1 RGBA is eight bytes"),
        )
        .expect("texture");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 4.0, 12.0);

    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let plain = Instance {
        transform: Mat4::from_translation(Vec3::new(-2.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };
    let cut = Instance {
        texture: Some(cutout),
        alpha_cutoff: 0.5,
        transform: Mat4::from_translation(Vec3::new(2.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };

    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: towards_the_eye,
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.shadow = Some(ShadowSample {
        lightness: 0.0,
        edge: ShadowEdge::Taps(1.0),
        ..ShadowSample::single(map, light, 0.002)
    });
    let receivers = [receiver];
    let casters = [plain, cut];
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&casters),
        )
        .expect("shadow pass");
    assert_eq!(
        (
            renderer.shadow_drawn_instances(),
            renderer.shadow_alpha_instances()
        ),
        (2, (1, 1)),
        "one caster on each path"
    );
    renderer
        .render(&scene.with_instances(&receivers))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let at = |x: u32| pixel(&rgba, SIZE, x, SIZE / 2);
    assert!(
        at(10)[0] < 32,
        "the untextured caster shadows: {:?}",
        at(10)
    );
    assert!(
        at(48)[0] > 200,
        "the cut caster's hole does not: {:?}",
        at(48)
    );
    assert!(
        at(58)[0] < 32,
        "the cut caster's opaque half does, and does it where the caster \
         actually is: {:?}",
        at(58)
    );
}

#[test]
fn a_cutoff_without_a_texture_does_not_pay_for_the_alpha_pipeline() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(64).expect("shadow map");
    let light = directional_light_view_proj(Vec3::new(0.0, 0.0, -1.0), Vec3::ZERO, 2.0, 10.0);
    let untextured = Instance {
        alpha_cutoff: 0.5,
        ..Instance::new(mesh)
    };
    let instances = [untextured];
    renderer
        .render_shadow_map(
            map,
            light,
            &flat_scene(unit_camera()).with_instances(&instances),
        )
        .expect("shadow pass");
    assert_eq!(renderer.shadow_drawn_instances(), 1);
    assert_eq!(renderer.shadow_alpha_instances(), (0, 0));
}

#[test]
fn turning_the_shadow_alpha_test_off_shadows_the_whole_quad_again() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let cutout = renderer
        .upload_texture(
            &TextureData::rgba8(2, 1, vec![255, 255, 255, 0, 255, 255, 255, 255])
                .expect("2x1 RGBA is eight bytes"),
        )
        .expect("texture");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 2.0, 10.0);
    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let caster = Instance {
        texture: Some(cutout),
        alpha_cutoff: 0.5,
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };

    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: towards_the_eye,
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.shadow = Some(ShadowSample {
        lightness: 0.0,
        edge: ShadowEdge::Taps(1.0),
        ..ShadowSample::single(map, light, 0.002)
    });
    let receivers = [receiver];
    let casters = [caster];
    let frame = scene.with_instances(&receivers);

    assert!(
        renderer.shadow_alpha_test(),
        "the alpha test is on by default"
    );
    renderer.set_shadow_alpha_test(false);
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&casters),
        )
        .expect("shadow pass");
    assert_eq!(
        renderer.shadow_alpha_instances(),
        (0, 0),
        "nothing may reach the alpha pipeline while the rung is off"
    );
    assert_eq!(
        renderer.shadow_drawn_instances(),
        1,
        "and the caster must still be drawn, not dropped"
    );
    renderer.render(&frame).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let hole = pixel(&rgba, SIZE, 26, SIZE / 2);
    assert!(
        hole[0] < 32,
        "off, the transparent half occludes as a solid rectangle: {hole:?}"
    );

    renderer.set_shadow_alpha_test(true);
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&casters),
        )
        .expect("shadow pass");
    renderer.render(&frame).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let hole = pixel(&rgba, SIZE, 26, SIZE / 2);
    assert!(
        hole[0] > 200,
        "and back on, the same pixel is lit again: {hole:?}"
    );
}

#[test]
fn a_shader_can_shadow_to_its_own_floor_instead_of_the_scenes() {
    const FLOOR_SHADER: &str = "
struct FloorBlock { floor: vec4<f32> };
@group(3) @binding(0) var<uniform> shadow_floor: FloorBlock;

struct Out {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(flat) instance: u32,
};

@vertex
fn vs_main(v: MintVertex, @builtin(instance_index) index: u32) -> Out {
    slot = index;
    var out: Out;
    let world = instances[index].model * vec4<f32>(v.position, 1.0);
    out.clip_position = frame.view_proj * world;
    out.world = world.xyz;
    out.instance = index;
    return out;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    slot = in.instance;
    let shade = mix(shadow_floor.floor.x, 1.0, mint_shadow_visibility(in.world));
    return vec4<f32>(shade, shade, shade, 1.0);
}
";
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let shader = renderer
        .register_custom_shader(&CustomShader {
            label: "test shadow floor",
            source: FLOOR_SHADER,
            material: CustomMaterialSource::Builtin,
            scene_uniform_size: 16,
            ..CustomShader::default()
        })
        .expect("the test shader compiles");

    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 4.0, 12.0);
    let receiver = Instance {
        custom_shader: Some(shader),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let caster = Instance {
        transform: Mat4::from_translation(Vec3::new(-2.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };
    let casters = [caster];
    let receivers = [receiver];

    let mut shaded = Vec::new();
    for floor in [0.0f32, 0.5] {
        let mut block = Vec::new();
        for value in [floor, 0.0, 0.0, 0.0] {
            block.extend_from_slice(&value.to_le_bytes());
        }
        renderer
            .set_custom_scene(shader, &[], &block)
            .expect("scene binding");
        renderer
            .render_shadow_map(
                map,
                light,
                &Scene::new(unit_camera()).with_instances(&casters),
            )
            .expect("shadow pass");
        let mut scene = Scene::new(unit_camera());
        scene.clear_color = [0.0, 0.0, 0.0, 1.0];
        scene.shadow = Some(ShadowSample {
            lightness: 0.9,
            edge: ShadowEdge::Taps(1.0),
            ..ShadowSample::single(map, light, 0.002)
        });
        renderer
            .render(&scene.with_instances(&receivers))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        shaded.push((
            pixel(&rgba, SIZE, 10, SIZE / 2)[0],
            pixel(&rgba, SIZE, 48, SIZE / 2)[0],
        ));
    }

    let [(dark_shadow, dark_lit), (mid_shadow, mid_lit)] = shaded[..] else {
        panic!("two runs");
    };
    assert!(
        dark_shadow < 32,
        "a floor of 0 is black in shadow whatever the scene's 0.9 says - so the \
         lookup really is unweighted: {dark_shadow}"
    );
    assert!(
        (96..=160).contains(&mid_shadow),
        "a floor of 0.5 is half lit - the target is linear, so 0.5 reads 127 -          and it is the shader's own number and not the scene's 0.9: {mid_shadow}"
    );
    assert!(
        dark_lit > 200 && mid_lit > 200,
        "and neither floor touches a pixel the caster does not reach: \
         {dark_lit}, {mid_lit}"
    );
}

#[test]
fn a_height_shear_leans_geometry_further_the_higher_it_is() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&upright_quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let high = (48, 22);
    let low = (48, 42);

    let plain = Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(upright_camera()).with_instances(&[plain]))
        .expect("render");
    let before = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&before, SIZE, high.0, high.1),
        [0, 0, 0, 255],
        "nothing is drawn to the right of the upright quad before the shear"
    );
    assert_eq!(pixel(&before, SIZE, low.0, low.1), [0, 0, 0, 255]);

    let leaning = Instance {
        height_shear: [0.5, 0.0, 0.0, 0.0],
        ..plain
    };
    renderer
        .render(&flat_scene(upright_camera()).with_instances(&[leaning]))
        .expect("render");
    let after = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&after, SIZE, high.0, high.1),
        [255, 255, 255, 255],
        "the top of the quad leans a whole unit and covers this pixel"
    );
    assert_eq!(
        pixel(&after, SIZE, low.0, low.1),
        [0, 0, 0, 255],
        "the bottom leans a twentieth of one and does not"
    );
}

#[test]
fn the_height_shear_plane_decides_which_end_stays_still() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&upright_quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let hanging = Instance {
        texture: Some(white),
        height_shear: [-0.5, 0.0, 0.0, 2.0],
        ..Instance::new(mesh)
    };
    renderer
        .render(&flat_scene(upright_camera()).with_instances(&[hanging]))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 48, 42),
        [255, 255, 255, 255],
        "the bottom swings out"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 48, 22),
        [0, 0, 0, 255],
        "the top is pinned to the plane"
    );
}

#[test]
fn a_sheared_caster_casts_a_sheared_shadow() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 4.0, 12.0);

    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let caster = Instance {
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };

    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: towards_the_eye,
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.shadow = Some(ShadowSample {
        lightness: 0.0,
        edge: ShadowEdge::Taps(1.0),
        ..ShadowSample::single(map, light, 0.002)
    });
    let receivers = [receiver];

    let outside = (16, 32);
    let inside = (38, 32);

    let shadowed = |rgba: &[u8], at: (u32, u32)| pixel(rgba, SIZE, at.0, at.1)[0] < 64;

    let plain = [caster];
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&plain),
        )
        .expect("shadow pass");
    renderer
        .render(&scene.clone().with_instances(&receivers))
        .expect("render");
    let before = renderer.read_pixels().expect("readback");
    assert!(
        !shadowed(&before, outside),
        "world -1.5 is outside the un-sheared caster: {:?}",
        pixel(&before, SIZE, outside.0, outside.1)
    );
    assert!(
        shadowed(&before, inside),
        "world +0.56 is inside it: {:?}",
        pixel(&before, SIZE, inside.0, inside.1)
    );

    let leaning = [Instance {
        height_shear: [0.25, 0.0, 0.0, 0.0],
        ..caster
    }];
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&leaning),
        )
        .expect("shadow pass");
    renderer
        .render(&scene.with_instances(&receivers))
        .expect("render");
    let after = renderer.read_pixels().expect("readback");
    assert!(
        shadowed(&after, outside),
        "the shadow moved one unit left and now covers world -1.5: {:?}",
        pixel(&after, SIZE, outside.0, outside.1)
    );
    assert!(
        !shadowed(&after, inside),
        "and has left world +0.56: {:?}",
        pixel(&after, SIZE, inside.0, inside.1)
    );
}

fn quadrant_texture() -> TextureData {
    let red = [255u8, 0, 0, 255];
    let green = [0u8, 255, 0, 255];
    let blue = [0u8, 0, 255, 255];
    let yellow = [255u8, 255, 0, 255];
    let mut data = Vec::with_capacity(4 * 4 * 4);
    for row in 0..4u32 {
        for col in 0..4u32 {
            let texel = match (row < 2, col < 2) {
                (true, true) => red,
                (true, false) => green,
                (false, true) => blue,
                (false, false) => yellow,
            };
            data.extend_from_slice(&texel);
        }
    }
    TextureData::single(PixelFormat::Rgba8Unorm, 4, 4, data)
}

fn ui_scene(quads: &[UiQuad]) -> Scene<'_> {
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.with_ui_quads(quads)
}

#[test]
fn a_ui_quad_sampling_a_render_target_sees_it_after_a_resize() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let target = renderer.create_render_target(8, 8).expect("target");
    let texture = renderer.render_target_texture(target).expect("texture");
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let quads = [UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0])];
    let fill = |renderer: &mut Renderer, rgba: [f64; 4]| {
        let mut scene = flat_scene(unit_camera());
        scene.clear_color = rgba;
        renderer
            .render_to_target(target, &scene)
            .expect("target pass");
        renderer.render(&ui_scene(&quads)).expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, 32, 32)
    };
    assert_eq!(fill(&mut renderer, [1.0, 0.0, 0.0, 1.0]), [255, 0, 0, 255]);
    renderer
        .resize_render_target(target, 16, 16)
        .expect("resize");
    assert_eq!(
        fill(&mut renderer, [0.0, 1.0, 0.0, 1.0]),
        [0, 255, 0, 255],
        "the quad must sample the resized target, not the texture it replaced"
    );
}

fn composite(renderer: &mut Renderer, clear: [f64; 4], quads: &[UiQuad]) -> [u8; 4] {
    let mut scene = ui_scene(quads);
    scene.clear_color = clear;
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)
}

#[test]
fn a_premultiplied_quad_composites_its_colour_as_stored() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let half = renderer
        .upload_texture(&solid_linear([128, 0, 0, 128]))
        .expect("upload");
    let opaque = renderer
        .upload_texture(&solid_linear([128, 0, 0, 255]))
        .expect("upload");
    let at = |texture, premultiplied| UiQuad {
        premultiplied,
        ..UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0])
    };
    let black = [0.0, 0.0, 0.0, 1.0];
    let reference = composite(&mut renderer, black, &[at(opaque, false)]);
    let stored = composite(&mut renderer, black, &[at(half, true)]);
    let straight = composite(&mut renderer, black, &[at(half, false)]);
    assert!(
        stored[0].abs_diff(reference[0]) <= 2,
        "{stored:?} vs {reference:?}"
    );
    assert!(straight[0] + 20 < stored[0], "{straight:?} vs {stored:?}");
    let glow = renderer
        .upload_texture(&solid_linear([128, 0, 0, 0]))
        .expect("upload");
    let green = [0.0, 1.0, 0.0, 1.0];
    let over = composite(&mut renderer, green, &[at(glow, true)]);
    assert_eq!(over[1], 255, "the green under a glow survives: {over:?}");
    assert!(over[0] > 60, "and the glow adds: {over:?}");
}

#[test]
fn a_greyscale_quad_draws_its_texture_as_luma() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 255]))
        .expect("upload");
    let grey = renderer
        .upload_texture(&solid_linear([76, 76, 76, 255]))
        .expect("upload");
    let whole =
        |texture| UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0]);
    let black = [0.0, 0.0, 0.0, 1.0];
    let greyed = composite(
        &mut renderer,
        black,
        &[UiQuad {
            greyscale: true,
            ..whole(red)
        }],
    );
    let reference = composite(&mut renderer, black, &[whole(grey)]);
    for channel in 0..3 {
        assert!(
            greyed[channel].abs_diff(reference[channel]) <= 2,
            "{greyed:?} vs {reference:?}"
        );
    }
}

#[test]
fn an_opaque_quad_draws_its_colour_where_its_texture_is_clear() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let clear_red = renderer
        .upload_texture(&solid_linear([255, 0, 0, 0]))
        .expect("upload");
    let whole = UiQuad::new(0.0, 0.0, size, size).with_texture(clear_red, [0.0, 0.0, 1.0, 1.0]);
    let black = [0.0, 0.0, 0.0, 1.0];
    let matted = composite(&mut renderer, black, &[whole]);
    let opaque = composite(
        &mut renderer,
        black,
        &[UiQuad {
            opaque: true,
            ..whole
        }],
    );
    let half = composite(
        &mut renderer,
        black,
        &[UiQuad {
            opaque: true,
            ..whole.with_color_transform([1.0, 1.0, 1.0, 0.5], [0.0; 4])
        }],
    );
    assert!(matted[0] <= 2, "clear texel covers nothing: {matted:?}");
    assert!(opaque[0] >= 253, "opaque: its colour: {opaque:?}");
    assert!(
        (120..=136).contains(&half[0]),
        "the transform still fades: {half:?}"
    );
}

fn composite_at(
    renderer: &mut Renderer,
    clear: [f64; 4],
    quads: &[UiQuad],
    at: [u32; 2],
) -> [u8; 4] {
    let mut scene = ui_scene(quads);
    scene.clear_color = clear;
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    pixel(&rgba, SIZE, at[0], at[1])
}

#[test]
fn a_disc_quad_draws_only_its_inscribed_circle() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let red =
        UiQuad::new(0.0, 0.0, size, size).with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0; 4]);
    let black = [0.0, 0.0, 0.0, 1.0];
    let square = composite_at(&mut renderer, black, &[red], [0, 0]);
    let corner = composite_at(
        &mut renderer,
        black,
        &[UiQuad { disc: true, ..red }],
        [0, 0],
    );
    let centre = composite_at(
        &mut renderer,
        black,
        &[UiQuad { disc: true, ..red }],
        [SIZE / 2, SIZE / 2],
    );
    assert!(square[0] >= 253, "the square covers its corner: {square:?}");
    assert!(corner[0] <= 2, "the disc does not: {corner:?}");
    assert!(centre[0] >= 253, "the disc covers its centre: {centre:?}");
}

#[test]
fn a_half_wedge_draws_the_sector_ending_at_twelve_oclock() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let red =
        UiQuad::new(0.0, 0.0, size, size).with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0; 4]);
    let black = [0.0, 0.0, 0.0, 1.0];
    let half = UiQuad { wedge: 0.5, ..red };
    let nine_oclock = composite_at(&mut renderer, black, &[half], [0, SIZE / 2]);
    let three_oclock = composite_at(&mut renderer, black, &[half], [SIZE - 1, SIZE / 2]);
    assert!(
        nine_oclock[0] >= 253,
        "nine o'clock, before twelve, draws: {nine_oclock:?}"
    );
    assert!(
        three_oclock[0] <= 2,
        "three o'clock, after twelve, is discarded: {three_oclock:?}"
    );
}

#[test]
fn a_hole_leaves_only_the_rim() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let red =
        UiQuad::new(0.0, 0.0, size, size).with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0; 4]);
    let black = [0.0, 0.0, 0.0, 1.0];
    let ring = UiQuad {
        disc: true,
        hole: 0.8,
        ..red
    };
    let centre = composite_at(&mut renderer, black, &[ring], [SIZE / 2, SIZE / 2]);
    let rim = composite_at(&mut renderer, black, &[ring], [1, SIZE / 2]);
    assert!(centre[0] <= 2, "the hole is discarded: {centre:?}");
    assert!(rim[0] >= 253, "the rim draws: {rim:?}");
}

#[test]
fn additive_light_in_a_render_target_leaves_what_is_under_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([128, 0, 0, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(red),
        blend: Blend::Additive,
        ..Instance::new(mesh)
    }];
    let target = renderer.create_render_target(8, 8).expect("target");
    let texture = renderer.render_target_texture(target).expect("texture");
    let mut scene = flat_scene(unit_camera()).with_instances(&instances);
    scene.clear_color = [0.0, 0.0, 0.0, 0.0];
    renderer
        .render_to_target(target, &scene)
        .expect("target pass");
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let layer = UiQuad {
        premultiplied: true,
        ..UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0])
    };
    let over = composite(&mut renderer, [0.0, 1.0, 0.0, 1.0], &[layer]);
    assert_eq!(over[1], 255, "green survives: {over:?}");
    assert!(over[0] > 60, "the light adds: {over:?}");
}

#[test]
fn a_solid_ui_quad_draws_at_its_screen_position_and_nowhere_else() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let quads = [UiQuad {
        color_multiply: [0.0, 1.0, 0.0, 1.0],
        ..UiQuad::new(8.0, 8.0, 16.0, 16.0)
    }];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [0, 255, 0, 255],
        "the quad's centre should be solid green"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 1, 1),
        [0, 0, 0, 255],
        "well outside the quad's 8..24 box, the clear colour must survive"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 40, 40),
        [0, 0, 0, 255],
        "and so must a point on the opposite side of the frame"
    );
}

#[test]
fn a_ui_quad_samples_its_texture_over_the_default_full_uv_rect() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let quads = [UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0])];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [255, 0, 0, 255],
        "top-left: red"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 48, 16),
        [0, 255, 0, 255],
        "top-right: green"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 16, 48),
        [0, 0, 255, 255],
        "bottom-left: blue"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 48, 48),
        [255, 255, 0, 255],
        "bottom-right: yellow"
    );
}

#[test]
fn a_ui_quad_uv_rect_restricts_sampling_to_its_own_sub_region() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let quads = [UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.5, 0.5, 0.5, 0.5])];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 32, 32),
        [255, 255, 0, 255],
        "uv_rect should confine sampling to the texture's yellow quadrant"
    );
}

#[test]
fn a_ui_quad_color_transform_multiplies_then_adds() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let quads = [UiQuad::new(0.0, 0.0, size, size)
        .with_color_transform([0.5, 0.2, 0.8, 1.0], [0.1, 0.1, 0.0, 0.0])];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let expected = [
        (0.1 + 0.5) * 255.0,
        (0.1 + 0.2) * 255.0,
        (0.0 + 0.8) * 255.0,
        255.0,
    ];
    let got = pixel(&rgba, SIZE, 32, 32);
    for (channel, (byte, want)) in got.iter().zip(expected).enumerate() {
        let diff = (f32::from(*byte) - want).abs();
        assert!(
            diff <= 2.0,
            "channel {channel}: got {byte}, expected close to {want} (full pixel {got:?})"
        );
    }
}

#[test]
fn a_ui_quad_clip_rect_discards_outside_pixels_without_touching_them() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let quads = [UiQuad::new(0.0, 0.0, size, size)
        .with_color_transform([0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0])
        .with_clip([16.0, 16.0, 48.0, 48.0])];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 32, 32),
        [0, 255, 0, 255],
        "inside the clip rect, the quad draws normally"
    );
    for (x, y) in [(4, 4), (60, 4), (4, 60), (60, 60), (32, 4)] {
        assert_eq!(
            pixel(&rgba, SIZE, x, y),
            [0, 0, 0, 255],
            "outside the clip rect at ({x}, {y}), the clear colour must survive untouched"
        );
    }
}

#[test]
fn a_ui_quad_affine_matrix_is_a_real_rotation_not_a_bounding_box_scale() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let (width, height) = (40.0f32, 10.0f32);
    let quads = [UiQuad {
        affine: [[0.0, -height], [width, 0.0]],
        translation: [height, 0.0],
        color_multiply: [1.0, 0.0, 0.0, 1.0],
        ..UiQuad::new(0.0, 0.0, 0.0, 0.0)
    }];
    renderer.render(&ui_scene(&quads)).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 5, 20),
        [255, 0, 0, 255],
        "(5, 20) is inside the rotated 10-wide, 40-tall footprint"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 25, 5),
        [0, 0, 0, 255],
        "(25, 5) is inside the *un*rotated 40-wide, 10-tall footprint only - \
         a bounding-box scale would wrongly draw here"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 5, 50),
        [0, 0, 0, 255],
        "(5, 50) is past the rotated footprint's 40-tall extent"
    );
}

#[test]
fn ui_quads_blend_over_each_other_in_submission_order() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let red = UiQuad::new(16.0, 16.0, 32.0, 32.0)
        .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]);
    let translucent_blue = UiQuad::new(16.0, 16.0, 32.0, 32.0)
        .with_color_transform([0.0, 0.0, 1.0, 0.5], [0.0, 0.0, 0.0, 0.0]);

    renderer
        .render(&ui_scene(&[red, translucent_blue]))
        .expect("render");
    let blue_over_red = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);

    renderer
        .render(&ui_scene(&[translucent_blue, red]))
        .expect("render");
    let red_over_blue = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);

    assert_eq!(
        red_over_blue,
        [255, 0, 0, 255],
        "an opaque quad drawn last must fully replace what is under it"
    );
    assert_ne!(
        blue_over_red, red_over_blue,
        "the two draw orders must produce visibly different pixels, or \
         submission order is not actually being honoured"
    );
    assert!(
        blue_over_red[2] > 0 && blue_over_red[0] > 0,
        "blue drawn last over red should leave both channels present: {blue_over_red:?}"
    );
}

#[test]
fn a_ui_quad_naming_another_renderers_texture_is_an_error_and_not_a_wild_read() {
    let Some(mut first) = renderer(SIZE) else {
        return;
    };
    let Some(mut second) = renderer(SIZE) else {
        return;
    };
    let stranger = first
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");

    let quads = [UiQuad::new(0.0, 0.0, 8.0, 8.0).with_texture(stranger, [0.0, 0.0, 1.0, 1.0])];
    assert!(matches!(
        second.render(&ui_scene(&quads)),
        Err(Error::UnknownHandle)
    ));
}

fn ui_geometry_scene<'a>(geometry: &'a [UiGeometry<'a>]) -> Scene<'a> {
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.with_ui_geometry(geometry)
}

#[test]
fn a_ui_geometry_triangle_draws_at_its_transformed_position_and_nowhere_else() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [32.0, 0.0], [0.0, 32.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_transform([[1.0, 0.0], [0.0, 1.0]], [8.0, 8.0])
        .with_color([0.0, 1.0, 0.0, 1.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 12, 12),
        [0, 255, 0, 255],
        "comfortably inside the triangle, near its right-angle corner at (8, 8)"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 36, 36),
        [0, 0, 0, 255],
        "inside the triangle's own bounding box but beyond its hypotenuse"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 1, 1),
        [0, 0, 0, 255],
        "well outside the triangle's bounding box entirely"
    );
}

#[test]
fn a_ui_geometry_triangle_color_add_lands_after_the_multiply() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [64.0, 0.0], [0.0, 64.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_color([0.2, 0.0, 0.0, 1.0])
        .with_color_add([0.1, 0.4, 0.0, 0.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let expected = [(0.2 + 0.1) * 255.0, (0.0 + 0.4) * 255.0, 0.0, 255.0];
    let got = pixel(&rgba, SIZE, 12, 12);
    for (channel, (byte, want)) in got.iter().zip(expected).enumerate() {
        let diff = (f32::from(*byte) - want).abs();
        assert!(
            diff <= 2.0,
            "channel {channel}: got {byte}, expected close to {want} (full pixel {got:?})"
        );
    }
}

#[test]
fn a_ui_geometry_line_draws_along_its_segment_and_nowhere_else() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let segment: [[f32; 2]; 2] = [[8.0, 32.5], [56.0, 32.5]];
    let geometry = [UiGeometry::new()
        .with_lines(&segment)
        .with_color([0.0, 1.0, 0.0, 1.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 32, 32),
        [0, 255, 0, 255],
        "on the segment, at its own row"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 32, 10),
        [0, 0, 0, 255],
        "well off the segment's row, the clear colour must survive"
    );
}

#[test]
fn a_ui_geometry_transform_places_local_vertices_not_just_translates_them() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [40.0, 0.0], [0.0, 10.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_transform([[0.0, -1.0], [1.0, 0.0]], [20.0, 5.0])
        .with_color([1.0, 0.0, 0.0, 1.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 18, 10),
        [255, 0, 0, 255],
        "(18, 10) is inside the rotated 10-wide, 40-tall footprint"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 40, 10),
        [0, 0, 0, 255],
        "(40, 10) is inside the *un*rotated 40-wide, 10-tall footprint only - \
         a bounding-box scale would wrongly draw here"
    );
}

#[test]
fn a_ui_geometry_clip_rect_discards_outside_pixels_without_touching_them() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [64.0, 0.0], [0.0, 64.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_color([0.0, 1.0, 0.0, 1.0])
        .with_clip([16.0, 16.0, 32.0, 32.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 20, 20),
        [0, 255, 0, 255],
        "inside the clip rect and inside the triangle, draws normally"
    );
    for (x, y) in [(2, 2), (40, 2), (2, 40)] {
        assert_eq!(
            pixel(&rgba, SIZE, x, y),
            [0, 0, 0, 255],
            "({x}, {y}) is inside the triangle but outside the clip rect - \
             the clear colour must survive untouched, not merely dimmed"
        );
    }
}

#[test]
fn a_ui_geometry_triangle_count_not_a_multiple_of_three_is_truncated_not_rejected() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let vertices: [[f32; 2]; 4] = [[0.0, 0.0], [32.0, 0.0], [0.0, 32.0], [999.0, 999.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&vertices)
        .with_color([0.0, 1.0, 0.0, 1.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("a vertex count that is not a multiple of 3 must not error or panic");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 5, 5),
        [0, 255, 0, 255],
        "the one complete triangle from the first 3 vertices still draws; \
         the dangling 4th vertex is silently dropped, not read as a stray point"
    );
}

#[test]
fn a_ui_geometry_textured_triangle_samples_its_texture_and_multiplies_by_color() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    let square: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [4.0, 0.0],
        [0.0, 4.0],
        [4.0, 0.0],
        [4.0, 4.0],
        [0.0, 4.0],
    ];
    let geometry = [UiGeometry::new()
        .with_triangles(&square)
        .with_transform([[16.0, 0.0], [0.0, 16.0]], [0.0, 0.0])
        .with_texture(texture, [[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0])
        .with_color([1.0, 0.5, 1.0, 1.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [255, 0, 0, 255],
        "top-left: red, untouched by the multiply"
    );
    let green = pixel(&rgba, SIZE, 48, 16);
    assert_eq!(green[0], 0, "top-right: no red");
    assert!(
        (f32::from(green[1]) - 127.5).abs() <= 1.0,
        "top-right: green halved by `color`'s 0.5, got {green:?}"
    );
    assert_eq!(green[2], 0, "top-right: no blue");
    assert_eq!(
        pixel(&rgba, SIZE, 16, 48),
        [0, 0, 255, 255],
        "bottom-left: blue, untouched by the multiply"
    );
    let yellow = pixel(&rgba, SIZE, 48, 48);
    assert_eq!(yellow[0], 255, "bottom-right: full red");
    assert!(
        (f32::from(yellow[1]) - 127.5).abs() <= 1.0,
        "bottom-right: yellow halved in its green channel, got {yellow:?}"
    );
    assert_eq!(yellow[2], 0, "bottom-right: no blue");
}

#[test]
fn a_ui_geometry_textured_triangle_color_add_lands_after_the_multiply() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    let square: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [4.0, 0.0],
        [0.0, 4.0],
        [4.0, 0.0],
        [4.0, 4.0],
        [0.0, 4.0],
    ];
    let geometry = [UiGeometry::new()
        .with_triangles(&square)
        .with_transform([[16.0, 0.0], [0.0, 16.0]], [0.0, 0.0])
        .with_texture(texture, [[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0])
        .with_color([1.0, 1.0, 1.0, 1.0])
        .with_color_add([0.0, 0.0, 0.3, 0.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    let got = pixel(&rgba, SIZE, 16, 16);
    assert_eq!(got[0], 255, "red channel untouched by the add");
    assert_eq!(got[1], 0, "green channel untouched by the add");
    assert!(
        (f32::from(got[2]) - 0.3 * 255.0).abs() <= 2.0,
        "blue channel should be sampled(0) + add(0.3), got {got:?}"
    );
}

#[test]
fn a_ui_geometry_textured_triangle_transform_is_not_its_own_transpose() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    let rect: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [8.0, 0.0],
        [0.0, 4.0],
        [8.0, 0.0],
        [8.0, 4.0],
        [0.0, 4.0],
    ];
    let geometry = [UiGeometry::new()
        .with_triangles(&rect)
        .with_transform([[8.0, 0.0], [0.0, 16.0]], [0.0, 0.0])
        .with_texture(texture, [[0.0, 1.0], [0.5, 0.0]], [0.0, 0.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [255, 0, 0, 255],
        "u = y = 1, v = 0.5x = 1 -> red; a transposed mapping would give \
         u = 0.5x = 1 here too (coincides), so this point alone cannot fail \
         on a transpose - it only anchors the other three"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 16, 48),
        [0, 255, 0, 255],
        "u = y = 3, v = 0.5x = 1 -> green; the transposed reading gives \
         u = 0.5x = 1, v = y = 3 -> blue instead, so this point alone \
         distinguishes the two conventions"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 48, 16),
        [0, 0, 255, 255],
        "u = y = 1, v = 0.5x = 3 -> blue; the transposed reading gives \
         u = 0.5x = 3, v = y = 1 -> green instead"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 48, 48),
        [255, 255, 0, 255],
        "u = y = 3, v = 0.5x = 3 -> yellow; symmetric under transpose, like \
         the first point"
    );
}

#[test]
fn a_ui_geometry_textured_triangle_clip_rect_discards_outside_pixels() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_transform([[16.0, 0.0], [0.0, 16.0]], [0.0, 0.0])
        .with_texture(texture, [[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0])
        .with_clip([0.0, 0.0, 20.0, 20.0])];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 10, 10),
        [255, 0, 0, 255],
        "inside both the triangle and the clip rect"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 25, 10),
        [0, 0, 0, 255],
        "inside the triangle's footprint but past the clip rect's right edge"
    );
}

#[test]
fn a_ui_geometry_textured_triangle_naming_another_renderers_texture_is_an_error() {
    let Some(mut first) = renderer(SIZE) else {
        return;
    };
    let Some(mut second) = renderer(SIZE) else {
        return;
    };
    let foreign = first.upload_texture(&quadrant_texture()).expect("upload");

    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [2.0, 0.0], [0.0, 2.0]];
    let geometry = [UiGeometry::new().with_triangles(&triangle).with_texture(
        foreign,
        [[1.0, 0.0], [0.0, 1.0]],
        [0.0, 0.0],
    )];
    let result = second.render(&ui_geometry_scene(&geometry));
    assert!(
        matches!(result, Err(Error::UnknownHandle)),
        "got {result:?}"
    );
}

#[test]
fn ui_geometry_draws_before_ui_quads_within_the_same_screen_pass() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [64.0, 0.0], [0.0, 64.0]];
    let geometry = [UiGeometry::new()
        .with_triangles(&triangle)
        .with_color([0.0, 1.0, 0.0, 1.0])];
    let quads = [UiQuad::new(0.0, 0.0, 64.0, 64.0)
        .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0])];

    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ui_geometry(&geometry).with_ui_quads(&quads);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [255, 0, 0, 255],
        "the quad must be drawn after the geometry, so the opaque quad fully \
         replaces it rather than the other way round"
    );
}

#[test]
fn ordered_screen_draws_interleave_in_the_callers_own_order() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [64.0, 0.0], [0.0, 64.0]];

    let quad_last = [
        ScreenDraw::Geometry(
            UiGeometry::new()
                .with_triangles(&triangle)
                .with_color([0.0, 1.0, 0.0, 1.0]),
        ),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&quad_last);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [255, 0, 0, 255],
        "quad submitted after the geometry must cover it"
    );

    let geometry_last = [
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::Geometry(
            UiGeometry::new()
                .with_triangles(&triangle)
                .with_color([0.0, 1.0, 0.0, 1.0]),
        ),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&geometry_last);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 16, 16),
        [0, 255, 0, 255],
        "geometry submitted after the quad must cover it"
    );
}

#[test]
fn ordered_screen_quad_runs_still_batch_but_never_across_an_interrupting_geometry_entry() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let triangle: [[f32; 2]; 3] = [[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]];
    let items = [
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 20.0, 64.0)
                .with_color_transform([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::Geometry(
            UiGeometry::new()
                .with_triangles(&triangle)
                .with_color([0.0, 1.0, 0.0, 1.0]),
        ),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 44.0, 64.0, 20.0)
                .with_color_transform([1.0, 1.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&items);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 4, 4),
        [0, 255, 0, 255],
        "the geometry (green) is submitted after the first quad (blue) and \
         overlaps it, so it must be on top - a run wrongly merged across the \
         interruption would draw the geometry first and the (batched) blue \
         quad on top of it instead"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 16, 8),
        [0, 0, 255, 255],
        "the first quad (blue) still covers the part of its own rect the \
         geometry does not overlap"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 8, 56),
        [255, 255, 0, 255],
        "the run resumed after the interruption (yellow, bottom strip) must still draw"
    );
}

#[test]
fn a_masked_quad_draws_only_inside_the_mask_shape() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mask_triangles: [[f32; 2]; 3] = [[0.0, 0.0], [0.0, 64.0], [64.0, 64.0]];
    let mask = MaskShape::new(&mask_triangles);
    let items = [
        ScreenDraw::PushMask(mask),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::PopMask(mask),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&items);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 8, 56),
        [255, 0, 0, 255],
        "comfortably inside the mask triangle (near its bottom-left corner): \
         the quad must show"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 56, 8),
        [0, 0, 0, 255],
        "comfortably outside the mask triangle (near the top-right corner, \
         inside the quad's own bounds but on the wrong side of the diagonal): \
         a bounding-box clip would wrongly show the quad here, a real stencil \
         test must not"
    );
}

#[test]
fn pop_mask_reverts_masking_for_content_drawn_after_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mask_triangles: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [32.0, 0.0],
        [0.0, 64.0],
        [32.0, 0.0],
        [32.0, 64.0],
        [0.0, 64.0],
    ];
    let mask = MaskShape::new(&mask_triangles);
    let items = [
        ScreenDraw::PushMask(mask),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::PopMask(mask),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&items);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 8, 32),
        [0, 255, 0, 255],
        "left half: the unmasked green quad, drawn after the pop, must cover \
         the masked red one"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 56, 32),
        [0, 255, 0, 255],
        "right half: outside the mask, so only the unmasked green quad ever \
         drew here at all - the pop having happened or not makes no visible \
         difference on this side, this just confirms the green quad itself \
         drew everywhere it was submitted to"
    );
}

#[test]
fn nested_masks_intersect_and_unwind_one_level_at_a_time() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mask_a_triangles: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [32.0, 0.0],
        [0.0, 64.0],
        [32.0, 0.0],
        [32.0, 64.0],
        [0.0, 64.0],
    ];
    let mask_b_triangles: [[f32; 2]; 6] = [
        [0.0, 0.0],
        [64.0, 0.0],
        [0.0, 32.0],
        [64.0, 0.0],
        [64.0, 32.0],
        [0.0, 32.0],
    ];
    let mask_a = MaskShape::new(&mask_a_triangles);
    let mask_b = MaskShape::new(&mask_b_triangles);
    let items = [
        ScreenDraw::PushMask(mask_a),
        ScreenDraw::PushMask(mask_b),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::PopMask(mask_b),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::PopMask(mask_a),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&items);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");

    assert_eq!(
        pixel(&rgba, SIZE, 8, 8),
        [0, 0, 255, 255],
        "top-left (inside both A and B): the red draw at depth 2 happened \
         here, but the blue draw at depth 1 came after and covers it"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 8, 56),
        [0, 0, 255, 255],
        "bottom-left (inside A only): the red depth-2 draw never reached \
         here (B excluded it), but the blue depth-1 draw does, proving the \
         pop of B returned this region to depth-1 visibility rather than \
         leaving it however B's push left it"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 56, 8),
        [0, 0, 0, 255],
        "top-right (inside B only, outside A): neither draw is visible here, \
         since both are masked by A"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 56, 56),
        [0, 0, 0, 255],
        "bottom-right (outside both): clear colour, nothing drew here"
    );
}

#[test]
fn a_mask_of_overlapping_pieces_masks_their_union() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mask_triangles: [[f32; 2]; 12] = [
        [0.0, 0.0],
        [40.0, 0.0],
        [0.0, 64.0],
        [40.0, 0.0],
        [40.0, 64.0],
        [0.0, 64.0],
        [24.0, 0.0],
        [64.0, 0.0],
        [24.0, 64.0],
        [64.0, 0.0],
        [64.0, 64.0],
        [24.0, 64.0],
    ];
    let mask = MaskShape::new(&mask_triangles);
    let items = [
        ScreenDraw::PushMask(mask),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 64.0)
                .with_color_transform([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
        ScreenDraw::PopMask(mask),
        ScreenDraw::Quad(
            UiQuad::new(0.0, 0.0, 64.0, 8.0)
                .with_color_transform([0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 0.0, 0.0]),
        ),
    ];
    let mut scene = Scene::new(unit_camera());
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    let scene = scene.with_ordered_screen(&items);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 8, 32),
        [255, 0, 0, 255],
        "first box only"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 32, 32),
        [255, 0, 0, 255],
        "the overlap is inside the mask, not a hole in it"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 56, 32),
        [255, 0, 0, 255],
        "second box only"
    );
    assert_eq!(
        pixel(&rgba, SIZE, 32, 4),
        [0, 255, 0, 255],
        "after the pop the overlap is unmasked again"
    );
}

#[test]
fn ui_geometry_entries_composite_in_submission_order_not_just_r1_over_r2() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let big: [[f32; 2]; 3] = [[0.0, 0.0], [64.0, 0.0], [0.0, 64.0]];
    let small: [[f32; 2]; 3] = [[0.0, 0.0], [32.0, 0.0], [0.0, 32.0]];
    let red_big = UiGeometry::new()
        .with_triangles(&big)
        .with_color([1.0, 0.0, 0.0, 1.0]);
    let green_small = UiGeometry::new()
        .with_triangles(&small)
        .with_color([0.0, 1.0, 0.0, 1.0]);

    let geometry = [red_big, green_small];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 10, 10),
        [0, 255, 0, 255],
        "red first, green second: green (drawn last) must be on top"
    );

    let geometry = [green_small, red_big];
    renderer
        .render(&ui_geometry_scene(&geometry))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, 10, 10),
        [255, 0, 0, 255],
        "green first, red second: red (drawn last) must be on top - the \
         reverse of the case above, so this cannot pass by coincidence"
    );
}

#[test]
fn the_house_colour_replaces_the_lit_base_by_the_masks_alpha() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let base = renderer
        .upload_texture(&solid_linear([128, 128, 128, 255]))
        .expect("upload");
    let mut upload = |rgba| {
        renderer
            .upload_texture(&solid_linear(rgba))
            .expect("upload")
    };
    let opaque = upload([200, 0, 0, 255]);
    let quarter = upload([204, 0, 0, 64]);
    let clear = upload([255, 255, 255, 0]);

    let red = [1.0, 0.0, 0.0, 1.0];
    let draw = |renderer: &mut Renderer, mask| {
        let instances = [Instance {
            texture: Some(base),
            texture2: Some(mask),
            detail_combine: mint_render::DetailCombine::HouseColor,
            house_color: red,
            ..Instance::new(mesh)
        }];
        let mut scene = flat_scene(unit_camera());
        scene.ambient_color = [0.5; 3];
        renderer
            .render(&scene.with_instances(&instances))
            .expect("render");
        pixel(
            &renderer.read_pixels().expect("readback"),
            SIZE,
            SIZE / 2,
            SIZE / 2,
        )
    };

    let full = draw(&mut renderer, opaque);
    assert_eq!(
        full,
        [200, 0, 0, 255],
        "an opaque mask shows mask.r * colour, unlit"
    );
    let none = draw(&mut renderer, clear);
    assert_eq!(
        none,
        [64, 64, 64, 255],
        "a transparent mask leaves the lit base, whatever its RGB"
    );
    let partial = draw(&mut renderer, quarter);
    for (channel, expected) in [(0usize, 99.0f32), (1, 48.0), (2, 48.0)] {
        let got = f32::from(partial[channel]);
        assert!(
            (got - expected).abs() <= 2.0,
            "channel {channel}: {got} vs {expected}, lerp(64, 204, 64/255) on red"
        );
    }
}

#[test]
fn the_house_colour_mixes_three_colours_by_the_masks_channels() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let base = renderer
        .upload_texture(&solid_linear([128, 128, 128, 255]))
        .expect("upload");
    let mask = renderer
        .upload_texture(&solid_linear([100, 200, 0, 255]))
        .expect("upload");
    let mut draw = |extra: [[f32; 4]; 2]| {
        let instances = [Instance {
            texture: Some(base),
            texture2: Some(mask),
            detail_combine: mint_render::DetailCombine::HouseColor,
            house_color: [1.0, 0.0, 0.0, 1.0],
            house_color_extra: extra,
            ..Instance::new(mesh)
        }];
        renderer
            .render(&flat_scene(unit_camera()).with_instances(&instances))
            .expect("render");
        pixel(
            &renderer.read_pixels().expect("readback"),
            SIZE,
            SIZE / 2,
            SIZE / 2,
        )
    };
    assert_eq!(
        draw([[0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]]),
        [100, 200, 0, 255],
        "hair on red, skin on green, paint on blue"
    );
    assert_eq!(
        draw([[0.0; 4]; 2]),
        [100, 0, 0, 255],
        "a one-colour list reads the mask's red channel only"
    );
}

#[test]
fn a_house_colour_with_no_second_stage_changes_nothing() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let base = renderer
        .upload_texture(&solid_linear([64, 128, 192, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(base),
        texture2: None,
        detail_combine: mint_render::DetailCombine::HouseColor,
        house_color: [1.0, 0.0, 0.0, 1.0],
        unlit: true,
        ..Instance::new(mesh)
    }];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [64, 128, 192, 255],
        "with no mask bound the instance must draw as its base texture"
    );
}

fn skinned_grid() -> (MeshData, Vec<mint_render::SkinVertex>) {
    let cells = 12u32;
    let mut vertices = Vec::new();
    let mut skin = Vec::new();
    for row in 0..=cells {
        for column in 0..=cells {
            let u = column as f32 / cells as f32;
            let v = row as f32 / cells as f32;
            let x = u * 1.4 - 0.7;
            let y = v * 1.4 - 0.7;
            vertices.push(
                Vertex::new([x, y, 0.0], [0.0, 0.0, 1.0], [u, v])
                    .with_tangent([1.0, 0.0, 0.0, 1.0]),
            );
            skin.push(mint_render::SkinVertex {
                bones: [0, 1],
                weights: [1.0 - u, u],
                position2: [x * 0.8 + 0.1, y * 0.9 - 0.05, 0.1],
                normal2: [0.3, 0.0, 0.954],
            });
        }
    }
    let mut indices = Vec::new();
    let stride = cells + 1;
    for row in 0..cells {
        for column in 0..cells {
            let a = row * stride + column;
            let (b, c, d) = (a + 1, a + stride + 1, a + stride);
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    (MeshData { vertices, indices }, skin)
}

fn two_bones() -> [Mat4; 2] {
    let (c0, s0) = (0.955_336_5_f32, 0.295_520_2_f32);
    let (c1, s1) = (0.968_912_4_f32, 0.247_404_f32);
    [
        Mat4::from_cols([
            [c0, s0, 0.0, 0.0],
            [-s0, c0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.05, -0.1, 0.0, 1.0],
        ]),
        Mat4::from_cols([
            [c1, 0.0, -s1, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [s1, 0.0, c1, 0.0],
            [-0.1, 0.15, -0.05, 1.0],
        ]),
    ]
}

fn cpu_posed(mesh: &MeshData, skin: &[mint_render::SkinVertex], bones: &[Mat4]) -> MeshData {
    let mut posed = mesh.clone();
    for (vertex, entry) in posed.vertices.iter_mut().zip(skin) {
        let (p, n, t) = mint_render::skin_point(
            bones,
            Vec3::from_array(vertex.position),
            Vec3::from_array(vertex.normal),
            Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]),
            entry,
        );
        vertex.position = p.to_array();
        vertex.normal = n.to_array();
        vertex.tangent = [t.x, t.y, t.z, vertex.tangent[3]];
    }
    posed
}

fn side_lit_scene() -> Scene<'static> {
    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.2, 0.2, 0.2];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: Vec3::new(0.6, 0.2, -0.77),
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene
}

fn frame_difference(a: &[u8], b: &[u8], tolerance: u8) -> (usize, u8) {
    let mut over = 0usize;
    let mut largest = 0u8;
    for (pa, pb) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let worst = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        largest = largest.max(worst);
        if worst > tolerance {
            over += 1;
        }
    }
    (over, largest)
}

#[test]
fn a_skinned_mesh_draws_where_its_cpu_twin_does() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let (mesh, skin) = skinned_grid();
    let bones = two_bones();

    let skinned = renderer
        .upload_skinned_mesh(&mesh, &skin)
        .expect("skinned upload");
    renderer.set_bones(&bones).expect("bones");
    let gpu = [Instance {
        texture: Some(white),
        bone_base: Some(0),
        ..Instance::new(skinned)
    }];
    renderer
        .render(&side_lit_scene().with_instances(&gpu))
        .expect("render");
    let from_gpu = renderer.read_pixels().expect("readback");

    let twin = renderer
        .upload_mesh(&cpu_posed(&mesh, &skin, &bones))
        .expect("upload");
    let cpu = [Instance {
        texture: Some(white),
        ..Instance::new(twin)
    }];
    renderer
        .render(&side_lit_scene().with_instances(&cpu))
        .expect("render");
    let from_cpu = renderer.read_pixels().expect("readback");

    let bind = renderer.upload_mesh(&mesh).expect("upload");
    let unposed = [Instance {
        texture: Some(white),
        ..Instance::new(bind)
    }];
    renderer
        .render(&side_lit_scene().with_instances(&unposed))
        .expect("render");
    let from_bind = renderer.read_pixels().expect("readback");

    let (posed_moved, _) = frame_difference(&from_bind, &from_cpu, 8);
    assert!(
        posed_moved > 200,
        "the pose must move the picture for the comparison to mean anything \
         ({posed_moved} pixels changed)"
    );
    let (over, largest) = frame_difference(&from_gpu, &from_cpu, 2);
    assert!(
        over <= 4,
        "{over} pixels differ by more than 2 levels (largest {largest}) between the \
         vertex shader's pose and the CPU's"
    );
}

#[test]
fn two_instances_of_one_skinned_mesh_take_their_own_bones_in_one_draw() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let mut small = quad();
    for vertex in &mut small.vertices {
        vertex.position = [vertex.position[0] * 0.2, vertex.position[1] * 0.2, 0.0];
    }
    let skin = vec![
        mint_render::SkinVertex {
            bones: [0, 0],
            weights: [1.0, 0.0],
            ..mint_render::SkinVertex::default()
        };
        small.vertices.len()
    ];
    let mesh = renderer
        .upload_skinned_mesh(&small, &skin)
        .expect("skinned upload");
    renderer
        .set_bones(&[
            Mat4::from_translation(Vec3::new(-0.6, 0.0, 0.0)),
            Mat4::from_translation(Vec3::new(0.6, 0.0, 0.0)),
        ])
        .expect("bones");
    let instances = [
        Instance {
            texture: Some(white),
            bone_base: Some(0),
            ..Instance::new(mesh)
        },
        Instance {
            texture: Some(white),
            bone_base: Some(1),
            ..Instance::new(mesh)
        },
    ];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, 13, SIZE / 2), [255, 255, 255, 255]);
    assert_eq!(pixel(&rgba, SIZE, 51, SIZE / 2), [255, 255, 255, 255]);
    assert_eq!(
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
        [0, 0, 0, 255],
        "the bind pose - the centre - must be empty: both copies were moved"
    );
    assert_eq!(renderer.draw_calls(), 1, "one mesh, one material, one call");
}

#[test]
fn a_skinned_draw_outside_the_palette_is_refused() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh_data = quad();
    let reads_bone_one = vec![
        mint_render::SkinVertex {
            bones: [1, 0],
            weights: [1.0, 0.0],
            ..mint_render::SkinVertex::default()
        };
        mesh_data.vertices.len()
    ];
    let mesh = renderer
        .upload_skinned_mesh(&mesh_data, &reads_bone_one)
        .expect("upload");
    renderer.set_bones(&[Mat4::IDENTITY]).expect("bones");

    let past = [Instance {
        bone_base: Some(0),
        ..Instance::new(mesh)
    }];
    assert!(matches!(
        renderer.render(&flat_scene(unit_camera()).with_instances(&past)),
        Err(Error::BoneRangeOutOfBounds { .. })
    ));
    let unnamed = [Instance::new(mesh)];
    assert!(matches!(
        renderer.render(&flat_scene(unit_camera()).with_instances(&unnamed)),
        Err(Error::BoneRangeOutOfBounds { .. })
    ));
    renderer
        .set_bones(&[Mat4::IDENTITY, Mat4::IDENTITY])
        .expect("bones");
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&past))
        .expect("the range fits now");

    assert!(matches!(
        renderer.upload_skinned_mesh(&mesh_data, &reads_bone_one[1..]),
        Err(Error::SkinLengthMismatch { .. })
    ));
}

#[test]
fn neighbours_sharing_a_diffuse_but_not_a_second_stage_keep_their_own() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mut half = quad();
    for vertex in &mut half.vertices {
        vertex.position = [vertex.position[0] * 0.4, vertex.position[1] * 0.4, 0.0];
    }
    let mesh = renderer.upload_mesh(&half).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("texture");
    let left = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.0)),
        ..Instance::new(mesh)
    };
    let right = Instance {
        texture2: Some(red),
        detail_combine: mint_render::DetailCombine::Detail,
        transform: Mat4::from_translation(Vec3::new(0.5, 0.0, 0.0)),
        ..left
    };
    let instances = [left, right, left];
    renderer
        .render(&flat_scene(unit_camera()).with_instances(&instances))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    assert_eq!(pixel(&rgba, SIZE, 16, SIZE / 2), [255, 255, 255, 255]);
    assert_eq!(
        pixel(&rgba, SIZE, 48, SIZE / 2),
        [255, 0, 0, 255],
        "the right quad's own second stage replaces its colour"
    );
}

#[test]
fn a_skinned_caster_shadows_where_its_cpu_twin_does() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let receiver_mesh = renderer.upload_mesh(&quad()).expect("upload");
    let map = renderer.create_shadow_map(256).expect("shadow map");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let towards_the_eye = Vec3::new(0.0, 0.0, 1.0);
    let light = directional_light_view_proj(towards_the_eye, Vec3::new(0.0, 0.0, -3.0), 4.0, 12.0);
    let receivers = [Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(receiver_mesh)
    }];
    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: towards_the_eye,
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    scene.shadow = Some(ShadowSample {
        lightness: 0.0,
        edge: ShadowEdge::Taps(1.0),
        ..ShadowSample::single(map, light, 0.002)
    });

    let (mesh, skin) = skinned_grid();
    let bones = two_bones();
    let at = Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0));
    let skinned = renderer
        .upload_skinned_mesh(&mesh, &skin)
        .expect("skinned upload");
    let twin = renderer
        .upload_mesh(&cpu_posed(&mesh, &skin, &bones))
        .expect("upload");
    let bind = renderer.upload_mesh(&mesh).expect("upload");
    renderer
        .set_bones(&[
            Mat4::from_translation(Vec3::new(5.0, 5.0, 0.0)),
            bones[0],
            bones[1],
        ])
        .expect("bones");

    let mut frame_with = |caster: Instance| {
        let casters = [caster];
        renderer
            .render_shadow_map(
                map,
                light,
                &Scene::new(unit_camera()).with_instances(&casters),
            )
            .expect("shadow pass");
        renderer
            .render(&scene.clone().with_instances(&receivers))
            .expect("render");
        renderer.read_pixels().expect("readback")
    };
    let from_gpu = frame_with(Instance {
        transform: at,
        bone_base: Some(1),
        ..Instance::new(skinned)
    });
    let from_cpu = frame_with(Instance {
        transform: at,
        ..Instance::new(twin)
    });
    let from_bind = frame_with(Instance {
        transform: at,
        ..Instance::new(bind)
    });

    let (posed_moved, _) = frame_difference(&from_bind, &from_cpu, 8);
    assert!(
        posed_moved > 20,
        "the pose must move the shadow ({posed_moved} pixels changed)"
    );
    let (over, largest) = frame_difference(&from_gpu, &from_cpu, 8);
    assert!(
        over <= 4,
        "{over} shadow pixels differ (largest {largest}) between the skinned caster \
         and its CPU twin"
    );
}

#[test]
fn a_turned_instance_is_lit_as_the_same_mesh_turned_before_upload() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let (mesh, _) = skinned_grid();
    let turn = two_bones()[1];
    let mut turned = mesh.clone();
    for vertex in &mut turned.vertices {
        vertex.position = turn
            .transform_point(Vec3::from_array(vertex.position))
            .to_array();
        vertex.normal = turn
            .transform_vector(Vec3::from_array(vertex.normal))
            .to_array();
        let t = turn.transform_vector(Vec3::new(
            vertex.tangent[0],
            vertex.tangent[1],
            vertex.tangent[2],
        ));
        vertex.tangent = [t.x, t.y, t.z, vertex.tangent[3]];
    }
    let plain = renderer.upload_mesh(&mesh).expect("upload");
    let baked = renderer.upload_mesh(&turned).expect("upload");
    let mut draw = |instance: Instance| {
        renderer
            .render(&side_lit_scene().with_instances(&[instance]))
            .expect("render");
        renderer.read_pixels().expect("readback")
    };
    let by_instance = draw(Instance {
        texture: Some(white),
        transform: turn,
        ..Instance::new(plain)
    });
    let by_upload = draw(Instance {
        texture: Some(white),
        ..Instance::new(baked)
    });
    let unturned = draw(Instance {
        texture: Some(white),
        ..Instance::new(plain)
    });
    let (moved, _) = frame_difference(&unturned, &by_upload, 8);
    assert!(
        moved > 200,
        "the turn must change the picture ({moved} pixels)"
    );
    let (over, largest) = frame_difference(&by_instance, &by_upload, 2);
    assert!(
        over <= 4,
        "{over} pixels differ by more than 2 levels (largest {largest})"
    );
}

#[test]
fn an_instance_takes_its_light_set_and_only_that_set_takes_the_cookie() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let grey = renderer
        .upload_texture(&solid_linear([128, 128, 128, 255]))
        .expect("upload");
    let mut scene = flat_scene(unit_camera());
    scene.ambient_color = [0.25; 3];
    scene.light_sets[0] = Some(LightSet {
        lights: [DirectionalLight::default(); 3],
        ambient_color: [0.75; 3],
        cookie: true,
    });
    scene.light_cookie = Some(LightCookie {
        texture: grey,
        transform: [0.0, 0.0, 0.0, 1.0],
        tint: [1.0; 3],
    });
    let mut centre = |light_set: u8| {
        let instances = [Instance {
            texture: Some(white),
            light_set,
            ..Instance::new(mesh)
        }];
        renderer
            .render(&scene.clone().with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)[0]
    };
    let own = centre(0);
    let objects = centre(1);
    let unset = centre(2);
    assert!(
        (63..=65).contains(&own),
        "the scene's own rig: 0.25 x 255, got {own}"
    );
    assert!(
        (94..=97).contains(&objects),
        "set 1 under the half-grey cookie: 0.75 x 128, got {objects}"
    );
    assert_eq!(
        unset, own,
        "a None set falls back to the scene's rig, with no cookie"
    );
}

fn shadowed_receiver_scene(renderer: &mut Renderer) -> (Scene<'static>, Instance, Instance) {
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let caster = Instance {
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0)),
        ..Instance::new(mesh)
    };
    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [0.0, 0.0, 0.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: Vec3::new(0.0, 0.0, 1.0),
        color: [1.0, 1.0, 1.0],
    };
    scene.clear_color = [0.0, 0.0, 0.0, 1.0];
    (scene, receiver, caster)
}

#[test]
fn a_point_takes_the_nearest_cascade_that_holds_it() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let (scene, receiver, caster) = shadowed_receiver_scene(&mut renderer);
    let map = renderer.create_shadow_cascades(256, 2).expect("cascades");
    assert_eq!(renderer.shadow_map_cascades(map), Some(2));
    let travel = Vec3::new(0.0, 0.0, 1.0);
    let wide = directional_light_view_proj(travel, Vec3::new(0.0, 0.0, -3.0), 3.0, 10.0);
    let aside = directional_light_view_proj(travel, Vec3::new(8.0, 0.0, -3.0), 1.0, 10.0);
    let centred = directional_light_view_proj(travel, Vec3::new(0.0, 0.0, -3.0), 1.5, 10.0);
    let casters = [caster];
    let receivers = [receiver];
    let centre = |renderer: &mut Renderer, near: Mat4, near_casters: &[Instance]| {
        renderer
            .render_shadow_cascade(
                map,
                0,
                near,
                &Scene::new(unit_camera()).with_instances(near_casters),
            )
            .expect("cascade 0");
        renderer
            .render_shadow_cascade(
                map,
                1,
                wide,
                &Scene::new(unit_camera()).with_instances(&casters),
            )
            .expect("cascade 1");
        let mut sample = ShadowSample::single(map, near, 0.002);
        sample.cascades[1] = Some(ShadowCascade::new(wide, 0.002));
        let mut frame = scene.clone().with_instances(&receivers);
        frame.shadow = Some(sample);
        renderer.render(&frame).expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)
    };
    let through = centre(&mut renderer, aside, &casters);
    assert!(
        through[0] < 32,
        "outside cascade 0, cascade 1's caster must shadow the centre: {through:?}"
    );
    let nearer = centre(&mut renderer, centred, &[]);
    assert!(
        nearer[0] > 200,
        "inside an empty cascade 0, the centre must be lit whatever cascade 1 says: {nearer:?}"
    );
    assert!(
        renderer
            .render_shadow_cascade(map, 2, wide, &Scene::new(unit_camera()))
            .is_err(),
        "a cascade past the ones the map was made with is refused"
    );
}

#[test]
fn the_tent_edge_shadows_the_caster_and_grades_its_edge() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let (mut scene, receiver, caster) = shadowed_receiver_scene(&mut renderer);
    let map = renderer.create_shadow_map(32).expect("shadow map");
    let light = directional_light_view_proj(
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(0.0, 0.0, -3.0),
        3.0,
        10.0,
    );
    let casters = [caster];
    renderer
        .render_shadow_map(
            map,
            light,
            &Scene::new(unit_camera()).with_instances(&casters),
        )
        .expect("shadow pass");
    scene.shadow = Some(ShadowSample {
        edge: ShadowEdge::Tent,
        ..ShadowSample::single(map, light, 0.002)
    });
    let receivers = [receiver];
    renderer
        .render(&scene.with_instances(&receivers))
        .expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    let middle = pixel(&rgba, SIZE, SIZE / 2, SIZE / 2);
    let corner = pixel(&rgba, SIZE, 3, 3);
    assert!(
        middle[0] < 32,
        "the caster's middle is shadowed: {middle:?}"
    );
    assert!(
        corner[0] > 200,
        "a corner well past the caster is lit: {corner:?}"
    );
    let row: Vec<u8> = (36..50)
        .map(|x| pixel(&rgba, SIZE, x, SIZE / 2)[0])
        .collect();
    assert!(
        row.iter().any(|value| (48..208).contains(value)),
        "the edge must grade rather than step: {row:?}"
    );
}

#[test]
fn contact_shadows_darken_what_the_depth_buffer_blocks() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid([255, 255, 255, 255]))
        .expect("texture");
    let receiver = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0))
            .mul_mat4(Mat4::from_scale(Vec3::splat(3.0))),
        ..Instance::new(mesh)
    };
    let blocker = Instance {
        texture: Some(white),
        transform: Mat4::from_translation(Vec3::new(0.5, 0.0, -1.5))
            .mul_mat4(Mat4::from_scale(Vec3::splat(0.25))),
        ..Instance::new(mesh)
    };
    let mut scene = Scene::new(unit_camera());
    scene.ambient_color = [1.0, 1.0, 1.0];
    scene.lights = [DirectionalLight::default(); 3];
    scene.lights[0] = DirectionalLight {
        direction: Vec3::new(-1.0, 0.0, -1.0),
        color: [0.0, 0.0, 0.0],
    };
    let instances = [receiver, blocker];
    let mut read = |contact: Option<ContactShadows>| {
        let mut frame = scene.clone().with_instances(&instances);
        frame.contact_shadows = contact;
        renderer.render(&frame).expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        (
            pixel(&rgba, SIZE, SIZE / 2, SIZE / 2),
            pixel(&rgba, SIZE, 8, SIZE / 2),
        )
    };
    let (plain_centre, plain_left) = read(None);
    let (centre, left) = read(Some(ContactShadows {
        length: 2.0,
        steps: 32,
        thickness: 0.5,
        lightness: 0.25,
    }));
    assert!(
        plain_centre[0] > 200,
        "without the pass, nothing darkens: {plain_centre:?}"
    );
    assert!(
        centre[0] < 96,
        "the ray from the centre passes behind the blocker: {centre:?}"
    );
    assert_eq!(
        left, plain_left,
        "a ray with nothing in its way changes nothing"
    );
}

fn post_pixel(post: PostEffects<'_>, clear: [f64; 4]) -> Option<[u8; 4]> {
    let mut renderer = renderer(SIZE)?;
    let mut scene = flat_scene(unit_camera());
    scene.clear_color = clear;
    scene.post = post;
    renderer.render(&scene).expect("render");
    Some(pixel(
        &renderer.read_pixels().expect("readback"),
        SIZE,
        32,
        32,
    ))
}

#[test]
fn the_glow_keeps_what_is_above_the_threshold_and_screen_adds_it_back() {
    let taps = [GlowTap {
        offset: 0.0,
        weight: 1.0,
    }];
    let post = PostEffects {
        glow: Some(Glow {
            bright_offset: [-0.5; 3],
            texture_size: 4,
            taps: &taps,
            intensity: 1.0,
        }),
        lut: None,
    };
    let Some(got) = post_pixel(post, [0.9, 0.5, 0.1, 1.0]) else {
        return;
    };
    let expected = [240u8, 128, 26];
    for (channel, (&g, &e)) in got.iter().zip(&expected).enumerate() {
        assert!(g.abs_diff(e) <= 3, "channel {channel}: got {g}, want {e}");
    }
}

#[test]
fn a_lookup_volume_replaces_the_frame_by_its_blend_factor() {
    let mut renderer_opt = renderer(SIZE);
    let Some(renderer) = renderer_opt.as_mut() else {
        return;
    };
    let volume = renderer
        .upload_color_volume(2, &[200, 100, 0, 255].repeat(8))
        .expect("upload");
    let mut scene = flat_scene(unit_camera());
    scene.clear_color = [0.0, 0.0, 1.0, 1.0];
    scene.post = PostEffects {
        glow: None,
        lut: Some(ColorLut { volume, blend: 0.5 }),
    };
    renderer.render(&scene).expect("render");
    let got = pixel(&renderer.read_pixels().expect("readback"), SIZE, 32, 32);
    let expected = [100u8, 50, 128];
    for (channel, (&g, &e)) in got.iter().zip(&expected).enumerate() {
        assert!(g.abs_diff(e) <= 3, "channel {channel}: got {g}, want {e}");
    }
}

#[test]
fn distance_fog_covers_a_far_surface_and_fades_an_additive_one_to_black() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let mut scene = flat_scene(unit_camera());
    scene.fog = Some(DistanceFog {
        color: [0.0, 1.0, 0.0],
        start: 0.25,
        end: 0.5,
    });
    let mut centre = |instance: Instance| {
        let instances = [instance];
        renderer
            .render(&scene.clone().with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)
    };
    let opaque = Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    };
    assert_eq!(centre(opaque), [0, 255, 0, 255], "takes the fog colour");
    let additive = Instance {
        blend: Blend::Additive,
        ..opaque
    };
    assert_eq!(centre(additive), [0, 0, 0, 255], "adds nothing once fogged");
    let exempt = Instance {
        fog: false,
        ..opaque
    };
    assert_eq!(
        centre(exempt),
        [255, 255, 255, 255],
        "an exempt instance is not fogged"
    );
}

#[test]
fn the_visibility_mask_multiplies_after_the_fog() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let white = renderer
        .upload_texture(&solid_linear([255, 255, 255, 255]))
        .expect("upload");
    let grey = renderer
        .upload_texture(&solid_linear([128, 128, 128, 128]))
        .expect("upload");
    let mut scene = flat_scene(unit_camera());
    scene.fog = Some(DistanceFog {
        color: [0.0, 1.0, 0.0],
        start: 0.25,
        end: 0.5,
    });
    scene.visibility_mask = Some(VisibilityMask {
        texture: grey,
        transform: [1.0, 1.0, 0.0, 0.0],
        strength: 1.0,
    });
    let mut centre = |instance: Instance| {
        let instances = [instance];
        renderer
            .render(&scene.clone().with_instances(&instances))
            .expect("render");
        let rgba = renderer.read_pixels().expect("readback");
        pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)
    };
    let plain = Instance {
        texture: Some(white),
        ..Instance::new(mesh)
    };
    assert_eq!(
        centre(plain),
        [0, 255, 0, 255],
        "an unmasked instance is not masked"
    );
    let masked = centre(Instance {
        visibility_mask: true,
        ..plain
    });
    assert!(
        masked[0] == 0 && masked[1].abs_diff(128) <= 2 && masked[3] == 255,
        "half the fog colour: {masked:?}"
    );
}

#[test]
fn a_rotated_quad_reads_its_rect_turned_a_quarter_turn() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let corners = renderer
        .upload_texture(&TextureData::single(
            PixelFormat::Rgba8Unorm,
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        ))
        .expect("upload");
    let quad = UiQuad {
        uv_rotated_cw: true,
        ..UiQuad::new(0.0, 0.0, size, size).with_texture(corners, [0.0, 0.0, 1.0, 1.0])
    };
    let black = [0.0, 0.0, 0.0, 1.0];
    let (lo, hi) = (SIZE / 4, SIZE * 3 / 4);
    let at = |renderer: &mut Renderer, x, y| composite_at(renderer, black, &[quad], [x, y]);
    let top_left = at(&mut renderer, lo, lo);
    let top_right = at(&mut renderer, hi, lo);
    let bottom_left = at(&mut renderer, lo, hi);
    let bottom_right = at(&mut renderer, hi, hi);
    assert!(
        top_left[1] > 128 && top_left[0] < 128,
        "green: {top_left:?}"
    );
    assert!(
        top_right.iter().take(3).all(|&c| c > 128),
        "white: {top_right:?}"
    );
    assert!(
        bottom_left[0] > 128 && bottom_left[1] < 128,
        "red: {bottom_left:?}"
    );
    assert!(
        bottom_right[2] > 128 && bottom_right[0] < 128,
        "blue: {bottom_right:?}"
    );
}

fn target_then_frame(
    renderer: &mut Renderer,
    target_scene: &Scene<'_>,
    frame_instances: &[Instance],
    between: impl FnOnce(&mut Renderer),
) -> [u8; 4] {
    let target = renderer.create_render_target(8, 8).expect("target");
    let texture = renderer.render_target_texture(target).expect("texture");
    renderer
        .render_to_target(target, target_scene)
        .expect("target pass");
    between(renderer);
    #[allow(clippy::cast_possible_truncation)]
    let size = f64::from(SIZE) as f32;
    let layer = [UiQuad::new(0.0, 0.0, size, size).with_texture(texture, [0.0, 0.0, 1.0, 1.0])];
    let away = Camera {
        target: Vec3::new(0.0, 0.0, 2.0),
        ..unit_camera()
    };
    let scene = flat_scene(away)
        .with_instances(frame_instances)
        .with_ui_quads(&layer);
    renderer.render(&scene).expect("render");
    let rgba = renderer.read_pixels().expect("readback");
    pixel(&rgba, SIZE, SIZE / 2, SIZE / 2)
}

#[test]
fn passes_submitted_together_keep_their_own_uniforms() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");
    let target_instances = [Instance {
        texture: Some(red),
        ..Instance::new(mesh)
    }];
    let frame_instances = [Instance {
        texture: Some(red),
        transform: Mat4::from_translation(Vec3::new(100.0, 0.0, 0.0)),
        ..Instance::new(mesh)
    }];
    let target_scene = flat_scene(unit_camera()).with_instances(&target_instances);
    let shown = target_then_frame(&mut renderer, &target_scene, &frame_instances, |_| {});
    assert_eq!(shown, [255, 0, 0, 255], "the target holds its own quad");
    assert_eq!(renderer.submits(), (1, 0), "one submit for both passes");
}

#[test]
fn a_buffer_write_between_passes_is_seen_only_by_later_passes() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let mesh = renderer.upload_mesh(&quad()).expect("upload");
    let red = renderer
        .upload_texture(&solid([255, 0, 0, 255]))
        .expect("upload");
    let instances = [Instance {
        texture: Some(red),
        ..Instance::new(mesh)
    }];
    let target_scene = flat_scene(unit_camera()).with_instances(&instances);
    let mut moved = quad();
    for vertex in &mut moved.vertices {
        vertex.position[0] += 100.0;
    }
    let shown = target_then_frame(&mut renderer, &target_scene, &[], |renderer| {
        renderer.update_mesh_vertices(mesh, &moved).expect("update");
    });
    assert_eq!(
        shown,
        [255, 0, 0, 255],
        "the target was drawn before the move"
    );
    assert_eq!(renderer.submits(), (1, 0));
}

#[test]
fn a_stale_texture_on_a_hidden_instance_is_still_an_error() {
    let Some(mut first) = renderer(SIZE) else {
        return;
    };
    let Some(mut second) = renderer(SIZE) else {
        return;
    };
    let _ = first
        .upload_texture(&solid([0, 0, 0, 255]))
        .expect("upload");
    let stranger = first
        .upload_texture(&solid([0, 0, 0, 255]))
        .expect("upload");
    let mesh = second.upload_mesh(&quad()).expect("upload");
    let instances = [Instance {
        texture: Some(stranger),
        visible: false,
        ..Instance::new(mesh)
    }];
    let scene = flat_scene(unit_camera()).with_instances(&instances);
    assert!(matches!(second.render(&scene), Err(Error::UnknownHandle)));
    let target = second.create_render_target(8, 8).expect("target");
    assert!(matches!(
        second.render_to_target(target, &scene),
        Err(Error::UnknownHandle)
    ));
}

fn retention_frame<'a>(
    texture: mint_render::TextureHandle,
    shapes: &'a [Vec<[f32; 2]>; 3],
    dx: f32,
    retain: bool,
) -> Vec<UiGeometry<'a>> {
    let keyed = |geometry: UiGeometry<'a>, key: u64| {
        if retain {
            geometry.with_retention(key)
        } else {
            geometry
        }
    };
    vec![
        keyed(
            UiGeometry::new()
                .with_triangles(&shapes[0])
                .with_texture(texture, [[1.0, 0.0], [0.0, 1.0]], [0.0, 0.0])
                .with_transform([[12.0, 0.0], [0.0, 12.0]], [dx, 2.0])
                .with_color([1.0, 0.5, 1.0, 1.0])
                .with_clip([0.0, 0.0, 40.0 + dx, 64.0]),
            11,
        ),
        keyed(
            UiGeometry::new()
                .with_triangles(&shapes[1])
                .with_lines(&shapes[2])
                .with_transform([[0.8, 0.3], [-0.3, 0.8]], [10.0 + dx, 30.0])
                .with_color([0.0, 1.0, 0.0, 0.75])
                .with_color_add([0.1, 0.0, 0.0, 0.0]),
            12,
        ),
        keyed(
            UiGeometry::new()
                .with_lines(&shapes[2])
                .with_transform([[1.0, 0.0], [0.0, 1.0]], [4.0, 50.0 - dx])
                .with_color([1.0, 1.0, 0.0, 1.0]),
            13,
        ),
    ]
}

#[test]
fn retained_ui_geometry_draws_as_the_same_geometry_unretained_whatever_its_placement() {
    let Some(mut renderer) = renderer(SIZE) else {
        return;
    };
    let texture = renderer
        .upload_texture(&quadrant_texture())
        .expect("upload");
    let shapes = [
        vec![
            [0.0, 0.0],
            [4.0, 0.0],
            [0.0, 4.0],
            [4.0, 0.0],
            [4.0, 4.0],
            [0.0, 4.0],
        ],
        vec![[0.0, 0.0], [32.0, 0.0], [0.0, 32.0], [1.0, 1.0]],
        vec![[0.0, 0.0], [40.0, 24.0], [3.0, 3.0]],
    ];
    for dx in [0.0, 6.5, 0.0, 13.0] {
        let plain = retention_frame(texture, &shapes, dx, false);
        renderer.render(&ui_geometry_scene(&plain)).expect("render");
        let expected = renderer.read_pixels().expect("readback");
        let retained = retention_frame(texture, &shapes, dx, true);
        renderer
            .render(&ui_geometry_scene(&retained))
            .expect("render");
        let actual = renderer.read_pixels().expect("readback");
        assert!(
            actual.iter().any(|byte| *byte != 0),
            "the frame must draw something"
        );
        assert_eq!(actual, expected, "dx = {dx}");
    }
}
