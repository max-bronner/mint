#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used
)]

use mint_render::{Error as RenderError, Renderer, TextureData};
use mint_sage_map::{BlendTileData, HeightMapData, TextureClass};
use mint_sage_terrain::present::{upload_class_textures, upload_terrain_plain};
use mint_sage_terrain::{ClassTextures, Terrain};

fn renderer() -> Option<Renderer> {
    match Renderer::offscreen(64, 64) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::NoAdapter) => {
            eprintln!(
                "SKIPPED: no graphics adapter on this machine, so nothing in \
                 tests/present.rs was actually verified"
            );
            None
        }
        Err(error) => panic!("creating an offscreen renderer failed: {error}"),
    }
}

fn flat_scene() -> (HeightMapData, BlendTileData) {
    let (width, height) = (3u32, 3u32);
    let height_map = HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; (width * height) as usize],
    };
    let blend_tile = BlendTileData {
        version: 18,
        width,
        height,
        cell_count: width * height,
        tile_index: vec![0; (width * height) as usize],
        blend_index: vec![0; (width * height) as usize],
        extra_blend_index: vec![0; (width * height) as usize],
        cliff_info_index: vec![0; (width * height) as usize],
        unknown_cell_byte: None,
        bit_planes: Vec::new(),
        num_bitmap_tiles: 16,
        texture_classes: vec![TextureClass {
            first_tile: 0,
            num_tiles: 16,
            width: 4,
            name: "test_class".to_owned(),
        }],
        num_edge_tiles: 0,
        edge_texture_classes: Vec::new(),
        blend_records: Vec::new(),
        cliff_records: Vec::new(),
    };
    (height_map, blend_tile)
}

fn class_textures() -> ClassTextures {
    let pixels = vec![200u8; 4 * 4 * 4];
    ClassTextures {
        textures: vec![TextureData::rgba8(4, 4, pixels)],
        failures: Vec::new(),
        alpha_discarded: Vec::new(),
    }
}

#[test]
fn every_base_part_uploads_one_mesh_and_one_textured_instance() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (height_map, blend_tile) = flat_scene();
    let terrain = Terrain::build(&height_map, &blend_tile, 1.0);
    assert!(
        !terrain.chunks.is_empty(),
        "a 3x3 grid must build at least one chunk"
    );
    let part_count = terrain
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.parts)
        .filter(|part| part.layer == mint_sage_terrain::TerrainLayer::Base)
        .count();
    assert!(part_count > 0, "a flat, one-class map must have base parts");

    let loaded = class_textures();
    let textures = upload_class_textures(&mut renderer, &loaded).expect("texture upload");
    assert_eq!(textures.len(), 1);
    assert!(textures[0].is_some(), "the one class texture must upload");

    let (handles, instances) =
        upload_terrain_plain(&mut renderer, &terrain, &textures).expect("mesh upload");
    assert_eq!(handles.len(), part_count);
    assert_eq!(instances.len(), part_count);
    for instance in &instances {
        assert_eq!(
            instance.texture, textures[0],
            "every part in the one class must draw with that class's texture"
        );
        assert!(instance.cull, "terrain instances cull back faces");
    }
}

#[test]
fn a_class_whose_texture_failed_to_load_leaves_the_part_untextured_not_dropped() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (height_map, blend_tile) = flat_scene();
    let terrain = Terrain::build(&height_map, &blend_tile, 1.0);

    let loaded = ClassTextures {
        textures: vec![None],
        failures: Vec::new(),
        alpha_discarded: Vec::new(),
    };
    let textures = upload_class_textures(&mut renderer, &loaded).expect("texture upload");
    assert_eq!(textures, vec![None]);

    let (handles, instances) =
        upload_terrain_plain(&mut renderer, &terrain, &textures).expect("mesh upload");
    assert!(!handles.is_empty(), "geometry still uploads");
    assert!(
        instances.iter().all(|instance| instance.texture.is_none()),
        "an unresolved class draws pale rather than being dropped"
    );
}
