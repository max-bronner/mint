use mint_bytes::{array_at, u32_at};

use crate::{CursorImage, Error, Result};

const RIFF_HEADER_LEN: usize = 12;
const CHUNK_HEADER_LEN: usize = 8;
const ANIH_LEN: usize = 36;

#[derive(Debug, Clone, PartialEq)]
pub struct AnimatedCursor {
    frames: Vec<CursorImage>,
    sequence: Vec<u32>,
    rate_jiffies: Vec<u32>,
}

impl AnimatedCursor {
    #[must_use]
    pub fn steps(&self) -> usize {
        self.sequence.len()
    }

    #[must_use]
    pub fn step(&self, step: usize) -> Option<(&CursorImage, u32)> {
        if self.sequence.is_empty() {
            return None;
        }
        let index = step.checked_rem(self.sequence.len())?;
        let frame_index = usize::try_from(*self.sequence.get(index)?).ok()?;
        let image = self.frames.get(frame_index)?;
        let jiffies = self.rate_jiffies.get(index).copied().unwrap_or(6);
        Some((image, jiffies))
    }

    #[must_use]
    pub fn frames(&self) -> &[CursorImage] {
        &self.frames
    }
}

pub fn decode(bytes: &[u8]) -> Result<AnimatedCursor> {
    if bytes.len() < RIFF_HEADER_LEN {
        return Err(Error::HeaderTruncated {
            format: "ani",
            len: bytes.len(),
            needed: RIFF_HEADER_LEN,
            offset: 0,
        });
    }
    if array_at::<4>(bytes, 0) != Some(*b"RIFF") {
        return Err(Error::BadField {
            format: "ani",
            field: "RIFF magic",
            found: u32::from_le_bytes(array_at::<4>(bytes, 0).unwrap_or_default()),
            expected: u32::from_le_bytes(*b"RIFF"),
        });
    }
    if array_at::<4>(bytes, 8) != Some(*b"ACON") {
        return Err(Error::BadField {
            format: "ani",
            field: "form type",
            found: u32::from_le_bytes(array_at::<4>(bytes, 8).unwrap_or_default()),
            expected: u32::from_le_bytes(*b"ACON"),
        });
    }

    let mut frames = Vec::new();
    let mut disp_rate = 6u32;
    let mut n_steps: Option<usize> = None;
    let mut rate: Option<Vec<u32>> = None;
    let mut seq: Option<Vec<u32>> = None;

    let mut offset = RIFF_HEADER_LEN;
    while let Some(header) = bytes.get(offset..) {
        if header.len() < CHUNK_HEADER_LEN {
            break;
        }
        let id = array_at::<4>(bytes, offset).ok_or(Error::DataTruncated {
            format: "ani",
            offset,
            needed: CHUNK_HEADER_LEN,
            available: bytes.len().saturating_sub(offset),
        })?;
        let size = u32_at(bytes, offset.saturating_add(4)).ok_or(Error::DataTruncated {
            format: "ani",
            offset,
            needed: CHUNK_HEADER_LEN,
            available: bytes.len().saturating_sub(offset),
        })?;
        let size = usize::try_from(size).map_err(|_| Error::DataTruncated {
            format: "ani",
            offset,
            needed: 0,
            available: 0,
        })?;
        let data_start = offset
            .checked_add(CHUNK_HEADER_LEN)
            .ok_or(Error::DataTruncated {
                format: "ani",
                offset,
                needed: CHUNK_HEADER_LEN,
                available: bytes.len().saturating_sub(offset),
            })?;
        let data_end = data_start.checked_add(size).ok_or(Error::DataTruncated {
            format: "ani",
            offset: data_start,
            needed: size,
            available: bytes.len().saturating_sub(data_start.min(bytes.len())),
        })?;
        let data = bytes
            .get(data_start..data_end)
            .ok_or(Error::DataTruncated {
                format: "ani",
                offset: data_start,
                needed: size,
                available: bytes.len().saturating_sub(data_start.min(bytes.len())),
            })?;

        match &id {
            b"anih" => {
                if data.len() < ANIH_LEN {
                    return Err(Error::HeaderTruncated {
                        format: "ani anih",
                        len: data.len(),
                        needed: ANIH_LEN,
                        offset: data_start,
                    });
                }
                n_steps = u32_at(data, 8).map(|v| usize::try_from(v).unwrap_or(0));
                disp_rate = u32_at(data, 28).unwrap_or(6);
            }
            b"rate" => {
                rate = Some(read_u32_array(data));
            }
            b"seq " => {
                seq = Some(read_u32_array(data));
            }
            b"LIST" => {
                let list_type = array_at::<4>(data, 0);
                let list_body = data.get(4..).unwrap_or(&[]);
                if list_type == Some(*b"fram") {
                    frames = read_frames(list_body)?;
                }
            }
            _ => {}
        }

        let padded = size
            .checked_add(size.checked_rem(2).unwrap_or(0))
            .unwrap_or(size);
        offset = data_start.checked_add(padded).ok_or(Error::DataTruncated {
            format: "ani",
            offset: data_start,
            needed: padded,
            available: 0,
        })?;
    }

    if frames.is_empty() {
        return Err(Error::NoFrames { format: "ani" });
    }
    let steps = n_steps.filter(|&n| n > 0).unwrap_or(frames.len());
    let sequence = seq.unwrap_or_else(|| (0..u32::try_from(frames.len()).unwrap_or(0)).collect());
    let sequence = if sequence.len() >= steps {
        sequence
    } else {
        (0..u32::try_from(frames.len()).unwrap_or(0)).collect()
    };
    let rate_jiffies = rate
        .filter(|r| r.len() >= sequence.len())
        .unwrap_or_else(|| vec![disp_rate; sequence.len()]);

    Ok(AnimatedCursor {
        frames,
        sequence,
        rate_jiffies,
    })
}

fn read_frames(body: &[u8]) -> Result<Vec<CursorImage>> {
    let mut frames = Vec::new();
    let mut offset = 0usize;
    while let Some(header) = body.get(offset..) {
        if header.len() < CHUNK_HEADER_LEN {
            break;
        }
        let id = array_at::<4>(body, offset).ok_or(Error::DataTruncated {
            format: "ani fram",
            offset,
            needed: CHUNK_HEADER_LEN,
            available: body.len().saturating_sub(offset),
        })?;
        let size = u32_at(body, offset.saturating_add(4)).ok_or(Error::DataTruncated {
            format: "ani fram",
            offset,
            needed: CHUNK_HEADER_LEN,
            available: body.len().saturating_sub(offset),
        })?;
        let size = usize::try_from(size).map_err(|_| Error::DataTruncated {
            format: "ani fram",
            offset,
            needed: 0,
            available: 0,
        })?;
        let data_start = offset
            .checked_add(CHUNK_HEADER_LEN)
            .ok_or(Error::DataTruncated {
                format: "ani fram",
                offset,
                needed: CHUNK_HEADER_LEN,
                available: body.len().saturating_sub(offset),
            })?;
        let data_end = data_start.checked_add(size).ok_or(Error::DataTruncated {
            format: "ani fram",
            offset: data_start,
            needed: size,
            available: body.len().saturating_sub(data_start.min(body.len())),
        })?;
        let data = body.get(data_start..data_end).ok_or(Error::DataTruncated {
            format: "ani fram",
            offset: data_start,
            needed: size,
            available: body.len().saturating_sub(data_start.min(body.len())),
        })?;
        if id == *b"icon" {
            frames.push(crate::cur::decode(data)?);
        }
        let padded = size
            .checked_add(size.checked_rem(2).unwrap_or(0))
            .unwrap_or(size);
        offset = data_start.checked_add(padded).ok_or(Error::DataTruncated {
            format: "ani fram",
            offset: data_start,
            needed: padded,
            available: 0,
        })?;
    }
    Ok(frames)
}

fn read_u32_array(data: &[u8]) -> Vec<u32> {
    let count = data.len().checked_div(4).unwrap_or(0);
    (0..count)
        .filter_map(|i| u32_at(data, i.checked_mul(4)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::AniBuilder;

    #[test]
    fn a_two_frame_animation_decodes_both_frames_in_order() {
        let bytes = AniBuilder::new(6)
            .frame_1bpp(&[(0, 0, 0), (255, 255, 255)], &[0b1000_0000], &[0])
            .frame_1bpp(&[(0, 0, 0), (255, 255, 255)], &[0b0000_0000], &[0])
            .build();
        let anim = decode(&bytes).unwrap();
        assert_eq!(anim.frames().len(), 2);
        assert_eq!(anim.steps(), 2);
        let (first, jiffies) = anim.step(0).unwrap();
        assert_eq!(first.pixels(), &[255, 255, 255, 255]);
        assert_eq!(jiffies, 6);
        let (second, _) = anim.step(1).unwrap();
        assert_eq!(second.pixels(), &[0, 0, 0, 255]);
        assert_eq!(anim.step(2).unwrap().0.pixels(), first.pixels());
    }

    #[test]
    fn an_unknown_list_type_is_skipped_by_size_not_descended_into() {
        let bytes = AniBuilder::new(6)
            .frame_1bpp(&[(0, 0, 0), (255, 255, 255)], &[0b1000_0000], &[0])
            .with_watermark_list(b"Crea", b"Created or Modified by IconCool Editor")
            .build();
        let anim = decode(&bytes).unwrap();
        assert_eq!(anim.frames().len(), 1);
    }

    #[test]
    fn declaring_the_whole_file_length_as_the_riff_size_does_not_overrun() {
        let mut bytes = AniBuilder::new(6)
            .frame_1bpp(&[(0, 0, 0), (255, 255, 255)], &[0b1000_0000], &[0])
            .build();
        let whole_file_len = u32::try_from(bytes.len()).unwrap();
        bytes[4..8].copy_from_slice(&whole_file_len.to_le_bytes());
        assert!(decode(&bytes).is_ok());
    }

    #[test]
    fn truncated_bytes_are_an_error_not_a_panic() {
        let bytes = AniBuilder::new(6)
            .frame_1bpp(&[(0, 0, 0), (255, 255, 255)], &[0b1000_0000], &[0])
            .build();
        for len in 0..bytes.len() {
            assert!(
                decode(&bytes[..len]).is_err(),
                "len {len} should not decode"
            );
        }
    }

    #[test]
    fn no_frames_at_all_is_an_error_not_a_panic() {
        assert!(matches!(
            decode(&AniBuilder::new(6).build()),
            Err(Error::NoFrames { .. })
        ));
    }
}
