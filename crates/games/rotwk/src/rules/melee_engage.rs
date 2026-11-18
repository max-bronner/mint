use mint_movement::{Passability as _, Vec2};
use mint_sim::{Entity, Tick};

use super::unit_cells::{Enemies, UnitCells, HEAVY_MELEE_HITTER};
use super::{Ids, RotwkRules};
use crate::goal_cells::{self, Footprint};

const FALLBACK_GAIN: f32 = 20.0;

pub(crate) const ENGAGE_WAIT_MS: u32 = 10_000;

const CLIMBING_CONDITIONS: [&str; 4] = [
    "CLIMBING",
    "RAPPELLING",
    "SCALING_WALL_HORDE",
    "SCALING_WALL",
];

const CLIMBING_MELEE_REACH_SQUARED: f32 = 5.0;

fn climbing_in_reach(at: Vec2, radius: f32, victim_at: Vec2, victim_radius: f32) -> bool {
    let distance_squared = mint_sage_combat::surface_distance_squared_2d(
        mint_sage_combat::Point3::new(at.x, at.y, 0.0),
        radius,
        mint_sage_combat::Point3::new(victim_at.x, victim_at.y, 0.0),
        victim_radius,
    );
    distance_squared.is_finite() && distance_squared < CLIMBING_MELEE_REACH_SQUARED
}

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
    units: UnitCells,
    footprint: Footprint,
    heavy: bool,
    mover: mint_movement::MoverClass,
    start: Option<mint_movement::Cell>,
    radius: f32,
    from: Vec2,
    victim_at: Vec2,
    victim_radius: f32,
    victim_cells: VictimCells,
    victim_climbing: bool,
    range: f32,
}

#[derive(Debug, Clone, PartialEq)]
enum VictimCells {
    Footprint(Footprint),
    Obstacle(Vec<mint_movement::Cell>),
    Distance,
}

fn melee_reach(
    rules: &RotwkRules,
    (at, radius, footprint): (Vec2, f32, Footprint),
    (victim_at, victim_radius, victim_cells): (Vec2, f32, &VictimCells),
    range: f32,
    victim_climbing: bool,
) -> bool {
    if victim_climbing {
        return climbing_in_reach(at, radius, victim_at, victim_radius);
    }
    let geometry = rules.passability.geometry;
    #[allow(clippy::cast_possible_truncation)]
    let grow = (range / geometry.cell_size() + 0.1)
        .ceil()
        .clamp(0.0, 1000.0) as i32;
    let anchor = goal_cells::anchor(geometry, at, footprint);
    match victim_cells {
        VictimCells::Footprint(theirs) => goal_cells::overlaps_grown(
            anchor,
            footprint,
            goal_cells::anchor(geometry, victim_at, *theirs),
            *theirs,
            grow,
        ),
        VictimCells::Obstacle(cells) => goal_cells::rings_hold(anchor, footprint, grow, cells),
        VictimCells::Distance => {
            crate::combat::in_range(at, radius, victim_at, victim_radius, 0.0, range)
        }
    }
}

fn footprint_cells(rules: &RotwkRules, at: Vec2, footprint: Footprint) -> Vec<(i32, i32)> {
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
        melee_reach(
            self.rules,
            (at, self.radius, self.footprint),
            (self.victim_at, self.victim_radius, &self.victim_cells),
            self.range,
            self.victim_climbing,
        )
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
            victim_cells: self.melee_cells(tick, ids, victim),
            victim_climbing: self.is_climbing_target(tick, victim),
            range,
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

    fn melee_cells(&self, tick: &Tick<'_>, ids: Ids, victim: Entity) -> VictimCells {
        let world = tick.state();
        let structure = world
            .get(ids.shape, victim)
            .is_some_and(|shape| shape.kind_of.contains(mint_sage_asset::KindOf::STRUCTURE));
        if !structure {
            return VictimCells::Footprint(self.footprint_of(tick, ids, victim));
        }
        crate::obstacles::layer_of(world, ids.obstacles)
            .and_then(|layer| {
                layer.cells_of(
                    self.passability.geometry,
                    self.passability.grid.extent(),
                    victim,
                )
            })
            .map_or(VictimCells::Distance, VictimCells::Obstacle)
    }

    pub(super) fn melee_in_reach(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: &crate::combat::Foe,
        victim: &crate::combat::Foe,
        range: f32,
    ) -> bool {
        let footprint = self.footprint_of(tick, ids, attacker.entity);
        self.melee_in_reach_from(tick, ids, attacker, footprint, victim, range)
    }

    pub(super) fn melee_in_reach_from(
        &self,
        tick: &Tick<'_>,
        ids: Ids,
        attacker: &crate::combat::Foe,
        footprint: Footprint,
        victim: &crate::combat::Foe,
        range: f32,
    ) -> bool {
        melee_reach(
            self,
            (attacker.position, attacker.radius, footprint),
            (
                victim.position,
                victim.radius,
                &self.melee_cells(tick, ids, victim.entity),
            ),
            range,
            self.is_climbing_target(tick, victim.entity),
        )
    }

    fn is_climbing_target(&self, tick: &Tick<'_>, entity: Entity) -> bool {
        CLIMBING_CONDITIONS
            .iter()
            .any(|name| self.holds_model_condition(tick, entity, name))
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
        let staged = edge.plus(dir.scale(self.melee_placement.approach_dist));
        let step = self.ground_height(staged.x, staged.y) - self.ground_height(edge.x, edge.y);
        let wanted = if step.abs() > APPROACH_MAX_STEP {
            edge
        } else {
            staged
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
        let mut fewest: Option<(u32, Vec2)> = None;
        for &(dx, dy) in super::attack::melee_search_offsets(self.max_cells_adjust_destination) {
            let at = place.cell_centre(place.victim_at, dx, dy);
            let Some(allies) = place.units.allied_goals(place.cells(at).into_iter()) else {
                continue;
            };
            if fewest.is_some_and(|(held, _)| held <= allies) || !place.ground(at) {
                continue;
            }
            fewest = Some((allies, at));
            if allies == 0 {
                break;
            }
        }
        let gap = |at: Vec2| at.minus(place.victim_at).length();
        Some(match fewest {
            Some((_, at)) if gap(at) + FALLBACK_GAIN <= gap(here) => Engage::At(at),
            _ => Engage::Wait,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_climbing_victim_is_in_melee_reach_by_edge_distance_alone() {
        assert!(climbing_in_reach(
            Vec2::new(0.0, 0.0),
            1.0,
            Vec2::new(4.0, 0.0),
            1.0,
        ));
        assert!(!climbing_in_reach(
            Vec2::new(0.0, 0.0),
            1.0,
            Vec2::new(5.0, 0.0),
            1.0,
        ));
    }
}
