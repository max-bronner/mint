use mint_bytes::Cursor;

use crate::{Result, SaveError};

pub const MAX_NEST_DEPTH: usize = 8;

const BLOCK_TAG: [u8; 4] = *b"KOLB";
const LONG_STRING: u8 = 0xFF;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsciiString(pub Vec<u8>);

impl AsciiString {
    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

impl From<&str> for AsciiString {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WideString(pub Vec<u16>);

impl WideString {
    #[must_use]
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.0)
    }
}

impl From<&str> for WideString {
    fn from(s: &str) -> Self {
        Self(s.encode_utf16().collect())
    }
}

#[derive(Debug, Clone)]
pub struct Reader<'a> {
    cur: Cursor<'a>,
    depth: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(data: &'a [u8], base: usize) -> Self {
        Self {
            cur: Cursor::new_at(data, base),
            depth: 0,
        }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.cur.position()
    }

    #[must_use]
    pub fn remaining(&self) -> usize {
        self.cur.remaining()
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        Ok(self.cur.take(n)?)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.cur.u8()?)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(self.cur.u16()?)
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(self.cur.u32()?)
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.cur.i32()?)
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(self.cur.f32()?)
    }

    pub fn bool(&mut self) -> Result<bool> {
        let offset = self.position();
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(SaveError::InvalidBool { offset, value }),
        }
    }

    pub fn version(&mut self, what: &'static str, min: u8, max: u8) -> Result<u8> {
        let version = self.u8()?;
        if (min..=max).contains(&version) {
            Ok(version)
        } else {
            Err(SaveError::UnsupportedVersion { what, version })
        }
    }

    fn length(&mut self) -> Result<usize> {
        let offset = self.position();
        let short = self.u8()?;
        let n = if short == LONG_STRING {
            let n = self.u32()?;
            if n < u32::from(LONG_STRING) {
                return Err(SaveError::NonCanonicalLength { offset });
            }
            n
        } else {
            u32::from(short)
        };
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    pub fn ascii(&mut self) -> Result<AsciiString> {
        let n = self.length()?;
        Ok(AsciiString(self.bytes(n)?.to_vec()))
    }

    pub fn wide(&mut self) -> Result<WideString> {
        let n = self.length()?;
        let bytes = self.cur.take(n.saturating_mul(2))?;
        Ok(WideString(
            bytes
                .chunks_exact(2)
                .map(|c| match *c {
                    [lo, hi] => u16::from_le_bytes([lo, hi]),
                    _ => 0,
                })
                .collect(),
        ))
    }

    pub fn nested(&mut self) -> Result<Reader<'a>> {
        let marker_at = self.position();
        if self.cur.array::<4>()? != BLOCK_TAG {
            return Err(SaveError::BadBlockMarker { offset: marker_at });
        }
        let end_at = self.position();
        let end = self.u32()?;
        let start = self.position();
        let bad_end = SaveError::BlockEnd {
            offset: end_at,
            end,
        };
        let len = usize::try_from(end)
            .ok()
            .and_then(|e| e.checked_sub(start))
            .ok_or_else(|| bad_end.clone())?;
        let depth = self.depth.saturating_add(1);
        if depth > MAX_NEST_DEPTH {
            return Err(SaveError::NestingTooDeep(MAX_NEST_DEPTH));
        }
        let payload = self.cur.take(len).map_err(|_| bad_end)?;
        Ok(Reader {
            cur: Cursor::new_at(payload, start),
            depth,
        })
    }

    pub fn finish(&self, what: &'static str) -> Result<()> {
        match self.remaining() {
            0 => Ok(()),
            count => Err(SaveError::NotExact {
                what,
                offset: self.position(),
                count,
            }),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Writer {
    buf: Vec<u8>,
    base: usize,
}

impl Writer {
    #[must_use]
    pub fn new(base: usize) -> Self {
        Self {
            buf: Vec::new(),
            base,
        }
    }

    #[must_use]
    pub fn position(&self) -> usize {
        self.base.saturating_add(self.buf.len())
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    fn length(&mut self, n: usize) {
        match u8::try_from(n) {
            Ok(short) if short != LONG_STRING => self.u8(short),
            _ => {
                self.u8(LONG_STRING);
                self.u32(u32::try_from(n).unwrap_or(u32::MAX));
            }
        }
    }

    pub fn ascii(&mut self, s: &AsciiString) {
        self.length(s.0.len());
        self.bytes(&s.0);
    }

    pub fn wide(&mut self, s: &WideString) {
        self.length(s.0.len());
        for unit in &s.0 {
            self.u16(*unit);
        }
    }

    pub fn nested(&mut self, f: impl FnOnce(&mut Writer)) {
        let start = self.position().saturating_add(8);
        let mut inner = Writer::new(start);
        f(&mut inner);
        let end = inner.position();
        self.bytes(&BLOCK_TAG);
        self.u32(u32::try_from(end).unwrap_or(u32::MAX));
        self.bytes(&inner.buf);
    }
}
