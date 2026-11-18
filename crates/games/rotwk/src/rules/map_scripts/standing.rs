use std::collections::{BTreeMap, BTreeSet};

use mint_sim::{Entity, PlayerId, StateHasher, Tick};

use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PlayerStanding {
    attacked_by: BTreeMap<u8, BTreeSet<u8>>,
    object_counts: BTreeMap<(u8, String), i32>,
    hunting: BTreeSet<u8>,
    last_hit: BTreeMap<Entity, u8>,
}

impl PlayerStanding {
    pub(crate) fn note_hit(&mut self, victim: Entity, attacker: PlayerId) {
        self.last_hit.insert(victim, attacker.get());
    }

    pub(crate) fn note_attack(&mut self, victim: PlayerId, attacker: PlayerId) {
        self.attacked_by
            .entry(victim.get())
            .or_default()
            .insert(attacker.get());
    }

    pub(crate) fn last_hitter(&self, victim: Entity) -> Option<PlayerId> {
        self.last_hit.get(&victim).copied().map(PlayerId::new)
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.last_hit.retain(|&entity, _| is_live(entity));
    }

    pub(crate) fn was_attacked_by(&self, victim: PlayerId, attacker: PlayerId) -> bool {
        self.attacked_by
            .get(&victim.get())
            .is_some_and(|attackers| attackers.contains(&attacker.get()))
    }

    pub(crate) fn object_count(&self, player: PlayerId, types: &str) -> i32 {
        self.object_counts
            .get(&(player.get(), types.to_owned()))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn set_object_count(&mut self, player: PlayerId, types: &str, count: i32) {
        self.object_counts
            .insert((player.get(), types.to_owned()), count);
    }

    pub(crate) fn start_hunting(&mut self, player: PlayerId) {
        self.hunting.insert(player.get());
    }

    pub(crate) fn has_hunters(&self) -> bool {
        !self.hunting.is_empty()
    }

    pub(crate) fn is_hunting(&self, player: PlayerId) -> bool {
        self.hunting.contains(&player.get())
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.attacked_by.is_empty()
            && self.object_counts.is_empty()
            && self.hunting.is_empty()
            && self.last_hit.is_empty()
        {
            return;
        }
        hasher.write_usize(self.attacked_by.len());
        for (victim, attackers) in &self.attacked_by {
            hasher.write_u8(*victim);
            hasher.write_usize(attackers.len());
            for attacker in attackers {
                hasher.write_u8(*attacker);
            }
        }
        hasher.write_usize(self.object_counts.len());
        for ((player, types), count) in &self.object_counts {
            hasher.write_u8(*player);
            hasher.write_bytes(types.as_bytes());
            hasher.write_u8(0);
            hasher.write_i32(*count);
        }
        hasher.write_usize(self.hunting.len());
        for player in &self.hunting {
            hasher.write_u8(*player);
        }
        hasher.write_usize(self.last_hit.len());
        for (victim, player) in &self.last_hit {
            victim.hash_into(hasher);
            hasher.write_u8(*player);
        }
    }
}

impl RotwkRules {
    pub(in crate::rules) fn note_attacked_by(
        tick: &mut Tick<'_>,
        ids: Ids,
        victim: Entity,
        attacker: Entity,
    ) {
        let world = tick.state();
        let struck = Self::commanded_unit(world, ids, victim);
        let (Some(victim), Some(attacker)) = (
            world.get(ids.owner, victim).map(|o| o.0),
            world.get(ids.owner, attacker).map(|o| o.0),
        ) else {
            return;
        };
        if let Some((_, state)) = tick
            .state_mut()
            .store_mut(ids.map_script_state)
            .and_then(|store| store.iter_mut().next())
        {
            state.rt.standing.note_attack(victim, attacker);
            state.rt.standing.note_hit(struck, attacker);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_standing_hashes_nothing_and_attacks_are_directional() {
        let (a, b) = (PlayerId::new(1), PlayerId::new(2));
        let mut standing = PlayerStanding::default();
        let mut hasher = StateHasher::new();
        standing.hash_into(&mut hasher);
        assert_eq!(hasher.finish(), StateHasher::new().finish());
        standing.note_attack(a, b);
        assert!(standing.was_attacked_by(a, b));
        assert!(!standing.was_attacked_by(b, a));
        let mut hasher = StateHasher::new();
        standing.hash_into(&mut hasher);
        assert_ne!(hasher.finish(), StateHasher::new().finish());
    }
}
