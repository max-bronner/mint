use mint_movement::Vec2;
use mint_sim::{Component, ComponentId, ComponentStore, Entity, World, PAGE_SLOTS};

use super::unit_cells::GoalHolder;
use super::{Ids, KindOf, RotwkRules};
use crate::goal_cells::{self, CellRect, Footprint};
use crate::unit::{Body, Shape};

const BUCKET: f32 = 40.0;
const NONE: u32 = u32::MAX;
const OVERFLOW: u32 = u32::MAX - 1;

#[derive(Debug, Clone, Copy)]
pub(super) struct BodyRecord {
    pub(super) entity: Entity,
    pub(super) position: Vec2,
    pub(super) radius: f32,
    pub(super) left_out: bool,
    pub(super) footprint: Footprint,
    pub(super) body_rect: Option<CellRect>,
    pub(super) member: Option<(Entity, Option<u16>)>,
    pub(super) holder: GoalHolder,
}

impl BodyRecord {
    pub(super) fn read(
        rules: &RotwkRules,
        world: &World,
        ids: Ids,
        entity: Entity,
        shape: &Shape,
        body: &Body,
    ) -> Self {
        let size = rules.passability.geometry.cell_size();
        let entry = world
            .get(ids.kind, entity)
            .and_then(|kind| rules.kinds.get(*kind));
        let footprint = Footprint::of(
            shape.radius,
            entry.is_some_and(|entry| entry.facts.large_footprint),
            size,
            entry.and_then(|entry| entry.record.pathfind_diameter),
        );
        let filed = if shape.kind_of.contains(KindOf::PROJECTILE)
            || shape.kind_of.contains(KindOf::INERT)
        {
            world
                .get(ids.own_cell_entry, entity)
                .map_or(body.0.position, |entry| entry.0)
        } else {
            body.0.position
        };
        Self {
            entity,
            position: filed,
            radius: shape.radius,
            left_out: world.contains(ids.horde, entity)
                || shape.kind_of.contains(KindOf::IMMOBILE)
                || world.contains(ids.flight, entity)
                || world.contains(ids.doomed, entity)
                || world.contains(ids.concealed, entity),
            footprint,
            body_rect: square(rules, filed, footprint),
            member: world
                .get(ids.member, entity)
                .map(|member| (member.horde, member.slot)),
            holder: RotwkRules::holder_from(world, ids, entity, entry, false),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    record: Option<BodyRecord>,
    at: u32,
    home: u32,
    in_overflow: bool,
}

impl Slot {
    const EMPTY: Self = Self {
        record: None,
        at: NONE,
        home: NONE,
        in_overflow: false,
    };
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    slot: u32,
    position: Vec2,
}

const WALKS_BEFORE_BUILDING: u8 = if cfg!(debug_assertions) { 0 } else { 2 };

const WATCHED: usize = 9;

#[derive(Debug, Default)]
pub(super) struct UnitIndex {
    active: bool,
    built: bool,
    walked: u8,
    seen: [u64; WATCHED],
    slots: Vec<Slot>,
    origin: Vec2,
    width: u32,
    height: u32,
    starts: Vec<u32>,
    entries: Vec<Entry>,
    overflow: Vec<u32>,
    records: usize,
    kept: Vec<u64>,
}

fn writes<T: Component>(world: &World, id: ComponentId<T>) -> u64 {
    world.store(id).map_or(0, ComponentStore::writes)
}

fn pages<T: Component>(world: &World, id: ComponentId<T>, since: u64, into: &mut Vec<u32>) {
    if let Some(store) = world.store(id) {
        into.extend(store.pages_written_since(since));
    }
}

impl UnitIndex {
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
        ]
    }

    pub(super) fn sync(&mut self, rules: &RotwkRules, world: &World, ids: Ids) -> bool {
        if !self.active {
            return false;
        }
        let (Some(shapes), Some(bodies)) = (world.store(ids.shape), world.store(ids.body)) else {
            return false;
        };
        let now = Self::watched(world, ids);
        if !self.built {
            if WALKS_BEFORE_BUILDING.saturating_sub(self.walked) > 0 {
                self.walked = self.walked.saturating_add(1);
                return false;
            }
            self.build(rules, world, ids, shapes, bodies);
            self.seen = now;
            return true;
        }
        if now == self.seen {
            return true;
        }
        let seen = self.seen;
        let mut full = Vec::new();
        let [_, shape, horde, flight, doomed, concealed, member, kind, owner] = seen;
        pages(world, ids.shape, shape, &mut full);
        pages(world, ids.horde, horde, &mut full);
        pages(world, ids.flight, flight, &mut full);
        pages(world, ids.doomed, doomed, &mut full);
        pages(world, ids.concealed, concealed, &mut full);
        pages(world, ids.member, member, &mut full);
        pages(world, ids.kind, kind, &mut full);
        pages(world, ids.owner, owner, &mut full);
        full.sort_unstable();
        full.dedup();
        let mut moved = Vec::new();
        pages(world, ids.body, seen[0], &mut moved);
        for first in &full {
            self.refresh(rules, world, ids, shapes, bodies, *first, true);
        }
        for first in moved {
            if full.binary_search(&first).is_err() {
                self.refresh(rules, world, ids, shapes, bodies, first, false);
            }
        }
        self.seen = now;
        if self.overflow.len() > (self.records / 8).max(64) {
            self.rebucket();
        }
        true
    }

    fn build(
        &mut self,
        rules: &RotwkRules,
        world: &World,
        ids: Ids,
        shapes: &ComponentStore<Shape>,
        bodies: &ComponentStore<Body>,
    ) {
        let geometry = rules.passability.geometry;
        let extent = rules.passability.grid.extent();
        let size = geometry.cell_size();
        let span = |cells: u16| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let buckets = (f32::from(cells) * size / BUCKET)
                .ceil()
                .clamp(1.0, 65_536.0) as u32;
            buckets
        };
        self.origin = geometry.origin();
        self.width = span(extent.width);
        self.height = span(extent.height);
        self.slots.clear();
        let last = shapes.entities().last().map_or(0, |e| e.index());
        self.slots.resize(
            usize::try_from(last).map_or(0, |l| l.saturating_add(1)),
            Slot::EMPTY,
        );
        self.records = 0;
        for (entity, shape, body) in mint_sim::join(shapes, bodies) {
            let record = BodyRecord::read(rules, world, ids, entity, shape, body);
            if let Some(slot) = self.slots.get_mut(slot_of(entity)) {
                slot.record = Some(record);
                self.records = self.records.saturating_add(1);
            }
        }
        self.rebucket();
        self.built = true;
    }

    fn bucket_of(&self, at: Vec2) -> Option<u32> {
        if !(at.x.is_finite() && at.y.is_finite()) {
            return None;
        }
        let (x, y) = self.cell(at);
        Some(y.saturating_mul(self.width).saturating_add(x))
    }

    fn cell(&self, at: Vec2) -> (u32, u32) {
        #[allow(clippy::cast_precision_loss)]
        let along = |v: f32, o: f32, n: u32| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let cell = ((v - o) / BUCKET)
                .floor()
                .clamp(0.0, n.saturating_sub(1) as f32) as u32;
            cell
        };
        (
            along(at.x, self.origin.x, self.width),
            along(at.y, self.origin.y, self.height),
        )
    }

    fn target(&self, record: Option<&BodyRecord>) -> u32 {
        match record {
            Some(record) if !record.left_out => self.bucket_of(record.position).unwrap_or(OVERFLOW),
            _ => NONE,
        }
    }

    fn rebucket(&mut self) {
        let buckets = usize::try_from(self.width.saturating_mul(self.height)).unwrap_or(0);
        self.starts.clear();
        self.starts.resize(buckets.saturating_add(1), 0);
        self.overflow.clear();
        let targets: Vec<u32> = self
            .slots
            .iter()
            .map(|slot| self.target(slot.record.as_ref()))
            .collect();
        for target in &targets {
            if let Some(count) = usize::try_from(*target)
                .ok()
                .and_then(|b| self.starts.get_mut(b.saturating_add(1)))
            {
                *count = count.saturating_add(1);
            }
        }
        let mut running = 0u32;
        for start in &mut self.starts {
            running = running.saturating_add(*start);
            *start = running;
        }
        let mut next: Vec<u32> = self.starts.clone();
        self.entries.clear();
        self.entries.resize(
            usize::try_from(running).unwrap_or(0),
            Entry {
                slot: NONE,
                position: Vec2::ZERO,
            },
        );
        for (index, (slot, target)) in self.slots.iter_mut().zip(targets).enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            *slot = Slot {
                record: slot.record,
                ..Slot::EMPTY
            };
            if target == OVERFLOW {
                slot.in_overflow = true;
                self.overflow.push(index);
            } else if let (Some(at), Some(record)) = (
                usize::try_from(target).ok().and_then(|b| next.get_mut(b)),
                slot.record,
            ) {
                if let Some(entry) = usize::try_from(*at)
                    .ok()
                    .and_then(|e| self.entries.get_mut(e))
                {
                    *entry = Entry {
                        slot: index,
                        position: record.position,
                    };
                    slot.at = *at;
                    slot.home = target;
                }
                *at = at.saturating_add(1);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn refresh(
        &mut self,
        rules: &RotwkRules,
        world: &World,
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
                self.slots.resize(need, Slot::EMPTY);
            }
        }
        let mut shaped = shaped.iter().peekable();
        for index in first..end {
            let at = usize::try_from(index).unwrap_or(usize::MAX);
            if at >= self.slots.len() {
                break;
            }
            let entity = shaped.next_if(|e| e.index() == index).copied();
            let old = self.slots.get(at).and_then(|slot| slot.record);
            let live = entity.and_then(|entity| {
                let body = bodies.get(entity)?;
                Some(match old {
                    Some(old) if !full && old.entity == entity => BodyRecord {
                        position: body.0.position,
                        body_rect: square(rules, body.0.position, old.footprint),
                        ..old
                    },
                    _ => {
                        let shape = shapes.get(entity)?;
                        BodyRecord::read(rules, world, ids, entity, shape, body)
                    }
                })
            });
            self.place(at, live);
        }
    }

    fn place(&mut self, at: usize, record: Option<BodyRecord>) {
        let target = self.target(record.as_ref());
        let Some(slot) = self.slots.get_mut(at) else {
            return;
        };
        match (slot.record.is_some(), record.is_some()) {
            (false, true) => self.records = self.records.saturating_add(1),
            (true, false) => self.records = self.records.saturating_sub(1),
            _ => {}
        }
        slot.record = record;
        if slot.in_overflow {
            return;
        }
        let entry = usize::try_from(slot.at)
            .ok()
            .and_then(|e| self.entries.get_mut(e));
        if let Some(entry) = entry {
            if target == slot.home {
                if let Some(record) = record {
                    entry.position = record.position;
                }
                return;
            }
            entry.slot = NONE;
            slot.at = NONE;
            slot.home = NONE;
        }
        if target != NONE {
            slot.in_overflow = true;
            self.overflow.push(u32::try_from(at).unwrap_or(u32::MAX));
        }
    }

    pub(super) fn query(&mut self, centre: Vec2, reach_sq: f32) -> Option<Vec<&BodyRecord>> {
        if !(self.built && centre.x.is_finite() && centre.y.is_finite() && reach_sq.is_finite()) {
            return None;
        }
        let half = reach_sq.max(0.0).sqrt() * (1.0 + 1.0e-4) + 1.0;
        let low = self.cell(Vec2::new(centre.x - half, centre.y - half));
        let high = self.cell(Vec2::new(centre.x + half, centre.y + half));
        let start = |b: u32| {
            usize::try_from(b)
                .ok()
                .and_then(|b| self.starts.get(b))
                .and_then(|s| usize::try_from(*s).ok())
                .unwrap_or(0)
        };
        let within = |at: Vec2| kept_by(at, centre, reach_sq);
        let mut kept = core::mem::take(&mut self.kept);
        kept.clear();
        kept.resize(self.slots.len().div_ceil(64), 0);
        let mut keep = |slot: u32| {
            if let Some(word) = usize::try_from(slot >> 6)
                .ok()
                .and_then(|w| kept.get_mut(w))
            {
                *word |= 1u64 << (slot & 63);
            }
        };
        for y in low.1..=high.1 {
            let row = y.saturating_mul(self.width);
            let first = start(row.saturating_add(low.0));
            let last = start(row.saturating_add(high.0).saturating_add(1));
            for entry in self.entries.get(first..last).unwrap_or(&[]) {
                if entry.slot != NONE && within(entry.position) {
                    keep(entry.slot);
                }
            }
        }
        for index in &self.overflow {
            let record = usize::try_from(*index)
                .ok()
                .and_then(|at| self.slots.get(at))
                .and_then(|slot| slot.record);
            if record.is_some_and(|record| !record.left_out && within(record.position)) {
                keep(*index);
            }
        }
        let mut found = Vec::new();
        for (word_index, word) in kept.iter().enumerate() {
            let mut rest = *word;
            while rest != 0 {
                let at = word_index * 64 + rest.trailing_zeros() as usize;
                if let Some(record) = self.slots.get(at).and_then(|slot| slot.record.as_ref()) {
                    found.push(record);
                }
                rest &= rest - 1;
            }
        }
        self.kept = kept;
        Some(found)
    }

    #[cfg(test)]
    pub(super) fn records(&self) -> Vec<BodyRecord> {
        self.slots.iter().filter_map(|slot| slot.record).collect()
    }
}

pub(super) fn square(rules: &RotwkRules, at: Vec2, footprint: Footprint) -> Option<CellRect> {
    let geometry = rules.passability.geometry;
    goal_cells::rect(
        rules.passability.grid.extent(),
        goal_cells::anchor(geometry, at, footprint),
        footprint,
    )
}

fn slot_of(entity: Entity) -> usize {
    usize::try_from(entity.index()).unwrap_or(usize::MAX)
}

pub(super) fn kept_by(at: Vec2, centre: Vec2, reach_sq: f32) -> bool {
    at.minus(centre).length_squared().partial_cmp(&reach_sq) != Some(core::cmp::Ordering::Greater)
}
