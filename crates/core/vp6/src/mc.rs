use crate::tables::VP6_BICUBIC_COEFFS;
use crate::Plane;

const TMP_STRIDE: usize = 16;
const TMP_LEN: usize = TMP_STRIDE * 12;
const ORIGIN: usize = 2 * TMP_STRIDE + 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Mv {
    pub(crate) x: i16,
    pub(crate) y: i16,
}

impl Mv {
    pub(crate) fn add(self, o: Self) -> Self {
        Self {
            x: self.x.wrapping_add(o.x),
            y: self.y.wrapping_add(o.y),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Filter {
    pub(crate) advanced: bool,
    pub(crate) autosel: bool,
    pub(crate) var_thresh: u16,
    pub(crate) mv_thresh: u8,
    pub(crate) bicubic: bool,
    pub(crate) filter_alpha: usize,
    pub(crate) loop_thr: i16,
    pub(crate) deblock: bool,
}

fn px(buf: &[u8; TMP_LEN], i: usize) -> u8 {
    buf.get(i).copied().unwrap_or(0)
}

fn get_block(tmp: &mut [u8; TMP_LEN], src: &Plane, x: isize, y: isize) {
    let w = isize::try_from(src.width).unwrap_or(0);
    let h = isize::try_from(src.height).unwrap_or(0);
    if w == 0 || h == 0 {
        tmp.fill(0);
        return;
    }
    for (row, line) in tmp.chunks_exact_mut(TMP_STRIDE).enumerate() {
        let sy = y.wrapping_add_unsigned(row).clamp(0, h.wrapping_sub(1));
        let base = usize::try_from(sy).unwrap_or(0).wrapping_mul(src.width);
        let Some(srow) = src.data.get(base..base.wrapping_add(src.width)) else {
            continue;
        };
        if x >= 0 && x.wrapping_add(12) <= w {
            let sx = usize::try_from(x).unwrap_or(0);
            if let (Some(d), Some(s)) = (line.get_mut(..12), srow.get(sx..sx.wrapping_add(12))) {
                d.copy_from_slice(s);
            }
        } else {
            for (col, d) in line.iter_mut().take(12).enumerate() {
                let sx = x.wrapping_add_unsigned(col).clamp(0, w.wrapping_sub(1));
                *d = srow
                    .get(usize::try_from(sx).unwrap_or(0))
                    .copied()
                    .unwrap_or(0);
            }
        }
    }
}

fn loop_filter(buf: &mut [u8; TMP_LEN], mut off: usize, step: usize, stride: usize, loop_str: i16) {
    let s = i32::from(loop_str);
    for _ in 0..12 {
        let at = |o: Option<usize>| o.and_then(|o| buf.get(o)).map_or(0, |&v| i32::from(v));
        let a = at(off.checked_sub(step.wrapping_mul(2)));
        let b = at(off.checked_sub(step));
        let c = at(Some(off));
        let d = at(off.checked_add(step));
        let mut diff = a
            .wrapping_sub(d)
            .wrapping_add(c.wrapping_sub(b).wrapping_mul(3))
            .wrapping_add(4)
            .wrapping_shr(3);
        if diff.wrapping_abs() >= s.wrapping_mul(2) {
            diff = 0;
        } else if diff.wrapping_abs() >= s {
            diff = if diff < 0 {
                diff.wrapping_neg().wrapping_sub(s.wrapping_mul(2))
            } else {
                diff.wrapping_neg().wrapping_add(s.wrapping_mul(2))
            };
        }
        if diff != 0 {
            if let Some(p) = off.checked_sub(step).and_then(|o| buf.get_mut(o)) {
                *p = clamp_u8(b.wrapping_add(diff));
            }
            if let Some(p) = buf.get_mut(off) {
                *p = clamp_u8(c.wrapping_sub(diff));
            }
        }
        off = off.wrapping_add(stride);
    }
}

fn clamp_u8(v: i32) -> u8 {
    u8::try_from(v.clamp(0, 255)).unwrap_or(0)
}

fn variance(buf: &[u8; TMP_LEN], off: usize) -> u16 {
    let mut sum = 0u32;
    let mut ssum = 0u32;
    for row in 0..4usize {
        for col in 0..4usize {
            let p = u32::from(px(
                buf,
                at(off, row.wrapping_mul(2), col.wrapping_mul(2), TMP_STRIDE),
            ));
            sum = sum.wrapping_add(p);
            ssum = ssum.wrapping_add(p.wrapping_mul(p));
        }
    }
    u16::try_from(
        ssum.wrapping_mul(16)
            .wrapping_sub(sum.wrapping_mul(sum))
            .wrapping_shr(8),
    )
    .unwrap_or(u16::MAX)
}

fn bilinear(a: u8, b: u8, frac: u16) -> u8 {
    let v = u16::from(a)
        .wrapping_mul(8u16.wrapping_sub(frac))
        .wrapping_add(u16::from(b).wrapping_mul(frac))
        .wrapping_add(4)
        .wrapping_shr(3);
    u8::try_from(v).unwrap_or(u8::MAX)
}

fn bicubic(src: &[u8], off: usize, step: usize, k: &[i16; 4]) -> u8 {
    let at = |o: Option<usize>| o.and_then(|o| src.get(o)).map_or(0, |&v| i32::from(v));
    let v = at(off.checked_sub(step))
        .wrapping_mul(i32::from(k[0]))
        .wrapping_add(at(Some(off)).wrapping_mul(i32::from(k[1])))
        .wrapping_add(at(off.checked_add(step)).wrapping_mul(i32::from(k[2])))
        .wrapping_add(
            at(step.checked_mul(2).and_then(|s| off.checked_add(s))).wrapping_mul(i32::from(k[3])),
        )
        .wrapping_add(64)
        .wrapping_shr(7);
    clamp_u8(v)
}

fn dst_rows(dst: &mut Plane, x: usize, y: usize) -> impl Iterator<Item = &mut [u8]> {
    let stride = dst.width.max(1);
    dst.data
        .chunks_exact_mut(stride)
        .skip(y)
        .take(8)
        .filter_map(move |row| row.get_mut(x..x.wrapping_add(8)))
}

pub(crate) fn mc_block(
    dst: &mut Plane,
    src: &Plane,
    x: usize,
    y: usize,
    mv: Mv,
    luma: bool,
    f: &Filter,
) {
    let (sx, sy, mx, my, msx, msy) = if luma {
        (
            mv.x >> 2,
            mv.y >> 2,
            (mv.x & 3) << 1,
            (mv.y & 3) << 1,
            mv.x / 4,
            mv.y / 4,
        )
    } else {
        (mv.x >> 3, mv.y >> 3, mv.x & 7, mv.y & 7, mv.x / 8, mv.y / 8)
    };
    let mut tmp = [0u8; TMP_LEN];
    let bx = isize::try_from(x)
        .unwrap_or(0)
        .wrapping_add(isize::from(sx))
        .wrapping_sub(2);
    let by = isize::try_from(y)
        .unwrap_or(0)
        .wrapping_add(isize::from(sy))
        .wrapping_sub(2);
    get_block(&mut tmp, src, bx, by);
    if f.deblock && msx & 7 != 0 {
        let foff = 8usize.wrapping_sub(usize::from((sx & 7).unsigned_abs()));
        loop_filter(&mut tmp, foff.wrapping_add(2), 1, TMP_STRIDE, f.loop_thr);
    }
    if f.deblock && msy & 7 != 0 {
        let foff = 8usize.wrapping_sub(usize::from((sy & 7).unsigned_abs()));
        loop_filter(
            &mut tmp,
            foff.wrapping_add(2).wrapping_mul(TMP_STRIDE),
            TMP_STRIDE,
            1,
            f.loop_thr,
        );
    }

    let copy = mx == 0 && my == 0;
    let mut use_bicubic = !copy && luma && f.bicubic;
    if luma && !copy && f.advanced {
        if !f.autosel {
            use_bicubic = true;
        } else {
            let limit = 1i16.wrapping_shl(u32::from(f.mv_thresh).wrapping_add(1));
            if mv.x.wrapping_abs() <= limit && mv.y.wrapping_abs() <= limit {
                let mut var_off = ORIGIN;
                if mv.x < 0 {
                    var_off = var_off.wrapping_add(1);
                }
                if mv.y < 0 {
                    var_off = var_off.wrapping_add(TMP_STRIDE);
                }
                if variance(&tmp, var_off) >= f.var_thresh {
                    use_bicubic = true;
                }
            }
        }
    }

    if copy {
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            let s = ORIGIN.wrapping_add(r.wrapping_mul(TMP_STRIDE));
            if let Some(src) = tmp.get(s..s.wrapping_add(8)) {
                row.copy_from_slice(src);
            }
        }
    } else if use_bicubic {
        let table = VP6_BICUBIC_COEFFS
            .get(f.filter_alpha)
            .unwrap_or(&VP6_BICUBIC_COEFFS[16]);
        let kh = table
            .get(usize::from(mx.unsigned_abs()))
            .unwrap_or(&table[0]);
        let kv = table
            .get(usize::from(my.unsigned_abs()))
            .unwrap_or(&table[0]);
        mc_bicubic(dst, x, y, &tmp, kh, kv);
    } else {
        mc_bilinear(dst, x, y, &tmp, mx.unsigned_abs(), my.unsigned_abs());
    }
}

fn at(base: usize, row: usize, col: usize, stride: usize) -> usize {
    base.wrapping_add(row.wrapping_mul(stride))
        .wrapping_add(col)
}

fn mc_bilinear(dst: &mut Plane, x: usize, y: usize, tmp: &[u8; TMP_LEN], mx: u16, my: u16) {
    let h = |row: usize| -> [u8; 8] {
        std::array::from_fn(|i| {
            let o = at(ORIGIN, row, i, TMP_STRIDE);
            bilinear(px(tmp, o), px(tmp, o.wrapping_add(1)), mx)
        })
    };
    if my == 0 {
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            row.copy_from_slice(&h(r));
        }
    } else if mx == 0 {
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            for (i, d) in row.iter_mut().enumerate() {
                let o = at(ORIGIN, r, i, TMP_STRIDE);
                *d = bilinear(px(tmp, o), px(tmp, o.wrapping_add(TMP_STRIDE)), my);
            }
        }
    } else {
        let mut prev = h(0);
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            let cur = h(r.wrapping_add(1));
            for ((d, &a), &b) in row.iter_mut().zip(&prev).zip(&cur) {
                *d = bilinear(a, b, my);
            }
            prev = cur;
        }
    }
}

fn mc_bicubic(
    dst: &mut Plane,
    x: usize,
    y: usize,
    tmp: &[u8; TMP_LEN],
    kh: &[i16; 4],
    kv: &[i16; 4],
) {
    if kv[1] == 128 {
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            for (i, d) in row.iter_mut().enumerate() {
                *d = bicubic(tmp, at(ORIGIN, r, i, TMP_STRIDE), 1, kh);
            }
        }
    } else if kh[1] == 128 {
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            for (i, d) in row.iter_mut().enumerate() {
                *d = bicubic(tmp, at(ORIGIN, r, i, TMP_STRIDE), TMP_STRIDE, kv);
            }
        }
    } else {
        let mut buf = [0u8; 16 * 11];
        for (r, line) in buf.chunks_exact_mut(16).enumerate() {
            for (i, d) in line.iter_mut().take(8).enumerate() {
                *d = bicubic(tmp, at(ORIGIN - TMP_STRIDE, r, i, TMP_STRIDE), 1, kh);
            }
        }
        for (r, row) in dst_rows(dst, x, y).enumerate() {
            for (i, d) in row.iter_mut().enumerate() {
                *d = bicubic(&buf, at(16, r, i, 16), 16, kv);
            }
        }
    }
}
