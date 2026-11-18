use mint_sim::{Entity, Result, Tick};

use super::counters::TICKS_PER_SCRIPT_FRAME;
use super::team::TeamId;
use super::{MapScriptEngine, MapScriptState};
use crate::diag::diag;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::rules::{Ids, RotwkRules};

impl RotwkRules {
    pub(super) fn step_generic_scripts(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let frame = tick.frame().get() / TICKS_PER_SCRIPT_FRAME;
        let teams: Vec<(TeamId, usize)> = state
            .rt
            .teams
            .instances()
            .filter(|team| team.activated)
            .map(|team| (team.id, team.proto))
            .collect();
        for (team, proto) in teams {
            let hooks = engine.generic_hooks(proto);
            if hooks.is_empty() {
                continue;
            }
            let active = state.rt.teams.generic_snapshot(proto, || {
                hooks
                    .iter()
                    .enumerate()
                    .filter(|(_, hook)| {
                        hook.is_some_and(|index| state.active.get(index).copied().unwrap_or(false))
                    })
                    .fold(0u32, |mask, (slot, _)| mask | slot_bit(slot))
            });
            for (slot, hook) in hooks.iter().enumerate() {
                self.try_generic_slot(
                    tick,
                    ids,
                    engine,
                    state,
                    (team, slot, *hook),
                    active & slot_bit(slot) != 0,
                    frame,
                    kills,
                )?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn try_generic_slot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        (team, slot, hook): (TeamId, usize, Option<usize>),
        active: bool,
        frame: u64,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(instance) = state.rt.teams.get(team) else {
            return Ok(());
        };
        if !instance.generic_tried(slot) {
            return Ok(());
        }
        let def = hook.and_then(|index| engine.scripts.get(index).map(|def| (index, def)));
        let Some((index, def)) = def.filter(|_| active) else {
            state.rt.teams.stop_generic(team, slot);
            return Ok(());
        };
        if !def.supported {
            return Ok(());
        }
        let delay = u64::try_from(def.delay_eval_seconds).unwrap_or(0);
        if delay > 0 && instance.generic_next(slot) > frame {
            return Ok(());
        }
        let player = self.team_owner(engine, state, team);
        let saved = (state.rt.calling_team, state.rt.condition_team);
        state.rt.calling_team = Some(team);
        state.rt.condition_team = None;
        let mut outcome = Ok(());
        if self.conditions_hold(tick, ids, state, def, player) {
            if def.one_shot {
                state.rt.teams.stop_generic(team, slot);
            }
            if def.sequence.run_sequentially || state.rt.seq.is_flagged(index) {
                if def.one_shot {
                    state
                        .rt
                        .seq
                        .append(Some(team), None, index, def.sequence.loop_count);
                    diag!(
                        "script: generic {:?} queued sequentially on team {}",
                        def.name,
                        team.get()
                    );
                }
            } else {
                diag!("script: generic {:?} runs on team {}", def.name, team.get());
                for action in def.actions.clone() {
                    outcome = action.run(self, tick, ids, engine, state, player, 0, kills);
                    if outcome.is_err() {
                        break;
                    }
                }
            }
        }
        (state.rt.calling_team, state.rt.condition_team) = saved;
        outcome?;
        if delay > 0 {
            let next = frame.saturating_add(delay.saturating_mul(u64::from(RETAIL_LOGIC_HZ)));
            state.rt.teams.set_generic_next(team, slot, next);
        }
        Ok(())
    }
}

pub(super) fn slot_bit(slot: usize) -> u32 {
    u32::try_from(slot)
        .ok()
        .and_then(|slot| 1u32.checked_shl(slot))
        .unwrap_or(0)
}
