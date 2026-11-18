#![allow(clippy::arithmetic_side_effects, clippy::cast_possible_truncation)]

use super::*;

fn riff(chunks: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut body = b"WAVE".to_vec();
    for (id, payload) in chunks {
        body.extend_from_slice(id);
        body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        body.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn fmt_pcm(channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let block_align = channels * bits / 8;
    let mut v = Vec::new();
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    v.extend_from_slice(&block_align.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v
}

fn fmt_ima(channels: u16, rate: u32, block_align: u16, samples_per_block: u16) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&17u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&block_align.to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&samples_per_block.to_le_bytes());
    v
}

fn pcm16_data(samples: &[i16]) -> Vec<u8> {
    samples.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn ima_block(predictor: i16, index: u8, nibbles: &[u8], block_align: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&predictor.to_le_bytes());
    v.push(index);
    v.push(0);
    v.extend_from_slice(nibbles);
    v.resize(block_align, 0);
    v
}

#[test]
fn pcm16_decodes_the_samples_it_was_given() {
    let want: Vec<i16> = vec![0, 1, -1, 32767, -32768, 100];
    let file = riff(&[
        (*b"fmt ", fmt_pcm(1, 22050, 16)),
        (*b"data", pcm16_data(&want)),
    ]);
    let pcm = decode_wav(&file).unwrap();
    assert_eq!(pcm.samples, want);
    assert_eq!(pcm.sample_rate, 22050);
    assert_eq!(pcm.channels, 1);
    assert_eq!(pcm.frames(), 6);
}

#[test]
fn pcm8_is_centred_on_128() {
    let file = riff(&[
        (*b"fmt ", fmt_pcm(1, 11025, 8)),
        (*b"data", vec![128, 129, 127, 255, 0]),
    ]);
    let pcm = decode_wav(&file).unwrap();
    assert_eq!(pcm.samples, vec![0, 256, -256, 32512, -32768]);
}

#[test]
fn duration_is_exact_integer_milliseconds() {
    let one_second = riff(&[
        (*b"fmt ", fmt_pcm(1, 44100, 16)),
        (*b"data", pcm16_data(&vec![0i16; 44100])),
    ]);
    assert_eq!(probe_wav(&one_second).unwrap().duration_ms(), 1000);

    let half = riff(&[
        (*b"fmt ", fmt_pcm(2, 22050, 16)),
        (*b"data", pcm16_data(&vec![0i16; 11025 * 2])),
    ]);
    let info = probe_wav(&half).unwrap();
    assert_eq!(info.frames, 11025);
    assert_eq!(info.duration_ms(), 500);
}

#[test]
fn duration_needs_no_decode() {
    let file = riff(&[
        (*b"fmt ", fmt_pcm(1, 48000, 16)),
        (*b"data", pcm16_data(&vec![7i16; 24000])),
    ]);
    let info = probe_wav(&file).unwrap();
    assert_eq!(info.duration_ms(), 500);
    assert_eq!(decode_wav(&file).unwrap().frames() as u64, info.frames);
}

#[test]
fn stock_block_relation_holds_for_the_shipped_shape() {
    let format = WaveFormat {
        tag: WaveTag::ImaAdpcm,
        raw_tag: 17,
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 4,
        block_align: 1024,
        declared_bytes_per_second: 0,
        samples_per_block: 2041,
        samples_per_block_declared: true,
    };
    assert_eq!(format.stock_samples_per_block(), Some(2041));
}

#[test]
fn adpcm_zero_nibbles_hold_the_predictor() {
    let block = ima_block(1234, 0, &[0u8; 12], 16);
    let file = riff(&[(*b"fmt ", fmt_ima(1, 22050, 16, 25)), (*b"data", block)]);
    let pcm = decode_wav(&file).unwrap();
    assert_eq!(pcm.samples.len(), 25);
    assert!(pcm.samples.iter().all(|s| *s == 1234), "{:?}", pcm.samples);
}

#[test]
fn adpcm_follows_the_published_step_and_index_tables() {
    let block = ima_block(0, 0, &[0x44, 0, 0, 0], 8);
    let file = riff(&[(*b"fmt ", fmt_ima(1, 22050, 8, 9)), (*b"data", block)]);
    let pcm = decode_wav(&file).unwrap();
    assert_eq!(pcm.samples.first().copied(), Some(0));
    assert_eq!(pcm.samples.get(1).copied(), Some(7));
    assert_eq!(pcm.samples.get(2).copied(), Some(17));
}

#[test]
fn adpcm_stereo_interleaves_by_four_byte_group() {
    let mut block = Vec::new();
    block.extend_from_slice(&100i16.to_le_bytes());
    block.push(0);
    block.push(0);
    block.extend_from_slice(&(-100i16).to_le_bytes());
    block.push(0);
    block.push(0);
    block.extend_from_slice(&[0u8; 8]);
    let file = riff(&[(*b"fmt ", fmt_ima(2, 22050, 16, 9)), (*b"data", block)]);
    let pcm = decode_wav(&file).unwrap();
    assert_eq!(pcm.channels, 2);
    assert_eq!(pcm.frames(), 9);
    for frame in 0..9 {
        assert_eq!(pcm.samples.get(frame * 2).copied(), Some(100));
        assert_eq!(pcm.samples.get(frame * 2 + 1).copied(), Some(-100));
    }
}

#[test]
fn a_data_chunk_larger_than_the_file_is_clamped_not_believed() {
    let mut file = riff(&[
        (*b"fmt ", fmt_pcm(1, 44100, 16)),
        (*b"data", pcm16_data(&[1, 2, 3, 4])),
    ]);
    let at = file.len() - 8 - 4;
    file[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let info = probe_wav(&file).unwrap();
    assert_eq!(info.data_bytes, 8);
    assert_eq!(info.frames, 4);
    assert!(info.is_truncated());
    assert_eq!(decode_wav(&file).unwrap().samples.len(), 4);
}

#[test]
fn a_fact_chunk_lowers_frames_but_never_raises_them() {
    let low = riff(&[
        (*b"fmt ", fmt_pcm(1, 44100, 16)),
        (*b"fact", 3u32.to_le_bytes().to_vec()),
        (*b"data", pcm16_data(&[1, 2, 3, 4, 5])),
    ]);
    assert_eq!(probe_wav(&low).unwrap().frames, 3);

    let high = riff(&[
        (*b"fmt ", fmt_pcm(1, 44100, 16)),
        (*b"fact", 9_999_999u32.to_le_bytes().to_vec()),
        (*b"data", pcm16_data(&[1, 2, 3, 4, 5])),
    ]);
    assert_eq!(probe_wav(&high).unwrap().frames, 5);
    assert_eq!(decode_wav(&high).unwrap().samples.len(), 5);
}

#[test]
fn an_overstated_samples_per_block_falls_back_to_the_relation() {
    let block = ima_block(0, 0, &[0u8; 12], 16);
    let file = riff(&[(*b"fmt ", fmt_ima(1, 22050, 16, 60000)), (*b"data", block)]);
    let info = probe_wav(&file).unwrap();
    assert_eq!(info.format.samples_per_block, 25);
    assert!(!info.format.samples_per_block_declared);
    assert_eq!(decode_wav(&file).unwrap().samples.len(), 25);
}

#[test]
fn extensible_resolves_to_its_subformat() {
    let mut fmt = fmt_pcm(1, 44100, 16);
    fmt[0..2].copy_from_slice(&0xFFFEu16.to_le_bytes());
    fmt.extend_from_slice(&22u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    fmt.extend_from_slice(&4u32.to_le_bytes());
    fmt.extend_from_slice(&1u32.to_le_bytes());
    fmt.extend_from_slice(&[0u8; 12]);
    let file = riff(&[(*b"fmt ", fmt), (*b"data", pcm16_data(&[5, 6]))]);
    let info = probe_wav(&file).unwrap();
    assert_eq!(info.format.tag, WaveTag::Pcm);
    assert_eq!(info.format.raw_tag, 1);
}

#[test]
fn zero_channels_and_zero_rate_are_rejected() {
    for (offset, width, field) in [(2usize, 2usize, "nChannels"), (4, 4, "nSamplesPerSec")] {
        let mut fmt = fmt_pcm(1, 44100, 16);
        fmt[offset..offset + width].fill(0);
        let file = riff(&[(*b"fmt ", fmt), (*b"data", pcm16_data(&[1]))]);
        match probe_wav(&file) {
            Err(Error::ZeroField { field: got, .. }) => assert_eq!(got, field),
            other => panic!("{field}: expected ZeroField, got {other:?}"),
        }
    }
}

#[test]
fn an_unsupported_tag_is_an_error_not_a_panic() {
    let mut fmt = fmt_pcm(1, 44100, 16);
    fmt[0..2].copy_from_slice(&85u16.to_le_bytes());
    let file = riff(&[(*b"fmt ", fmt), (*b"data", vec![0; 64])]);
    assert!(matches!(
        probe_wav(&file),
        Err(Error::UnsupportedFormat { tag: 85, .. })
    ));
}

#[test]
fn a_chunk_size_of_u32_max_terminates() {
    let mut file = riff(&[(*b"fmt ", fmt_pcm(1, 44100, 16))]);
    file.extend_from_slice(b"junk");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&4u32.to_le_bytes());
    file.extend_from_slice(&pcm16_data(&[1, 2]));
    assert!(matches!(probe_wav(&file), Err(Error::NoDataChunk { .. })));
}

#[test]
fn truncation_sweep_never_panics() {
    let nibbles: Vec<u8> = (0..1020u32).map(|i| (i % 251) as u8).collect();
    let file = riff(&[
        (*b"fmt ", fmt_ima(1, 44100, 1024, 2041)),
        (*b"data", ima_block(512, 12, &nibbles, 1024)),
    ]);
    for len in 0..file.len() {
        let prefix = &file[..len];
        let _ = probe_wav(prefix);
        if let Ok(pcm) = decode_wav(prefix) {
            assert!(pcm.samples.len() <= 2041);
        }
    }
}

#[test]
fn corruption_sweep_never_panics() {
    let file = riff(&[
        (*b"fmt ", fmt_ima(2, 44100, 512, 1017)),
        (*b"data", vec![0xA5; 512 * 3]),
    ]);
    for byte in 0..file.len() {
        for bit in 0..8 {
            let mut damaged = file.clone();
            damaged[byte] ^= 1 << bit;
            let _ = probe_wav(&damaged);
            let _ = decode_wav(&damaged);
        }
    }
}

#[test]
fn a_file_with_no_usable_chunks_errors() {
    assert!(matches!(probe_wav(b""), Err(Error::TooShort { len: 0 })));
    assert!(matches!(
        probe_wav(b"RIFX\0\0\0\0WAVE"),
        Err(Error::BadMagic { offset: 0, .. })
    ));
    assert!(matches!(
        probe_wav(b"RIFF\0\0\0\0WAVX"),
        Err(Error::BadMagic { offset: 8, .. })
    ));
    assert!(matches!(
        probe_wav(b"RIFF\0\0\0\0WAVE"),
        Err(Error::NoFormatChunk { .. })
    ));
}
