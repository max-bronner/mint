use super::*;
use crate::testutil::chunk;
use crate::Chunks;

fn one_chunk(bytes: &[u8]) -> Chunk<'_> {
    Chunks::new(bytes)
        .next()
        .and_then(Result::ok)
        .expect("one chunk")
}

#[test]
fn filter_table_first_sixteen_are_powers_of_ten() {
    let table = filter_table();
    let expected: [f32; 16] = [
        1e-8, 1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7,
    ];
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(table.get(i).copied(), Some(*want), "entry {i}");
    }
}

#[test]
fn filter_table_falls_from_one_to_near_zero() {
    let table = filter_table();
    assert_eq!(table.get(16).copied(), Some(1.0));
    let last = table.get(255).copied().expect("in range");
    assert!((0.0..0.001).contains(&last), "entry 255 was {last}");
    for w in 16..255 {
        let a = table.get(w).copied().unwrap_or(0.0);
        let b = table.get(w + 1).copied().unwrap_or(0.0);
        assert!(a >= b, "table not falling at {w}: {a} then {b}");
    }
}

#[test]
fn filter_index_zero_means_nothing_changes() {
    assert_eq!(filter_step(0) * 1000.0, 1e-5);
}

fn dense_channel_payload(
    first: u16,
    last: u16,
    vector_len: u16,
    kind: u16,
    pivot: u16,
    values: &[f32],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&first.to_le_bytes());
    buf.extend_from_slice(&last.to_le_bytes());
    buf.extend_from_slice(&vector_len.to_le_bytes());
    buf.extend_from_slice(&kind.to_le_bytes());
    buf.extend_from_slice(&pivot.to_le_bytes());
    buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
    for v in values {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

#[test]
fn dense_translation_channel_decodes_one_scalar_per_frame() {
    let payload = dense_channel_payload(10, 12, 1, 0, 3, &[1.0, 2.0, 3.0]);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    assert_eq!(decoded.pivot, 3);
    assert_eq!(decoded.kind, ChannelKind::TranslationX);
    assert!(decoded.vector_len_matches_type());
    match decoded.data {
        ChannelData::Dense {
            first_frame,
            values,
        } => {
            assert_eq!(first_frame, 10);
            assert_eq!(
                values,
                vec![Value::Scalar(1.0), Value::Scalar(2.0), Value::Scalar(3.0)]
            );
        }
        other => panic!("expected Dense, got {other:?}"),
    }
}

#[test]
fn dense_rotation_channel_reads_whole_quaternions() {
    let q = Quat {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };
    let mut floats = Vec::new();
    for v in [q.x, q.y, q.z, q.w] {
        floats.push(v);
    }
    let payload = dense_channel_payload(0, 0, 4, 6, 0, &floats);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    assert_eq!(decoded.kind, ChannelKind::Rotation);
    match decoded.data {
        ChannelData::Dense { values, .. } => {
            assert_eq!(values, vec![Value::Rotation(q)]);
        }
        other => panic!("expected Dense, got {other:?}"),
    }
}

#[test]
fn a_channel_type_the_corpus_never_uses_is_not_a_decode_error() {
    let payload = dense_channel_payload(0, 0, 1, 3, 0, &[7.5]);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    assert_eq!(decoded.kind, ChannelKind::Other(3));
}

#[test]
fn trailing_bytes_past_the_declared_array_are_tolerated() {
    let mut payload = dense_channel_payload(0, 0, 1, 0, 0, &[1.0]);
    payload.extend_from_slice(&[0xAA; 16]);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::Dense { values, .. } => assert_eq!(values, vec![Value::Scalar(1.0)]),
        other => panic!("expected Dense, got {other:?}"),
    }
}

#[test]
fn a_payload_shorter_than_the_declared_array_is_an_error_not_a_panic() {
    let payload = dense_channel_payload(0, 1, 1, 0, 0, &[1.0]);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    assert!(Channel::parse(&top, None).is_err());
}

#[test]
fn last_before_first_is_a_typed_error() {
    let payload = dense_channel_payload(5, 2, 1, 0, 0, &[]);
    let c = chunk(ChunkType::ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    assert!(matches!(
        Channel::parse(&top, None),
        Err(Error::InvalidFrameRange {
            first: 5,
            last: 2,
            ..
        })
    ));
}

#[test]
fn bit_channel_unpacks_least_significant_bit_first() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&9u16.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&4u16.to_le_bytes());
    payload.push(1);
    payload.push(0b0000_0101);
    payload.push(0b0000_0001);

    let c = chunk(ChunkType::BIT_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    assert_eq!(decoded.kind, ChannelKind::Visibility);
    match decoded.data {
        ChannelData::Bits {
            first_frame,
            last_frame,
            default,
            packed,
        } => {
            assert_eq!((first_frame, last_frame), (0, 9));
            assert!(default);
            assert_eq!(packed, vec![0b0000_0101, 0b0000_0001]);
        }
        other => panic!("expected Bits, got {other:?}"),
    }
}

fn compressed_time_coded_payload(
    pivot: u16,
    vector_len: u8,
    kind: u8,
    keys: &[(u32, bool, f32)],
) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&u32::try_from(keys.len()).unwrap().to_le_bytes());
    buf.extend_from_slice(&pivot.to_le_bytes());
    buf.push(vector_len);
    buf.push(kind);
    for &(frame, hold, value) in keys {
        let raw = frame | if hold { 0x8000_0000 } else { 0 };
        buf.extend_from_slice(&raw.to_le_bytes());
        buf.extend_from_slice(&value.to_le_bytes());
    }
    buf
}

#[test]
fn compressed_time_coded_masks_the_hold_flag() {
    let payload = compressed_time_coded_payload(2, 1, 0, &[(0, false, 1.0), (30, true, 2.0)]);
    let c = chunk(ChunkType::COMPRESSED_ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, Some(0))
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::TimeCoded(keys) => {
            assert_eq!(keys.len(), 2);
            assert_eq!(keys[0].frame, 0);
            assert!(!keys[0].hold);
            assert_eq!(keys[1].frame, 30);
            assert!(keys[1].hold);
            assert_eq!(keys[1].value, Value::Scalar(2.0));
        }
        other => panic!("expected TimeCoded, got {other:?}"),
    }
}

fn pack_nibbles(deltas: [i32; 16]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (i, pair) in deltas.chunks(2).enumerate() {
        let lo = u8::try_from(pair[0] & 0x0F).unwrap_or(0);
        let hi = u8::try_from(pair[1] & 0x0F).unwrap_or(0);
        if let Some(slot) = out.get_mut(i) {
            *slot = lo | (hi << 4);
        }
    }
    out
}

#[test]
fn adaptive_delta_reconstructs_a_running_sum_per_component() {
    let deltas = [1, -1, 2, -2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3];
    let mut payload = Vec::new();
    payload.extend_from_slice(&17u32.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.push(1);
    payload.push(0);
    payload.extend_from_slice(&1.0f32.to_le_bytes());
    payload.extend_from_slice(&10.0f32.to_le_bytes());
    payload.push(8);
    payload.extend_from_slice(&pack_nibbles(deltas));

    let c = chunk(ChunkType::COMPRESSED_ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, Some(1))
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::AdaptiveDelta { values } => {
            assert_eq!(values.len(), 17);
            assert_eq!(values[0], Value::Scalar(10.0));
            assert_eq!(values[1], Value::Scalar(11.0));
            assert_eq!(values[2], Value::Scalar(10.0));
            assert_eq!(values[16], Value::Scalar(13.0));
        }
        other => panic!("expected AdaptiveDelta, got {other:?}"),
    }
}

#[test]
fn a_header_lying_about_frame_count_cannot_blow_up_allocation() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u32::MAX.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.push(1);
    payload.push(0);
    payload.extend_from_slice(&1.0f32.to_le_bytes());
    payload.extend_from_slice(&0.0f32.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&[0u8; 8]);

    let c = chunk(ChunkType::COMPRESSED_ANIMATION_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, Some(1))
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::AdaptiveDelta { values } => {
            assert_eq!(values.len(), 17);
        }
        other => panic!("expected AdaptiveDelta, got {other:?}"),
    }
}

#[test]
fn eight_bit_deltas_are_biased_by_128_and_rescaled() {
    let mut payload = vec![0u8, 2, 1, 0];
    payload.extend_from_slice(&17u16.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&16.0f32.to_le_bytes());
    payload.extend_from_slice(&0.0f32.to_le_bytes());
    payload.push(8);
    let mut deltas = [0u8; 16];
    deltas[0] = 129;
    payload.extend_from_slice(&deltas);

    let c = chunk(
        ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
        false,
        &payload,
    );
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::AdaptiveDelta { values } => {
            assert_eq!(values.len(), 17);
            assert_eq!(values[1], Value::Scalar(1.0));
        }
        other => panic!("expected AdaptiveDelta, got {other:?}"),
    }
}

#[test]
fn motion_channel_time_code_flag_is_masked_and_not_part_of_the_frame() {
    let mut payload = vec![0u8, 0, 1, 0];
    payload.extend_from_slice(&2u16.to_le_bytes());
    payload.extend_from_slice(&7u16.to_le_bytes());
    payload.extend_from_slice(&5u16.to_le_bytes());
    payload.extend_from_slice(&(0x8000u16 | 40).to_le_bytes());
    payload.extend_from_slice(&1.0f32.to_le_bytes());
    payload.extend_from_slice(&2.0f32.to_le_bytes());

    let c = chunk(
        ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
        false,
        &payload,
    );
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    assert_eq!(decoded.pivot, 7);
    match decoded.data {
        ChannelData::TimeCoded(keys) => {
            assert_eq!(
                keys[0],
                TimeCodedKey {
                    frame: 5,
                    value: Value::Scalar(1.0),
                    hold: false
                }
            );
            assert_eq!(
                keys[1],
                TimeCodedKey {
                    frame: 40,
                    value: Value::Scalar(2.0),
                    hold: true
                }
            );
        }
        other => panic!("expected TimeCoded, got {other:?}"),
    }
}

#[test]
fn motion_channel_odd_key_count_realigns_with_a_pad() {
    let mut payload = vec![0u8, 0, 1, 0];
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.extend_from_slice(&0u16.to_le_bytes());
    payload.extend_from_slice(&3u16.to_le_bytes());
    payload.extend_from_slice(&[0u8; 2]);
    payload.extend_from_slice(&9.0f32.to_le_bytes());

    let c = chunk(
        ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
        false,
        &payload,
    );
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::TimeCoded(keys) => {
            assert_eq!(keys.len(), 1);
            assert_eq!(keys[0].frame, 3);
            assert_eq!(keys[0].value, Value::Scalar(9.0));
        }
        other => panic!("expected TimeCoded, got {other:?}"),
    }
}

#[test]
fn an_unrecognised_delta_type_is_a_typed_error_not_a_panic() {
    let mut payload = vec![0u8; 8];
    payload[1] = 255;
    let c = chunk(
        ChunkType::COMPRESSED_ANIMATION_MOTION_CHANNEL,
        false,
        &payload,
    );
    let top = one_chunk(&c);
    assert!(matches!(
        Channel::parse(&top, None),
        Err(Error::UnknownChannelEncoding { .. })
    ));
}

#[test]
fn compressed_bit_channel_reads_sparse_toggles() {
    let mut payload = Vec::new();
    payload.extend_from_slice(&2u32.to_le_bytes());
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.push(0);
    payload.push(1);
    payload.extend_from_slice(&(0x8000_0000u32 | 5).to_le_bytes());
    payload.extend_from_slice(&20u32.to_le_bytes());

    let c = chunk(ChunkType::COMPRESSED_BIT_CHANNEL, false, &payload);
    let top = one_chunk(&c);
    let decoded = Channel::parse(&top, None)
        .expect("valid")
        .expect("recognised");
    match decoded.data {
        ChannelData::SparseBits { default, keys } => {
            assert!(default);
            assert_eq!(keys, vec![(5, true), (20, false)]);
        }
        other => panic!("expected SparseBits, got {other:?}"),
    }
}

#[test]
fn every_truncation_of_every_synthetic_channel_errors_but_never_panics() {
    let payloads: Vec<Vec<u8>> = vec![
        dense_channel_payload(0, 3, 1, 0, 0, &[1.0, 2.0, 3.0, 4.0]),
        {
            let mut p = Vec::new();
            p.extend_from_slice(&0u16.to_le_bytes());
            p.extend_from_slice(&15u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            p.push(1);
            p.extend_from_slice(&[0xFF; 2]);
            p
        },
        compressed_time_coded_payload(0, 1, 0, &[(0, false, 1.0), (5, true, 2.0)]),
    ];
    let kinds = [
        ChunkType::ANIMATION_CHANNEL,
        ChunkType::BIT_CHANNEL,
        ChunkType::COMPRESSED_ANIMATION_CHANNEL,
    ];
    for (kind, payload) in kinds.into_iter().zip(payloads.iter()) {
        for cut in 0..=payload.len() {
            let Some(prefix) = payload.get(..cut) else {
                continue;
            };
            let c = chunk(kind, false, prefix);
            if let Some(Ok(top)) = Chunks::new(&c).next() {
                let _ = Channel::parse(&top, Some(0));
            }
        }
    }
}
