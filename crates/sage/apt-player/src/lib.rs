#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod actions;
mod arena;
pub mod avm;
pub mod expose;
mod geom;
mod instance;
mod library;
mod load;
mod mouse;
mod player;
mod render;
mod timeline;
pub mod trace;

#[cfg(test)]
mod expose_tests;
#[cfg(test)]
mod mouse_tests;
#[cfg(any(test, feature = "synth-testing"))]
pub mod synth;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vm_tests;

pub use actions::{clip_event, ActionQueue, ActionSource, Priority, QueuedAction};
pub use arena::{Arena, Full, Handle};
pub use avm::{Halt, ObjectId, Outward, ScriptStats, SharedGlobal, SyncHost, Value, VmLimits};
pub use expose::{Exposure, ExposureStats, ImageChange, ImageKey, MovieEvent, Readiness};
pub use geom::{ColorTransform, Matrix};
pub use instance::{ButtonVisual, ClipState, Instance, InstanceId, InstanceKind};
pub use library::{
    ButtonHit, CharacterRef, Library, LibraryMovie, LoadReport, MovieId, Timeline, TimelineId,
    MAX_FRAMES, MAX_IMPORT_CHASE,
};
pub use mouse::{HitArea, HitMode};
pub use player::{Counters, EventKind, HeapStats, Limits, Player, TimelineEvent};
pub use render::{RenderCommand, RenderList, RenderOptions};
pub use timeline::AVM_DEPTH_BIAS;
pub use trace::{enabled as trace_enabled, set_sink as set_trace_sink, Sink as TraceSink};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("{movie}: {source}")]
    Parse {
        movie: String,
        source: mint_sage_apt::Error,
    },
    #[error("{what} limit reached ({limit})")]
    Limit { what: &'static str, limit: usize },
    #[error("no movie {0} in the library")]
    UnknownMovie(String),
    #[error("the library must be linked before it is played")]
    Unlinked,
}
