use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, StateHasher, Tick};

use super::area::{truncated, Area};
use super::{parameter_string, ConditionVerb, MapScriptEngine, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Track {
    inside: BTreeSet<Entity>,
    entered: BTreeSet<Entity>,
    exited: BTreeSet<Entity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AreaWatch {
    tracks: BTreeMap<String, Track>,
}

impl AreaWatch {
    fn advance(&mut self, area: &str, now_inside: BTreeSet<Entity>) {
        let track = self.tracks.entry(area.to_owned()).or_default();
        track.entered = now_inside.difference(&track.inside).copied().collect();
        track.exited = track.inside.difference(&now_inside).copied().collect();
        track.inside = now_inside;
    }

    fn entered(&self, area: &str, entity: Entity) -> bool {
        self.tracks
            .get(area)
            .is_some_and(|track| track.entered.contains(&entity))
    }

    fn exited(&self, area: &str, entity: Entity) -> bool {
        self.tracks
            .get(area)
            .is_some_and(|track| track.exited.contains(&entity))
    }

    fn inside(&self, area: &str, entity: Entity) -> bool {
        self.tracks
            .get(area)
            .is_some_and(|track| track.inside.contains(&entity))
    }

    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.tracks.is_empty() {
            return;
        }
        hasher.write_usize(self.tracks.len());
        for (name, track) in &self.tracks {
            hasher.write_bytes(name.as_bytes());
            hasher.write_usize(track.inside.len());
            for entity in &track.inside {
                entity.hash_into(hasher);
            }
        }
    }
}

impl RotwkRules {
    pub(super) fn update_area_watch(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        engine: &MapScriptEngine,
        state: &mut MapScriptState,
    ) {
        let areas = engine.watched_areas();
        if areas.is_empty() {
            return;
        }
        let world = tick.state();
        let Some(bodies) = world.store(ids.body) else {
            return;
        };
        for area in areas {
            let mut now = BTreeSet::new();
            for (entity, body) in bodies.iter() {
                if !area.contains(truncated(body.0.position))
                    || !world.is_live(entity)
                    || world.contains(ids.doomed, entity)
                    || world.contains(ids.member, entity)
                    || world.contains(ids.contained, entity)
                    || self.kind_skipped(tick, ids, entity, &["INERT", "PROJECTILE"])
                {
                    continue;
                }
                now.insert(entity);
            }
            state.rt.area_watch.advance(&area.name, now);
        }
    }
}

struct ConditionNamedCrossed {
    unit: String,
    area: Area,
    exit: bool,
}

impl ConditionVerb for ConditionNamedCrossed {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let Some(entity) = rules.object_named(tick, ids, &state.rt, &self.unit, "") else {
            return false;
        };
        if self.exit {
            return self.area.exists() && state.rt.area_watch.exited(&self.area.name, entity);
        }
        self.area.exists()
            && !rules.kind_skipped(tick, ids, entity, &["INERT"])
            && state.rt.area_watch.entered(&self.area.name, entity)
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }

    fn watched_area(&self) -> Option<&Area> {
        Some(&self.area)
    }
}

#[derive(Clone, Copy)]
enum Crossing {
    AllEntered,
    AnyEntered,
    AllExited,
    AnyExited,
}

struct ConditionTeamCrossed {
    team: String,
    side: String,
    area: Area,
    surfaces: i32,
    crossing: Crossing,
}

impl ConditionVerb for ConditionTeamCrossed {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        if !self.area.exists() {
            return false;
        }
        let watch = &state.rt.area_watch;
        let name = &self.area.name;
        let members = rules.subject_members(tick, ids, state, &self.team, Some(&self.side));
        let mut considered = false;
        let (mut crossed, mut stayed_behind) = (false, false);
        for member in members {
            if rules.kind_skipped(tick, ids, member, &["INERT"])
                || !rules.counts_for_surface(tick, ids, member, self.surfaces)
            {
                continue;
            }
            considered = true;
            match self.crossing {
                Crossing::AnyEntered => crossed |= watch.entered(name, member),
                Crossing::AnyExited => crossed |= watch.exited(name, member),
                Crossing::AllEntered => {
                    if watch.entered(name, member) {
                        crossed = true;
                    } else if !watch.inside(name, member) {
                        stayed_behind = true;
                    }
                }
                Crossing::AllExited => {
                    if watch.exited(name, member) {
                        crossed = true;
                    } else if watch.inside(name, member) {
                        stayed_behind = true;
                    }
                }
            }
        }
        considered && crossed && !stayed_behind
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }

    fn watched_area(&self) -> Option<&Area> {
        Some(&self.area)
    }
}

fn named(params: &[Parameter], ctx: &ParseCtx<'_>, exit: bool) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionNamedCrossed {
        unit: parameter_string(params, 0)?,
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        exit,
    }))
}

fn team(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    crossing: Crossing,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTeamCrossed {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        surfaces: super::area::surfaces_param(params),
        crossing,
    }))
}

pub(super) fn parse_named_entered(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    named(params, ctx, false)
}

pub(super) fn parse_named_exited(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    named(params, ctx, true)
}

pub(super) fn parse_team_entered_entirely(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team(params, ctx, Crossing::AllEntered)
}

pub(super) fn parse_team_entered_partially(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team(params, ctx, Crossing::AnyEntered)
}

pub(super) fn parse_team_exited_entirely(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team(params, ctx, Crossing::AllExited)
}

pub(super) fn parse_team_exited_partially(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team(params, ctx, Crossing::AnyExited)
}
