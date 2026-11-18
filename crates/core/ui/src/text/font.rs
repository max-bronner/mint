use std::fmt;

use skrifa::MetadataProvider;

use crate::text::error::{Error, Result};
use crate::text::outline::{ContourPen, GlyphOutline};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Advance(pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub win_ascent: f32,
    pub win_descent: f32,
}

impl FontMetrics {
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

pub const MAX_SIZE_PX: u32 = 256;

pub struct Font {
    bytes: Vec<u8>,
}

impl fmt::Debug for Font {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Font")
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

fn descent_magnitude(raw: f32) -> f32 {
    raw.abs()
}

fn win_scaled(ascent: u16, descent: u16, upem: u16, size_px: u32) -> (f32, f32) {
    if upem == 0 {
        return (0.0, 0.0);
    }
    let scale = px(size_px) / f32::from(upem);
    (f32::from(ascent) * scale, f32::from(descent) * scale)
}

impl Font {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        skrifa::FontRef::new(&bytes).map_err(|e| Error::InvalidFont(Box::new(e)))?;
        Ok(Self { bytes })
    }

    fn font_ref(&self) -> Result<skrifa::FontRef<'_>> {
        skrifa::FontRef::new(&self.bytes).map_err(|e| Error::InvalidFont(Box::new(e)))
    }

    pub fn metrics(&self, size_px: u32) -> Result<FontMetrics> {
        check_size(size_px)?;
        let font = self.font_ref()?;
        let m = font.metrics(
            skrifa::instance::Size::new(px(size_px)),
            skrifa::instance::LocationRef::default(),
        );
        let win = Self::win_metrics(&font, size_px);
        Ok(FontMetrics {
            ascent: m.ascent,
            descent: descent_magnitude(m.descent),
            line_gap: m.leading.max(0.0),
            win_ascent: win.map_or(m.ascent, |w| w.0),
            win_descent: win.map_or(descent_magnitude(m.descent), |w| w.1),
        })
    }

    fn win_metrics(font: &skrifa::FontRef<'_>, size_px: u32) -> Option<(f32, f32)> {
        use skrifa::raw::TableProvider;
        let os2 = font.os2().ok()?;
        let upem = font.head().ok()?.units_per_em();
        Some(win_scaled(
            os2.us_win_ascent(),
            os2.us_win_descent(),
            upem,
            size_px,
        ))
    }

    #[must_use]
    pub fn glyph_for_char(&self, c: char) -> Option<GlyphId> {
        let font = self.font_ref().ok()?;
        font.charmap().map(c).map(|g| GlyphId(g.to_u32()))
    }

    #[must_use]
    pub fn advance(&self, glyph: GlyphId, size_px: u32) -> Option<Advance> {
        if check_size(size_px).is_err() {
            return None;
        }
        let font = self.font_ref().ok()?;
        let metrics = font.glyph_metrics(
            skrifa::instance::Size::new(px(size_px)),
            skrifa::instance::LocationRef::default(),
        );
        metrics
            .advance_width(skrifa::GlyphId::new(glyph.0))
            .map(Advance)
    }

    pub fn outline(&self, glyph: GlyphId, size_px: u32) -> Result<GlyphOutline> {
        check_size(size_px)?;
        let font = self.font_ref()?;
        let skrifa_id = skrifa::GlyphId::new(glyph.0);
        let outline = font
            .outline_glyphs()
            .get(skrifa_id)
            .ok_or(Error::NoOutline(glyph))?;
        let settings = skrifa::outline::DrawSettings::unhinted(
            skrifa::instance::Size::new(px(size_px)),
            skrifa::instance::LocationRef::default(),
        );
        let mut pen = ContourPen::new();
        outline
            .draw(settings, &mut pen)
            .map_err(|e| Error::DrawFailed(glyph, Box::new(e)))?;
        pen.finish(glyph)
    }
}

#[allow(clippy::cast_precision_loss)]
fn px(size_px: u32) -> f32 {
    size_px as f32
}

fn check_size(size_px: u32) -> Result<()> {
    if size_px == 0 || size_px > MAX_SIZE_PX {
        return Err(Error::SizeOutOfRange(size_px, MAX_SIZE_PX));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_bytes_are_rejected_not_panicked_on() {
        let err = Font::from_bytes(vec![0, 1, 2, 3]).unwrap_err();
        assert!(matches!(err, Error::InvalidFont(_)));
    }

    #[test]
    fn an_empty_file_is_rejected() {
        let err = Font::from_bytes(Vec::new()).unwrap_err();
        assert!(matches!(err, Error::InvalidFont(_)));
    }

    #[test]
    fn a_negative_descender_becomes_a_positive_magnitude() {
        assert!((descent_magnitude(-3.5) - 3.5).abs() < f32::EPSILON);
    }

    #[test]
    fn an_already_positive_descender_is_unchanged() {
        assert!((descent_magnitude(3.5) - 3.5).abs() < f32::EPSILON);
        assert!(descent_magnitude(0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn win_metrics_scale_by_size_over_the_em() {
        let (a, d) = win_scaled(946, 250, 1000, 14);
        assert!((a - 13.244).abs() < 1e-4, "{a}");
        assert!((d - 3.5).abs() < 1e-4, "{d}");
        assert_eq!(win_scaled(946, 250, 0, 14), (0.0, 0.0));
    }

    #[test]
    fn size_zero_and_size_past_the_cap_are_rejected() {
        assert!(matches!(
            check_size(0),
            Err(Error::SizeOutOfRange(0, MAX_SIZE_PX))
        ));
        assert!(matches!(
            check_size(MAX_SIZE_PX + 1),
            Err(Error::SizeOutOfRange(s, MAX_SIZE_PX)) if s == MAX_SIZE_PX + 1
        ));
        assert!(check_size(1).is_ok());
        assert!(check_size(MAX_SIZE_PX).is_ok());
    }
}
