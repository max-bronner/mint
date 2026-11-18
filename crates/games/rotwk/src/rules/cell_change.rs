use mint_sim::{Component, Entity, Result, StateHasher, Tick};

use super::{Ids, KindOf, RotwkRules};
use crate::goal_cells::{self, CellRect};

const FIRST_FRAME: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellMark((i32, i32));

impl Component for CellMark {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_i32(self.0 .0);
        hasher.write_i32(self.0 .1);
    }
}

#[derive(Debug, Clone, Copy)]
struct Beside {
    entity: Entity,
    overlapping: bool,
    horde: Option<Entity>,
}

impl RotwkRules {
    pub(super) fn run_cell_changes(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let fps = u64::from(crate::game_data::RETAIL_LOGIC_HZ);
        let ticks = (u64::from(mint_sim::SIMULATION_HZ) / fps).max(1);
        if tick.frame().get() % ticks != 0 {
            return Ok(());
        }
        let changed = self.mark_cells(tick, ids)?;
        if tick.frame().get() / ticks < FIRST_FRAME {
            return Ok(());
        }
        for entity in changed {
            self.notify_cell_change(tick, ids, entity, true, false)?;
        }
        Ok(())
    }

    fn mark_cells(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<Vec<Entity>> {
        let geometry = self.passability.geometry;
        let mut changed = Vec::new();
        let entities: Vec<Entity> = tick
            .state()
            .store(ids.body)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        for entity in entities {
            if !self.searches_beside(tick, ids, entity) {
                continue;
            }
            let Some(at) = tick
                .state()
                .get(ids.body, entity)
                .map(|body| body.0.position)
            else {
                continue;
            };
            let cell = goal_cells::anchor(geometry, at, self.footprint_of(tick, ids, entity));
            if tick.state().get(ids.cell_mark, entity) != Some(&CellMark(cell)) {
                tick.state_mut()
                    .insert(ids.cell_mark, entity, CellMark(cell))?;
                changed.push(entity);
            }
        }
        Ok(changed)
    }

    fn searches_beside(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        !(world.contains(ids.member, entity)
            || world.contains(ids.horde, entity)
            || world.contains(ids.contained, entity)
            || world.contains(ids.flight, entity)
            || Self::gone(tick, ids, entity)
            || Self::is_immobile(tick, ids, entity))
            && self.mover_limits(tick, ids, entity).max_speed > 0.0
            && self.moves_on_ground(tick, ids, entity)
    }

    fn gone(tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        world.contains(ids.doomed, entity)
            || world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead())
    }

    fn notify_cell_change(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        force: bool,
        nested: bool,
    ) -> Result<()> {
        let world = tick.state();
        if Self::gone(tick, ids, entity)
            || world.contains(ids.knocked, entity)
            || (!force && self.melee_attacking(world, ids, entity))
            || !self.can_stop_here(tick, ids, entity)
        {
            return Ok(());
        }
        if let Some(victim) = self.adjacent_victim(tick, ids, entity, nested)? {
            self.melee_attack_adjacent_victim(tick, ids, entity, victim)?;
        }
        Ok(())
    }

    fn can_stop_here(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> bool {
        let world = tick.state();
        let (Some(here), Some(radius)) = (
            world.get(ids.body, entity).map(|body| body.0.position),
            world.get(ids.shape, entity).map(|shape| shape.radius),
        ) else {
            return false;
        };
        let Some(rect) = self.cell_rect(tick, ids, entity) else {
            return false;
        };
        let units = self.unit_cells(
            tick,
            ids,
            entity,
            None,
            here,
            radius + self.passability.geometry.cell_size(),
        );
        let heavy = self.has_kind_bit(world, ids, entity, super::unit_cells::HEAVY_MELEE_HITTER);
        let (x0, y0, x1, y1) = rect.span();
        units.nobody_settled((y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))), heavy)
    }

    fn cell_rect(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Option<CellRect> {
        let at = tick.state().get(ids.body, entity)?.0.position;
        let footprint = self.footprint_of(tick, ids, entity);
        goal_cells::rect(
            self.passability.grid.extent(),
            goal_cells::anchor(self.passability.geometry, at, footprint),
            footprint,
        )
    }

    fn in_melee_cells(&self, tick: &Tick<'_>, ids: Ids, entity: Entity, other: Entity) -> bool {
        let Some(at) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return false;
        };
        let striker = super::melee_contact::Striker {
            entity,
            at,
            footprint: self.footprint_of(tick, ids, entity),
        };
        self.melee_contact(tick, ids, striker, other)
    }

    fn units_beside(&self, tick: &Tick<'_>, ids: Ids, entity: Entity) -> Vec<Beside> {
        let world = tick.state();
        let Some(mine) = self.cell_rect(tick, ids, entity) else {
            return Vec::new();
        };
        let Some(store) = world.store(ids.body) else {
            return Vec::new();
        };
        let mut overlap = Vec::new();
        let mut ring = Vec::new();
        for other in store.entities().iter().copied() {
            if other == entity
                || world.contains(ids.horde, other)
                || world.contains(ids.contained, other)
                || world.contains(ids.flight, other)
                || Self::is_immobile(tick, ids, other)
                || !self.moves_on_ground(tick, ids, other)
            {
                continue;
            }
            let Some(theirs) = self.cell_rect(tick, ids, other) else {
                continue;
            };
            let beside = Beside {
                entity: other,
                overlapping: mine.overlaps(theirs),
                horde: world.get(ids.member, other).map(|member| member.horde),
            };
            if beside.overlapping {
                overlap.push(beside);
            } else if mine.touches(theirs) {
                ring.push(beside);
            }
        }
        overlap.extend(ring);
        overlap
    }

    fn adjacent_victim(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        nested: bool,
    ) -> Result<Option<Entity>> {
        let world = tick.state();
        let Some(_) = world.get(ids.owner, entity).map(|owner| owner.0) else {
            return Ok(None);
        };
        let current = world
            .get(ids.attack_order, entity)
            .map(|order| order.target)
            .or_else(|| world.get(ids.loose_melee, entity).map(|plan| plan.victim()))
            .or_else(|| world.get(ids.armed, entity).and_then(|armed| armed.target));
        let current_horde =
            current.and_then(|victim| world.get(ids.member, victim).map(|m| m.horde));
        let enemy = |tick: &Tick<'_>, other: Entity| {
            let world = tick.state();
            !Self::gone(tick, ids, other)
                && !world.contains(ids.knocked, other)
                && self.object_hostile(world, entity, other)
        };
        let only_if_current = |tick: &Tick<'_>, other: Entity| {
            Some(other) != current
                && tick.state().get(ids.shape, other).is_some_and(|shape| {
                    shape.kind_of.contains(KindOf::STRUCTURE)
                        || shape.kind_of.contains(KindOf::NOT_AUTOACQUIRABLE)
                })
        };
        let mut pick: Option<Entity> = None;
        for beside in self.units_beside(tick, ids, entity) {
            let other = beside.entity;
            if beside.overlapping {
                if !enemy(tick, other) || only_if_current(tick, other) {
                    continue;
                }
                if pick.is_none() || pick != current {
                    pick = Some(other);
                }
                continue;
            }
            if current_horde.is_some() && beside.horde == current_horde {
                pick = Some(other);
            }
            if !enemy(tick, other)
                || !self.in_melee_cells(tick, ids, entity, other)
                || (pick.is_some() && pick == current)
                || only_if_current(tick, other)
            {
                continue;
            }
            pick = Some(other);
            if beside.horde.is_none() && !nested {
                self.notify_cell_change(tick, ids, other, false, true)?;
            }
        }
        Ok(pick.filter(|&victim| self.in_melee_cells(tick, ids, entity, victim)))
    }
}
