#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod ai_power;
mod command;
pub mod module;
pub mod replay;
mod rules;
mod simulation;
mod time;
mod upgrade;

pub use command::{Command, CommandBus, CommandKey, Envelope, PlayerId, MAX_PENDING_COMMANDS};
pub use module::{
    ActivateContext, Activation, Capability, CapabilityMask, CaptureContext, CreateContext,
    CreatedBy, Death, DieContext, Effects, FieldError, ModuleFieldSource, ModuleKind,
    ModuleRegistry, ModuleTypeId, Phase, Prototype, PrototypeOutcome, Reads, Requester, Sleep,
    UpdateContext, UpgradeContext,
};
pub use replay::{
    commands_for_frame, describe, first_divergence, is_ordered, Replay, VerifiedReplay,
    REPLAY_FORMAT_VERSION,
};
pub use rules::{Rules, Tick};
pub use simulation::{FrameHash, FrameReport, Simulation};
pub use time::{Frame, LogicFrames, SIMULATION_HZ};
pub use upgrade::{UpgradeBit, UpgradeMask};

pub use mint_ecs::{
    join, Component, ComponentId, ComponentStore, Entity, HashedVec, StateHasher, World, PAGE_SLOTS,
};
pub use mint_math::rng::Pcg32;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("entity storage: {0}")]
    Ecs(#[from] mint_ecs::Error),

    #[error("command targets frame {target}, but frame {next} is next to run")]
    CommandTooLate { target: u64, next: u64 },

    #[error("command {frame}/player {player}#{sequence} is already queued")]
    DuplicateCommand {
        frame: u64,
        player: u8,
        sequence: u32,
    },

    #[error("command queue is full at {limit} pending commands")]
    CommandQueueFull { limit: usize },

    #[error("player {player} has exhausted its command sequence numbers")]
    SequenceExhausted { player: u8 },

    #[error("the frame counter has reached the end of its range")]
    TimelineExhausted,

    #[error("frame {frame}: {reason}")]
    Rules { frame: u64, reason: String },

    #[error("in {rule}")]
    InRule {
        rule: std::borrow::Cow<'static, str>,
        #[source]
        source: Box<Error>,
    },

    #[error("the simulation is poisoned by an earlier frame's failure")]
    Poisoned,

    #[error("the initial state differs: recorded {expected:#018x}, got {actual:#018x}")]
    SetupDivergence { expected: u64, actual: u64 },

    #[error("diverged at frame {frame}: recorded {expected:#018x}, got {actual:#018x}")]
    Divergence {
        frame: u64,
        expected: u64,
        actual: u64,
    },

    #[error("module type was never registered with this registry")]
    ModuleTypeNotRegistered,

    #[error("module '{name}' is not registered, and this registry is strict")]
    UnregisteredModule { name: String },

    #[error(
        "{remaining} recorded commands are for frames past the recording, from frame {first_frame}"
    )]
    UnconsumedCommands { remaining: usize, first_frame: u64 },

    #[error(
        "frame {frame}: incremental world hash {incremental:#018x}, from scratch {full:#018x}"
    )]
    StaleHashCache {
        frame: u64,
        incremental: u64,
        full: u64,
    },
}

impl Error {
    #[must_use]
    pub fn in_rule(rule: impl Into<std::borrow::Cow<'static, str>>, source: Self) -> Self {
        Self::InRule {
            rule: rule.into(),
            source: Box::new(source),
        }
    }
}
