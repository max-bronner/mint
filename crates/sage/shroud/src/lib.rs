#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod disc;
pub mod grid;
pub mod object;
pub mod sighting;

pub use disc::{circle_fill, small_fill, DiscreteCircle, Span, MAX_CELL_RADIUS};
pub use grid::{
    CellVisibility, Error, GridExtent, PlayerMask, VisibilityGrid, MAX_GRID_SLOTS, MAX_PLAYERS,
};
pub use object::{cells_touched, shroudedness, ObjectTraits, ObjectVisibility, Shroudedness};
pub use sighting::{
    Looker, LookerSet, ReconcileReport, Sighting, UndoQueue, VisionShape, DEFAULT_UNLOOK_PERSIST,
    MAX_PENDING_UNDOS, MAX_WATCHERS,
};

pub type Result<T> = core::result::Result<T, Error>;

pub const PARTITION_CELL_SIZE: f32 = 40.0;

pub const SHROUD_CLEAR_STANDARD: f32 = 25.0;
