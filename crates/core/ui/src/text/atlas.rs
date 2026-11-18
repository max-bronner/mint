use std::collections::{HashMap, HashSet};

use crate::text::error::{Error, Result};
use crate::text::font::{Font, GlyphId};
use crate::text::raster::{self, RasterizedGlyph};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

pub const MAX_ATLAS_DIMENSION: u32 = 4096;

type CacheKey = (FontId, u32, GlyphId);

#[derive(Debug, Clone, Copy)]
struct Cell {
    key: Option<CacheKey>,
    touch: Option<u64>,
    width: u32,
    height: u32,
    bearing_x: f32,
    bearing_y: f32,
}

impl Cell {
    const EMPTY: Cell = Cell {
        key: None,
        touch: None,
        width: 0,
        height: 0,
        bearing_x: 0.0,
        bearing_y: 0.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphAtlasConfig {
    pub page_width: u32,
    pub page_height: u32,
    pub cell_size: u32,
    pub max_new_glyphs_per_frame: u32,
}

impl GlyphAtlasConfig {
    #[must_use]
    pub fn default_config() -> Self {
        Self {
            page_width: 2048,
            page_height: 2048,
            cell_size: 64,
            max_new_glyphs_per_frame: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphSlot {
    pub uv_rect: [f32; 4],
    pub size_px: [f32; 2],
    pub bearing_px: [f32; 2],
    pub placeholder: bool,
}

impl GlyphSlot {
    const PLACEHOLDER: GlyphSlot = GlyphSlot {
        uv_rect: [0.0, 0.0, 0.0, 0.0],
        size_px: [0.0, 0.0],
        bearing_px: [0.0, 0.0],
        placeholder: true,
    };

    fn empty_ink() -> Self {
        Self {
            placeholder: false,
            ..Self::PLACEHOLDER
        }
    }
}

pub struct GlyphAtlas {
    config: GlyphAtlasConfig,
    cells_x: u32,
    cells: Vec<Cell>,
    index: HashMap<CacheKey, usize>,
    empty_glyphs: HashSet<CacheKey>,
    pixels: Vec<u8>,
    touch_clock: u64,
    new_this_frame: u32,
    dirty: bool,
    upload_rows: Option<(u32, u32)>,
    prerasterized: HashMap<CacheKey, Result<RasterizedGlyph>>,
}

impl GlyphAtlas {
    pub fn new(config: GlyphAtlasConfig) -> Result<Self> {
        if config.cell_size == 0 {
            return Err(Error::InvalidAtlasConfig("cell_size must be positive"));
        }
        if config.page_width == 0 || config.page_height == 0 {
            return Err(Error::InvalidAtlasConfig(
                "page_width and page_height must be positive",
            ));
        }
        if config.page_width > MAX_ATLAS_DIMENSION || config.page_height > MAX_ATLAS_DIMENSION {
            return Err(Error::InvalidAtlasConfig(
                "page dimensions exceed MAX_ATLAS_DIMENSION - R7's hard maximum",
            ));
        }
        if config.page_width % config.cell_size != 0 || config.page_height % config.cell_size != 0 {
            return Err(Error::InvalidAtlasConfig(
                "page dimensions must be a multiple of cell_size",
            ));
        }
        let cells_x = config.page_width / config.cell_size;
        let cells_y = config.page_height / config.cell_size;
        #[allow(clippy::cast_possible_truncation)]
        let cell_count = (cells_x as usize).saturating_mul(cells_y as usize);
        let pixel_count = (config.page_width as usize)
            .saturating_mul(config.page_height as usize)
            .saturating_mul(4);
        Ok(Self {
            config,
            cells_x,
            cells: vec![Cell::EMPTY; cell_count],
            index: HashMap::new(),
            empty_glyphs: HashSet::new(),
            pixels: vec![0u8; pixel_count],
            touch_clock: 0,
            new_this_frame: 0,
            dirty: false,
            upload_rows: None,
            prerasterized: HashMap::new(),
        })
    }

    pub fn begin_frame(&mut self) {
        self.new_this_frame = 0;
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.config.page_width, self.config.page_height)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.iter().filter(|c| c.key.is_some()).count()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.cells.len()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn take_upload(&mut self) -> Option<(u32, &[u8])> {
        let (start, end) = self.upload_rows.take()?;
        let stride = (self.config.page_width as usize).saturating_mul(4);
        let rows = self
            .pixels
            .get((start as usize).saturating_mul(stride)..(end as usize).saturating_mul(stride))?;
        Some((start, rows))
    }

    pub fn glyph(
        &mut self,
        font_id: FontId,
        font: &Font,
        size_px: u32,
        glyph: GlyphId,
    ) -> Result<GlyphSlot> {
        let key: CacheKey = (font_id, size_px, glyph);
        let mut ready = self.prerasterized.remove(&key);
        let slot = self.glyph_with(font_id, size_px, glyph, || {
            ready
                .take()
                .unwrap_or_else(|| rasterize_glyph(font, size_px, glyph))
        });
        if let Some(unused) = ready {
            self.prerasterized.insert(key, unused);
        }
        slot
    }

    pub fn prerasterize(&mut self, runs: &[(FontId, &Font, u32, &str)]) {
        let mut todo: Vec<(CacheKey, &Font)> = Vec::new();
        let mut queued = HashSet::new();
        let room = self.cells.len().saturating_sub(self.prerasterized.len());
        for &(font_id, font, size_px, text) in runs {
            for c in text.chars().filter(|c| !c.is_whitespace()) {
                let Some(glyph) = font.glyph_for_char(c) else {
                    continue;
                };
                let key: CacheKey = (font_id, size_px, glyph);
                let known = self.index.contains_key(&key)
                    || self.empty_glyphs.contains(&key)
                    || self.prerasterized.contains_key(&key);
                if !known && todo.len() < room && queued.insert(key) {
                    todo.push((key, font));
                }
            }
        }
        let made = mint_pool::map_ordered(&todo, |((_, size_px, glyph), font)| {
            rasterize_glyph(font, *size_px, *glyph)
        });
        for ((key, _), bitmap) in todo.into_iter().zip(made) {
            self.prerasterized.insert(key, bitmap);
        }
    }

    fn glyph_with(
        &mut self,
        font_id: FontId,
        size_px: u32,
        glyph: GlyphId,
        rasterize: impl FnOnce() -> Result<RasterizedGlyph>,
    ) -> Result<GlyphSlot> {
        let key: CacheKey = (font_id, size_px, glyph);

        if self.empty_glyphs.contains(&key) {
            return Ok(GlyphSlot::empty_ink());
        }

        if let Some(&index) = self.index.get(&key) {
            self.touch_clock = self.touch_clock.saturating_add(1);
            if let Some(cell) = self.cells.get_mut(index) {
                cell.touch = Some(self.touch_clock);
                return Ok(slot_for(cell, index, &self.config, self.cells_x));
            }
        }

        if self.new_this_frame >= self.config.max_new_glyphs_per_frame {
            return Ok(GlyphSlot::PLACEHOLDER);
        }

        let rasterized = rasterize()?;
        self.new_this_frame = self.new_this_frame.saturating_add(1);

        if rasterized.width == 0 || rasterized.height == 0 {
            self.empty_glyphs.insert(key);
            return Ok(GlyphSlot::empty_ink());
        }
        if rasterized.width > self.config.cell_size || rasterized.height > self.config.cell_size {
            return Err(Error::GlyphTooLargeForAtlas(
                rasterized.width,
                rasterized.height,
                self.config.cell_size,
            ));
        }

        let cell_index = self.cell_for_insert();
        self.evict(cell_index);
        self.blit(cell_index, &rasterized);

        self.touch_clock = self.touch_clock.saturating_add(1);
        let new_cell = Cell {
            key: Some(key),
            touch: Some(self.touch_clock),
            width: rasterized.width,
            height: rasterized.height,
            bearing_x: rasterized.bearing_x,
            bearing_y: rasterized.bearing_y,
        };
        let Some(slot_mut) = self.cells.get_mut(cell_index) else {
            return Err(Error::InvalidAtlasConfig(
                "internal: cell index out of range",
            ));
        };
        *slot_mut = new_cell;
        self.index.insert(key, cell_index);
        self.dirty = true;

        Ok(slot_for(&new_cell, cell_index, &self.config, self.cells_x))
    }

    fn cell_for_insert(&self) -> usize {
        let mut best_index = 0usize;
        let mut best_touch = u64::MAX;
        for (i, cell) in self.cells.iter().enumerate() {
            if cell.key.is_none() {
                return i;
            }
            let touch = cell.touch.unwrap_or(0);
            if touch < best_touch {
                best_touch = touch;
                best_index = i;
            }
        }
        best_index
    }

    fn evict(&mut self, cell_index: usize) {
        if let Some(cell) = self.cells.get(cell_index) {
            if let Some(old_key) = cell.key {
                self.index.remove(&old_key);
            }
        }
    }

    fn blit(&mut self, cell_index: usize, rasterized: &RasterizedGlyph) {
        #[allow(clippy::cast_possible_truncation)]
        let (cell_x, cell_y) = (
            (cell_index as u32) % self.cells_x,
            (cell_index as u32) / self.cells_x,
        );
        let origin_x = cell_x.saturating_mul(self.config.cell_size);
        let origin_y = cell_y.saturating_mul(self.config.cell_size);
        let page_width = self.config.page_width;
        let end_y = origin_y
            .saturating_add(self.config.cell_size)
            .min(self.config.page_height);
        self.upload_rows = Some(match self.upload_rows {
            Some((start, end)) => (start.min(origin_y), end.max(end_y)),
            None => (origin_y, end_y),
        });

        for row in 0..self.config.cell_size {
            for col in 0..self.config.cell_size {
                let px = origin_x.saturating_add(col);
                let py = origin_y.saturating_add(row);
                let coverage = if row < rasterized.height && col < rasterized.width {
                    let src_index =
                        (row as usize).saturating_mul(rasterized.width as usize) + col as usize;
                    rasterized.coverage.get(src_index).copied().unwrap_or(0)
                } else {
                    0
                };
                let byte_index =
                    ((py.saturating_mul(page_width)).saturating_add(px) as usize).saturating_mul(4);
                if let Some([r, g, b, a]) = self
                    .pixels
                    .get_mut(byte_index..byte_index.saturating_add(4))
                {
                    *r = 255;
                    *g = 255;
                    *b = 255;
                    *a = coverage;
                }
            }
        }
    }
}

fn rasterize_glyph(font: &Font, size_px: u32, glyph: GlyphId) -> Result<RasterizedGlyph> {
    let outline = font.outline(glyph, size_px)?;
    raster::rasterize(&outline).ok_or(Error::OutlineTooLarge(glyph, raster::MAX_RASTER_DIMENSION))
}

fn slot_for(cell: &Cell, cell_index: usize, config: &GlyphAtlasConfig, cells_x: u32) -> GlyphSlot {
    #[allow(clippy::cast_possible_truncation)]
    let (cell_x, cell_y) = ((cell_index as u32) % cells_x, (cell_index as u32) / cells_x);
    let origin_x = cell_x.saturating_mul(config.cell_size);
    let origin_y = cell_y.saturating_mul(config.cell_size);
    #[allow(clippy::cast_precision_loss)]
    let uv_rect = [
        origin_x as f32 / config.page_width as f32,
        origin_y as f32 / config.page_height as f32,
        cell.width as f32 / config.page_width as f32,
        cell.height as f32 / config.page_height as f32,
    ];
    GlyphSlot {
        uv_rect,
        #[allow(clippy::cast_precision_loss)]
        size_px: [cell.width as f32, cell.height as f32],
        bearing_px: [cell.bearing_x, -cell.bearing_y],
        placeholder: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_config() -> GlyphAtlasConfig {
        GlyphAtlasConfig {
            page_width: 16,
            page_height: 16,
            cell_size: 8,
            max_new_glyphs_per_frame: 100,
        }
    }

    #[test]
    fn a_config_with_a_dimension_not_a_multiple_of_cell_size_is_rejected() {
        let bad = GlyphAtlasConfig {
            page_width: 17,
            ..tiny_config()
        };
        assert!(matches!(
            GlyphAtlas::new(bad),
            Err(Error::InvalidAtlasConfig(_))
        ));
    }

    #[test]
    fn a_page_past_the_hard_maximum_is_rejected() {
        let bad = GlyphAtlasConfig {
            page_width: MAX_ATLAS_DIMENSION + 64,
            page_height: MAX_ATLAS_DIMENSION + 64,
            cell_size: 64,
            ..tiny_config()
        };
        assert!(matches!(
            GlyphAtlas::new(bad),
            Err(Error::InvalidAtlasConfig(_))
        ));
    }

    #[test]
    fn default_config_is_itself_valid() {
        assert!(GlyphAtlas::new(GlyphAtlasConfig::default_config()).is_ok());
    }

    #[allow(clippy::cast_precision_loss)]
    fn solid_glyph(size: u32) -> RasterizedGlyph {
        RasterizedGlyph {
            width: size,
            height: size,
            bearing_x: 0.0,
            bearing_y: size as f32,
            coverage: vec![255; (size * size) as usize],
        }
    }

    fn glyph_key(n: u32) -> GlyphId {
        GlyphId(n)
    }

    #[test]
    fn a_cache_hit_never_rasterises_again() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        let mut calls = 0u32;
        let first = atlas
            .glyph_with(FontId(1), 12, glyph_key(1), || {
                calls += 1;
                Ok(solid_glyph(4))
            })
            .unwrap();
        let second = atlas
            .glyph_with(FontId(1), 12, glyph_key(1), || {
                calls += 1;
                Ok(solid_glyph(4))
            })
            .unwrap();
        assert_eq!(calls, 1, "the second call must be a cache hit");
        assert_eq!(first, second, "a cached lookup must return the same slot");
    }

    #[test]
    fn a_full_atlas_evicts_the_least_recently_touched_cell_instead_of_failing() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        for n in 0..4u32 {
            let slot = atlas
                .glyph_with(FontId(1), 12, glyph_key(n), || Ok(solid_glyph(4)))
                .unwrap();
            assert!(!slot.placeholder);
        }
        assert_eq!(atlas.len(), 4, "all four cells now occupied");

        for n in [0u32, 2, 3] {
            atlas
                .glyph_with(FontId(1), 12, glyph_key(n), || {
                    panic!("must be a cache hit, not a rasterisation")
                })
                .unwrap();
        }

        let fifth = atlas
            .glyph_with(FontId(1), 12, glyph_key(4), || Ok(solid_glyph(4)))
            .unwrap();
        assert!(!fifth.placeholder, "eviction must make room, not fail");
        assert_eq!(atlas.len(), 4, "still exactly at capacity, not grown");

        let mut recomputed = false;
        atlas
            .glyph_with(FontId(1), 12, glyph_key(1), || {
                recomputed = true;
                Ok(solid_glyph(4))
            })
            .unwrap();
        assert!(
            recomputed,
            "the evicted glyph must have been actually evicted"
        );

        atlas
            .glyph_with(FontId(1), 12, glyph_key(4), || {
                panic!("glyph 4 must still be cached - it was not the LRU entry")
            })
            .unwrap();
    }

    #[test]
    fn the_per_frame_cap_defers_rasterisation_past_the_limit() {
        let config = GlyphAtlasConfig {
            max_new_glyphs_per_frame: 2,
            ..tiny_config()
        };
        let mut atlas = GlyphAtlas::new(config).unwrap();
        atlas.begin_frame();

        for n in 0..2u32 {
            let slot = atlas
                .glyph_with(FontId(1), 12, glyph_key(n), || Ok(solid_glyph(4)))
                .unwrap();
            assert!(!slot.placeholder, "within the cap must rasterise for real");
        }

        let mut called = false;
        let third = atlas
            .glyph_with(FontId(1), 12, glyph_key(2), || {
                called = true;
                Ok(solid_glyph(4))
            })
            .unwrap();
        assert!(
            third.placeholder,
            "the third new glyph this frame must be deferred"
        );
        assert!(
            !called,
            "a deferred glyph must not be rasterised at all this frame"
        );

        atlas.begin_frame();
        let after_new_frame = atlas
            .glyph_with(FontId(1), 12, glyph_key(2), || Ok(solid_glyph(4)))
            .unwrap();
        assert!(
            !after_new_frame.placeholder,
            "the deferred glyph must succeed once the per-frame budget resets"
        );
    }

    #[test]
    fn cache_hits_do_not_spend_the_per_frame_budget() {
        let config = GlyphAtlasConfig {
            max_new_glyphs_per_frame: 1,
            ..tiny_config()
        };
        let mut atlas = GlyphAtlas::new(config).unwrap();
        atlas.begin_frame();
        atlas
            .glyph_with(FontId(1), 12, glyph_key(0), || Ok(solid_glyph(4)))
            .unwrap();

        let hit = atlas
            .glyph_with(FontId(1), 12, glyph_key(0), || {
                panic!("must be a cache hit")
            })
            .unwrap();
        assert!(!hit.placeholder);
    }

    #[test]
    fn an_empty_ink_glyph_is_cached_without_occupying_a_cell() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        let mut calls = 0u32;
        for _ in 0..3 {
            let slot = atlas
                .glyph_with(FontId(1), 12, glyph_key(0), || {
                    calls += 1;
                    Ok(RasterizedGlyph::default())
                })
                .unwrap();
            assert!(!slot.placeholder, "empty ink is valid, not deferred");
            assert_eq!(slot.size_px, [0.0, 0.0]);
        }
        assert_eq!(
            calls, 1,
            "an empty-ink glyph must be cached, not re-rasterised"
        );
        assert_eq!(atlas.len(), 0, "an empty-ink glyph must not occupy a cell");
    }

    #[test]
    fn a_glyph_larger_than_one_cell_is_an_error() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        let err = atlas
            .glyph_with(FontId(1), 12, glyph_key(0), || Ok(solid_glyph(9)))
            .unwrap_err();
        assert!(matches!(err, Error::GlyphTooLargeForAtlas(9, 9, 8)));
    }

    #[test]
    fn a_rasterisation_error_propagates_rather_than_becoming_a_placeholder() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        let err = atlas
            .glyph_with(FontId(1), 12, glyph_key(0), || {
                Err(Error::OutlineTooLarge(glyph_key(0), 4096))
            })
            .unwrap_err();
        assert!(matches!(err, Error::OutlineTooLarge(GlyphId(0), 4096)));
    }

    #[test]
    fn prerasterized_glyphs_insert_exactly_as_plain_calls_do() {
        let font = Font::from_bytes(
            crate::text::bundled_substitute_font("Tinos")
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let config = GlyphAtlasConfig {
            page_width: 256,
            page_height: 256,
            cell_size: 32,
            max_new_glyphs_per_frame: 5,
        };
        let text = "Destroy the Dunedain outpost, 0123!";
        let run = |prerasterize: bool| {
            let mut atlas = GlyphAtlas::new(config).unwrap();
            let mut slots = Vec::new();
            for _frame in 0..8 {
                atlas.begin_frame();
                if prerasterize {
                    atlas.prerasterize(&[(FontId(3), &font, 18, text)]);
                }
                for c in text.chars().filter(|c| !c.is_whitespace()) {
                    let glyph = font.glyph_for_char(c).unwrap();
                    slots.push(atlas.glyph(FontId(3), &font, 18, glyph).unwrap());
                }
            }
            (slots, atlas.pixels().to_vec(), atlas.prerasterized.len())
        };
        let (plain_slots, plain_pixels, _) = run(false);
        let (slots, pixels, waiting) = run(true);
        assert!(
            plain_slots.iter().any(|s| s.placeholder),
            "the cap defers some"
        );
        assert_eq!(slots, plain_slots);
        assert_eq!(pixels, plain_pixels);
        assert_eq!(waiting, 0, "every waiting bitmap was asked for and went in");
    }

    #[test]
    fn a_copy_updated_from_the_upload_rows_equals_the_pixels() {
        let mut atlas = GlyphAtlas::new(tiny_config()).unwrap();
        let mut copy = atlas.pixels().to_vec();
        assert!(atlas.take_upload().is_none(), "nothing written yet");
        for (frame, glyphs) in [[0u32, 1], [2, 3], [4, 5]].into_iter().enumerate() {
            atlas.begin_frame();
            for n in glyphs {
                atlas
                    .glyph_with(FontId(1), 12, glyph_key(n), || Ok(solid_glyph(2 + n % 5)))
                    .unwrap();
            }
            let (first, rows) = atlas.take_upload().expect("glyphs were written");
            let at = first as usize * 16 * 4;
            copy[at..at + rows.len()].copy_from_slice(rows);
            assert_eq!(copy, atlas.pixels(), "frame {frame}");
            assert!(atlas.take_upload().is_none(), "taken once");
        }
    }
}
