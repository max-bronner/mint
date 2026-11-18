use crate::reader::{checked_add, checked_mul, cstr_at, u32_at};
use crate::{Error, Result};

const MAGIC: &[u8] = b"Apt constant file\x1a\0\0";

#[derive(Debug, Clone, PartialEq)]
pub enum ConstItem {
    Text(String),
    Number(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstFile {
    pub movie_offset: u32,
    pub items: Vec<ConstItem>,
}

pub fn parse_const(bytes: &[u8]) -> Result<ConstFile> {
    let magic = bytes
        .get(..MAGIC.len())
        .ok_or(Error::ConstHeaderTruncated { len: bytes.len() })?;
    if magic != MAGIC {
        return Err(Error::ConstBadMagic);
    }
    let movie_offset = u32_at(bytes, 0x14)?;
    let item_count = u32_at(bytes, 0x18)? as usize;
    let items_offset = u32_at(bytes, 0x1c)? as usize;

    let mut items =
        Vec::with_capacity(item_count.min(bytes.len().saturating_div(8).saturating_add(1)));
    for index in 0..item_count {
        let entry_offset = checked_add(items_offset, checked_mul(index, 8)?)?;
        let ty = u32_at(bytes, entry_offset)?;
        let value = u32_at(bytes, checked_add(entry_offset, 4)?)?;
        let item = match ty {
            1 => ConstItem::Text(cstr_at(bytes, value as usize)?),
            4 => ConstItem::Number(value),
            other => return Err(Error::ConstBadItemType { index, ty: other }),
        };
        items.push(item);
    }

    Ok(ConstFile {
        movie_offset,
        items,
    })
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

    fn synth_const(movie_offset: u32, items: &[(u32, u32)]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&movie_offset.to_le_bytes());
        bytes.extend_from_slice(&(items.len() as u32).to_le_bytes());
        let items_offset = bytes.len() as u32 + 4;
        bytes.extend_from_slice(&items_offset.to_le_bytes());
        for (ty, value) in items {
            bytes.extend_from_slice(&ty.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_movie_offset_and_number_items() {
        let bytes = synth_const(0x34, &[(4, 7), (4, 99)]);
        let parsed = parse_const(&bytes).unwrap();
        assert_eq!(parsed.movie_offset, 0x34);
        assert_eq!(
            parsed.items,
            vec![ConstItem::Number(7), ConstItem::Number(99)]
        );
    }

    #[test]
    fn parses_string_items_from_a_value_offset() {
        let mut bytes = synth_const(0, &[(1, 0)]);
        let string_offset = bytes.len() as u32;
        let value_field_offset = MAGIC.len() + 12 + 4;
        bytes[value_field_offset..value_field_offset + 4]
            .copy_from_slice(&string_offset.to_le_bytes());
        bytes.extend_from_slice(b"hello\0");
        let parsed = parse_const(&bytes).unwrap();
        assert_eq!(parsed.items, vec![ConstItem::Text("hello".to_string())]);
    }

    #[test]
    fn rejects_a_short_header() {
        assert!(matches!(
            parse_const(&[0u8; 10]),
            Err(Error::ConstHeaderTruncated { len: 10 })
        ));
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = MAGIC.to_vec();
        bytes[0] = b'X';
        bytes.extend_from_slice(&[0u8; 12]);
        assert!(matches!(parse_const(&bytes), Err(Error::ConstBadMagic)));
    }

    #[test]
    fn rejects_an_unknown_item_type() {
        let bytes = synth_const(0, &[(2, 0)]);
        assert!(matches!(
            parse_const(&bytes),
            Err(Error::ConstBadItemType { index: 0, ty: 2 })
        ));
    }

    #[test]
    fn an_item_count_larger_than_the_buffer_errors_instead_of_over_allocating() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(&(bytes.len() as u32 + 4).to_le_bytes());
        assert!(parse_const(&bytes).is_err());
    }

    #[test]
    fn every_prefix_of_a_valid_const_file_parses_without_panicking() {
        let bytes = synth_const(0x34, &[(4, 7), (1, 0)]);
        for len in 0..=bytes.len() {
            let _ = parse_const(&bytes[..len]);
        }
    }
}
