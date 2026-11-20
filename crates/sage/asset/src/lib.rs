#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod catalogue;
mod hero_store;
mod kind;
mod kindof;
mod library;
mod pose;
mod resolve;
mod sway;
#[cfg(test)]
mod synth;
mod user_data;
mod uv_mapper;

pub use catalogue::{AssetEntry, AssetRecord, Catalogue, DependencyRecord, MAX_FILE_LEN, VERSION};
pub use hero_store::{read_stats, write_stats, HeroSource, HeroStore, StoredHero};
pub use kind::AssetKind;
pub use kindof::{KindOf, PickType};
pub use library::{AssetLibrary, ResolvedAsset};
pub use pose::{
    pivot_reach, sample_blend_into, sample_pose, sample_pose_into, PoseBuffers, SampledPose,
    Transform,
};
pub use resolve::{alpha_sidecar, Resolver, DEFAULT_TEXTURE_EXTENSIONS};
pub use sway::{sway_index, Breeze, MAX_SWAY_TYPES, SWAY_OFFSET_COUNT};
pub use user_data::{install_info_value, user_data_leaf, SAVE_FOLDER};
pub use uv_mapper::{
    sample_material_uv_transforms, sample_uv_transform, MaterialUvTransforms, UvTransform,
};

use std::path::PathBuf;

use mint_vfs::VfsPath;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path} is {len} bytes, over the {limit}-byte limit for a catalogue")]
    FileTooLarge { path: PathBuf, len: u64, limit: u64 },

    #[error("{catalogue} is {len} bytes, too short for a 16-byte catalogue header")]
    HeaderTruncated { catalogue: String, len: usize },

    #[error("{catalogue} is not an asset catalogue: magic is {magic:02x?}, not \"ALAE\"")]
    NotCatalogue { catalogue: String, magic: [u8; 4] },

    #[error("{catalogue} is version {version:#010x}, and this reader only knows {known:#010x}")]
    UnsupportedVersion {
        catalogue: String,
        version: u32,
        known: u32,
    },

    #[error(
        "{catalogue}: {count} {what} claimed at byte {offset} need at least \
         {needed} bytes, but only {available} remain"
    )]
    CountTooLarge {
        catalogue: String,
        what: &'static str,
        count: u64,
        needed: u64,
        available: u64,
        offset: u64,
    },

    #[error(
        "{catalogue}: {what} at byte {offset} needs {needed} bytes, \
         but only {available} remain"
    )]
    Truncated {
        catalogue: String,
        what: &'static str,
        offset: u64,
        needed: u64,
        available: u64,
    },

    #[error("{catalogue}: {what} at byte {offset} is zero-length")]
    EmptyName {
        catalogue: String,
        what: &'static str,
        offset: u64,
    },

    #[error("{catalogue}: dependencies recorded for {file} :: {sub_asset}, which does not exist")]
    UnknownDependencyOwner {
        catalogue: String,
        file: String,
        sub_asset: String,
    },

    #[error("{catalogue}: {file} :: {sub_asset} references {reference}, which does not exist")]
    DanglingReference {
        catalogue: String,
        file: String,
        sub_asset: String,
        reference: String,
    },

    #[error("{entry} spans {offset}..{end}, but its file is {file_len} bytes")]
    EntryOutOfBounds {
        entry: String,
        offset: u32,
        end: u64,
        file_len: u64,
    },

    #[error("{name:?} is not a usable logical asset name: {reason}")]
    BadLogicalName { name: String, reason: &'static str },

    #[error(
        "no mounted file for logical name {name}; tried {}",
        .tried.iter().map(VfsPath::as_str).collect::<Vec<_>>().join(", ")
    )]
    Unresolved { name: String, tried: Vec<VfsPath> },
}
