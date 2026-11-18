use mint_sim::{Entity, PlayerId, Result, Tick};

use super::team::TeamId;
use super::team_build::QueuedTeam;
use super::{MapScriptEngine, MapScriptState};
use crate::diag::diag;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::rules::{Ids, RotwkRules};

const READY_WAIT_SECONDS: u64 = 60;

impl RotwkRules {
    pub(super) fn check_ready_teams(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
    ) {
        let frame = tick.frame().get() / super::counters::TICKS_PER_SCRIPT_FRAME;
        let Some(ready) = state
            .rt
            .builders
            .get(player)
            .map(|builder| builder.ready_queue.clone())
        else {
            return;
        };
        let mut waiting = Vec::new();
        for mut entry in ready {
            let Some(proto) = state
                .rt
                .teams
                .get(entry.team)
                .map(|instance| instance.proto)
            else {
                continue;
            };
            let members = self.live_ai_members(tick, ids, state, entry.team);
            let any_idle = members
                .iter()
                .any(|&member| self.sequence_member_idle(tick, ids, state, member));
            let mut all_idle = members
                .iter()
                .all(|&member| self.sequence_member_idle(tick, ids, state, member));
            let executes = engine
                .team_protos
                .get(proto)
                .is_some_and(|entry| entry.template.executes_actions_on_create);
            if any_idle && executes && self.production_has_actions(engine, state, proto) {
                all_idle = true;
            }
            let wait = READY_WAIT_SECONDS.saturating_mul(u64::from(RETAIL_LOGIC_HZ));
            if entry.started.saturating_add(wait) < frame {
                all_idle = true;
            }
            if !all_idle {
                waiting.push(entry);
                continue;
            }
            entry.sent_to_start = true;
            release_hold(state, entry.team);
            if let Some(instance) = state.rt.teams.get_mut(entry.team) {
                instance.set_active();
            }
            diag!(
                "script: team-build player {} releases team {}",
                player.get(),
                entry.team.get()
            );
        }
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.ready_queue = waiting;
        }
    }

    pub(super) fn check_queued_teams(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        player: PlayerId,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let frame = tick.frame().get() / super::counters::TICKS_PER_SCRIPT_FRAME;
        let Some(queue) = state
            .rt
            .builders
            .get(player)
            .map(|builder| builder.build.clone())
        else {
            return Ok(());
        };
        let mut remaining = Vec::new();
        for entry in queue {
            let Some(proto) = state
                .rt
                .teams
                .get(entry.team)
                .map(|instance| instance.proto)
            else {
                continue;
            };
            let idle_seconds = engine
                .team_protos
                .get(proto)
                .map_or(0, |entry| entry.template.initial_idle_seconds);
            if !entry.build_time_expired(idle_seconds, frame) {
                remaining.push(entry);
            } else if !entry.minimum_built() {
                self.disband_queued(engine, state, &entry);
                diag!(
                    "script: team-build player {} disbands team {} - minimum not built",
                    player.get(),
                    entry.team.get()
                );
            } else if entry.builds_complete() {
                self.promote(state, player, entry);
            } else {
                remaining.push(entry);
            }
        }
        let mut waiting = Vec::new();
        for entry in remaining {
            if entry.is_all_built() {
                self.promote(state, player, entry);
                continue;
            }
            let team = entry.team;
            let proto = state.rt.teams.get(team).map(|instance| instance.proto);
            waiting.push(entry);
            let Some(proto) = proto else {
                continue;
            };
            let executes = engine
                .team_protos
                .get(proto)
                .is_some_and(|entry| entry.template.executes_actions_on_create);
            let members = self.live_ai_members(tick, ids, state, team);
            let any_idle = members
                .iter()
                .any(|&member| self.sequence_member_idle(tick, ids, state, member));
            if any_idle && executes {
                self.run_production_actions(
                    tick,
                    ids,
                    engine,
                    state,
                    proto,
                    Some(team),
                    player,
                    kills,
                )?;
            }
        }
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.build = waiting;
        }
        Ok(())
    }

    fn promote(&self, state: &mut MapScriptState, player: PlayerId, entry: QueuedTeam) {
        release_hold(state, entry.team);
        if let Some(builder) = state.rt.builders.get_mut(player) {
            builder.ready_queue.insert(0, entry);
        }
    }

    fn disband_queued(
        &self,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        entry: &QueuedTeam,
    ) {
        let default = self.default_team_of(engine, state, entry.team);
        state.rt.teams.disband(entry.team, default);
        if let Some(instance) = state.rt.teams.get_mut(entry.team) {
            instance.set_active();
        }
    }

    fn live_ai_members(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> Vec<Entity> {
        let world = tick.state();
        state.rt.teams.get(team).map_or_else(Vec::new, |instance| {
            instance
                .members()
                .iter()
                .copied()
                .filter(|&member| {
                    world.is_live(member)
                        && !world.contains(ids.doomed, member)
                        && self.has_ai(world, ids, member)
                })
                .collect()
        })
    }

    fn production_has_actions(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        proto: usize,
    ) -> bool {
        let index = state.rt.builders.proto(proto).script.or_else(|| {
            let entry = engine.team_protos.get(proto)?;
            let name = entry.template.production_condition.as_deref()?;
            engine.find_script(name, &entry.owner)
        });
        index
            .and_then(|index| engine.scripts.get(index))
            .is_some_and(|def| !def.actions.is_empty())
    }

    pub(super) fn apply_slot_bonuses(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> Result<()> {
        let members = state
            .rt
            .teams
            .get(team)
            .map_or_else(Vec::new, |instance| instance.members().to_vec());
        for member in members {
            let Some(bonus) = state.rt.builders.bonus(member) else {
                continue;
            };
            for upgrade in &bonus.upgrades {
                self.grant_script_upgrade(tick, ids, member, upgrade)?;
            }
            let receiver = self.sink_of(tick.state(), ids, member);
            let Some((index, _)) = self.experience_of(tick.state(), ids, receiver) else {
                continue;
            };
            let current = i32::try_from(index).unwrap_or(i32::MAX).saturating_add(1);
            if let Ok(delta) = u32::try_from(bonus.level.saturating_sub(current)) {
                if delta > 0 {
                    self.gain_levels(tick, ids, member, delta, None)?;
                }
            }
        }
        Ok(())
    }
}

fn release_hold(state: &mut MapScriptState, team: TeamId) {
    if let Some(instance) = state.rt.teams.get_mut(team) {
        instance.wait_for_build = false;
        instance.hold_create = false;
    }
}
