use crate::xfer::{Reader, Writer};
use crate::{Result, SaveError};

pub const MAGIC: [u8; 8] = *b"ALAE2STR";
pub const HEADER_LEN: usize = 16;
pub const END_TOKEN: &[u8] = b"SG_EOF";

const BLOCK_TAG: [u8; 4] = *b"KOLB";
const BLOCK_OVERHEAD: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    name: Vec<u8>,
    payload: Vec<u8>,
}

impl RawBlock {
    pub fn new(name: &[u8], payload: Vec<u8>) -> Result<Self> {
        if u8::try_from(name.len()).is_err() {
            return Err(SaveError::NameTooLong(name.len()));
        }
        Ok(Self {
            name: name.to_vec(),
            payload,
        })
    }

    #[must_use]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    fn size(&self) -> usize {
        BLOCK_OVERHEAD
            .saturating_add(self.name.len())
            .saturating_add(self.payload.len())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveFile {
    pub blocks: Vec<RawBlock>,
}

impl SaveFile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes, 0);
        let head = r.bytes(HEADER_LEN).map_err(|_| SaveError::BadHeader)?;
        if head != header_bytes() {
            return Err(SaveError::BadHeader);
        }
        let mut blocks = Vec::new();
        loop {
            let name_len = r.u8()?;
            let name = r.bytes(usize::from(name_len))?.to_vec();
            if name.as_slice() == END_TOKEN {
                return match r.remaining() {
                    0 => Ok(Self { blocks }),
                    count => Err(SaveError::TrailingBytes {
                        offset: r.position(),
                        count,
                    }),
                };
            }
            let mut inner = r.nested()?;
            let payload = inner.bytes(inner.remaining())?.to_vec();
            blocks.push(RawBlock { name, payload });
        }
    }

    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new(0);
        w.bytes(&header_bytes());
        for block in &self.blocks {
            w.u8(u8::try_from(block.name.len()).unwrap_or(u8::MAX));
            w.bytes(&block.name);
            w.bytes(&BLOCK_TAG);
            let end = w
                .position()
                .saturating_add(4)
                .saturating_add(block.payload.len());
            w.u32(u32::try_from(end).unwrap_or(u32::MAX));
            w.bytes(&block.payload);
        }
        w.u8(u8::try_from(END_TOKEN.len()).unwrap_or(u8::MAX));
        w.bytes(END_TOKEN);
        w.into_bytes()
    }

    #[must_use]
    pub fn payload_offset(&self, index: usize) -> Option<usize> {
        let before = self.blocks.get(..index)?;
        let block = self.blocks.get(index)?;
        let sizes = before
            .iter()
            .fold(HEADER_LEN, |n, b| n.saturating_add(b.size()));
        Some(
            sizes
                .saturating_add(1)
                .saturating_add(block.name.len())
                .saturating_add(8),
        )
    }

    #[must_use]
    pub fn find(&self, name: &[u8]) -> Option<(usize, &RawBlock)> {
        self.blocks.iter().enumerate().find(|(_, b)| b.name == name)
    }

    pub fn push_with(&mut self, name: &[u8], encode: impl FnOnce(usize) -> Vec<u8>) -> Result<()> {
        if u8::try_from(name.len()).is_err() {
            return Err(SaveError::NameTooLong(name.len()));
        }
        let offset = self
            .blocks
            .iter()
            .fold(HEADER_LEN, |n, b| n.saturating_add(b.size()))
            .saturating_add(1)
            .saturating_add(name.len())
            .saturating_add(8);
        let payload = encode(offset);
        self.blocks.push(RawBlock {
            name: name.to_vec(),
            payload,
        });
        Ok(())
    }
}

pub(crate) fn header_bytes() -> Vec<u8> {
    let mut w = Writer::new(0);
    w.bytes(&MAGIC);
    w.u32(1);
    w.u32(0);
    w.into_bytes()
}
