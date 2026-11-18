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
pub struct Reader<'a> {
    cursor: Cursor<'a>,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.cursor.position()
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.cursor.remaining()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cursor.is_empty()
    }

    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.cursor.rest()
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        self.cursor.take(n).map_err(eof)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.cursor.skip(n).map_err(eof)
    }

    pub fn u8(&mut self) -> Result<u8> {
        self.cursor.u8().map_err(eof)
    }

    pub fn u16(&mut self) -> Result<u16> {
        self.cursor.u16().map_err(eof)
    }

    pub fn u32(&mut self) -> Result<u32> {
        self.cursor.u32().map_err(eof)
    }

    pub fn i32(&mut self) -> Result<i32> {
        self.cursor.i32().map_err(eof)
    }

    pub fn f32(&mut self) -> Result<f32> {
        self.cursor.f32().map_err(eof)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.cursor.array::<N>().map_err(eof)
    }

    pub fn fixed_name(&mut self, n: usize) -> Result<String> {
        let raw = self.take(n)?;
        Ok(decode_name(raw))
    }
}

pub(crate) fn decode_name(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    raw.get(..end)
        .unwrap_or_default()
        .iter()
        .map(|&b| char::from(b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_le_scalars() {
        let mut r = Reader::new(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f]);
        assert_eq!(r.u32(), Ok(1));
        assert_eq!(r.f32(), Ok(1.0));
        assert!(r.is_empty());
    }

    #[test]
    fn short_read_fails_without_moving_the_cursor() {
        let mut r = Reader::new(&[0x01, 0x02]);
        assert!(r.u32().is_err());
        assert_eq!(r.position(), 0);
        assert_eq!(r.u16(), Ok(0x0201));
    }

    #[test]
    fn take_cannot_overflow_the_cursor() {
        let mut r = Reader::new(&[0u8; 4]);
        assert!(r.take(usize::MAX).is_err());
    }

    #[test]
    fn names_stop_at_nul() {
        let mut r = Reader::new(b"TORSO\0\0\0\0\0\0\0\0\0\0\0");
        assert_eq!(r.fixed_name(16).as_deref(), Ok("TORSO"));
    }

    #[test]
    fn unterminated_name_uses_the_whole_field() {
        assert_eq!(decode_name(b"ABCD"), "ABCD");
    }

    #[test]
    fn high_bytes_survive_as_latin1() {
        assert_eq!(decode_name(&[0xE9, 0x41]), "\u{e9}A");
    }
}
