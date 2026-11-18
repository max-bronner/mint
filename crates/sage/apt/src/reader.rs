use crate::{Error, Result};

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    if offset.checked_add(4).is_none() {
        return Err(Error::Overflow);
    }
    mint_bytes::u32_at(bytes, offset).ok_or(Error::OutOfBounds {
        offset,
        len: bytes.len(),
    })
}

pub(crate) fn i32_at(bytes: &[u8], offset: usize) -> Result<i32> {
    u32_at(bytes, offset).map(|v| v as i32)
}

pub(crate) fn f32_at(bytes: &[u8], offset: usize) -> Result<f32> {
    u32_at(bytes, offset).map(f32::from_bits)
}

pub(crate) fn cstr_at(bytes: &[u8], offset: usize) -> Result<String> {
    let rest = bytes.get(offset..).ok_or(Error::OutOfBounds {
        offset,
        len: bytes.len(),
    })?;
    let nul = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or(Error::UnterminatedString { offset })?;
    let text = rest.get(..nul).ok_or(Error::OutOfBounds {
        offset,
        len: bytes.len(),
    })?;
    Ok(String::from_utf8_lossy(text).into_owned())
}

pub(crate) fn checked_mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b).ok_or(Error::Overflow)
}

pub(crate) fn checked_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or(Error::Overflow)
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;

    #[test]
    fn u32_at_reads_little_endian() {
        let bytes = [0x78, 0x56, 0x34, 0x12];
        assert_eq!(u32_at(&bytes, 0).unwrap(), 0x1234_5678);
    }

    #[test]
    fn u32_at_out_of_bounds_errors_not_panics() {
        let bytes = [1, 2, 3];
        assert!(matches!(
            u32_at(&bytes, 0),
            Err(Error::OutOfBounds { offset: 0, len: 3 })
        ));
    }

    #[test]
    fn u32_at_offset_past_end_errors() {
        let bytes = [1, 2, 3, 4];
        assert!(u32_at(&bytes, 1).is_err());
    }

    #[test]
    fn u32_at_offset_overflow_errors() {
        let bytes = [1, 2, 3, 4];
        assert!(matches!(u32_at(&bytes, usize::MAX), Err(Error::Overflow)));
    }

    #[test]
    fn f32_at_reads_ieee754_bits_not_a_numeric_cast() {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&1.5f32.to_le_bytes());
        assert_eq!(f32_at(&bytes, 0).unwrap(), 1.5);
    }

    #[test]
    fn cstr_at_reads_up_to_nul() {
        let bytes = b"hello\0world\0";
        assert_eq!(cstr_at(bytes, 0).unwrap(), "hello");
        assert_eq!(cstr_at(bytes, 6).unwrap(), "world");
    }

    #[test]
    fn cstr_at_unterminated_errors_rather_than_reading_past_the_buffer() {
        let bytes = b"no terminator";
        assert!(matches!(
            cstr_at(bytes, 0),
            Err(Error::UnterminatedString { offset: 0 })
        ));
    }

    #[test]
    fn cstr_at_lossy_replaces_invalid_utf8_rather_than_failing() {
        let bytes = [0xffu8, 0xfe, 0x00];
        let s = cstr_at(&bytes, 0).unwrap();
        assert!(s.contains('\u{fffd}'));
    }

    #[test]
    fn checked_mul_overflow_is_an_error() {
        assert!(matches!(checked_mul(usize::MAX, 2), Err(Error::Overflow)));
        assert_eq!(checked_mul(3, 4).unwrap(), 12);
    }
}
