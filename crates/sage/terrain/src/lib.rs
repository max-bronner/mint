#![deny(clippy::arithmetic_side_effects)]

pub mod blend;
mod chunk;
pub mod cliff;
pub mod mip;
pub mod objects;
pub mod present;
pub mod river_shader;
pub mod rivers;
pub mod road_shader;
pub mod roads;
pub mod shroud_scene;
pub mod sky;
pub mod sky_scene;
pub mod surface;
pub mod terrain_shader;
mod terrain_types;
mod textures;
pub mod tile;
mod tuning_file;
pub mod water;
pub mod water_depth;
pub mod water_shader;
pub mod water_shading;
pub mod water_tuning;
pub mod wave_shader;
pub mod wave_surface;
pub mod waves;

pub use blend::{cell_blend, corner_mask, corner_ramp, ramp_alpha, CellBlend};
pub use chunk::{quad_folds_along_ad, ChunkExtent, TerrainLayer, TerrainPart};
pub use cliff::{cell_cliff, map_cliff_class, overlay_cliff_uv, stretch_uv, CellCliff};
pub use mip::class_mip_levels;
pub use objects::{
    forward, is_waypoint, place_objects, prototype_scale, proxy_mesh, proxy_transform,
    wants_terrain_alignment, ObjectProxy, ALIGN_KEY, PROTOTYPE_SCALE_KEY, PROXY_SIZE, WAYPOINT_KEY,
};
pub use river_shader::{
    RiverFlow, RiverLook, RiverStage, RiverUniforms, RIVER_BANK_TURBULENCE, RIVER_EDGE_BAND,
    RIVER_NOISE_WORLD_SCALE, RIVER_REFLECTION_STRENGTH, RIVER_REFRACTION_STRENGTH,
    RIVER_RIPPLE_DETAIL_RATE, RIVER_RIPPLE_REPEAT, RIVER_RIPPLE_SLOPE, RIVER_SCENE_TEXTURE_SLOTS,
    RIVER_SCENE_UNIFORM_SIZE, RIVER_SCROLL_UNIT, RIVER_SHADER_WGSL, RIVER_SHALLOW_RIPPLE,
    RIVER_SPARKLE_GAIN, RIVER_SPECULAR_EXPONENT, RIVER_SPECULAR_GAIN, RIVER_TEXTURE_SLOTS,
    RIVER_UNIFORM_SIZE,
};
pub use rivers::{
    build_rivers, river_scroll_offset, RiverNetwork, RiverRibbon, RiverSkip, RiverStats,
    FALLBACK_RIVER_WIDTH, MAX_RIVER_SECTIONS,
};
pub use road_shader::{
    ROAD_SCENE_TEXTURE_SLOTS, ROAD_SCENE_UNIFORM_SIZE, ROAD_SHADER_WGSL, ROAD_TEXTURE_SLOTS,
};
pub use roads::{
    build_roads, road_segments, RoadChainOrder, RoadHeight, RoadMesh, RoadNetwork, RoadOptions,
    RoadSegment, RoadSegmentBuild, RoadSegments, RoadSkip, RoadStats, RoadStyle, RoadTessellation,
    StepsPerCell, ROAD_END_CAP_FLAG, ROAD_END_FLAG, ROAD_LIFT_HEIGHT_STEP_EIGHTH, ROAD_LIFT_XY_128,
    ROAD_START_FLAG, STRAIGHT_BAND_V, TEXTURE_REPEAT_WIDTHS,
};
pub use shroud_scene::{
    ShroudSceneBlock, SHROUD_SCENE_TEXTURE_SLOTS, SHROUD_SCENE_UNIFORM_SIZE, SHROUD_SCENE_WGSL,
};
pub use sky::{
    sky_mesh, SkyDraw, SkyLook, SkyPlaneTerms, SkyUniforms, SKY_CLOUD_HEIGHT, SKY_CLOUD_REPEAT,
    SKY_CLOUD_TRAVEL, SKY_SHADER_WGSL, SKY_TEXTURE_SLOTS, SKY_UNIFORM_SIZE,
};
pub use sky_scene::{
    parse_water_set, parse_water_sky_texture, SkyScene, SkySceneError, WaterSkySet,
};
pub use surface::{
    drawn_surface_height, max_cell_height, pick_surface, surface_height, surface_normal,
};
pub use terrain_shader::{
    TerrainCloud, TerrainMacro, TerrainModulation, TerrainRelief, TerrainShadows, TerrainShroud,
    TerrainSun, TerrainUniforms, TERRAIN_CLOUD_REPEAT, TERRAIN_CLOUD_SCROLL,
    TERRAIN_FLAT_NORMAL_TEXEL, TERRAIN_MACRO_REPEAT, TERRAIN_SCENE_TEXTURE_SLOTS,
    TERRAIN_SCENE_UNIFORM_SIZE, TERRAIN_SHADER_WGSL, TERRAIN_TEXTURE_SLOTS,
};
pub use terrain_types::TerrainTypes;
pub use textures::{
    force_opaque, load_class_normal_maps, load_class_textures, ClassTextures, TextureError,
    TERRAIN_NORMAL_MAP_SUFFIX,
};
pub use tile::{CellTile, TileLayout};
pub use tuning_file::{default_tuning_path, TuningFile};
pub use water::{
    build_standing_water, build_standing_water_tuned, is_convex, triangulate, Triangulation,
    WaterBody, WaterSkip, WaterStats, WaterSurface, MAX_POLYGON_POINTS, WATER_TINT,
    WATER_UV_WORLD_SIZE,
};
pub use water_depth::{
    ground_samples, subdivide, subdivision_levels, subdivision_levels_with, MAX_WATER_SUBDIVISION,
    MAX_WATER_VERTICES, WATER_DEPTH_EDGE,
};
pub use water_shader::{
    water_motion, water_motion_capped, water_motion_drift, WaterDepth, WaterDepthMode,
    WaterDistortion, WaterDrift, WaterHighlights, WaterMotion, WaterNormal, WaterReflection,
    WaterReflectionComposite, WaterReflectionSource, WaterRefraction, WaterShaderParams,
    WaterSurfaceLook, WaterTechnique, WaterUniforms, WATER_BUMP_SLOPE, WATER_CREST_THRESHOLD,
    WATER_DRIFT_RATE_CAP, WATER_DRIFT_SPEED_CAP, WATER_FOAM_BAND, WATER_FOAM_GAIN,
    WATER_FRESNEL_F0, WATER_PLANAR_PERTURBATION, WATER_REFLECTION_BLUR, WATER_REFRACTION_FALLOFF,
    WATER_REFRACTION_SCALE, WATER_RIPPLE_DETAIL, WATER_RIPPLE_LOD_BIAS, WATER_RIPPLE_REPEAT,
    WATER_SCENE_TEXTURE_SLOTS, WATER_SCENE_UNIFORM_SIZE, WATER_SHADER_WGSL, WATER_SPECULAR_GAIN,
    WATER_STILL_ORBIT, WATER_TEXTURE_SLOTS, WATER_UNIFORM_SIZE, WATER_UNIT_SLOPE, WATER_UV_BASE,
};
pub use water_shading::{
    body_absorption, fit_ramp, load_ramp, parse_water_transparency, shade_body, BodyShading,
    DepthRange, RampError, RampFit, RampSource, WaterAlphaSource, WaterTint, WaterTransparency,
    RAMP_FIT_TOLERANCE,
};
pub use water_tuning::{
    ParsedTuning, Section, TextEntry, TuningEntry, TuningName, WaterTuning, ENTRIES,
    MAX_TUNING_NAME, TEXTS,
};
pub use wave_shader::{
    WaveDraw, WaveLook, WavePca, WaveUniforms, WAVE_ALPHA_GAIN, WAVE_BEACH_BAND,
    WAVE_COMPRESSED_DEPTH, WAVE_CREST_HEIGHT, WAVE_FADE_IN, WAVE_FADE_OUT,
    WAVE_SCENE_TEXTURE_SLOTS, WAVE_SCENE_UNIFORM_SIZE, WAVE_SHADER_WGSL, WAVE_TEXTURE_SLOTS,
    WAVE_UNIFORM_SIZE,
};
pub use wave_surface::{
    surface_waves, SurfaceWave, WaveSurface, MAX_SURFACE_WAVES, WAVE_SURFACE_BYTES,
    WAVE_SURFACE_ROWS,
};
pub use waves::{
    build_standing_waves, WaveArea, WaveField, WaveLine, WaveParams, WaveScale, WaveSkip,
    WaveStats, MAX_WAVE_TILES, WAVE_LIFT, WAVE_TILE_OVERLAP, WAVE_VELOCITY_UNIT,
};

use mint_render::{Aabb, Vec3};
use mint_sage_map::{cell_to_world, BlendTileData, Boundary, HeightMapData};

#[derive(Debug, Clone)]
pub struct TerrainChunk {
    pub extent: ChunkExtent,
    pub parts: Vec<TerrainPart>,
    pub bounds: Aabb,
}

#[derive(Debug, Clone)]
pub struct Terrain {
    pub width: u32,
    pub height: u32,
    pub border_size: u32,
    pub boundaries: Vec<Boundary>,
    pub vertical_scale: f32,
    pub chunks: Vec<TerrainChunk>,
    pub layout: TileLayout,
    pub map_cliff_class: Option<usize>,
}

impl Terrain {
    #[must_use]
    pub fn build(
        height_map: &HeightMapData,
        blend_tile: &BlendTileData,
        vertical_scale: f32,
    ) -> Self {
        let layout = TileLayout::new(&blend_tile.texture_classes);
        let map_cliff_class = cliff::map_cliff_class(&layout, &blend_tile.cliff_records);
        let chunks = chunk::chunk_extents(height_map.width, height_map.height)
            .into_iter()
            .map(|extent| {
                let parts = chunk::build_chunk_parts(
                    height_map,
                    blend_tile,
                    &layout,
                    &extent,
                    vertical_scale,
                    map_cliff_class,
                );
                let bounds = parts_bounds(&parts);
                TerrainChunk {
                    extent,
                    parts,
                    bounds,
                }
            })
            .collect();

        Self {
            width: height_map.width,
            height: height_map.height,
            border_size: height_map.border_size,
            boundaries: height_map.boundaries.clone(),
            vertical_scale,
            chunks,
            layout,
            map_cliff_class,
        }
    }

    pub fn set_vertical_scale(
        &mut self,
        height_map: &HeightMapData,
        blend_tile: &BlendTileData,
        vertical_scale: f32,
    ) {
        self.vertical_scale = vertical_scale;
        for chunk in &mut self.chunks {
            chunk.parts = chunk::build_chunk_parts(
                height_map,
                blend_tile,
                &self.layout,
                &chunk.extent,
                vertical_scale,
                self.map_cliff_class,
            );
            chunk.bounds = parts_bounds(&chunk.parts);
        }
    }

    #[must_use]
    pub fn bounds(&self) -> Aabb {
        let mut bounds = Aabb::EMPTY;
        for chunk in &self.chunks {
            bounds.union(&chunk.bounds);
        }
        bounds
    }

    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.chunks
            .iter()
            .flat_map(|chunk| &chunk.parts)
            .map(|part| part.mesh.triangle_count())
            .sum()
    }

    #[must_use]
    pub fn part_count(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.parts.len()).sum()
    }

    #[must_use]
    pub fn playable_bounds(&self) -> Aabb {
        let overall = self.bounds();
        if overall.is_empty() {
            return Aabb::EMPTY;
        }
        let (Some((x0, x1)), Some((y0, y1))) = (
            inset_range(self.width, self.border_size),
            inset_range(self.height, self.border_size),
        ) else {
            return overall;
        };

        let min = Vec3::new(
            cell_to_world(chunk::cell_i32(x0), self.border_size),
            cell_to_world(chunk::cell_i32(y0), self.border_size),
            overall.min().z,
        );
        let max = Vec3::new(
            cell_to_world(chunk::cell_i32(x1), self.border_size),
            cell_to_world(chunk::cell_i32(y1), self.border_size),
            overall.max().z,
        );
        Aabb::from_corners(min, max)
    }

    #[must_use]
    pub fn active_boundary_bounds(&self) -> Aabb {
        self.boundary_bounds(0)
    }

    #[must_use]
    pub fn boundary_bounds(&self, index: usize) -> Aabb {
        let overall = self.bounds();
        if overall.is_empty() {
            return Aabb::EMPTY;
        }
        let Some(boundary) = self.boundaries.get(index) else {
            return self.playable_bounds();
        };
        if boundary.x <= 0 || boundary.y <= 0 {
            return self.playable_bounds();
        }
        let border = chunk::cell_i32(self.border_size);
        let min = Vec3::new(
            cell_to_world(border, self.border_size),
            cell_to_world(border, self.border_size),
            overall.min().z,
        );
        let max = Vec3::new(
            cell_to_world(border.saturating_add(boundary.x), self.border_size),
            cell_to_world(border.saturating_add(boundary.y), self.border_size),
            overall.max().z,
        );
        Aabb::from_corners(min, max)
    }
}

fn parts_bounds(parts: &[TerrainPart]) -> Aabb {
    let mut bounds = Aabb::EMPTY;
    for part in parts {
        bounds.union(&part.mesh.bounds());
    }
    bounds
}

fn inset_range(extent: u32, border: u32) -> Option<(u32, u32)> {
    if extent == 0 {
        return None;
    }
    let max = extent.saturating_sub(1);
    if border.saturating_mul(2) >= extent {
        Some((0, max))
    } else {
        Some((border, max.saturating_sub(border)))
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::chunk::tests::{blend_tile, one_class};

    fn sloped_height_map(width: u32, height: u32, border_size: u32) -> HeightMapData {
        let mut samples = Vec::with_capacity((width * height) as usize);
        for _ in 0..height {
            for x in 0..width {
                samples.push(u16::try_from(x * 10).unwrap_or(u16::MAX));
            }
        }
        HeightMapData {
            width,
            height,
            border_size,
            boundaries: Vec::new(),
            samples,
        }
    }

    fn one_class_blend_tile(width: u32, height: u32) -> BlendTileData {
        blend_tile(
            width,
            height,
            vec![0; (width * height) as usize],
            vec![one_class(16, 4)],
        )
    }

    #[test]
    fn level_ground_keeps_its_texture_coordinates_at_every_scale() {
        let hm = crate::chunk::tests::flat_height_map(5, 5, 1234);
        let bt = one_class_blend_tile(5, 5);
        let mut terrain = Terrain::build(&hm, &bt, 1.0);
        let before = terrain.chunks[0].parts[0].mesh.clone();
        terrain.set_vertical_scale(&hm, &bt, 64.0);
        for (b, a) in before
            .vertices
            .iter()
            .zip(&terrain.chunks[0].parts[0].mesh.vertices)
        {
            assert_eq!(b.uv, a.uv);
        }
    }

    #[test]
    fn build_covers_the_full_grid_including_the_border_ring() {
        let hm = sloped_height_map(5, 5, 1);
        let bt = one_class_blend_tile(5, 5);
        let terrain = Terrain::build(&hm, &bt, 1.0);
        let min_x = terrain.chunks.iter().map(|c| c.extent.x0).min().unwrap();
        let max_x = terrain.chunks.iter().map(|c| c.extent.x1).max().unwrap();
        assert_eq!(min_x, 0);
        assert_eq!(max_x, hm.width - 1);
    }

    #[test]
    fn a_1x1_heightmap_builds_with_no_chunks_and_no_panic() {
        let hm = sloped_height_map(1, 1, 0);
        let bt = one_class_blend_tile(1, 1);
        let terrain = Terrain::build(&hm, &bt, 1.0);
        assert!(terrain.chunks.is_empty());
        assert!(terrain.bounds().is_empty());
        assert!(terrain.playable_bounds().is_empty());
    }

    #[test]
    fn a_zero_size_heightmap_builds_with_no_chunks_and_no_panic() {
        let hm = HeightMapData {
            width: 0,
            height: 0,
            border_size: 0,
            boundaries: Vec::new(),
            samples: Vec::new(),
        };
        let bt = blend_tile(0, 0, Vec::new(), vec![one_class(16, 4)]);
        let terrain = Terrain::build(&hm, &bt, 1.0);
        assert!(terrain.chunks.is_empty());
        assert!(terrain.bounds().is_empty());
    }

    #[test]
    fn changing_the_vertical_scale_rebuilds_positions_and_normals() {
        let hm = sloped_height_map(3, 3, 0);
        let bt = one_class_blend_tile(3, 3);
        let mut terrain = Terrain::build(&hm, &bt, 1.0);
        let before = terrain.chunks[0].parts[0].mesh.clone();

        terrain.set_vertical_scale(&hm, &bt, 2.0);
        let after = &terrain.chunks[0].parts[0].mesh;

        assert_eq!(terrain.vertical_scale, 2.0);
        assert_eq!(before.vertices.len(), after.vertices.len());
        let mut z_changed = false;
        let mut normal_changed = false;
        for (b, a) in before.vertices.iter().zip(&after.vertices) {
            assert_eq!(b.position[0], a.position[0]);
            assert_eq!(b.position[1], a.position[1]);
            if b.position[2] != a.position[2] {
                z_changed = true;
            }
            if b.normal != a.normal {
                normal_changed = true;
            }
        }
        assert!(
            z_changed,
            "doubling the scale must move at least one vertex"
        );
        assert!(
            normal_changed,
            "a sloped heightmap's normal must change with the scale too"
        );
    }

    #[test]
    fn playable_bounds_is_inset_from_the_full_bounds_by_the_border() {
        let hm = sloped_height_map(20, 20, 3);
        let bt = one_class_blend_tile(20, 20);
        let terrain = Terrain::build(&hm, &bt, 1.0);
        let full = terrain.bounds();
        let playable = terrain.playable_bounds();

        assert!(playable.min().x > full.min().x);
        assert!(playable.max().x < full.max().x);
        assert!(playable.min().y > full.min().y);
        assert!(playable.max().y < full.max().y);
    }

    #[test]
    fn playable_bounds_falls_back_to_the_full_grid_when_the_border_covers_it() {
        let hm = sloped_height_map(4, 4, 10);
        let bt = one_class_blend_tile(4, 4);
        let terrain = Terrain::build(&hm, &bt, 1.0);
        assert_eq!(terrain.playable_bounds(), terrain.bounds());
    }
}
