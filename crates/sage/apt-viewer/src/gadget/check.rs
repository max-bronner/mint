use mint_sage_wnd::Layout;

use super::list::unit;
use crate::host::SurfaceIcon;

pub type CheckStyle = CheckLook<SurfaceIcon>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckContent {
    pub checked: bool,
    pub label: Option<String>,
    pub enabled: bool,
}

impl Default for CheckContent {
    fn default() -> Self {
        Self {
            checked: false,
            label: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Enabled = 0,
    Hilite = 1,
    Disabled = 2,
}

impl State {
    #[must_use]
    pub fn of(enabled: bool, hovered: bool) -> Self {
        match (enabled, hovered) {
            (false, _) => Self::Disabled,
            (true, true) => Self::Hilite,
            (true, false) => Self::Enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckLook<I> {
    pub font: String,
    pub points: f32,
    pub text: [[f32; 4]; 3],
    pub boxes: [[Option<I>; 2]; 3],
}

impl<I> CheckLook<I> {
    #[must_use]
    pub fn plain() -> Self {
        Self {
            font: "Albertus MT".to_owned(),
            points: 16.0,
            text: [[1.0; 4]; 3],
            boxes: [[None, None], [None, None], [None, None]],
        }
    }

    pub fn resolved<J>(self, mut resolve: impl FnMut(&I) -> Option<J>) -> CheckLook<J> {
        let boxes = self
            .boxes
            .map(|pair| pair.map(|image| image.as_ref().and_then(&mut resolve)));
        CheckLook {
            font: self.font,
            points: self.points,
            text: self.text,
            boxes,
        }
    }
}

impl CheckLook<String> {
    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        let mut look = Self::plain();
        let Some(window) = layout.windows.first() else {
            return look;
        };
        if let Some(font) = window.font() {
            look.font = font.name;
            #[allow(clippy::cast_precision_loss)]
            let points = font.size as f32;
            if points > 0.0 {
                look.points = points;
            }
        }
        if let Some(c) = window.text_colors() {
            look.text = [unit(c.enabled), unit(c.hilite), unit(c.disabled)];
        }
        let image = |key: &str, index: usize| {
            window
                .draw_data(key)
                .and_then(|d| d.into_iter().nth(index))
                .and_then(|item| item.image)
        };
        look.boxes = ["ENABLEDDRAWDATA", "HILITEDRAWDATA", "DISABLEDDRAWDATA"]
            .map(|key| [image(key, 1), image(key, 2)]);
        look
    }
}

#[must_use]
pub fn check_box(bounds: [f32; 4], aspect: f32, pixel: f32) -> [f32; 4] {
    let [l, t, _, b] = bounds;
    let side = (b - t - 6.0 * pixel).max(0.0);
    let top = t + 3.0 * pixel;
    [l, top, l + side * aspect, top + side]
}

#[must_use]
pub fn check_label(bounds: [f32; 4]) -> [f32; 2] {
    let [l, t, _, b] = bounds;
    [l + (b - t), (t + b) * 0.5]
}

#[derive(Debug, Clone)]
pub struct CheckView {
    pub content: std::sync::Arc<CheckContent>,
    pub checked: bool,
    pub hover: bool,
    pub bounds: Option<[f32; 4]>,
}

impl CheckView {
    #[must_use]
    pub fn new(content: std::sync::Arc<CheckContent>) -> Self {
        Self {
            checked: content.checked,
            content,
            hover: false,
            bounds: None,
        }
    }

    pub fn set_content(&mut self, content: std::sync::Arc<CheckContent>) {
        if !std::sync::Arc::ptr_eq(&self.content, &content) {
            self.checked = content.checked;
            self.content = content;
        }
    }

    #[must_use]
    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.bounds
            .is_some_and(|b| p[0] >= b[0] && p[0] < b[2] && p[1] >= b[1] && p[1] < b[3])
    }

    pub fn click(&mut self, p: [f32; 2]) -> Option<bool> {
        if !self.content.enabled || !self.contains(p) {
            return None;
        }
        self.checked = !self.checked;
        Some(self.checked)
    }

    #[must_use]
    pub fn state(&self) -> State {
        State::of(self.content.enabled, self.hover)
    }
}

#[cfg(test)]
mod tests;
