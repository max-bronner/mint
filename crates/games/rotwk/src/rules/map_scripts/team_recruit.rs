use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Result, Tick};

use super::counters::SideKey;
use super::orders::ScriptOrder;
use super::status::ScriptStatusBit;
use super::team::TeamId;
use super::team_build::QueuedTeam;
use super::team_build_step::creation_order;
use super::{
    counters::TICKS_PER_SCRIPT_FRAME, parameter_int, parameter_real, parameter_string, ActionVerb,
    MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};

const MIN_RADIUS: f32 = 1.0;
const MAP_WIDE_RADIUS: f32 = 99_999.0;

struct ActionRecruitAtTeam {
    proto: String,
    radius: f32,
    at: String,
    side: String,
}

impl RotwkRules {
    #[allow(clippy::too_many_arguments)]
    fn recruit_specific_team(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        proto: usize,
        radius: f32,
        pos: Option<Vec2>,
    ) -> Result<()> {
        let Some(entry) = engine.team_protos.get(proto) else {
            return Ok(());
        };
        let Some(player) = state
            .rt
            .teams
            .controller(proto)
            .or_else(|| self.resolve_side_player(&entry.owner))
        else {
            return Ok(());
        };
        if state.rt.builders.get(player).is_none() {
            return Ok(());
        }
        let radius = if radius < MIN_RADIUS {
            MAP_WIDE_RADIUS
        } else {
            radius
        };
        let existing = state.rt.teams.instances_of(proto).first().copied();
        if entry.singleton
            && existing.is_some_and(|team| !self.live_members(tick, state, team).is_empty())
        {
            return Ok(());
        }
        let team = match existing.filter(|_| entry.singleton) {
            Some(team) => team,
            None => state.rt.teams.create_instance(proto, false),
        };
        let home = self.team_home(engine, state, team);
        let centre = pos.or(home).unwrap_or(Vec2::new(0.0, 0.0));
        let mut recruited = 0_u32;
        for slot in &entry.template.units {
            let Some(kind) = self.kinds.kind(&slot.template) else {
                continue;
            };
            for _ in 0..slot.max.max(0) {
                let Some(unit) = self.try_to_recruit(
                    tick.state(),
                    ids,
                    engine,
                    state,
                    player,
                    team,
                    kind,
                    centre,
                    radius,
                ) else {
                    break;
                };
                recruited = recruited.saturating_add(1);
                state.rt.teams.add_member(team, unit);
                if self.has_ai(tick.state(), ids, unit) {
                    let target = pos
                        .or(home)
                        .or_else(|| self.team_position(tick, ids, state, team));
                    if let Some(target) = target {
                        self.issue_script_order(
                            tick,
                            ids,
                            &[unit],
                            ScriptOrder::Move(target, false),
                        )?;
                    }
                }
            }
        }
        if recruited > 0 {
            let started = tick.frame().get() / TICKS_PER_SCRIPT_FRAME;
            if let Some(builder) = state.rt.builders.get_mut(player) {
                builder.ready_queue.insert(
                    0,
                    QueuedTeam {
                        team,
                        orders: Vec::new(),
                        priority: false,
                        started,
                        sent_to_start: false,
                        reinforce: None,
                    },
                );
            }
            diag!(
                "script: team {:?} recruited {recruited} unit(s)",
                entry.name
            );
        } else if !entry.singleton {
            state.rt.teams.delete_instance(team);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn recruit_from_team(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
        dest: TeamId,
        source: TeamId,
        count: i32,
        types: &str,
    ) -> Result<u32> {
        if count <= 0 {
            return Ok(0);
        }
        let Some(source_proto) = state.rt.teams.get(source).map(|team| team.proto) else {
            return Ok(0);
        };
        let centre = self
            .team_position(tick, ids, state, dest)
            .unwrap_or(Vec2::new(0.0, 0.0));
        let wanted: Vec<_> = state
            .rt
            .lists
            .types_of(types)
            .iter()
            .filter_map(|name| self.kinds.kind(name))
            .collect();
        let mut taken = 0_u32;
        for _ in 0..count {
            let mut candidates = Vec::new();
            for team in state.rt.teams.instances_of(source_proto) {
                for member in self.live_members(tick, state, team) {
                    candidates.push((member, ()));
                }
            }
            let world = tick.state();
            let mut best: Option<(f32, Entity)> = None;
            for (member, ()) in creation_order(candidates) {
                let matches = world.get(ids.kind, member).is_some_and(|have| {
                    wanted
                        .iter()
                        .any(|&want| self.kinds.equivalent(want, *have))
                });
                if !matches
                    || world.contains(ids.doomed, member)
                    || world.contains(ids.held, member)
                    || state
                        .rt
                        .statuses
                        .has(member, ScriptStatusBit::NotRecruitable)
                {
                    continue;
                }
                let Some(at) = self.position_of_entity(tick, ids, member) else {
                    continue;
                };
                let (dx, dy) = (centre.x - at.x, centre.y - at.y);
                let distance = dx * dx + dy * dy;
                if best.is_none_or(|(held, _)| distance <= held) {
                    best = Some((distance, member));
                }
            }
            let Some((_, unit)) = best else {
                break;
            };
            self.move_to_team(tick, ids, engine, state, unit, dest)?;
            taken = taken.saturating_add(1);
        }
        Ok(taken)
    }
}

impl ActionVerb for ActionRecruitAtTeam {
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
        let key = SideKey::qualify(&self.proto, &self.side);
        let Some(proto) = engine.team_protos.find(&key.side, &key.name) else {
            return Ok(());
        };
        let pos = if self.at.is_empty() {
            None
        } else {
            let Some(at) = rules.peek_team(state, &self.at, &self.side) else {
                return Ok(());
            };
            let Some(centre) = rules.team_position(tick, ids, state, at) else {
                return Ok(());
            };
            Some(centre)
        };
        rules.recruit_specific_team(tick, ids, engine, state, proto, self.radius, pos)
    }
}

struct ActionRecruitFromTeam {
    dest: String,
    count: i32,
    types: String,
    source: String,
    side: String,
}

impl ActionVerb for ActionRecruitFromTeam {
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
        let Some(dest) = rules.get_team(state, &self.dest, &self.side, true) else {
            return Ok(());
        };
        let Some(source) = rules.peek_team(state, &self.source, &self.side) else {
            return Ok(());
        };
        let taken = rules.recruit_from_team(
            tick,
            ids,
            engine,
            state,
            dest,
            source,
            self.count,
            &self.types,
        )?;
        diag!(
            "script: {:?} took {taken} unit(s) from {:?}",
            self.dest,
            self.source
        );
        Ok(())
    }
}

pub(super) fn parse_recruit_at_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRecruitAtTeam {
        proto: parameter_string(params, 0)?,
        radius: parameter_real(params, 1)?,
        at: parameter_string(params, 2)?,
        side: ctx.side.to_owned(),
    }))
}

pub(super) fn parse_recruit_from_team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionRecruitFromTeam {
        dest: parameter_string(params, 0)?,
        count: parameter_int(params, 1)?,
        types: parameter_string(params, 2)?,
        source: parameter_string(params, 3)?,
        side: ctx.side.to_owned(),
    }))
}
