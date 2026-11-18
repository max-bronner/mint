use mint_sage_wnd::Layout;

use crate::host::SurfaceIcon;

pub type SliderStyle = SliderLook<SurfaceIcon>;

pub const THUMB: [f32; 2] = [13.0, 16.0];
pub const SEGMENT_GAP: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderContent {
    pub value: i32,
    pub enabled: bool,
}

impl Default for SliderContent {
    fn default() -> Self {
        Self {
            value: 0,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SliderLook<I> {
    pub filled: Option<I>,
    pub empty: Option<I>,
    pub outline: Option<I>,
    pub thumb: [Option<I>; 2],
    pub segment: [f32; 2],
    pub range: [i32; 2],
}

impl<I> SliderLook<I> {
    #[must_use]
    pub fn plain() -> Self {
        Self {
            filled: None,
            empty: None,
            outline: None,
            thumb: [None, None],
            segment: [6.0, 24.0],
            range: [0, 100],
        }
    }

    pub fn resolved<J>(
        self,
        mut resolve: impl FnMut(&I) -> Option<(J, [f32; 2])>,
    ) -> SliderLook<J> {
        let filled = self.filled.as_ref().and_then(&mut resolve);
        let segment = filled.as_ref().map_or(self.segment, |(_, size)| *size);
        let mut image = |i: Option<I>| i.as_ref().and_then(&mut resolve).map(|(j, _)| j);
        let [t0, t1] = self.thumb;
        SliderLook {
            empty: image(self.empty),
            outline: image(self.outline),
            thumb: [image(t0), image(t1)],
            filled: filled.map(|(j, _)| j),
            segment,
            range: self.range,
        }
    }
}

impl SliderLook<String> {
    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        let mut look = Self::plain();
        let Some(window) = layout.windows.first() else {
            return look;
        };
        let image = |key: &str, index: usize| {
            window
                .draw_data(key)
                .and_then(|d| d.into_iter().nth(index))
                .and_then(|item| item.image)
        };
        look.filled = image("DISABLEDDRAWDATA", 0);
        look.empty = image("DISABLEDDRAWDATA", 1);
        look.outline = image("HILITEDRAWDATA", 0);
        look.thumb = [
            image("SLIDERTHUMBENABLEDDRAWDATA", 0),
            image("SLIDERTHUMBHILITEDRAWDATA", 0),
        ];
        if let Some(data) = window.slider() {
            if data.max > data.min {
                look.range = [data.min, data.max];
            }
        }
        look
    }
}

#[must_use]
pub fn units_per_step(width: f32, range: [i32; 2]) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let steps = (range[1] - range[0]).max(1) as f32;
    (width - THUMB[0]).max(0.0) / steps
}

#[must_use]
pub fn thumb_rect(bounds: [f32; 4], value: i32, range: [i32; 2]) -> [f32; 4] {
    let per = units_per_step(bounds[2] - bounds[0], range);
    #[allow(clippy::cast_precision_loss)]
    let x = bounds[0] + (value.clamp(range[0], range[1]) - range[0]) as f32 * per;
    let y = bounds[1] + THUMB[1] * 2.0 / 3.0;
    [x, y, x + THUMB[0], y + THUMB[1]]
}

#[must_use]
pub fn value_at(bounds: [f32; 4], x: f32, range: [i32; 2]) -> i32 {
    let [l, _, r, _] = bounds;
    let half = THUMB[0] * 0.5;
    if x >= r - half {
        return range[1];
    }
    if x < l + half {
        return range[0];
    }
    let per = units_per_step(r - l, range);
    if per <= 0.0 {
        return range[0];
    }
    #[allow(clippy::cast_possible_truncation)]
    let steps = ((x - l - half) / per) as i32;
    (range[0] + steps).clamp(range[0], range[1])
}

#[must_use]
pub fn clicked_value(bounds: [f32; 4], x: f32, value: i32, range: [i32; 2]) -> i32 {
    let thumb = thumb_rect(bounds, value, range);
    let centre = (thumb[0] + thumb[2]) * 0.5;
    let page = (bounds[2] - bounds[0]) / 5.0;
    let target = if x >= centre {
        (centre + page).min(x)
    } else {
        (centre - page).max(x)
    };
    value_at(bounds, target, range)
}

#[must_use]
pub fn segments(bounds: [f32; 4], segment: f32, value: i32, range: [i32; 2]) -> (Vec<f32>, usize) {
    let [l, _, r, _] = bounds;
    let width = r - l;
    if segment <= 0.0 || width <= 0.0 {
        return (Vec::new(), 0);
    }
    #[allow(clippy::cast_precision_loss)]
    let filled_to = l + (value - range[0]) as f32 / ((range[1] - range[0]).max(1) as f32) * width;
    let mut starts = Vec::new();
    let mut filled = 0usize;
    let mut start = l;
    while start + segment < r && starts.len() < 4096 {
        if start <= filled_to && value != range[0] {
            filled += 1;
        }
        starts.push(start);
        start += segment + SEGMENT_GAP;
    }
    let covered = starts.last().map_or(0.0, |last| last + segment - l);
    let shift = (width - covered) * 0.5;
    (starts.into_iter().map(|x| x + shift).collect(), filled)
}

#[derive(Debug, Clone, Copy)]
pub struct SliderView {
    pub content: SliderContent,
    pub value: i32,
    pub hover: bool,
    pub dragging: bool,
    pub bounds: Option<[f32; 4]>,
    pub range: [i32; 2],
}

impl SliderView {
    #[must_use]
    pub fn new(content: SliderContent) -> Self {
        Self {
            content,
            value: content.value,
            hover: false,
            dragging: false,
            bounds: None,
            range: [0, 100],
        }
    }

    pub fn set_content(&mut self, content: SliderContent) {
        if content != self.content {
            self.value = content.value;
            self.content = content;
        }
    }

    #[must_use]
    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.bounds
            .is_some_and(|b| p[0] >= b[0] && p[0] < b[2] && p[1] >= b[1] && p[1] < b[3])
    }

    pub fn click(&mut self, p: [f32; 2]) -> Option<i32> {
        let bounds = self.bounds?;
        if !self.content.enabled || !self.contains(p) {
            return None;
        }
        let value = clicked_value(bounds, p[0], self.value, self.range);
        (value != self.value).then(|| {
            self.value = value;
            value
        })
    }

    pub fn drag(&mut self, p: [f32; 2]) -> Option<i32> {
        let bounds = self.bounds?;
        if !self.content.enabled {
            return None;
        }
        let value = value_at(bounds, p[0], self.range);
        (value != self.value).then(|| {
            self.value = value;
            value
        })
    }
}

#[cfg(test)]
mod tests;
