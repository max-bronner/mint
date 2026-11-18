use crate::xfer::{AsciiString, Reader, WideString, Writer};
use crate::Result;

pub const CHUNK_GAME_STATE: &[u8] = b"CHUNK_GameState";

const OUTER_VERSION: u8 = 1;
const INFO_VERSION: u8 = 3;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameStateBlock {
    pub save_file_type: u32,
    pub mission_map_name: AsciiString,
    pub date: [u16; 8],
    pub description: WideString,
    pub map_label: AsciiString,
    pub is_auto_save: u32,
    pub extra_a: WideString,
    pub extra_b: WideString,
}

impl GameStateBlock {
    pub fn decode(payload: &[u8], payload_offset: usize) -> Result<Self> {
        let mut r = Reader::new(payload, payload_offset);
        let block = Self::read(&mut r)?;
        r.finish("CHUNK_GameState")?;
        Ok(block)
    }

    pub fn decode_info(payload: &[u8], payload_offset: usize) -> Result<Self> {
        Self::read(&mut Reader::new(payload, payload_offset))
    }

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.version("GameState", OUTER_VERSION, OUTER_VERSION)?;
        r.version("SaveGameInfo", INFO_VERSION, INFO_VERSION)?;
        let save_file_type = r.u32()?;
        let mission_map_name = r.ascii()?;
        let mut date = [0u16; 8];
        for slot in &mut date {
            *slot = r.u16()?;
        }
        Ok(Self {
            save_file_type,
            mission_map_name,
            date,
            description: r.wide()?,
            map_label: r.ascii()?,
            is_auto_save: r.u32()?,
            extra_a: r.wide()?,
            extra_b: r.wide()?,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.u8(OUTER_VERSION);
        w.u8(INFO_VERSION);
        w.u32(self.save_file_type);
        w.ascii(&self.mission_map_name);
        for unit in self.date {
            w.u16(unit);
        }
        w.wide(&self.description);
        w.ascii(&self.map_label);
        w.u32(self.is_auto_save);
        w.wide(&self.extra_a);
        w.wide(&self.extra_b);
        w.into_bytes()
    }
}
