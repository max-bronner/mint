use std::collections::BTreeMap;

use mint_movement::Vec2;
use mint_sim::{Entity, Result, StateHasher, Tick};

use super::orders::ScriptOrder;
use super::MapScriptState;
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq)]
struct Walk {
    label: String,
    walking: bool,
    done: bool,
    team: Option<TeamLeg>,
    own: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TeamLeg {
    key: Entity,
    offset: Vec2,
    leg: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct TeamPath {
    points: Vec<Vec2>,
    current: usize,
    attack: bool,
}

pub(super) fn within_group_fudge(centre: Vec2, goal: Vec2, members: usize, fudge: f32) -> bool {
    #[allow(clippy::cast_precision_loss)]
    let reach = members as f32 * fudge;
    centre.minus(goal).length() <= reach
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Walks {
    walkers: BTreeMap<Entity, Walk>,
    teams: BTreeMap<Entity, TeamPath>,
}

impl Walks {
    pub(super) fn begin(&mut self, entity: Entity, label: &str) {
        self.walkers.insert(
            entity,
            Walk {
                label: label.to_owned(),
                walking: false,
                done: false,
                team: None,
                own: false,
            },
        );
    }

    pub(super) fn begin_own(&mut self, entity: Entity, label: &str) {
        self.begin(entity, label);
        if let Some(walk) = self.walkers.get_mut(&entity) {
            walk.own = true;
        }
    }

    pub(super) fn complete(&mut self, entity: Entity) {
        if let Some(walk) = self.walkers.get_mut(&entity) {
            walk.done = true;
        }
    }

    pub(super) fn begin_team(
        &mut self,
        members: &[(Entity, Vec2)],
        label: &str,
        points: Vec<Vec2>,
        attack: bool,
    ) {
        let Some(&(key, _)) = members.first() else {
            return;
        };
        self.teams.insert(
            key,
            TeamPath {
                points,
                current: 0,
                attack,
            },
        );
        for &(entity, offset) in members {
            self.walkers.insert(
                entity,
                Walk {
                    label: label.to_owned(),
                    walking: false,
                    done: false,
                    own: false,
                    team: Some(TeamLeg {
                        key,
                        offset,
                        leg: 0,
                    }),
                },
            );
        }
    }

    pub(super) fn forget(&mut self, entity: Entity) {
        self.walkers.remove(&entity);
    }

    pub(super) fn finished(&self, entity: Entity, label: &str) -> bool {
        self.walkers
            .get(&entity)
            .is_some_and(|walk| walk.done && walk.label.eq_ignore_ascii_case(label))
    }

    pub(crate) fn retain_live(&mut self, is_live: impl Fn(Entity) -> bool) {
        self.walkers.retain(|&entity, _| is_live(entity));
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.walkers.is_empty() {
            return;
        }
        hasher.write_usize(self.walkers.len());
        for (entity, walk) in &self.walkers {
            entity.hash_into(hasher);
            hasher.write_bytes(walk.label.as_bytes());
            hasher.write_bool(walk.walking);
            hasher.write_bool(walk.done);
            if let Some(leg) = &walk.team {
                leg.key.hash_into(hasher);
                hasher.write_f32(leg.offset.x);
                hasher.write_f32(leg.offset.y);
                hasher.write_usize(leg.leg);
            }
        }
        for (key, team) in &self.teams {
            key.hash_into(hasher);
            hasher.write_usize(team.current);
            hasher.write_usize(team.points.len());
            if team.attack {
                hasher.write_bool(true);
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn step_walks(&self, tick: &Tick<'_>, ids: Ids, state: &mut MapScriptState) {
        let world = tick.state();
        for (&entity, walk) in &mut state.rt.walks.walkers {
            if walk.done || walk.own || walk.team.is_some() {
                continue;
            }
            if world.contains(ids.path, entity) {
                walk.walking = true;
            } else if walk.walking {
                walk.done = true;
            }
        }
    }

    pub(super) fn step_team_walks(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
    ) -> Result<()> {
        let keys: Vec<Entity> = state.rt.walks.teams.keys().copied().collect();
        for key in keys {
            let walks = &mut state.rt.walks;
            let members: Vec<(Entity, TeamLeg)> = walks
                .walkers
                .iter()
                .filter_map(|(&entity, walk)| walk.team.map(|leg| (entity, leg)))
                .filter(|(_, leg)| leg.key == key)
                .collect();
            if members.is_empty() {
                walks.teams.remove(&key);
                continue;
            }
            let mut retargets: Vec<Entity> = Vec::new();
            for &(entity, leg) in &members {
                let Some(team) = walks.teams.get_mut(&key) else {
                    break;
                };
                let Some(walk) = walks.walkers.get_mut(&entity) else {
                    continue;
                };
                if walk.done {
                    continue;
                }
                if leg.leg != team.current {
                    retargets.push(entity);
                    continue;
                }
                let pather = Self::pather(tick, ids, entity);
                let travelling = tick.state().contains(ids.path, pather)
                    || tick.state().contains(ids.attack_move_order, pather);
                if travelling {
                    walk.walking = true;
                }
                let finished = (!travelling && walk.walking)
                    || self.close_enough_for_skirmish_ai(tick, ids, &members, entity, team, leg);
                if finished {
                    if team.current + 1 < team.points.len() {
                        team.current += 1;
                        retargets.push(entity);
                    } else {
                        walk.done = true;
                    }
                }
            }
            for entity in retargets {
                self.retarget_team_member(tick, ids, state, key, entity)?;
            }
        }
        Ok(())
    }

    fn close_enough_for_skirmish_ai(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        members: &[(Entity, TeamLeg)],
        entity: Entity,
        team: &TeamPath,
        leg: TeamLeg,
    ) -> bool {
        let Some(owner) = tick.state().get(ids.owner, entity).map(|owner| owner.0) else {
            return false;
        };
        let is_skirmish_ai = self.ai_seeds.iter().any(|seed| seed.player == owner)
            || (self.group_formation.force_skirmish_ai && self.is_computer(owner));
        let (Some(&point), true) = (team.points.get(team.current), is_skirmish_ai) else {
            return false;
        };
        let positions: Vec<Vec2> = members
            .iter()
            .filter_map(|(member, _)| self.position_of_entity(tick, ids, *member))
            .collect();
        let Some(centre) = super::cinematic::mean_position(&positions) else {
            return false;
        };
        within_group_fudge(
            centre,
            point.plus(leg.offset),
            members.len(),
            self.group_formation.skirmish_group_fudge,
        )
    }

    pub(super) fn retarget_team_member(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        state: &mut MapScriptState,
        key: Entity,
        entity: Entity,
    ) -> Result<()> {
        let walks = &mut state.rt.walks;
        let Some(team) = walks.teams.get(&key) else {
            return Ok(());
        };
        let (current, attack, more) = (
            team.current,
            team.attack,
            team.current + 1 < team.points.len(),
        );
        let Some(&point) = team.points.get(current) else {
            return Ok(());
        };
        let Some(walk) = walks.walkers.get_mut(&entity) else {
            return Ok(());
        };
        let Some(leg) = walk.team.as_mut() else {
            return Ok(());
        };
        leg.leg = current;
        let goal = point.plus(leg.offset);
        let order = match (attack, more) {
            (true, true) => ScriptOrder::AttackFollowLeg(goal, false),
            (true, false) => ScriptOrder::AttackFollow(goal, false),
            (false, true) => ScriptOrder::MoveLeg(goal, false),
            (false, false) => ScriptOrder::MoveFollow(goal, false),
        };
        self.issue_script_order(tick, ids, &[entity], order)?;
        let pather = Self::pather(tick, ids, entity);
        walk.walking = !attack && !tick.state().contains(ids.path, pather);
        Ok(())
    }

    pub(super) fn pather(tick: &Tick<'_>, ids: Ids, entity: Entity) -> Entity {
        tick.state()
            .get(ids.member, entity)
            .map_or(entity, |member| member.horde)
    }
}
