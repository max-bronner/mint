#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use crate::bool_decoder::BoolDecoder;
use crate::idct;
use crate::tables::*;
use crate::{Decoder, Error, Picture};

struct BoolEncoder {
    out: Vec<u8>,
    range: u32,
    bottom: u32,
    bit_count: i32,
}

impl BoolEncoder {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            range: 255,
            bottom: 0,
            bit_count: 24,
        }
    }

    fn carry(&mut self) {
        for b in self.out.iter_mut().rev() {
            if *b == 255 {
                *b = 0;
            } else {
                *b += 1;
                return;
            }
        }
    }

    fn put(&mut self, bit: bool, prob: u8) {
        let split = 1 + (((self.range - 1) * u32::from(prob)) >> 8);
        if bit {
            self.bottom = self.bottom.wrapping_add(split);
            self.range -= split;
        } else {
            self.range = split;
        }
        while self.range < 128 {
            self.range <<= 1;
            if self.bottom & (1 << 31) != 0 {
                self.carry();
            }
            self.bottom <<= 1;
            self.bit_count -= 1;
            if self.bit_count == 0 {
                self.out.push((self.bottom >> 24) as u8);
                self.bottom &= (1 << 24) - 1;
                self.bit_count = 8;
            }
        }
    }

    fn bits(&mut self, value: u32, n: u32) {
        for i in (0..n).rev() {
            self.put((value >> i) & 1 != 0, 128);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        let mut c = self.bit_count;
        let mut v = self.bottom;
        if v & (1 << (32 - c)) != 0 {
            self.carry();
        }
        v <<= c & 7;
        c >>= 3;
        while c > 0 {
            v <<= 8;
            c -= 1;
        }
        for _ in 0..4 {
            self.out.push((v >> 24) as u8);
            v <<= 8;
        }
        self.out
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_bool_decoder_reads_back_what_the_reference_encoder_wrote() {
    let mut rng = Rng(0x5eed_0001);
    let mut symbols: Vec<(bool, u8)> = vec![
        (true, 1),
        (false, 1),
        (true, 255),
        (false, 255),
        (true, 128),
    ];
    for _ in 0..5000 {
        let r = rng.next();
        symbols.push(((r >> 8) & 1 != 0, (r & 0xFF).max(1) as u8));
    }
    let mut enc = BoolEncoder::new();
    for &(bit, prob) in &symbols {
        enc.put(bit, prob);
    }
    let bytes = enc.finish();
    let mut dec = BoolDecoder::new(&bytes);
    let read: Vec<(bool, u8)> = symbols
        .iter()
        .map(|&(_, p)| (dec.read_prob(p), p))
        .collect();
    assert_eq!(read, symbols);
}

const COS16: [f64; 9] = [
    1.0,
    0.980_785_280_403_230_4,
    0.923_879_532_511_286_7,
    0.831_469_612_302_545_2,
    std::f64::consts::FRAC_1_SQRT_2,
    0.555_570_233_019_602_2,
    0.382_683_432_365_089_8,
    0.195_090_322_016_128_3,
    0.0,
];

fn cos16(m: usize) -> f64 {
    match m % 32 {
        m @ 0..=8 => COS16[m],
        m @ 9..=16 => -COS16[16 - m],
        m @ 17..=24 => -COS16[m - 16],
        m => COS16[32 - m],
    }
}

fn reference_idct(coeffs: &[i16; 64]) -> [f64; 64] {
    let c = |k: usize| if k == 0 { COS16[4] } else { 1.0 };
    let mut out = [0.0; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                for u in 0..8 {
                    s += c(u)
                        * c(v)
                        * f64::from(coeffs[v * 8 + u])
                        * cos16((2 * x + 1) * u)
                        * cos16((2 * y + 1) * v);
                }
            }
            out[y * 8 + x] = s / 16.0;
        }
    }
    out
}

#[test]
fn the_integer_idct_matches_the_transform_formula_within_one() {
    let mut rng = Rng(0x1dc7);
    for case in 0..200 {
        let mut coeffs = [0i16; 64];
        for _ in 0..(1 + case % 6) {
            let r = rng.next();
            coeffs[(r % 64) as usize] = ((r >> 8) % 2001) as i16 - 1000;
        }
        let expected = reference_idct(&coeffs);
        let mut got = coeffs;
        idct::idct(&mut got);
        for (i, (&g, &e)) in got.iter().zip(&expected).enumerate() {
            assert!(
                (f64::from(g) - e).abs() <= 1.5,
                "case {case} sample {i}: {g} vs {e:.2} for {coeffs:?}"
            );
        }
    }
}

#[test]
fn a_dc_only_block_transforms_to_a_flat_block_of_its_dc_value() {
    for dc in [-2000i16, -301, -1, 0, 1, 77, 100, 2047] {
        let mut block = [0i16; 64];
        block[0] = dc;
        idct::idct(&mut block);
        assert!(
            block.iter().all(|&v| v == idct::idct_dc(dc)),
            "dc {dc}: {block:?}"
        );
    }
}

fn put_dc_then_eob(enc: &mut BoolEncoder, value: i16, ctx: usize) {
    let dc_probs: [u8; 5] = std::array::from_fn(|k| {
        let w = VP6_DC_WEIGHTS[k][ctx];
        (((128 * i32::from(w[0]) + 128) >> 8) + i32::from(w[1])).clamp(1, 255) as u8
    });
    let mag = value.unsigned_abs();
    if mag == 0 {
        enc.put(false, dc_probs[0]);
    } else {
        enc.put(true, dc_probs[0]);
        match mag {
            1 => enc.put(false, dc_probs[2]),
            2..=4 => {
                enc.put(true, dc_probs[2]);
                enc.put(false, dc_probs[3]);
                if mag == 2 {
                    enc.put(false, dc_probs[4]);
                } else {
                    enc.put(true, dc_probs[4]);
                    enc.put(mag == 4, 128);
                }
            }
            _ => {
                enc.put(true, dc_probs[2]);
                enc.put(true, dc_probs[3]);
                let cat = VP56_COEF_BASE
                    .iter()
                    .rposition(|&b| b as u16 <= mag)
                    .unwrap();
                match cat {
                    0 | 1 => {
                        enc.put(false, 128);
                        enc.put(cat == 1, 128);
                    }
                    _ => {
                        enc.put(true, 128);
                        enc.put(cat >= 4, 128);
                        enc.put(cat % 2 == 1, 128);
                    }
                }
                let add = mag - VP56_COEF_BASE[cat] as u16;
                let probs: Vec<u8> = VP56_COEF_ADD_PROBS[cat]
                    .iter()
                    .copied()
                    .take_while(|&p| p != 128)
                    .collect();
                for (i, &p) in probs.iter().enumerate() {
                    enc.put((add >> (probs.len() - 1 - i)) & 1 != 0, p);
                }
            }
        }
        enc.put(value < 0, 128);
    }
    enc.put(false, 128);
    enc.put(false, 128);
}

fn predicted(dcs: &[i16], cols: usize, i: usize, first: i16) -> i16 {
    let (col, row) = (i % cols, i / cols);
    match (col > 0, row > 0) {
        (true, true) => (dcs[i - 1] + dcs[i - cols]) / 2,
        (true, false) => dcs[i - 1],
        (false, true) => dcs[i - cols],
        (false, false) => first,
    }
}

const MB_W: usize = 2;
const MB_H: usize = 2;
const QUANT: u32 = 0;

fn target_dcs() -> [Vec<i16>; 3] {
    [
        vec![3, -8, 12, 5, 7, 20, -15, 1, -3, 9, 14, -20, 0, 11, -6, 2],
        vec![150, 133, 141, 120],
        vec![100, 117, 90, 131],
    ]
}

fn dc_keyframe() -> Vec<u8> {
    let dcs = target_dcs();
    let mut enc = BoolEncoder::new();
    enc.bits(MB_H as u32, 8);
    enc.bits(MB_W as u32, 8);
    enc.bits(MB_H as u32, 8);
    enc.bits(MB_W as u32, 8);
    enc.bits(0, 2);
    enc.put(false, 128);
    enc.put(false, 128);
    enc.put(false, 128);
    for plane in &HAS_COEF_PROBS {
        for &p in plane {
            enc.put(false, p);
        }
    }
    enc.put(false, 128);
    for comp in &HAS_ZERO_RUN_PROBS {
        for &p in comp {
            enc.put(false, p);
        }
    }
    for p in VP6_AC_PROBS.iter().flatten().flatten().flatten() {
        enc.put(false, *p);
    }

    let raw: [Vec<i16>; 3] = std::array::from_fn(|plane| {
        let cols = if plane == 0 { MB_W * 2 } else { MB_W };
        let first = if plane == 0 { 0 } else { 128 };
        (0..dcs[plane].len())
            .map(|i| dcs[plane][i] - predicted(&dcs[plane], cols, i, first))
            .collect()
    });
    let nz = |plane: usize, i: usize| -> usize { usize::from(raw[plane][i] != 0) };
    for mb_y in 0..MB_H {
        for mb_x in 0..MB_W {
            for blk in 0..6 {
                let (plane, cols, col, row) = if blk < 4 {
                    (0, MB_W * 2, mb_x * 2 + (blk & 1), mb_y * 2 + (blk >> 1))
                } else {
                    (blk - 3, MB_W, mb_x, mb_y)
                };
                let i = row * cols + col;
                let left = if col > 0 { nz(plane, i - 1) } else { 0 };
                let up = if row > 0 { nz(plane, i - cols) } else { 0 };
                put_dc_then_eob(&mut enc, raw[plane][i], left + up);
            }
        }
    }
    let mut frame = vec![(QUANT << 1) as u8, (6 << 3) | (3 << 1)];
    frame.extend(enc.finish());
    frame
}

fn expected_sample(dc: i16) -> f64 {
    let dc_quant = f64::from(VP56_DC_QUANTS[QUANT as usize] * 4);
    (128.0 + f64::from(dc) * dc_quant / 32.0).clamp(0.0, 255.0)
}

fn block_value(pic: &Picture, plane: usize, col: usize, row: usize) -> Vec<u8> {
    let p = [pic.y(), pic.u(), pic.v()][plane];
    (0..8)
        .flat_map(|y| p.row(row * 8 + y).unwrap()[col * 8..col * 8 + 8].to_vec())
        .collect()
}

#[test]
fn a_dc_only_key_frame_reconstructs_each_block_from_its_predicted_dc() {
    let frame = dc_keyframe();
    let mut dec = Decoder::new();
    let pic = dec.decode(&frame).unwrap();
    assert_eq!((pic.width(), pic.height()), (MB_W * 16, MB_H * 16));
    for (plane, dcs) in target_dcs().iter().enumerate() {
        let cols = if plane == 0 { MB_W * 2 } else { MB_W };
        for (i, &dc) in dcs.iter().enumerate() {
            let block = block_value(pic, plane, i % cols, i / cols);
            let want = expected_sample(dc);
            assert!(
                block.iter().all(|&v| (f64::from(v) - want).abs() <= 1.0),
                "plane {plane} block {i}: want {want:.1}, got {block:?}"
            );
        }
    }
}

fn skip_frame() -> Vec<u8> {
    let mut enc = BoolEncoder::new();
    enc.put(false, 128);
    enc.put(false, 128);
    enc.put(false, 128);
    for _ in 0..3 {
        enc.put(false, 174);
        enc.put(false, 254);
    }
    for comp in 0..2 {
        enc.put(false, HAS_NZ_PROB[comp]);
        enc.put(false, HAS_SIGN_PROB[comp]);
    }
    for p in HAS_TREE_PROB
        .iter()
        .flatten()
        .chain(HAS_RAW_PROB.iter().flatten())
    {
        enc.put(false, *p);
    }
    for p in HAS_COEF_PROBS.iter().flatten() {
        enc.put(false, *p);
    }
    enc.put(false, 128);
    for p in HAS_ZERO_RUN_PROBS.iter().flatten() {
        enc.put(false, *p);
    }
    for p in VP6_AC_PROBS.iter().flatten().flatten().flatten() {
        enc.put(false, *p);
    }
    for _ in 0..MB_W * MB_H {
        enc.put(true, 10);
        for _ in 0..6 {
            put_dc_then_eob(&mut enc, 0, 0);
        }
    }
    let mut frame = vec![0x80 | (QUANT << 1) as u8];
    frame.extend(enc.finish());
    frame
}

#[test]
fn an_inter_frame_of_skipped_macroblocks_repeats_the_previous_picture() {
    let mut dec = Decoder::new();
    let key = dec.decode(&dc_keyframe()).unwrap().clone();
    let inter = dec.decode(&skip_frame()).unwrap();
    assert_eq!(*inter, key);
}

#[test]
fn an_inter_frame_before_any_key_frame_is_an_error() {
    assert_eq!(
        Decoder::new().decode(&skip_frame()).err(),
        Some(Error::MissingReference)
    );
}

#[test]
fn truncated_frames_never_panic() {
    let key = dc_keyframe();
    let inter = skip_frame();
    for len in 0..key.len() {
        let _ = Decoder::new().decode(&key[..len]);
    }
    let mut dec = Decoder::new();
    for len in 0..inter.len() {
        dec.decode(&key).unwrap();
        let _ = dec.decode(&inter[..len]);
    }
}

#[test]
fn corrupt_and_random_frames_never_panic() {
    let key = dc_keyframe();
    let mut rng = Rng(0xbad_5eed);
    let mut dec = Decoder::new();
    for round in 0..3000 {
        let mut frame = key.clone();
        match round % 3 {
            0 => {
                let r = rng.next();
                let i = (r % frame.len() as u64) as usize;
                frame[i] ^= 1 << ((r >> 32) % 8);
            }
            1 => {
                let len = 2 + (rng.next() % 600) as usize;
                frame = (0..len).map(|_| rng.next() as u8).collect();
                frame[0] &= 0x7F;
            }
            _ => {
                let len = 1 + (rng.next() % 600) as usize;
                frame = (0..len).map(|_| rng.next() as u8).collect();
                frame[0] |= 0x80;
                let _ = dec.decode(&key);
            }
        }
        let _ = dec.decode(&frame);
    }
}
