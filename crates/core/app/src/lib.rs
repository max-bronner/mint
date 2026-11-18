#![deny(clippy::arithmetic_side_effects)]

mod clock;
mod frame_log;
mod input;
mod orbit;
mod rts_camera;
mod window;

pub use clock::{FixedTimestep, SIMULATION_HZ};
pub use frame_log::{FrameLog, FrameMarks};
pub use input::{InputEvent, Key, PointerButton};
pub use orbit::OrbitCamera;
pub use rts_camera::{RtsCamera, DEFAULT_EDGE_SCROLL_MARGIN};
pub use window::{run, App, CursorRequest, WindowConfig};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{operation} failed")]
    EventLoop {
        operation: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("the renderer failed")]
    Render(#[from] mint_render::Error),

    #[error("the application failed to start")]
    App(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("the application has already been built")]
    AlreadyStarted,
}

impl Error {
    pub(crate) fn event_loop<E>(operation: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::EventLoop {
            operation,
            source: Box::new(source),
        }
    }
}
