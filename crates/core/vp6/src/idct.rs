use crate::Plane;

const C1S7: i32 = 64277;
const C2S6: i32 = 60547;
const C3S5: i32 = 54491;
const C4S4: i32 = 46341;
const C5S3: i32 = 36410;
const C6S2: i32 = 25080;
const C7S1: i32 = 12785;

fn mul16(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b).wrapping_shr(16)
}

fn idct8(s: [i32; 8], bias: i32, shift: u32) -> [i32; 8] {
    let t_a = mul16(C1S7, s[1]).wrapping_add(mul16(C7S1, s[7]));
    let t_b = mul16(C7S1, s[1]).wrapping_sub(mul16(C1S7, s[7]));
    let t_c = mul16(C3S5, s[3]).wrapping_add(mul16(C5S3, s[5]));
    let t_d = mul16(C3S5, s[5]).wrapping_sub(mul16(C5S3, s[3]));
    let t_a1 = mul16(C4S4, t_a.wrapping_sub(t_c));
    let t_b1 = mul16(C4S4, t_b.wrapping_sub(t_d));
    let t_c = t_a.wrapping_add(t_c);
    let t_d = t_b.wrapping_add(t_d);
    let t_e = mul16(C4S4, s[0].wrapping_add(s[4])).wrapping_add(bias);
    let t_f = mul16(C4S4, s[0].wrapping_sub(s[4])).wrapping_add(bias);
    let t_g = mul16(C2S6, s[2]).wrapping_add(mul16(C6S2, s[6]));
    let t_h = mul16(C6S2, s[2]).wrapping_sub(mul16(C2S6, s[6]));
    let t_e1 = t_e.wrapping_sub(t_g);
    let t_g = t_e.wrapping_add(t_g);
    let t_a = t_f.wrapping_add(t_a1);
    let t_f = t_f.wrapping_sub(t_a1);
    let t_b = t_b1.wrapping_sub(t_h);
    let t_h = t_b1.wrapping_add(t_h);
    [
        t_g.wrapping_add(t_c),
        t_a.wrapping_add(t_h),
        t_a.wrapping_sub(t_h),
        t_e1.wrapping_add(t_d),
        t_e1.wrapping_sub(t_d),
        t_f.wrapping_add(t_b),
        t_f.wrapping_sub(t_b),
        t_g.wrapping_sub(t_c),
    ]
    .map(|v| v.wrapping_shr(shift))
}

pub(crate) fn idct(coeffs: &mut [i16; 64]) {
    let mut tmp = [[0i32; 8]; 8];
    for (row, out) in coeffs.chunks_exact(8).zip(tmp.iter_mut()) {
        let mut s = [0i32; 8];
        for (d, &c) in s.iter_mut().zip(row) {
            *d = i32::from(c);
        }
        *out = idct8(s, 0, 0);
    }
    for col in 0..8 {
        let s = tmp.map(|row| row.get(col).copied().unwrap_or(0));
        for (row, v) in idct8(s, 8, 4).into_iter().enumerate() {
            if let Some(c) = coeffs.get_mut(row.wrapping_mul(8).wrapping_add(col)) {
                *c = truncate(v);
            }
        }
    }
}

pub(crate) fn idct_dc(dc: i16) -> i16 {
    truncate(
        mul16(C4S4, mul16(C4S4, i32::from(dc)))
            .wrapping_add(8)
            .wrapping_shr(4),
    )
}

#[allow(clippy::cast_possible_truncation)]
fn truncate(v: i32) -> i16 {
    v as i16
}

fn clamp_u8(v: i32) -> u8 {
    u8::try_from(v.clamp(0, 255)).unwrap_or(0)
}

#[derive(Clone, Copy)]
pub(crate) enum Rows {
    Frame,
    Field,
}

fn rows_mut(
    plane: &mut Plane,
    bx: usize,
    by: usize,
    rows: Rows,
) -> impl Iterator<Item = &mut [u8]> {
    let stride = plane.width;
    let (first, step) = match rows {
        Rows::Frame => (by.wrapping_mul(8), 1usize),
        Rows::Field => ((by & !1).wrapping_mul(8).wrapping_add(by & 1), 2),
    };
    let x = bx.wrapping_mul(8);
    plane
        .data
        .chunks_exact_mut(stride.max(1))
        .skip(first)
        .step_by(step)
        .take(8)
        .filter_map(move |row| row.get_mut(x..x.wrapping_add(8)))
}

pub(crate) fn put_block(
    coeffs: &mut [i16; 64],
    dc_only: bool,
    plane: &mut Plane,
    bx: usize,
    by: usize,
    rows: Rows,
) {
    if dc_only {
        let px = clamp_u8(i32::from(idct_dc(coeffs[0])).wrapping_add(128));
        for row in rows_mut(plane, bx, by, rows) {
            row.fill(px);
        }
    } else {
        idct(coeffs);
        for (row, src) in rows_mut(plane, bx, by, rows).zip(coeffs.chunks_exact(8)) {
            for (d, &s) in row.iter_mut().zip(src) {
                *d = clamp_u8(i32::from(s).wrapping_add(128));
            }
        }
    }
}

pub(crate) fn add_block(
    coeffs: &mut [i16; 64],
    dc_only: bool,
    plane: &mut Plane,
    bx: usize,
    by: usize,
    rows: Rows,
) {
    if dc_only {
        let dc = i32::from(idct_dc(coeffs[0]));
        for row in rows_mut(plane, bx, by, rows) {
            for d in row.iter_mut() {
                *d = clamp_u8(i32::from(*d).wrapping_add(dc));
            }
        }
    } else {
        idct(coeffs);
        for (row, src) in rows_mut(plane, bx, by, rows).zip(coeffs.chunks_exact(8)) {
            for (d, &s) in row.iter_mut().zip(src) {
                *d = clamp_u8(i32::from(*d).wrapping_add(i32::from(s)));
            }
        }
    }
}
