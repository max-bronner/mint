#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod crc;
mod hero;
mod language_filter;
pub mod stats;

pub use crc::Crc32;
pub use hero::{
    compute_crc, file_name, id_from_guid, parse, write, CahError, HeroRecord, ParsedHero,
    PowerEntry, POWER_SLOTS,
};
pub use language_filter::LanguageFilter;

pub type Result<T> = core::result::Result<T, CahError>;
