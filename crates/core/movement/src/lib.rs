#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod avoidance;
pub mod destination;
pub mod flight;
pub mod following;
pub mod geom;
pub mod grid;
pub mod height;
pub mod pipeline;
pub mod reachability;
pub mod search;
pub mod smoothing;

pub use avoidance::{
    Agent, Avoidance, AvoidanceMode, MoverId, Neighbour, NeighbourQuery, NoAvoidance, NoNeighbours,
    SeparationAvoidance, Stagger,
};
pub use flight::{FlightPath, Vec3};
pub use following::{
    depart, look_ahead, round_corners, Departure, Follower, LookAhead, Motion, MotionLimits,
    MotionPhase, MotionStatus, MoveContext, MoverState, Ramp, Route, SlidingFollower,
    SteeringFollower, TurnCircle, TurnSlowdown, WheelBacking,
};
pub use geom::{CellGeometry, Facing, Vec2};
pub use grid::{
    Asker, Cell, CellFlags, CellRect, CostGrid, GridExtent, Leg, MoverClass, Passability, Portal,
    PortalLink, PortalNode, PortalPass, MAX_GRID_CELLS,
};
pub use height::{
    attack_height, step_damped, step_level, AttackHeightInput, AttackPhase, HeightField, HeightRule,
};
pub use pipeline::{Pipeline, PipelineConfig, Plan, PlanOutcome, SearchMode};
pub use reachability::Reachability;
pub use search::{
    AStar, CellPath, GoalId, Heuristic, NodeBudget, Octile, PathRequest, PortalHop, Search,
    SearchOutcome, SearchProgress, SearchResult, StraightLineSearch, ZeroHeuristic, COST_DIAGONAL,
    COST_ORTHOGONAL,
};
pub use smoothing::{NoSmoothing, PortalEdge, Smoothing, SmoothingMode, StringPull};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("a grid must have at least one cell in each dimension")]
    EmptyGrid,

    #[error("a grid of {cells} cells exceeds the {limit}-cell limit")]
    GridTooLarge { cells: u64, limit: u32 },

    #[error("cell ({x}, {y}) on layer {layer} is outside the grid")]
    CellOutOfBounds { layer: u8, x: u16, y: u16 },

    #[error("cell size {cell_size} is not a finite positive length")]
    InvalidCellSize { cell_size: f32 },

    #[error("a height field of {expected} nodes was given {given} samples")]
    HeightFieldSize { given: u32, expected: u32 },

    #[error("the search could not reconstruct the path it found")]
    PathReconstruction,
}
