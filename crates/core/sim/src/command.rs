use mint_ecs::StateHasher;

use crate::time::Frame;
use crate::{Error, Result};

pub const MAX_PENDING_COMMANDS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct PlayerId(u8);

impl PlayerId {
    #[must_use]
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u8(self.0);
    }
}

impl core::fmt::Display for PlayerId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "player {}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommandKey {
    pub frame: Frame,
    pub player: PlayerId,
    pub sequence: u32,
}

impl CommandKey {
    #[must_use]
    pub const fn new(frame: Frame, player: PlayerId, sequence: u32) -> Self {
        Self {
            frame,
            player,
            sequence,
        }
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        self.frame.hash_into(hasher);
        self.player.hash_into(hasher);
        hasher.write_u32(self.sequence);
    }
}

impl core::fmt::Display for CommandKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}/{}#{}", self.frame, self.player, self.sequence)
    }
}

pub trait Command: Clone + core::fmt::Debug + 'static {
    fn hash_into(&self, hasher: &mut StateHasher);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope<C> {
    key: CommandKey,
    command: C,
}

impl<C> Envelope<C> {
    #[must_use]
    pub const fn new(key: CommandKey, command: C) -> Self {
        Self { key, command }
    }

    #[must_use]
    pub const fn key(&self) -> CommandKey {
        self.key
    }

    #[must_use]
    pub const fn command(&self) -> &C {
        &self.command
    }

    #[must_use]
    pub fn into_parts(self) -> (CommandKey, C) {
        (self.key, self.command)
    }
}

impl<C: Command> Envelope<C> {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        self.key.hash_into(hasher);
        self.command.hash_into(hasher);
    }
}

#[derive(Debug)]
pub struct CommandBus<C> {
    pending: Vec<Envelope<C>>,
    sequences: Vec<(PlayerId, Option<u32>)>,
}

impl<C> CommandBus<C> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
            sequences: Vec::new(),
        }
    }

    pub fn next_sequence(&mut self, player: PlayerId) -> Result<u32> {
        let exhausted = Error::SequenceExhausted {
            player: player.get(),
        };
        match self.sequences.binary_search_by_key(&player, |&(p, _)| p) {
            Ok(position) => {
                let Some(entry) = self.sequences.get_mut(position) else {
                    return Err(exhausted);
                };
                let issued = entry.1.ok_or(exhausted)?;
                entry.1 = issued.checked_add(1);
                Ok(issued)
            }
            Err(position) => {
                self.sequences.insert(position, (player, Some(1)));
                Ok(0)
            }
        }
    }

    fn observe_sequence(&mut self, player: PlayerId, sequence: u32) {
        let next = sequence.checked_add(1);
        match self.sequences.binary_search_by_key(&player, |&(p, _)| p) {
            Ok(position) => {
                if let Some(entry) = self.sequences.get_mut(position) {
                    entry.1 = match (entry.1, next) {
                        (Some(current), Some(candidate)) => Some(current.max(candidate)),
                        _ => None,
                    };
                }
            }
            Err(position) => self.sequences.insert(position, (player, next)),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    #[must_use]
    pub fn pending(&self) -> &[Envelope<C>] {
        &self.pending
    }

    pub fn submit(&mut self, next_frame: Frame, key: CommandKey, command: C) -> Result<()> {
        if key.frame < next_frame {
            return Err(Error::CommandTooLate {
                target: key.frame.get(),
                next: next_frame.get(),
            });
        }
        if self.pending.len() >= MAX_PENDING_COMMANDS {
            return Err(Error::CommandQueueFull {
                limit: MAX_PENDING_COMMANDS,
            });
        }

        let position = self.pending.partition_point(|envelope| envelope.key < key);
        if self
            .pending
            .get(position)
            .is_some_and(|envelope| envelope.key == key)
        {
            return Err(Error::DuplicateCommand {
                frame: key.frame.get(),
                player: key.player.get(),
                sequence: key.sequence,
            });
        }

        self.pending.insert(position, Envelope::new(key, command));
        self.observe_sequence(key.player, key.sequence);
        Ok(())
    }

    pub fn drain_due(&mut self, frame: Frame) -> impl Iterator<Item = Envelope<C>> + '_ {
        let end = self
            .pending
            .partition_point(|envelope| envelope.key.frame <= frame);
        self.pending.drain(..end)
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

impl<C> Default for CommandBus<C> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Say(u32);

    impl Command for Say {
        fn hash_into(&self, hasher: &mut StateHasher) {
            hasher.write_u32(self.0);
        }
    }

    fn key(frame: u64, player: u8, sequence: u32) -> CommandKey {
        CommandKey::new(Frame::new(frame), PlayerId::new(player), sequence)
    }

    fn drained(bus: &mut CommandBus<Say>, frame: u64) -> Vec<CommandKey> {
        bus.drain_due(Frame::new(frame))
            .map(|envelope| envelope.key())
            .collect()
    }

    #[test]
    fn ordering_is_frame_then_player_then_sequence() {
        assert!(key(1, 9, 99) < key(2, 0, 0));
        assert!(key(1, 0, 99) < key(1, 1, 0));
        assert!(key(1, 1, 0) < key(1, 1, 1));

        let mut keys = vec![key(2, 0, 0), key(1, 1, 0), key(1, 0, 5), key(1, 0, 1)];
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![key(1, 0, 1), key(1, 0, 5), key(1, 1, 0), key(2, 0, 0)]
        );
    }

    #[test]
    fn arrival_order_does_not_survive_submission() {
        let arrivals = [key(3, 0, 0), key(1, 2, 7), key(1, 0, 0), key(2, 1, 1)];
        let mut forward = CommandBus::new();
        for (index, k) in arrivals.iter().enumerate() {
            let value = u32::try_from(index).unwrap_or(0);
            forward.submit(Frame::ZERO, *k, Say(value)).unwrap();
        }
        let mut backward = CommandBus::new();
        for (index, k) in arrivals.iter().enumerate().rev() {
            let value = u32::try_from(index).unwrap_or(0);
            backward.submit(Frame::ZERO, *k, Say(value)).unwrap();
        }

        let expected: Vec<CommandKey> = {
            let mut sorted = arrivals.to_vec();
            sorted.sort_unstable();
            sorted
        };
        let forward_keys: Vec<CommandKey> = forward.pending().iter().map(Envelope::key).collect();
        let backward_keys: Vec<CommandKey> = backward.pending().iter().map(Envelope::key).collect();
        assert_eq!(forward_keys, expected);
        assert_eq!(backward_keys, expected);
    }

    #[test]
    fn draining_a_frame_takes_that_frame_only_and_leaves_the_rest_ordered() {
        let mut bus = CommandBus::new();
        for k in [key(0, 0, 0), key(1, 0, 0), key(0, 1, 0), key(2, 0, 0)] {
            bus.submit(Frame::ZERO, k, Say(0)).unwrap();
        }
        assert_eq!(bus.len(), 4);
        assert_eq!(drained(&mut bus, 0), vec![key(0, 0, 0), key(0, 1, 0)]);
        assert_eq!(bus.len(), 2);
        assert_eq!(drained(&mut bus, 1), vec![key(1, 0, 0)]);
        assert_eq!(drained(&mut bus, 2), vec![key(2, 0, 0)]);
        assert!(bus.is_empty());
    }

    #[test]
    fn a_command_for_an_executed_frame_is_refused_rather_than_dropped() {
        let mut bus: CommandBus<Say> = CommandBus::new();
        assert_eq!(
            bus.submit(Frame::new(10), key(9, 0, 0), Say(0)),
            Err(Error::CommandTooLate {
                target: 9,
                next: 10
            })
        );
        assert!(bus.is_empty());
        assert!(bus.submit(Frame::new(10), key(10, 0, 0), Say(0)).is_ok());
    }

    #[test]
    fn a_duplicate_key_is_refused_because_the_tie_would_be_broken_by_arrival() {
        let mut bus = CommandBus::new();
        bus.submit(Frame::ZERO, key(1, 0, 0), Say(1)).unwrap();
        assert_eq!(
            bus.submit(Frame::ZERO, key(1, 0, 0), Say(2)),
            Err(Error::DuplicateCommand {
                frame: 1,
                player: 0,
                sequence: 0,
            })
        );
        assert_eq!(bus.len(), 1);
        assert_eq!(bus.pending().first().map(Envelope::command), Some(&Say(1)));
    }

    #[test]
    fn sequence_numbers_are_per_player_and_start_at_zero() {
        let mut bus: CommandBus<Say> = CommandBus::new();
        let a = PlayerId::new(0);
        let b = PlayerId::new(7);
        assert_eq!(bus.next_sequence(a), Ok(0));
        assert_eq!(bus.next_sequence(a), Ok(1));
        assert_eq!(bus.next_sequence(b), Ok(0));
        assert_eq!(bus.next_sequence(a), Ok(2));
        assert_eq!(bus.next_sequence(b), Ok(1));
    }

    #[test]
    fn an_externally_supplied_sequence_is_not_reissued() {
        let mut bus = CommandBus::new();
        let player = PlayerId::new(3);
        bus.submit(Frame::ZERO, key(1, 3, 40), Say(0)).unwrap();
        assert_eq!(bus.next_sequence(player), Ok(41));
        bus.submit(Frame::ZERO, key(1, 3, 5), Say(0)).unwrap();
        assert_eq!(bus.next_sequence(player), Ok(42));
    }

    #[test]
    fn the_queue_is_bounded_because_a_peer_fills_it() {
        let mut bus = CommandBus::new();
        for index in 0..MAX_PENDING_COMMANDS {
            let sequence = u32::try_from(index).unwrap_or(u32::MAX);
            bus.submit(Frame::ZERO, key(1, 0, sequence), Say(0))
                .unwrap();
        }
        assert_eq!(bus.len(), MAX_PENDING_COMMANDS);
        assert_eq!(
            bus.submit(Frame::ZERO, key(1, 0, u32::MAX), Say(0)),
            Err(Error::CommandQueueFull {
                limit: MAX_PENDING_COMMANDS
            })
        );
    }

    #[test]
    fn a_sequence_counter_reports_exhaustion_rather_than_wrapping() {
        let mut bus: CommandBus<Say> = CommandBus::new();
        let player = PlayerId::new(1);
        bus.submit(Frame::ZERO, key(1, 1, u32::MAX.saturating_sub(1)), Say(0))
            .unwrap();
        assert_eq!(bus.next_sequence(player), Ok(u32::MAX));
        assert_eq!(
            bus.next_sequence(player),
            Err(Error::SequenceExhausted { player: 1 })
        );
        assert_eq!(
            bus.next_sequence(player),
            Err(Error::SequenceExhausted { player: 1 }),
            "exhaustion is permanent, not a one-off"
        );
    }

    #[test]
    fn clearing_drops_pending_commands_but_not_issued_sequence_numbers() {
        let mut bus = CommandBus::new();
        let player = PlayerId::new(2);
        let sequence = bus.next_sequence(player).unwrap();
        bus.submit(Frame::ZERO, key(0, 2, sequence), Say(0))
            .unwrap();
        bus.clear();
        assert!(bus.is_empty());
        assert_eq!(bus.next_sequence(player), Ok(1));
    }

    #[test]
    fn a_command_left_by_an_unticked_frame_is_swept_rather_than_stranded() {
        let mut bus = CommandBus::new();
        bus.submit(Frame::ZERO, key(1, 0, 0), Say(0)).unwrap();
        bus.submit(Frame::ZERO, key(4, 0, 1), Say(0)).unwrap();
        assert_eq!(drained(&mut bus, 4), vec![key(1, 0, 0), key(4, 0, 1)]);
        assert!(bus.is_empty());
    }

    #[test]
    fn commands_hash_with_their_keys() {
        let mut same = StateHasher::new();
        Envelope::new(key(1, 0, 0), Say(5)).hash_into(&mut same);
        let mut other_command = StateHasher::new();
        Envelope::new(key(1, 0, 0), Say(6)).hash_into(&mut other_command);
        let mut other_key = StateHasher::new();
        Envelope::new(key(1, 0, 1), Say(5)).hash_into(&mut other_key);
        assert_ne!(same.finish(), other_command.finish());
        assert_ne!(same.finish(), other_key.finish());
    }
}
