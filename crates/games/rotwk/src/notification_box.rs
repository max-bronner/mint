#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Anchor {
    #[default]
    Main,
    SpellStore,
    Objectives,
}

impl Anchor {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::SpellStore => "spellStore",
            Self::Objectives => "objectives",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    pub(crate) title: String,
    pub(crate) icon: Option<String>,
    pub(crate) open_audio: Option<String>,
    pub(crate) message: String,
    pub(crate) font: Option<mint_sage_ini::MessageFont>,
    pub(crate) colour: Option<[u8; 4]>,
    pub(crate) ms: u32,
    pub(crate) anchor: Anchor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Uninitialized,
    Idle,
    Opening,
    Open,
    Closing,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    Present(Entry),
    Close,
    Hide,
}

pub(crate) fn icon_for(
    type_icon: Option<&str>,
    object_type: &str,
    button_image: impl Fn(&str) -> Option<Option<String>>,
) -> Option<String> {
    if object_type.is_empty() {
        return type_icon.map(str::to_owned);
    }
    button_image(object_type).unwrap_or_else(|| type_icon.map(str::to_owned))
}

#[derive(Debug, Default)]
pub(crate) struct Manager {
    phase: Phase,
    pending: Option<Entry>,
    current: Option<Entry>,
    opened_at_ms: u64,
    message_width: f32,
}

impl Manager {
    pub(crate) fn set_message_width(&mut self, width: f32) {
        self.message_width = width;
    }

    pub(crate) fn submit(&mut self, entry: Entry) {
        self.pending = Some(entry);
    }

    pub(crate) fn on_initialized(&mut self) {
        if self.phase == Phase::Uninitialized {
            self.phase = Phase::Idle;
        }
    }

    pub(crate) fn on_open(&mut self, now_ms: u64) {
        if self.phase == Phase::Opening {
            self.phase = Phase::Open;
            self.opened_at_ms = now_ms;
        }
    }

    pub(crate) fn on_close_button(&mut self) -> Option<Effect> {
        (self.phase == Phase::Open).then(|| {
            self.phase = Phase::Closing;
            Effect::Close
        })
    }

    pub(crate) fn on_closed(&mut self) {
        if self.phase == Phase::Closing {
            self.phase = Phase::Idle;
            self.current = None;
        }
    }

    pub(crate) fn update(&mut self, now_ms: u64, letterboxed: bool) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.phase != Phase::Uninitialized {
            if let Some(entry) = self.pending.take() {
                if self.message_width > 0.0 && entry.font.is_some() && !letterboxed {
                    self.phase = Phase::Opening;
                    self.current = Some(entry.clone());
                    effects.push(Effect::Present(entry));
                } else {
                    self.current = None;
                    self.phase = Phase::Idle;
                    effects.push(Effect::Hide);
                }
            }
        }
        if self.phase == Phase::Open {
            let timed = self.current.as_ref().is_some_and(|entry| {
                entry.ms != 0 && now_ms.saturating_sub(self.opened_at_ms) >= u64::from(entry.ms)
            });
            if timed {
                self.phase = Phase::Closing;
                effects.push(Effect::Close);
            }
        }
        effects
    }
}

#[cfg(test)]
mod tests;
