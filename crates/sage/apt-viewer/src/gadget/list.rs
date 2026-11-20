use std::sync::Arc;

use mint_sage_wnd::{Layout, Rgba, Window};

use crate::host::SurfaceIcon;

pub const SCROLL_BUTTON: [f32; 2] = [21.0, 22.0];
pub const THUMB_HEIGHT: f32 = 16.0;
pub const TEXT_X_OFFSET: f32 = 5.0;
pub const TEXT_WIDTH_OFFSET: f32 = 7.0;
pub const ROWS_TOP: f32 = 4.0;
pub const POINTS_TO_UNITS: f32 = 96.0 / 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Centre,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListColumn {
    pub percent: f32,
    pub align: Align,
}

#[derive(Debug, Clone)]
pub enum ListCell {
    Text(String),
    Image {
        icon: Option<SurfaceIcon>,
        natural: Option<[f32; 2]>,
        size: Option<[f32; 2]>,
    },
    Swatch([f32; 4]),
}

#[derive(Debug, Clone, Default)]
pub struct ListContent {
    pub columns: Vec<ListColumn>,
    pub rows: Vec<Vec<ListCell>>,
    pub selected: Option<usize>,
    pub scroll_bar: Option<bool>,
    pub text_inset: f32,
    pub scroll_to_top: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Slot<I> {
    pub image: Option<I>,
    pub fill: [f32; 4],
    pub border: [f32; 4],
}

impl<I> Default for Slot<I> {
    fn default() -> Self {
        Self {
            image: None,
            fill: [0.0; 4],
            border: [0.0; 4],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Thumb<I> {
    pub top: Option<I>,
    pub middle: Option<I>,
    pub bottom: Option<I>,
    pub caps: [f32; 2],
}

impl<I> Default for Thumb<I> {
    fn default() -> Self {
        Self {
            top: None,
            middle: None,
            bottom: None,
            caps: [0.0; 2],
        }
    }
}

#[derive(Debug, Clone)]
pub struct ListLook<I> {
    pub font: String,
    pub points: f32,
    pub text: [f32; 4],
    pub body: Slot<I>,
    pub selected: [Slot<I>; 2],
    pub up: [Option<I>; 2],
    pub down: [Option<I>; 2],
    pub track: Slot<I>,
    pub thumb: [Thumb<I>; 2],
    pub scroll_bar: bool,
    pub auto_hide: bool,
}

pub type ListStyle = ListLook<SurfaceIcon>;

impl<I> ListLook<I> {
    #[must_use]
    pub fn plain() -> Self {
        let selected = || Slot {
            image: None,
            fill: [0.3, 0.4, 0.5, 0.8],
            border: [0.0; 4],
        };
        Self {
            font: "Albertus MT".to_owned(),
            points: 10.0,
            text: [1.0; 4],
            body: Slot::default(),
            selected: [selected(), selected()],
            up: [None, None],
            down: [None, None],
            track: Slot {
                image: None,
                fill: [0.0; 4],
                border: [0.5, 0.67, 0.73, 1.0],
            },
            thumb: [Thumb::default(), Thumb::default()],
            scroll_bar: true,
            auto_hide: false,
        }
    }

    pub fn resolved<J>(self, mut resolve: impl FnMut(&I) -> Option<(J, [f32; 2])>) -> ListLook<J> {
        let mut slot = |s: Slot<I>| Slot {
            image: s.image.as_ref().and_then(&mut resolve).map(|(j, _)| j),
            fill: s.fill,
            border: s.border,
        };
        let [s0, s1] = self.selected;
        let (body, selected, track) = (slot(self.body), [slot(s0), slot(s1)], slot(self.track));
        let mut image = |i: Option<I>| i.as_ref().and_then(&mut resolve).map(|(j, _)| j);
        let [u0, u1] = self.up;
        let [d0, d1] = self.down;
        let (up, down) = ([image(u0), image(u1)], [image(d0), image(d1)]);
        let mut thumb = |t: Thumb<I>| {
            let top = t.top.as_ref().and_then(&mut resolve);
            let bottom = t.bottom.as_ref().and_then(&mut resolve);
            let cap = |p: &Option<(J, [f32; 2])>| {
                p.as_ref()
                    .map_or(0.0, |(_, [w, h])| if *w > 0.0 { h / w } else { 0.0 })
            };
            Thumb {
                caps: [cap(&top), cap(&bottom)],
                middle: t.middle.as_ref().and_then(&mut resolve).map(|(j, _)| j),
                top: top.map(|(j, _)| j),
                bottom: bottom.map(|(j, _)| j),
            }
        };
        let [t0, t1] = self.thumb;
        let thumb = [thumb(t0), thumb(t1)];
        ListLook {
            font: self.font,
            points: self.points,
            text: self.text,
            body,
            selected,
            up,
            down,
            track,
            thumb,
            scroll_bar: self.scroll_bar,
            auto_hide: self.auto_hide,
        }
    }
}

impl ListLook<String> {
    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        layout.windows.first().map_or_else(Self::plain, |window| {
            Self::read_window(window, ["ENABLEDDRAWDATA", "HILITEDRAWDATA"])
        })
    }

    #[must_use]
    pub fn read_window(window: &Window, body: [&str; 2]) -> Self {
        let mut look = Self::plain();
        if let Some(font) = window.font() {
            look.font = font.name;
            #[allow(clippy::cast_precision_loss)]
            let points = font.size as f32;
            if points > 0.0 {
                look.points = points;
            }
        }
        if let Some(colours) = window.text_colors() {
            look.text = unit(colours.enabled);
        }
        let slot = |key: &str, index: usize| -> Slot<String> {
            window
                .draw_data(key)
                .and_then(|d| d.into_iter().nth(index))
                .map_or_else(Slot::default, |item| Slot {
                    image: item.image,
                    fill: unit(item.color),
                    border: unit(item.border),
                })
        };
        look.body = slot(body[0], 0);
        look.selected = body.map(|key| Slot {
            image: slot(key, 3).image,
            ..slot(key, 1)
        });
        look.up = [
            slot("LISTBOXENABLEDUPBUTTONDRAWDATA", 0).image,
            slot("LISTBOXHILITEUPBUTTONDRAWDATA", 0).image,
        ];
        look.down = [
            slot("LISTBOXENABLEDDOWNBUTTONDRAWDATA", 0).image,
            slot("LISTBOXHILITEDOWNBUTTONDRAWDATA", 0).image,
        ];
        look.track = slot("LISTBOXENABLEDSLIDERDRAWDATA", 0);
        look.thumb = ["SLIDERTHUMBENABLEDDRAWDATA", "SLIDERTHUMBHILITEDRAWDATA"].map(|key| Thumb {
            top: slot(key, 0).image,
            middle: slot(key, 5).image,
            bottom: slot(key, 6).image,
            caps: [0.0; 2],
        });
        look.scroll_bar = window.list_box().is_none_or(|data| data.scroll_bar);
        look.auto_hide = window.window_type() == Some("SCROLLLISTBOX");
        look
    }
}

#[must_use]
pub fn unit(c: Rgba) -> [f32; 4] {
    c.map(|v| f32::from(v) / 255.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListFrame {
    pub bounds: [f32; 4],
    pub rows: [f32; 4],
    pub clip: [f32; 4],
    pub columns: Vec<[f32; 2]>,
    pub up: Option<[f32; 4]>,
    pub down: Option<[f32; 4]>,
    pub track: Option<[f32; 4]>,
}

impl ListFrame {
    #[must_use]
    pub fn new(bounds: [f32; 4], scroll_bar: bool, columns: &[ListColumn]) -> Self {
        let [l, t, r, b] = bounds;
        let width = r - l;
        let [bw, bh] = SCROLL_BUTTON;
        let (up, down, track, text_width, column_total) = if scroll_bar {
            (
                Some([r - bw - 2.0, t + 2.0, r - 2.0, t + 2.0 + bh]),
                Some([r - bw - 2.0, b - bh - 2.0, r - 2.0, b - 2.0]),
                Some([r - bw - 2.0, t + bh + 3.0, r - 2.0, b - bh - 3.0]),
                width - (bw + 3.0),
                width - bw,
            )
        } else {
            (None, None, None, width, width - 3.0)
        };
        let mut xs = Vec::new();
        if columns.len() <= 1 {
            xs.push([l, l + column_total]);
        } else {
            let mut x = l;
            for column in columns {
                let w = column.percent * column_total / 100.0;
                xs.push([x, x + w]);
                x += w;
            }
        }
        Self {
            bounds,
            rows: [l, t + ROWS_TOP, l + text_width, b],
            clip: [l + 1.0, t + 1.0, l + text_width - 1.0, b - 1.0],
            columns: xs,
            up,
            down,
            track,
        }
    }

    #[must_use]
    pub fn hiding_bar_while_content_fits(mut self, heights: &[f32]) -> Self {
        if row_offset(heights, heights.len()) <= self.display_height() {
            self.up = None;
            self.down = None;
            self.track = None;
        }
        self
    }

    #[must_use]
    pub fn display_height(&self) -> f32 {
        self.bounds[3] - self.bounds[1]
    }
}

#[must_use]
pub fn image_cell(size: Option<[f32; 2]>, natural: Option<[f32; 2]>, column: f32) -> [f32; 2] {
    size.unwrap_or_else(|| {
        let w = natural.map_or(column, |[w, _]| w.min(column));
        [w, w]
    })
}

#[must_use]
pub fn row_height(cells: impl IntoIterator<Item = f32>, line: f32) -> f32 {
    cells
        .into_iter()
        .fold(None, |tallest: Option<f32>, h| {
            Some(tallest.map_or(h, |t| t.max(h)))
        })
        .unwrap_or(line)
}

#[must_use]
pub fn text_top(row_top: f32, row_height: f32, text_height: f32) -> f32 {
    row_top + ((row_height - text_height) * 0.5).max(0.0)
}

#[must_use]
pub fn row_offset(heights: &[f32], entry: usize) -> f32 {
    heights.iter().take(entry).map(|h| h + 1.0).sum()
}

#[must_use]
pub fn top_entry(heights: &[f32], display_pos: f32) -> usize {
    let mut bottom = 0.0;
    for (i, h) in heights.iter().enumerate() {
        bottom += h + 1.0;
        if bottom > display_pos {
            return i;
        }
    }
    0
}

#[must_use]
pub fn max_display(heights: &[f32], display_height: f32) -> f32 {
    (row_offset(heights, heights.len()) - (display_height - 2.0) + 1.0).max(0.0)
}

#[must_use]
pub fn stepped(heights: &[f32], display_pos: f32, display_height: f32, rows: i32) -> f32 {
    let total = row_offset(heights, heights.len());
    if rows > 0 && display_pos + display_height > total {
        return display_pos;
    }
    if rows < 0 && display_pos <= 0.0 {
        return display_pos;
    }
    let Some(last) = heights.len().checked_sub(1) else {
        return 0.0;
    };
    let top = top_entry(heights, display_pos);
    let entry = if rows < 0 {
        top.saturating_sub(rows.unsigned_abs() as usize)
    } else {
        top.saturating_add(rows.unsigned_abs() as usize).min(last)
    };
    row_offset(heights, entry)
}

#[must_use]
pub fn revealed(heights: &[f32], display_pos: f32, display_height: f32, row: usize) -> f32 {
    let top = top_entry(heights, display_pos);
    if row < top {
        return row_offset(heights, row);
    }
    let bottom_edge = display_pos + display_height;
    let row_bottom = row_offset(heights, row) + heights.get(row).copied().unwrap_or(0.0);
    if row_bottom <= bottom_edge {
        return display_pos;
    }
    let mut on_screen = 0usize;
    let mut y = 0.0;
    for h in heights.iter().skip(top) {
        y += h + 1.0;
        if y > display_height {
            break;
        }
        on_screen += 1;
    }
    let back = (on_screen / 2).max(1);
    row_offset(heights, row.saturating_sub(back)).min(max_display(heights, display_height))
}

#[must_use]
pub fn thumb_drag(track: [f32; 4], grab: f32, p_y: f32, max: f32) -> f32 {
    if p_y < track[1] {
        return 0.0;
    }
    if p_y >= track[3] {
        return max;
    }
    let travel = (track[3] - track[1] - THUMB_HEIGHT).max(0.0);
    if travel <= 0.0 {
        return 0.0;
    }
    let top = (p_y - grab).clamp(track[1], track[1] + travel);
    ((top - track[1]) / travel * max).clamp(0.0, max)
}

#[must_use]
pub fn bar_row(tracking: bool, hover_row: Option<usize>, selected: Option<usize>) -> Option<usize> {
    if tracking {
        hover_row
    } else {
        selected
    }
}

#[must_use]
pub fn thumb_rect(track: [f32; 4], display_pos: f32, max: f32) -> [f32; 4] {
    let travel = (track[3] - track[1] - THUMB_HEIGHT).max(0.0);
    let at = if max > 0.0 {
        (display_pos / max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let top = track[1] + at * travel;
    [
        track[0],
        top,
        track[2],
        top + THUMB_HEIGHT.min(track[3] - track[1]),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListPart {
    Up,
    Down,
    Thumb,
    Track(f32),
    Row(usize),
    Body,
}

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
}

#[must_use]
pub fn hit(frame: &ListFrame, heights: &[f32], display_pos: f32, p: [f32; 2]) -> Option<ListPart> {
    if !inside(frame.bounds, p) {
        return None;
    }
    if frame.up.is_some_and(|r| inside(r, p)) {
        return Some(ListPart::Up);
    }
    if frame.down.is_some_and(|r| inside(r, p)) {
        return Some(ListPart::Down);
    }
    if let Some(track) = frame.track {
        let max = max_display(heights, frame.display_height());
        if inside(thumb_rect(track, display_pos, max), p) {
            return Some(ListPart::Thumb);
        }
        if inside(track, p) {
            let span = (track[3] - track[1]).max(f32::EPSILON);
            return Some(ListPart::Track(((p[1] - track[1]) / span).clamp(0.0, 1.0)));
        }
    }
    if inside(frame.clip, p) && p[0] < frame.rows[2] {
        let mut top = frame.rows[1] - display_pos;
        for (i, h) in heights.iter().enumerate() {
            let bottom = top + h + 1.0;
            if p[1] >= top && p[1] < bottom {
                return Some(ListPart::Row(i));
            }
            top = bottom;
        }
    }
    Some(ListPart::Body)
}

pub fn wrapped(text: &str, width: f32, mut measure: impl FnMut(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if line.is_empty() || measure(&candidate) <= width {
                line = candidate;
            } else {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            }
        }
        lines.push(line);
    }
    lines
}

#[derive(Debug, Clone)]
pub struct ListView {
    pub content: Arc<ListContent>,
    pub selected: Option<usize>,
    pub display_pos: f32,
    pub hover: Option<ListPart>,
    pub heights: Vec<f32>,
    pub frame: Option<ListFrame>,
    pub thumb_grab: Option<f32>,
    pub tracking: bool,
    pub hover_row: Option<usize>,
    reveal: bool,
}

impl ListView {
    #[must_use]
    pub fn new(content: Arc<ListContent>) -> Self {
        Self {
            selected: content.selected,
            reveal: content.selected.is_some(),
            content,
            display_pos: 0.0,
            hover: None,
            heights: Vec::new(),
            frame: None,
            thumb_grab: None,
            tracking: false,
            hover_row: None,
        }
    }

    pub fn set_content(&mut self, content: Arc<ListContent>) {
        if Arc::ptr_eq(&self.content, &content) {
            return;
        }
        if content.selected != self.selected {
            self.selected = content.selected;
            self.reveal = self.selected.is_some();
        }
        if content.scroll_to_top {
            self.display_pos = 0.0;
        }
        self.content = content;
    }

    pub fn laid_out(&mut self, frame: ListFrame, heights: Vec<f32>) {
        let display = frame.display_height();
        if self.reveal {
            if let Some(row) = self.selected {
                self.display_pos = revealed(&heights, self.display_pos, display, row);
            }
            self.reveal = false;
        }
        self.display_pos = self.display_pos.clamp(0.0, max_display(&heights, display));
        self.heights = heights;
        self.frame = Some(frame);
    }

    #[must_use]
    pub fn part_at(&self, p: [f32; 2]) -> Option<ListPart> {
        hit(self.frame.as_ref()?, &self.heights, self.display_pos, p)
    }

    pub fn click(&mut self, p: [f32; 2]) -> Option<usize> {
        let display = self.frame.as_ref()?.display_height();
        match self.part_at(p)? {
            ListPart::Up => self.scroll(-1),
            ListPart::Down => self.scroll(1),
            ListPart::Track(at) => {
                self.display_pos = at * max_display(&self.heights, display);
            }
            ListPart::Row(row) if self.selected != Some(row) => {
                self.selected = Some(row);
                return Some(row);
            }
            ListPart::Row(_) | ListPart::Thumb | ListPart::Body => {}
        }
        None
    }

    pub fn start_thumb_drag(&mut self, p: [f32; 2]) -> bool {
        let Some(frame) = &self.frame else {
            return false;
        };
        let Some(track) = frame.track else {
            return false;
        };
        let max = max_display(&self.heights, frame.display_height());
        let thumb = thumb_rect(track, self.display_pos, max);
        if !inside(thumb, p) {
            return false;
        }
        self.thumb_grab = Some(p[1] - thumb[1]);
        true
    }

    pub fn drag_thumb(&mut self, p: [f32; 2]) {
        let (Some(frame), Some(grab)) = (&self.frame, self.thumb_grab) else {
            return;
        };
        let Some(track) = frame.track else {
            return;
        };
        let max = max_display(&self.heights, frame.display_height());
        self.display_pos = thumb_drag(track, grab, p[1], max);
    }

    pub fn end_thumb_drag(&mut self) {
        self.thumb_grab = None;
    }

    pub fn scroll(&mut self, rows: i32) {
        if let Some(frame) = &self.frame {
            self.display_pos = stepped(
                &self.heights,
                self.display_pos,
                frame.display_height(),
                rows,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
