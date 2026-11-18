use mint_ecs::{StateHasher, World};
use mint_math::rng::Pcg32;

use crate::command::{CommandBus, CommandKey, Envelope, PlayerId};
use crate::replay::Replay;
use crate::rules::{Rules, Tick};
use crate::time::{Frame, LogicFrames};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameReport {
    pub frame: Frame,
    pub hash: u64,
    pub commands_applied: usize,
}

impl core::fmt::Display for FrameReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} hash={:#018x} commands={}",
            self.frame, self.hash, self.commands_applied
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameHash {
    Incremental,
    Verified,
    Legacy,
}

impl FrameHash {
    #[must_use]
    pub const fn default_for_build() -> Self {
        if cfg!(debug_assertions) {
            Self::Verified
        } else {
            Self::Incremental
        }
    }
}

pub struct Simulation<R: Rules> {
    rules: R,
    world: World,
    rng: Pcg32,
    seed: u64,
    frame: Frame,
    commands: CommandBus<R::Command>,
    recording: Replay<R::Command>,
    batch: Vec<Envelope<R::Command>>,
    poisoned: bool,
    frame_hash: FrameHash,
    hash: u64,
}

impl<R: Rules> Simulation<R> {
    pub fn new(seed: u64, mut rules: R) -> Result<Self> {
        let mut world = World::new();
        let mut rng = Pcg32::new(seed);
        {
            let mut tick = Tick::new(&mut world, &mut rng, Frame::ZERO);
            rules.setup(&mut tick)?;
        }
        let frame_hash = FrameHash::default_for_build();
        let initial_hash = Self::hash_of(&mut world, &rng, frame_hash, Frame::ZERO)?;
        Ok(Self {
            rules,
            world,
            rng,
            seed,
            frame: Frame::ZERO,
            commands: CommandBus::new(),
            recording: Replay::new(seed, initial_hash),
            batch: Vec::new(),
            poisoned: false,
            frame_hash,
            hash: initial_hash,
        })
    }

    pub fn set_frame_hash(&mut self, frame_hash: FrameHash) -> Result<()> {
        self.frame_hash = frame_hash;
        self.hash = Self::hash_of(&mut self.world, &self.rng, frame_hash, self.frame)?;
        if self.frame == Frame::ZERO {
            self.recording = Replay::new(self.seed, self.hash);
        }
        Ok(())
    }

    #[must_use]
    pub const fn frame_hash(&self) -> FrameHash {
        self.frame_hash
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn frame(&self) -> Frame {
        self.frame
    }

    #[must_use]
    pub const fn state(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub const fn rules(&self) -> &R {
        &self.rules
    }

    #[must_use]
    pub const fn state_hash(&self) -> u64 {
        self.hash
    }

    #[must_use]
    pub fn world_hash(&self) -> u64 {
        match self.frame_hash {
            FrameHash::Legacy => self.world.legacy_state_hash(),
            FrameHash::Incremental | FrameHash::Verified => self.world.state_hash(),
        }
    }

    #[must_use]
    pub fn rng_state(&self) -> (u64, u64) {
        self.rng.to_parts()
    }

    #[must_use]
    pub const fn recording(&self) -> &Replay<R::Command> {
        &self.recording
    }

    #[must_use]
    pub fn into_recording(self) -> Replay<R::Command> {
        self.recording
    }

    #[must_use]
    pub fn pending_commands(&self) -> usize {
        self.commands.len()
    }

    #[must_use]
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn submit(&mut self, player: PlayerId, command: R::Command) -> Result<CommandKey> {
        self.submit_at(self.frame, player, command)
    }

    pub fn submit_after(
        &mut self,
        delay: LogicFrames,
        player: PlayerId,
        command: R::Command,
    ) -> Result<CommandKey> {
        let target = self
            .frame
            .checked_add(delay)
            .ok_or(Error::TimelineExhausted)?;
        self.submit_at(target, player, command)
    }

    pub fn submit_at(
        &mut self,
        frame: Frame,
        player: PlayerId,
        command: R::Command,
    ) -> Result<CommandKey> {
        if frame < self.frame {
            return Err(Error::CommandTooLate {
                target: frame.get(),
                next: self.frame.get(),
            });
        }
        let sequence = self.commands.next_sequence(player)?;
        let key = CommandKey::new(frame, player, sequence);
        self.commands.submit(self.frame, key, command)?;
        Ok(key)
    }

    pub fn submit_with_key(&mut self, key: CommandKey, command: R::Command) -> Result<()> {
        self.commands.submit(self.frame, key, command)
    }

    pub fn tick(&mut self) -> Result<FrameReport> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let frame = self.frame;
        let Some(next) = frame.next() else {
            self.poisoned = true;
            return Err(Error::TimelineExhausted);
        };

        let mut batch = core::mem::take(&mut self.batch);
        batch.clear();
        batch.extend(self.commands.drain_due(frame));

        let outcome = {
            let rules = &self.rules;
            let mut tick = Tick::new(&mut self.world, &mut self.rng, frame);
            batch
                .iter()
                .try_for_each(|envelope| {
                    let key = envelope.key();
                    rules
                        .apply(&mut tick, key, envelope.command())
                        .map_err(|error| {
                            Error::in_rule(
                                format!(
                                    "command {}/player {}#{}",
                                    key.frame.get(),
                                    key.player.get(),
                                    key.sequence
                                ),
                                error,
                            )
                        })
                })
                .and_then(|()| rules.step(&mut tick))
        };
        if let Err(error) = outcome {
            self.batch = batch;
            self.poisoned = true;
            return Err(error);
        }

        let commands_applied = batch.len();
        let hash = match Self::hash_of(&mut self.world, &self.rng, self.frame_hash, frame) {
            Ok(hash) => hash,
            Err(error) => {
                self.batch = batch;
                self.poisoned = true;
                return Err(error);
            }
        };
        self.hash = hash;
        self.recording.record_frame(batch.drain(..), hash);
        self.batch = batch;
        self.frame = next;

        Ok(FrameReport {
            frame,
            hash,
            commands_applied,
        })
    }

    pub fn run(&mut self, duration: LogicFrames) -> Result<()> {
        let mut remaining = duration;
        while !remaining.is_zero() {
            self.tick()?;
            remaining = remaining.saturating_sub(LogicFrames::ONE);
        }
        Ok(())
    }

    fn hash_of(world: &mut World, rng: &Pcg32, frame_hash: FrameHash, frame: Frame) -> Result<u64> {
        let world_hash = match frame_hash {
            FrameHash::Incremental => world.state_hash_incremental(),
            FrameHash::Verified => {
                let incremental = world.state_hash_incremental();
                let full = world.state_hash();
                if incremental != full {
                    return Err(Error::StaleHashCache {
                        frame: frame.get(),
                        incremental,
                        full,
                    });
                }
                incremental
            }
            FrameHash::Legacy => world.legacy_state_hash(),
        };
        let mut hasher = StateHasher::new();
        hasher.write_u64(world_hash);
        let (state, increment) = rng.to_parts();
        hasher.write_u64(state);
        hasher.write_u64(increment);
        Ok(hasher.finish())
    }
}

impl<R: Rules> core::fmt::Debug for Simulation<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Simulation")
            .field("frame", &self.frame.get())
            .field("entities", &self.world.len())
            .field("pending_commands", &self.commands.len())
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
