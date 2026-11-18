use mint_movement::{Passability as _, Vec2};
use mint_sim::{Entity, Tick};

use super::melee_contact::Striker;
use super::unit_cells::{Enemies, UnitCells, HEAVY_MELEE_HITTER};
use super::{Ids, RotwkRules};
use crate::goal_cells::{self, Footprint};

const FALLBACK_GAIN: f32 = 20.0;

const WALK_BACK_STEP: f32 = 20.0;

pub(crate) const ENGAGE_WAIT_MS: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Engage {
    At(Vec2),
    Wait,
}

const ENGAGE_OFFSET: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeleePlacement {
    pub(crate) engagement_cells: u32,
    pub(crate) approach_cells: u32,
    pub(crate) approach_dist: f32,
    pub(crate) approach_tolerance: f32,
    pub(crate) chase_from_behind: f32,
}

impl Default for MeleePlacement {
    fn default() -> Self {
        Self {
            engagement_cells: 50,
            approach_cells: 400,
            approach_dist: 48.0,
            approach_tolerance: 20.0,
            chase_from_behind: 50.0,
        }
    }
}

const APPROACH_MAX_STEP: f32 = 10.0;

struct Placing<'a> {
    rules: &'a RotwkRules,
    tick: &'a Tick<'a>,
    ids: Ids,
    attacker: Entity,
    victim: Entity,
    units: UnitCells,
    footprint: Footprint,
    heavy: bool,
    mover: mint_movement::MoverClass,
    start: Option<mint_movement::Cell>,
    radius: f32,
    from: Vec2,
    victim_at: Vec2,
    victim_radius: f32,
}

pub(super) fn footprint_cells(
    rules: &RotwkRules,
    at: Vec2,
    footprint: Footprint,
) -> Vec<(i32, i32)> {
    let geometry = rules.passability.geometry;
    let extent = rules.passability.grid.extent();
    let Some(rect) = goal_cells::rect(
        extent,
        goal_cells::anchor(geometry, at, footprint),
        footprint,
    ) else {
        return Vec::new();
    };
    let (x0, y0, x1, y1) = rect.span();
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .collect()
}

impl Placing<'_> {
    fn cells(&self, at: Vec2) -> Vec<(i32, i32)> {
        footprint_cells(self.rules, at, self.footprint)
    }

    fn ground(&self, at: Vec2) -> bool {
        let cells = self.cells(at);
        if cells.is_empty() {
            return false;
        }
        let reachable = self.rules.passability.cell_at(at).is_some_and(|cell| {
            self.start
                .is_none_or(|s| self.rules.reachable(self.mover, s, cell))
        });
        let view = self.rules.dynamic_view(self.tick, self.ids);
        reachable
            && cells.iter().all(|&(x, y)| {
                u16::try_from(x)
                    .ok()
                    .zip(u16::try_from(y).ok())
                    .is_some_and(|(x, y)| {
                        view.is_passable(mint_movement::Cell::new(x, y), self.mover)
                    })
            })
    }

    fn in_reach(&self, at: Vec2) -> bool {
        let striker = Striker {
            entity: self.attacker,
            at,
            footprint: self.footprint,
        };
        self.rules
            .melee_in_range(self.tick, self.ids, striker, self.victim, false)
    }

    fn free(&self, at: Vec2) -> bool {
        self.units
            .melee_spot_free(self.cells(at).into_iter(), self.heavy)
            && self.units.walk_clear_of_enemies(self.from, at, self.radius)
    }

    fn cell_centre(&self, at: Vec2, dx: i32, dy: i32) -> Vec2 {
        let geometry = self.rules.passability.geometry;
        let (x, y) = goal_cells::anchor(geometry, at, self.footprint);
        goal_cells::anchor_point(
            geometry,
            (x.saturating_add(dx), y.saturating_add(dy)),
            self.footprint,
        )
    }
}

impl RotwkRules {
    fn placing<'a>(
        &'a self,
        tick: &'a Tick<'a>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
        range: f32,
    ) -> Option<(Placing<'a>, Vec2)> {
        let world = tick.state();
        let me = world.get(ids.body, attacker)?.0;
        let victim_at = world.get(ids.body, victim)?.0.position;
        let radius = |entity: Entity| world.get(ids.shape, entity).map_or(0.0, |s| s.radius);
        let cell_size = self.passability.geometry.cell_size();
        let reach = me.position.minus(victim_at).length() + range + 12.0 * cell_size;
        let placing = Placing {
            rules: self,
            tick,
            ids,
            attacker,
            victim,
            units: self.unit_cells_reading(
                tick,
                ids,
                attacker,
                None,
                victim_at,
                reach,
                Enemies::Listed,
            ),
            footprint: self.footprint_of(tick, ids, attacker),
            heavy: self.has_kind_bit(world, ids, attacker, HEAVY_MELEE_HITTER),
            mover: self.mover_class(tick, ids, attacker),
            start: self.passable_cell(tick, ids, me.position),
            radius: radius(attacker),
            from: me.position,
            victim_at,
            victim_radius: radius(victim),
        };
        let ahead = me.position.plus(me.facing.to_vector().scale(ENGAGE_OFFSET));
        let towards = ahead
            .minus(victim_at)
            .normalised()
            .unwrap_or(Vec2::new(1.0, 0.0));
        Some((placing, towards))
    }

    pub(super) fn melee_engage_spot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
        range: f32,
    ) -> Engage {
        self.engage(tick, ids, attacker, victim, range)
            .unwrap_or(Engage::Wait)
    }

    pub(super) fn melee_in_reach(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: &crate::combat::Foe,
        victim: &crate::combat::Foe,
    ) -> bool {
        let footprint = self.footprint_of(tick, ids, attacker.entity);
        self.melee_in_reach_from(tick, ids, attacker, footprint, victim)
    }

    pub(super) fn melee_in_reach_from(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: &crate::combat::Foe,
        footprint: Footprint,
        victim: &crate::combat::Foe,
    ) -> bool {
        let striker = Striker {
            entity: attacker.entity,
            at: attacker.position,
            footprint,
        };
        self.melee_in_range(tick, ids, striker, victim.entity, false)
    }

    pub(super) fn melee_stand_free(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> bool {
        let world = tick.state();
        let (Some(here), Some(radius)) = (
            world.get(ids.body, attacker).map(|body| body.0.position),
            world.get(ids.shape, attacker).map(|shape| shape.radius),
        ) else {
            return true;
        };
        let units = self.unit_cells(
            tick,
            ids,
            attacker,
            Some(victim),
            here,
            radius + self.passability.geometry.cell_size(),
        );
        let footprint = self.footprint_of(tick, ids, attacker);
        let heavy = self.has_kind_bit(world, ids, attacker, HEAVY_MELEE_HITTER);
        units.nobody_settled(footprint_cells(self, here, footprint).into_iter(), heavy)
    }

    pub(super) fn melee_approach_spot(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
    ) -> Option<Vec2> {
        let world = tick.state();
        let me = world.get(ids.body, attacker)?.0.position;
        let victim_at = world.get(ids.body, victim)?.0.position;
        let victim_radius = world.get(ids.shape, victim).map_or(0.0, |s| s.radius);
        let dir = me.minus(victim_at).normalised()?;
        let edge = victim_at.plus(dir.scale(victim_radius));
        let wanted = if let Some(goal) = self.siege_tower_wall_goal(world, ids, attacker, victim) {
            goal
        } else {
            let offset = me
                .minus(edge)
                .length()
                .min(self.melee_placement.approach_dist);
            let staged = edge.plus(dir.scale(offset));
            let step = self.ground_height(staged.x, staged.y) - self.ground_height(edge.x, edge.y);
            if step.abs() > APPROACH_MAX_STEP {
                edge
            } else {
                staged
            }
        };
        let (place, _) = self.placing(tick, ids, attacker, victim, 0.0)?;
        let limit = wanted.minus(me).length();
        super::attack::melee_search_offsets(self.melee_placement.approach_cells)
            .iter()
            .map(|&(dx, dy)| place.cell_centre(wanted, dx, dy))
            .find(|&at| {
                at.minus(me).length() <= limit
                    && place
                        .units
                        .melee_spot_free(place.cells(at).into_iter(), place.heavy)
                    && place.ground(at)
            })
    }

    fn engage(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: Entity,
        victim: Entity,
        range: f32,
    ) -> Option<Engage> {
        let (place, towards) = self.placing(tick, ids, attacker, victim, range)?;
        let here = tick.state().get(ids.body, attacker)?.0.position;
        if place.in_reach(here)
            && place.ground(here)
            && place
                .units
                .nobody_settled(place.cells(here).into_iter(), place.heavy)
        {
            return Some(Engage::At(here));
        }
        let edge = place.victim_at.plus(towards.scale(place.victim_radius));
        for offset in [ENGAGE_OFFSET, 0.0] {
            let start = edge.plus(towards.scale(offset));
            if place.ground(start)
                && place
                    .units
                    .destination_clear(place.cells(start).into_iter())
                && place
                    .units
                    .walk_clear_of_enemies(place.from, start, place.radius)
                && place.in_reach(start)
            {
                return Some(Engage::At(start));
            }
            for &(dx, dy) in
                super::attack::melee_search_offsets(self.melee_placement.engagement_cells)
            {
                let at = place.cell_centre(start, dx, dy);
                if place.in_reach(at) && place.free(at) && place.ground(at) {
                    return Some(Engage::At(at));
                }
            }
        }
        Some(Engage::Wait)
    }

    pub(super) fn engage_fallback_point(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        me: &crate::combat::Foe,
        victim: &crate::combat::Foe,
    ) -> Option<Vec2> {
        let world = tick.state();
        let goal = victim.position;
        let delta = goal.minus(me.position);
        let dist0 = delta.length();
        if Self::siege_deployed(world, ids, victim.entity)
            && self.has_kind_bit(world, ids, victim.entity, "SIEGE_TOWER")
        {
            return Some(goal);
        }
        let clear = !self.view_blocked(tick, ids, me.position, goal, victim.radius);
        if clear && self.rally_path_exists(me.position, goal) {
            return Some(goal);
        }
        let toward = delta.normalised()?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((dist0 / WALK_BACK_STEP).trunc() - 1.0).max(0.0) as u32;
        let geometry = self.passability.geometry;
        let extent = self.passability.grid.extent();
        let walls = crate::wall_layer::layer_of(world, ids.wall_layer);
        let mut crossed = false;
        let mut point = goal;
        let mut found = None;
        for _ in 0..steps {
            point = point.minus(toward.scale(WALK_BACK_STEP));
            if self.rally_path_exists(me.position, point) {
                found = Some(point);
                break;
            }
            crossed |=
                walls.is_some_and(|layer| layer.layer_at(geometry, extent, point).is_wall_top());
        }
        let mut point = found?;
        if crossed && !clear {
            point = point.minus(toward.scale(self.castle_siege_stand_back_distance + me.radius));
        } else if goal.minus(point).length() + FALLBACK_GAIN > dist0 {
            return None;
        }
        self.reachable_near(tick, ids, me, point)
    }

    fn reachable_near(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        me: &crate::combat::Foe,
        point: Vec2,
    ) -> Option<Vec2> {
        if self.rally_path_exists(me.position, point) {
            return Some(point);
        }
        let mover = self.mover_class(tick, ids, me.entity);
        let own = self.passable_cell_for(tick, ids, me.position, mover)?;
        let size = self.passability.geometry.cell_size();
        super::attack::melee_search_offsets(self.max_cells_adjust_destination)
            .iter()
            .map(|&(dx, dy)| {
                #[allow(clippy::cast_precision_loss)]
                point.plus(Vec2::new(dx as f32 * size, dy as f32 * size))
            })
            .find_map(|at| {
                let cell = self.passability.cell_at(at)?;
                self.reachable(mover, own, cell)
                    .then(|| self.passability.geometry.centre_of(cell))
            })
    }
}
