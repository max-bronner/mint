use mint_bytes::{Cursor, Truncated};

use crate::{Error, Result};

fn eof(e: Truncated) -> Error {
    Error::UnexpectedEof {
        offset: e.offset,
        needed: e.needed,
        available: e.available,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Reader<'a> {
    cursor: Cursor<'a>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    pub(crate) fn new_at(data: &'a [u8], base: usize) -> Self {
        Self {
            cursor: Cursor::new_at(data, base),
        }
    }

    pub(crate) fn position(&self) -> usize {
        self.cursor.position()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.cursor.remaining()
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        self.cursor.take(n).map_err(eof)
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        self.cursor.u8().map_err(eof)
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        self.cursor.u16().map_err(eof)
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        self.cursor.u32().map_err(eof)
    }

    pub(crate) fn i32(&mut self) -> Result<i32> {
        self.cursor.i32().map_err(eof)
    }

    pub(crate) fn f32(&mut self) -> Result<f32> {
        self.cursor.f32().map_err(eof)
    }

    pub(crate) fn ascii_string(&mut self) -> Result<String> {
        let len = usize::from(self.u16()?);
        let bytes = self.take(len)?;
        Ok(bytes.iter().map(|&b| char::from(b)).collect())
    }

    pub(crate) fn wide_string(&mut self) -> Result<String> {
        let units = usize::from(self.u16()?);
        let mut buf = Vec::new();
        for _ in 0..units {
            buf.push(self.u16()?);
        }
        Ok(String::from_utf16_lossy(&buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_read_fails_without_moving_the_cursor() {
        let mut r = Reader::new(&[0x01, 0x02]);
        assert!(r.u32().is_err());
        assert_eq!(r.u16().unwrap(), 0x0201);
    }

    #[test]
    fn position_is_absolute_when_offset() {
        let mut r = Reader::new_at(&[1, 2, 3, 4], 100);
        let _ = r.u16();
        assert_eq!(r.position(), 102);
    }

    #[test]
    fn ascii_and_wide_strings_round_trip() {
        let mut bytes = 3u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"abc");
        let mut r = Reader::new(&bytes);
        assert_eq!(r.ascii_string().unwrap(), "abc");

        let mut wide = 2u16.to_le_bytes().to_vec();
        wide.extend_from_slice(&(u16::from(b'h')).to_le_bytes());
        wide.extend_from_slice(&(u16::from(b'i')).to_le_bytes());
        let mut r = Reader::new(&wide);
        assert_eq!(r.wide_string().unwrap(), "hi");
    }
}
