use crate::bits::BitReader;
use crate::bool_decoder::{BoolDecoder, MAX_OVERRUN};
use crate::header::{self, Header, StreamState, PROFILE_ADVANCED};
use crate::idct::{self, Rows};
use crate::mc::{self, Filter, Mv};
use crate::models::{tree, CoeffModel, Models, MvModel};
use crate::tables::*;
use crate::{Error, Picture, Result};

const TOKEN_LARGE: u8 = 5;
const TOKEN_EOB: u8 = 42;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RefFrame {
    #[default]
    Intra = 0,
    Previous = 1,
    Golden = 2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum MbType {
    #[default]
    Intra,
    InterNoMv,
    InterMv,
    InterNearest,
    InterNear,
    InterFourMv,
    GoldenNoMv,
    GoldenMv,
    GoldenNearest,
    GoldenNear,
}

impl MbType {
    fn reference(self) -> RefFrame {
        match self {
            Self::Intra => RefFrame::Intra,
            Self::InterNoMv
            | Self::InterMv
            | Self::InterNearest
            | Self::InterNear
            | Self::InterFourMv => RefFrame::Previous,
            _ => RefFrame::Golden,
        }
    }

    fn model_index(self) -> usize {
        match self {
            Self::InterNoMv => 0,
            Self::Intra => 1,
            Self::InterMv => 2,
            Self::InterNearest => 3,
            Self::InterNear => 4,
            Self::GoldenNoMv => 5,
            Self::GoldenMv => 6,
            Self::InterFourMv => 7,
            Self::GoldenNearest => 8,
            Self::GoldenNear => 9,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MbInfo {
    mb_type: MbType,
    mv: Mv,
}

#[derive(Default)]
struct DcPred {
    dc: [Vec<i16>; 3],
    refs_y: Vec<Option<RefFrame>>,
    refs_c: Vec<Option<RefFrame>>,
    left_y: [i16; 2],
    left_u: i16,
    left_v: i16,
    left_ref: Option<RefFrame>,
    y_idx: usize,
    c_idx: usize,
}

impl DcPred {
    fn resize(&mut self, mb_w: usize) {
        let luma = mb_w.saturating_mul(2).saturating_add(2);
        let chroma = mb_w.saturating_add(2);
        self.dc = [vec![0; luma], vec![0; chroma], vec![0; chroma]];
        self.refs_y = vec![None; luma];
        self.refs_c = vec![None; chroma];
    }

    fn reset(&mut self) {
        self.new_row();
        self.refs_y.fill(None);
        self.refs_c.fill(None);
    }

    fn new_row(&mut self) {
        self.y_idx = 1;
        self.c_idx = 1;
        self.left_y = [0; 2];
        self.left_u = 0;
        self.left_v = 0;
        self.left_ref = None;
    }

    fn next_mb(&mut self) {
        self.y_idx = self.y_idx.wrapping_add(2);
        self.c_idx = self.c_idx.wrapping_add(1);
    }
}

#[derive(Default)]
struct BlockState {
    coeff_cat: [u8; 4],
    last_idx: [usize; 4],
    dc_quant: i16,
    ac_quant: i16,
    dc_zero_run: [usize; 2],
    ac_zero_run: [usize; 2],
}

enum CoeffReader<'a> {
    Main,
    Bool(BoolDecoder<'a>),
    Huff(BitReader<'a>),
}

pub struct Decoder {
    st: StreamState,
    mb_w: usize,
    mb_h: usize,
    models: Models,
    mb_info: Vec<MbInfo>,
    dc_pred: DcPred,
    last_dc: [[i16; 3]; 3],
    top_ctx: [Vec<u8>; 3],
    coeffs: [[i16; 64]; 6],
    last_mbt: MbType,
    ilace_prob: u8,
    ilace_mb: bool,
    blk: BlockState,
    filter: Filter,
    work: Picture,
    last: Option<Picture>,
    golden: Option<Picture>,
}

impl Default for Decoder {
    fn default() -> Self {
        Self {
            st: StreamState::default(),
            mb_w: 0,
            mb_h: 0,
            models: Models::default(),
            mb_info: Vec::new(),
            dc_pred: DcPred::default(),
            last_dc: [[0; 3]; 3],
            top_ctx: [Vec::new(), Vec::new(), Vec::new()],
            coeffs: [[0; 64]; 6],
            last_mbt: MbType::InterNoMv,
            ilace_prob: 0,
            ilace_mb: false,
            blk: BlockState::default(),
            filter: Filter::default(),
            work: Picture::default(),
            last: None,
            golden: None,
        }
    }
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn decode(&mut self, frame: &[u8]) -> Result<&Picture> {
        let (hdr, mut bc) = header::parse(frame, &mut self.st)?;
        if hdr.multistream && hdr.offset >= frame.len() {
            return Err(Error::Truncated(
                "coefficient partition offset past the frame",
            ));
        }
        if hdr.is_intra {
            self.last = None;
            self.golden = None;
            if hdr.mb_w != self.mb_w || hdr.mb_h != self.mb_h || self.work.width() == 0 {
                self.set_dimensions(hdr.mb_w, hdr.mb_h);
            }
        } else if self.last.is_none() {
            return Err(Error::MissingReference);
        }

        let mut work = std::mem::take(&mut self.work);
        let res = self.decode_planes(&mut work, frame, &mut bc, &hdr);
        if let Err(e) = res {
            self.work = work;
            return Err(e);
        }
        if hdr.is_golden {
            match &mut self.golden {
                Some(g) => g.clone_from(&work),
                None => self.golden = Some(work.clone()),
            }
        }
        let previous = self.last.replace(work);
        self.work = previous.unwrap_or_else(|| Picture::new(self.mb_w, self.mb_h));
        self.last.as_ref().ok_or(Error::MissingReference)
    }

    fn set_dimensions(&mut self, mb_w: usize, mb_h: usize) {
        self.mb_w = mb_w;
        self.mb_h = mb_h;
        self.mb_info = vec![MbInfo::default(); mb_w.saturating_mul(mb_h)];
        self.top_ctx = [
            vec![0; mb_w.saturating_mul(2)],
            vec![0; mb_w],
            vec![0; mb_w],
        ];
        self.dc_pred.resize(mb_w);
        self.work = Picture::new(mb_w, mb_h);
    }

    fn decode_planes(
        &mut self,
        frm: &mut Picture,
        src: &[u8],
        bc: &mut BoolDecoder<'_>,
        hdr: &Header,
    ) -> Result<()> {
        let mut cr = if hdr.multistream {
            let part = src
                .get(hdr.offset..)
                .ok_or(Error::Truncated("coefficient partition"))?;
            if hdr.use_huffman {
                CoeffReader::Huff(BitReader::new(part))
            } else {
                CoeffReader::Bool(BoolDecoder::new(part))
            }
        } else {
            CoeffReader::Main
        };

        if hdr.is_intra {
            self.models.reset(self.st.interlaced);
        } else {
            self.models.decode_mode_probs(bc)?;
            self.models.decode_mv_models(bc);
        }
        self.models
            .decode_coeff_models(bc, hdr.is_intra, self.st.interlaced);
        if hdr.use_huffman {
            self.models.build_huffman()?;
        }
        if hdr.interlaced {
            self.ilace_prob = u8::try_from(bc.read_bits(8)).unwrap_or(0);
        }

        let q = hdr.quant;
        self.blk = BlockState {
            dc_quant: VP56_DC_QUANTS.get(q).copied().unwrap_or(0).wrapping_mul(4),
            ac_quant: VP56_AC_QUANTS.get(q).copied().unwrap_or(0).wrapping_mul(4),
            ..BlockState::default()
        };
        self.filter = Filter {
            advanced: self.st.profile == PROFILE_ADVANCED,
            autosel: self.st.autosel_pm,
            var_thresh: self.st.var_thresh,
            mv_thresh: self.st.mv_thresh,
            bicubic: self.st.bicubic,
            filter_alpha: self.st.filter_alpha,
            loop_thr: i16::from(VP56_FILTER_LIMITS.get(q).copied().unwrap_or(0)),
            deblock: self.st.loop_filter,
        };
        self.last_mbt = MbType::InterNoMv;
        for ctx in &mut self.top_ctx {
            ctx.fill(0);
        }
        self.last_dc = [[0; 3]; 3];
        self.last_dc[0][1] = 0x80;
        self.last_dc[0][2] = 0x80;
        self.dc_pred.reset();
        self.ilace_mb = false;

        for mb_y in 0..self.mb_h {
            self.blk.coeff_cat = [0; 4];
            self.blk.last_idx = [24; 4];
            for mb_x in 0..self.mb_w {
                self.decode_mb(frm, bc, &mut cr, hdr, mb_x, mb_y)?;
                self.dc_pred.next_mb();
                let coeff_overrun = match &cr {
                    CoeffReader::Bool(b) => b.overrun(),
                    _ => 0,
                };
                if bc.overrun() > MAX_OVERRUN || coeff_overrun > MAX_OVERRUN {
                    return Err(Error::Truncated("bool-coded data ran out"));
                }
            }
            self.dc_pred.new_row();
        }
        Ok(())
    }

    fn mb_at(&self, x: usize, y: usize) -> Option<&MbInfo> {
        self.mb_info.get(y.checked_mul(self.mb_w)?.checked_add(x)?)
    }

    fn find_mv_pred(&self, mb_x: usize, mb_y: usize, want: RefFrame) -> (usize, Mv, Mv, Mv) {
        const CAND_POS: [(isize, isize); 12] = [
            (-1, 0),
            (0, -1),
            (-1, -1),
            (-1, 1),
            (-2, 0),
            (0, -2),
            (-1, -2),
            (-2, -1),
            (-2, 1),
            (-1, 2),
            (-2, -2),
            (-2, 2),
        ];
        let mut nearest = Mv::default();
        let mut near = Mv::default();
        let mut pred = Mv::default();
        let mut count = 0usize;
        for (i, &(yoff, xoff)) in CAND_POS.iter().enumerate() {
            let (Some(cx), Some(cy)) =
                (mb_x.checked_add_signed(xoff), mb_y.checked_add_signed(yoff))
            else {
                continue;
            };
            if cx >= self.mb_w || cy >= self.mb_h {
                continue;
            }
            let Some(info) = self.mb_at(cx, cy) else {
                continue;
            };
            if info.mb_type.reference() != want || info.mv == Mv::default() {
                continue;
            }
            if count == 0 {
                nearest = info.mv;
                count = 1;
                if i < 2 {
                    pred = info.mv;
                }
            } else if info.mv != nearest {
                near = info.mv;
                count = 2;
                break;
            }
        }
        (count, nearest, near, pred)
    }

    fn decode_mv(&self, bc: &mut BoolDecoder<'_>) -> Mv {
        Mv {
            x: decode_mv_component(bc, &self.models.mv[0]),
            y: decode_mv_component(bc, &self.models.mv[1]),
        }
    }

    fn decode_mb_type(&mut self, bc: &mut BoolDecoder<'_>, ctx: usize) -> MbType {
        let probs = self
            .models
            .mbtype
            .get(ctx)
            .and_then(|c| c.get(self.last_mbt.model_index()))
            .copied()
            .unwrap_or([128; 10]);
        if !bc.read_prob(probs[9]) {
            self.last_mbt = tree!(
                bc,
                probs[0],
                tree!(
                    bc,
                    probs[1],
                    tree!(bc, probs[3], MbType::InterNoMv, MbType::InterMv),
                    tree!(bc, probs[4], MbType::InterNearest, MbType::InterNear)
                ),
                tree!(
                    bc,
                    probs[2],
                    tree!(bc, probs[5], MbType::Intra, MbType::InterFourMv),
                    tree!(
                        bc,
                        probs[6],
                        tree!(bc, probs[7], MbType::GoldenNoMv, MbType::GoldenMv),
                        tree!(bc, probs[8], MbType::GoldenNearest, MbType::GoldenNear)
                    )
                )
            );
        }
        self.last_mbt
    }

    fn decode_mb(
        &mut self,
        frm: &mut Picture,
        bc: &mut BoolDecoder<'_>,
        cr: &mut CoeffReader<'_>,
        hdr: &Header,
        mb_x: usize,
        mb_y: usize,
    ) -> Result<()> {
        const FOURMV_SUB_TYPE: [MbType; 4] = [
            MbType::InterNoMv,
            MbType::InterMv,
            MbType::InterNearest,
            MbType::InterNear,
        ];

        self.coeffs = [[0; 64]; 6];
        let mb_pos = mb_y
            .checked_mul(self.mb_w)
            .and_then(|p| p.checked_add(mb_x))
            .filter(|&p| p < self.mb_info.len())
            .ok_or(Error::InvalidData("macroblock position"))?;
        let mut four_mv = [Mv::default(); 4];

        if hdr.interlaced {
            let ip = self.ilace_prob;
            let prob = if mb_x == 0 {
                ip
            } else if !self.ilace_mb {
                ip.wrapping_add(u8::try_from(256u16.wrapping_sub(u16::from(ip)) >> 1).unwrap_or(0))
            } else {
                ip.wrapping_sub(ip >> 1)
            };
            self.ilace_mb = bc.read_prob(prob);
        }

        let (count, nearest, near, pred) = if hdr.is_intra {
            (0, Mv::default(), Mv::default(), Mv::default())
        } else {
            self.find_mv_pred(mb_x, mb_y, RefFrame::Previous)
        };
        let mb_type = if hdr.is_intra {
            MbType::Intra
        } else {
            self.decode_mb_type(bc, count.wrapping_add(1) % 3)
        };
        let mv = match mb_type {
            MbType::Intra | MbType::InterNoMv => Mv::default(),
            MbType::InterMv => pred.add(self.decode_mv(bc)),
            MbType::InterNearest => nearest,
            MbType::InterNear => near,
            MbType::InterFourMv => {
                let mut sub = [MbType::InterNoMv; 4];
                for s in &mut sub {
                    *s = FOURMV_SUB_TYPE
                        .get(usize::try_from(bc.read_bits(2)).unwrap_or(0))
                        .copied()
                        .unwrap_or(MbType::InterNoMv);
                }
                for (mv, s) in four_mv.iter_mut().zip(sub) {
                    *mv = match s {
                        MbType::InterMv => pred.add(self.decode_mv(bc)),
                        MbType::InterNearest => nearest,
                        MbType::InterNear => near,
                        _ => Mv::default(),
                    };
                }
                four_mv[3]
            }
            MbType::GoldenNoMv | MbType::GoldenMv | MbType::GoldenNearest | MbType::GoldenNear => {
                let (_, nearest, near, pred) = self.find_mv_pred(mb_x, mb_y, RefFrame::Golden);
                match mb_type {
                    MbType::GoldenMv => pred.add(self.decode_mv(bc)),
                    MbType::GoldenNearest => nearest,
                    MbType::GoldenNear => near,
                    _ => Mv::default(),
                }
            }
        };
        if let Some(info) = self.mb_info.get_mut(mb_pos) {
            *info = MbInfo { mb_type, mv };
        }

        match mb_type.reference() {
            RefFrame::Intra => {}
            reference => {
                let src = match reference {
                    RefFrame::Golden => self.golden.as_ref(),
                    _ => self.last.as_ref(),
                }
                .ok_or(Error::MissingReference)?;
                predict_mb(frm, src, mb_x, mb_y, mb_type, mv, &four_mv, &self.filter);
            }
        }

        for blk_no in 0..6usize {
            let luma = blk_no < 4;
            let plane = if luma { 0 } else { blk_no.wrapping_sub(3) };
            let ctx_idx = if luma {
                blk_no >> 1
            } else {
                blk_no.wrapping_sub(2)
            };
            let top_pos = if luma {
                mb_x.wrapping_mul(2).wrapping_add(blk_no & 1)
            } else {
                mb_x
            };
            let top_ctx = self
                .top_ctx
                .get(plane)
                .and_then(|t| t.get(top_pos))
                .copied()
                .unwrap_or(0);
            let model = self
                .models
                .coeff
                .get(usize::from(!luma))
                .copied()
                .unwrap_or_default();
            let coeffs = self
                .coeffs
                .get_mut(blk_no)
                .ok_or(Error::InvalidData("block"))?;
            let new_top = match cr {
                CoeffReader::Main => decode_block(
                    bc,
                    coeffs,
                    &model,
                    &self.models.zigzag,
                    &self.models.zero_run_probs,
                    &mut self.blk,
                    ctx_idx,
                    top_ctx,
                )?,
                CoeffReader::Bool(c) => decode_block(
                    c,
                    coeffs,
                    &model,
                    &self.models.zigzag,
                    &self.models.zero_run_probs,
                    &mut self.blk,
                    ctx_idx,
                    top_ctx,
                )?,
                CoeffReader::Huff(br) => {
                    decode_block_huff(
                        br,
                        coeffs,
                        &self.models,
                        &mut self.blk,
                        ctx_idx,
                        usize::from(!luma),
                    )?;
                    top_ctx
                }
            };
            if let Some(t) = self.top_ctx.get_mut(plane).and_then(|t| t.get_mut(top_pos)) {
                *t = new_top;
            }
            self.predict_dc(mb_type, blk_no)?;

            let has_ac = self.blk.last_idx.get(ctx_idx).copied().unwrap_or(0) > 0;
            let (bx, by) = if luma {
                (
                    mb_x.wrapping_mul(2).wrapping_add(blk_no & 1),
                    mb_y.wrapping_mul(2).wrapping_add(blk_no >> 1),
                )
            } else {
                (mb_x, mb_y)
            };
            let rows = if luma && self.ilace_mb {
                Rows::Field
            } else {
                Rows::Frame
            };
            let dc_only = !has_ac && matches!(rows, Rows::Frame);
            let coeffs = self
                .coeffs
                .get_mut(blk_no)
                .ok_or(Error::InvalidData("block"))?;
            let dst = frm
                .planes
                .get_mut(plane)
                .ok_or(Error::InvalidData("plane"))?;
            if mb_type == MbType::Intra {
                idct::put_block(coeffs, dc_only, dst, bx, by, rows);
            } else {
                idct::add_block(coeffs, dc_only, dst, bx, by, rows);
            }
        }
        Ok(())
    }

    fn predict_dc(&mut self, mb_type: MbType, blk_no: usize) -> Result<()> {
        const BAD: Error = Error::InvalidData("DC prediction");
        let luma = blk_no < 4;
        let plane = if luma { 0 } else { blk_no.wrapping_sub(3) };
        let reference = mb_type.reference();
        let dp = &mut self.dc_pred;
        let dc_idx = if luma {
            dp.y_idx.wrapping_add(blk_no & 1)
        } else {
            dp.c_idx
        };
        let refs = if luma { &mut dp.refs_y } else { &mut dp.refs_c };
        let dcs = dp.dc.get_mut(plane).ok_or(BAD)?;

        let mut pred = 0i16;
        let mut count = 0u8;
        let has_left_blk = luma && (blk_no & 1) == 1;
        if has_left_blk || dp.left_ref == Some(reference) {
            pred = pred.wrapping_add(match blk_no {
                0 | 1 => dp.left_y[0],
                2 | 3 => dp.left_y[1],
                4 => dp.left_u,
                _ => dp.left_v,
            });
            count = count.wrapping_add(1);
        }
        let top_ref = refs.get_mut(dc_idx).ok_or(BAD)?;
        let top_dc = dcs.get_mut(dc_idx).ok_or(BAD)?;
        if *top_ref == Some(reference) {
            pred = pred.wrapping_add(*top_dc);
            count = count.wrapping_add(1);
        }
        let last_dc = self
            .last_dc
            .get_mut(reference as usize)
            .and_then(|l| l.get_mut(plane))
            .ok_or(BAD)?;
        match count {
            0 => pred = *last_dc,
            2 => pred /= 2,
            _ => {}
        }
        let coeffs = self.coeffs.get_mut(blk_no).ok_or(BAD)?;
        coeffs[0] = coeffs[0].wrapping_add(pred);
        let dc = coeffs[0];

        if blk_no != 4 {
            *top_ref = Some(reference);
        }
        match blk_no {
            0 | 1 => dp.left_y[0] = dc,
            2 | 3 => dp.left_y[1] = dc,
            4 => dp.left_u = dc,
            _ => {
                dp.left_v = dc;
                dp.left_ref = Some(reference);
            }
        }
        *top_dc = dc;
        *last_dc = dc;
        coeffs[0] = dc.wrapping_mul(self.blk.dc_quant);
        Ok(())
    }
}

fn predict_mb(
    frm: &mut Picture,
    src: &Picture,
    mb_x: usize,
    mb_y: usize,
    mb_type: MbType,
    mv: Mv,
    four_mv: &[Mv; 4],
    f: &Filter,
) {
    let x = mb_x.wrapping_mul(16);
    let y = mb_y.wrapping_mul(16);
    let [dy, du, dv] = &mut frm.planes;
    let [sy, su, sv] = &src.planes;
    let chroma_mv = if mb_type == MbType::InterFourMv {
        for (blk, &bmv) in four_mv.iter().enumerate() {
            let bx = x.wrapping_add((blk & 1).wrapping_mul(8));
            let by = y.wrapping_add((blk & 2).wrapping_mul(4));
            mc::mc_block(dy, sy, bx, by, bmv, true, f);
        }
        let sum = four_mv.iter().fold(Mv::default(), |a, &b| a.add(b));
        Mv {
            x: sum.x.wrapping_div(4),
            y: sum.y.wrapping_div(4),
        }
    } else {
        for (bx, by) in [(0, 0), (8, 0), (0, 8), (8, 8)] {
            mc::mc_block(dy, sy, x.wrapping_add(bx), y.wrapping_add(by), mv, true, f);
        }
        mv
    };
    let (cx, cy) = (mb_x.wrapping_mul(8), mb_y.wrapping_mul(8));
    mc::mc_block(du, su, cx, cy, chroma_mv, false, f);
    mc::mc_block(dv, sv, cx, cy, chroma_mv, false, f);
}

fn decode_mv_component(bc: &mut BoolDecoder<'_>, m: &MvModel) -> i16 {
    let tp = &m.tree_probs;
    let val: i16 = if !bc.read_prob(m.nz_prob) {
        tree!(
            bc,
            tp[0],
            tree!(bc, tp[1], tree!(bc, tp[2], 0, 1), tree!(bc, tp[3], 2, 3)),
            tree!(bc, tp[4], tree!(bc, tp[5], 4, 5), tree!(bc, tp[6], 6, 7))
        )
    } else {
        let mut raw = 0i16;
        for &bit in &LONG_VECTOR_ORDER {
            let prob = m.raw_probs.get(bit).copied().unwrap_or(128);
            raw |= i16::from(bc.read_prob(prob)).wrapping_shl(u32::try_from(bit).unwrap_or(0));
        }
        if raw & 0xF0 != 0 {
            raw |= i16::from(bc.read_prob(m.raw_probs[3])).wrapping_shl(3);
        } else {
            raw |= 1 << 3;
        }
        raw
    };
    if val != 0 && bc.read_prob(m.sign_prob) {
        val.wrapping_neg()
    } else {
        val
    }
}

fn expand_token(bc: &mut BoolDecoder<'_>, val_probs: &[u8; 11], token: u8) -> i16 {
    let level = if token < TOKEN_LARGE {
        i16::from(token)
    } else {
        let cat: usize = tree!(
            bc,
            val_probs[6],
            tree!(bc, val_probs[7], 0, 1),
            tree!(
                bc,
                val_probs[8],
                tree!(bc, val_probs[9], 2, 3),
                tree!(bc, val_probs[10], 4, 5)
            )
        );
        let mut add = 0i16;
        for &prob in VP56_COEF_ADD_PROBS.get(cat).map_or(&[][..], |p| &p[..]) {
            if prob == 128 {
                break;
            }
            add = add.wrapping_shl(1) | i16::from(bc.read_prob(prob));
        }
        VP56_COEF_BASE
            .get(cat)
            .copied()
            .unwrap_or(0)
            .wrapping_add(add)
    };
    if token != 0 && bc.read_bool() {
        level.wrapping_neg()
    } else {
        level
    }
}

fn decode_token(bc: &mut BoolDecoder<'_>, probs: &[u8; 11], is_dc: bool, has_nnz: bool) -> u8 {
    if has_nnz && !bc.read_prob(probs[0]) {
        if is_dc || bc.read_prob(probs[1]) {
            0
        } else {
            TOKEN_EOB
        }
    } else {
        tree!(
            bc,
            probs[2],
            1,
            tree!(
                bc,
                probs[3],
                tree!(bc, probs[4], 2, tree!(bc, probs[5], 3, 4)),
                TOKEN_LARGE
            )
        )
    }
}

fn decode_zero_run(bc: &mut BoolDecoder<'_>, probs: &[u8; 14]) -> usize {
    let val = tree!(
        bc,
        probs[0],
        tree!(
            bc,
            probs[1],
            tree!(bc, probs[2], 0, 1),
            tree!(bc, probs[3], 2, 3)
        ),
        tree!(
            bc,
            probs[4],
            tree!(
                bc,
                probs[5],
                tree!(bc, probs[6], 4, 5),
                tree!(bc, probs[7], 6, 7)
            ),
            42
        )
    );
    if val != 42 {
        return val;
    }
    let mut run = 8usize;
    for (i, &p) in probs.iter().skip(8).enumerate() {
        run = run.wrapping_add(usize::from(bc.read_prob(p)) << i);
    }
    run
}

#[allow(clippy::too_many_arguments)]
fn decode_block(
    bc: &mut BoolDecoder<'_>,
    coeffs: &mut [i16; 64],
    model: &CoeffModel,
    zigzag: &[usize; 64],
    zero_run_probs: &[[u8; 14]; 2],
    blk: &mut BlockState,
    ctx_idx: usize,
    top_ctx: u8,
) -> Result<u8> {
    const BAD: Error = Error::InvalidData("coefficient context");
    let left_ctx = blk.coeff_cat.get(ctx_idx).copied().ok_or(BAD)?;
    let dc_mode = usize::from(top_ctx.wrapping_add(left_ctx));
    let dc_probs = model.dc_token_probs.get(dc_mode).ok_or(BAD)?;
    let mut probs = model.dc_value_probs;
    probs[..5].copy_from_slice(dc_probs);
    let token = decode_token(bc, &probs, true, true);
    let val = expand_token(bc, &model.dc_value_probs, token);
    coeffs[0] = val;

    let mut idx = 1usize;
    let mut last_val = val;
    while idx < 64 {
        let band = VP6_IDX_TO_AC_BAND.get(idx).copied().unwrap_or(5);
        let mode = usize::from(last_val.unsigned_abs().min(2));
        let probs = model
            .ac_val_probs
            .get(mode)
            .and_then(|m| m.get(band))
            .ok_or(BAD)?;
        let token = decode_token(bc, probs, false, idx == 1 || last_val != 0);
        if token == TOKEN_EOB {
            break;
        }
        let val = expand_token(bc, probs, token);
        let pos = zigzag.get(idx).copied().unwrap_or(0);
        if let Some(c) = coeffs.get_mut(pos) {
            *c = val.wrapping_mul(blk.ac_quant);
        }
        idx = idx.wrapping_add(1);
        last_val = val;
        if val == 0 {
            let table = zero_run_probs.get(usize::from(idx >= 7)).ok_or(BAD)?;
            idx = idx.wrapping_add(decode_zero_run(bc, table));
            if idx > 64 {
                return Err(Error::InvalidData("zero run past the block end"));
            }
        }
    }
    let cat = u8::from(coeffs[0] != 0);
    if let Some(c) = blk.coeff_cat.get_mut(ctx_idx) {
        *c = cat;
    }
    if let Some(l) = blk.last_idx.get_mut(ctx_idx) {
        *l = idx;
    }
    Ok(cat)
}

fn decode_token_huff(br: &mut BitReader<'_>, huff: &crate::huffman::Huff) -> Result<Option<i16>> {
    let tok = huff.read(br)?;
    let val = match tok {
        0 => return Ok(Some(0)),
        1..=4 => i16::from(tok),
        5..=10 => {
            let base = usize::from(tok.wrapping_sub(5));
            let nbits = VP6_COEF_ADD_BITS.get(base).copied().unwrap_or(0);
            let add = i16::try_from(br.read(u32::from(nbits))?).unwrap_or(0);
            VP56_COEF_BASE
                .get(base)
                .copied()
                .unwrap_or(0)
                .wrapping_add(add)
        }
        _ => return Ok(None),
    };
    Ok(Some(if br.read_bool()? {
        val.wrapping_neg()
    } else {
        val
    }))
}

fn decode_eob_run_huff(br: &mut BitReader<'_>) -> Result<usize> {
    let run = match br.read(2)? {
        0 => 0,
        1 => 1,
        2 => br.read(2)?.wrapping_add(2),
        _ => {
            if br.read_bool()? {
                br.read(6)?.wrapping_add(10)
            } else {
                br.read(2)?.wrapping_add(6)
            }
        }
    };
    Ok(usize::try_from(run).unwrap_or(0))
}

fn decode_block_huff(
    br: &mut BitReader<'_>,
    coeffs: &mut [i16; 64],
    models: &Models,
    blk: &mut BlockState,
    ctx_idx: usize,
    plane: usize,
) -> Result<()> {
    const BAD: Error = Error::InvalidData("Huffman context");
    let dc_run = blk.dc_zero_run.get_mut(plane).ok_or(BAD)?;
    let mut last_val = 0i16;
    if *dc_run == 0 {
        let tree = models.huff.dc.get(plane).ok_or(BAD)?;
        let Some(val) = decode_token_huff(br, tree)? else {
            return Ok(());
        };
        last_val = val;
        coeffs[0] = val;
        if val == 0 {
            *dc_run = decode_eob_run_huff(br)?;
        }
    } else {
        *dc_run = dc_run.wrapping_sub(1);
    }

    let ac_run = blk.ac_zero_run.get_mut(plane).ok_or(BAD)?;
    if *ac_run > 0 {
        *ac_run = ac_run.wrapping_sub(1);
        if let Some(l) = blk.last_idx.get_mut(ctx_idx) {
            *l = 0;
        }
        return Ok(());
    }

    let trees = models.huff.ac.get(plane).ok_or(BAD)?;
    let mut idx = 1usize;
    while idx < 64 {
        let band = VP6_IDX_TO_AC_BAND.get(idx).copied().unwrap_or(5).min(3);
        let mode = usize::from(last_val.unsigned_abs().min(2));
        let tree = trees.get(mode).and_then(|t| t.get(band)).ok_or(BAD)?;
        let Some(val) = decode_token_huff(br, tree)? else {
            if idx == 1 {
                *ac_run = decode_eob_run_huff(br)?;
            }
            break;
        };
        let pos = models.zigzag.get(idx).copied().unwrap_or(0);
        if let Some(c) = coeffs.get_mut(pos) {
            *c = val.wrapping_mul(blk.ac_quant);
        }
        idx = idx.wrapping_add(1);
        last_val = val;
        if val == 0 {
            let tree = models.huff.zero_run.get(usize::from(idx >= 7)).ok_or(BAD)?;
            let sym = tree.read(br)?;
            let run = if sym < 8 {
                usize::from(sym)
            } else {
                usize::try_from(br.read(6)?).unwrap_or(0).wrapping_add(8)
            };
            idx = idx.wrapping_add(run);
            if idx > 64 {
                return Err(Error::InvalidData("zero run past the block end"));
            }
        }
    }
    if let Some(l) = blk.last_idx.get_mut(ctx_idx) {
        *l = idx;
    }
    Ok(())
}
