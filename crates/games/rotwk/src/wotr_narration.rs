use mint_render::{Renderer, TextureData, TextureHandle, UiQuad};
use mint_ui::{Font, FontId, GlyphAtlas, GlyphAtlasConfig};
use std::time::Duration;

pub(crate) const FONT: &str = "SachaWynter";
const FONT_POINTS: f32 = 14.0;
const FONT_ID: FontId = FontId(0);

const FRAMES_PER_SECOND: u64 = 30;
const REVEAL_FRAMES: u64 = FRAMES_PER_SECOND / 4 + 1;
const LINE_STAGGER_FRAMES: u64 = FRAMES_PER_SECOND;
const FADE_FRAMES: u64 = FRAMES_PER_SECOND;
const UNTIMED_FADE_AFTER_FRAMES: u64 = 15 * FRAMES_PER_SECOND;
const LINE_HEIGHT: f32 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Area {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) wrap: f32,
}

pub(crate) const DEFAULT_AREA: Area = Area {
    x: 0.019_531_25,
    y: 0.162_760_42,
    wrap: 0.960_937_5,
};

pub(crate) const DIALOG_AREA: Area = Area {
    x: 0.380_859,
    y: 0.721_354,
    wrap: 0.599_609,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placed {
    pub(crate) text: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct Shown {
    text: String,
    started: u64,
    clears: Option<u64>,
}

pub(crate) struct Narration {
    area: Area,
    shown: Option<Shown>,
    frame: u64,
    carry: f32,
    atlas: Option<GlyphAtlas>,
    texture: Option<TextureHandle>,
    page: Option<TextureHandle>,
    quads: Vec<UiQuad>,
}

impl Default for Narration {
    fn default() -> Self {
        Self {
            area: DEFAULT_AREA,
            shown: None,
            frame: 0,
            carry: 0.0,
            atlas: GlyphAtlas::new(GlyphAtlasConfig::default_config()).ok(),
            texture: None,
            page: None,
            quads: Vec::new(),
        }
    }
}

pub(crate) fn lines(text: &str, wrap: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let joined = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && measure(&joined) > wrap {
                out.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = joined;
            }
        }
        out.push(line);
    }
    out
}

impl Narration {
    pub(crate) fn show(&mut self, text: &str, seconds: f32) {
        let clears = (seconds > 0.0).then(|| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let frames = (f64::from(seconds) * FRAMES_PER_SECOND as f64).min(1e12) as u64;
            self.frame.saturating_add(frames)
        });
        self.shown = Some(Shown {
            text: text.to_owned(),
            started: self.frame,
            clears,
        });
    }

    pub(crate) fn clear(&mut self) {
        self.shown = None;
    }

    pub(crate) fn set_area(&mut self, area: Area) {
        self.area = area;
    }

    #[cfg(test)]
    pub(crate) fn area(&self) -> Area {
        self.area
    }

    #[cfg(test)]
    pub(crate) fn is_showing(&self) -> bool {
        self.shown.is_some()
    }

    pub(crate) fn advance(&mut self, dt: Duration) {
        self.carry += dt.as_secs_f32() * FRAMES_PER_SECOND as f32;
        let whole = self.carry.floor();
        self.carry -= whole;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = whole as u64;
        self.frame = self.frame.saturating_add(frames);
        if self
            .shown
            .as_ref()
            .and_then(|s| s.clears)
            .is_some_and(|clears| self.frame >= clears)
        {
            self.shown = None;
        }
    }

    fn alpha(&self, shown: &Shown, index: usize) -> f32 {
        let start = shown
            .started
            .saturating_add((index as u64).saturating_mul(LINE_STAGGER_FRAMES));
        let revealed = self.frame.saturating_sub(start);
        let fade_in = if self.frame < start {
            0.0
        } else {
            (revealed as f32 / REVEAL_FRAMES as f32).min(1.0)
        };
        let fade_out = match shown.clears {
            Some(clears) => clears.saturating_sub(self.frame) as f32 / FADE_FRAMES as f32,
            None => {
                let fading = self
                    .frame
                    .saturating_sub(shown.started.saturating_add(UNTIMED_FADE_AFTER_FRAMES));
                1.0 - fading as f32 / FADE_FRAMES as f32
            }
        };
        (fade_in * fade_out.clamp(0.0, 1.0)).clamp(0.0, 1.0)
    }

    pub(crate) fn placed(
        &self,
        window: (f32, f32),
        px: f32,
        measure: &dyn Fn(&str) -> f32,
    ) -> Vec<Placed> {
        let Some(shown) = &self.shown else {
            return Vec::new();
        };
        let height = px * LINE_HEIGHT;
        lines(&shown.text, window.0 * self.area.wrap, measure)
            .into_iter()
            .enumerate()
            .map(|(i, text)| Placed {
                text,
                x: window.0 * self.area.x,
                y: window.1 * self.area.y + height * i as f32,
                alpha: self.alpha(shown, i),
            })
            .collect()
    }

    pub(crate) fn build(&mut self, renderer: &mut Renderer, font: Option<&Font>) {
        self.quads.clear();
        if self.shown.is_none() {
            return;
        }
        if self.texture.is_none() {
            self.texture = TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
                .and_then(|data| renderer.upload_texture(&data).ok());
        }
        let (w, h) = renderer.size();
        let window = (
            f32::from(u16::try_from(w).unwrap_or(u16::MAX)),
            f32::from(u16::try_from(h).unwrap_or(u16::MAX)),
        );
        let size_px = crate::visuals::caption_text_px(FONT_POINTS, w);
        #[allow(clippy::cast_precision_loss)]
        let px = size_px as f32;
        let ascent = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or(px * 0.8, |m| m.win_ascent);
        let measure = |text: &str| mint_ui::measure_text(text, font, size_px);
        let placed = self.placed(window, px, &measure);
        let (Some(atlas), Some(texture)) = (self.atlas.as_mut(), self.texture) else {
            return;
        };
        for line in placed.iter().filter(|l| l.alpha > 0.0) {
            for (dx, dy, rgb) in [(1.0, 1.0, 0.0), (0.0, 0.0, 1.0)] {
                self.quads.extend(mint_ui::layout_text(
                    &line.text,
                    [line.x + dx, line.y + ascent + dy],
                    [rgb, rgb, rgb, line.alpha],
                    FONT_ID,
                    font,
                    atlas,
                    size_px,
                    texture,
                ));
            }
        }
        if atlas.take_dirty() {
            if let Some(page) = mint_ui::sync_atlas_texture(atlas, &mut self.page, renderer) {
                self.texture = Some(page);
                for quad in &mut self.quads {
                    if quad.texture == Some(texture) {
                        quad.texture = Some(page);
                    }
                }
            }
        }
    }

    pub(crate) fn quads(&self) -> &[UiQuad] {
        &self.quads
    }
}

#[cfg(test)]
mod tests;
