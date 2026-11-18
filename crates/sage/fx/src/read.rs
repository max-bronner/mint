use crate::{Error, Result};

pub(crate) fn word_offset(word: usize) -> usize {
    word.saturating_mul(4)
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    mint_bytes::u32_at(bytes, offset).ok_or(Error::Truncated { offset })
}

pub(crate) fn slice_at(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    mint_bytes::slice_at(bytes, offset, len).ok_or(Error::Truncated { offset })
}

pub(crate) fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap_or([0; 4])))
        .collect()
}

pub(crate) struct Cursor<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(bytes: &'a [u8], pos: usize) -> Self {
        Self { bytes, pos }
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let value = u32_at(self.bytes, self.pos)?;
        self.pos = self
            .pos
            .checked_add(4)
            .ok_or(Error::Truncated { offset: self.pos })?;
        Ok(value)
    }

    pub(crate) fn padded(&mut self, len: usize) -> Result<&'a [u8]> {
        let data = slice_at(self.bytes, self.pos, len)?;
        let padded = len
            .checked_add(3)
            .map(|n| n & !3)
            .ok_or(Error::Truncated { offset: self.pos })?;
        self.pos = self
            .pos
            .checked_add(padded)
            .ok_or(Error::Truncated { offset: self.pos })?;
        Ok(data)
    }
}
