use mint_ecs::StateHasher;

use crate::command::{Command, CommandKey, Envelope};
use crate::rules::Rules;
use crate::simulation::Simulation;
use crate::time::{Frame, LogicFrames};
use crate::{Error, Result};

pub const REPLAY_FORMAT_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replay<C> {
    seed: u64,
    initial_hash: u64,
    commands: Vec<Envelope<C>>,
    hashes: Vec<u64>,
}

impl<C> Replay<C> {
    #[must_use]
    pub const fn new(seed: u64, initial_hash: u64) -> Self {
        Self {
            seed,
            initial_hash,
            commands: Vec::new(),
            hashes: Vec::new(),
        }
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    #[must_use]
    pub const fn initial_hash(&self) -> u64 {
        self.initial_hash
    }

    #[must_use]
    pub const fn format_version(&self) -> u32 {
        REPLAY_FORMAT_VERSION
    }

    #[must_use]
    pub fn commands(&self) -> &[Envelope<C>] {
        &self.commands
    }

    #[must_use]
    pub fn hashes(&self) -> &[u64] {
        &self.hashes
    }

    #[must_use]
    pub const fn from_parts(
        seed: u64,
        initial_hash: u64,
        commands: Vec<Envelope<C>>,
        hashes: Vec<u64>,
    ) -> Self {
        Self {
            seed,
            initial_hash,
            commands,
            hashes,
        }
    }

    #[must_use]
    pub fn frames(&self) -> LogicFrames {
        LogicFrames::from_frames(u32::try_from(self.hashes.len()).unwrap_or(u32::MAX))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    pub(crate) fn record_frame<I>(&mut self, commands: I, hash: u64)
    where
        I: IntoIterator<Item = Envelope<C>>,
    {
        self.commands.extend(commands);
        self.hashes.push(hash);
    }
}

impl<C: Command> Replay<C> {
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut hasher = StateHasher::new();
        hasher.write_u32(REPLAY_FORMAT_VERSION);
        hasher.write_u64(self.seed);
        hasher.write_u64(self.initial_hash);
        hasher.write_usize(self.commands.len());
        for envelope in &self.commands {
            envelope.hash_into(&mut hasher);
        }
        hasher.write_usize(self.hashes.len());
        for hash in &self.hashes {
            hasher.write_u64(*hash);
        }
        hasher.finish()
    }

    pub fn verify<R>(&self, rules: R) -> Result<VerifiedReplay>
    where
        R: Rules<Command = C>,
    {
        let mut simulation = Simulation::new(self.seed, rules)?;
        if simulation.state_hash() != self.initial_hash {
            return Err(Error::SetupDivergence {
                expected: self.initial_hash,
                actual: simulation.state_hash(),
            });
        }

        let mut cursor = 0usize;
        for (index, expected) in self.hashes.iter().enumerate() {
            let frame = Frame::new(u64::try_from(index).unwrap_or(u64::MAX));
            while let Some(envelope) = self.commands.get(cursor) {
                if envelope.key().frame > frame {
                    break;
                }
                simulation.submit_with_key(envelope.key(), envelope.command().clone())?;
                cursor = cursor.saturating_add(1);
            }

            let report = simulation.tick()?;
            if report.hash != *expected {
                return Err(Error::Divergence {
                    frame: report.frame.get(),
                    expected: *expected,
                    actual: report.hash,
                });
            }
        }

        if let Some(envelope) = self.commands.get(cursor) {
            return Err(Error::UnconsumedCommands {
                remaining: self.commands.len().saturating_sub(cursor),
                first_frame: envelope.key().frame.get(),
            });
        }

        Ok(VerifiedReplay {
            frames: self.hashes.len(),
            commands: self.commands.len(),
            final_hash: simulation.state_hash(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedReplay {
    pub frames: usize,
    pub commands: usize,
    pub final_hash: u64,
}

impl core::fmt::Display for VerifiedReplay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} frames, {} commands, final hash {:#018x}",
            self.frames, self.commands, self.final_hash
        )
    }
}

#[must_use]
pub fn first_divergence(recorded: &[u64], observed: &[u64]) -> Option<usize> {
    recorded
        .iter()
        .zip(observed.iter())
        .position(|(a, b)| a != b)
        .or_else(|| {
            (recorded.len() != observed.len()).then_some(recorded.len().min(observed.len()))
        })
}

#[must_use]
pub fn commands_for_frame<C>(commands: &[Envelope<C>], frame: Frame) -> &[Envelope<C>] {
    let start = commands.partition_point(|envelope| envelope.key().frame < frame);
    let end = commands.partition_point(|envelope| envelope.key().frame <= frame);
    commands.get(start..end).unwrap_or(&[])
}

#[must_use]
pub fn is_ordered<C>(commands: &[Envelope<C>]) -> bool {
    commands
        .windows(2)
        .all(|pair| match (pair.first(), pair.get(1)) {
            (Some(a), Some(b)) => a.key() < b.key(),
            _ => true,
        })
}

#[must_use]
pub fn describe(key: CommandKey) -> String {
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::PlayerId;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Nop;

    impl Command for Nop {
        fn hash_into(&self, hasher: &mut StateHasher) {
            hasher.write_u8(0);
        }
    }

    fn envelope(frame: u64, player: u8, sequence: u32) -> Envelope<Nop> {
        Envelope::new(
            CommandKey::new(Frame::new(frame), PlayerId::new(player), sequence),
            Nop,
        )
    }

    #[test]
    fn first_divergence_names_the_frame_and_not_merely_that_there_is_one() {
        assert_eq!(first_divergence(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(first_divergence(&[1, 2, 3], &[1, 9, 3]), Some(1));
        assert_eq!(first_divergence(&[1, 2, 3], &[9, 2, 3]), Some(0));
        assert_eq!(first_divergence(&[1, 2, 3], &[1, 2]), Some(2));
        assert_eq!(first_divergence(&[1, 2], &[1, 2, 3]), Some(2));
    }

    #[test]
    fn a_frame_slice_covers_that_frame_only() {
        let log = vec![
            envelope(0, 0, 0),
            envelope(1, 0, 1),
            envelope(1, 2, 0),
            envelope(4, 0, 2),
        ];
        assert_eq!(commands_for_frame(&log, Frame::new(1)).len(), 2);
        assert_eq!(commands_for_frame(&log, Frame::new(0)).len(), 1);
        assert_eq!(commands_for_frame(&log, Frame::new(2)).len(), 0);
        assert_eq!(commands_for_frame(&log, Frame::new(9)).len(), 0);
    }

    #[test]
    fn a_log_out_of_order_or_with_a_repeated_key_is_detectable() {
        assert!(is_ordered(&[envelope(0, 0, 0), envelope(1, 0, 1)]));
        assert!(!is_ordered(&[envelope(1, 0, 1), envelope(0, 0, 0)]));
        assert!(!is_ordered(&[envelope(0, 0, 0), envelope(0, 0, 0)]));
        assert!(is_ordered::<Nop>(&[]));
    }

    #[test]
    fn the_digest_covers_the_seed_the_commands_and_the_hashes() {
        let mut base: Replay<Nop> = Replay::new(1, 100);
        base.record_frame([envelope(0, 0, 0)], 500);
        let reference = base.digest();

        let mut other_seed: Replay<Nop> = Replay::new(2, 100);
        other_seed.record_frame([envelope(0, 0, 0)], 500);
        assert_ne!(reference, other_seed.digest());

        let mut other_command: Replay<Nop> = Replay::new(1, 100);
        other_command.record_frame([envelope(0, 0, 1)], 500);
        assert_ne!(reference, other_command.digest());

        let mut other_hash: Replay<Nop> = Replay::new(1, 100);
        other_hash.record_frame([envelope(0, 0, 0)], 501);
        assert_ne!(reference, other_hash.digest());

        let mut same: Replay<Nop> = Replay::new(1, 100);
        same.record_frame([envelope(0, 0, 0)], 500);
        assert_eq!(reference, same.digest());
    }

    #[test]
    fn the_digest_of_a_fixed_recording_is_a_recorded_constant() {
        let mut replay: Replay<Nop> = Replay::new(0xabcd, 0x1234);
        replay.record_frame([envelope(0, 0, 0), envelope(0, 1, 0)], 0x5555);
        replay.record_frame([envelope(1, 0, 1)], 0x6666);
        assert_eq!(replay.format_version(), REPLAY_FORMAT_VERSION);
        assert_eq!(replay.digest(), 0x561e_648e_3e8f_f8ac);
    }

    #[test]
    fn an_empty_recording_reports_zero_frames() {
        let replay: Replay<Nop> = Replay::new(3, 9);
        assert!(replay.is_empty());
        assert_eq!(replay.frames(), LogicFrames::ZERO);
        assert_eq!(replay.seed(), 3);
        assert_eq!(replay.initial_hash(), 9);
    }

    #[test]
    fn describe_names_frame_player_and_sequence() {
        assert_eq!(
            describe(CommandKey::new(Frame::new(4), PlayerId::new(2), 9)),
            "frame 4/player 2#9"
        );
    }
}
