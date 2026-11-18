use mint_sim::{Component, ComponentId, ComponentStore, Entity, PlayerId, World, PAGE_SLOTS};

use super::unit_index::square;
use super::{Ids, KindOf, RotwkRules};
use crate::goal_cells::{CellRect, Footprint};
use crate::unit::{Body, Shape};

const WALKS_BEFORE_BUILDING: u8 = if cfg!(debug_assertions) { 0 } else { 1 };

const WATCHED: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct MoverRow {
    pub(super) entity: Entity,
    pub(super) owner: Option<PlayerId>,
    pub(super) rect: Option<CellRect>,
    pub(super) horde: Option<Entity>,
    pub(super) crushable: Option<i16>,
    pub(super) each_other: bool,
    pub(super) infantry: bool,
    pub(super) large: bool,
    pub(super) listed: bool,
    volatile: bool,
    footprint: Footprint,
}

impl MoverRow {
    fn read(
        rules: &RotwkRules,
        tick: &mint_sim::Tick<'_>,
        ids: Ids,
        entity: Entity,
        shape: &Shape,
        body: &Body,
    ) -> Self {
        let world = tick.state();
        let entry = world
            .get(ids.kind, entity)
            .and_then(|kind| rules.kinds.get(*kind));
        let facts = entry.map(|entry| entry.facts).unwrap_or_default();
        let footprint = rules.footprint_of(tick, ids, entity);
        let left_out = world.contains(ids.horde, entity)
            || shape.kind_of.contains(KindOf::IMMOBILE)
            || world.contains(ids.flight, entity)
            || world.contains(ids.doomed, entity)
            || world.contains(ids.concealed, entity)
            || world
                .get(ids.health, entity)
                .is_some_and(|health| health.0.is_dead());
        let kinematics = world
            .get(ids.locomotion, entity)
            .map(|locomotion| locomotion.0)
            .or_else(|| entry.and_then(|entry| entry.limits));
        let volatile = !left_out
            && (kinematics.is_some_and(|k| k.charge_available || k.crew_powered)
                || world.get(ids.power_book, entity).is_some_and(|book| {
                    book.0.iter().any(|instance| {
                        rules
                            .power_cast_rules
                            .get(usize::from(instance.power.get()))
                            .is_some_and(|rule| rule.charge_attack)
                    })
                })
                || entry
                    .and_then(|entry| entry.flight)
                    .is_some_and(|flight| flight.river_modifier != 1.0));
        Self {
            entity,
            owner: world.get(ids.owner, entity).map(|owner| owner.0),
            rect: square(rules, body.0.position, footprint),
            horde: world.get(ids.member, entity).map(|member| member.horde),
            crushable: entry.map(|entry| i16::from(entry.record.crush.crushable_level)),
            each_other: facts.path_through_each_other,
            infantry: facts.infantry,
            large: facts.large_rectangle_pathfind,
            listed: !left_out && Self::moves(rules, tick, ids, entity),
            volatile,
            footprint,
        }
    }

    fn moves(rules: &RotwkRules, tick: &mint_sim::Tick<'_>, ids: Ids, entity: Entity) -> bool {
        !matches!(
            rules
                .mover_limits(tick, ids, entity)
                .max_speed
                .partial_cmp(&0.0),
            Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
        )
    }
}

#[derive(Debug, Default)]
pub(super) struct MoverRows {
    active: bool,
    built: bool,
    walked: u8,
    seen: [u64; WATCHED],
    slots: Vec<Option<MoverRow>>,
    volatile: Vec<u32>,
}

fn writes<T: Component>(world: &World, id: ComponentId<T>) -> u64 {
    world.store(id).map_or(0, ComponentStore::writes)
}

fn pages<T: Component>(world: &World, id: ComponentId<T>, since: u64, into: &mut Vec<u32>) {
    if let Some(store) = world.store(id) {
        into.extend(store.pages_written_since(since));
    }
}

fn slot_of(entity: Entity) -> usize {
    usize::try_from(entity.index()).unwrap_or(usize::MAX)
}

impl MoverRows {
    pub(super) fn begin(&mut self) {
        self.active = true;
        self.built = false;
        self.walked = 0;
    }

    pub(super) fn end(&mut self) {
        self.active = false;
        self.built = false;
    }

    fn watched(world: &World, ids: Ids) -> [u64; WATCHED] {
        [
            writes(world, ids.body),
            writes(world, ids.shape),
            writes(world, ids.horde),
            writes(world, ids.flight),
            writes(world, ids.doomed),
            writes(world, ids.concealed),
            writes(world, ids.member),
            writes(world, ids.kind),
            writes(world, ids.owner),
            writes(world, ids.health),
            writes(world, ids.locomotion),
            writes(world, ids.power_book),
            writes(world, ids.bonuses),
            writes(world, ids.formation_speed),
        ]
    }

    pub(super) fn sync(&mut self, rules: &RotwkRules, tick: &mint_sim::Tick<'_>, ids: Ids) -> bool {
        if !self.active {
            return false;
        }
        let world = tick.state();
        let (Some(shapes), Some(bodies)) = (world.store(ids.shape), world.store(ids.body)) else {
            return false;
        };
        let now = Self::watched(world, ids);
        if !self.built {
            if WALKS_BEFORE_BUILDING.saturating_sub(self.walked) > 0 {
                self.walked = self.walked.saturating_add(1);
                return false;
            }
            self.build(rules, tick, ids, shapes, bodies);
            self.seen = now;
            return true;
        }
        if now != self.seen {
            let seen = self.seen;
            let mut full = Vec::new();
            let [_, shape, horde, flight, doomed, concealed, member, kind, owner, health, loco, book, bonuses, cap] =
                seen;
            pages(world, ids.shape, shape, &mut full);
            pages(world, ids.horde, horde, &mut full);
            pages(world, ids.flight, flight, &mut full);
            pages(world, ids.doomed, doomed, &mut full);
            pages(world, ids.concealed, concealed, &mut full);
            pages(world, ids.member, member, &mut full);
            pages(world, ids.kind, kind, &mut full);
            pages(world, ids.owner, owner, &mut full);
            pages(world, ids.health, health, &mut full);
            pages(world, ids.locomotion, loco, &mut full);
            pages(world, ids.power_book, book, &mut full);
            pages(world, ids.bonuses, bonuses, &mut full);
            pages(world, ids.formation_speed, cap, &mut full);
            full.sort_unstable();
            full.dedup();
            let mut moved = Vec::new();
            pages(world, ids.body, seen[0], &mut moved);
            for first in &full {
                self.refresh(rules, tick, ids, shapes, bodies, *first, true);
            }
            for first in moved {
                if full.binary_search(&first).is_err() {
                    self.refresh(rules, tick, ids, shapes, bodies, first, false);
                }
            }
            self.seen = now;
            self.collect_volatile();
        }
        for at in &self.volatile {
            let Some(row) = usize::try_from(*at)
                .ok()
                .and_then(|at| self.slots.get_mut(at))
                .and_then(Option::as_mut)
            else {
                continue;
            };
            row.listed = MoverRow::moves(rules, tick, ids, row.entity);
        }
        true
    }

    fn build(
        &mut self,
        rules: &RotwkRules,
        tick: &mint_sim::Tick<'_>,
        ids: Ids,
        shapes: &ComponentStore<Shape>,
        bodies: &ComponentStore<Body>,
    ) {
        self.slots.clear();
        let last = shapes.entities().last().map_or(0, |e| e.index());
        self.slots.resize(
            usize::try_from(last).map_or(0, |l| l.saturating_add(1)),
            None,
        );
        for (entity, shape, body) in mint_sim::join(shapes, bodies) {
            let row = MoverRow::read(rules, tick, ids, entity, shape, body);
            if let Some(slot) = self.slots.get_mut(slot_of(entity)) {
                *slot = Some(row);
            }
        }
        self.collect_volatile();
        self.built = true;
    }

    fn collect_volatile(&mut self) {
        self.volatile.clear();
        for (at, row) in self.slots.iter().enumerate() {
            if row.is_some_and(|row| row.volatile) {
                self.volatile.push(u32::try_from(at).unwrap_or(u32::MAX));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refresh(
        &mut self,
        rules: &RotwkRules,
        tick: &mint_sim::Tick<'_>,
        ids: Ids,
        shapes: &ComponentStore<Shape>,
        bodies: &ComponentStore<Body>,
        first: u32,
        full: bool,
    ) {
        let end = first.saturating_add(PAGE_SLOTS);
        let entities = shapes.entities();
        let from = entities.partition_point(|e| e.index() < first);
        let to = entities.partition_point(|e| e.index() < end);
        let shaped = entities.get(from..to).unwrap_or(&[]);
        if let Some(last) = shaped.last() {
            let need = slot_of(*last).saturating_add(1);
            if need > self.slots.len() {
                self.slots.resize(need, None);
            }
        }
        let mut shaped = shaped.iter().peekable();
        for index in first..end {
            let at = usize::try_from(index).unwrap_or(usize::MAX);
            if at >= self.slots.len() {
                break;
            }
            let entity = shaped.next_if(|e| e.index() == index).copied();
            let old = self.slots.get(at).copied().flatten();
            let live = entity.and_then(|entity| {
                let body = bodies.get(entity)?;
                Some(match old {
                    Some(old) if !full && old.entity == entity => MoverRow {
                        rect: square(rules, body.0.position, old.footprint),
                        ..old
                    },
                    _ => {
                        let shape = shapes.get(entity)?;
                        MoverRow::read(rules, tick, ids, entity, shape, body)
                    }
                })
            });
            if let Some(slot) = self.slots.get_mut(at) {
                *slot = live;
            }
        }
    }

    pub(super) fn listed(&self) -> impl Iterator<Item = &MoverRow> {
        self.slots
            .iter()
            .filter_map(Option::as_ref)
            .filter(|row| row.listed)
    }
}
