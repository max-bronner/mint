#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod between_missions;
mod campaign;
mod container;
mod error;
mod game_state;
mod listing;
mod xfer;

#[cfg(test)]
mod tests;

pub use between_missions::{
    between_missions_defaults, AudioBlock, GameLogicBlock, GameStateMapBlock, LivingWorldBlock,
    MusicSystemState, CHUNK_AUDIO, CHUNK_GAME_LOGIC, CHUNK_GAME_STATE_MAP,
    CHUNK_LIVING_WORLD_LOGIC,
};
pub use campaign::{
    CampaignBlock, CampaignState, CarriedSub, CarryoverUnit, CurrentCampaign, UpgradeList,
    CHUNK_CAMPAIGN,
};
pub use container::{RawBlock, SaveFile, END_TOKEN, HEADER_LEN, MAGIC};
pub use error::{Result, SaveError};
pub use game_state::{GameStateBlock, CHUNK_GAME_STATE};
pub use listing::{
    default_save_name, extension, has_retail_extension, is_scratch_map_name, mint_extension,
    read_info, sort_newest_first, tab_accepts, SaveDate, SaveTab,
};
pub use xfer::{AsciiString, Reader, WideString, Writer, MAX_NEST_DEPTH};
