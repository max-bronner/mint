use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
struct Entry {
    id: u64,
    at: [f32; 2],
}

#[derive(Debug, Clone)]
pub(crate) struct JumpList {
    entries: VecDeque<Entry>,
    next_id: u64,
    cursor: Option<u64>,
    last_press: Option<Duration>,
    min_distance: f32,
    max_reset: Duration,
}

impl JumpList {
    pub(crate) fn new(min_distance: f32, max_reset_ms: u32) -> Self {
        Self {
            entries: VecDeque::new(),
            next_id: 0,
            cursor: None,
            last_press: None,
            min_distance,
            max_reset: Duration::from_millis(u64::from(max_reset_ms)),
        }
    }

    pub(crate) fn push(&mut self, at: [f32; 2]) {
        if self
            .entries
            .front()
            .is_some_and(|front| distance(front.at, at) <= self.min_distance)
        {
            if let Some(front) = self.entries.pop_front() {
                if self.cursor == Some(front.id) {
                    self.cursor = None;
                }
            }
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push_front(Entry { id, at });
    }

    pub(crate) fn press(&mut self, now: Duration) -> Option<[f32; 2]> {
        if self.entries.is_empty() {
            return None;
        }
        let index = match (self.cursor, self.last_press) {
            (Some(id), Some(last)) if now.saturating_sub(last) <= self.max_reset => self
                .entries
                .iter()
                .position(|e| e.id == id)
                .map_or(0, |i| if i + 1 < self.entries.len() { i + 1 } else { 0 }),
            _ => 0,
        };
        self.last_press = Some(now);
        let entry = *self.entries.get(index)?;
        self.cursor = Some(entry.id);
        Some(entry.at)
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (a[0] - b[0], a[1] - b[1]);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests;
