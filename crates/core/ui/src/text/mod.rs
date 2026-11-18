mod atlas;
mod bundled;
mod error;
mod fallback;
mod font;
mod host;
mod layout;
mod outline;
mod raster;

pub use atlas::{FontId, GlyphAtlas, GlyphAtlasConfig, GlyphSlot, MAX_ATLAS_DIMENSION};
pub use bundled::bundled_substitute_font;
pub use error::{Error, Result};
pub use fallback::{
    chain_for, resolve_font_name, FontFallback, ALBERTUS_MT, ALL, ARIAL, ARIAL_UNICODE_MS, COURIER,
    COURIER_NEW, OMNIA_LT_STD, SACHA_WYNTER, TIMES_NEW_ROMAN,
};
pub use font::{Advance, Font, FontMetrics, GlyphId, MAX_SIZE_PX};
pub use host::{os_font_dir, os_font_file_exists, read_os_font_file};
pub use layout::{layout_text, measure_text, sync_atlas_texture};
pub use outline::{GlyphOutline, MAX_OUTLINE_POINTS};
pub use raster::{rasterize, RasterizedGlyph, MAX_RASTER_DIMENSION};
