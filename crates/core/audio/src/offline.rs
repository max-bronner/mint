use crate::device::{DeviceFormat, Source};

#[must_use]
pub fn render(source: &mut dyn Source, format: DeviceFormat, frames: usize) -> Vec<f32> {
    let channels = usize::from(format.channels.max(1));
    let mut out = vec![0.0f32; frames.saturating_mul(channels)];
    source.render(&mut out, format);
    out
}

#[must_use]
pub fn render_chunked(
    source: &mut dyn Source,
    format: DeviceFormat,
    frames: usize,
    chunk: usize,
) -> Vec<f32> {
    let channels = usize::from(format.channels.max(1));
    let chunk = chunk.max(1);
    let mut out: Vec<f32> = Vec::with_capacity(frames.saturating_mul(channels));
    let mut buffer = vec![0.0f32; chunk.saturating_mul(channels)];
    let mut done = 0usize;
    while done < frames {
        let want = chunk.min(frames.saturating_sub(done));
        let slice = buffer
            .get_mut(..want.saturating_mul(channels))
            .unwrap_or(&mut []);
        source.render(slice, format);
        out.extend_from_slice(slice);
        done = done.saturating_add(want);
    }
    out
}

#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
pub fn to_wav(samples: &[f32], format: DeviceFormat) -> Vec<u8> {
    let channels = format.channels.max(1);
    let block_align = channels.saturating_mul(2);
    let byte_rate = format.sample_rate.saturating_mul(u32::from(block_align));
    let data_len = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX);

    let mut out = Vec::with_capacity(samples.len().saturating_mul(2).saturating_add(44));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data_len.saturating_add(36).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&format.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * 32767.0) as i32;
        out.extend_from_slice(&(scaled.clamp(-32768, 32767) as i16).to_le_bytes());
    }
    out
}
