use mint_bytes::{array_at, i16_at, u16_at, u32_at};

use crate::{Error, Result};

const RIFF_HEADER: usize = 12;

const CHUNK_HEADER: usize = 8;

const FMT_MIN: usize = 16;

const TAG_PCM: u16 = 1;

const TAG_IMA_ADPCM: u16 = 17;

const TAG_EXTENSIBLE: u16 = 0xFFFE;

const MAX_STEP_INDEX: i32 = 88;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WaveTag {
    Pcm,
    ImaAdpcm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveFormat {
    pub tag: WaveTag,
    pub raw_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub block_align: u16,
    pub declared_bytes_per_second: u32,
    pub samples_per_block: u32,
    pub samples_per_block_declared: bool,
}

impl WaveFormat {
    #[must_use]
    pub fn stock_samples_per_block(&self) -> Option<u32> {
        if self.tag != WaveTag::ImaAdpcm {
            return Some(1);
        }
        let channels = u32::from(self.channels);
        let preamble = channels.checked_mul(4)?;
        let payload = u32::from(self.block_align).checked_sub(preamble)?;
        let bits = u32::from(self.bits_per_sample).checked_mul(channels)?;
        if bits == 0 {
            return None;
        }
        payload.checked_mul(8)?.checked_div(bits)?.checked_add(1)
    }

    #[must_use]
    pub fn implied_bytes_per_second(&self) -> Option<u32> {
        let rate = u64::from(self.sample_rate);
        let align = u64::from(self.block_align);
        let per_block = match self.tag {
            WaveTag::Pcm => 1,
            WaveTag::ImaAdpcm => u64::from(self.samples_per_block),
        };
        if per_block == 0 {
            return None;
        }
        u32::try_from(rate.checked_mul(align)?.checked_div(per_block)?).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveInfo {
    pub format: WaveFormat,
    pub data_offset: usize,
    pub data_bytes: usize,
    pub declared_data_bytes: u64,
    pub fact_frames: Option<u32>,
    pub frames: u64,
}

impl WaveInfo {
    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        let rate = u64::from(self.format.sample_rate);
        if rate == 0 {
            return 0;
        }
        self.frames
            .saturating_mul(1000)
            .checked_div(rate)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn is_truncated(&self) -> bool {
        self.declared_data_bytes > self.data_bytes as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcm16 {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl Pcm16 {
    #[must_use]
    pub fn frames(&self) -> usize {
        let channels = usize::from(self.channels);
        if channels == 0 {
            return 0;
        }
        self.samples.len().checked_div(channels).unwrap_or(0)
    }
}

pub fn probe_wav(bytes: &[u8]) -> Result<WaveInfo> {
    if bytes.len() < RIFF_HEADER {
        return Err(Error::TooShort { len: bytes.len() });
    }
    let riff = array_at::<4>(bytes, 0).ok_or(Error::TooShort { len: bytes.len() })?;
    if &riff != b"RIFF" {
        return Err(Error::BadMagic {
            found: riff,
            offset: 0,
        });
    }
    let wave = array_at::<4>(bytes, 8).ok_or(Error::TooShort { len: bytes.len() })?;
    if &wave != b"WAVE" {
        return Err(Error::BadMagic {
            found: wave,
            offset: 8,
        });
    }

    let mut fmt: Option<(usize, usize)> = None;
    let mut data: Option<(usize, usize, u64)> = None;
    let mut fact: Option<u32> = None;
    let mut pos = RIFF_HEADER;

    while let Some(payload_start) = pos.checked_add(CHUNK_HEADER) {
        if payload_start > bytes.len() {
            break;
        }
        let Some(id) = array_at::<4>(bytes, pos) else {
            break;
        };
        let Some(declared) = u32_at(bytes, pos.saturating_add(4)) else {
            break;
        };
        let available = bytes.len().saturating_sub(payload_start);
        let present = (declared as usize).min(available);

        match &id {
            b"fmt " => fmt = fmt.or(Some((payload_start, present))),
            b"data" => data = data.or(Some((payload_start, present, u64::from(declared)))),
            b"fact" => fact = fact.or_else(|| u32_at(bytes, payload_start)),
            _ => {}
        }

        let padded = (declared as usize).saturating_add((declared & 1) as usize);
        pos = payload_start.saturating_add(padded);
    }

    let (fmt_at, fmt_len) = fmt.ok_or(Error::NoFormatChunk {
        scanned: bytes.len(),
    })?;
    let format = parse_format(bytes, fmt_at, fmt_len)?;
    let (data_offset, data_bytes, declared_data_bytes) = data.ok_or(Error::NoDataChunk {
        scanned: bytes.len(),
    })?;

    let derived = frames_from_bytes(&format, data_bytes);
    let frames = match fact {
        Some(f) => derived.min(u64::from(f)),
        None => derived,
    };

    Ok(WaveInfo {
        format,
        data_offset,
        data_bytes,
        declared_data_bytes,
        fact_frames: fact,
        frames,
    })
}

pub fn decode_wav(bytes: &[u8]) -> Result<Pcm16> {
    let info = probe_wav(bytes)?;
    let end = info.data_offset.saturating_add(info.data_bytes);
    let data = bytes.get(info.data_offset..end).unwrap_or(&[]);
    let channels = usize::from(info.format.channels);
    let wanted = usize::try_from(info.frames)
        .unwrap_or(usize::MAX)
        .saturating_mul(channels);

    let mut samples = match info.format.tag {
        WaveTag::Pcm => decode_pcm(&info.format, data)?,
        WaveTag::ImaAdpcm => decode_ima(&info.format, data),
    };
    if samples.len() > wanted {
        samples.truncate(wanted);
    }

    Ok(Pcm16 {
        sample_rate: info.format.sample_rate,
        channels: info.format.channels,
        samples,
    })
}

fn parse_format(bytes: &[u8], at: usize, len: usize) -> Result<WaveFormat> {
    if len < FMT_MIN {
        return Err(Error::ShortFormatChunk { offset: at, len });
    }
    let read16 = |o: usize| u16_at(bytes, at.saturating_add(o));
    let read32 = |o: usize| u32_at(bytes, at.saturating_add(o));

    let short = || Error::ShortFormatChunk { offset: at, len };
    let mut raw_tag = read16(0).ok_or_else(short)?;
    let channels = read16(2).ok_or_else(short)?;
    let sample_rate = read32(4).ok_or_else(short)?;
    let declared_bytes_per_second = read32(8).ok_or_else(short)?;
    let block_align = read16(12).ok_or_else(short)?;
    let bits_per_sample = read16(14).ok_or_else(short)?;

    if raw_tag == TAG_EXTENSIBLE {
        if let Some(sub) = read32(24) {
            raw_tag = (sub & 0xFFFF) as u16;
        }
    }

    if channels == 0 {
        return Err(Error::ZeroField {
            offset: at,
            field: "nChannels",
        });
    }
    if sample_rate == 0 {
        return Err(Error::ZeroField {
            offset: at,
            field: "nSamplesPerSec",
        });
    }
    if block_align == 0 {
        return Err(Error::ZeroField {
            offset: at,
            field: "nBlockAlign",
        });
    }

    let tag = match (raw_tag, bits_per_sample) {
        (TAG_PCM, 8 | 16) => WaveTag::Pcm,
        (TAG_IMA_ADPCM, 4) => WaveTag::ImaAdpcm,
        _ => {
            return Err(Error::UnsupportedFormat {
                tag: raw_tag,
                bits: bits_per_sample,
                channels,
            })
        }
    };

    let mut format = WaveFormat {
        tag,
        raw_tag,
        channels,
        sample_rate,
        bits_per_sample,
        block_align,
        declared_bytes_per_second,
        samples_per_block: 1,
        samples_per_block_declared: false,
    };

    if tag == WaveTag::ImaAdpcm {
        let preamble = u32::from(channels).saturating_mul(4);
        if u32::from(block_align) < preamble {
            return Err(Error::ShortAdpcmBlock {
                block_align,
                needed: preamble,
                channels,
            });
        }
        let declared = if len >= 20 && read16(16).is_some_and(|cb| cb >= 2) {
            read16(18).map(u32::from).filter(|v| *v > 0)
        } else {
            None
        };
        let stock = format.stock_samples_per_block();
        match (declared, stock) {
            (Some(d), Some(s)) if d <= s => {
                format.samples_per_block = d;
                format.samples_per_block_declared = true;
            }
            (_, Some(s)) => format.samples_per_block = s,
            (Some(_) | None, None) => {
                return Err(Error::ShortAdpcmBlock {
                    block_align,
                    needed: preamble,
                    channels,
                })
            }
        }
    }

    Ok(format)
}

fn frames_from_bytes(format: &WaveFormat, data_bytes: usize) -> u64 {
    let block = usize::from(format.block_align);
    if block == 0 {
        return 0;
    }
    let full = data_bytes.checked_div(block).unwrap_or(0) as u64;
    let remainder = data_bytes.checked_rem(block).unwrap_or(0);

    match format.tag {
        WaveTag::Pcm => full,
        WaveTag::ImaAdpcm => {
            let row = usize::from(format.channels).saturating_mul(4);
            let partial = if row == 0 || remainder < row {
                0u64
            } else {
                let rows = remainder.saturating_sub(row).checked_div(row).unwrap_or(0) as u64;
                rows.saturating_mul(8).saturating_add(1)
            };
            full.saturating_mul(u64::from(format.samples_per_block))
                .saturating_add(partial)
        }
    }
}

fn decode_pcm(format: &WaveFormat, data: &[u8]) -> Result<Vec<i16>> {
    match format.bits_per_sample {
        16 => {
            let count = data.len().checked_div(2).unwrap_or(0);
            let mut out = Vec::with_capacity(count);
            for i in 0..count {
                out.push(i16_at(data, i.saturating_mul(2)).unwrap_or(0));
            }
            Ok(out)
        }
        8 => {
            let mut out = Vec::with_capacity(data.len());
            for byte in data {
                let centred = i16::from(*byte).saturating_sub(128);
                out.push(centred.saturating_mul(256));
            }
            Ok(out)
        }
        bits => Err(Error::UnsupportedFormat {
            tag: format.raw_tag,
            bits,
            channels: format.channels,
        }),
    }
}

const STEP_TABLE: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const INDEX_TABLE: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

#[derive(Clone, Copy)]
struct ImaState {
    predictor: i32,
    index: i32,
}

impl ImaState {
    fn step(&mut self, nibble: u8) -> i16 {
        let idx = usize::try_from(self.index.clamp(0, MAX_STEP_INDEX)).unwrap_or(0);
        let step = STEP_TABLE.get(idx).copied().unwrap_or(7);

        let mut diff = step >> 3;
        if nibble & 4 != 0 {
            diff = diff.saturating_add(step);
        }
        if nibble & 2 != 0 {
            diff = diff.saturating_add(step >> 1);
        }
        if nibble & 1 != 0 {
            diff = diff.saturating_add(step >> 2);
        }
        self.predictor = if nibble & 8 != 0 {
            self.predictor.saturating_sub(diff)
        } else {
            self.predictor.saturating_add(diff)
        }
        .clamp(-32768, 32767);

        let adjust = INDEX_TABLE
            .get(usize::from(nibble & 0x0F))
            .copied()
            .unwrap_or(-1);
        self.index = self.index.saturating_add(adjust).clamp(0, MAX_STEP_INDEX);

        i16::try_from(self.predictor).unwrap_or(0)
    }
}

fn decode_ima(format: &WaveFormat, data: &[u8]) -> Vec<i16> {
    let channels = usize::from(format.channels);
    let block_align = usize::from(format.block_align);
    if channels == 0 || block_align == 0 {
        return Vec::new();
    }
    let preamble = channels.saturating_mul(4);
    let blocks = data.len().div_ceil(block_align);
    let capacity = blocks
        .saturating_mul(format.samples_per_block as usize)
        .saturating_mul(channels);
    let mut out = vec![0i16; capacity];
    let mut frame_base = 0usize;
    let mut offset = 0usize;

    while offset < data.len() {
        let end = offset.saturating_add(block_align).min(data.len());
        let Some(block) = data.get(offset..end) else {
            break;
        };
        if block.len() < preamble {
            break;
        }

        let mut state = vec![
            ImaState {
                predictor: 0,
                index: 0
            };
            channels
        ];
        for (ch, st) in state.iter_mut().enumerate() {
            let at = ch.saturating_mul(4);
            st.predictor = i32::from(i16_at(block, at).unwrap_or(0));
            st.index = i32::from(block.get(at.saturating_add(2)).copied().unwrap_or(0))
                .clamp(0, MAX_STEP_INDEX);
            let slot = frame_base.saturating_mul(channels).saturating_add(ch);
            if let Some(cell) = out.get_mut(slot) {
                *cell = i16::try_from(st.predictor.clamp(-32768, 32767)).unwrap_or(0);
            }
        }

        let mut group = preamble;
        let mut frame = frame_base.saturating_add(1);
        'rows: while group.saturating_add(preamble) <= block.len() {
            for (ch, st) in state.iter_mut().enumerate() {
                let base = group.saturating_add(ch.saturating_mul(4));
                for byte_index in 0..4usize {
                    let Some(byte) = block.get(base.saturating_add(byte_index)).copied() else {
                        break 'rows;
                    };
                    for half in 0..2usize {
                        let nibble = if half == 0 { byte & 0x0F } else { byte >> 4 };
                        let value = st.step(nibble);
                        let slot = frame
                            .saturating_add(byte_index.saturating_mul(2))
                            .saturating_add(half)
                            .saturating_mul(channels)
                            .saturating_add(ch);
                        if let Some(cell) = out.get_mut(slot) {
                            *cell = value;
                        }
                    }
                }
            }
            group = group.saturating_add(preamble);
            frame = frame.saturating_add(8);
        }

        frame_base = frame_base.saturating_add(format.samples_per_block as usize);
        offset = offset.saturating_add(block_align);
    }

    out
}

#[cfg(test)]
mod tests;
