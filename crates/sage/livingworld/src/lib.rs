#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod ai;
pub mod battle;
mod bonus_text;
mod bonuses;
pub mod economy;
mod engine;
pub mod harness;
mod hash;
pub mod model;
mod moves;
mod queue;
mod retreat;
pub mod rules;
pub mod save;
pub mod screen_queries;
mod screens;
mod setup;
pub mod stand_in;
pub mod state;
pub mod tutorial;
mod victory;

#[cfg(test)]
mod tests;

pub use ai::{AiSeat, BattleAnswer, Predictor, Skill, StrategicAi};
pub use battle::{battle_sides, AutoResolve, BattleSpec, SpecSide};
pub use bonus_text::{
    bonus_lines, region_bonus, territory_bonus, territory_name, territory_of, territory_text,
};
pub use engine::{apply, apply_with};
pub use model::*;
pub use rules::{Catalog, PriceClass, RankGrant, Rules, SetupError, UnitInfo};
pub use setup::{
    assign_start_regions, clear_start_region, default_start_regions, reset_start_regions,
    set_seat_faction, set_start_region, PlayerSetup, Setup,
};
pub use state::{LivingWorldState, PalantirMarker};
