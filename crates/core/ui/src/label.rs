use mint_render::{TextureHandle, UiQuad};

use crate::input::Rect;
use crate::text::{layout_text, Font, FontId, GlyphAtlas};

#[derive(Debug, Clone, Copy)]
pub struct Label<'a> {
    pub rect: Rect,
    pub text: &'a str,
    pub color: [f32; 4],
}

impl<'a> Label<'a> {
    #[must_use]
    pub fn new(rect: Rect, text: &'a str) -> Self {
        Self {
            rect,
            text,
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }

    #[must_use]
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    #[must_use]
    pub fn quad(&self) -> UiQuad {
        let [x, y] = self.rect.min;
        let [max_x, max_y] = self.rect.max;
        UiQuad::new(x, y, (max_x - x).max(0.0), (max_y - y).max(0.0))
            .with_color_transform(self.color, [0.0, 0.0, 0.0, 0.0])
    }

    #[must_use]
    pub fn glyph_quads(
        &self,
        font_id: FontId,
        font: Option<&Font>,
        atlas: &mut GlyphAtlas,
        size_px: u32,
        texture: TextureHandle,
    ) -> Vec<UiQuad> {
        let baseline_y = font
            .and_then(|f| f.metrics(size_px).ok())
            .map_or(self.rect.min[1], |m| self.rect.min[1] + m.ascent);
        layout_text(
            self.text,
            [self.rect.min[0], baseline_y],
            self.color,
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

    #[test]
    fn a_label_draws_its_own_rect_filled_with_its_own_color() {
        let label = Label::new(
            Rect {
                min: [2.0, 3.0],
                max: [12.0, 9.0],
            },
            "Score: 0",
        )
        .with_color([0.1, 0.2, 0.3, 1.0]);
        let quad = label.quad();
        assert_eq!(quad.translation, [2.0, 3.0]);
        assert_eq!(quad.affine, [[10.0, 0.0], [0.0, 6.0]]);
        assert_eq!(quad.color_multiply, [0.1, 0.2, 0.3, 1.0]);
    }

    #[test]
    fn quad_never_reads_text() {
        let rect = Rect {
            min: [0.0, 0.0],
            max: [1.0, 1.0],
        };
        let a = Label::new(rect, "anything at all");
        let b = Label::new(rect, "");
        assert_eq!(a.quad(), b.quad());
    }

    fn empty_font() -> Font {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 8]);
        Font::from_bytes(bytes).unwrap()
    }

    fn dummy_texture() -> TextureHandle {
        let mut renderer = mint_render::Renderer::offscreen(4, 4).unwrap();
        let data = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).unwrap();
        renderer.upload_texture(&data).unwrap()
    }

    #[test]
    fn glyph_quads_never_panics_and_draws_tofu_when_the_font_has_no_glyphs() {
        let label = Label::new(
            Rect {
                min: [0.0, 0.0],
                max: [100.0, 20.0],
            },
            "Score: 0",
        );
        let font = empty_font();
        let mut atlas = GlyphAtlas::new(crate::text::GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = label.glyph_quads(FontId(0), Some(&font), &mut atlas, 12, dummy_texture());
        assert!(
            !quads.is_empty(),
            "every non-whitespace character in \"Score: 0\" must draw as tofu"
        );
        assert!(quads.iter().all(|q| q.texture.is_none()), "all tofu");
    }

    #[test]
    fn glyph_quads_with_no_font_at_all_still_draws_tofu() {
        let label = Label::new(
            Rect {
                min: [0.0, 0.0],
                max: [100.0, 20.0],
            },
            "hi",
        );
        let mut atlas = GlyphAtlas::new(crate::text::GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = label.glyph_quads(FontId(0), None, &mut atlas, 12, dummy_texture());
        assert_eq!(quads.len(), 2, "one tofu box per visible character");
    }
}
