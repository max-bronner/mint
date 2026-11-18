use mint_sage_wnd::{Layout, Window};

use super::check::State;
use super::list::unit;
use crate::host::SurfaceIcon;

pub const BAR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub const LINE: [f32; 4] = [50.0 / 255.0, 50.0 / 255.0, 50.0 / 255.0, 1.0];

pub type MapWindowStyle = MapWindowLook<SurfaceIcon>;

#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub at: [f32; 2],
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MapWindowContent {
    pub backdrop: Option<SurfaceIcon>,
    pub picture: Option<SurfaceIcon>,
    pub extent: [f32; 2],
    pub spots: Vec<Spot>,
    pub enabled: bool,
}

impl Default for MapWindowContent {
    fn default() -> Self {
        Self {
            backdrop: None,
            picture: None,
            extent: [1.0, 1.0],
            spots: Vec::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapWindowLook<I> {
    pub font: String,
    pub points: f32,
    pub text: [[f32; 4]; 3],
    pub spot: [Option<I>; 3],
    pub pushed: Option<I>,
    pub spot_size: [f32; 2],
    pub creation: [f32; 2],
}

impl<I> MapWindowLook<I> {
    #[must_use]
    pub fn plain() -> Self {
        Self {
            font: "Albertus MT".to_owned(),
            points: super::combo::TEXT_POINTS,
            text: [[1.0; 4], [0.0, 0.77, 0.0, 1.0], [0.75, 0.75, 0.75, 1.0]],
            spot: [None, None, None],
            pushed: None,
            spot_size: [16.0, 17.0],
            creation: [800.0, 600.0],
        }
    }

    pub fn resolved<J>(self, mut resolve: impl FnMut(&I) -> Option<J>) -> MapWindowLook<J> {
        MapWindowLook {
            font: self.font,
            points: self.points,
            text: self.text,
            spot: self.spot.map(|image| image.as_ref().and_then(&mut resolve)),
            pushed: self.pushed.as_ref().and_then(&mut resolve),
            spot_size: self.spot_size,
            creation: self.creation,
        }
    }

    #[must_use]
    pub fn spot_units(&self, stage: [f32; 2]) -> [f32; 2] {
        let per = |i: usize| {
            let creation = self.creation.get(i).copied().unwrap_or(1.0).max(1.0);
            self.spot_size.get(i).copied().unwrap_or(0.0) * stage.get(i).copied().unwrap_or(0.0)
                / creation
        };
        [per(0), per(1)]
    }
}

impl MapWindowLook<String> {
    #[must_use]
    pub fn read(layout: &Layout) -> Self {
        let mut look = Self::plain();
        let Some(window) = layout.windows.first() else {
            return look;
        };
        let Some(button) = window
            .children
            .iter()
            .find(|c| c.window_type() == Some("PUSHBUTTON"))
        else {
            return look;
        };
        if let Some(font) = button.font() {
            look.font = font.name;
        }
        if let Some(c) = button.text_colors() {
            look.text = [unit(c.enabled), unit(c.hilite), unit(c.disabled)];
        }
        if let Some(rect) = button.screen_rect() {
            #[allow(clippy::cast_precision_loss)]
            let size = |i: usize| {
                let (Some(a), Some(b)) = (rect.upper_left.get(i), rect.bottom_right.get(i)) else {
                    return 0.0;
                };
                b.saturating_sub(*a).max(0) as f32
            };
            look.spot_size = [size(0), size(1)];
            #[allow(clippy::cast_precision_loss)]
            let creation = rect.creation_resolution.map(|v| v.max(1) as f32);
            look.creation = creation;
        }
        let image = |window: &Window, key: &str, index: usize| {
            window
                .draw_data(key)
                .and_then(|d| d.into_iter().nth(index))
                .and_then(|item| item.image)
        };
        look.spot = ["ENABLEDDRAWDATA", "HILITEDRAWDATA", "DISABLEDDRAWDATA"]
            .map(|key| image(button, key, 0));
        look.pushed = image(button, "ENABLEDDRAWDATA", 1);
        look
    }
}

#[must_use]
pub fn inset_half_texel(uv_rect: [f32; 4], texels: [f32; 2]) -> [f32; 4] {
    let [x, y, w, h] = uv_rect;
    let half = |extent: f32, n: f32| if n > 1.0 { extent * 0.5 / n } else { 0.0 };
    let (dx, dy) = (half(w, texels[0]), half(h, texels[1]));
    [x + dx, y + dy, w - 2.0 * dx, h - 2.0 * dy]
}

#[must_use]
pub fn picture_rect(bounds: [f32; 4], extent: [f32; 2]) -> [f32; 4] {
    let [l, t, r, b] = bounds;
    let (w, h) = ((r - l).max(0.0), (b - t).max(0.0));
    let [ew, eh] = extent;
    if !(ew > 0.0 && eh > 0.0 && w > 0.0 && h > 0.0) {
        return bounds;
    }
    let (ratio_w, ratio_h) = (ew / w, eh / h);
    if ratio_w >= ratio_h {
        let height = eh / ratio_w;
        let top = t + (h - height) * 0.5;
        [l, top, r, top + height]
    } else {
        let width = ew / ratio_h;
        let left = l + (w - width) * 0.5;
        [left, t, left + width, b]
    }
}

#[must_use]
pub fn spot_rects(picture: [f32; 4], spots: &[Spot], size: [f32; 2]) -> Vec<[f32; 4]> {
    let [l, t, r, b] = picture;
    let (w, h) = (r - l, b - t);
    let [sw, sh] = size;
    let mut rects: Vec<[f32; 4]> = Vec::with_capacity(spots.len());
    for spot in spots {
        let mut x = l + spot.at[0] * w - sw * 0.5;
        let mut y = t + (1.0 - spot.at[1]) * h - sh * 0.5;
        for other in &rects {
            if x > other[0] && x < other[2] && y > other[1] && y < other[3] {
                let right = other[2] - x;
                let bottom = other[3] - y;
                if right < bottom {
                    x = other[2] + 1.0;
                } else if bottom < right {
                    y = other[3] + 1.0;
                } else {
                    x = other[2] + 1.0;
                    y = other[3] + 1.0;
                }
            }
        }
        rects.push([x, y, x + sw, y + sh]);
    }
    rects
}

#[derive(Debug, Clone)]
pub struct MapWindowView {
    pub content: std::sync::Arc<MapWindowContent>,
    pub rects: Vec<[f32; 4]>,
    pub hover: Option<usize>,
}

impl MapWindowView {
    #[must_use]
    pub fn new(content: std::sync::Arc<MapWindowContent>) -> Self {
        Self {
            content,
            rects: Vec::new(),
            hover: None,
        }
    }

    #[must_use]
    pub fn spot_at(&self, p: [f32; 2]) -> Option<usize> {
        if !self.content.enabled {
            return None;
        }
        self.rects
            .iter()
            .rposition(|r| p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3])
    }

    #[must_use]
    pub fn state(&self, index: usize) -> State {
        State::of(self.content.enabled, self.hover == Some(index))
    }
}

#[cfg(test)]
mod tests;
