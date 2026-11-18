use mint_render::{MeshData, Vertex};
use mint_sage_map::{cell_to_world, BlendTileData, HeightMapData, MAP_XY_FACTOR};

use crate::blend::cell_blend;
use crate::cliff::{cell_cliff, overlay_cliff_uv};
use crate::tile::TileLayout;

const CHUNK_CELLS: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkExtent {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TerrainLayer {
    Base,
    Blend,
    ExtraBlend,
}

#[derive(Debug, Clone)]
pub struct TerrainPart {
    pub layer: TerrainLayer,
    pub texture_class: Option<usize>,
    pub mesh: MeshData,
}

pub(crate) fn chunk_extents(width: u32, height: u32) -> Vec<ChunkExtent> {
    let mut out = Vec::new();
    if width < 2 || height < 2 {
        return out;
    }
    let max_x = width.saturating_sub(1);
    let max_y = height.saturating_sub(1);

    let mut y0 = 0u32;
    while y0 < max_y {
        let y1 = y0.saturating_add(CHUNK_CELLS).min(max_y);
        let mut x0 = 0u32;
        while x0 < max_x {
            let x1 = x0.saturating_add(CHUNK_CELLS).min(max_x);
            out.push(ChunkExtent { x0, y0, x1, y1 });
            x0 = x1;
        }
        y0 = y1;
    }
    out
}

pub(crate) fn cell_i32(cell: u32) -> i32 {
    i32::try_from(cell).unwrap_or(i32::MAX)
}

fn normal_at(height_map: &HeightMapData, x: u32, y: u32, vertical_scale: f32) -> [f32; 3] {
    let max_x = height_map.width.saturating_sub(1);
    let max_y = height_map.height.saturating_sub(1);
    let x0 = x.saturating_sub(1);
    let x1 = x.saturating_add(1).min(max_x);
    let y0 = y.saturating_sub(1);
    let y1 = y.saturating_add(1).min(max_y);

    let world_height =
        |cx: u32, cy: u32| f32::from(height_map.sample(cx, cy).unwrap_or(0)) * vertical_scale;

    let dx_cells = x1.saturating_sub(x0);
    let dy_cells = y1.saturating_sub(y0);

    let slope_x = if dx_cells > 0 {
        (world_height(x1, y) - world_height(x0, y)) / (dx_cells as f32 * MAP_XY_FACTOR)
    } else {
        0.0
    };
    let slope_y = if dy_cells > 0 {
        (world_height(x, y1) - world_height(x, y0)) / (dy_cells as f32 * MAP_XY_FACTOR)
    } else {
        0.0
    };

    mint_render::Vec3::new(-slope_x, -slope_y, 1.0)
        .normalize()
        .to_array()
}

fn grid_vertex(height_map: &HeightMapData, x: u32, y: u32, scale: f32) -> ([f32; 3], [f32; 3]) {
    let sample = height_map.sample(x, y).unwrap_or(0);
    let position = [
        cell_to_world(cell_i32(x), height_map.border_size),
        cell_to_world(cell_i32(y), height_map.border_size),
        f32::from(sample) * scale,
    ];
    (position, normal_at(height_map, x, y, scale))
}

pub(crate) fn build_chunk_parts(
    height_map: &HeightMapData,
    blend_tile: &BlendTileData,
    layout: &TileLayout,
    extent: &ChunkExtent,
    vertical_scale: f32,
    map_cliff_class: Option<usize>,
) -> Vec<TerrainPart> {
    let mut groups: Vec<((TerrainLayer, Option<usize>), MeshData)> = Vec::new();

    for y in extent.y0..extent.y1 {
        for x in extent.x0..extent.x1 {
            let index = cell_index(blend_tile, x, y);
            let cell = index
                .and_then(|index| blend_tile.tile_index.get(index).copied())
                .and_then(|value| layout.cell(value));
            let cliff_index = index
                .and_then(|index| blend_tile.cliff_info_index.get(index).copied())
                .filter(|value| *value != 0);
            let cliff = cliff_index.and_then(|value| {
                cell_cliff(
                    layout,
                    &blend_tile.cliff_records,
                    value,
                    cell.map(|cell| cell.class),
                )
            });
            let (class, uv) = match cliff {
                Some(cliff) => (map_cliff_class.or(Some(cliff.class)), cliff.uv),
                None => (
                    cell.map(|cell| cell.class),
                    cell.map_or(WHOLE_TEXTURE, |cell| corner_uv(cell.uv_xy, cell.uv_x1y1)),
                ),
            };
            push_quad(
                &mut groups,
                height_map,
                (TerrainLayer::Base, class),
                x,
                y,
                uv,
                [1.0; 4],
                vertical_scale,
            );

            if cliff.is_some() {
                continue;
            }

            for (layer, array) in [
                (TerrainLayer::Blend, &blend_tile.blend_index),
                (TerrainLayer::ExtraBlend, &blend_tile.extra_blend_index),
            ] {
                let Some(overlay) = index
                    .and_then(|index| array.get(index).copied())
                    .and_then(|value| cell_blend(layout, &blend_tile.blend_records, value))
                else {
                    continue;
                };
                let plain = corner_uv(overlay.tile.uv_xy, overlay.tile.uv_x1y1);
                let uv = match cliff_index {
                    Some(cliff_index) => overlay_cliff_uv(
                        layout,
                        &blend_tile.cliff_records,
                        cliff_index,
                        (overlay.tile.uv_xy, overlay.tile.uv_x1y1),
                        overlay.tile.class,
                        corner_heights(height_map, x, y),
                        vertical_scale / MAP_XY_FACTOR,
                    )
                    .unwrap_or(plain),
                    None => plain,
                };
                let overlay_class = match cliff_index {
                    Some(_) => map_cliff_class.unwrap_or(overlay.tile.class),
                    None => overlay.tile.class,
                };
                push_quad(
                    &mut groups,
                    height_map,
                    (layer, Some(overlay_class)),
                    x,
                    y,
                    uv,
                    overlay.ramp,
                    vertical_scale,
                );
            }
        }
    }

    groups.sort_by_key(|((layer, class), _)| (*layer, class.unwrap_or(usize::MAX)));
    groups
        .into_iter()
        .map(|((layer, texture_class), mesh)| TerrainPart {
            layer,
            texture_class,
            mesh,
        })
        .collect()
}

fn corner_heights(height_map: &HeightMapData, x: u32, y: u32) -> [u16; 4] {
    let (x1, y1) = (x.saturating_add(1), y.saturating_add(1));
    [(x, y), (x1, y), (x1, y1), (x, y1)].map(|(cx, cy)| height_map.sample(cx, cy).unwrap_or(0))
}

const WHOLE_TEXTURE: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

fn corner_uv(uv_xy: [f32; 2], uv_x1y1: [f32; 2]) -> [[f32; 2]; 4] {
    [
        [uv_xy[0], uv_xy[1]],
        [uv_x1y1[0], uv_xy[1]],
        [uv_xy[0], uv_x1y1[1]],
        [uv_x1y1[0], uv_x1y1[1]],
    ]
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    groups: &mut Vec<((TerrainLayer, Option<usize>), MeshData)>,
    height_map: &HeightMapData,
    key: (TerrainLayer, Option<usize>),
    x: u32,
    y: u32,
    uv: [[f32; 2]; 4],
    alpha: [f32; 4],
    vertical_scale: f32,
) {
    let slot = match groups.iter().position(|(existing, _)| *existing == key) {
        Some(slot) => slot,
        None => {
            groups.push((
                key,
                MeshData {
                    vertices: Vec::new(),
                    indices: Vec::new(),
                },
            ));
            groups.len().saturating_sub(1)
        }
    };
    let Some((_, mesh)) = groups.get_mut(slot) else {
        return;
    };

    let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    for (corner, (cx, cy)) in [
        (x, y),
        (x.saturating_add(1), y),
        (x, y.saturating_add(1)),
        (x.saturating_add(1), y.saturating_add(1)),
    ]
    .into_iter()
    .enumerate()
    {
        let (position, normal) = grid_vertex(height_map, cx, cy, vertical_scale);
        let mut vertex = Vertex::new(
            position,
            normal,
            uv.get(corner).copied().unwrap_or([0.0, 0.0]),
        );
        vertex.color = [1.0, 1.0, 1.0, alpha.get(corner).copied().unwrap_or(1.0)];
        mesh.vertices.push(vertex);
    }
    let indices = if quad_folds_along_ad(height_map, x, y) {
        [0u32, 1, 3, 0, 3, 2]
    } else {
        [0u32, 1, 2, 1, 3, 2]
    };
    for offset in indices {
        mesh.indices.push(base.saturating_add(offset));
    }
}

#[must_use]
pub fn quad_folds_along_ad(height_map: &HeightMapData, x: u32, y: u32) -> bool {
    let sample = |cx: u32, cy: u32| i32::from(height_map.sample(cx, cy).unwrap_or(0));
    let (x1, y1) = (x.saturating_add(1), y.saturating_add(1));
    let across_ad = sample(x, y).saturating_sub(sample(x1, y1)).abs();
    let across_bc = sample(x1, y).saturating_sub(sample(x, y1)).abs();
    across_ad < across_bc
}

fn cell_index(blend_tile: &BlendTileData, x: u32, y: u32) -> Option<usize> {
    if x >= blend_tile.width || y >= blend_tile.height {
        return None;
    }
    let index = y.checked_mul(blend_tile.width)?.checked_add(x)?;
    usize::try_from(index).ok()
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
pub(crate) mod tests;
