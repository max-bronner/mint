#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = tag.to_vec();
    v.extend_from_slice(&u32::try_from(payload.len() + 8).unwrap().to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn header(frames: u32) -> Vec<u8> {
    let mut p = b"vp60".to_vec();
    p.extend_from_slice(&64u16.to_le_bytes());
    p.extend_from_slice(&48u16.to_le_bytes());
    for v in [frames, 100, 30000, 1001] {
        p.extend_from_slice(&v.to_le_bytes());
    }
    p
}

fn sample(alpha: bool) -> Vec<u8> {
    let mut f = Vec::new();
    if alpha {
        f.extend(chunk(b"AVP6", &[]));
    }
    f.extend(chunk(b"MVhd", &header(2)));
    if alpha {
        f.extend(chunk(b"AVhd", &header(2)));
        f.extend(chunk(b"AV0K", &[9, 9]));
    }
    f.extend(chunk(b"MV0K", &[1, 2, 3]));
    if alpha {
        f.extend(chunk(b"AV0F", &[8]));
    }
    f.extend(chunk(b"MV0F", &[4, 5]));
    f
}

fn walk(data: &[u8]) -> Result<Vec<(ChunkKind, Vec<u8>)>> {
    Container::parse(data)?
        .chunks()
        .map(|c| c.map(|c| (c.kind, c.data.to_vec())))
        .collect()
}

#[test]
fn colour_file_yields_headers_and_frames_in_order() {
    let data = sample(false);
    let c = Container::parse(&data).unwrap();
    assert_eq!((c.header.width, c.header.height), (64, 48));
    assert_eq!(c.header.frame_count, 2);
    assert_eq!(c.header.rate_numerator, 30000);
    assert!(c.alpha_header.is_none());
    assert_eq!(
        walk(&data).unwrap(),
        vec![
            (ChunkKind::ColourKey, vec![1, 2, 3]),
            (ChunkKind::ColourInter, vec![4, 5])
        ]
    );
}

#[test]
fn alpha_file_yields_alpha_chunks_before_their_colour_chunk() {
    let data = sample(true);
    assert!(Container::parse(&data).unwrap().alpha_header.is_some());
    let kinds: Vec<_> = walk(&data).unwrap().into_iter().map(|c| c.0).collect();
    assert_eq!(
        kinds,
        [
            ChunkKind::AlphaKey,
            ChunkKind::ColourKey,
            ChunkKind::AlphaInter,
            ChunkKind::ColourInter
        ]
    );
}

#[test]
fn a_header_payload_that_is_not_24_bytes_is_rejected() {
    for len in [20, 23, 25, 28] {
        let mut p = header(1);
        p.resize(len, 0);
        let data = chunk(b"MVhd", &p);
        assert!(
            matches!(Container::parse(&data), Err(Error::BadHeaderSize { payload, .. }) if payload == len),
            "len {len}"
        );
    }
}

#[test]
fn a_chunk_size_below_eight_is_rejected() {
    let mut data = sample(false);
    data.extend(b"MV0F");
    data.extend(4u32.to_le_bytes());
    assert!(matches!(
        walk(&data),
        Err(Error::ChunkTooSmall { size: 4, .. })
    ));
}

#[test]
fn a_huge_chunk_size_errors_without_allocating() {
    let mut data = sample(false);
    data.extend(b"MV0F");
    data.extend(u32::MAX.to_le_bytes());
    assert!(matches!(walk(&data), Err(Error::ChunkOverrun { .. })));
}

#[test]
fn a_repeated_header_tag_among_frames_is_rejected() {
    let mut data = sample(false);
    data.extend(chunk(b"MVhd", &header(1)));
    assert!(matches!(walk(&data), Err(Error::UnexpectedTag { .. })));
}

#[test]
fn trailing_bytes_short_of_a_chunk_header_are_an_error() {
    let mut data = sample(false);
    data.extend([0, 0, 0]);
    assert!(matches!(walk(&data), Err(Error::Truncated(_))));
}

#[test]
fn every_truncation_of_a_valid_file_errors_or_ends_on_a_chunk_boundary() {
    for alpha in [false, true] {
        let data = sample(alpha);
        let full = walk(&data).unwrap().len();
        for n in 0..data.len() {
            if let Ok(chunks) = walk(&data[..n]) {
                assert!(chunks.len() < full, "prefix {n} walked as a whole file");
            }
        }
    }
}

#[test]
fn mutated_header_bytes_never_panic() {
    let data = sample(true);
    for i in 0..data.len().min(64) {
        for v in [0u8, 1, 0x7f, 0x80, 0xff] {
            let mut d = data.clone();
            d[i] = v;
            let _ = walk(&d);
        }
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut s = 0x1234_5678_9abc_def0_u64;
    let mut next = || {
        s = s
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        s >> 33
    };
    for _ in 0..2000 {
        let len = usize::try_from(next() % 96).unwrap();
        let d: Vec<u8> = (0..len)
            .map(|_| u8::try_from(next() & 0xff).unwrap())
            .collect();
        let _ = walk(&d);
    }
}
