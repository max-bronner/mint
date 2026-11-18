use crate::xfer::{AsciiString, Reader, Writer};
use crate::{Result, SaveError};

pub const CHUNK_LIVING_WORLD_LOGIC: &[u8] = b"CHUNK_LivingWorldLogic";
pub const CHUNK_AUDIO: &[u8] = b"CHUNK_Audio";
pub const CHUNK_GAME_STATE_MAP: &[u8] = b"CHUNK_GameStateMap";
pub const CHUNK_GAME_LOGIC: &[u8] = b"CHUNK_GameLogic";

const LIVING_WORLD_VERSION: u8 = 6;
const AUDIO_VERSION: u8 = 4;
const GAME_STATE_MAP_VERSION: u8 = 2;
const GAME_LOGIC_VERSION: u8 = 8;
const MUSIC_SYSTEMS: usize = 2;
const VOLUMES: usize = 12;
const SKIRMISH_MODE: i32 = 2;
const TERRAIN_RESOURCE_ZEROS: usize = 48;

fn require(ok: bool, what: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(SaveError::NotBetweenMissions(what))
    }
}

fn read_empty_container(r: &mut Reader<'_>, what: &'static str) -> Result<()> {
    r.version(what, 1, 1)?;
    require(r.u32()? == 0, what)
}

fn write_empty_container(w: &mut Writer) {
    w.u8(1);
    w.u32(0);
}

fn read_zero_i32(r: &mut Reader<'_>, what: &'static str) -> Result<()> {
    require(r.i32()? == 0, what)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivingWorldBlock {
    pub next_army_id: u32,
    pub next_building_id: u32,
    pub next_build_plot_id: u32,
    pub next_spawn_army_id: u32,
    pub player_id: u32,
}

impl LivingWorldBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        r.version(
            "LivingWorldLogic",
            LIVING_WORLD_VERSION,
            LIVING_WORLD_VERSION,
        )?;
        let block = Self {
            next_army_id: r.u32()?,
            next_building_id: r.u32()?,
            next_build_plot_id: r.u32()?,
            next_spawn_army_id: r.u32()?,
            player_id: r.u32()?,
        };
        require(!r.bool()?, "LivingWorldLogic session active")?;
        r.finish("CHUNK_LivingWorldLogic")?;
        Ok(block)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.u8(LIVING_WORLD_VERSION);
        w.u32(self.next_army_id);
        w.u32(self.next_building_id);
        w.u32(self.next_build_plot_id);
        w.u32(self.next_spawn_army_id);
        w.u32(self.player_id);
        w.bool(false);
        w.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MusicSystemState {
    pub music_system: u32,
    pub volumes: [f32; VOLUMES],
    pub level: f32,
    pub audio_affect: u32,
}

impl MusicSystemState {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let music_system = r.u32()?;
        r.version("MusicSubState", 1, 1)?;
        let mut volumes = [0.0f32; VOLUMES];
        for v in &mut volumes {
            *v = r.f32()?;
        }
        read_empty_container(r, "MusicSubState set")?;
        let level = r.f32()?;
        require(!r.bool()?, "MusicSubState extra volumes")?;
        read_empty_container(r, "MusicSubState map")?;
        read_empty_container(r, "MusicSystem set")?;
        Ok(Self {
            music_system,
            volumes,
            level,
            audio_affect: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u32(self.music_system);
        w.u8(1);
        for v in self.volumes {
            w.f32(v);
        }
        write_empty_container(w);
        w.f32(self.level);
        w.bool(false);
        write_empty_container(w);
        write_empty_container(w);
        w.u32(self.audio_affect);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioBlock {
    pub view_type: u32,
    pub view_type_bits: u32,
    pub flag: bool,
    pub music: [MusicSystemState; MUSIC_SYSTEMS],
    pub counter: i32,
    pub client_frame: u32,
}

impl AudioBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        r.version("Audio", AUDIO_VERSION, AUDIO_VERSION)?;
        let view_type = r.u32()?;
        let view_type_bits = r.u32()?;
        let flag = r.bool()?;
        let first = MusicSystemState::read(&mut r)?;
        let second = MusicSystemState::read(&mut r)?;
        read_zero_i32(&mut r, "Audio vector")?;
        let counter = r.i32()?;
        let client_frame = r.u32()?;
        read_zero_i32(&mut r, "Audio playing events")?;
        for _ in 0..MUSIC_SYSTEMS {
            read_zero_i32(&mut r, "Audio music events")?;
            read_zero_i32(&mut r, "Audio music events")?;
        }
        read_empty_container(&mut r, "Audio wide container")?;
        read_empty_container(&mut r, "Audio ascii container")?;
        r.finish("CHUNK_Audio")?;
        Ok(Self {
            view_type,
            view_type_bits,
            flag,
            music: [first, second],
            counter,
            client_frame,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.u8(AUDIO_VERSION);
        w.u32(self.view_type);
        w.u32(self.view_type_bits);
        w.bool(self.flag);
        for m in &self.music {
            m.write(&mut w);
        }
        w.i32(0);
        w.i32(self.counter);
        w.u32(self.client_frame);
        w.i32(0);
        for _ in 0..MUSIC_SYSTEMS {
            w.i32(0);
            w.i32(0);
        }
        write_empty_container(&mut w);
        write_empty_container(&mut w);
        w.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameStateMapBlock {
    pub game_mode: i32,
    pub session_kind: i32,
    pub next_object_id: u32,
    pub next_drawable_id: u32,
}

impl GameStateMapBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        r.version(
            "GameStateMap",
            GAME_STATE_MAP_VERSION,
            GAME_STATE_MAP_VERSION,
        )?;
        require(r.ascii()?.0.is_empty(), "GameStateMap map name")?;
        let game_mode = r.i32()?;
        let session_kind = r.i32()?;
        require(
            game_mode != SKIRMISH_MODE && session_kind != 0,
            "GameStateMap skirmish info",
        )?;
        let block = Self {
            game_mode,
            session_kind,
            next_object_id: r.u32()?,
            next_drawable_id: r.u32()?,
        };
        r.finish("CHUNK_GameStateMap")?;
        Ok(block)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.u8(GAME_STATE_MAP_VERSION);
        w.ascii(&AsciiString::default());
        w.i32(self.game_mode);
        w.i32(self.session_kind);
        w.u32(self.next_object_id);
        w.u32(self.next_drawable_id);
        w.into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLogicBlock {
    pub game_mode: i32,
    pub session_kind: i32,
    pub frame: u32,
    pub field_17c: i32,
    pub flag_11c: bool,
    pub invisibility_counter: u32,
    pub scoring_enabled: bool,
    pub rank_level_limit: i32,
    pub show_behind_building_markers: bool,
    pub draw_icon_ui: bool,
    pub show_dynamic_lod: bool,
    pub script_hulk_max_lifetime_override: i32,
    pub game_difficulty: u32,
    pub field_180: u32,
    pub rank_points_at_start: i32,
    pub flag_9f: bool,
}

impl GameLogicBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        r.version("GameLogic", GAME_LOGIC_VERSION, GAME_LOGIC_VERSION)?;
        let game_mode = r.i32()?;
        let session_kind = r.i32()?;
        require(
            game_mode != SKIRMISH_MODE && session_kind != 0,
            "GameLogic skirmish info",
        )?;
        let frame = r.u32()?;
        let field_17c = r.i32()?;
        let flag_11c = r.bool()?;
        read_empty_container(&mut r, "GameLogic object table")?;
        require(r.u32()? == 0, "GameLogic objects")?;
        r.version("CaveSystem", 1, 1)?;
        require(r.u16()? == 0, "CaveSystem tunnels")?;
        r.version("TerrainResourceManager", 3, 3)?;
        for _ in 0..TERRAIN_RESOURCE_ZEROS {
            require(r.u8()? == 0, "TerrainResourceManager contents")?;
        }
        r.version("DOTManager", 1, 1)?;
        read_zero_i32(&mut r, "DOTManager entries")?;
        r.version("InvisibilityManager", 2, 2)?;
        let invisibility_counter = r.u32()?;
        require(r.u32()? == 0, "InvisibilityManager entries")?;
        r.version("LargeGroupAudio", 5, 5)?;
        require(r.bool()?, "LargeGroupAudio active")?;
        let scoring_enabled = r.bool()?;
        let rank_level_limit = r.i32()?;
        require(r.ascii()?.0.is_empty(), "GameLogic buildable status")?;
        let show_behind_building_markers = r.bool()?;
        let draw_icon_ui = r.bool()?;
        let show_dynamic_lod = r.bool()?;
        let script_hulk_max_lifetime_override = r.i32()?;
        let game_difficulty = r.u32()?;
        let field_180 = r.u32()?;
        require(r.i32()? == -1, "GameLogic list entries")?;
        let rank_points_at_start = r.i32()?;
        let flag_9f = r.bool()?;
        r.version("DelayedExperienceLevelGrantSystem", 1, 1)?;
        read_zero_i32(&mut r, "DelayedExperienceLevelGrantSystem entries")?;
        r.finish("CHUNK_GameLogic")?;
        Ok(Self {
            game_mode,
            session_kind,
            frame,
            field_17c,
            flag_11c,
            invisibility_counter,
            scoring_enabled,
            rank_level_limit,
            show_behind_building_markers,
            draw_icon_ui,
            show_dynamic_lod,
            script_hulk_max_lifetime_override,
            game_difficulty,
            field_180,
            rank_points_at_start,
            flag_9f,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.u8(GAME_LOGIC_VERSION);
        w.i32(self.game_mode);
        w.i32(self.session_kind);
        w.u32(self.frame);
        w.i32(self.field_17c);
        w.bool(self.flag_11c);
        write_empty_container(&mut w);
        w.u32(0);
        w.u8(1);
        w.u16(0);
        w.u8(3);
        w.bytes(&[0u8; TERRAIN_RESOURCE_ZEROS]);
        w.u8(1);
        w.i32(0);
        w.u8(2);
        w.u32(self.invisibility_counter);
        w.u32(0);
        w.u8(5);
        w.bool(true);
        w.bool(self.scoring_enabled);
        w.i32(self.rank_level_limit);
        w.ascii(&AsciiString::default());
        w.bool(self.show_behind_building_markers);
        w.bool(self.draw_icon_ui);
        w.bool(self.show_dynamic_lod);
        w.i32(self.script_hulk_max_lifetime_override);
        w.u32(self.game_difficulty);
        w.u32(self.field_180);
        w.i32(-1);
        w.i32(self.rank_points_at_start);
        w.bool(self.flag_9f);
        w.u8(1);
        w.i32(0);
        w.into_bytes()
    }
}

#[must_use]
pub fn between_missions_defaults() -> (
    LivingWorldBlock,
    AudioBlock,
    GameStateMapBlock,
    GameLogicBlock,
) {
    let music = MusicSystemState {
        music_system: 0,
        volumes: [1.0; VOLUMES],
        level: 1.0,
        audio_affect: 0,
    };
    (
        LivingWorldBlock {
            next_army_id: 0,
            next_building_id: 0,
            next_build_plot_id: 0,
            next_spawn_army_id: 1094,
            player_id: u32::MAX,
        },
        AudioBlock {
            view_type: 2,
            view_type_bits: 0,
            flag: false,
            music: [music.clone(), music],
            counter: 0,
            client_frame: 0,
        },
        GameStateMapBlock {
            game_mode: 9,
            session_kind: 3,
            next_object_id: 1,
            next_drawable_id: 1,
        },
        GameLogicBlock {
            game_mode: 9,
            session_kind: 3,
            frame: 0,
            field_17c: 2,
            flag_11c: false,
            invisibility_counter: 5,
            scoring_enabled: true,
            rank_level_limit: 1000,
            show_behind_building_markers: true,
            draw_icon_ui: true,
            show_dynamic_lod: true,
            script_hulk_max_lifetime_override: -1,
            game_difficulty: 1,
            field_180: 0,
            rank_points_at_start: 0,
            flag_9f: true,
        },
    )
}
