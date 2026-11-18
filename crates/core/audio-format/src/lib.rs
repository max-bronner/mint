#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod wav;

pub use wav::{decode_wav, probe_wav, Pcm16, WaveFormat, WaveInfo, WaveTag};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("not a RIFF file: {len} bytes is shorter than the 12-byte header")]
    TooShort { len: usize },

    #[error("not a RIFF/WAVE file: magic {found:?} at byte {offset}")]
    BadMagic { found: [u8; 4], offset: usize },

    #[error("no `fmt ` chunk in {scanned} bytes of chunk headers")]
    NoFormatChunk { scanned: usize },

    #[error("`fmt ` chunk at byte {offset} is {len} bytes, need at least 16")]
    ShortFormatChunk { offset: usize, len: usize },

    #[error("no `data` chunk in {scanned} bytes of chunk headers")]
    NoDataChunk { scanned: usize },

    #[error("`fmt ` chunk at byte {offset} has {field} = 0")]
    ZeroField { offset: usize, field: &'static str },

    #[error("unsupported `fmt ` tag {tag:#06x} ({bits} bit, {channels} channels)")]
    UnsupportedFormat { tag: u16, bits: u16, channels: u16 },

    #[error("IMA ADPCM block align {block_align} is smaller than the {needed}-byte preamble for {channels} channels")]
    ShortAdpcmBlock {
        block_align: u16,
        needed: u32,
        channels: u16,
    },
}

pub type Result<T> = core::result::Result<T, Error>;
