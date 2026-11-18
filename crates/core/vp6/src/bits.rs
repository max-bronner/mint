use crate::{Error, Result};

#[derive(Clone)]
pub(crate) struct BitReader<'a> {
    src: &'a [u8],
    byte: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            byte: 0,
            bit: 0,
        }
    }

    fn next_bit(&mut self) -> Option<u32> {
        let byte = self.src.get(self.byte)?;
        let val = u32::from(byte.wrapping_shr(7u32.wrapping_sub(self.bit)) & 1);
        if self.bit == 7 {
            self.bit = 0;
            self.byte = self.byte.wrapping_add(1);
        } else {
            self.bit = self.bit.wrapping_add(1);
        }
        Some(val)
    }

    pub(crate) fn read(&mut self, n: u32) -> Result<u32> {
        let mut val = 0u32;
        for _ in 0..n {
            let bit = self.next_bit().ok_or(Error::Truncated("bit reader"))?;
            val = val.wrapping_shl(1) | bit;
        }
        Ok(val)
    }

    pub(crate) fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read(1)? != 0)
    }

    pub(crate) fn peek16(&self) -> u32 {
        let mut ahead = self.clone();
        (0..16).fold(0u32, |acc, _| {
            acc.wrapping_shl(1) | ahead.next_bit().unwrap_or(0)
        })
    }

    pub(crate) fn skip(&mut self, n: u32) -> Result<()> {
        self.read(n).map(|_| ())
    }

    pub(crate) fn bytes_read(&self) -> usize {
        self.byte
    }
}
