#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod entity;
mod hash;
mod hashed_vec;
mod pages;
mod store;
mod world;

pub use entity::{Entity, EntityAllocator};
pub use hash::StateHasher;
pub use hashed_vec::HashedVec;
pub use store::{join, join_mut, ComponentStore, PAGE_SLOTS};
pub use world::{Component, ComponentId, World, WorldSnapshot};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("entity space exhausted: all 2^32 entity slots are live or retired")]
    EntitySpaceExhausted,

    #[error("entity {index}v{generation} is not live")]
    StaleEntity { index: u32, generation: u32 },

    #[error("component id {index} is not registered in this world")]
    UnknownComponent { index: usize },
}

pub(crate) const fn to_usize(value: u32) -> usize {
    value as usize
}
