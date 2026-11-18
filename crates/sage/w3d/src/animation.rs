use std::sync::OnceLock;

use crate::array::Element;
use crate::chunk::Chunk;
use crate::reader::Reader;
use crate::{ChunkType, Error, Quat, Result};

const FILTER_TABLE_LEN: usize = 256;

fn filter_table() -> &'static [f32; FILTER_TABLE_LEN] {
    static TABLE: OnceLock<[f32; FILTER_TABLE_LEN]> = OnceLock::new();
    TABLE.get_or_init(build_filter_table)
}

fn build_filter_table() -> [f32; FILTER_TABLE_LEN] {
    let mut table = [0f32; FILTER_TABLE_LEN];
    const POWERS_OF_TEN: [f32; 16] = [
        1e-8, 1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7,
    ];
    if let Some(head) = table.get_mut(..16) {
        head.copy_from_slice(&POWERS_OF_TEN);
    }
    for i in 0..240usize {
        let degrees = 90.0 * (i as f32) / 240.0;
        let value = 1.0 - mint_math::f32::sin(degrees.to_radians());
        if let Some(slot) = i.checked_add(16).and_then(|idx| table.get_mut(idx)) {
            *slot = value;
        }
    }
    table
}

fn filter_step(index: u8) -> f32 {
    filter_table()
        .get(usize::from(index))
        .copied()
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    TranslationX,
    TranslationY,
    TranslationZ,
    Rotation,
    Unestablished,
    Visibility,
    Other(u16),
}

impl ChannelKind {
    fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::TranslationX,
            1 => Self::TranslationY,
            2 => Self::TranslationZ,
            6 => Self::Rotation,
            15 => Self::Unestablished,
            other => Self::Other(other),
        }
    }

    #[must_use]
    pub fn vector_len(self) -> u32 {
        match self {
            Self::Rotation => 4,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Scalar(f32),
    Rotation(Quat),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeCodedKey {
    pub frame: u32,
    pub value: Value,
    pub hold: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChannelData {
    Dense {
        first_frame: u32,
        values: Vec<Value>,
    },
    Bits {
        first_frame: u32,
        last_frame: u32,
        default: bool,
        packed: Vec<u8>,
    },
    TimeCoded(Vec<TimeCodedKey>),
    SparseBits {
        default: bool,
        keys: Vec<(u32, bool)>,
    },
    AdaptiveDelta {
        values: Vec<Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub pivot: u32,
    pub kind: ChannelKind,
    pub declared_vector_len: Option<u32>,
    pub data: ChannelData,
}

impl Channel {
    #[must_use]
    pub fn vector_len_matches_type(&self) -> bool {
        match self.declared_vector_len {
            Some(declared) => declared == self.kind.vector_len(),
            None => true,
        }
    }

    pub(crate) fn parse(chunk: &Chunk<'_>, header_flavor: Option<u16>) -> Result<Option<Self>> {
        let decoded = match chunk.kind() {
            ChunkType::ANIMATION_CHANNEL => Self::parse_dense(chunk)?,
            ChunkType::BIT_CHANNEL => Self::parse_bits(chunk)?,
            ChunkType::COMPRESSED_ANIMATION_CHANNEL => {
                Self::parse_compressed(chunk, header_flavor.unwrap_or(0))?
            }
            ChunkType::COMPRESSED_BIT_CHANNEL => Self::parse_compressed_bit(chunk)?,
            ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL => Self::parse_motion(chunk)?,
            _ => return Ok(None),
        };
        Ok(Some(decoded))
    }

    fn parse_dense(chunk: &Chunk<'_>) -> Result<Self> {
        let mut r = chunk.reader();
        let first_frame = u32::from(r.u16()?);
        let last_frame = u32::from(r.u16()?);
        let declared_vector_len = u32::from(r.u16()?);
        let type_raw = r.u16()?;
        let pivot = u32::from(r.u16()?);
        r.skip(2)?;

        if last_frame < first_frame {
            return Err(Error::InvalidFrameRange {
                what: "ANIMATION_CHANNEL",
                offset: chunk.offset(),
                first: first_frame,
                last: last_frame,
            });
        }
        let kind = ChannelKind::from_raw(type_raw);
        let vector_len = kind.vector_len();
        let frame_count = last_frame.saturating_sub(first_frame).saturating_add(1);

        let capacity = usize::try_from(frame_count).unwrap_or(0).min(r.remaining());
        let mut values = Vec::with_capacity(capacity);
        for _ in 0..frame_count {
            values.push(read_value(&mut r, vector_len)?);
        }

        Ok(Self {
            pivot,
            kind,
            declared_vector_len: Some(declared_vector_len),
            data: ChannelData::Dense {
                first_frame,
                values,
            },
        })
    }

    fn parse_bits(chunk: &Chunk<'_>) -> Result<Self> {
        let mut r = chunk.reader();
        let first_frame = u32::from(r.u16()?);
        let last_frame = u32::from(r.u16()?);
        r.skip(2)?;
        let pivot = u32::from(r.u16()?);
        let default = r.u8()? != 0;

        if last_frame < first_frame {
            return Err(Error::InvalidFrameRange {
                what: "BIT_CHANNEL",
                offset: chunk.offset(),
                first: first_frame,
                last: last_frame,
            });
        }
        let bit_count = last_frame.saturating_sub(first_frame).saturating_add(1);
        let byte_count = usize::try_from(bit_count.saturating_add(7) / 8).unwrap_or(0);
        let packed = r.take(byte_count)?.to_vec();

        Ok(Self {
            pivot,
            kind: ChannelKind::Visibility,
            declared_vector_len: None,
            data: ChannelData::Bits {
                first_frame,
                last_frame,
                default,
                packed,
            },
        })
    }

    fn parse_compressed(chunk: &Chunk<'_>, header_flavor: u16) -> Result<Self> {
        let mut r = chunk.reader();
        let count = r.u32()?;
        let pivot = u32::from(r.u16()?);
        let declared_vector_len = u32::from(r.u8()?);
        let kind = ChannelKind::from_raw(u16::from(r.u8()?));
        let vector_len = kind.vector_len();

        let data = match header_flavor {
            1 => {
                let scale = r.f32()?;
                let frame0 = read_raw_vector(&mut r, vector_len)?;
                let values = reconstruct_adaptive_delta(
                    &mut r,
                    vector_len,
                    DeltaBits::Four,
                    scale,
                    &frame0,
                    count,
                )?;
                ChannelData::AdaptiveDelta { values }
            }
            0 => ChannelData::TimeCoded(read_time_coded_32(&mut r, vector_len, count)?),
            other => {
                return Err(Error::UnknownChannelEncoding {
                    what: "COMPRESSED_ANIMATION_HEADER flavor",
                    offset: chunk.offset(),
                    value: other,
                })
            }
        };

        Ok(Self {
            pivot,
            kind,
            declared_vector_len: Some(declared_vector_len),
            data,
        })
    }

    fn parse_compressed_bit(chunk: &Chunk<'_>) -> Result<Self> {
        let mut r = chunk.reader();
        let count = r.u32()?;
        let pivot = u32::from(r.u16()?);
        r.skip(1)?;
        let default = r.u8()? != 0;

        let capacity = usize::try_from(count)
            .unwrap_or(0)
            .min(r.remaining().checked_div(4).unwrap_or(0));
        let mut keys = Vec::with_capacity(capacity);
        for _ in 0..count {
            let raw = r.u32()?;
            let frame = raw & 0x7FFF_FFFF;
            let value = raw & 0x8000_0000 != 0;
            keys.push((frame, value));
        }

        Ok(Self {
            pivot,
            kind: ChannelKind::Visibility,
            declared_vector_len: None,
            data: ChannelData::SparseBits { default, keys },
        })
    }

    fn parse_motion(chunk: &Chunk<'_>) -> Result<Self> {
        let mut r = chunk.reader();
        r.skip(1)?;
        let delta_type = r.u8()?;
        let declared_vector_len = u32::from(r.u8()?);
        let kind = ChannelKind::from_raw(u16::from(r.u8()?));
        let vector_len = kind.vector_len();
        let count = r.u16()?;
        let pivot = u32::from(r.u16()?);

        let data = match delta_type {
            0 => ChannelData::TimeCoded(read_motion_time_coded(&mut r, vector_len, count)?),
            1 | 2 => {
                let bits = if delta_type == 1 {
                    DeltaBits::Four
                } else {
                    DeltaBits::Eight
                };
                let scale = r.f32()?;
                let frame0 = read_raw_vector(&mut r, vector_len)?;
                let values = reconstruct_adaptive_delta(
                    &mut r,
                    vector_len,
                    bits,
                    scale,
                    &frame0,
                    u32::from(count),
                )?;
                ChannelData::AdaptiveDelta { values }
            }
            other => {
                return Err(Error::UnknownChannelEncoding {
                    what: "COMPRESSED_ANIMATION_MOTION_CHANNEL delta_type",
                    offset: chunk.offset(),
                    value: u16::from(other),
                })
            }
        };

        Ok(Self {
            pivot,
            kind,
            declared_vector_len: Some(declared_vector_len),
            data,
        })
    }
}

fn read_value(r: &mut Reader<'_>, vector_len: u32) -> Result<Value> {
    if vector_len == 4 {
        Ok(Value::Rotation(Quat::read(r)?))
    } else {
        Ok(Value::Scalar(r.f32()?))
    }
}

fn read_raw_vector(r: &mut Reader<'_>, vector_len: u32) -> Result<Vec<f32>> {
    let n = usize::try_from(vector_len).unwrap_or(0);
    let mut out = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        out.push(r.f32()?);
    }
    Ok(out)
}

fn vector_len_bytes(vector_len: u32) -> usize {
    usize::try_from(vector_len).unwrap_or(0).saturating_mul(4)
}

fn read_time_coded_32(
    r: &mut Reader<'_>,
    vector_len: u32,
    count: u32,
) -> Result<Vec<TimeCodedKey>> {
    let record_size = 4usize.saturating_add(vector_len_bytes(vector_len)).max(1);
    let capacity = usize::try_from(count)
        .unwrap_or(0)
        .min(r.remaining().checked_div(record_size).unwrap_or(0));
    let mut keys = Vec::with_capacity(capacity);
    for _ in 0..count {
        let raw = r.u32()?;
        let frame = raw & 0x7FFF_FFFF;
        let hold = raw & 0x8000_0000 != 0;
        let value = read_value(r, vector_len)?;
        keys.push(TimeCodedKey { frame, value, hold });
    }
    Ok(keys)
}

fn read_motion_time_coded(
    r: &mut Reader<'_>,
    vector_len: u32,
    count: u16,
) -> Result<Vec<TimeCodedKey>> {
    let time_capacity = usize::from(count).min(r.remaining().checked_div(2).unwrap_or(0));
    let mut frames = Vec::with_capacity(time_capacity);
    let mut holds = Vec::with_capacity(time_capacity);
    for _ in 0..count {
        let raw = r.u16()?;
        frames.push(u32::from(raw & 0x7FFF));
        holds.push(raw & 0x8000 != 0);
    }
    if count % 2 == 1 {
        r.skip(2)?;
    }
    let value_capacity = usize::from(count).min(
        r.remaining()
            .checked_div(vector_len_bytes(vector_len).max(1))
            .unwrap_or(0),
    );
    let mut values = Vec::with_capacity(value_capacity);
    for _ in 0..count {
        values.push(read_value(r, vector_len)?);
    }

    Ok(frames
        .into_iter()
        .zip(holds)
        .zip(values)
        .map(|((frame, hold), value)| TimeCodedKey { frame, value, hold })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeltaBits {
    Four,
    Eight,
}

impl DeltaBits {
    fn block_bytes(self) -> usize {
        match self {
            Self::Four => 9,
            Self::Eight => 17,
        }
    }

    fn bit_scale(self) -> f32 {
        match self {
            Self::Four => 1.0,
            Self::Eight => 1.0 / 16.0,
        }
    }

    fn unpack(self, raw: &[u8]) -> [i32; 16] {
        let mut deltas = [0i32; 16];
        match self {
            Self::Four => {
                for (i, &byte) in raw.iter().take(8).enumerate() {
                    let lo = byte & 0x0F;
                    let hi = (byte >> 4) & 0x0F;
                    if let Some(idx) = i.checked_mul(2) {
                        if let Some(slot) = deltas.get_mut(idx) {
                            *slot = sign_extend_nibble(lo);
                        }
                        if let Some(slot) = idx.checked_add(1).and_then(|j| deltas.get_mut(j)) {
                            *slot = sign_extend_nibble(hi);
                        }
                    }
                }
            }
            Self::Eight => {
                for (i, &byte) in raw.iter().take(16).enumerate() {
                    if let Some(slot) = deltas.get_mut(i) {
                        *slot = i32::from(byte).wrapping_sub(128);
                    }
                }
            }
        }
        deltas
    }
}

fn sign_extend_nibble(n: u8) -> i32 {
    let n = i32::from(n & 0x0F);
    if n >= 8 {
        n.wrapping_sub(16)
    } else {
        n
    }
}

fn reconstruct_adaptive_delta(
    r: &mut Reader<'_>,
    vector_len: u32,
    bits: DeltaBits,
    scale: f32,
    frame0: &[f32],
    n: u32,
) -> Result<Vec<Value>> {
    let vector_len_usize = usize::try_from(vector_len).unwrap_or(0);
    let bytes_per_group = bits.block_bytes().saturating_mul(vector_len_usize.max(1));
    let groups_available = if bytes_per_group == 0 {
        0
    } else {
        r.remaining().checked_div(bytes_per_group).unwrap_or(0)
    };
    let max_frames_from_bytes = groups_available.saturating_mul(16).saturating_add(1);
    let n_usize = usize::try_from(n)
        .unwrap_or(usize::MAX)
        .min(max_frames_from_bytes);
    let groups_needed = n_usize.saturating_sub(1).saturating_add(15) / 16;

    let total_len = n_usize
        .saturating_mul(vector_len_usize)
        .max(vector_len_usize);
    let mut flat = vec![0f32; total_len];
    for (slot, v) in flat.iter_mut().zip(frame0.iter()) {
        *slot = *v;
    }

    for g in 0..groups_needed {
        for c in 0..vector_len_usize {
            let filter_index = r.u8()?;
            let raw = r.take(bits.block_bytes().saturating_sub(1))?;
            let deltas = bits.unpack(raw);
            let step = filter_step(filter_index) * scale * bits.bit_scale();
            for (k, delta) in deltas.iter().enumerate() {
                let frame = g.saturating_mul(16).saturating_add(k).saturating_add(1);
                if frame >= n_usize {
                    continue;
                }
                let prev_idx = frame
                    .saturating_sub(1)
                    .saturating_mul(vector_len_usize)
                    .saturating_add(c);
                let cur_idx = frame.saturating_mul(vector_len_usize).saturating_add(c);
                let prev = flat.get(prev_idx).copied().unwrap_or(0.0);
                let value = prev + step * (*delta as f32);
                if let Some(slot) = flat.get_mut(cur_idx) {
                    *slot = value;
                }
            }
        }
    }

    let mut values = Vec::with_capacity(n_usize);
    for frame in 0..n_usize {
        let base = frame.saturating_mul(vector_len_usize);
        if vector_len_usize == 4 {
            let x = flat.get(base).copied().unwrap_or(0.0);
            let y = flat.get(base.saturating_add(1)).copied().unwrap_or(0.0);
            let z = flat.get(base.saturating_add(2)).copied().unwrap_or(0.0);
            let w = flat.get(base.saturating_add(3)).copied().unwrap_or(0.0);
            values.push(Value::Rotation(Quat { x, y, z, w }));
        } else {
            values.push(Value::Scalar(flat.get(base).copied().unwrap_or(0.0)));
        }
    }
    Ok(values)
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
mod tests;
