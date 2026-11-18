use std::collections::VecDeque;

use crate::instance::InstanceId;
use crate::library::MovieId;

pub mod clip_event {
    pub const LOAD: u32 = 1 << 0;
    pub const ENTER_FRAME: u32 = 1 << 1;
    pub const UNLOAD: u32 = 1 << 2;
    pub const INITIALIZE: u32 = 1 << 9;
    pub const PRESS: u32 = 1 << 10;
    pub const RELEASE: u32 = 1 << 11;
    pub const RELEASE_OUTSIDE: u32 = 1 << 12;
    pub const ROLL_OVER: u32 = 1 << 13;
    pub const ROLL_OUT: u32 = 1 << 14;
    pub const DRAG_OVER: u32 = 1 << 15;
    pub const DRAG_OUT: u32 = 1 << 16;
    pub const CONSTRUCT: u32 = 1 << 18;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionSource {
    Frame { block: u32 },
    ClipEvent { event: u32, block: u32 },
    Initialize { block: u32 },
    Construct { blocks: Vec<u32> },
    Button { condition: u32, block: u32 },
    InitAction { block: u32 },
    Method { name: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Initialize = 0,
    Construct = 1,
    Normal = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedAction {
    pub clip: InstanceId,
    pub movie: MovieId,
    pub source: ActionSource,
    pub is_unload: bool,
}

impl QueuedAction {
    fn priority(&self) -> Priority {
        match self.source {
            ActionSource::Initialize { .. } | ActionSource::InitAction { .. } => {
                Priority::Initialize
            }
            ActionSource::Construct { .. } => Priority::Construct,
            _ => Priority::Normal,
        }
    }
}

#[derive(Debug)]
pub struct ActionQueue {
    buckets: [VecDeque<QueuedAction>; 3],
    len: usize,
    cap: usize,
}

impl ActionQueue {
    #[must_use]
    pub fn with_cap(cap: usize) -> Self {
        Self {
            buckets: [VecDeque::new(), VecDeque::new(), VecDeque::new()],
            len: 0,
            cap,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push(&mut self, action: QueuedAction) -> bool {
        if self.len >= self.cap {
            return false;
        }
        if let Some(bucket) = self.buckets.get_mut(action.priority() as usize) {
            bucket.push_back(action);
            self.len = self.len.saturating_add(1);
            true
        } else {
            false
        }
    }

    pub fn pop(&mut self) -> Option<QueuedAction> {
        let action = self.buckets.iter_mut().find_map(VecDeque::pop_front)?;
        self.len = self.len.saturating_sub(1);
        Some(action)
    }

    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.len = 0;
    }
}
