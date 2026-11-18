#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod dir;
mod mount;
mod path;
#[cfg(test)]
mod testutil;
mod vfs;

pub use dir::DirMount;
pub use mount::Mount;
pub use path::VfsPath;
pub use vfs::{MountReport, Vfs};

use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no such file in the virtual filesystem: {path}")]
    NotFound { path: VfsPath },

    #[error("{path} nests deeper than the {limit}-directory limit")]
    TooDeep { path: PathBuf, limit: usize },

    #[error("mount {mount} failed to read {path}")]
    Mount {
        mount: String,
        path: VfsPath,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}
