use std::fmt;
use std::marker::PhantomData;

use crate::reader::Reader;
use crate::{Error, Result};

pub trait Element: Sized {
    const SIZE: usize;

    fn read(r: &mut Reader<'_>) -> Result<Self>;
}

pub struct Array<'a, T> {
    data: &'a [u8],
    _marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Array<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Array<'_, T> {}

impl<T> Default for Array<'_, T> {
    fn default() -> Self {
        Self {
            data: &[],
            _marker: PhantomData,
        }
    }
}

impl<'a, T: Element> Array<'a, T> {
    pub fn new(what: &'static str, data: &'a [u8]) -> Result<Self> {
        if data.len().checked_rem(T::SIZE) != Some(0) {
            return Err(Error::RaggedArray {
                what,
                stride: T::SIZE,
                actual: data.len(),
            });
        }
        Ok(Self {
            data,
            _marker: PhantomData,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len().checked_div(T::SIZE).unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.data
    }

    #[must_use]
    pub fn get(&self, i: usize) -> Option<Result<T>> {
        let start = i.checked_mul(T::SIZE)?;
        let end = start.checked_add(T::SIZE)?;
        let slice = self.data.get(start..end)?;
        Some(T::read(&mut Reader::new(slice)))
    }

    #[must_use]
    pub fn iter(&self) -> ArrayIter<'a, T> {
        ArrayIter {
            array: *self,
            next: 0,
        }
    }

    pub fn to_vec(&self) -> Result<Vec<T>> {
        let mut out = Vec::with_capacity(self.len());
        for item in self.iter() {
            out.push(item?);
        }
        Ok(out)
    }
}

impl<T: Element> fmt::Debug for Array<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Array<{}>[{}]", std::any::type_name::<T>(), self.len())
    }
}

impl<'a, T: Element> IntoIterator for Array<'a, T> {
    type Item = Result<T>;
    type IntoIter = ArrayIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct ArrayIter<'a, T> {
    array: Array<'a, T>,
    next: usize,
}

impl<T: Element> Iterator for ArrayIter<'_, T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.array.get(self.next)?;
        self.next = self.next.saturating_add(1);
        Some(item)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.array.len().saturating_sub(self.next);
        (n, Some(n))
    }
}

impl<T: Element> ExactSizeIterator for ArrayIter<'_, T> {}

impl Element for u32 {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.u32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    #[test]
    fn decodes_elements_lazily() {
        let mut bytes = Vec::new();
        for i in 0..3u8 {
            bytes.extend_from_slice(&f32::from(i).to_le_bytes());
            bytes.extend_from_slice(&0f32.to_le_bytes());
            bytes.extend_from_slice(&0f32.to_le_bytes());
        }
        let a = Array::<Vec3>::new("VERTICES", &bytes).expect("well-formed");
        assert_eq!(a.len(), 3);
        assert_eq!(a.get(2).and_then(Result::ok).map(|v| v.x), Some(2.0));
        assert!(a.get(3).is_none());
        assert_eq!(a.iter().count(), 3);
        assert_eq!(a.to_vec().map(|v| v.len()), Ok(3));
    }

    #[test]
    fn a_ragged_payload_is_rejected() {
        let bytes = [0u8; 13];
        assert!(matches!(
            Array::<Vec3>::new("VERTICES", &bytes),
            Err(Error::RaggedArray { stride: 12, .. })
        ));
    }

    #[test]
    fn a_zero_length_payload_is_a_valid_empty_array() {
        let a = Array::<Vec3>::new("VERTICES", &[]).expect("zero is a whole number of elements");
        assert!(a.is_empty());
        assert_eq!(a.iter().count(), 0);
    }

    #[test]
    fn an_index_that_would_overflow_returns_none_rather_than_wrapping() {
        let a = Array::<Vec3>::new("VERTICES", &[0u8; 12]).expect("one element");
        assert!(a.get(usize::MAX).is_none());
    }
}
