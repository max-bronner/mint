use super::*;
use mint_sage_map::{BlendRecord, CliffRecord, TextureClass};

pub(crate) fn flat_height_map(width: u32, height: u32, value: u16) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![value; (width * height) as usize],
    }
}

pub(crate) fn blend_tile(
    width: u32,
    height: u32,
    tile_index: Vec<u16>,
    classes: Vec<TextureClass>,
) -> BlendTileData {
    let cells = (width * height) as usize;
    BlendTileData {
        version: 18,
        width,
        height,
        cell_count: width * height,
        tile_index,
        blend_index: vec![0; cells],
        extra_blend_index: vec![0; cells],
        cliff_info_index: vec![0; cells],
        unknown_cell_byte: None,
        bit_planes: Vec::new(),
        num_bitmap_tiles: classes.iter().map(|c| c.num_tiles).sum(),
        texture_classes: classes,
        num_edge_tiles: 0,
        edge_texture_classes: Vec::new(),
        blend_records: Vec::new(),
        cliff_records: Vec::new(),
    }
}

pub(crate) fn one_class(num_tiles: i32, width: i32) -> TextureClass {
    TextureClass {
        first_tile: 0,
        num_tiles,
        width,
        name: "grass".to_owned(),
    }
}

#[test]
fn every_quad_is_covered_exactly_once_across_chunk_seams() {
    let (width, height) = (130u32, 70u32);
    let extents = chunk_extents(width, height);
    assert!(!extents.is_empty());

    let quads_x = (width - 1) as usize;
    let quads_y = (height - 1) as usize;
    let mut covered = vec![vec![0u32; quads_x]; quads_y];
    for extent in &extents {
        for qy in extent.y0..extent.y1 {
            for qx in extent.x0..extent.x1 {
                covered[qy as usize][qx as usize] += 1;
            }
        }
    }
    for (y, row) in covered.iter().enumerate() {
        for (x, &count) in row.iter().enumerate() {
            assert_eq!(count, 1, "quad ({x}, {y}) covered {count} times");
        }
    }
}

#[test]
fn a_grid_thinner_than_two_cells_on_either_axis_has_no_chunks() {
    assert!(chunk_extents(0, 0).is_empty());
    assert!(chunk_extents(1, 1).is_empty());
    assert!(chunk_extents(1, 100).is_empty());
    assert!(chunk_extents(100, 1).is_empty());
    assert_eq!(chunk_extents(2, 2).len(), 1);
}

#[test]
fn a_single_quad_gets_its_own_four_vertices_and_its_cells_uv_rectangle() {
    let hm = flat_height_map(2, 2, 0);
    let bt = blend_tile(2, 2, vec![(6 << 2) | 3; 4], vec![one_class(16, 4)]);
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);

    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].texture_class, Some(0));
    let mesh = &parts[0].mesh;
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.indices, vec![0, 1, 2, 1, 3, 2]);

    let positions: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    assert_eq!(
        positions,
        vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [10.0, 10.0, 0.0]
        ]
    );
    let uvs: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.uv).collect();
    assert_eq!(
        uvs,
        vec![[0.625, 0.625], [0.75, 0.625], [0.625, 0.5], [0.75, 0.5]]
    );
}

#[test]
fn a_quad_folds_along_a_diagonal_step_rather_than_across_it() {
    let fold_of = |samples: Vec<u16>| {
        let hm = HeightMapData {
            width: 2,
            height: 2,
            border_size: 0,
            boundaries: Vec::new(),
            samples,
        };
        let bt = blend_tile(2, 2, vec![0; 4], vec![one_class(16, 4)]);
        let layout = TileLayout::new(&bt.texture_classes);
        let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);
        parts[0].mesh.indices.clone()
    };
    assert_eq!(fold_of(vec![50, 100, 0, 50]), vec![0, 1, 3, 0, 3, 2]);
    assert_eq!(fold_of(vec![0, 50, 50, 100]), vec![0, 1, 2, 1, 3, 2]);
    assert_eq!(fold_of(vec![0, 0, 0, 0]), vec![0, 1, 2, 1, 3, 2]);
}

#[test]
fn winding_matches_the_renderers_front_face() {
    let hm = HeightMapData {
        width: 3,
        height: 3,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![50, 100, 0, 0, 50, 100, 100, 0, 50],
    };
    let bt = blend_tile(3, 3, vec![0; 9], vec![one_class(16, 4)]);
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(3, 3)[0], 1.0, None);

    for part in &parts {
        for tri in part.mesh.indices.chunks_exact(3) {
            let [i0, i1, i2] = [tri[0], tri[1], tri[2]];
            let p0 = part.mesh.vertices[i0 as usize].position;
            let p1 = part.mesh.vertices[i1 as usize].position;
            let p2 = part.mesh.vertices[i2 as usize].position;
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let cross_z = e1[0] * e2[1] - e1[1] * e2[0];
            assert!(cross_z > 0.0, "triangle {tri:?} winds the wrong way");
        }
    }
}

#[test]
fn cells_are_split_by_class_with_every_quad_kept_exactly_once() {
    let hm = flat_height_map(3, 3, 0);
    let tiles = vec![0 << 2, 17 << 2, 0, 0, 999 << 2, 0, 0, 0, 0];
    let classes = vec![
        TextureClass {
            first_tile: 0,
            num_tiles: 16,
            width: 4,
            name: "a".to_owned(),
        },
        TextureClass {
            first_tile: 16,
            num_tiles: 16,
            width: 4,
            name: "b".to_owned(),
        },
    ];
    let bt = blend_tile(3, 3, tiles, classes);
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(3, 3)[0], 1.0, None);

    let quads: usize = parts.iter().map(|p| p.mesh.indices.len() / 6).sum();
    assert_eq!(quads, 4, "a 3x3 grid has exactly four quads");
    let keys: Vec<Option<usize>> = parts.iter().map(|p| p.texture_class).collect();
    assert_eq!(keys, vec![Some(0), Some(1), None], "untextured part last");
    assert_eq!(parts[0].mesh.indices.len() / 6, 2);
    assert_eq!(parts[1].mesh.indices.len() / 6, 1);
    assert_eq!(parts[2].mesh.indices.len() / 6, 1);
}

#[test]
fn overlays_follow_the_base_layer_in_order_and_carry_their_corner_alpha() {
    let hm = flat_height_map(2, 2, 0);
    let mut bt = blend_tile(2, 2, vec![0; 4], vec![one_class(16, 4)]);
    bt.blend_index = vec![1, 0, 0, 0];
    bt.extra_blend_index = vec![2, 0, 0, 0];
    bt.blend_records = vec![
        BlendRecord {
            tile_index: 4 << 2,
            horizontal: true,
            vertical: false,
            right_diagonal: false,
            left_diagonal: false,
            inverted: false,
            long_diagonal: 0,
            custom_blend_edge_class: -1,
        },
        BlendRecord {
            tile_index: 8 << 2,
            horizontal: false,
            vertical: true,
            right_diagonal: false,
            left_diagonal: false,
            inverted: false,
            long_diagonal: 0,
            custom_blend_edge_class: -1,
        },
    ];
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);

    let layers: Vec<TerrainLayer> = parts.iter().map(|p| p.layer).collect();
    assert_eq!(
        layers,
        vec![
            TerrainLayer::Base,
            TerrainLayer::Blend,
            TerrainLayer::ExtraBlend
        ]
    );

    let alpha = |part: &TerrainPart| -> Vec<f32> {
        part.mesh.vertices.iter().map(|v| v.color[3]).collect()
    };
    assert_eq!(alpha(&parts[0]), vec![1.0; 4], "the base layer is opaque");
    assert_eq!(
        alpha(&parts[1]),
        vec![0.0, 1.0, 0.0, 1.0],
        "a horizontal blend fades in toward +x"
    );
    assert_eq!(
        alpha(&parts[2]),
        vec![0.0, 0.0, 1.0, 1.0],
        "a vertical blend fades in toward +y"
    );
    assert_ne!(parts[1].mesh.vertices[0].uv, parts[0].mesh.vertices[0].uv);
    assert_ne!(parts[2].mesh.vertices[0].uv, parts[1].mesh.vertices[0].uv);
}

fn two_classes() -> Vec<TextureClass> {
    vec![
        one_class(16, 4),
        TextureClass {
            first_tile: 16,
            num_tiles: 16,
            width: 4,
            name: "cliff".to_owned(),
        },
    ]
}

#[test]
fn a_cliff_cell_draws_the_records_coordinates_rather_than_its_tile_rectangle() {
    let hm = flat_height_map(2, 2, 0);
    let mut bt = blend_tile(2, 2, vec![20 << 2; 4], two_classes());
    bt.cliff_info_index = vec![1, 0, 0, 0];
    bt.cliff_records = vec![CliffRecord {
        tile_index: 24 << 2,
        tex_coords: [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)],
        flip: 0,
        unknown: 0,
    }];
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);

    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].texture_class, Some(1));
    let uvs: Vec<[f32; 2]> = parts[0].mesh.vertices.iter().map(|v| v.uv).collect();
    assert_eq!(uvs, vec![[0.1, 0.2], [0.3, 0.4], [0.7, 0.8], [0.5, 0.6]]);
}

#[test]
fn a_cliff_cell_draws_no_blend_overlay() {
    let hm = flat_height_map(2, 2, 0);
    let mut bt = blend_tile(2, 2, vec![20 << 2; 4], two_classes());
    bt.cliff_info_index = vec![1, 0, 0, 0];
    bt.cliff_records = vec![CliffRecord {
        tile_index: 24 << 2,
        tex_coords: [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)],
        flip: 0,
        unknown: 0,
    }];
    bt.blend_index = vec![1, 0, 0, 0];
    bt.extra_blend_index = vec![1, 0, 0, 0];
    bt.blend_records = vec![BlendRecord {
        tile_index: 4 << 2,
        horizontal: true,
        vertical: false,
        right_diagonal: false,
        left_diagonal: false,
        inverted: false,
        long_diagonal: 0,
        custom_blend_edge_class: -1,
    }];
    let layout = TileLayout::new(&bt.texture_classes);
    let extent = chunk_extents(2, 2)[0];

    let parts = build_chunk_parts(&hm, &bt, &layout, &extent, 1.0, None);
    assert_eq!(
        parts.iter().map(|p| p.layer).collect::<Vec<_>>(),
        vec![TerrainLayer::Base],
        "the cliff face draws alone"
    );

    let mut rejected = bt.clone();
    rejected.tile_index = vec![4 << 2; 4];
    let rejected_layout = TileLayout::new(&rejected.texture_classes);
    let parts = build_chunk_parts(&hm, &rejected, &rejected_layout, &extent, 1.0, None);
    assert!(
        parts.iter().any(|p| p.layer == TerrainLayer::Blend),
        "a cell whose record the gate rejects keeps its overlay"
    );
}

#[test]
fn a_gate_rejected_cell_blends_in_the_maps_cliff_texture() {
    let hm = flat_height_map(2, 2, 0);
    let mut bt = blend_tile(2, 2, vec![0; 4], two_classes());
    bt.cliff_info_index = vec![1, 0, 0, 0];
    bt.cliff_records = vec![CliffRecord {
        tile_index: 20 << 2,
        tex_coords: [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)],
        flip: 0,
        unknown: 0,
    }];
    bt.blend_index = vec![1, 0, 0, 0];
    bt.blend_records = vec![BlendRecord {
        tile_index: 0,
        horizontal: true,
        vertical: false,
        right_diagonal: false,
        left_diagonal: false,
        inverted: false,
        long_diagonal: 0,
        custom_blend_edge_class: -1,
    }];
    let layout = TileLayout::new(&bt.texture_classes);
    let extent = chunk_extents(2, 2)[0];
    let map_cliff = crate::cliff::map_cliff_class(&layout, &bt.cliff_records);
    assert_eq!(map_cliff, Some(1), "the map's cliff texture is class 1");

    let parts = build_chunk_parts(&hm, &bt, &layout, &extent, 1.0, map_cliff);
    let overlay = parts
        .iter()
        .find(|p| p.layer == TerrainLayer::Blend)
        .expect("the rejected cell keeps its overlay");
    assert_eq!(
        overlay.texture_class,
        Some(1),
        "the overlay draws the map's cliff texture, not the blend record's class 0"
    );

    let plain = build_chunk_parts(&hm, &bt, &layout, &extent, 1.0, None);
    let overlay = plain
        .iter()
        .find(|p| p.layer == TerrainLayer::Blend)
        .expect("still there");
    assert_eq!(overlay.texture_class, Some(0));
}

#[test]
fn a_cell_without_a_cliff_record_keeps_its_overlay() {
    let hm = flat_height_map(2, 2, 0);
    let mut bt = blend_tile(2, 2, vec![0; 4], vec![one_class(16, 4)]);
    bt.blend_index = vec![1, 0, 0, 0];
    bt.blend_records = vec![BlendRecord {
        tile_index: 4 << 2,
        horizontal: true,
        vertical: false,
        right_diagonal: false,
        left_diagonal: false,
        inverted: false,
        long_diagonal: 0,
        custom_blend_edge_class: -1,
    }];
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);
    assert_eq!(
        parts.iter().map(|p| p.layer).collect::<Vec<_>>(),
        vec![TerrainLayer::Base, TerrainLayer::Blend]
    );
}

#[test]
fn an_out_of_range_tile_index_keeps_its_geometry_untextured() {
    let hm = flat_height_map(2, 2, 0);
    let bt = blend_tile(2, 2, vec![9999 << 2; 4], vec![one_class(16, 4)]);
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(2, 2)[0], 1.0, None);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].texture_class, None);
    assert_eq!(parts[0].mesh.vertices.len(), 4);
}

#[test]
fn a_short_tile_index_array_is_untextured_rather_than_a_panic() {
    let hm = flat_height_map(4, 4, 0);
    let bt = blend_tile(4, 4, vec![0; 3], vec![one_class(16, 4)]);
    let layout = TileLayout::new(&bt.texture_classes);
    let parts = build_chunk_parts(&hm, &bt, &layout, &chunk_extents(4, 4)[0], 1.0, None);
    let quads: usize = parts.iter().map(|p| p.mesh.indices.len() / 6).sum();
    assert_eq!(quads, 9);
}

#[test]
fn a_flat_heightmap_has_a_straight_up_normal_at_any_scale() {
    let hm = flat_height_map(3, 3, 500);
    for scale in [0.1, 1.0, 25.0] {
        let n = normal_at(&hm, 1, 1, scale);
        assert!((n[0]).abs() < 1e-6, "n {n:?} at scale {scale}");
        assert!((n[1]).abs() < 1e-6, "n {n:?} at scale {scale}");
        assert!((n[2] - 1.0).abs() < 1e-6, "n {n:?} at scale {scale}");
    }
}

#[test]
fn a_sloped_heightmap_tilts_further_as_the_scale_grows() {
    let mut hm = flat_height_map(3, 3, 0);
    hm.samples = vec![0, 100, 200, 0, 100, 200, 0, 100, 200];
    let low = normal_at(&hm, 1, 1, 1.0);
    let high = normal_at(&hm, 1, 1, 4.0);

    assert!(low[0] < 0.0);
    assert!(high[0] < 0.0);
    assert!(high[2] < low[2], "low {low:?} high {high:?}");
    assert!(high[0] < low[0], "low {low:?} high {high:?}");
    assert!(low[1].abs() < 1e-6 && high[1].abs() < 1e-6);
}

#[test]
fn a_zero_size_heightmap_produces_no_geometry_without_panicking() {
    let hm = flat_height_map(0, 0, 0);
    assert!(chunk_extents(hm.width, hm.height).is_empty());
    assert_eq!(hm.sample(0, 0), None);
}
