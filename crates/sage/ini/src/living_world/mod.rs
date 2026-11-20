pub mod auto_resolve;
pub mod campaign;
pub mod effects;
pub(crate) mod grammar;
pub mod hud;
pub mod load;
pub mod players;
pub mod presentation;
pub mod region;
pub mod resolve;

mod data;

#[cfg(test)]
mod tests;

pub use data::{BlockCount, LivingWorldData, ROOT_KEYWORDS};
pub use grammar::{
    Bits, ColorInt, Coord2, Coord3, Defined, EnumList, FontSpec, Millis, Percent, Rgb, ShadowBits,
    Store, TeamList, TokenList, Tokens,
};
pub use load::{
    check_block, load_files, load_living_world, read_documents, LivingWorldLoad, LoadError,
    SUBSYSTEM_LEGEND_PATHS,
};
pub use resolve::{resolve, ExternalNames, RegionGraph, Unresolved};
