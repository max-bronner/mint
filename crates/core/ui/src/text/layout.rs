use mint_render::{PixelFormat, Renderer, TextureData, TextureHandle, UiQuad};

use crate::text::atlas::{FontId, GlyphAtlas};
use crate::text::font::Font;

const TOFU_WIDTH_RATIO: f32 = 0.6;

fn default_metrics(size_px: u32) -> (f32, f32) {
    let size = f32::from(u16::try_from(size_px).unwrap_or(u16::MAX));
    (size * 0.8, size * 0.2)
}

fn tofu_quad(pen_x: f32, pen_y: f32, ascent: f32, descent: f32, color: [f32; 4]) -> UiQuad {
    let height = (ascent + descent).max(1.0);
    let width = height * TOFU_WIDTH_RATIO;
    UiQuad::new(pen_x, pen_y - ascent, width, height).with_color_transform(color, [0.0; 4])
}

fn char_advance(c: char, font: Option<&Font>, size_px: u32) -> f32 {
    let glyph = font.and_then(|f| f.glyph_for_char(c));
    if c.is_whitespace() {
        return font
            .zip(glyph)
            .and_then(|(f, g)| f.advance(g, size_px))
            .map_or_else(|| default_metrics(size_px).0 * 0.5, |a| a.0);
    }
    match (font, glyph) {
        (Some(f), Some(glyph_id)) => f.advance(glyph_id, size_px).map_or(0.0, |a| a.0),
        _ => {
            let (ascent, descent) = font
                .and_then(|f| f.metrics(size_px).ok())
                .map_or_else(|| default_metrics(size_px), |m| (m.ascent, m.descent));
            (ascent + descent).max(1.0) * TOFU_WIDTH_RATIO
        }
    }
}

#[must_use]
pub fn measure_text(text: &str, font: Option<&Font>, size_px: u32) -> f32 {
    text.chars().map(|c| char_advance(c, font, size_px)).sum()
}

#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn layout_text(
    text: &str,
    origin: [f32; 2],
    color: [f32; 4],
    font_id: FontId,
    font: Option<&Font>,
    atlas: &mut GlyphAtlas,
    size_px: u32,
    texture: TextureHandle,
) -> Vec<UiQuad> {
    let mut quads = Vec::new();
    let mut pen_x = origin[0];
    let pen_y = origin[1];

    for c in text.chars() {
        let glyph = font.and_then(|f| f.glyph_for_char(c));

        if c.is_whitespace() {
            pen_x += char_advance(c, font, size_px);
            continue;
        }

        let advance = char_advance(c, font, size_px);
        match (font, glyph) {
            (Some(f), Some(glyph_id)) => {
                if let Ok(slot) = atlas.glyph(font_id, f, size_px, glyph_id) {
                    if !slot.placeholder && slot.size_px[0] > 0.0 && slot.size_px[1] > 0.0 {
                        let x = pen_x + slot.bearing_px[0];
                        let y = pen_y + slot.bearing_px[1];
                        let quad = UiQuad::new(x, y, slot.size_px[0], slot.size_px[1])
                            .with_texture(texture, slot.uv_rect)
                            .with_color_transform(color, [0.0, 0.0, 0.0, 0.0]);
                        quads.push(quad);
                    }
                }
                pen_x += advance;
            }
            _ => {
                let (ascent, descent) = font
                    .and_then(|f| f.metrics(size_px).ok())
                    .map_or_else(|| default_metrics(size_px), |m| (m.ascent, m.descent));
                quads.push(tofu_quad(pen_x, pen_y, ascent, descent, color));
                pen_x += advance;
            }
        }
    }

    quads
}

pub fn sync_atlas_texture(
    atlas: &mut GlyphAtlas,
    page: &mut Option<TextureHandle>,
    renderer: &mut Renderer,
) -> Option<TextureHandle> {
    let (width, height) = atlas.dimensions();
    let (first_row, rows) = atlas.take_upload()?;
    match *page {
        Some(current)
            if renderer
                .update_texture_rows(current, first_row, rows)
                .is_ok() =>
        {
            return None;
        }
        Some(_) => {}
        None => {
            if let Ok(fresh) = renderer.zeroed_texture(width, height, PixelFormat::Rgba8Unorm) {
                if renderer.update_texture_rows(fresh, first_row, rows).is_ok() {
                    *page = Some(fresh);
                    return *page;
                }
            }
        }
    }
    let whole = TextureData::rgba8(width, height, atlas.pixels().to_vec())?;
    *page = renderer.upload_texture(&whole).ok();
    *page
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::atlas::GlyphAtlasConfig;

    fn dummy_texture() -> mint_render::TextureHandle {
        let mut renderer = mint_render::Renderer::offscreen(4, 4).unwrap();
        let data = mint_render::TextureData::rgba8(1, 1, vec![255, 255, 255, 255]).unwrap();
        renderer.upload_texture(&data).unwrap()
    }

    fn minimal_valid_font() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes.extend_from_slice(&0u16.to_be_bytes());
        bytes
    }

    #[test]
    fn empty_text_produces_no_quads_and_does_not_panic() {
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = layout_text(
            "",
            [0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            Some(&font),
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert!(quads.is_empty());
    }

    #[test]
    fn no_font_at_all_still_produces_a_tofu_quad_per_visible_character() {
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = layout_text(
            "hi",
            [0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            None,
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert_eq!(quads.len(), 2, "one tofu box per non-whitespace character");
        for quad in &quads {
            assert_eq!(
                quad.texture, None,
                "a tofu quad is untextured - the property that distinguishes \
                 it from a real, rasterised glyph"
            );
        }
        assert_ne!(
            quads[0].translation[0], quads[1].translation[0],
            "the pen must still advance between tofu boxes"
        );
    }

    #[test]
    fn a_font_with_no_matching_glyph_produces_tofu_not_a_skip() {
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = layout_text(
            "hello",
            [0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            Some(&font),
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert_eq!(
            quads.len(),
            5,
            "every codepoint is unmapped in this font, so every one is tofu"
        );
    }

    #[test]
    fn whitespace_never_becomes_tofu_with_or_without_a_font() {
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = layout_text(
            "  \t",
            [0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            None,
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert!(
            quads.is_empty(),
            "whitespace never draws, tofu or otherwise"
        );
    }

    #[test]
    fn text_with_no_mappable_glyphs_produces_tofu_boxes_not_an_empty_run() {
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        atlas.begin_frame();
        let quads = layout_text(
            "hello",
            [0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            FontId(0),
            Some(&font),
            &mut atlas,
            12,
            dummy_texture(),
        );
        assert!(
            !quads.is_empty(),
            "R8 supersedes I4's plain skip: an unmapped codepoint renders as \
             tofu, not nothing"
        );
    }

    #[test]
    fn measure_text_is_where_layout_text_leaves_the_pen() {
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        let mut atlas = GlyphAtlas::new(GlyphAtlasConfig::default_config()).unwrap();
        let texture = dummy_texture();
        let mut pen_gap = |text: &str| -> f32 {
            atlas.begin_frame();
            let quads = layout_text(
                text,
                [0.0, 0.0],
                [1.0; 4],
                FontId(0),
                Some(&font),
                &mut atlas,
                16,
                texture,
            );
            assert_eq!(quads.len(), 2, "{text:?} should draw exactly two boxes");
            quads[1].translation[0] - quads[0].translation[0]
        };
        let one = measure_text("a", Some(&font), 16);
        assert!(one > 0.0, "a visible character has a positive advance");
        assert!(
            (pen_gap("aa") - one).abs() < 0.01,
            "two adjacent glyphs sit measure_text(\"a\") = {one} apart, got {}",
            pen_gap("aa")
        );
        let with_space = measure_text("a ", Some(&font), 16);
        assert!(
            (pen_gap("a a") - with_space).abs() < 0.01,
            "a space between them widens the gap to measure_text(\"a \") =              {with_space}, got {}",
            pen_gap("a a")
        );
    }

    #[test]
    fn measure_text_counts_spaces() {
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        let tight = measure_text("ab", Some(&font), 16);
        let spaced = measure_text("a b", Some(&font), 16);
        assert!(
            spaced > tight,
            "a space advances the pen: {spaced} should exceed {tight}"
        );
    }

    #[test]
    fn measure_text_of_nothing_is_zero_with_and_without_a_font() {
        assert_eq!(measure_text("", None, 16), 0.0);
        let font = Font::from_bytes(minimal_valid_font()).unwrap();
        assert_eq!(measure_text("", Some(&font), 16), 0.0);
    }
}
