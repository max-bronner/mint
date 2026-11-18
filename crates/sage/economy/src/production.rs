use mint_sim::{LogicFrames, StateHasher};

use crate::command_points::CommandPointPool;
use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemId(u32);

impl ItemId {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProductionId(u32);

impl ProductionId {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

pub const MAX_QUEUE_ENTRIES_CAP: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    #[error("the production queue is full at {limit} entries")]
    QueueFull { limit: u32 },
    #[error("costs {cost} and the player holds {held}")]
    CannotAfford { cost: i64, held: i64 },
    #[error("needs {needed} command points and {free} are free of {limit}")]
    NoCommandPoints { needed: i32, free: i32, limit: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    Unit { builder: bool },
    Upgrade,
    Revival { builder: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueueRequest {
    pub item: ItemId,
    pub cost: i64,
    pub build_time: LogicFrames,
    pub quantity: u32,
    pub command_points: i32,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProductionEntry {
    pub item: ItemId,
    pub id: ProductionId,
    pub frames_done: u32,
    pub frames_total: u32,
    pub quantity_total: u32,
    pub quantity_produced: u32,
    pub paid: i64,
    pub command_points: i32,
    pub kind: EntryKind,
}

impl ProductionEntry {
    #[must_use]
    pub const fn quantity_remaining(&self) -> u32 {
        self.quantity_total.saturating_sub(self.quantity_produced)
    }

    #[must_use]
    pub fn fraction(&self) -> f32 {
        if self.frames_total == 0 {
            return 1.0;
        }
        #[allow(clippy::cast_precision_loss)]
        let done = self.frames_done as f32;
        #[allow(clippy::cast_precision_loss)]
        let total = self.frames_total as f32;
        (done / total).clamp(0.0, 1.0)
    }

    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.item.index());
        hasher.write_u32(self.id.get());
        hasher.write_u32(self.frames_done);
        hasher.write_u32(self.frames_total);
        hasher.write_u32(self.quantity_total);
        hasher.write_u32(self.quantity_produced);
        hasher.write_i64(self.paid);
        hasher.write_i32(self.command_points);
        let (tag, builder) = match self.kind {
            EntryKind::Unit { builder } => (0u8, builder),
            EntryKind::Upgrade => (1u8, false),
            EntryKind::Revival { builder } => (2u8, builder),
        };
        hasher.write_u8(tag);
        hasher.write_bool(builder);
    }

    fn is_ready(&self) -> bool {
        self.frames_done >= self.frames_total
    }

    fn is_builder(&self) -> bool {
        matches!(
            self.kind,
            EntryKind::Unit { builder: true } | EntryKind::Revival { builder: true }
        )
    }

    const fn is_revival(&self) -> bool {
        matches!(self.kind, EntryKind::Revival { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Idle,
    Building { id: ProductionId, item: ItemId },
    Ready { id: ProductionId, item: ItemId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProductionQueue {
    entries: Vec<ProductionEntry>,
    max_entries: u32,
    next_id: u32,
    revival_cooldown: u32,
}

impl ProductionQueue {
    #[must_use]
    pub fn new(max_entries: u32) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: max_entries.clamp(1, MAX_QUEUE_ENTRIES_CAP),
            next_id: 1,
            revival_cooldown: 0,
        }
    }

    #[must_use]
    pub const fn max_entries(&self) -> u32 {
        self.max_entries
    }

    #[must_use]
    pub const fn revival_cooldown(&self) -> u32 {
        self.revival_cooldown
    }

    pub fn set_revival_cooldown(&mut self, frames: u32) {
        self.revival_cooldown = frames;
    }

    pub fn decay_revival_cooldown(&mut self) {
        self.revival_cooldown = self.revival_cooldown.saturating_sub(1);
    }

    #[must_use]
    pub fn entries(&self) -> &[ProductionEntry] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn reserved_command_points(&self) -> i32 {
        self.entries
            .iter()
            .fold(0i32, |sum, entry| sum.saturating_add(entry.command_points))
    }

    pub fn queue(
        &mut self,
        request: QueueRequest,
        purse: &mut Money,
        points: &CommandPointPool,
    ) -> Result<ProductionId, Refusal> {
        if u32::try_from(self.entries.len()).unwrap_or(u32::MAX) >= self.max_entries {
            return Err(Refusal::QueueFull {
                limit: self.max_entries,
            });
        }
        if !purse.can_afford(request.cost) {
            return Err(Refusal::CannotAfford {
                cost: request.cost,
                held: purse.amount(),
            });
        }
        let free = points.free().saturating_sub(self.reserved_command_points());
        if request.command_points > 0 && request.command_points > free {
            return Err(Refusal::NoCommandPoints {
                needed: request.command_points,
                free,
                limit: points.limit(),
            });
        }

        let paid = purse.withdraw(request.cost);
        let id = ProductionId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let entry = ProductionEntry {
            item: request.item,
            id,
            frames_done: 0,
            frames_total: request.build_time.get(),
            quantity_total: request.quantity.max(1),
            quantity_produced: 0,
            paid,
            command_points: request.command_points,
            kind: request.kind,
        };
        self.entries.push(entry);
        Ok(id)
    }

    #[must_use]
    pub fn entry_to_process(&self) -> Option<ProductionId> {
        self.entries
            .iter()
            .find(|entry| entry.quantity_produced != 0)
            .or_else(|| self.entries.iter().find(|entry| entry.is_builder()))
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|entry| entry.is_revival() && entry.is_ready())
            })
            .or_else(|| self.entries.first())
            .map(|entry| entry.id)
    }

    #[must_use]
    pub fn running(&self) -> Option<&ProductionEntry> {
        let id = self.entry_to_process()?;
        self.entries.iter().find(|entry| entry.id == id)
    }

    #[must_use]
    pub fn ready(&self) -> Option<&ProductionEntry> {
        let entry = self.running()?;
        let complete = if entry.is_revival() {
            entry.is_ready() && self.revival_cooldown == 0
        } else {
            entry.is_ready()
        };
        complete.then_some(entry)
    }

    pub fn set_frames_needed(&mut self, id: ProductionId, frames: u32) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.id == id && entry.frames_done == 0)
        else {
            return false;
        };
        entry.frames_total = frames;
        true
    }

    pub fn tick_revivals(&mut self, fits: impl Fn(ItemId) -> bool) {
        for entry in &mut self.entries {
            if entry.is_revival() && fits(entry.item) {
                entry.frames_done = entry.frames_done.saturating_add(1);
            }
        }
    }

    pub fn tick(&mut self, frames: u32, allowed: bool, fits: impl Fn(ItemId) -> bool) -> Progress {
        self.revival_cooldown = self.revival_cooldown.saturating_sub(1);
        self.tick_revivals(&fits);
        let Some(id) = self.entry_to_process() else {
            return Progress::Idle;
        };
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return Progress::Idle;
        };
        let fits_now = fits(entry.item);
        match entry.kind {
            EntryKind::Upgrade => {
                entry.frames_done = entry.frames_done.saturating_add(frames);
            }
            EntryKind::Unit { .. } if allowed && fits_now => {
                entry.frames_done = entry.frames_done.saturating_add(frames);
            }
            EntryKind::Unit { .. } | EntryKind::Revival { .. } => {}
        }
        let complete = if entry.is_revival() {
            entry.is_ready() && allowed && fits_now && self.revival_cooldown == 0
        } else {
            entry.is_ready()
        };
        let (id, item) = (entry.id, entry.item);
        if complete {
            Progress::Ready { id, item }
        } else {
            Progress::Building { id, item }
        }
    }

    pub fn advance(&mut self) -> Progress {
        self.tick(1, true, |_| true)
    }

    pub fn deliver_one(&mut self) -> Option<Delivery> {
        let id = self
            .ready()
            .or_else(|| self.entries.first())
            .map(|entry| entry.id)?;
        self.deliver(id)
    }

    pub fn deliver(&mut self, id: ProductionId) -> Option<Delivery> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        let entry = self.entries.get_mut(index)?;
        entry.quantity_produced = entry.quantity_produced.saturating_add(1);
        let item = entry.item;
        let finished = entry.quantity_remaining() == 0;
        let command_points = entry.command_points;
        if finished {
            self.entries.remove(index);
        }
        Some(Delivery {
            item,
            command_points,
            entry_finished: finished,
        })
    }

    #[must_use]
    pub fn last_of(&self, item: ItemId) -> Option<&ProductionEntry> {
        self.entries.iter().rev().find(|entry| entry.item == item)
    }

    #[must_use]
    pub fn first_of(&self, item: ItemId) -> Option<&ProductionEntry> {
        self.entries.iter().find(|entry| entry.item == item)
    }

    pub fn cancel(&mut self, id: ProductionId, purse: &mut Money) -> Option<Cancellation> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        let entry = self.entries.get(index).copied()?;
        self.entries.remove(index);
        purse.deposit(entry.paid);
        Some(Cancellation {
            item: entry.item,
            refunded: entry.paid,
            command_points: entry.command_points,
        })
    }

    pub fn cancel_all(&mut self, purse: &mut Money) -> i64 {
        let mut refunded = 0i64;
        for entry in &self.entries {
            purse.deposit(entry.paid);
            refunded = refunded.saturating_add(entry.paid);
        }
        self.entries.clear();
        refunded
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.max_entries);
        hasher.write_u32(self.next_id);
        hasher.write_u32(self.revival_cooldown);
        hasher.write_usize(self.entries.len());
        for entry in &self.entries {
            entry.hash_into(hasher);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivery {
    pub item: ItemId,
    pub command_points: i32,
    pub entry_finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancellation {
    pub item: ItemId,
    pub refunded: i64,
    pub command_points: i32,
}

#[cfg(test)]
mod tests;
