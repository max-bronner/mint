#![deny(clippy::arithmetic_side_effects)]

mod blend;
mod camera;
mod custom;
mod depth;
mod detail;
mod envmap;
mod fade;
mod frustum;
mod math;
mod mesh;
mod modulate;
mod pipeline_cache;
mod post;
mod renderer;
mod shadow;
mod skin;
mod texture;
mod ui;

pub use blend::Blend;
pub use camera::{Camera, Ray, DEFAULT_FOV_Y};
pub use custom::{
    CustomMaterialHandle, CustomMaterialSource, CustomShader, CustomShaderHandle,
    MAX_CUSTOM_TEXTURE_SLOTS, MAX_CUSTOM_UNIFORM_SIZE,
};
pub use depth::{DepthDirection, DepthTest};
pub use detail::{DetailAlphaCombine, DetailCombine};
pub use envmap::EnvMap;
pub use fade::{FadeBlend, ScreenFade};
pub use frustum::{transformed_bounds, Frustum};
pub use math::{Mat4, Vec3};
pub use mesh::{Aabb, MeshData, Vertex};
pub use modulate::Modulate2x;
pub use pipeline_cache::PipelineCacheState;
pub use post::{ColorLut, ColorVolumeHandle, Glow, GlowTap, PostEffects};
pub use renderer::{
    DeviceInfo, DirectionalLight, DistanceFog, Instance, LightCookie, LightSet, MeshHandle,
    RenderTargetHandle, Renderer, Scene, SurfaceTarget, TextureHandle, VisibilityMask, LIGHT_SETS,
    MAX_ANISOTROPY, MAX_DIRECTIONAL_LIGHTS,
};
pub use shadow::{
    directional_light_view_proj, fit_directional_light_to_view, ContactHardening, ContactShadows,
    FittedShadow, ShadowCascade, ShadowEdge, ShadowFit, ShadowMapHandle, ShadowSample,
    MAX_SHADOW_CASCADES,
};
pub use skin::{skin_point, SkinVertex};
pub use texture::{AddressMode, MipLevel, PixelFormat, TextureData};
pub use ui::{MaskShape, ScreenDraw, UiGeometry, UiQuad};

pub const CUSTOM_SHADER_PRELUDE: &str = concat!(
    include_str!("instance_block.wgsl"),
    include_str!("custom_vertex.wgsl")
);

pub const SCENE_DEPTH_PRELUDE: &str =
    "\n@group(2) @binding(0) var mint_scene_depth: texture_depth_2d;\n";

pub const SCENE_COLOR_PRELUDE: &str = concat!(
    "\n@group(2) @binding(1) var mint_scene_color: texture_2d<f32>;\n",
    "@group(2) @binding(2) var mint_scene_color_sampler: sampler;\n"
);

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("no graphics adapter is available")]
    NoAdapter,

    #[error("{operation} failed")]
    Backend {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("the graphics device did not respond")]
    DeviceTimeout,

    #[error("the window surface is not usable with this adapter")]
    SurfaceUnsupported,

    #[error("a mesh must have at least one vertex and one triangle")]
    EmptyMesh,

    #[error("index {at} points outside the {vertices}-vertex array")]
    BadIndex { at: usize, vertices: usize },

    #[error("{count} indices is more than an index buffer can hold")]
    TooManyIndices { count: usize },

    #[error("{count} instances in one frame is more than the uniform buffer can address")]
    TooManyInstances { count: usize },

    #[error("the texture has no usable mip level")]
    EmptyTexture,

    #[error("this device cannot upload BC-compressed blocks")]
    BlockCompressionUnsupported,

    #[error("texture is {width}x{height}, over this device's {limit}-pixel limit")]
    TextureTooLarge { width: u32, height: u32, limit: u32 },

    #[error(
        "that texture is {width}x{height}; the replacement is          {supplied_width}x{supplied_height} or a different format"
    )]
    TextureShapeMismatch {
        width: u32,
        height: u32,
        supplied_width: u32,
        supplied_height: u32,
    },

    #[error("that mesh or texture handle was not issued by this renderer")]
    UnknownHandle,

    #[error("a custom shader asked for {asked} texture slots, over the limit of {limit}")]
    TooManyTextureSlots { asked: u32, limit: u32 },

    #[error("a custom shader asked for a {asked}-byte uniform block, over the limit of {limit}")]
    CustomUniformTooLarge { asked: u32, limit: u32 },

    #[error("this custom shader declared {declared} {what}, the caller supplied {supplied}")]
    CustomMaterialMismatch {
        what: &'static str,
        declared: usize,
        supplied: usize,
    },

    #[error("the custom shader {label:?} was rejected: {message}")]
    CustomShaderRejected { label: String, message: String },

    #[error(
        "the custom shader `{label}` reads the frame's own colour or depth, so \
         it cannot be drawn opaque: the opaque pass is still writing them"
    )]
    SceneDepthWritesDepth { label: String },

    #[error(
        "the custom shader `{label}` reads the frame's own colour or depth, so          it cannot be drawn into a render target"
    )]
    FrameInputsOffTarget { label: String },

    #[error("this surface cannot be copied from, so the scene colour cannot be captured")]
    SceneColorUnsupported,

    #[error("only an offscreen renderer can read pixels back")]
    NotOffscreen,

    #[error("reading the rendered frame back from the GPU failed")]
    ReadbackFailed,

    #[error("mesh was uploaded with {original} vertices, the update supplied {supplied}")]
    VertexCountMismatch { original: usize, supplied: usize },

    #[error("a skinned mesh has {vertices} vertices and {skin} skin entries")]
    SkinLengthMismatch { vertices: usize, skin: usize },

    #[error(
        "a skinned instance needs bones {base}..{end} but the palette holds {palette} \
         (or it named no bone_base)"
    )]
    BoneRangeOutOfBounds { base: u32, end: u64, palette: usize },

    #[error("a skinned mesh cannot be drawn through a custom shader")]
    SkinnedCustomShader,

    #[error("the custom shader `{label}` {problem}")]
    CustomMaterialSourceMismatch {
        label: String,
        problem: &'static str,
    },

    #[error("{subject}")]
    About {
        subject: String,
        #[source]
        source: Box<Error>,
    },
}

impl Error {
    #[must_use]
    pub fn about(self, subject: impl Into<String>) -> Self {
        Self::About {
            subject: subject.into(),
            source: Box::new(self),
        }
    }

    pub(crate) fn backend<E>(operation: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Backend {
            operation,
            source: Box::new(source),
        }
    }
}
