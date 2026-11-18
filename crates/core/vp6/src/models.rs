use crate::bool_decoder::BoolDecoder;
use crate::huffman::Huff;
use crate::tables::*;
use crate::{Error, Result};

macro_rules! tree {
    ($bc:expr, $prob:expr, $a:expr, $b:expr) => {
        if !$bc.read_prob($prob) {
            $a
        } else {
            $b
        }
    };
}
pub(crate) use tree;

#[derive(Clone, Copy, Default)]
pub(crate) struct MvModel {
    pub(crate) nz_prob: u8,
    pub(crate) sign_prob: u8,
    pub(crate) raw_probs: [u8; 8],
    pub(crate) tree_probs: [u8; 7],
}

#[derive(Clone, Copy, Default)]
pub(crate) struct CoeffModel {
    pub(crate) dc_token_probs: [[u8; 5]; 3],
    pub(crate) dc_value_probs: [u8; 11],
    pub(crate) ac_val_probs: [[[u8; 11]; 6]; 3],
}

#[derive(Clone, Copy, Default)]
pub(crate) struct HuffModels {
    pub(crate) dc: [Huff; 2],
    pub(crate) ac: [[[Huff; 6]; 3]; 2],
    pub(crate) zero_run: [Huff; 2],
}

pub(crate) struct Models {
    pub(crate) mv: [MvModel; 2],
    pub(crate) mbtype: [[[u8; 10]; 10]; 3],
    pub(crate) coeff: [CoeffModel; 2],
    prob_xmitted: [[u8; 20]; 3],
    scan_order: [usize; 64],
    pub(crate) zigzag: [usize; 64],
    pub(crate) zero_run_probs: [[u8; 14]; 2],
    pub(crate) huff: HuffModels,
}

impl Default for Models {
    fn default() -> Self {
        Self {
            mv: [MvModel::default(); 2],
            mbtype: [[[0; 10]; 10]; 3],
            coeff: [CoeffModel::default(); 2],
            prob_xmitted: [[0; 20]; 3],
            scan_order: [0; 64],
            zigzag: ZIGZAG,
            zero_run_probs: [[0; 14]; 2],
            huff: HuffModels::default(),
        }
    }
}

fn rescale_mode_prob(prob: u32, total: u32) -> u8 {
    let v = 255u32
        .wrapping_mul(prob)
        .checked_div(total.wrapping_add(1))
        .unwrap_or(0);
    u8::try_from(v).unwrap_or(u8::MAX)
}

fn rescale_prob(prob: u8, weights: [i16; 2]) -> u8 {
    let v = i32::from(prob)
        .wrapping_mul(i32::from(weights[0]))
        .wrapping_add(128)
        .wrapping_shr(8)
        .wrapping_add(i32::from(weights[1]));
    u8::try_from(v.clamp(1, 255)).unwrap_or(1)
}

impl Models {
    pub(crate) fn reset(&mut self, interlaced: bool) {
        for (i, m) in self.mv.iter_mut().enumerate() {
            m.nz_prob = NZ_PROBS.get(i).copied().unwrap_or(128);
            m.sign_prob = 128;
            m.raw_probs = RAW_PROBS.get(i).copied().unwrap_or_default();
            m.tree_probs = TREE_PROBS.get(i).copied().unwrap_or_default();
        }
        self.zero_run_probs = ZERO_RUN_PROBS;
        self.reset_scan(interlaced);
        self.prob_xmitted = DEFAULT_XMITTED_PROBS;
    }

    fn reset_scan(&mut self, interlaced: bool) {
        self.scan_order = if interlaced {
            VP6_INTERLACED_SCAN_ORDER
        } else {
            VP6_DEFAULT_SCAN_ORDER
        };
        self.zigzag = ZIGZAG;
    }

    fn update_scan(&mut self) {
        let mut scan = [0usize; 64];
        let mut idx = 1usize;
        for band in 0..16 {
            for (i, &order) in self.scan_order.iter().enumerate().skip(1) {
                if order == band {
                    if let Some(s) = scan.get_mut(idx) {
                        *s = i;
                    }
                    idx = idx.wrapping_add(1);
                }
            }
        }
        for (z, &s) in self.zigzag.iter_mut().zip(scan.iter()).skip(1) {
            *z = ZIGZAG.get(s).copied().unwrap_or(0);
        }
    }

    pub(crate) fn decode_mode_probs(&mut self, bc: &mut BoolDecoder<'_>) -> Result<()> {
        for ctx in 0..3usize {
            let px = self
                .prob_xmitted
                .get_mut(ctx)
                .ok_or(Error::InvalidData("mode context"))?;
            if bc.read_prob(174) {
                let idx = usize::try_from(bc.read_bits(4)).unwrap_or(0);
                let vq = VP56_MODE_VQ
                    .get(ctx)
                    .and_then(|c| c.get(idx))
                    .ok_or(Error::InvalidData("mode vector"))?;
                for (i, &v) in vq.iter().enumerate() {
                    if let Some(p) = px.get_mut(i ^ 1) {
                        *p = v;
                    }
                }
            }
            if bc.read_prob(254) {
                for set in 0..20usize {
                    if !bc.read_prob(205) {
                        continue;
                    }
                    let sign = bc.read_bool();
                    let diff = tree!(
                        bc,
                        171,
                        tree!(bc, 83, 2, 1),
                        tree!(
                            bc,
                            199,
                            tree!(bc, 140, tree!(bc, 125, tree!(bc, 104, 6, 5), 4), 3),
                            bc.read_bits(7)
                        )
                    )
                    .wrapping_mul(4);
                    let diff = u8::try_from(diff).map_err(|_| Error::InvalidData("mode delta"))?;
                    let p = px
                        .get_mut(set ^ 1)
                        .ok_or(Error::InvalidData("mode delta"))?;
                    *p = if sign {
                        p.checked_sub(diff)
                    } else {
                        p.checked_add(diff)
                    }
                    .ok_or(Error::InvalidData("mode delta"))?;
                }
            }
        }
        for (ctx, px) in self.prob_xmitted.iter().enumerate() {
            let Some(models) = self.mbtype.get_mut(ctx) else {
                continue;
            };
            for (mode, probs) in models.iter_mut().enumerate() {
                let p = |i: usize| u32::from(px.get(i).copied().unwrap_or(0));
                let mut cnt = [0u32; 10];
                let mut total = 0u32;
                for (i, c) in cnt.iter_mut().enumerate() {
                    if i != mode {
                        *c = p(i.wrapping_mul(2)).wrapping_mul(100);
                        total = total.wrapping_add(*c);
                    }
                }
                let m2 = mode.wrapping_mul(2);
                let sum = p(m2).wrapping_add(p(m2.wrapping_add(1)));
                probs[9] = 255u8.wrapping_sub(rescale_mode_prob(p(m2.wrapping_add(1)), sum));

                let inter_mv0 = cnt[0].wrapping_add(cnt[2]);
                let inter_mv1 = cnt[3].wrapping_add(cnt[4]);
                let gold_mv0 = cnt[5].wrapping_add(cnt[6]);
                let gold_mv1 = cnt[8].wrapping_add(cnt[9]);
                let mix = cnt[1].wrapping_add(cnt[7]);
                let r = |a: u32, b: u32| rescale_mode_prob(a, b).saturating_add(1);
                probs[0] = r(inter_mv0.wrapping_add(inter_mv1), total);
                probs[1] = r(inter_mv0, inter_mv0.wrapping_add(inter_mv1));
                probs[2] = r(mix, mix.wrapping_add(gold_mv0).wrapping_add(gold_mv1));
                probs[3] = r(cnt[0], inter_mv0);
                probs[4] = r(cnt[3], inter_mv1);
                probs[5] = r(cnt[1], mix);
                probs[6] = r(gold_mv0, gold_mv0.wrapping_add(gold_mv1));
                probs[7] = r(cnt[5], gold_mv0);
                probs[8] = r(cnt[8], gold_mv1);
            }
        }
        Ok(())
    }

    pub(crate) fn decode_mv_models(&mut self, bc: &mut BoolDecoder<'_>) {
        for (m, (&nz, &sign)) in self
            .mv
            .iter_mut()
            .zip(HAS_NZ_PROB.iter().zip(HAS_SIGN_PROB.iter()))
        {
            if bc.read_prob(nz) {
                m.nz_prob = bc.read_probability();
            }
            if bc.read_prob(sign) {
                m.sign_prob = bc.read_probability();
            }
        }
        for (m, has) in self.mv.iter_mut().zip(HAS_TREE_PROB.iter()) {
            for (p, &h) in m.tree_probs.iter_mut().zip(has.iter()) {
                if bc.read_prob(h) {
                    *p = bc.read_probability();
                }
            }
        }
        for (m, has) in self.mv.iter_mut().zip(HAS_RAW_PROB.iter()) {
            for (p, &h) in m.raw_probs.iter_mut().zip(has.iter()) {
                if bc.read_prob(h) {
                    *p = bc.read_probability();
                }
            }
        }
    }

    pub(crate) fn decode_coeff_models(
        &mut self,
        bc: &mut BoolDecoder<'_>,
        is_intra: bool,
        interlaced: bool,
    ) {
        let mut def_prob = [128u8; 11];
        for (model, has) in self.coeff.iter_mut().zip(HAS_COEF_PROBS.iter()) {
            for ((p, &h), def) in model
                .dc_value_probs
                .iter_mut()
                .zip(has.iter())
                .zip(def_prob.iter_mut())
            {
                if bc.read_prob(h) {
                    *def = bc.read_probability();
                    *p = *def;
                } else if is_intra {
                    *p = *def;
                }
            }
        }

        if bc.read_bool() {
            for (order, &h) in self
                .scan_order
                .iter_mut()
                .zip(HAS_SCAN_UPD_PROBS.iter())
                .skip(1)
            {
                if bc.read_prob(h) {
                    *order = usize::try_from(bc.read_bits(4)).unwrap_or(0);
                }
            }
            self.update_scan();
        } else {
            self.reset_scan(interlaced);
        }

        for (probs, has) in self
            .zero_run_probs
            .iter_mut()
            .zip(HAS_ZERO_RUN_PROBS.iter())
        {
            for (p, &h) in probs.iter_mut().zip(has.iter()) {
                if bc.read_prob(h) {
                    *p = bc.read_probability();
                }
            }
        }

        for (ctype, planes) in VP6_AC_PROBS.iter().enumerate() {
            for (model, groups) in self.coeff.iter_mut().zip(planes.iter()) {
                let Some(dst) = model.ac_val_probs.get_mut(ctype) else {
                    continue;
                };
                for (dgroup, has) in dst.iter_mut().zip(groups.iter()) {
                    for ((p, &h), def) in dgroup.iter_mut().zip(has.iter()).zip(def_prob.iter_mut())
                    {
                        if bc.read_prob(h) {
                            *def = bc.read_probability();
                            *p = *def;
                        } else if is_intra {
                            *p = *def;
                        }
                    }
                }
            }
        }

        for model in &mut self.coeff {
            for (ctx, probs) in model.dc_token_probs.iter_mut().enumerate() {
                for (k, p) in probs.iter_mut().enumerate() {
                    let weights = VP6_DC_WEIGHTS
                        .get(k)
                        .and_then(|w| w.get(ctx))
                        .copied()
                        .unwrap_or([0, 1]);
                    *p = rescale_prob(model.dc_value_probs.get(k).copied().unwrap_or(128), weights);
                }
            }
        }
    }

    pub(crate) fn build_huffman(&mut self) -> Result<()> {
        for (tree, model) in self.huff.dc.iter_mut().zip(self.coeff.iter()) {
            *tree = Huff::from_token_probs(&model.dc_value_probs)?;
        }
        for (trees, model) in self.huff.ac.iter_mut().zip(self.coeff.iter()) {
            for (mode_trees, mode_probs) in trees.iter_mut().zip(model.ac_val_probs.iter()) {
                for (tree, probs) in mode_trees.iter_mut().zip(mode_probs.iter()) {
                    *tree = Huff::from_token_probs(probs)?;
                }
            }
        }
        for (tree, probs) in self
            .huff
            .zero_run
            .iter_mut()
            .zip(self.zero_run_probs.iter())
        {
            *tree = Huff::from_zero_run_probs(probs)?;
        }
        Ok(())
    }
}
