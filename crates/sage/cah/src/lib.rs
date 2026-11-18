#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod crc;
mod hero;
pub mod stats;

pub use crc::Crc32;
pub use hero::{
    compute_crc, file_name, id_from_guid, parse, write, CahError, HeroRecord, ParsedHero,
    PowerEntry, POWER_SLOTS,
};

pub type Result<T> = core::result::Result<T, CahError>;
