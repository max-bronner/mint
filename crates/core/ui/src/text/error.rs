#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not parse font data")]
    InvalidFont(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("glyph {0:?} has no outline in this font")]
    NoOutline(crate::text::GlyphId),
    #[error("glyph {0:?} could not be drawn")]
    DrawFailed(
        crate::text::GlyphId,
        #[source] Box<dyn std::error::Error + Send + Sync>,
    ),
    #[error("requested size {0}px is outside the supported range (1..={1})")]
    SizeOutOfRange(u32, u32),
    #[error("glyph {0:?}'s outline exceeds the {1}-point safety cap")]
    OutlineTooComplex(crate::text::GlyphId, usize),
    #[error("glyph {0:?}'s outline bounding box exceeds the {1}px rasterisation limit")]
    OutlineTooLarge(crate::text::GlyphId, u32),
    #[error(
        "rasterised glyph is {0}x{1}px, larger than this atlas's {2}x{2}px cell - request a \
         smaller size or a larger cell"
    )]
    GlyphTooLargeForAtlas(u32, u32, u32),
    #[error("invalid atlas configuration: {0}")]
    InvalidAtlasConfig(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
