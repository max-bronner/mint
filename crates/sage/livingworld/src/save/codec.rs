use mint_sage_save::{Reader, SaveError, Writer};

use super::LoadError;

pub(crate) struct Enc(Writer);

impl Enc {
    pub(crate) fn new() -> Self {
        Self(Writer::new(0))
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes()
    }

    pub(crate) fn u8(&mut self, v: u8) {
        self.0.u8(v);
    }

    pub(crate) fn u16(&mut self, v: u16) {
        self.0.u16(v);
    }

    pub(crate) fn u32(&mut self, v: u32) {
        self.0.u32(v);
    }

    pub(crate) fn i32(&mut self, v: i32) {
        self.0.i32(v);
    }

    pub(crate) fn bool(&mut self, v: bool) {
        self.0.bool(v);
    }

    pub(crate) fn u64(&mut self, v: u64) {
        self.0.u32(u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0));
        self.0.u32(u32::try_from(v >> 32).unwrap_or(0));
    }

    pub(crate) fn str(&mut self, s: &str) {
        self.0.ascii(&s.into());
    }

    pub(crate) fn len(&mut self, n: usize) {
        self.0.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    pub(crate) fn opt<T>(&mut self, v: Option<T>, put: impl FnOnce(&mut Self, T)) {
        self.0.bool(v.is_some());
        if let Some(v) = v {
            put(self, v);
        }
    }

    pub(crate) fn list<T>(&mut self, items: &[T], mut put: impl FnMut(&mut Self, &T)) {
        self.len(items.len());
        for item in items {
            put(self, item);
        }
    }
}

pub(crate) struct Dec<'a>(Reader<'a>);

impl<'a> Dec<'a> {
    pub(crate) fn new(data: &'a [u8], base: usize) -> Self {
        Self(Reader::new(data, base))
    }

    pub(crate) fn finish(&self, what: &'static str) -> Result<(), LoadError> {
        Ok(self.0.finish(what)?)
    }

    pub(crate) fn version(&mut self, what: &'static str, max: u8) -> Result<u8, LoadError> {
        Ok(self.0.version(what, 1, max)?)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.0.u8()?)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, LoadError> {
        Ok(self.0.u16()?)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, LoadError> {
        Ok(self.0.u32()?)
    }

    pub(crate) fn i32(&mut self) -> Result<i32, LoadError> {
        Ok(self.0.i32()?)
    }

    pub(crate) fn bool(&mut self) -> Result<bool, LoadError> {
        Ok(self.0.bool()?)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, LoadError> {
        let low = u64::from(self.0.u32()?);
        let high = u64::from(self.0.u32()?);
        Ok(low | (high << 32))
    }

    pub(crate) fn str(&mut self) -> Result<String, LoadError> {
        Ok(self.0.ascii()?.to_string_lossy())
    }

    pub(crate) fn opt<T>(
        &mut self,
        get: impl FnOnce(&mut Self) -> Result<T, LoadError>,
    ) -> Result<Option<T>, LoadError> {
        if self.0.bool()? {
            Ok(Some(get(self)?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn list<T>(
        &mut self,
        mut get: impl FnMut(&mut Self) -> Result<T, LoadError>,
    ) -> Result<Vec<T>, LoadError> {
        let count = self.0.u32()?;
        let mut out = Vec::new();
        for _ in 0..count {
            out.push(get(self)?);
        }
        Ok(out)
    }

    pub(crate) fn invalid(&self, what: &'static str) -> LoadError {
        LoadError::Invalid(what)
    }
}

impl From<SaveError> for LoadError {
    fn from(e: SaveError) -> Self {
        Self::Format(e)
    }
}
