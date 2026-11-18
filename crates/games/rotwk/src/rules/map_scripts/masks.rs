use mint_sim::PlayerId;

use super::relations::Relation;
use super::runtime::ScriptRuntime;
use crate::rules::RotwkRules;

pub(crate) const MASK_PLAYERS: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PlayerMask(u32);

impl PlayerMask {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) const ALL: Self = Self(u32::MAX);

    pub(crate) fn of(player: PlayerId) -> Self {
        if player.get() < MASK_PLAYERS {
            Self(1_u32 << player.get())
        } else {
            Self::EMPTY
        }
    }

    pub(crate) fn with(self, player: PlayerId) -> Self {
        Self(self.0 | Self::of(player).0)
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(crate) fn contains(self, player: PlayerId) -> bool {
        self.0 & Self::of(player).0 != 0
    }

    pub(crate) fn players(self) -> impl Iterator<Item = PlayerId> {
        (0..MASK_PLAYERS)
            .map(PlayerId::new)
            .filter(move |&p| self.contains(p))
    }

    pub(crate) fn single(self) -> Option<PlayerId> {
        let mut players = self.players();
        let first = players.next()?;
        players.next().is_none().then_some(first)
    }

    pub(crate) fn first(self) -> Option<PlayerId> {
        self.players().next()
    }

    pub(crate) fn any(self, mut test: impl FnMut(PlayerId) -> bool) -> bool {
        self.players().any(&mut test)
    }

    pub(crate) fn every(self, mut test: impl FnMut(PlayerId) -> bool) -> bool {
        self.players().all(&mut test)
    }

    pub(crate) fn sum(self, count: impl FnMut(PlayerId) -> i64) -> i64 {
        self.players().map(count).sum()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Standing {
    Enemies,
    Allies,
    AlliesAndSelf,
}

impl RotwkRules {
    fn mask_universe(&self) -> impl Iterator<Item = PlayerId> {
        let total = usize::from(self.players).saturating_add(self.map_players.len());
        let total = u8::try_from(total.min(usize::from(MASK_PLAYERS))).unwrap_or(MASK_PLAYERS);
        (0..total).map(PlayerId::new)
    }

    pub(crate) fn human_players(&self) -> impl Iterator<Item = PlayerId> + '_ {
        self.mask_universe().filter(|&p| self.is_human(p))
    }

    pub(crate) fn is_human(&self, player: PlayerId) -> bool {
        self.script_local_player == Some(player)
            || (player.get() < self.players
                && self.command_source(player) == mint_sage_combat::CommandSource::Player)
    }

    fn replay_observer(&self) -> Option<PlayerId> {
        let total = self.mask_universe().count();
        u8::try_from(total)
            .ok()
            .filter(|&index| index < MASK_PLAYERS)
            .map(PlayerId::new)
    }

    pub(crate) fn script_relation(
        &self,
        runtime: &ScriptRuntime,
        from: PlayerId,
        to: PlayerId,
    ) -> Relation {
        runtime
            .relations
            .player_to_player(from, to, self.alliances.stance(from, to).into())
    }

    fn players_standing(
        &self,
        runtime: &ScriptRuntime,
        base: Option<PlayerId>,
        standing: Standing,
    ) -> PlayerMask {
        let Some(base) = base else {
            return PlayerMask::EMPTY;
        };
        self.mask_universe()
            .filter(|&p| {
                let relation = self.script_relation(runtime, base, p);
                match standing {
                    Standing::Enemies => p != base && relation == Relation::Enemies,
                    Standing::Allies => p != base && relation == Relation::Allies,
                    Standing::AlliesAndSelf => p == base || relation == Relation::Allies,
                }
            })
            .fold(PlayerMask::EMPTY, PlayerMask::with)
    }

    pub(crate) fn player_mask(
        &self,
        runtime: &ScriptRuntime,
        text: &str,
        current: Option<PlayerId>,
    ) -> PlayerMask {
        let local = self.script_local_player;
        let single = |player: Option<PlayerId>| player.map_or(PlayerMask::EMPTY, PlayerMask::of);
        match text {
            "<This Player's Enemies>" => self.players_standing(runtime, current, Standing::Enemies),
            "<This Player's Allies incl Self>" => {
                self.players_standing(runtime, current, Standing::AlliesAndSelf)
            }
            "<This Player's Allies>" => self.players_standing(runtime, current, Standing::Allies),
            "<This Player>" => single(current),
            "<Local Player>" => single(local),
            "<Local Player's Enemies>" => self.players_standing(runtime, local, Standing::Enemies),
            "<Local Player's Allies incl Self>" => {
                self.players_standing(runtime, local, Standing::AlliesAndSelf)
            }
            "<Local Player's Allies>" => self.players_standing(runtime, local, Standing::Allies),
            "<All Players>" => self
                .mask_universe()
                .chain(self.replay_observer())
                .fold(PlayerMask::EMPTY, PlayerMask::with),
            name => single(self.resolve_side_player(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(ids: &[u8]) -> PlayerMask {
        ids.iter()
            .fold(PlayerMask::EMPTY, |m, &i| m.with(PlayerId::new(i)))
    }

    #[test]
    fn a_mask_iterates_in_index_order_and_single_demands_exactly_one() {
        let m = mask(&[7, 2, 19]);
        let order: Vec<u8> = m.players().map(PlayerId::get).collect();
        assert_eq!(order, [2, 7, 19]);
        assert_eq!(m.single(), None);
        assert_eq!(m.first(), Some(PlayerId::new(2)));
        assert_eq!(mask(&[5]).single(), Some(PlayerId::new(5)));
        assert_eq!(PlayerMask::EMPTY.single(), None);
        assert_eq!(PlayerMask::of(PlayerId::new(20)), PlayerMask::EMPTY);
    }

    #[test]
    fn the_empty_mask_is_false_for_any_true_for_every_and_zero_for_a_sum() {
        let empty = PlayerMask::EMPTY;
        assert!(!empty.any(|_| true));
        assert!(empty.every(|_| false));
        assert_eq!(empty.sum(|_| 9), 0);
        assert_eq!(mask(&[1, 3]).sum(|p| i64::from(p.get())), 4);
    }
}
