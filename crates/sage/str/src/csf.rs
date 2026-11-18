use mint_bytes::{Cursor, Truncated};

use crate::{Error, Result, StringRecord};

const MAGIC: [u8; 4] = *b" FSC";
const TAG_LABEL: [u8; 4] = *b" LBL";
const TAG_STRING: [u8; 4] = *b" RTS";
const TAG_STRING_WITH_WAVE: [u8; 4] = *b"WRTS";
const HEADER_LEN: usize = 24;

pub fn parse_csf(bytes: &[u8]) -> Result<Vec<StringRecord>> {
    if bytes.len() < HEADER_LEN {
        return Err(Error::CsfHeaderTruncated { len: bytes.len() });
    }
    if bytes.get(0..4) != Some(&MAGIC) {
        return Err(Error::CsfBadMagic);
    }

    let mut reader = Reader::new(bytes, HEADER_LEN);
    let mut records = Vec::with_capacity(bytes.len().saturating_div(12));

    while let Some(tag) = reader.tag() {
        if tag != TAG_LABEL {
            return Err(Error::CsfBadTag {
                offset: reader.at().saturating_sub(4),
                tag,
            });
        }
        let string_count = reader.u32("label string count")?;
        let label_len = reader.u32("label length")? as usize;
        let label_bytes = reader.take(label_len, "label")?;
        let label = crate::decode_cp1252(label_bytes);

        let mut text: Option<String> = None;
        let mut speech: Option<String> = None;
        for index in 0..string_count {
            let Some(tag) = reader.tag() else {
                return Err(Error::CsfTruncated {
                    offset: reader.at(),
                    what: "string tag",
                    len: 4,
                    left: bytes.len().saturating_sub(reader.at()),
                });
            };
            if tag != TAG_STRING && tag != TAG_STRING_WITH_WAVE {
                return Err(Error::CsfBadTag {
                    offset: reader.at().saturating_sub(4),
                    tag,
                });
            }
            let chars = reader.u32("string length")? as usize;
            let payload = reader.take(chars.saturating_mul(2), "string")?;
            if index == 0 {
                text = Some(decode_inverted_utf16(payload));
            }
            if tag == TAG_STRING_WITH_WAVE {
                let wave_len = reader.u32("wave length")? as usize;
                let wave = reader.take(wave_len, "wave")?;
                if index == 0 && !wave.is_empty() {
                    speech = Some(crate::decode_cp1252(wave));
                }
            }
        }

        if let Some(text) = text {
            records.push(StringRecord {
                label,
                text,
                speech,
            });
        }
    }

    Ok(records)
}

fn decode_inverted_utf16(payload: &[u8]) -> String {
    let units: Vec<u16> = payload
        .chunks_exact(2)
        .map(|pair| {
            let low = u16::from(pair.first().copied().unwrap_or(0));
            let high = u16::from(pair.get(1).copied().unwrap_or(0));
            !(low | high.wrapping_shl(8))
        })
        .collect();
    let decoded: String = char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    String::from_utf8_lossy(&crate::text::strip_spaces(decoded.as_bytes())).into_owned()
}

struct Reader<'a> {
    cursor: Cursor<'a>,
}

fn truncated(e: Truncated, what: &'static str) -> Error {
    Error::CsfTruncated {
        offset: e.offset,
        what,
        len: e.needed,
        left: e.available,
    }
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], at: usize) -> Self {
        Self {
            cursor: Cursor::new_at(bytes.get(at..).unwrap_or_default(), at),
        }
    }

    fn at(&self) -> usize {
        self.cursor.position()
    }

    fn tag(&mut self) -> Option<[u8; 4]> {
        self.cursor.array::<4>().ok()
    }

    fn u32(&mut self, what: &'static str) -> Result<u32> {
        self.cursor.u32().map_err(|e| truncated(e, what))
    }

    fn take(&mut self, len: usize, what: &'static str) -> Result<&'a [u8]> {
        self.cursor.take(len).map_err(|e| truncated(e, what))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation
)]
mod tests {
    use super::*;

    fn synth(labels: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(labels.len() as u32).to_le_bytes());
        out.extend_from_slice(&(labels.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for (label, text) in labels {
            out.extend_from_slice(&TAG_LABEL);
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(label.len() as u32).to_le_bytes());
            out.extend_from_slice(label.as_bytes());
            out.extend_from_slice(&TAG_STRING);
            let units: Vec<u16> = text.encode_utf16().collect();
            out.extend_from_slice(&(units.len() as u32).to_le_bytes());
            for unit in units {
                out.extend_from_slice(&(!unit).to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn parses_a_synthetic_file() {
        let bytes = synth(&[("GUI:Ok", "OK"), ("GUI:Cancel", "Cancel")]);
        let records = parse_csf(&bytes).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].label, "GUI:Ok");
        assert_eq!(records[0].text, "OK");
        assert_eq!(records[1].text, "Cancel");
    }

    #[test]
    fn the_payload_really_is_bit_inverted() {
        let bytes = synth(&[("K", "OK")]);
        let plain = {
            let mut copy = bytes.clone();
            let start = copy.len() - 4;
            copy[start..].copy_from_slice(&[b'O', 0, b'K', 0]);
            copy
        };
        assert_ne!(parse_csf(&plain).unwrap()[0].text, "OK");
    }

    #[test]
    fn a_short_header_is_an_error() {
        assert_eq!(
            parse_csf(b" FSC").unwrap_err(),
            Error::CsfHeaderTruncated { len: 4 }
        );
    }

    #[test]
    fn bad_magic_is_an_error() {
        let mut bytes = synth(&[("K", "V")]);
        bytes[0] = b'X';
        assert_eq!(parse_csf(&bytes).unwrap_err(), Error::CsfBadMagic);
    }

    #[test]
    fn a_huge_declared_label_length_does_not_allocate() {
        let mut bytes = synth(&[("K", "V")]);
        bytes[HEADER_LEN + 8..HEADER_LEN + 12].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = parse_csf(&bytes).unwrap_err();
        assert!(
            matches!(err, Error::CsfTruncated { what: "label", .. }),
            "{err:?}"
        );
    }

    #[test]
    fn a_huge_declared_string_count_stops_at_the_end_of_the_file() {
        let mut bytes = synth(&[("K", "V")]);
        bytes[HEADER_LEN + 4..HEADER_LEN + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_csf(&bytes).is_err());
    }

    #[test]
    fn truncation_sweep_never_panics() {
        let bytes = synth(&[("GUI:Ok", "OK"), ("GUI:Cancel", "Cancel")]);
        for end in 0..bytes.len() {
            let _ = parse_csf(&bytes[..end]);
        }
    }

    #[test]
    fn bit_corruption_sweep_never_panics() {
        let bytes = synth(&[("GUI:Ok", "OK")]);
        for index in 0..bytes.len() {
            for bit in 0..8u32 {
                let mut copy = bytes.clone();
                copy[index] ^= 1 << bit;
                let _ = parse_csf(&copy);
            }
        }
    }
}
