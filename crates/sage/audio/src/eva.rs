use mint_sage_ini::eva::{EvaDefs, EvaEventDef};

pub const NOWHERE: [f32; 3] = [-100.0, -100.0, 0.0];

const QUIET_START_FRAMES: u32 = 2;

#[derive(Debug, Clone, Copy, Default)]
struct Status {
    blocked_ms: f32,
    pending_ms: f32,
    at: Option<[f32; 3]>,
    played: Option<(u32, [f32; 3])>,
}

#[derive(Debug, Clone, Copy)]
struct Delayed {
    due_frame: u32,
    id: usize,
    at: Option<[f32; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvaLine {
    pub event: String,
    pub sound: String,
    pub at: [f32; 3],
}

impl EvaLine {
    #[must_use]
    pub fn has_place(&self) -> bool {
        self.at != NOWHERE
    }
}

#[derive(Debug, Clone)]
pub struct Eva {
    defs: Vec<EvaEventDef>,
    blocks: Vec<Vec<usize>>,
    status: Vec<Status>,
    delayed: Vec<Delayed>,
    logic_hz: u32,
    speaking_ms: f32,
}

impl Eva {
    #[must_use]
    pub fn new(defs: &EvaDefs, logic_hz: u32) -> Self {
        let blocks = defs
            .events
            .iter()
            .map(|e| e.blocks.iter().filter_map(|b| defs.position(b)).collect())
            .collect();
        Self {
            defs: defs.events.clone(),
            blocks,
            status: vec![
                Status {
                    pending_ms: -1.0,
                    ..Status::default()
                };
                defs.events.len()
            ],
            delayed: Vec::new(),
            logic_hz: logic_hz.max(1),
            speaking_ms: 0.0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    #[must_use]
    pub fn id(&self, name: &str) -> Option<usize> {
        self.defs
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    #[must_use]
    pub fn def(&self, id: usize) -> Option<&EvaEventDef> {
        self.defs.get(id)
    }

    pub fn report(&mut self, id: usize, frame: u32, at: Option<[f32; 3]>) {
        let Some(def) = self.defs.get(id) else {
            return;
        };
        if def.wait_before_playing_ms > 0 {
            let frames = duration_frames(def.wait_before_playing_ms, self.logic_hz);
            self.delayed.push(Delayed {
                due_frame: frame.saturating_add(frames),
                id,
                at,
            });
            return;
        }
        self.internal_report(id, at);
    }

    fn internal_report(&mut self, id: usize, at: Option<[f32; 3]>) {
        let (Some(def), Some(status)) = (self.defs.get(id), self.status.get_mut(id)) else {
            return;
        };
        if status.blocked_ms > 0.0 {
            status.blocked_ms = status.blocked_ms.max(ms(def.quiet_time_ms));
            return;
        }
        status.pending_ms = ms(def.expiration_time_ms);
        status.at = at;
    }

    #[must_use]
    pub fn is_blocked(&self, id: usize) -> bool {
        self.status.get(id).is_some_and(|s| s.blocked_ms > 0.0)
    }

    #[must_use]
    pub fn is_about_to_play(&self, id: usize) -> bool {
        self.status.get(id).is_some_and(|s| s.pending_ms >= 0.0)
    }

    #[must_use]
    pub fn last_played(&self, id: usize) -> Option<(u32, [f32; 3])> {
        self.status.get(id).and_then(|s| s.played)
    }

    pub fn update(
        &mut self,
        frame: u32,
        elapsed_ms: f32,
        side: &str,
        home_base: Option<[f32; 3]>,
        play: &mut dyn FnMut(&EvaLine) -> Option<u32>,
    ) -> Option<EvaLine> {
        let mut spoken = None;
        if self.speaking_ms <= 0.0 && frame > QUIET_START_FRAMES {
            spoken = self.speak(frame, side, home_base, play);
        }
        self.speaking_ms -= elapsed_ms;
        for status in &mut self.status {
            status.blocked_ms -= elapsed_ms;
            status.pending_ms -= elapsed_ms;
        }
        let mut due = Vec::new();
        self.delayed.retain(|d| {
            let ready = d.due_frame <= frame;
            if ready {
                due.push(*d);
            }
            !ready
        });
        due.sort_by_key(|d| d.due_frame);
        for d in due {
            self.internal_report(d.id, d.at);
        }
        spoken
    }

    fn speak(
        &mut self,
        frame: u32,
        side: &str,
        home_base: Option<[f32; 3]>,
        play: &mut dyn FnMut(&EvaLine) -> Option<u32>,
    ) -> Option<EvaLine> {
        loop {
            let mut best: Option<(usize, u32, f32)> = None;
            for (id, (def, status)) in self.defs.iter().zip(&self.status).enumerate() {
                if status.pending_ms < 0.0 {
                    continue;
                }
                let age = ms(def.expiration_time_ms) - status.pending_ms;
                let better = match best {
                    None => true,
                    Some((_, priority, best_age)) => {
                        def.priority > priority || (def.priority == priority && age < best_age)
                    }
                };
                if better {
                    best = Some((id, def.priority, age));
                }
            }
            let (id, _, _) = best?;
            let def = self.defs.get(id)?;
            let status = self.status.get(id).copied()?;
            let at = match status.at {
                Some(at) if !def.always_play_from_home_base => at,
                _ => home_base.unwrap_or(NOWHERE),
            };
            let line = def.sound_for(side).map(|sound| EvaLine {
                event: def.name.clone(),
                sound: sound.to_string(),
                at,
            });
            self.mark_played(id, frame, at);
            if let Some(line) = line {
                if let Some(length) = play(&line) {
                    self.speaking_ms = ms(length);
                    return Some(line);
                }
            }
        }
    }

    fn mark_played(&mut self, id: usize, frame: u32, at: [f32; 3]) {
        let blocked = self.blocks.get(id).cloned().unwrap_or_default();
        self.count_as_played(id);
        for other in blocked {
            self.count_as_played(other);
        }
        if let Some(status) = self.status.get_mut(id) {
            status.played = Some((frame, at));
        }
    }

    fn count_as_played(&mut self, id: usize) {
        let (Some(def), Some(status)) = (self.defs.get(id), self.status.get_mut(id)) else {
            return;
        };
        status.pending_ms = -1.0;
        status.blocked_ms = ms(def.time_between_events_ms.max(def.quiet_time_ms));
    }
}

#[allow(clippy::cast_precision_loss)]
fn ms(value: u32) -> f32 {
    value as f32
}

fn duration_frames(ms: u32, logic_hz: u32) -> u32 {
    let scaled = u64::from(ms).saturating_mul(u64::from(logic_hz));
    u32::try_from(scaled.div_ceil(1000)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
