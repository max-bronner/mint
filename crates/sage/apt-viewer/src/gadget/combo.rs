use std::sync::Arc;

use mint_sage_wnd::{Layout, Window};

use super::check::State;
use super::list::{unit, ListCell, ListColumn, ListContent, ListLook, ListView};
use crate::host::SurfaceIcon;

pub const TEXT_POINTS: f32 = 10.0;

pub const TEXT_TOP: f32 = 5.0;

pub const SWATCH: f32 = 20.0;

pub type ComboStyle = ComboLook<SurfaceIcon>;

#[derive(Debug, Clone)]
pub struct ComboItem {
    pub text: String,
    pub colour: Option<[f32; 4]>,
    pub swatch: Option<[f32; 4]>,
    pub image: Option<(SurfaceIcon, [f32; 2])>,
}

impl ComboItem {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            colour: None,
            swatch: None,
            image: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComboContent {
    pub items: Vec<ComboItem>,
    pub selected: Option<usize>,
    pub enabled: bool,
}

impl Default for ComboContent {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComboLook<I> {
    pub font: String,
    pub points: f32,
    pub text: [[f32; 4]; 3],
    pub button: [Option<I>; 3],
    pub pushed: Option<I>,
    pub list: ListLook<I>,
    pub max_display: usize,
}

impl<I> ComboLook<I> {
    #[must_use]
    pub fn plain() -> Self {
        Self {
            font: "Albertus MT".to_owned(),
            points: TEXT_POINTS,
            text: [[1.0; 4], [1.0; 4], [0.75, 0.75, 0.75, 1.0]],
            button: [None, None, None],
            pushed: None,
            list: ListLook::plain(),
            max_display: 10,
        }
    }

    pub fn resolved<J>(self, mut resolve: impl FnMut(&I) -> Option<(J, [f32; 2])>) -> ComboLook<J> {
        let mut image = |i: Option<I>| i.as_ref().and_then(&mut resolve).map(|(j, _)| j);
        let [b0, b1, b2] = self.button;
        let button = [image(b0), image(b1), image(b2)];
        let pushed = image(self.pushed);
        ComboLook {
            font: self.font,
            points: self.points,
            text: self.text,
            button,
            pushed,
            list: self.list.resolved(resolve),
            max_display: self.max_display,
        }
    }
}

impl ComboLook<String> {
    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        let mut look = Self::plain();
        let Some(window) = layout.windows.first() else {
            return look;
        };
        if let Some(font) = window.font() {
            look.font = font.name;
        }
        if let Some(c) = window.text_colors() {
            look.text = [unit(c.enabled), unit(c.hilite), unit(c.disabled)];
        }
        if let Some(data) = window.combo_box() {
            look.max_display = usize::try_from(data.max_display.max(1)).unwrap_or(1);
        }
        let image = |w: &Window, key: &str, index: usize| {
            w.draw_data(key)
                .and_then(|d| d.into_iter().nth(index))
                .and_then(|item| item.image)
        };
        let kind = |w: &Window, t: &str| {
            w.window_type()
                .is_some_and(|found| found.eq_ignore_ascii_case(t))
        };
        if kind(window, "COMBOBOX") {
            let key = |state: &str| format!("COMBOBOXDROPDOWNBUTTON{state}DRAWDATA");
            look.button = ["ENABLED", "HILITE", "DISABLED"].map(|s| image(window, &key(s), 0));
            look.pushed = image(window, &key("ENABLED"), 1);
            look.list = ListLook::read_window(
                window,
                [
                    "COMBOBOXLISTBOXENABLEDDRAWDATA",
                    "COMBOBOXLISTBOXHILITEDRAWDATA",
                ],
            );
        } else {
            if let Some(button) = window.children.iter().find(|c| kind(c, "PUSHBUTTON")) {
                look.button = ["ENABLEDDRAWDATA", "HILITEDRAWDATA", "DISABLEDDRAWDATA"]
                    .map(|key| image(button, key, 0));
                look.pushed = image(button, "ENABLEDDRAWDATA", 1);
            }
            if let Some(list) = window.children.iter().find(|c| kind(c, "SCROLLLISTBOX")) {
                look.list = ListLook::read_window(list, ["ENABLEDDRAWDATA", "HILITEDRAWDATA"]);
            }
        }
        look.list.points = TEXT_POINTS;
        look
    }
}

#[must_use]
pub fn button_rect(bounds: [f32; 4], aspect: f32) -> [f32; 4] {
    let [l, t, r, b] = bounds;
    let side = ((b - t) * aspect).min(r - l).max(0.0);
    [r - side, t, r, b]
}

#[must_use]
pub fn text_rect(bounds: [f32; 4], aspect: f32) -> [f32; 4] {
    let button = button_rect(bounds, aspect);
    [bounds[0], bounds[1], button[0], bounds[3]]
}

#[must_use]
pub fn list_height(line: f32, count: usize, max_display: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let n = count.min(max_display).max(1) as f32;
    line * n + n * 2.0 + 4.0
}

#[must_use]
pub fn needs_scroll_bar(count: usize, max_display: usize) -> bool {
    count > max_display
}

#[must_use]
pub fn list_rect(bounds: [f32; 4], height: f32, upward: bool) -> [f32; 4] {
    let [l, t, r, b] = bounds;
    if upward {
        [l, t - height, r, t]
    } else {
        [l, b, r, b + height]
    }
}

#[must_use]
pub fn opens_upward(bounds: [f32; 4], height: f32, top_limit: f32, limit: f32) -> bool {
    let below = limit - bounds[3];
    let above = bounds[1] - top_limit;
    height > below && above > below
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Confirm,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct ComboView {
    pub content: Arc<ComboContent>,
    pub selected: Option<usize>,
    pub open: bool,
    pub upward: bool,
    pub hover: bool,
    pub bounds: Option<[f32; 4]>,
    pub list: Option<ListView>,
    pub list_rect: Option<[f32; 4]>,
    pub max_display: usize,
}

impl ComboView {
    #[must_use]
    pub fn new(content: Arc<ComboContent>) -> Self {
        Self {
            selected: content.selected,
            content,
            open: false,
            upward: false,
            hover: false,
            bounds: None,
            list: None,
            list_rect: None,
            max_display: 10,
        }
    }

    pub fn set_content(&mut self, content: Arc<ComboContent>) {
        if Arc::ptr_eq(&self.content, &content) {
            return;
        }
        self.selected = content.selected;
        self.content = content;
        if !self.content.enabled {
            self.close();
        } else if self.list.is_some() {
            let rows = Arc::new(self.list_content());
            if let Some(list) = &mut self.list {
                list.set_content(rows);
            }
        }
    }

    #[must_use]
    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.bounds.is_some_and(|b| inside(b, p))
    }

    #[must_use]
    pub fn list_contains(&self, p: [f32; 2]) -> bool {
        self.open && self.list_rect.is_some_and(|r| inside(r, p))
    }

    #[must_use]
    pub fn list_content(&self) -> ListContent {
        ListContent {
            columns: vec![ListColumn {
                percent: 100.0,
                align: super::list::Align::Left,
            }],
            rows: self
                .content
                .items
                .iter()
                .map(|item| {
                    vec![match (&item.image, item.swatch) {
                        (Some((icon, size)), _) => ListCell::Image {
                            icon: Some(*icon),
                            natural: Some(*size),
                            size: Some(*size),
                        },
                        (None, Some(colour)) => ListCell::Swatch(colour),
                        (None, None) => ListCell::Text(item.text.clone()),
                    }]
                })
                .collect(),
            selected: self.selected,
            scroll_bar: Some(needs_scroll_bar(self.content.items.len(), self.max_display)),
            text_inset: 0.0,
        }
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else if self.content.enabled {
            self.open = true;
            let mut list = ListView::new(Arc::new(self.list_content()));
            list.tracking = true;
            list.hover_row = self.selected;
            self.list = Some(list);
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.list = None;
        self.list_rect = None;
    }

    pub fn choose(&mut self, row: usize) -> Option<usize> {
        self.close();
        if row >= self.content.items.len() || self.selected == Some(row) {
            return None;
        }
        self.selected = Some(row);
        Some(row)
    }

    pub fn key(&mut self, nav: Nav) -> Option<usize> {
        if !self.open {
            return None;
        }
        let count = self.content.items.len();
        let next = match (nav, self.selected) {
            (Nav::Confirm | Nav::Cancel, _) => {
                self.close();
                return None;
            }
            (_, None) if count > 0 => 0,
            (Nav::Up, Some(at)) => at.saturating_sub(1),
            (Nav::Down, Some(at)) => at.saturating_add(1).min(count.saturating_sub(1)),
            (_, None) => return None,
        };
        if self.selected == Some(next) {
            return None;
        }
        self.selected = Some(next);
        let content = Arc::new(self.list_content());
        if let Some(list) = &mut self.list {
            list.set_content(content);
        }
        Some(next)
    }

    #[must_use]
    pub fn state(&self) -> State {
        State::of(self.content.enabled, self.hover || self.open)
    }
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
