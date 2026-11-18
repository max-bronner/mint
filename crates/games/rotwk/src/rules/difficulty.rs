use mint_sim::{Entity, PlayerId, Result, Tick};

use super::{Ids, RotwkRules};
use crate::lobby::Difficulty;

const MULTIPLAYER_UPGRADES: [(Difficulty, &str); 4] = [
    (Difficulty::Easy, "Upgrade_EasyAIMultiPlayer"),
    (Difficulty::Medium, "Upgrade_MediumAIMultiPlayer"),
    (Difficulty::Hard, "Upgrade_HardAIMultiPlayer"),
    (Difficulty::Brutal, "Upgrade_BrutalAIMultiPlayer"),
];

const SINGLE_PLAYER_UPGRADES: [(Difficulty, &str); 4] = [
    (Difficulty::Easy, "Upgrade_EasyAISinglePlayer"),
    (Difficulty::Medium, "Upgrade_MediumAISinglePlayer"),
    (Difficulty::Hard, "Upgrade_HardAISinglePlayer"),
    (Difficulty::Brutal, "Upgrade_BrutalAISinglePlayer"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DifficultyPlayer {
    pub(crate) player: PlayerId,
    pub(crate) human: bool,
    pub(crate) playable: bool,
    pub(crate) ai: Option<Difficulty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DifficultySetup {
    pub(crate) multiplayer: bool,
    pub(crate) game: Difficulty,
    pub(crate) players: Vec<DifficultyPlayer>,
}

impl Default for DifficultySetup {
    fn default() -> Self {
        Self {
            multiplayer: true,
            game: Difficulty::Medium,
            players: Vec::new(),
        }
    }
}

fn upgrade_name(table: &[(Difficulty, &'static str); 4], difficulty: Difficulty) -> &'static str {
    table
        .iter()
        .find(|(tier, _)| *tier == difficulty)
        .map_or("", |(_, name)| name)
}

impl RotwkRules {
    pub(super) fn difficulty_upgrade_name(&self, player: PlayerId) -> Option<&'static str> {
        let entry = self
            .difficulty
            .players
            .iter()
            .find(|p| p.player == player)?;
        if entry.human || !entry.playable {
            return None;
        }
        Some(match (self.difficulty.multiplayer, entry.ai) {
            (true, Some(own)) => upgrade_name(&MULTIPLAYER_UPGRADES, own),
            _ => upgrade_name(&SINGLE_PLAYER_UPGRADES, self.difficulty.game),
        })
    }

    pub(super) fn easy_computer(&self, player: PlayerId) -> bool {
        let Some(entry) = self.difficulty.players.iter().find(|p| p.player == player) else {
            return false;
        };
        if entry.human || !entry.playable {
            return false;
        }
        let tier = match (self.difficulty.multiplayer, entry.ai) {
            (true, Some(own)) => own,
            _ => self.difficulty.game,
        };
        tier == Difficulty::Easy
    }

    pub(super) fn grant_difficulty_upgrade(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let Some(owner) = tick.state().get(ids.owner, entity).map(|o| o.0) else {
            return Ok(());
        };
        let Some(name) = self.difficulty_upgrade_name(owner) else {
            return Ok(());
        };
        let Some(bit) = self.upgrade_bits.bit(name) else {
            return Ok(());
        };
        let held = tick
            .state()
            .get(ids.object_upgrades, entity)
            .is_some_and(|held| held.0.contains(bit));
        if held {
            return Ok(());
        }
        crate::diag::diag!(
            "upgrade: difficulty {name} to entity {} of player {}",
            entity.index(),
            owner.get()
        );
        self.grant_object_upgrade(tick, ids, entity, bit)
    }
}
