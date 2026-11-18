use std::collections::BTreeSet;
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::team::TeamId;
use super::{
    parameter_int, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
    ScriptDef,
};
use crate::diag::diag;
use crate::game_data::RETAIL_LOGIC_HZ;
use crate::rules::guard::GuardState;
use crate::rules::{Ids, RotwkRules};

use super::orders::ScriptOrder;

const MAX_REVISITS: u32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SeqEntry {
    id: u32,
    team: Option<TeamId>,
    object: Option<Entity>,
    script: usize,
    instruction: i32,
    times_to_loop: i32,
    frames_to_wait: i32,
    blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Sequential {
    chains: Vec<Vec<SeqEntry>>,
    flagged: BTreeSet<usize>,
    next_id: u32,
}

impl Sequential {
    pub(super) fn is_flagged(&self, script: usize) -> bool {
        self.flagged.contains(&script)
    }

    pub(super) fn append(
        &mut self,
        team: Option<TeamId>,
        object: Option<Entity>,
        script: usize,
        times_to_loop: i32,
    ) {
        self.flagged.insert(script);
        let entry = SeqEntry {
            id: self.next_id,
            team,
            object,
            script,
            instruction: -1,
            times_to_loop,
            frames_to_wait: -1,
            blocked: false,
        };
        self.next_id = self.next_id.wrapping_add(1);
        let chain = self.chains.iter_mut().find(|chain| {
            chain.first().is_some_and(|head| {
                (object.is_some() && head.object == object) || (team.is_some() && head.team == team)
            })
        });
        match chain {
            Some(chain) => chain.push(entry),
            None => self.chains.push(vec![entry]),
        }
    }

    pub(super) fn remove_team(&mut self, team: TeamId) {
        self.chains
            .retain(|chain| chain.first().is_none_or(|head| head.team != Some(team)));
    }

    pub(super) fn remove_object(&mut self, object: Entity) {
        self.chains
            .retain(|chain| chain.first().is_none_or(|head| head.object != Some(object)));
    }

    pub(super) fn set_timer(&mut self, team: Option<TeamId>, object: Option<Entity>, frames: i32) {
        let head = self
            .chains
            .iter_mut()
            .filter_map(|c| c.first_mut())
            .find(|h| {
                (object.is_some() && h.object == object) || (team.is_some() && h.team == team)
            });
        if let Some(head) = head {
            head.frames_to_wait = frames;
        }
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.chains.is_empty() && self.flagged.is_empty() {
            return;
        }
        hasher.write_usize(self.chains.len());
        for chain in &self.chains {
            hasher.write_usize(chain.len());
            for entry in chain {
                hasher.write_u32(entry.id);
                hasher.write_u32(entry.team.map_or(0, TeamId::get));
                match entry.object {
                    Some(object) => {
                        hasher.write_u8(1);
                        object.hash_into(hasher);
                    }
                    None => hasher.write_u8(0),
                }
                hasher.write_usize(entry.script);
                hasher.write_i32(entry.instruction);
                hasher.write_i32(entry.times_to_loop);
                hasher.write_i32(entry.frames_to_wait);
                hasher.write_bool(entry.blocked);
            }
        }
        hasher.write_usize(self.flagged.len());
        for &script in &self.flagged {
            hasher.write_usize(script);
        }
        hasher.write_u32(self.next_id);
    }

    fn head(&self, slot: usize) -> Option<&SeqEntry> {
        self.chains.get(slot)?.first()
    }

    fn head_mut(&mut self, slot: usize) -> Option<&mut SeqEntry> {
        self.chains.get_mut(slot)?.first_mut()
    }

    fn slot_of(&self, id: u32) -> Option<usize> {
        self.chains
            .iter()
            .position(|chain| chain.first().is_some_and(|head| head.id == id))
    }

    fn finish_head(&mut self, slot: usize) {
        if let Some(chain) = self.chains.get_mut(slot) {
            if !chain.is_empty() {
                chain.remove(0);
            }
            if chain.is_empty() {
                self.chains.remove(slot);
            }
        }
    }

    fn drop_chain(&mut self, slot: usize) {
        if slot < self.chains.len() {
            self.chains.remove(slot);
        }
    }
}

impl RotwkRules {
    fn sequence_members(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        team: TeamId,
    ) -> Vec<Entity> {
        let world = tick.state();
        state
            .rt
            .teams
            .get(team)
            .map(|instance| {
                instance
                    .members()
                    .iter()
                    .copied()
                    .filter(|&m| world.is_live(m) && !world.contains(ids.doomed, m))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn sequence_member_idle(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        entity: Entity,
    ) -> bool {
        let world = tick.state();
        if world.contains(ids.member, entity) {
            return true;
        }
        self.regular_state_idle(world, ids, entity)
            && !state.rt.hunters.contains(entity)
            && !state.rt.facings.contains(entity)
            && !state.rt.attack_areas.holds(entity)
            && !world.contains(ids.entering, entity)
            && !world.contains(ids.deferred_order, entity)
            && world.get(ids.guard, entity).is_none_or(|guard| {
                !guard.scripted && matches!(guard.state, GuardState::Idle | GuardState::Return)
            })
    }

    fn sequence_idle(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        object: Option<Entity>,
        team: Option<TeamId>,
    ) -> bool {
        if let Some(object) = object {
            return self.sequence_member_idle(tick, ids, state, object);
        }
        let Some(team) = team else {
            return true;
        };
        self.sequence_members(tick, ids, state, team)
            .into_iter()
            .all(|member| self.sequence_member_idle(tick, ids, state, member))
    }

    fn idle_team(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        team: TeamId,
    ) -> Result<()> {
        let members = self.sequence_members(tick, ids, state, team);
        for &member in &members {
            state.rt.walks.forget(member);
            state.rt.hunters.forget(member);
            state.rt.attack_areas.forget(member);
            state.rt.facings.forget(member);
        }
        self.issue_script_order(tick, ids, &members, ScriptOrder::Stop)
    }

    pub(super) fn is_default_team(
        &self,
        engine: &MapScriptEngine,
        state: &MapScriptState,
        team: TeamId,
    ) -> bool {
        state
            .rt
            .teams
            .get(team)
            .and_then(|instance| engine.team_protos.get(instance.proto))
            .is_some_and(|proto| {
                proto
                    .name
                    .strip_prefix("team")
                    .is_some_and(|rest| rest.eq_ignore_ascii_case(&proto.owner))
            })
    }

    pub(super) fn queue_sequential(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        index: usize,
        def: &ScriptDef,
        player: Option<PlayerId>,
    ) -> Result<()> {
        let deactivate = |state: &mut MapScriptState| {
            if let Some(slot) = state.active.get_mut(index) {
                *slot = false;
            }
        };
        if !self.conditions_hold(tick, ids, state, def, player) {
            if !def.actions_false.is_empty() {
                deactivate(state);
            }
            return Ok(());
        }
        let target = &def.sequence.target;
        let (team, object) = if def.sequence.target_is_unit {
            (
                None,
                self.object_named(tick, ids, &state.rt, target, &def.side),
            )
        } else {
            (self.peek_team(state, target, &def.side), None)
        };
        if team.is_none() && object.is_none() {
            deactivate(state);
            return Ok(());
        }
        if let Some(team) = team {
            self.idle_team(tick, ids, state, team)?;
        }
        state
            .rt
            .seq
            .append(team, object, index, def.sequence.loop_count);
        diag!(
            "script: sequential {:?} queued on {target:?} (loop {})",
            def.name,
            def.sequence.loop_count
        );
        if def.one_shot {
            deactivate(state);
        }
        Ok(())
    }

    pub(super) fn step_sequential(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mut slot = 0usize;
        let mut last = usize::MAX;
        let mut revisits = 0u32;
        while slot < state.rt.seq.chains.len() {
            if slot == last {
                revisits += 1;
                if revisits > MAX_REVISITS {
                    slot += 1;
                    last = usize::MAX;
                    continue;
                }
            } else {
                last = slot;
                revisits = 0;
            }
            if !self.step_slot(tick, ids, engine, state, &mut slot, kills)? {
                slot += 1;
            }
        }
        Ok(())
    }

    fn step_slot(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        slot: &mut usize,
        kills: &mut Vec<(Entity, f32)>,
    ) -> Result<bool> {
        let Some(head) = state.rt.seq.head(*slot).cloned() else {
            return Ok(false);
        };
        let world = tick.state();
        let object = head.object.filter(|&o| world.is_live(o));
        let team = head.team.filter(|&t| state.rt.teams.get(t).is_some());
        if object.is_none() && team.is_none() {
            state.rt.seq.finish_head(*slot);
            return Ok(true);
        }
        let advance =
            head.frames_to_wait <= 0 && self.sequence_idle(tick, ids, state, object, team);
        if !advance {
            if let Some(head) = state.rt.seq.head_mut(*slot) {
                if head.frames_to_wait > 0 {
                    head.frames_to_wait -= 1;
                } else if !head.blocked {
                    head.blocked = true;
                    diag!(
                        "script: sequential {:?} waits for its target to go idle before step {}",
                        engine.scripts.get(head.script).map(|d| d.name.as_str()),
                        head.instruction.saturating_add(1)
                    );
                }
            }
            return Ok(false);
        }
        let instruction = head.instruction.saturating_add(1);
        let action = engine.scripts.get(head.script).and_then(|def| {
            usize::try_from(instruction)
                .ok()
                .and_then(|i| def.actions.get(i).cloned())
        });
        let Some(action) = action else {
            if head.times_to_loop != 0 {
                let remaining = if head.times_to_loop < 0 {
                    -1
                } else {
                    head.times_to_loop - 1
                };
                state
                    .rt
                    .seq
                    .append(head.team, head.object, head.script, remaining);
            }
            state.rt.seq.finish_head(*slot);
            return Ok(true);
        };
        let player = match object {
            Some(object) => tick.state().get(ids.owner, object).map(|o| o.0),
            None => team.and_then(|t| self.team_owner(engine, state, t)),
        };
        if let Some(entry) = state.rt.seq.head_mut(*slot) {
            entry.instruction = instruction;
            entry.frames_to_wait = -1;
            entry.blocked = false;
        }
        diag!(
            "script: sequential {:?} runs step {instruction}",
            engine.scripts.get(head.script).map(|d| d.name.as_str())
        );
        let saved = state.rt.condition_team;
        state.rt.condition_team = team;
        state.rt.objects.set_condition_object(object);
        let side = engine
            .scripts
            .get(head.script)
            .map_or_else(String::new, |def| def.side.clone());
        let outer = state.rt.objects.swap_running_side(side);
        let outcome = action.run(self, tick, ids, engine, state, player, 0, kills);
        state.rt.objects.swap_running_side(outer);
        state.rt.condition_team = saved;
        outcome?;
        let Some(now) = state.rt.seq.slot_of(head.id) else {
            return Ok(true);
        };
        *slot = now;
        if !self.sequence_idle(tick, ids, state, object, team) {
            return Ok(false);
        }
        let world = tick.state();
        let gone = match (object, team) {
            (Some(object), _) => !world.is_live(object) || world.contains(ids.doomed, object),
            (None, Some(team)) => {
                !self.team_has_units(tick, ids, state, team)
                    && !self.is_default_team(engine, state, team)
            }
            (None, None) => true,
        };
        if gone {
            state.rt.seq.drop_chain(*slot);
        }
        Ok(true)
    }
}

enum Kind {
    Execute { script: String, loops: Option<i32> },
    Stop,
    Hold { amount: i32, seconds: bool },
}

struct ActionSequential {
    name: String,
    side: String,
    is_unit: bool,
    kind: Kind,
    verb: &'static str,
}

impl ActionVerb for ActionSequential {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let (team, object) = if self.is_unit {
            (
                None,
                rules.object_named(tick, ids, &state.rt, &self.name, &self.side),
            )
        } else {
            (rules.peek_team(state, &self.name, &self.side), None)
        };
        if team.is_none() && object.is_none() {
            return Ok(());
        }
        match &self.kind {
            Kind::Execute { script, loops } => {
                let Some(index) = engine
                    .scripts
                    .iter()
                    .position(|s| s.name == *script && s.side == self.side)
                    .or_else(|| engine.scripts.iter().position(|s| s.name == *script))
                else {
                    return Ok(());
                };
                if let Some(team) = team {
                    rules.idle_team(tick, ids, state, team)?;
                }
                state.rt.seq.append(team, object, index, loops.unwrap_or(0));
                diag!("script: {} queues {script:?} on {:?}", self.verb, self.name);
            }
            Kind::Stop => {
                match (team, object) {
                    (Some(team), _) => state.rt.seq.remove_team(team),
                    (None, Some(object)) => state.rt.seq.remove_object(object),
                    (None, None) => {}
                }
                diag!("script: {} stops {:?}", self.verb, self.name);
            }
            Kind::Hold { amount, seconds } => {
                match (team, object) {
                    (Some(team), _) => rules.idle_team(tick, ids, state, team)?,
                    (None, Some(object)) => {
                        rules.issue_script_order(tick, ids, &[object], ScriptOrder::Stop)?;
                    }
                    (None, None) => {}
                }
                let hz = i32::try_from(RETAIL_LOGIC_HZ).unwrap_or(1);
                let frames = if *seconds {
                    amount.saturating_mul(hz)
                } else {
                    *amount
                };
                state.rt.seq.set_timer(team, object, frames);
                diag!(
                    "script: {} holds {:?} for {frames} script frame(s)",
                    self.verb,
                    self.name
                );
            }
        }
        Ok(())
    }
}

fn build(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    is_unit: bool,
    verb: &'static str,
    kind: impl FnOnce(&[Parameter]) -> Option<Kind>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionSequential {
        name: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        is_unit,
        kind: kind(params)?,
        verb,
    }))
}

fn execute(params: &[Parameter], looping: bool) -> Option<Kind> {
    Some(Kind::Execute {
        script: parameter_string(params, 1)?,
        loops: if looping {
            Some(parameter_int(params, 2)?.saturating_sub(1))
        } else {
            None
        },
    })
}

fn hold(params: &[Parameter], seconds: bool) -> Option<Kind> {
    Some(Kind::Hold {
        amount: parameter_int(params, 1)?,
        seconds,
    })
}

macro_rules! rows {
    ($($fn_name:ident, $unit:expr, $verb:literal, $kind:expr;)*) => {
        $(pub(super) fn $fn_name(
            params: &[Parameter],
            ctx: &ParseCtx<'_>,
        ) -> Option<Arc<dyn ActionVerb>> {
            build(params, ctx, $unit, $verb, $kind)
        })*
    };
}

rows! {
    parse_team_execute, false, "TEAM_EXECUTE_SEQUENTIAL_SCRIPT", |p| execute(p, false);
    parse_team_execute_looping, false, "TEAM_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING", |p| execute(p, true);
    parse_team_stop, false, "TEAM_STOP_SEQUENTIAL_SCRIPT", |_| Some(Kind::Stop);
    parse_unit_execute, true, "UNIT_EXECUTE_SEQUENTIAL_SCRIPT", |p| execute(p, false);
    parse_unit_execute_looping, true, "UNIT_EXECUTE_SEQUENTIAL_SCRIPT_LOOPING", |p| execute(p, true);
    parse_unit_stop, true, "UNIT_STOP_SEQUENTIAL_SCRIPT", |_| Some(Kind::Stop);
    parse_team_hold_seconds, false, "TEAM_IDLE_FOR_SECONDS", |p| hold(p, true);
    parse_team_hold_frames, false, "TEAM_IDLE_FOR_FRAMECOUNT", |p| hold(p, false);
    parse_unit_hold_frames, true, "UNIT_IDLE_FOR_FRAMECOUNT", |p| hold(p, false);
}
