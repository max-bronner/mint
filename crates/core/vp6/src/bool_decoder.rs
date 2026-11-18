pub(crate) const MAX_OVERRUN: u32 = 16;

pub(crate) struct BoolDecoder<'a> {
    src: &'a [u8],
    pos: usize,
    value: u32,
    range: u32,
    bits: i32,
    overrun: u32,
}

impl<'a> BoolDecoder<'a> {
    pub(crate) fn new(src: &'a [u8]) -> Self {
        let mut value = 0u32;
        let mut overrun = 0u32;
        for i in 0..4 {
            let byte = src.get(i).copied().unwrap_or_else(|| {
                overrun = overrun.wrapping_add(1);
                0
            });
            value = value.wrapping_shl(8) | u32::from(byte);
        }
        Self {
            src,
            pos: 4,
            value,
            range: 255,
            bits: 8,
            overrun,
        }
    }

    pub(crate) fn overrun(&self) -> u32 {
        self.overrun
    }

    pub(crate) fn read_prob(&mut self, prob: u8) -> bool {
        self.renorm();
        let split = self
            .range
            .wrapping_sub(1)
            .wrapping_mul(u32::from(prob))
            .wrapping_shr(8)
            .wrapping_add(1);
        let bound = split.wrapping_shl(24);
        if self.value < bound {
            self.range = split;
            false
        } else {
            self.range = self.range.wrapping_sub(split);
            self.value = self.value.wrapping_sub(bound);
            true
        }
    }

    pub(crate) fn read_bool(&mut self) -> bool {
        self.read_prob(128)
    }

    pub(crate) fn read_bits(&mut self, n: u32) -> u32 {
        (0..n).fold(0u32, |acc, _| {
            acc.wrapping_shl(1) | u32::from(self.read_prob(128))
        })
    }

    pub(crate) fn read_probability(&mut self) -> u8 {
        let val = u8::try_from(self.read_bits(7)).unwrap_or(0);
        if val == 0 {
            1
        } else {
            val.wrapping_shl(1)
        }
    }

    fn renorm(&mut self) {
        let shift = self.range.leading_zeros() & 7;
        self.range = self.range.wrapping_shl(shift);
        self.value = self.value.wrapping_shl(shift);
        self.bits = self.bits.wrapping_sub_unsigned(shift);
        if self.bits <= 0 {
            match self.src.get(self.pos) {
                Some(&byte) => {
                    self.value |= u32::from(byte).wrapping_shl(self.bits.unsigned_abs());
                    self.pos = self.pos.wrapping_add(1);
                }
                None => self.overrun = self.overrun.saturating_add(1),
            }
            self.bits = self.bits.wrapping_add(8);
        }
    }
}
