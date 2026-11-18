use mint_render::{TextureHandle, UiQuad};

use crate::input::{HitTarget, InputRouter, Rect, WidgetId};
use crate::text::{layout_text, Font, FontId, GlyphAtlas};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonStyle {
    pub idle: [f32; 4],
    pub hovered: [f32; 4],
    pub pressed: [f32; 4],
}

impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            idle: [0.25, 0.25, 0.28, 1.0],
            hovered: [0.35, 0.35, 0.40, 1.0],
            pressed: [0.15, 0.15, 0.18, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Button<'a> {
    pub id: WidgetId,
    pub rect: Rect,
    pub text: &'a str,
    pub style: ButtonStyle,
}

impl<'a> Button<'a> {
    #[must_use]
    pub fn new(id: WidgetId, rect: Rect, text: &'a str) -> Self {
        Self {
            id,
            rect,
            text,
            style: ButtonStyle::default(),
        }
    }

    #[must_use]
    pub fn with_style(mut self, style: ButtonStyle) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub fn hit_target(&self) -> HitTarget {
        HitTarget {
            id: self.id,
            region: self.rect,
            focusable: true,
        }
    }

    #[must_use]
    pub fn quad(&self, router: &InputRouter) -> UiQuad {
        let color = if router.captured() == Some(self.id) {
            self.style.pressed
        } else if router.hovered() == Some(self.id) {
            self.style.hovered
        } else {
            self.style.idle
        };
        let [x, y] = self.rect.min;
        let [max_x, max_y] = self.rect.max;
        UiQuad::new(x, y, (max_x - x).max(0.0), (max_y - y).max(0.0))
            .with_color_transform(color, [0.0, 0.0, 0.0, 0.0])
    }

    #[must_use]
    pub fn glyph_quads(
        &self,
        color: [f32; 4],
        font_id: FontId,
        font: Option<&Font>,
        atlas: &mut GlyphAtlas,
        size_px: u32,
        texture: TextureHandle,
    ) -> Vec<UiQuad> {
        let height = (self.rect.max[1] - self.rect.min[1]).max(0.0);
        let baseline_y = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or(self.rect.min[1] + height * 0.5, |m| {
                self.rect.min[1] + (height + m.ascent - m.descent) * 0.5
            });
        layout_text(
            self.text,
            [self.rect.min[0], baseline_y],
            color,
            font_id,
            font,
            atlas,
            size_px,
            texture,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::PointerButton;

    fn rect(min: [f32; 2], max: [f32; 2]) -> Rect {
        Rect { min, max }
    }

    #[test]
    fn an_unrouted_button_paints_idle() {
        let button = Button::new(WidgetId(1), rect([0.0, 0.0], [10.0, 10.0]), "Start");
        let router = InputRouter::new();
        assert_eq!(
            button.quad(&router).color_multiply,
            ButtonStyle::default().idle
        );
    }

    #[test]
    fn hovering_paints_hovered_and_capturing_paints_pressed() {
        let button = Button::new(WidgetId(1), rect([0.0, 0.0], [10.0, 10.0]), "Start");
        let targets = [button.hit_target()];
        let mut router = InputRouter::new();

        router.pointer_moved(&targets, [5.0, 5.0]);
        assert_eq!(
            button.quad(&router).color_multiply,
            ButtonStyle::default().hovered,
            "hovered but not pressed"
        );

        router.pointer_button(&targets, [5.0, 5.0], PointerButton::Primary, true);
        assert_eq!(
            button.quad(&router).color_multiply,
            ButtonStyle::default().pressed,
            "captured by a press"
        );

        router.pointer_button(&targets, [5.0, 5.0], PointerButton::Primary, false);
        assert_eq!(
            button.quad(&router).color_multiply,
            ButtonStyle::default().hovered,
            "released while still hovered: back to hovered, not idle"
        );
    }

    #[test]
    fn a_different_buttons_hover_does_not_paint_this_one() {
        let a = Button::new(WidgetId(1), rect([0.0, 0.0], [10.0, 10.0]), "A");
        let b = Button::new(WidgetId(2), rect([10.0, 0.0], [20.0, 10.0]), "B");
        let targets = [a.hit_target(), b.hit_target()];
        let mut router = InputRouter::new();

        router.pointer_moved(&targets, [15.0, 5.0]);
        assert_eq!(a.quad(&router).color_multiply, ButtonStyle::default().idle);
        assert_eq!(
            b.quad(&router).color_multiply,
            ButtonStyle::default().hovered
        );
    }

    fn empty_font() -> crate::text::Font {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        crate::text::Font::from_bytes(bytes).unwrap()
    }

    fn dummy_texture() -> mint_render::TextureHandle {
        let mut renderer = mint_render::Renderer::offscreen(4, 4).unwrap();
        let data = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).unwrap();
        renderer.upload_texture(&data).unwrap()
    }

    #[test]
    fn glyph_quads_never_panics_and_draws_tofu_when_the_font_has_no_glyphs() {
        let button = Button::new(WidgetId(1), rect([0.0, 0.0], [80.0, 20.0]), "Start");
        let font = empty_font();
        let mut atlas =
            crate::text::GlyphAtlas::new(crate::text::GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = button.glyph_quads(
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            Some(&font),
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert!(
            !quads.is_empty(),
            "every non-whitespace character in \"Start\" must draw as tofu"
        );
    }

    #[test]
    fn glyph_quads_with_no_font_at_all_still_draws_tofu() {
        let button = Button::new(WidgetId(1), rect([0.0, 0.0], [80.0, 20.0]), "Hi");
        let mut atlas =
            crate::text::GlyphAtlas::new(crate::text::GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = button.glyph_quads(
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            None,
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert_eq!(quads.len(), 2, "one tofu box per visible character");
    }
}
