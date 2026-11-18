pub mod animation;
pub mod cache;
pub mod convert;
pub mod gpu_skin;
pub mod load;
pub mod objects;
pub mod textures;

pub use animation::{faded, BindRefusal, Completion, ModelAnimation, PartPose};
pub use cache::{
    CachedModel, CachedPart, ModelCache, ModelCacheStats, PartTextures, INFANTRY_LIGHT_SET,
    OBJECT_LIGHT_SET,
};
pub use convert::{convert_model, ConvertedModel, Part};
pub use gpu_skin::{GpuSkin, GpuSkinnedPart};
pub use load::{find_models, load_animation, load_model, LoadedAnimation, LoadedModel};
pub use objects::{
    load_house_colors, load_multiplayer_colors, load_object_index, load_object_index_with_names,
    object_index_from_documents, ObjectIndexReport,
};
pub use textures::{load_bump_map, load_normal_map, load_texture, missing_texture, LoadedTexture};

pub use mint_sage_asset::{PoseBuffers, Resolver};
pub use mint_sage_ini::{
    AnimationFlags, AnimationMode, DefaultAnimation, NoModelReason, ObjectIndex, Template,
};

use mint_vfs::VfsPath;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("reading from the mounted archives failed")]
    Vfs(#[from] mint_vfs::Error),

    #[error("the model did not parse")]
    W3d(#[from] mint_sage_w3d::Error),

    #[error("the texture did not decode")]
    Image(#[from] mint_image::Error),

    #[error("the renderer failed")]
    Render(#[from] mint_render::Error),

    #[error("nothing in the mounted archives supplies {name}")]
    Unresolved {
        name: String,
        #[source]
        source: mint_sage_asset::Error,
    },

    #[error("{name} resolved to {path}, and .{extension} is not a format mint-image decodes")]
    UnsupportedTexture {
        name: String,
        path: VfsPath,
        extension: String,
    },

    #[error("a decoded image's buffer does not match its declared size")]
    MalformedDecodedImage,

    #[error("{sidecar} is {found:?}, but the image it carries the alpha of is {expected:?}")]
    AlphaSidecarSize {
        sidecar: String,
        expected: (u32, u32),
        found: (u32, u32),
    },

    #[error("{name} contains no meshes")]
    NoGeometry { name: String },

    #[error("{name} contains no animation (found: {found})")]
    NoAnimation { name: String, found: String },

    #[error(
        "{animation} names hierarchy {hierarchy}, which no mounted file supplies; tried {}",
        .tried.join(", ")
    )]
    SkeletonNotFound {
        animation: String,
        hierarchy: String,
        tried: Vec<String>,
    },
}
