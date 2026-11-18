use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_map::Parameter;
use mint_sim::{Entity, PlayerId, Tick};

use super::{compare, parameter_int, parameter_string, ConditionVerb, MapScriptState, ParseCtx};
use crate::rules::{Ids, RotwkRules};
use crate::templates::SURFACE_TABLE;

#[derive(Debug, Clone)]
pub(super) struct Area {
    pub(super) name: String,
    polygon: Option<Vec<(f32, f32)>>,
    bounds: (Vec2, Vec2),
}

impl Area {
    pub(super) fn resolve(ctx: &ParseCtx<'_>, name: &str) -> Self {
        let polygon = ctx.trigger_areas.get(name).cloned();
        let mut low = Vec2::new(f32::MAX, f32::MAX);
        let mut high = Vec2::new(f32::MIN, f32::MIN);
        for &(x, y) in polygon.iter().flatten() {
            low = Vec2::new(low.x.min(x), low.y.min(y));
            high = Vec2::new(high.x.max(x), high.y.max(y));
        }
        Self {
            name: name.to_owned(),
            polygon,
            bounds: (low, high),
        }
    }

    pub(super) fn exists(&self) -> bool {
        self.polygon.is_some()
    }

    pub(super) fn bounds(&self) -> (Vec2, Vec2) {
        self.bounds
    }

    pub(super) fn centre_and_radius(&self) -> Option<(Vec2, f32)> {
        self.polygon.as_ref()?;
        let (low, high) = self.bounds;
        let (half_x, half_y) = ((high.x - low.x) * 0.5, (high.y - low.y) * 0.5);
        Some((
            Vec2::new(low.x + half_x, low.y + half_y),
            (half_x * half_x + half_y * half_y).sqrt(),
        ))
    }

    pub(super) fn contains(&self, point: Vec2) -> bool {
        let Some(points) = &self.polygon else {
            return false;
        };
        let (low, high) = self.bounds;
        if point.x < low.x || point.x > high.x || point.y < low.y || point.y > high.y {
            return false;
        }
        let mut inside = false;
        let mut previous = points.last().copied();
        for &(x, y) in points {
            if let Some((px, py)) = previous {
                if (y > point.y) != (py > point.y) {
                    let at = (px - x) * (point.y - y) / (py - y) + x;
                    if point.x < at {
                        inside = !inside;
                    }
                }
            }
            previous = Some((x, y));
        }
        inside
    }
}

pub(super) fn surfaces_param(params: &[Parameter]) -> i32 {
    parameter_int(params, 2).unwrap_or(0)
}

pub(super) fn truncated(position: Vec2) -> Vec2 {
    Vec2::new(position.x.trunc(), position.y.trunc())
}

impl RotwkRules {
    pub(super) fn positions(&self, tick: &Tick<'_>, ids: Ids, entities: &[Entity]) -> Vec<Vec2> {
        entities
            .iter()
            .filter_map(|&e| tick.state().get(ids.body, e).map(|body| body.0.position))
            .collect()
    }

    fn count_in_area(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        player: PlayerId,
        area: &Area,
        filter: &dyn Fn(Entity) -> bool,
    ) -> i64 {
        let world = tick.state();
        let Some(owners) = world.store(ids.owner) else {
            return 0;
        };
        let mut count = 0_i64;
        for (entity, owner) in owners.iter() {
            if owner.0 != player || !world.is_live(entity) || world.contains(ids.doomed, entity) {
                continue;
            }
            let Some(body) = world.get(ids.body, entity) else {
                continue;
            };
            if !area.contains(body.0.position) || !filter(entity) {
                continue;
            }
            count += 1;
        }
        count
    }

    pub(super) fn counts_for_surface(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        surface: i32,
    ) -> bool {
        let Some(row) = u32::try_from(surface)
            .ok()
            .filter(|&row| (row as usize) < SURFACE_TABLE.len())
        else {
            return false;
        };
        let mask = tick
            .state()
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .and_then(|entry| entry.surfaces);
        match mask {
            Some(mask) => mask & (1_u16 << row) != 0,
            None => row == 0,
        }
    }

    pub(super) fn kind_skipped(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        entity: Entity,
        bits: &[&str],
    ) -> bool {
        bits.iter()
            .any(|bit| self.has_kind_bit(tick.state(), ids, entity, bit))
    }
}

struct ConditionNamedInside {
    unit: String,
    area: Area,
    negate: bool,
}

impl ConditionVerb for ConditionNamedInside {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        _: Option<PlayerId>,
    ) -> bool {
        let inside = self.area.exists()
            && rules
                .object_named(tick, ids, &state.rt, &self.unit, "")
                .and_then(|entity| rules.positions(tick, ids, &[entity]).first().copied())
                .is_some_and(|at| self.area.contains(truncated(at)));
        inside != self.negate
    }

    fn referenced_unit_name(&self) -> Option<&str> {
        Some(&self.unit)
    }
}

#[derive(Clone, Copy)]
enum TeamArea {
    AllInside,
    AnyInside,
    AllOutside,
}

struct ConditionTeamInArea {
    team: String,
    side: String,
    area: Area,
    surfaces: i32,
    test: TeamArea,
}

impl ConditionVerb for ConditionTeamInArea {
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
        let members: Vec<Entity> = rules
            .subject_members(tick, ids, state, &self.team, Some(&self.side))
            .into_iter()
            .filter(|&member| {
                !rules.kind_skipped(tick, ids, member, &["INERT", "MOVE_ONLY"])
                    && rules.counts_for_surface(tick, ids, member, self.surfaces)
            })
            .collect();
        let positions = rules.positions(tick, ids, &members);
        let inside = positions
            .iter()
            .filter(|&&at| self.area.contains(at))
            .count();
        match self.test {
            TeamArea::AllInside => !positions.is_empty() && inside == positions.len(),
            TeamArea::AnyInside => inside > 0,
            TeamArea::AllOutside => !positions.is_empty() && inside == 0,
        }
    }

    fn team_names(&self) -> Vec<String> {
        vec![self.team.clone()]
    }
}

struct ConditionPlayerUnitsInArea {
    player_text: String,
    area: Area,
    outside: bool,
}

impl ConditionVerb for ConditionPlayerUnitsInArea {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        if !self.area.exists() {
            return false;
        }
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        let any = mask.any(|p| {
            rules.count_in_area(tick, ids, p, &self.area, &|entity| {
                !rules.kind_skipped(
                    tick,
                    ids,
                    entity,
                    &["INERT", "MOVE_ONLY", "TAINT", "TAINTEFFECT", "PROJECTILE"],
                )
            }) > 0
        });
        any != self.outside
    }
}

struct ConditionTypeInArea {
    player_text: String,
    comparison: i32,
    count: i32,
    types: String,
    area: Area,
    built_only: bool,
}

impl ConditionVerb for ConditionTypeInArea {
    fn eval(
        &self,
        rules: &RotwkRules,
        tick: &Tick<'_>,
        ids: Ids,
        state: &MapScriptState,
        player: Option<PlayerId>,
    ) -> bool {
        let names = if state.rt.lists.is_list(&self.types) {
            state.rt.lists.types_of(&self.types)
        } else {
            vec![self.types.clone()]
        };
        let kinds: Vec<_> = names.iter().filter_map(|n| rules.kinds.kind(n)).collect();
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        let total = mask.sum(|p| {
            rules.count_in_area(tick, ids, p, &self.area, &|entity| {
                let world = tick.state();
                world
                    .get(ids.kind, entity)
                    .is_some_and(|kind| kinds.contains(kind))
                    && !rules.kind_skipped(tick, ids, entity, &["INERT"])
                    && !(self.built_only && world.contains(ids.construction, entity))
            })
        });
        compare(
            i32::try_from(total).unwrap_or(i32::MAX),
            self.comparison,
            self.count,
        )
    }
}

fn named_inside(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    negate: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionNamedInside {
        unit: parameter_string(params, 0)?,
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        negate,
    }))
}

pub(super) fn parse_named_inside(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    named_inside(params, ctx, false)
}

pub(super) fn parse_named_outside(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    named_inside(params, ctx, true)
}

fn team_in_area(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    test: TeamArea,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTeamInArea {
        team: parameter_string(params, 0)?,
        side: ctx.side.to_owned(),
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        surfaces: surfaces_param(params),
        test,
    }))
}

pub(super) fn parse_team_inside_entirely(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_in_area(params, ctx, TeamArea::AllInside)
}

pub(super) fn parse_team_inside_partially(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_in_area(params, ctx, TeamArea::AnyInside)
}

pub(super) fn parse_team_outside_entirely(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    team_in_area(params, ctx, TeamArea::AllOutside)
}

fn player_units(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    outside: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionPlayerUnitsInArea {
        player_text: parameter_string(params, 0)?,
        area: Area::resolve(ctx, &parameter_string(params, 1)?),
        outside,
    }))
}

pub(super) fn parse_skirmish_has_units(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    player_units(params, ctx, false)
}

pub(super) fn parse_skirmish_outside(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    player_units(params, ctx, true)
}

fn type_in_area(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
    built_only: bool,
) -> Option<Arc<dyn ConditionVerb>> {
    Some(Arc::new(ConditionTypeInArea {
        player_text: parameter_string(params, 0)?,
        comparison: parameter_int(params, 1)?,
        count: parameter_int(params, 2)?,
        types: parameter_string(params, 3)?,
        area: Area::resolve(ctx, &parameter_string(params, 4)?),
        built_only,
    }))
}

pub(super) fn parse_type_in_area(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    type_in_area(params, ctx, false)
}

pub(super) fn parse_type_in_area_built(
    params: &[Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ConditionVerb>> {
    type_in_area(params, ctx, true)
}
