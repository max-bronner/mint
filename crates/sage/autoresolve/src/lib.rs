#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod battle;
pub mod chain;
pub mod modifiers;
pub mod roster;
mod stl;
pub mod tables;

pub use battle::{resolve, ROUND_CAP};
pub use roster::{Army, Member, Options, Outcome, Participant};
pub use tables::Tables;
