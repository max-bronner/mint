use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, StateHasher, Tick, UpgradeBit};

use super::{parameter_string, ConditionVerb, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

const MAX_RECORDS: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CompletedUpgrades(Vec<(PlayerId, UpgradeBit, Entity)>);

impl CompletedUpgrades {
    fn push(&mut self, player: PlayerId, upgrade: UpgradeBit, source: Entity) {
        if self.0.len() >= MAX_RECORDS {
            self.0.remove(0);
        }
        self.0.push((player, upgrade, source));
    }

    fn find(&self, player: PlayerId, upgrade: UpgradeBit) -> Option<usize> {
        self.0
            .iter()
            .position(|(who, bit, _)| *who == player && *bit == upgrade)
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for (player, upgrade, source) in &self.0 {
            hasher.write_u8(player.get());
            hasher.write_u32(upgrade.index());
            source.hash_into(hasher);
        }
    }
}

pub(crate) fn note_upgrade_completed(
    rules: &RotwkRules,
    tick: &mut Tick<'_>,
    ids: Ids,
    player: PlayerId,
    upgrade: UpgradeBit,
    source: Entity,
) {
    if !rules
        .map_scripts
        .as_ref()
        .is_some_and(|engine| engine.reads_completed_upgrades())
    {
        return;
    }
    let Some(anchor) = tick
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(entity, _)| entity))
    else {
        return;
    };
    if let Some(state) = tick.state_mut().get_mut(ids.map_script_state, anchor) {
        state.rt.completed_upgrades.push(player, upgrade, source);
    }
}

struct ConditionBuiltUpgrade {
    player_text: String,
    upgrade: String,
}

impl ConditionVerb for ConditionBuiltUpgrade {
    fn eval(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let Some(bit) = rules.upgrade_bits.bit(&self.upgrade) else {
            return false;
        };
        rules
            .player_mask(&state.rt, &self.player_text, player)
            .any(|who| state.rt.completed_upgrades.find(who, bit).is_some())
    }

    fn eval_mut(
        &self,
        rules: &RotwkRules,
        _: &Tick<'_>,
        _: Ids,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let Some(bit) = rules.upgrade_bits.bit(&self.upgrade) else {
            return false;
        };
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for who in mask.players() {
            if let Some(index) = state.rt.completed_upgrades.find(who, bit) {
                state.rt.completed_upgrades.0.remove(index);
                return true;
            }
        }
        false
    }

    fn reads_completed_upgrades(&self) -> bool {
        true
    }
}

pub(super) fn parse_player_built_upgrade(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionBuiltUpgrade {
        player_text: parameter_string(params, 0).unwrap_or_default(),
        upgrade: parameter_string(params, 1)?,
    }))
}
