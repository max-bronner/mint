use std::collections::BTreeMap;

use mint_sage_ini::{Block, Document};

const WINDOW_TRANSITION: &str = "WindowTransition";

pub(crate) const POOLS: usize = 3;
const ALL_POOLS: u8 = 0b111;
const SOUND_FADE_END_DEFAULT: i32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    ScreenFade,
    SoundFade,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowDef {
    style: Style,
    delay: i32,
    start: i32,
    end: i32,
    views: u8,
    leave_silent: bool,
}

impl WindowDef {
    fn total(self) -> i32 {
        self.delay.saturating_add(self.end)
    }

    fn sound_curve(self, local: i32, reversed: bool) -> f32 {
        if self.start == self.end {
            return if reversed { 1.0 } else { 0.0 };
        }
        if !reversed && local == self.end {
            return 0.0;
        }
        if reversed && local == self.start {
            return 1.0;
        }
        let span = self.end.saturating_sub(self.start) as f32;
        (1.0 - local.saturating_sub(self.start) as f32 / span).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroupDef {
    fire_once: bool,
    windows: Vec<WindowDef>,
}

impl GroupDef {
    fn total_frames(&self) -> i32 {
        self.windows.iter().map(|w| w.total()).max().unwrap_or(0)
    }

    fn fade_fraction(&self, frame: i32) -> f32 {
        self.windows
            .iter()
            .filter(|w| w.style == Style::ScreenFade)
            .map(|w| {
                let f = frame.saturating_sub(w.delay);
                if f < w.start {
                    return 0.0;
                }
                let span = w.end.saturating_sub(1).max(1);
                (f.clamp(0, w.end) as f32 / span as f32).min(1.0)
            })
            .fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Groups {
    by_name: BTreeMap<String, GroupDef>,
}

impl Groups {
    pub(crate) fn insert_document(&mut self, document: &Document) {
        for block in document.roots_named(WINDOW_TRANSITION) {
            let Some(name) = block.name() else { continue };
            self.by_name
                .insert(name.to_ascii_lowercase(), read_group(block));
        }
    }

    fn get(&self, name: &str) -> Option<&GroupDef> {
        self.by_name.get(&name.to_ascii_lowercase())
    }

    pub(crate) fn len(&self) -> usize {
        self.by_name.len()
    }
}

fn views_mask(value: &str) -> u8 {
    value
        .split(|c: char| c.is_whitespace() || c == '|' || c == ',')
        .fold(0, |mask, token| {
            mask | match token.to_ascii_uppercase().as_str() {
                "TACTICAL" => 1,
                "WORLD" => 2,
                "SHELL" => 4,
                _ => 0,
            }
        })
}

fn yes(block: &Block, key: &str) -> bool {
    block
        .field(key)
        .and_then(|f| f.value())
        .is_some_and(|v| v.eq_ignore_ascii_case("yes") || v == "1")
}

fn read_group(block: &Block) -> GroupDef {
    let int = |b: &Block, key: &str| {
        b.field(key)
            .and_then(|f| f.value())
            .and_then(|v| v.trim().parse::<i32>().ok())
    };
    let windows = block
        .blocks_named("Window")
        .filter_map(|window| {
            let transition = window.blocks_named("Transition").next()?;
            let style = match transition.name().map(str::to_ascii_uppercase).as_deref() {
                Some("SCREENFADE") => Style::ScreenFade,
                Some("SOUNDFADE") => Style::SoundFade,
                _ => Style::Other,
            };
            let end_default = if style == Style::SoundFade {
                SOUND_FADE_END_DEFAULT
            } else {
                0
            };
            Some(WindowDef {
                style,
                delay: int(window, "FrameDelay").unwrap_or(0),
                start: int(transition, "StartFrame").unwrap_or(0),
                end: int(transition, "EndFrame").unwrap_or(end_default),
                views: transition
                    .field("ViewsToFade")
                    .and_then(|f| f.value())
                    .map_or(ALL_POOLS, views_mask),
                leave_silent: yes(transition, "LeaveSilent"),
            })
        })
        .collect();
    GroupDef {
        fire_once: yes(block, "FireOnce"),
        windows,
    }
}

#[derive(Debug, Clone)]
struct Active {
    def: GroupDef,
    name: String,
    frame: i32,
    reversed: bool,
    sound: Vec<f32>,
}

impl Active {
    fn start(name: &str, def: &GroupDef) -> Self {
        Self {
            sound: vec![1.0; def.windows.len()],
            def: def.clone(),
            name: name.to_ascii_lowercase(),
            frame: 0,
            reversed: false,
        }
    }

    fn finished(&self) -> bool {
        if self.reversed {
            self.frame <= 0
        } else {
            self.frame >= self.def.total_frames()
        }
    }

    fn sync_sound(&mut self) {
        for (window, factor) in self.def.windows.iter().zip(&mut self.sound) {
            if window.style != Style::SoundFade {
                continue;
            }
            let local = self.frame.saturating_sub(window.delay);
            if (window.start..=window.end).contains(&local) {
                *factor = window.sound_curve(local, self.reversed);
            }
        }
    }

    fn pool_factor(&self, pool: usize) -> f32 {
        self.def
            .windows
            .iter()
            .zip(&self.sound)
            .filter(|(w, _)| w.style == Style::SoundFade && (w.views >> pool) & 1 == 1)
            .fold(1.0, |product, (_, factor)| product * factor)
    }

    fn views(&self) -> u8 {
        self.def
            .windows
            .iter()
            .filter(|w| w.style == Style::SoundFade)
            .fold(0, |mask, w| mask | w.views)
    }

    fn skip_to_end(&mut self) {
        self.frame = if self.reversed {
            0
        } else {
            self.def.total_frames()
        };
        for (window, factor) in self.def.windows.iter().zip(&mut self.sound) {
            if window.style == Style::SoundFade {
                *factor = 0.0;
            }
        }
    }

    fn reverse(&mut self) {
        self.reversed = true;
        self.frame = self.def.total_frames();
    }

    fn step(&mut self) {
        if self.finished() {
            return;
        }
        self.frame = self
            .frame
            .saturating_add(if self.reversed { -1 } else { 1 });
        self.sync_sound();
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TransitionHandler {
    groups: Groups,
    current: Option<Active>,
    pending: Option<Active>,
    hold: bool,
    accumulator: f32,
    draw_is_current: bool,
    suspended: u32,
    applied: [f32; POOLS],
}

const CLIENT_FPS: f32 = 30.0;

impl TransitionHandler {
    pub(crate) fn new(groups: Groups) -> Self {
        Self {
            groups,
            applied: [1.0; POOLS],
            ..Self::default()
        }
    }

    pub(crate) fn pool_gain(&self, pool: usize) -> f32 {
        self.applied.get(pool).copied().unwrap_or(1.0)
    }

    pub(crate) fn suspend(&mut self) {
        self.suspended = self.suspended.saturating_add(1);
    }

    pub(crate) fn resume(&mut self) {
        self.suspended = self.suspended.saturating_sub(1);
        self.apply(ALL_POOLS);
    }

    fn apply(&mut self, mask: u8) {
        if self.suspended > 0 {
            return;
        }
        for pool in 0..POOLS {
            if (mask >> pool) & 1 == 0 {
                continue;
            }
            let mut product = 1.0;
            if let Some(current) = &self.current {
                let factor = current.pool_factor(pool);
                product *= factor;
                if self.draw_is_current {
                    product *= factor;
                }
            }
            if let Some(pending) = &self.pending {
                product *= pending.pool_factor(pool);
            }
            if let Some(slot) = self.applied.get_mut(pool) {
                *slot = product.clamp(0.0, 1.0);
            }
        }
    }

    fn live_views(&self) -> u8 {
        self.current
            .iter()
            .chain(&self.pending)
            .fold(0, |mask, group| mask | group.views())
    }

    fn release(&mut self, group: &Active) {
        let mask = group
            .def
            .windows
            .iter()
            .filter(|w| w.style == Style::SoundFade && !w.leave_silent)
            .fold(0, |mask, w| mask | w.views);
        self.apply(mask);
    }

    fn drop_current(&mut self) {
        self.draw_is_current = false;
        if let Some(group) = self.current.take() {
            self.release(&group);
        }
    }

    fn release_all(&mut self) {
        self.drop_current();
        if let Some(group) = self.pending.take() {
            self.release(&group);
        }
    }

    pub(crate) fn set_hold(&mut self, hold: bool) {
        self.hold = hold;
    }

    pub(crate) fn set_group(&mut self, name: &str) {
        let Some(def) = self.groups.get(name) else {
            return;
        };
        let group = Active::start(name, def);
        if let Some(current) = &mut self.current {
            if !current.def.fire_once && !current.reversed {
                current.reverse();
            }
            self.pending = Some(group);
        } else {
            self.current = Some(group);
        }
    }

    pub(crate) fn set_group_ended(&mut self, name: &str) {
        let Some(def) = self.groups.get(name) else {
            return;
        };
        let mut group = Active::start(name, def);
        group.skip_to_end();
        self.release_all();
        self.current = Some(group);
    }

    pub(crate) fn reset(&mut self) {
        self.release_all();
        self.hold = false;
    }

    pub(crate) fn reverse(&mut self, name: &str) {
        let key = name.to_ascii_lowercase();
        if let Some(current) = &mut self.current {
            if current.name == key {
                current.reverse();
                return;
            }
        }
        if self.pending.as_ref().is_some_and(|p| p.name == key) {
            self.pending = None;
            return;
        }
        let Some(def) = self.groups.get(name) else {
            return;
        };
        let mut group = Active::start(name, def);
        group.skip_to_end();
        group.reverse();
        self.release_all();
        self.current = Some(group);
    }

    #[cfg(test)]
    pub(crate) fn with_test_group(name: &str, end: i32) -> Self {
        let window = WindowDef {
            style: Style::ScreenFade,
            delay: 0,
            start: 0,
            end,
            views: 7,
            leave_silent: false,
        };
        let mut groups = Groups::default();
        groups.by_name.insert(
            name.to_ascii_lowercase(),
            GroupDef {
                fire_once: true,
                windows: vec![window],
            },
        );
        Self::new(groups)
    }

    pub(crate) fn group_length(&self, name: &str) -> u64 {
        self.groups
            .get(name)
            .map_or(0, |def| u64::try_from(def.total_frames()).unwrap_or(0))
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.current.as_ref().is_none_or(Active::finished)
    }

    pub(crate) fn has_current(&self) -> bool {
        self.current.is_some()
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        self.accumulator += dt.max(0.0);
        while self.accumulator >= 1.0 / CLIENT_FPS {
            self.accumulator -= 1.0 / CLIENT_FPS;
            self.update();
        }
    }

    pub(crate) fn update(&mut self) {
        self.draw_is_current = self.current.is_some();
        if let Some(current) = &mut self.current {
            current.step();
        }
        let finished = self.current.as_ref().is_some_and(Active::finished);
        let fire_once = self.current.as_ref().is_some_and(|c| c.def.fire_once);
        if finished && fire_once && self.pending.is_none() && !self.hold {
            self.drop_current();
        }
        if self.current.as_ref().is_none_or(Active::finished) {
            if let Some(next) = self.pending.take() {
                self.draw_is_current = false;
                if let Some(old) = self.current.replace(next) {
                    self.release(&old);
                }
            }
        }
        if self
            .current
            .as_ref()
            .is_some_and(|c| c.reversed && c.finished())
        {
            self.drop_current();
        }
        self.apply(self.live_views());
    }

    pub(crate) fn screen_alpha(&self) -> f32 {
        self.current
            .as_ref()
            .map_or(0.0, |c| c.def.fade_fraction(c.frame))
    }
}

#[cfg(test)]
mod tests;
