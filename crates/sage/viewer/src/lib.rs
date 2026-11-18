pub mod install;
pub mod screenshot;

use std::path::PathBuf;

pub use install::{mount_install, MountSummary};
pub use screenshot::{encode_tga, first_difference, write_tga};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{path} is not a directory")]
    NoInstall { path: PathBuf },

    #[error("the model could not be loaded")]
    Model(#[from] mint_sage_model::Error),

    #[error("the renderer failed")]
    Render(#[from] mint_render::Error),

    #[error("the application failed")]
    App(#[from] mint_app::Error),

    #[error("a {width}x{height} frame needs {} bytes, got {len}", u64::from(*width) * u64::from(*height) * 4)]
    ScreenshotSize { width: u32, height: u32, len: usize },

    #[error("the encoded screenshot differs from the rendered frame at byte {at}")]
    ScreenshotMismatch { at: usize },

    #[error("i/o error on {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{model} has no resolved skeleton; {animation} cannot be used to pose it - nothing here \
         is bound to a bone"
    )]
    NoModelSkeleton { model: String, animation: String },

    #[error(
        "{animation} names hierarchy {hierarchy}, which matches {model}'s own, but moves none \
         of its {parts} part(s) - nothing here is skinned or rigidly bound to a bone"
    )]
    AnimationHasNoEffect {
        animation: String,
        hierarchy: String,
        model: String,
        parts: usize,
    },
}
