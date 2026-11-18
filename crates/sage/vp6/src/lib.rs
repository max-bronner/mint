#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

use mint_bytes::{Cursor, Truncated};

const CHUNK_HEADER: usize = 8;
const HEADER_PAYLOAD: usize = 24;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("truncated at byte {}: needed {} more, {} left", .0.offset, .0.needed, .0.available)]
    Truncated(Truncated),
    #[error("chunk at byte {offset} has size {size}, below the 8-byte chunk header")]
    ChunkTooSmall { offset: usize, size: u32 },
    #[error("chunk at byte {offset} has size {size} but only {available} bytes remain")]
    ChunkOverrun {
        offset: usize,
        size: u32,
        available: usize,
    },
    #[error("chunk at byte {offset} has a {payload}-byte payload, expected {expected}")]
    BadHeaderSize {
        offset: usize,
        payload: usize,
        expected: usize,
    },
    #[error("unexpected chunk tag {tag:02x?} at byte {offset}")]
    UnexpectedTag { offset: usize, tag: [u8; 4] },
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<Truncated> for Error {
    fn from(t: Truncated) -> Self {
        Self::Truncated(t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub frame_count: u32,
    pub max_chunk: u32,
    pub rate_numerator: u32,
    pub rate_denominator: u32,
}

impl Header {
    fn read(payload: &[u8], offset: usize) -> Result<Self> {
        if payload.len() != HEADER_PAYLOAD {
            return Err(Error::BadHeaderSize {
                offset,
                payload: payload.len(),
                expected: HEADER_PAYLOAD,
            });
        }
        let mut c = Cursor::new(payload);
        Ok(Self {
            fourcc: c.array()?,
            width: c.u16()?,
            height: c.u16()?,
            frame_count: c.u32()?,
            max_chunk: c.u32()?,
            rate_numerator: c.u32()?,
            rate_denominator: c.u32()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    ColourKey,
    ColourInter,
    AlphaKey,
    AlphaInter,
}

impl ChunkKind {
    fn from_tag(tag: [u8; 4]) -> Option<Self> {
        match &tag {
            b"MV0K" => Some(Self::ColourKey),
            b"MV0F" => Some(Self::ColourInter),
            b"AV0K" => Some(Self::AlphaKey),
            b"AV0F" => Some(Self::AlphaInter),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_alpha(self) -> bool {
        matches!(self, Self::AlphaKey | Self::AlphaInter)
    }

    #[must_use]
    pub fn is_key(self) -> bool {
        matches!(self, Self::ColourKey | Self::AlphaKey)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub kind: ChunkKind,
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct Container<'a> {
    pub header: Header,
    pub alpha_header: Option<Header>,
    frames: &'a [u8],
    base: usize,
}

fn read_chunk<'a>(cur: &mut Cursor<'a>) -> Result<([u8; 4], &'a [u8], usize)> {
    let offset = cur.position();
    let available = cur.remaining();
    let tag: [u8; 4] = cur.array()?;
    let size = cur.u32()?;
    let payload_len = usize::try_from(size)
        .ok()
        .and_then(|s| s.checked_sub(CHUNK_HEADER))
        .ok_or(Error::ChunkTooSmall { offset, size })?;
    let payload = cur.take(payload_len).map_err(|_| Error::ChunkOverrun {
        offset,
        size,
        available,
    })?;
    Ok((tag, payload, offset))
}

fn expect_header(cur: &mut Cursor<'_>, want: &[u8; 4]) -> Result<Header> {
    let (tag, payload, offset) = read_chunk(cur)?;
    if &tag != want {
        return Err(Error::UnexpectedTag { offset, tag });
    }
    Header::read(payload, offset)
}

impl<'a> Container<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let mut cur = Cursor::new(data);
        let first: [u8; 4] = Cursor::new(data).array()?;
        let has_alpha = &first == b"AVP6";
        if has_alpha {
            let (_, payload, offset) = read_chunk(&mut cur)?;
            if !payload.is_empty() {
                return Err(Error::BadHeaderSize {
                    offset,
                    payload: payload.len(),
                    expected: 0,
                });
            }
        }
        let header = expect_header(&mut cur, b"MVhd")?;
        let alpha_header = if has_alpha {
            Some(expect_header(&mut cur, b"AVhd")?)
        } else {
            None
        };
        let base = cur.position();
        Ok(Self {
            header,
            alpha_header,
            frames: cur.rest(),
            base,
        })
    }

    #[must_use]
    pub fn chunks(&self) -> Chunks<'a> {
        Chunks {
            cur: Cursor::new_at(self.frames, self.base),
            done: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Chunks<'a> {
    cur: Cursor<'a>,
    done: bool,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.cur.is_empty() {
            return None;
        }
        let item = read_chunk(&mut self.cur).and_then(|(tag, data, offset)| {
            ChunkKind::from_tag(tag)
                .map(|kind| Chunk { kind, data })
                .ok_or(Error::UnexpectedTag { offset, tag })
        });
        if item.is_err() {
            self.done = true;
        }
        Some(item)
    }
}

#[cfg(test)]
mod tests;
