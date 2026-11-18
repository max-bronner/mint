#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("truncated at byte {offset}: needed {needed} more bytes, {available} remained")]
    Truncated {
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error("byte 1 is {byte:#04x}, not the 0xFB RefPack magic")]
    BadMagic { byte: u8 },

    #[error(
        "copy command at byte {offset} references {distance} bytes back, \
         but only {available} bytes of output exist yet"
    )]
    InvalidCopyDistance {
        offset: usize,
        distance: usize,
        available: usize,
    },

    #[error(
        "decompressed output exceeded the {MAX_OUTPUT}-byte safety cap before the stream ended"
    )]
    OutputTooLarge,
}

pub const MAX_OUTPUT: usize = 256 * 1024 * 1024;

const EAR_MAGIC: &[u8; 4] = b"EAR\0";

#[derive(Debug)]
struct Header {
    body_offset: usize,
    declared_len: u64,
}

impl Header {
    fn parse(stream: &[u8]) -> Result<Self> {
        let &flags = stream.first().ok_or(Error::Truncated {
            offset: 0,
            needed: 2,
            available: stream.len(),
        })?;
        let &magic = stream.get(1).ok_or(Error::Truncated {
            offset: 0,
            needed: 2,
            available: stream.len(),
        })?;
        if magic != 0xFB {
            return Err(Error::BadMagic { byte: magic });
        }

        let big = flags & 0x80 != 0;
        let compressed_size_present = flags & 0x01 != 0;
        let field_width: usize = if big { 4 } else { 3 };

        let mut offset: usize = 2;
        if compressed_size_present {
            offset = offset.checked_add(field_width).ok_or(Error::Truncated {
                offset: 2,
                needed: field_width,
                available: stream.len().saturating_sub(2),
            })?;
        }

        let end = offset.checked_add(field_width).ok_or(Error::Truncated {
            offset,
            needed: field_width,
            available: stream.len().saturating_sub(offset.min(stream.len())),
        })?;
        let size_bytes = stream.get(offset..end).ok_or(Error::Truncated {
            offset,
            needed: field_width,
            available: stream.len().saturating_sub(offset.min(stream.len())),
        })?;
        let declared_len = size_bytes
            .iter()
            .fold(0u64, |acc, &b| (acc << 8) | u64::from(b));

        Ok(Header {
            body_offset: end,
            declared_len,
        })
    }
}

pub fn decompress(stream: &[u8]) -> Result<Vec<u8>> {
    decompress_capped(stream, MAX_OUTPUT)
}

fn decompress_capped(stream: &[u8], cap: usize) -> Result<Vec<u8>> {
    let header = Header::parse(stream)?;
    let mut input = stream.get(header.body_offset..).ok_or(Error::Truncated {
        offset: header.body_offset,
        needed: 1,
        available: 0,
    })?;
    let mut output = Vec::new();

    loop {
        let offset = stream.len().saturating_sub(input.len());
        let (&opcode, rest) = input.split_first().ok_or(Error::Truncated {
            offset,
            needed: 1,
            available: 0,
        })?;
        input = rest;

        match opcode {
            0xFC..=0xFF => {
                let literal = usize::from(opcode & 0x03);
                take_literal(&mut output, &mut input, literal, offset, cap)?;
                break;
            }
            0xE0..=0xFB => {
                let literal = usize::from(opcode & 0x1F)
                    .saturating_add(1)
                    .saturating_mul(4);
                take_literal(&mut output, &mut input, literal, offset, cap)?;
            }
            0xC0..=0xDF => {
                let (&b1, r) = input
                    .split_first()
                    .ok_or(truncated(offset, 3, input.len()))?;
                let (&b2, r) = r.split_first().ok_or(truncated(offset, 3, input.len()))?;
                let (&b3, r) = r.split_first().ok_or(truncated(offset, 3, input.len()))?;
                input = r;

                let literal = usize::from(opcode & 0x03);
                let length =
                    ((usize::from(opcode & 0x0C) << 6) | usize::from(b3)).saturating_add(5);
                let distance =
                    ((usize::from(opcode & 0x10) << 12) | (usize::from(b1) << 8) | usize::from(b2))
                        .saturating_add(1);

                take_literal(&mut output, &mut input, literal, offset, cap)?;
                copy_back(&mut output, distance, length, offset, cap)?;
            }
            0x80..=0xBF => {
                let (&b1, r) = input
                    .split_first()
                    .ok_or(truncated(offset, 2, input.len()))?;
                let (&b2, r) = r.split_first().ok_or(truncated(offset, 2, input.len()))?;
                input = r;

                let literal = usize::from(b1 >> 6);
                let length = usize::from(opcode & 0x3F).saturating_add(4);
                let distance = ((usize::from(b1 & 0x3F) << 8) | usize::from(b2)).saturating_add(1);

                take_literal(&mut output, &mut input, literal, offset, cap)?;
                copy_back(&mut output, distance, length, offset, cap)?;
            }
            0x00..=0x7F => {
                let (&b1, r) = input
                    .split_first()
                    .ok_or(truncated(offset, 1, input.len()))?;
                input = r;

                let literal = usize::from(opcode & 0x03);
                let length = (usize::from(opcode & 0x1C) >> 2).saturating_add(3);
                let distance =
                    ((usize::from(opcode & 0x60) << 3) | usize::from(b1)).saturating_add(1);

                take_literal(&mut output, &mut input, literal, offset, cap)?;
                copy_back(&mut output, distance, length, offset, cap)?;
            }
        }
    }

    Ok(output)
}

pub fn declared_len(stream: &[u8]) -> Result<u64> {
    Header::parse(stream).map(|header| header.declared_len)
}

pub fn strip_ear_wrapper(bytes: &[u8]) -> Option<&[u8]> {
    let rest = bytes.strip_prefix(EAR_MAGIC.as_slice())?;
    rest.get(4..)
}

pub fn decompress_ear_wrapped(bytes: &[u8]) -> Result<Vec<u8>> {
    const EAR_HEADER_LEN: usize = 8;
    let stream = strip_ear_wrapper(bytes).ok_or(Error::Truncated {
        offset: 0,
        needed: EAR_HEADER_LEN,
        available: bytes.len(),
    })?;
    decompress(stream)
}

pub fn ear_declared_len(bytes: &[u8]) -> Option<u32> {
    let rest = bytes.strip_prefix(EAR_MAGIC.as_slice())?;
    let field = rest.get(0..4)?;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(field);
    Some(u32::from_le_bytes(buf))
}

fn truncated(offset: usize, needed: usize, available: usize) -> Error {
    Error::Truncated {
        offset,
        needed,
        available,
    }
}

fn take_literal(
    output: &mut Vec<u8>,
    input: &mut &[u8],
    count: usize,
    command_offset: usize,
    cap: usize,
) -> Result<()> {
    check_cap(output.len(), count, cap)?;
    let chunk = input.get(..count).ok_or(Error::Truncated {
        offset: command_offset,
        needed: count,
        available: input.len(),
    })?;
    output.extend_from_slice(chunk);
    *input = input.get(count..).unwrap_or_default();
    Ok(())
}

fn copy_back(
    output: &mut Vec<u8>,
    distance: usize,
    length: usize,
    command_offset: usize,
    cap: usize,
) -> Result<()> {
    check_cap(output.len(), length, cap)?;
    let start = output
        .len()
        .checked_sub(distance)
        .ok_or(Error::InvalidCopyDistance {
            offset: command_offset,
            distance,
            available: output.len(),
        })?;
    for i in 0..length {
        let byte =
            output
                .get(start.saturating_add(i))
                .copied()
                .ok_or(Error::InvalidCopyDistance {
                    offset: command_offset,
                    distance,
                    available: output.len(),
                })?;
        output.push(byte);
    }
    Ok(())
}

fn check_cap(current_len: usize, additional: usize, cap: usize) -> Result<()> {
    if current_len.saturating_add(additional) > cap {
        return Err(Error::OutputTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(decompressed_len: u32) -> Vec<u8> {
        let bytes = decompressed_len.to_be_bytes();
        vec![0x10, 0xFB, bytes[1], bytes[2], bytes[3]]
    }

    #[test]
    fn literal_only_block_round_trips() {
        let mut stream = header(16);
        stream.push(0xE3);
        stream.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        stream.push(0xFC);

        let out = decompress(&stream).expect("valid stream");
        assert_eq!(out, (1..=16).collect::<Vec<u8>>());
    }

    #[test]
    fn short_command_back_reference_with_overlap() {
        let mut stream = header(7);
        stream.push(0xE0);
        stream.extend_from_slice(b"abcd");
        stream.push(0x00);
        stream.push(0x00);
        stream.push(0xFC);

        let out = decompress(&stream).expect("valid stream");
        assert_eq!(out, b"abcdddd");
    }

    #[test]
    fn truncated_header_is_an_error_not_a_panic() {
        assert!(matches!(decompress(&[0x10]), Err(Error::Truncated { .. })));
        assert!(matches!(decompress(&[]), Err(Error::Truncated { .. })));
    }

    #[test]
    fn bad_magic_is_reported() {
        let stream = [0x10, 0x00, 0, 0, 0];
        assert!(matches!(
            decompress(&stream),
            Err(Error::BadMagic { byte: 0 })
        ));
    }

    #[test]
    fn copy_distance_before_start_of_output_is_an_error() {
        let mut stream = header(3);
        stream.push(0x00);
        stream.push(0x00);
        let err = decompress(&stream).unwrap_err();
        assert!(matches!(err, Error::InvalidCopyDistance { .. }));
    }

    #[test]
    fn truncation_sweep_never_panics() {
        let mut valid = header(20);
        valid.push(0xE3);
        valid.extend_from_slice(&[0; 16]);
        valid.push(0x00);
        valid.push(0x00);
        valid.push(0xFC);

        for cut in 0..=valid.len() {
            let _ = decompress(&valid[..cut]);
        }
    }

    #[test]
    fn bit_corruption_sweep_never_panics() {
        let mut valid = header(20);
        valid.push(0xE3);
        valid.extend_from_slice(&[0; 16]);
        valid.push(0x00);
        valid.push(0x00);
        valid.push(0xFC);

        for i in 0..valid.len() {
            for bit in 0..8u8 {
                let mut corrupt = valid.clone();
                if let Some(byte) = corrupt.get_mut(i) {
                    *byte ^= 1 << bit;
                }
                let _ = decompress(&corrupt);
            }
        }
    }

    #[test]
    fn ear_wrapper_is_recognised_and_stripped() {
        let mut inner = header(4);
        inner.push(0xE0);
        inner.extend_from_slice(&[9, 9, 9, 9]);
        inner.push(0xFC);

        let mut wrapped = b"EAR\0".to_vec();
        wrapped.extend_from_slice(&4u32.to_le_bytes());
        wrapped.extend_from_slice(&inner);

        assert_eq!(strip_ear_wrapper(&wrapped), Some(inner.as_slice()));
        assert_eq!(ear_declared_len(&wrapped), Some(4));
        let out = decompress_ear_wrapped(&wrapped).expect("valid wrapped stream");
        assert_eq!(out, vec![9, 9, 9, 9]);

        assert_eq!(strip_ear_wrapper(b"not wrapped"), None);
    }

    #[test]
    fn declared_output_larger_than_cap_is_rejected_without_allocating_it() {
        let mut stream = header(u32::MAX);
        stream.push(0xE0);
        stream.extend_from_slice(&[0, 0, 0, 0]);
        stream.extend_from_slice(&[0xCC, 0x00, 0x00, 0xFF]);
        stream.push(0xFC);

        let err = decompress_capped(&stream, 64).unwrap_err();
        assert!(matches!(err, Error::OutputTooLarge));
    }
}
