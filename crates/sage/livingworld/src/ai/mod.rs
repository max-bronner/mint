#![allow(missing_docs)]

pub mod catalogue;
mod construct;
pub mod data;
mod garrison;
mod hero;
mod recruit;
mod retreat;
mod rolls;
mod save;
pub mod snapshot;
mod turn;

pub use construct::{BuildAction, BuildOrder};
pub use data::{AiRules, AiTemplate};
pub use recruit::RecruitOrder;
pub use retreat::BattleAnswer;
pub use rolls::{Roll, Rolls};
pub use turn::{AiSeat, Skill, StrategicAi, CAMPAIGN_DIFFICULTY};

use mint_math::rng::Pcg32;

use crate::battle::BattleSpec;
use crate::model::{ArmyId, PlayerId, RegionId};
use crate::state::LivingWorldState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub weapon: f32,
    pub armor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prediction {
    pub winner: Option<u8>,
    pub rounds: u32,
}

pub trait Predictor {
    fn predict(
        &mut self,
        spec: &BattleSpec,
        ai_side: u8,
        scale: Option<Scale>,
        rng: &mut Pcg32,
    ) -> Prediction;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    Move { region: RegionId },
    Transfer { to: ArmyId, member: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveOrder {
    pub army: ArmyId,
    pub heroless: bool,
    pub kind: MoveKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub recruits: Vec<RecruitOrder>,
    pub builds: Vec<BuildOrder>,
    pub moves: Vec<MoveOrder>,
}

pub(crate) struct Ctx<'a> {
    pub state: &'a LivingWorldState,
    pub snap: &'a snapshot::Snapshot,
    pub catalogue: &'a catalogue::Catalogue,
    pub template: &'a AiTemplate,
    pub me: PlayerId,
    pub difficulty: u8,
    pub rolls: &'a mut Rolls,
    pub predictor: &'a mut dyn Predictor,
}

#[cfg(test)]
pub(crate) mod tests;
