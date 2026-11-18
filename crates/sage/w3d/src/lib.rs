#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod animation;
mod array;
mod chunk;
mod hierarchy;
mod hlod;
mod material;
mod mesh;
mod model;
mod reader;
#[cfg(test)]
mod testutil;
mod types;

pub use animation::{Channel, ChannelData, ChannelKind, TimeCodedKey, Value};
pub use array::{Array, ArrayIter, Element};
pub use chunk::{Chunk, Chunks, MAX_CHUNK_DEPTH};
pub use hierarchy::{Hierarchy, HierarchyHeader, Pivot};
pub use hlod::{HLod, HLodArray, HLodHeader, HLodSubObject};
pub use material::{
    IndexTable, MapperArgs, MaterialInfo, MaterialPass, Rgb, Rgba, Shader, ShaderMaterial,
    ShaderMaterialProperty, ShaderMaterialValue, Texture, TextureInfo, TextureStage, UvMapperType,
    VertexMaterial, VertexMaterialInfo,
};
pub use mesh::{Mesh, MeshHeader3, Triangle, VertexInfluence};
pub use model::{Animation, Model};
pub use reader::Reader;
pub use types::{ChunkType, Quat, Vec2, Vec3};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(
        "unexpected end of data at offset {offset}: needed {needed} more byte(s), {available} left"
    )]
    UnexpectedEof {
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error(
        "chunk {kind} at offset {offset} claims {declared} payload bytes but only {available} remain"
    )]
    ChunkOverrun {
        kind: ChunkType,
        offset: usize,
        declared: usize,
        available: usize,
    },

    #[error("chunk nesting deeper than {max} levels at offset {offset}")]
    TooDeep { offset: usize, max: u32 },

    #[error("{what} chunk is {actual} bytes, need at least {expected}")]
    ShortStruct {
        what: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("{what} chunk is {actual} bytes, not a multiple of the {stride}-byte element")]
    RaggedArray {
        what: &'static str,
        stride: usize,
        actual: usize,
    },

    #[error("{what} at offset {offset} has last frame {last} before first frame {first}")]
    InvalidFrameRange {
        what: &'static str,
        offset: usize,
        first: u32,
        last: u32,
    },

    #[error("{what} at offset {offset} has unrecognised value {value}")]
    UnknownChannelEncoding {
        what: &'static str,
        offset: usize,
        value: u16,
    },
    #[error("HLOD states {stated} LOD array(s) but holds {found}")]
    MissingLodArrays { stated: u32, found: usize },
}
