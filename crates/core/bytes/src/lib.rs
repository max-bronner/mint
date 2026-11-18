#![no_std]
#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Truncated {
    pub offset: usize,
    pub needed: usize,
    pub available: usize,
}

impl fmt::Display for Truncated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "truncated at offset {}: needed {} bytes, {} available",
            self.offset, self.needed, self.available
        )
    }
}

impl core::error::Error for Truncated {}

#[must_use]
pub fn slice_at(bytes: &[u8], offset: usize, len: usize) -> Option<&[u8]> {
    let end = offset.checked_add(len)?;
    bytes.get(offset..end)
}

#[must_use]
pub fn array_at<const N: usize>(bytes: &[u8], offset: usize) -> Option<[u8; N]> {
    slice_at(bytes, offset, N)?.try_into().ok()
}

#[must_use]
pub fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    array_at(bytes, offset).map(u16::from_le_bytes)
}

#[must_use]
pub fn i16_at(bytes: &[u8], offset: usize) -> Option<i16> {
    array_at(bytes, offset).map(i16::from_le_bytes)
}

#[must_use]
pub fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    array_at(bytes, offset).map(u32::from_le_bytes)
}

#[must_use]
pub fn i32_at(bytes: &[u8], offset: usize) -> Option<i32> {
    array_at(bytes, offset).map(i32::from_le_bytes)
}

#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Cursor<'a> {
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base: 0,
        }
    }

    #[must_use]
    pub fn new_at(data: &'a [u8], base: usize) -> Self {
        Self { data, pos: 0, base }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.base.saturating_add(self.pos)
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or_default()
    }

    fn truncated(&self, needed: usize) -> Truncated {
        Truncated {
            offset: self.position(),
            needed,
            available: self.remaining(),
        }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Truncated> {
        let end = self.pos.checked_add(n).ok_or_else(|| self.truncated(n))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| self.truncated(n))?;
        self.pos = end;
        Ok(slice)
    }

    pub fn skip(&mut self, n: usize) -> Result<(), Truncated> {
        self.take(n).map(|_| ())
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Truncated> {
        let needed = self.truncated(N);
        self.take(N)?.try_into().map_err(|_| needed)
    }

    pub fn u8(&mut self) -> Result<u8, Truncated> {
        self.array::<1>().map(u8::from_le_bytes)
    }

    pub fn u16(&mut self) -> Result<u16, Truncated> {
        self.array().map(u16::from_le_bytes)
    }

    pub fn u32(&mut self) -> Result<u32, Truncated> {
        self.array().map(u32::from_le_bytes)
    }

    pub fn i32(&mut self) -> Result<i32, Truncated> {
        self.array().map(i32::from_le_bytes)
    }

    pub fn f32(&mut self) -> Result<f32, Truncated> {
        self.array().map(f32::from_le_bytes)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_ends_exactly_at_the_end_succeeds_and_one_byte_later_does_not() {
        let bytes = [1u8, 2, 3, 4];
        assert_eq!(u32_at(&bytes, 0), Some(0x0403_0201));
        assert_eq!(u16_at(&bytes, 2), Some(0x0403));
        assert_eq!(u16_at(&bytes, 3), None);
        assert_eq!(u16_at(&bytes, 4), None);
        assert_eq!(u16_at(&bytes, 5), None);
    }

    #[test]
    fn an_offset_that_would_overflow_the_address_space_is_none_rather_than_wrapping() {
        assert_eq!(u32_at(&[1, 2, 3, 4], usize::MAX), None);
        assert_eq!(slice_at(&[1, 2, 3, 4], usize::MAX, 4), None);
    }

    #[test]
    fn a_zero_length_take_at_the_end_succeeds_and_past_the_end_does_not() {
        let mut c = Cursor::new(&[1, 2]);
        c.skip(2).unwrap();
        assert_eq!(c.take(0), Ok(&[][..]));
        assert!(c.take(1).is_err());
    }

    #[test]
    fn a_failed_read_leaves_the_cursor_where_it_was() {
        let mut c = Cursor::new(&[1, 2]);
        assert!(c.u32().is_err());
        assert_eq!(c.position(), 0);
        assert_eq!(c.u16(), Ok(0x0201));
    }

    #[test]
    fn truncation_reports_the_offset_the_read_started_at_and_what_was_left() {
        let mut c = Cursor::new_at(&[1, 2, 3], 100);
        c.skip(1).unwrap();
        assert_eq!(
            c.u32(),
            Err(Truncated {
                offset: 101,
                needed: 4,
                available: 2,
            })
        );
    }

    #[test]
    fn a_cursor_over_a_taken_range_cannot_read_past_its_bound() {
        let mut c = Cursor::new(&[1, 2, 3, 4, 5, 6]);
        let base = c.position();
        let mut inner = Cursor::new_at(c.take(2).unwrap(), base);
        assert_eq!(inner.position(), 0);
        assert_eq!(inner.u16(), Ok(0x0201));
        assert!(inner.u8().is_err());
        assert_eq!(c.u32(), Ok(0x0605_0403));
    }
}
