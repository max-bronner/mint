use std::sync::Arc;

use mint_movement::Vec2;
use mint_sage_shroud::PlayerMask as ShroudMask;
use mint_sim::{Entity, PlayerId, Result, StateHasher, Tick};

use super::{
    parameter_real, parameter_string, ActionVerb, MapScriptEngine, MapScriptState, ParseCtx,
};
use crate::diag::diag;
use crate::rules::{Ids, RotwkRules};
use crate::shroud::{look_disc, look_polygon, map_wide, LookOp, MapShroudOp};

#[derive(Debug, Clone, PartialEq)]
enum Place {
    Waypoint(Option<Vec2>),
    Area(Option<Vec<(f32, f32)>>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NamedReveal {
    name: String,
    place: Place,
    radius: f32,
    player_text: String,
}

impl NamedReveal {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bytes(self.name.as_bytes());
        hasher.write_u8(0);
        hasher.write_bytes(self.player_text.as_bytes());
        hasher.write_u8(0);
        hasher.write_f32(self.radius);
        match &self.place {
            Place::Waypoint(at) => {
                hasher.write_u8(1);
                if let Some(at) = at {
                    hasher.write_f32(at.x);
                    hasher.write_f32(at.y);
                }
            }
            Place::Area(points) => {
                hasher.write_u8(2);
                for &(x, y) in points.iter().flatten() {
                    hasher.write_f32(x);
                    hasher.write_f32(y);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct NamedReveals(Vec<NamedReveal>);

impl NamedReveals {
    pub(super) fn hash_into(&self, hasher: &mut StateHasher) {
        if self.0.is_empty() {
            return;
        }
        hasher.write_usize(self.0.len());
        for reveal in &self.0 {
            reveal.hash_into(hasher);
        }
    }
}

fn look_at(
    rules: &RotwkRules,
    tick: &mut Tick<'_>,
    ids: Ids,
    state: &MapScriptState,
    player: Option<PlayerId>,
    reveal: &NamedReveal,
    op: LookOp,
) {
    for target in rules
        .player_mask(&state.rt, &reveal.player_text, player)
        .players()
    {
        let mask = ShroudMask::of(target);
        let shroud = RotwkRules::shroud_ids(ids);
        match &reveal.place {
            Place::Waypoint(Some(at)) => {
                look_disc(tick, shroud, (at.x, at.y, reveal.radius), mask, op)
            }
            Place::Area(Some(points)) => look_polygon(tick, shroud, points, mask, op),
            _ => {}
        }
        diag!(
            "script: named reveal {:?} {:?} for player {}",
            reveal.name,
            op,
            target.get()
        );
    }
}

struct ActionRevealPermanently {
    new: NamedReveal,
}
impl ActionVerb for ActionRevealPermanently {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let existing = state
            .reveals
            .0
            .iter()
            .position(|reveal| reveal.name == self.new.name);
        let index = existing.unwrap_or_else(|| {
            state.reveals.0.push(self.new.clone());
            state.reveals.0.len().saturating_sub(1)
        });
        if let Some(record) = state.reveals.0.get(index).cloned() {
            look_at(rules, tick, ids, state, player, &record, LookOp::Keep);
        }
        Ok(())
    }
}

struct ActionUndoNamedReveal {
    name: String,
}
impl ActionVerb for ActionUndoNamedReveal {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let Some(index) = state
            .reveals
            .0
            .iter()
            .position(|reveal| reveal.name == self.name)
        else {
            return Ok(());
        };
        let record = state.reveals.0.remove(index);
        look_at(rules, tick, ids, state, player, &record, LookOp::Undo);
        Ok(())
    }
}

struct ActionGlance {
    place: Place,
    radius: f32,
    player_text: String,
}
impl ActionVerb for ActionGlance {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let record = NamedReveal {
            name: String::new(),
            place: self.place.clone(),
            radius: self.radius,
            player_text: self.player_text.clone(),
        };
        look_at(rules, tick, ids, state, player, &record, LookOp::Glance);
        Ok(())
    }
}

struct ActionMapWide {
    op: MapShroudOp,
    player_text: String,
}
impl ActionVerb for ActionMapWide {
    fn run(
        &self,
        rules: &RotwkRules,
        tick: &mut Tick<'_>,
        ids: Ids,
        _: &MapScriptEngine,
        state: &mut MapScriptState,
        player: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        let mask = rules.player_mask(&state.rt, &self.player_text, player);
        let targets: Vec<PlayerId> = if mask.is_empty() {
            rules.human_players().collect()
        } else {
            mask.players().collect()
        };
        for target in targets {
            map_wide(tick, RotwkRules::shroud_ids(ids), self.op, target);
            diag!("script: map-wide {:?} for player {}", self.op, target.get());
        }
        Ok(())
    }
}

struct ActionNothing;
impl ActionVerb for ActionNothing {
    fn run(
        &self,
        _: &RotwkRules,
        _: &mut Tick<'_>,
        _: Ids,
        _: &MapScriptEngine,
        _: &mut MapScriptState,
        _: Option<PlayerId>,
        _: u8,
        _: &mut Vec<(Entity, f32)>,
    ) -> Result<()> {
        Ok(())
    }
}

fn waypoint_place(ctx: &ParseCtx<'_>, name: &str) -> Place {
    Place::Waypoint(ctx.waypoints.get(name).map(|(at, _)| *at))
}

fn area_place(ctx: &ParseCtx<'_>, name: &str) -> Place {
    Place::Area(ctx.trigger_areas.get(name).cloned())
}

pub(super) fn parse_reveal_in_trigger(
    params: &[mint_sage_map::Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let area = parameter_string(params, 0)?;
    let player_text = parameter_string(params, 1)?;
    let name = parameter_string(params, 2)?;
    Some(Arc::new(ActionRevealPermanently {
        new: NamedReveal {
            name,
            place: area_place(ctx, &area),
            radius: 0.0,
            player_text,
        },
    }))
}

pub(super) fn parse_reveal_at_waypoint(
    params: &[mint_sage_map::Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 0)?;
    let radius = parameter_real(params, 1)?;
    let player_text = parameter_string(params, 2)?;
    let name = parameter_string(params, 3)?;
    Some(Arc::new(ActionRevealPermanently {
        new: NamedReveal {
            name,
            place: waypoint_place(ctx, &waypoint),
            radius,
            player_text,
        },
    }))
}

pub(super) fn parse_undo_named_reveal(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionUndoNamedReveal {
        name: parameter_string(params, 0)?,
    }))
}

pub(super) fn parse_glance_at_waypoint(
    params: &[mint_sage_map::Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let waypoint = parameter_string(params, 0)?;
    Some(Arc::new(ActionGlance {
        place: waypoint_place(ctx, &waypoint),
        radius: parameter_real(params, 1)?,
        player_text: parameter_string(params, 2)?,
    }))
}

pub(super) fn parse_glance_in_trigger(
    params: &[mint_sage_map::Parameter],
    ctx: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    let area = parameter_string(params, 0)?;
    Some(Arc::new(ActionGlance {
        place: area_place(ctx, &area),
        radius: 0.0,
        player_text: parameter_string(params, 1)?,
    }))
}

fn map_wide_verb(
    params: &[mint_sage_map::Parameter],
    op: MapShroudOp,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionMapWide {
        op,
        player_text: parameter_string(params, 0)?,
    }))
}

pub(super) fn parse_reveal_all(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    map_wide_verb(params, MapShroudOp::RevealAll)
}

pub(super) fn parse_reveal_all_permanently(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    map_wide_verb(params, MapShroudOp::RevealAllPermanently)
}

pub(super) fn parse_undo_reveal_all_permanently(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    map_wide_verb(params, MapShroudOp::UndoRevealAllPermanently)
}

pub(super) fn parse_shroud_all(
    params: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    map_wide_verb(params, MapShroudOp::ShroudAll)
}

pub(super) fn parse_nothing(
    _: &[mint_sage_map::Parameter],
    _: &ParseCtx<'_>,
) -> Option<Arc<dyn ActionVerb>> {
    Some(Arc::new(ActionNothing))
}
