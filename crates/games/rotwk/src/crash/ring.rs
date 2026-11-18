use std::fmt::{self, Write};

pub(crate) const MAX_LINE_BYTES: usize = 240;

pub(crate) struct Ring {
    slots: Vec<String>,
    capacity: usize,
    next: usize,
    written: u64,
}

impl Ring {
    pub(crate) const fn new(capacity: usize) -> Self {
        Self {
            slots: Vec::new(),
            capacity,
            next: 0,
            written: 0,
        }
    }

    pub(crate) fn push(&mut self, line: fmt::Arguments<'_>) {
        if self.capacity == 0 {
            return;
        }
        if self.slots.len() < self.capacity {
            self.slots.push(String::new());
        }
        let index = self.next;
        self.next = index.saturating_add(1) % self.capacity;
        self.written = self.written.saturating_add(1);
        if let Some(slot) = self.slots.get_mut(index) {
            slot.clear();
            let _ = Capped(slot).write_fmt(line);
        }
    }

    pub(crate) fn lines(&self) -> impl Iterator<Item = &str> {
        let split = if self.slots.len() < self.capacity {
            0
        } else {
            self.next
        };
        let (newer, older) = self.slots.split_at(split.min(self.slots.len()));
        older.iter().chain(newer).map(String::as_str)
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.written
            .saturating_sub(u64::try_from(self.slots.len()).unwrap_or(u64::MAX))
    }
}

struct Capped<'a>(&'a mut String);

impl Write for Capped<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let room = MAX_LINE_BYTES.saturating_sub(self.0.len());
        let mut end = text.len().min(room);
        while end > 0 && !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        self.0.push_str(text.get(..end).unwrap_or_default());
        Ok(())
    }
}
