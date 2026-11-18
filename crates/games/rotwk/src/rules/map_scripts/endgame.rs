use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::counters::TICKS_PER_SCRIPT_FRAME;
use super::{
    parameter_string, ActionVerb, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::combat::DeathNaming;
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

impl RotwkRules {
    fn player_owns_objects(&self, tick: &Tick<'_>, ids: Ids, player: PlayerId) -> bool {
        let world = tick.state();
        let Some(store) = world.store(ids.owner) else {
            return false;
        };
        store.iter().any(|(entity, owner)| {
            owner.0 == player
                && world.is_live(entity)
                && !world.contains(ids.doomed, entity)
                && !world
                    .get(ids.health, entity)
                    .is_some_and(|health| health.0.is_dead())
                && world
                    .get(ids.kind, entity)
                    .and_then(|kind| self.kinds.get(*kind))
                    .is_some_and(|entry| {
                        !["INERT", "PROJECTILE", "MINE"]
                            .into_iter()
                            .any(|kind| entry.record.has_kind(kind))
                    })
        })
    }

    fn local_player_defeated_mid_match(&self, tick: &Tick<'_>, ids: Ids) -> bool {
        let Some(local) = self.script_local_player else {
            return false;
        };
        let world = tick.state();
        if !self.player_defeated(world, ids, local) {
            return false;
        }
        let winner = world
            .store(ids.outcome)
            .and_then(|store| store.iter().next().map(|(_, outcome)| outcome.winner));
        winner.is_none_or(|winner| self.alliances.allied(local, winner))
    }
}

struct ConditionAllDestroyed {
    player_text: String,
}

impl ConditionVerb for ConditionAllDestroyed {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        if tick.frame().get() / TICKS_PER_SCRIPT_FRAME < u64::from(rules.base_check_frames) {
            return false;
        }
        rules
            .player_mask(&state.rt, &self.player_text, player)
            .every(|p| !rules.player_owns_objects(tick, ids, p))
    }
}

struct ConditionPlayerDefeat;

impl ConditionVerb for ConditionPlayerDefeat {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        _: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        rules.local_player_defeated_mid_match(tick, ids)
    }
}

struct ActionPlayerKill {
    player_text: String,
}

impl ActionVerb for ActionPlayerKill {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        for target in mask.players() {
            diag!("script: PLAYER_KILL kills player {}", target.get());
            let doomed: Vec<Entity> = tick
                .state()
                .store(ids.owner)
                .map(|store| {
                    store
                        .iter()
                        .filter(|(entity, owner)| {
                            owner.0 == target
                                && tick.state().is_live(*entity)
                                && tick.state().contains(ids.kind, *entity)
                        })
                        .map(|(entity, _)| entity)
                        .collect()
                })
                .unwrap_or_default();
            for entity in doomed {
                if let Some(pair) = rules.kill(tick, ids, entity, None, DeathNaming::Normal)? {
                    kills.push(pair);
                }
            }
        }
        Ok(())
    }
}

pub(super) fn parse_player_all_destroyed(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionAllDestroyed {
        player_text: parameter_string(params, 0).unwrap_or_default(),
    }))
}

pub(super) fn parse_multiplayer_player_defeat(
    _: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionPlayerDefeat))
}

pub(super) fn parse_player_kill(
    params: &[Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionPlayerKill {
        player_text: parameter_string(params, 0)?,
    }))
}
