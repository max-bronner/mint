use std::collections::VecDeque;

use mint_sim::{Frame, LogicFrames, StateHasher};

use crate::grid::{PlayerMask, VisibilityGrid};

pub const DEFAULT_UNLOOK_PERSIST: LogicFrames = LogicFrames::from_frames(1);

pub const MAX_PENDING_UNDOS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sighting {
    pub world_x: f32,
    pub world_y: f32,
    pub radius: f32,
    pub shape: VisionShape,
    pub mask: PlayerMask,
}

impl Sighting {
    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.world_x);
        hasher.write_f32(self.world_y);
        hasher.write_f32(self.radius);
        self.shape.hash_into(hasher);
        self.mask.hash_into(hasher);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisionShape {
    facing_x: f32,
    facing_y: f32,
    side: f32,
    rear: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sector {
    Front,
    Side,
    Rear,
}

impl VisionShape {
    pub const ROUND: Self = Self {
        facing_x: 1.0,
        facing_y: 0.0,
        side: 1.0,
        rear: 1.0,
    };

    #[must_use]
    pub fn new(facing_x: f32, facing_y: f32, side: f32, rear: f32) -> Self {
        let fraction = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        let (side, rear) = (fraction(side), fraction(rear));
        if (side >= 1.0 && rear >= 1.0) || !facing_x.is_finite() || !facing_y.is_finite() {
            return Self::ROUND;
        }
        Self {
            facing_x,
            facing_y,
            side,
            rear,
        }
    }

    #[must_use]
    pub fn is_round(&self) -> bool {
        self.side >= 1.0 && self.rear >= 1.0
    }

    #[must_use]
    pub fn fraction(&self, dx: f32, dy: f32) -> f32 {
        self.fraction_of(self.sector(dx, dy))
    }

    pub(crate) fn fraction_of(&self, sector: Sector) -> f32 {
        match sector {
            Sector::Front => 1.0,
            Sector::Side => self.side,
            Sector::Rear => self.rear,
        }
    }

    pub(crate) fn sector(&self, dx: f32, dy: f32) -> Sector {
        let along = dx * self.facing_x + dy * self.facing_y;
        let across = (dx * self.facing_y - dy * self.facing_x).abs();
        if along >= across {
            Sector::Front
        } else if -along > across {
            Sector::Rear
        } else {
            Sector::Side
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.facing_x);
        hasher.write_f32(self.facing_y);
        hasher.write_f32(self.side);
        hasher.write_f32(self.rear);
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UndoQueue {
    pending: VecDeque<(Sighting, Frame)>,
}

impl UndoQueue {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn queue_undo(
        &mut self,
        grid: &mut VisibilityGrid,
        sighting: Sighting,
        now: Frame,
        persist: LogicFrames,
    ) -> bool {
        let forced = if self.pending.len() >= MAX_PENDING_UNDOS {
            if let Some((oldest, _)) = self.pending.pop_front() {
                grid.unreveal_sighting(&oldest);
            }
            true
        } else {
            false
        };
        let wake = now.checked_add(persist).unwrap_or(Frame::FOREVER);
        self.pending.push_back((sighting, wake));
        forced
    }

    pub fn drain_due(&mut self, grid: &mut VisibilityGrid, now: Frame) -> usize {
        let mut applied = 0usize;
        while self
            .pending
            .front()
            .is_some_and(|(_, wake)| wake.get() < now.get())
        {
            let Some((sighting, _)) = self.pending.pop_front() else {
                break;
            };
            grid.unreveal_sighting(&sighting);
            applied = applied.saturating_add(1);
        }
        applied
    }

    pub fn drain_all(&mut self, grid: &mut VisibilityGrid) -> usize {
        self.drain_due(grid, Frame::FOREVER)
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.pending.len());
        for (sighting, wake) in &self.pending {
            sighting.hash_into(hasher);
            wake.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Looker {
    laid: Option<(Sighting, i32, i32)>,
}

impl Looker {
    #[must_use]
    pub const fn new() -> Self {
        Self { laid: None }
    }

    #[must_use]
    pub const fn is_looking(&self) -> bool {
        self.laid.is_some()
    }

    #[must_use]
    pub const fn laid_down(&self) -> Option<&Sighting> {
        match &self.laid {
            Some((sighting, _, _)) => Some(sighting),
            None => None,
        }
    }

    pub fn update(
        &mut self,
        grid: &mut VisibilityGrid,
        queue: &mut UndoQueue,
        now: Frame,
        persist: LogicFrames,
        sighting: Sighting,
    ) -> bool {
        let (cell_x, cell_y) = grid.world_to_cell(sighting.world_x, sighting.world_y);
        if let Some((old, old_x, old_y)) = self.laid {
            let unchanged = old_x == cell_x
                && old_y == cell_y
                && old.radius.to_bits() == sighting.radius.to_bits()
                && old.shape == sighting.shape
                && old.mask == sighting.mask;
            if unchanged {
                return false;
            }
            grid.reveal_sighting(&sighting);
            queue.queue_undo(grid, old, now, persist);
        } else {
            grid.reveal_sighting(&sighting);
        }
        self.laid = Some((sighting, cell_x, cell_y));
        true
    }

    pub fn stop_looking(
        &mut self,
        queue: &mut UndoQueue,
        grid: &mut VisibilityGrid,
        now: Frame,
        persist: LogicFrames,
    ) {
        if let Some((old, _, _)) = self.laid.take() {
            queue.queue_undo(grid, old, now, persist);
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.laid {
            Some((sighting, x, y)) => {
                hasher.write_bool(true);
                sighting.hash_into(hasher);
                hasher.write_i32(*x);
                hasher.write_i32(*y);
            }
            None => hasher.write_bool(false),
        }
    }
}

pub const MAX_WATCHERS: usize = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReconcileReport {
    pub added: usize,
    pub relaid: usize,
    pub retired: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LookerSet {
    watchers: Vec<(u64, Looker)>,
    scratch: Vec<(u64, Looker)>,
}

impl LookerSet {
    #[must_use]
    pub fn new() -> Self {
        Self {
            watchers: Vec::new(),
            scratch: Vec::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.watchers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.watchers.is_empty()
    }

    pub fn reconcile(
        &mut self,
        grid: &mut VisibilityGrid,
        queue: &mut UndoQueue,
        now: Frame,
        persist: LogicFrames,
        desired: &[(u64, Sighting)],
    ) -> ReconcileReport {
        let mut report = ReconcileReport::default();
        let mut next = core::mem::take(&mut self.scratch);
        next.clear();
        next.reserve(desired.len().min(MAX_WATCHERS));
        let mut old = self.watchers.iter().copied().peekable();
        let mut input_was_sorted = true;
        let mut previous_id: Option<u64> = None;

        for &(id, sighting) in desired.iter().take(MAX_WATCHERS) {
            if previous_id.is_some_and(|last| last >= id) {
                input_was_sorted = false;
            }
            previous_id = Some(id);

            while old.peek().is_some_and(|(old_id, _)| *old_id < id) {
                if let Some((_, mut looker)) = old.next() {
                    looker.stop_looking(queue, grid, now, persist);
                    report.retired = report.retired.saturating_add(1);
                }
            }
            let (mut looker, existed) = match old.peek() {
                Some((old_id, looker)) if *old_id == id => {
                    let carried = *looker;
                    let _ = old.next();
                    (carried, true)
                }
                _ => (Looker::new(), false),
            };
            let changed = looker.update(grid, queue, now, persist, sighting);
            if !existed {
                report.added = report.added.saturating_add(1);
            } else if changed {
                report.relaid = report.relaid.saturating_add(1);
            } else {
                report.unchanged = report.unchanged.saturating_add(1);
            }
            next.push((id, looker));
        }

        for (_, mut looker) in old {
            looker.stop_looking(queue, grid, now, persist);
            report.retired = report.retired.saturating_add(1);
        }

        if !input_was_sorted {
            next.sort_unstable_by_key(|(id, _)| *id);
            debug_assert!(
                next.windows(2)
                    .all(|pair| matches!(pair, [(a, _), (b, _)] if a != b)),
                "duplicate looker id"
            );
        }
        self.scratch = core::mem::replace(&mut self.watchers, next);
        report
    }

    pub fn clear(
        &mut self,
        grid: &mut VisibilityGrid,
        queue: &mut UndoQueue,
        now: Frame,
        persist: LogicFrames,
    ) {
        for (_, looker) in &mut self.watchers {
            looker.stop_looking(queue, grid, now, persist);
        }
        self.watchers.clear();
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.watchers.len());
        for (id, looker) in &self.watchers {
            hasher.write_u64(*id);
            looker.hash_into(hasher);
        }
    }
}

#[cfg(test)]
mod tests;
