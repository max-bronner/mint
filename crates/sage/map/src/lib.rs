#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod base;
mod blend_tile;
mod camera_animations;
mod container;
mod dictionary;
mod environment;
mod global_lighting;
mod height_map;
mod named_cameras;
mod objects;
mod post_effects;
mod reader;
mod scripts;
mod sides;
mod skybox;
mod teams;
#[cfg(test)]
mod testutil;
mod transform;
mod trigger_areas;
mod water;
mod waypoints;
mod world_info;

pub use base::{
    base_path, BaseEntry, BaseLayout, BasePhase, BASE_ANCHOR_KEY, BASE_PHASE_KEY, BASE_PRIORITY_KEY,
};
pub use blend_tile::{BlendRecord, BlendTileData, CliffRecord, Flammability, TextureClass};
pub use camera_animations::{
    CameraAnimation, CameraAnimations, CameraTrack, FreeKey, Interpolation, Key, LookKey,
    CAMERA_ANIMATIONS_CHUNK,
};
pub use dictionary::{Dictionary, Value};
pub use environment::{EnvironmentData, ENVIRONMENT_CHUNK};
pub use global_lighting::{GlobalLighting, LightingConfig, MapLight, TimeOfDay, UnnamedBlock};
pub use height_map::{Boundary, HeightMapData};
pub use named_cameras::{NamedCamera, NAMED_CAMERAS_CHUNK};
pub use objects::{
    named_waypoint, start_waypoint_name, Object, ORIGINAL_OWNER_KEY, WAYPOINT_NAME_KEY,
    WAYPOINT_TEMPLATE,
};
pub use post_effects::{PostEffect, POST_EFFECTS_CHUNK};
pub use scripts::{
    initial_int_values, Condition, OrCondition, Parameter, Script, ScriptAction, ScriptGroup,
    ScriptNode, ScriptSequence, ScriptTree,
};
pub use sides::{BuildListEntry, Side, SIDES_CHUNK};
pub use skybox::{SkyboxSettings, SKYBOX_CHUNK};
pub use teams::{
    split_team_ref, TeamDef, TeamTemplate, UnitSlot, TEAMS_CHUNK, TEAM_GENERIC_HOOKS,
    TEAM_UNIT_SLOTS,
};
pub use transform::{cell_to_world, MAP_XY_FACTOR};
pub use trigger_areas::{TriggerArea, TRIGGER_AREAS_CHUNK};
pub use water::{
    RiverArea, RiverLine, StandingWaterArea, StandingWaveArea, Water, WaterAreas, RIVER_CHUNK,
    STANDING_WATER_CHUNK, STANDING_WAVE_CHUNK,
};
pub use waypoints::{
    build_links, Waypoint, WaypointLink, WaypointNode, MAX_WAYPOINT_LINKS, WAYPOINTS_CHUNK,
};
pub use world_info::WorldInfo;

use container::RawChunk;
use reader::Reader;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "unexpected end of data at offset {offset}: needed {needed} more byte(s), {available} left"
    )]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error("body does not begin with the CkMp magic")]
    BadMagic,

    #[error("file does not begin with the EAR\\0 wrapper")]
    NotEarWrapped,

    #[error("decompressing the map body: {0}")]
    Decompress(#[from] mint_sage_refpack::Error),

    #[error("offset {offset}: name-table index {index} is not defined")]
    UnknownNameIndex { offset: usize, index: u32 },

    #[error("chunk {name} at offset {offset} claims {declared} payload bytes but only {available} remain")]
    ChunkOverrun {
        name: String,
        offset: usize,
        declared: usize,
        available: usize,
    },

    #[error("offset {offset}: dictionary value type {kind} is not defined")]
    UnknownValueType { offset: usize, kind: u8 },

    #[error("required chunk {name} is not present in this file")]
    MissingChunk { name: &'static str },

    #[error(
        "HeightMapData declares {declared_samples} samples but width {width} * height {height} disagrees"
    )]
    HeightMapMismatch {
        declared_samples: u32,
        width: u32,
        height: u32,
    },

    #[error("BlendTileData declares cellCount {cell_count} but width {width} * height {height} disagrees")]
    CellCountMismatch {
        cell_count: u32,
        width: u32,
        height: u32,
    },

    #[error("BlendTileData version {version} has no known per-cell layout")]
    UnknownBlendVersion { version: u16 },

    #[error("blend record at offset {offset}: expected marker 0x7ADA0000, found {found:#010x}")]
    BadBlendMarker { offset: usize, found: u32 },

    #[error("{name} at offset {offset} left {remaining} byte(s) unread")]
    TrailingBytes {
        name: &'static str,
        offset: usize,
        remaining: usize,
    },

    #[error("ObjectsList child named {found}, expected only Object")]
    UnexpectedChild { parent: &'static str, found: String },

    #[error("EnvironmentData version {version} has no known layout")]
    UnknownEnvironmentVersion { version: u16 },

    #[error("SkyboxSettings version {version} has no known layout")]
    UnknownSkyboxVersion { version: u16 },

    #[error("GlobalLighting version {version} has no known layout")]
    UnknownLightingVersion { version: u16 },

    #[error("TriggerAreas version {version} has no known layout")]
    UnknownTriggerAreaVersion { version: u16 },

    #[error("{chunk} version {version} has no known layout")]
    UnknownWaterVersion { chunk: &'static str, version: u16 },

    #[error(
        "{chunk} at offset {offset}: a count of {count} x {item_size} byte(s) exceeds the {available} byte(s) present"
    )]
    CountExceedsPayload {
        chunk: &'static str,
        offset: usize,
        count: u32,
        item_size: usize,
        available: usize,
    },

    #[error("{name}: {reason}")]
    Malformed {
        name: &'static str,
        reason: &'static str,
    },

    #[error("{chunk}: key {key} is a {found}, expected {expected}")]
    WrongValueType {
        chunk: &'static str,
        key: String,
        expected: &'static str,
        found: &'static str,
    },

    #[error("{chunk} version {version} has no known layout")]
    UnknownScriptVersion { chunk: &'static str, version: u16 },
}

#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub name: String,
    pub version: u16,
    pub offset: usize,
    pub len: usize,
}

#[derive(Debug)]
pub struct MapFile {
    body: Vec<u8>,
    names: container::NameTable,
    pub height_map: HeightMapData,
    pub blend_tile: BlendTileData,
    pub objects: Vec<Object>,
    pub world_info: WorldInfo,
    pub global_lighting: GlobalLighting,
    pub environment: Option<EnvironmentData>,
    pub skybox: Option<SkyboxSettings>,
    pub post_effects: Vec<PostEffect>,
    pub water: Water,
    pub scripts: Option<ScriptTree>,
    pub chunks: Vec<ChunkInfo>,
}

impl MapFile {
    pub fn parse(file_bytes: &[u8]) -> Result<Self> {
        let stream =
            mint_sage_refpack::strip_ear_wrapper(file_bytes).ok_or(Error::NotEarWrapped)?;
        let body = mint_sage_refpack::decompress(stream)?;
        Self::parse_body(body)
    }

    pub fn parse_body(body: Vec<u8>) -> Result<Self> {
        let (names, raw_chunks) = container::parse_top_level(&body)?;

        let hm_chunk = find_chunk(&raw_chunks, "HeightMapData")?;
        let height_map = height_map::read_height_map(&body, hm_chunk)?;

        let bt_chunk = find_chunk(&raw_chunks, "BlendTileData")?;
        let blend_tile =
            blend_tile::read_blend_tile_data(&body, bt_chunk, height_map.width, height_map.height)?;

        let ol_chunk = find_chunk(&raw_chunks, "ObjectsList")?;
        let objects = objects::read_objects(&body, ol_chunk, &names)?;

        let wi_chunk = find_chunk(&raw_chunks, "WorldInfo")?;
        let world_info = world_info::read_world_info(&body, wi_chunk, &names)?;

        let gl_chunk = find_chunk(&raw_chunks, "GlobalLighting")?;
        let global_lighting = global_lighting::read_global_lighting(&body, gl_chunk)?;

        let environment = optional_chunk(&raw_chunks, environment::ENVIRONMENT_CHUNK)
            .map(|c| environment::read_environment(&body, c))
            .transpose()?;

        let skybox = optional_chunk(&raw_chunks, skybox::SKYBOX_CHUNK)
            .map(|c| skybox::read_skybox(&body, c))
            .transpose()?;

        let post_effects = optional_chunk(&raw_chunks, post_effects::POST_EFFECTS_CHUNK)
            .map(|c| post_effects::read_post_effects(&body, c))
            .transpose()?
            .unwrap_or_default();

        let water = Water {
            standing: optional_chunk(&raw_chunks, water::STANDING_WATER_CHUNK)
                .map(|c| water::read_standing_water(&body, c))
                .transpose()?,
            rivers: optional_chunk(&raw_chunks, water::RIVER_CHUNK)
                .map(|c| water::read_rivers(&body, c))
                .transpose()?,
            waves: optional_chunk(&raw_chunks, water::STANDING_WAVE_CHUNK)
                .map(|c| water::read_waves(&body, c))
                .transpose()?,
        };

        let scripts = optional_chunk(&raw_chunks, "PlayerScriptsList")
            .map(|c| scripts::read_player_scripts_list(&body, c, &names))
            .transpose()?;

        let chunks = raw_chunks
            .iter()
            .map(|c| ChunkInfo {
                name: c.name.clone(),
                version: c.version,
                offset: c.body,
                len: c.len,
            })
            .collect();

        Ok(Self {
            body,
            names,
            height_map,
            blend_tile,
            objects,
            world_info,
            global_lighting,
            environment,
            skybox,
            post_effects,
            water,
            scripts,
            chunks,
        })
    }

    pub fn sides(&self) -> Result<Vec<Side>> {
        self.chunks
            .iter()
            .find(|c| c.name == SIDES_CHUNK)
            .map_or_else(
                || Ok(Vec::new()),
                |info| {
                    let chunk = RawChunk {
                        name: info.name.clone(),
                        version: info.version,
                        body: info.offset,
                        len: info.len,
                    };
                    sides::read_sides(&self.body, &chunk, &self.names)
                },
            )
    }

    fn chunk_named(&self, name: &str) -> Option<RawChunk> {
        self.chunks
            .iter()
            .find(|c| c.name == name)
            .map(|info| RawChunk {
                name: info.name.clone(),
                version: info.version,
                body: info.offset,
                len: info.len,
            })
    }

    pub fn teams(&self) -> Result<Vec<TeamDef>> {
        self.chunk_named(TEAMS_CHUNK).map_or_else(
            || Ok(Vec::new()),
            |chunk| teams::read_teams(&self.body, &chunk, &self.names),
        )
    }

    pub fn waypoint_links(&self) -> Result<Vec<WaypointLink>> {
        self.chunk_named(WAYPOINTS_CHUNK).map_or_else(
            || Ok(Vec::new()),
            |chunk| waypoints::read_waypoint_links(&self.body, &chunk),
        )
    }

    pub fn waypoints(&self) -> Result<Vec<Waypoint>> {
        let mut out = Vec::new();
        for object in &self.objects {
            if let Some(waypoint) = Waypoint::from_object(object)? {
                out.push(waypoint);
            }
        }
        Ok(out)
    }

    pub fn camera_animations(&self) -> Result<CameraAnimations> {
        self.chunk_named(CAMERA_ANIMATIONS_CHUNK).map_or_else(
            || Ok(CameraAnimations::default()),
            |chunk| camera_animations::read_camera_animations(&self.body, &chunk),
        )
    }

    pub fn trigger_areas(&self) -> Result<Vec<TriggerArea>> {
        self.chunks
            .iter()
            .find(|c| c.name == TRIGGER_AREAS_CHUNK)
            .map_or_else(
                || Ok(Vec::new()),
                |info| {
                    let chunk = RawChunk {
                        name: info.name.clone(),
                        version: info.version,
                        body: info.offset,
                        len: info.len,
                    };
                    trigger_areas::read_trigger_areas(&self.body, &chunk)
                },
            )
    }

    pub fn named_cameras(&self) -> Result<Vec<NamedCamera>> {
        self.chunks
            .iter()
            .find(|c| c.name == NAMED_CAMERAS_CHUNK)
            .map_or_else(
                || Ok(Vec::new()),
                |info| {
                    let chunk = RawChunk {
                        name: info.name.clone(),
                        version: info.version,
                        body: info.offset,
                        len: info.len,
                    };
                    named_cameras::read_named_cameras(&self.body, &chunk)
                },
            )
    }

    pub fn library_map_lists(&self) -> Result<Vec<Vec<String>>> {
        self.chunk_named(LIBRARY_MAP_LISTS_CHUNK).map_or_else(
            || Ok(Vec::new()),
            |chunk| read_library_map_lists(&self.body, &chunk, &self.names),
        )
    }

    #[must_use]
    pub fn raw_payload(&self, info: &ChunkInfo) -> &[u8] {
        let end = info.offset.saturating_add(info.len);
        self.body.get(info.offset..end).unwrap_or_default()
    }
}

pub fn read_script_library(file_bytes: &[u8]) -> Result<Option<ScriptTree>> {
    let stream = mint_sage_refpack::strip_ear_wrapper(file_bytes).ok_or(Error::NotEarWrapped)?;
    let body = mint_sage_refpack::decompress(stream)?;
    read_script_library_body(&body)
}

const LIBRARY_MAP_LISTS_CHUNK: &str = "LibraryMapLists";

fn read_library_map_lists(
    body: &[u8],
    chunk: &RawChunk,
    names: &container::NameTable,
) -> Result<Vec<Vec<String>>> {
    const LIBRARY_MAPS_CHUNK: &str = "LibraryMaps";
    let end = chunk.end()?;
    let children = container::read_chunks(body, chunk.body, end, names)?;
    let mut lists = Vec::with_capacity(children.len());
    for child in &children {
        if child.name != LIBRARY_MAPS_CHUNK {
            return Err(Error::UnexpectedChild {
                parent: LIBRARY_MAP_LISTS_CHUNK,
                found: child.name.clone(),
            });
        }
        let payload = child.payload(body)?;
        let mut r = Reader::new_at(payload, child.body);
        let declared = r.u32()?;
        let wanted = usize::try_from(declared).unwrap_or(usize::MAX);
        let needed = wanted.saturating_mul(2);
        if needed > r.remaining() {
            return Err(Error::CountExceedsPayload {
                chunk: LIBRARY_MAPS_CHUNK,
                offset: r.position(),
                count: declared,
                item_size: 2,
                available: r.remaining(),
            });
        }
        let mut list = Vec::with_capacity(wanted);
        for _ in 0..wanted {
            list.push(r.ascii_string()?);
        }
        if r.remaining() != 0 {
            return Err(Error::TrailingBytes {
                name: LIBRARY_MAPS_CHUNK,
                offset: r.position(),
                remaining: r.remaining(),
            });
        }
        lists.push(list);
    }
    Ok(lists)
}

#[derive(Debug, Clone, Default)]
pub struct MapLibrary {
    pub scripts: Option<ScriptTree>,
    pub teams: Vec<TeamDef>,
    pub library_map_lists: Vec<Vec<String>>,
}

pub fn read_map_library(file_bytes: &[u8]) -> Result<MapLibrary> {
    let stream = mint_sage_refpack::strip_ear_wrapper(file_bytes).ok_or(Error::NotEarWrapped)?;
    let body = mint_sage_refpack::decompress(stream)?;
    let (names, raw_chunks) = container::parse_top_level(&body)?;
    let scripts = optional_chunk(&raw_chunks, "PlayerScriptsList")
        .map(|c| scripts::read_player_scripts_list(&body, c, &names))
        .transpose()?;
    let teams = optional_chunk(&raw_chunks, TEAMS_CHUNK)
        .map(|c| teams::read_teams(&body, c, &names))
        .transpose()?
        .unwrap_or_default();
    let library_map_lists = optional_chunk(&raw_chunks, LIBRARY_MAP_LISTS_CHUNK)
        .map(|c| read_library_map_lists(&body, c, &names))
        .transpose()?
        .unwrap_or_default();
    Ok(MapLibrary {
        scripts,
        teams,
        library_map_lists,
    })
}

fn read_script_library_body(body: &[u8]) -> Result<Option<ScriptTree>> {
    let (names, raw_chunks) = container::parse_top_level(body)?;
    optional_chunk(&raw_chunks, "PlayerScriptsList")
        .map(|c| scripts::read_player_scripts_list(body, c, &names))
        .transpose()
}

fn find_chunk<'a>(chunks: &'a [RawChunk], name: &'static str) -> Result<&'a RawChunk> {
    chunks
        .iter()
        .find(|c| c.name == name)
        .ok_or(Error::MissingChunk { name })
}

fn optional_chunk<'a>(chunks: &'a [RawChunk], name: &str) -> Option<&'a RawChunk> {
    chunks.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{
        blend_tile_payload, global_lighting_payload, height_map_payload, MapBuilder, ShapeParams,
    };

    fn synthetic_map() -> Vec<u8> {
        let mut b = MapBuilder::new();

        let hm_payload = height_map_payload(2, 2, 0, &[(2, 2)], &[10, 20, 30, 40]);
        b.chunk("HeightMapData", 5, &hm_payload);

        let mut shape = ShapeParams::minimal(2, 2);
        shape.num_bitmap_tiles = 1;
        let bt_payload = blend_tile_payload(8, &shape);
        b.chunk("BlendTileData", 8, &bt_payload);

        let entries = vec![b.dict_bool("objectEnabled", true)];
        let object_payload = b.object_fields(1.0, 1.0, 0.0, 0.0, 0, "Tower", entries);
        let object_bytes = b.nested_chunk("Object", 3, &object_payload);
        b.chunk("ObjectsList", 3, &object_bytes);

        let world_entries = vec![b.dict_int("weather", 1)];
        let world_payload = b.dict(world_entries);
        b.chunk("WorldInfo", 1, &world_payload);

        let lighting_payload = global_lighting_payload(8, 2);
        b.chunk("GlobalLighting", 8, &lighting_payload);

        b.chunk("Teams", 1, &[1, 2, 3]);

        b.build()
    }

    #[test]
    fn parses_every_required_chunk_and_catalogues_the_rest() {
        let map = MapFile::parse_body(synthetic_map()).expect("valid synthetic map");

        assert_eq!(map.height_map.width, 2);
        assert_eq!(map.height_map.samples, vec![10, 20, 30, 40]);
        assert_eq!(map.blend_tile.version, 8);
        assert_eq!(map.objects.len(), 1);
        assert_eq!(map.objects[0].template_name, "Tower");
        assert_eq!(
            map.world_info.properties.get("weather"),
            Some(&Value::Int(1))
        );
        assert_eq!(map.global_lighting.time_of_day, Some(TimeOfDay::Afternoon));

        let teams = map
            .chunks
            .iter()
            .find(|c| c.name == "Teams")
            .expect("Teams is catalogued");
        assert_eq!(map.raw_payload(teams), &[1, 2, 3]);
    }

    #[test]
    fn missing_required_chunk_is_an_error() {
        let mut b = MapBuilder::new();
        let hm_payload = height_map_payload(1, 1, 0, &[], &[0]);
        b.chunk("HeightMapData", 5, &hm_payload);
        let err = MapFile::parse_body(b.build()).unwrap_err();
        assert!(matches!(err, Error::MissingChunk { .. }));
    }

    fn ascii_string_bytes(s: &str) -> Vec<u8> {
        let mut out = (u16::try_from(s.len()).expect("test string fits u16"))
            .to_le_bytes()
            .to_vec();
        out.extend_from_slice(s.as_bytes());
        out
    }

    fn map_with_library_maps(b: &mut MapBuilder, nested_chunks: &[Vec<u8>]) -> Vec<u8> {
        let hm_payload = height_map_payload(1, 1, 0, &[], &[0]);
        b.chunk("HeightMapData", 5, &hm_payload);
        let mut shape = ShapeParams::minimal(1, 1);
        shape.num_bitmap_tiles = 1;
        let bt_payload = blend_tile_payload(8, &shape);
        b.chunk("BlendTileData", 8, &bt_payload);
        b.chunk("ObjectsList", 3, &[]);
        let world_payload = b.dict(Vec::new());
        b.chunk("WorldInfo", 1, &world_payload);
        let lighting_payload = global_lighting_payload(8, 1);
        b.chunk("GlobalLighting", 8, &lighting_payload);
        let list_payload: Vec<u8> = nested_chunks.iter().flatten().copied().collect();
        b.chunk("LibraryMapLists", 1, &list_payload);
        b.build()
    }

    #[test]
    fn library_map_lists_reads_the_u32_count_then_that_many_strings() {
        let mut payload = 2u32.to_le_bytes().to_vec();
        payload.extend(ascii_string_bytes("libraries\\a\\a.map"));
        payload.extend(ascii_string_bytes("libraries\\b\\b.map"));
        let mut b = MapBuilder::new();
        let maps_chunk = b.nested_chunk("LibraryMaps", 1, &payload);
        let map = MapFile::parse_body(map_with_library_maps(&mut b, &[maps_chunk]))
            .expect("valid synthetic map");
        assert_eq!(
            map.library_map_lists()
                .expect("a well-formed LibraryMaps chunk"),
            vec![vec![
                "libraries\\a\\a.map".to_owned(),
                "libraries\\b\\b.map".to_owned()
            ]]
        );
    }

    #[test]
    fn library_map_lists_is_empty_without_the_chunk() {
        let map = MapFile::parse_body(synthetic_map()).expect("valid synthetic map");
        assert_eq!(
            map.library_map_lists().expect("no chunk is not an error"),
            Vec::<Vec<String>>::new()
        );
    }

    #[test]
    fn library_map_lists_count_exceeding_the_payload_is_an_error() {
        let mut b = MapBuilder::new();
        let maps_chunk = b.nested_chunk("LibraryMaps", 1, &5u32.to_le_bytes());
        let map = MapFile::parse_body(map_with_library_maps(&mut b, &[maps_chunk]))
            .expect("valid synthetic map");
        assert!(matches!(
            map.library_map_lists(),
            Err(Error::CountExceedsPayload { .. })
        ));
    }

    #[test]
    fn library_map_lists_keeps_one_list_per_nested_chunk_in_side_order() {
        let mut b = MapBuilder::new();
        let empty_chunk = b.nested_chunk("LibraryMaps", 1, &0u32.to_le_bytes());
        let mut second_payload = 2u32.to_le_bytes().to_vec();
        second_payload.extend(ascii_string_bytes("libraries\\c\\c.map"));
        second_payload.extend(ascii_string_bytes("libraries\\d\\d.map"));
        let second_chunk = b.nested_chunk("LibraryMaps", 1, &second_payload);
        let map = MapFile::parse_body(map_with_library_maps(&mut b, &[empty_chunk, second_chunk]))
            .expect("valid synthetic map");
        assert_eq!(
            map.library_map_lists()
                .expect("two well-formed LibraryMaps chunks"),
            vec![
                Vec::new(),
                vec![
                    "libraries\\c\\c.map".to_owned(),
                    "libraries\\d\\d.map".to_owned()
                ]
            ]
        );
    }

    #[test]
    fn read_script_library_body_decodes_a_file_with_no_terrain_chunks_at_all() {
        let mut b = MapBuilder::new();
        let list_chunk = b.nested_chunk("ScriptList", 1, &[]);
        b.chunk("PlayerScriptsList", 1, &list_chunk);
        let body = b.build();

        assert!(matches!(
            MapFile::parse_body(body.clone()),
            Err(Error::MissingChunk { .. })
        ));

        let tree = read_script_library_body(&body)
            .expect("a body with a valid PlayerScriptsList decodes")
            .expect("this body carries one");
        assert_eq!(tree.player_slots.len(), 1);
        assert!(tree.player_slots[0].is_empty());
    }

    #[test]
    fn read_script_library_body_returns_none_without_a_playerscriptslist() {
        let mut b = MapBuilder::new();
        b.chunk("Teams", 1, &[1, 2, 3]);
        let body = b.build();
        assert_eq!(read_script_library_body(&body).unwrap(), None);
    }

    #[test]
    fn bad_magic_is_an_error_not_a_panic() {
        let err = MapFile::parse_body(b"NOPE".to_vec()).unwrap_err();
        assert!(matches!(err, Error::BadMagic));
    }

    #[test]
    fn truncation_sweep_never_panics() {
        let body = synthetic_map();
        for cut in 0..=body.len() {
            let _ = MapFile::parse_body(body[..cut].to_vec());
        }
    }

    #[test]
    fn bit_corruption_sweep_never_panics() {
        let body = synthetic_map();
        for i in 0..body.len() {
            for bit in 0..8u8 {
                let mut corrupt = body.clone();
                if let Some(byte) = corrupt.get_mut(i) {
                    *byte ^= 1 << bit;
                }
                let _ = MapFile::parse_body(corrupt);
            }
        }
    }
}
