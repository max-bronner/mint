use crate::bits::BitReader;
use crate::{Error, Result};

const MAX_ELEMS: usize = 12;
const MAX_NODES: usize = MAX_ELEMS * 2;

#[derive(Clone, Copy, Default)]
pub(crate) struct Huff {
    codes: [u16; MAX_ELEMS],
    bits: [u8; MAX_ELEMS],
}

#[derive(Clone, Copy, Default)]
struct Node {
    weight: u16,
    sym: Option<u8>,
    ch0: usize,
    ch1: usize,
}

fn weight(a: u8, b: u8) -> u8 {
    let w = u16::from(a).wrapping_mul(u16::from(b)).wrapping_shr(8);
    u8::try_from(w).unwrap_or(u8::MAX).max(1)
}

impl Huff {
    pub(crate) fn from_token_probs(p: &[u8; 11]) -> Result<Self> {
        let mut w = [0u8; 12];
        w[11] = weight(p[0], p[1]);
        w[0] = weight(p[0], !p[1]);
        w[1] = weight(!p[0], p[2]);
        let lvroot = weight(!p[0], !p[2]);
        let tworoot = weight(lvroot, p[3]);
        let hlroot = weight(lvroot, !p[3]);
        w[2] = weight(tworoot, p[4]);
        let root34 = weight(tworoot, !p[4]);
        w[3] = weight(root34, p[5]);
        w[4] = weight(root34, !p[5]);
        let c1root = weight(hlroot, p[6]);
        let c34root = weight(hlroot, !p[6]);
        w[5] = weight(c1root, p[7]);
        w[6] = weight(c1root, !p[7]);
        let c3root = weight(c34root, p[8]);
        let c4root = weight(c34root, !p[8]);
        w[7] = weight(c3root, p[9]);
        w[8] = weight(c3root, !p[9]);
        w[9] = weight(c4root, p[10]);
        w[10] = weight(c4root, !p[10]);
        Self::build(&w)
    }

    pub(crate) fn from_zero_run_probs(p: &[u8; 14]) -> Result<Self> {
        let mut w = [0u8; 9];
        let root = weight(p[0], p[1]);
        w[0] = weight(root, p[2]);
        w[1] = weight(root, !p[2]);
        let root = weight(p[0], !p[1]);
        w[2] = weight(root, p[3]);
        w[3] = weight(root, !p[3]);
        let root = weight(!p[0], p[4]);
        w[8] = weight(!p[0], !p[4]);
        let root1 = weight(root, p[5]);
        let root2 = weight(root, !p[5]);
        w[4] = weight(root1, p[6]);
        w[5] = weight(root1, !p[6]);
        w[6] = weight(root2, p[7]);
        w[7] = weight(root2, !p[7]);
        Self::build(&w)
    }

    fn build(weights: &[u8]) -> Result<Self> {
        const BAD: Error = Error::InvalidData("Huffman tree");
        let mut nodes = [Node::default(); MAX_NODES];
        let mut nlen = 0usize;

        for (sym, &w) in weights.iter().enumerate().rev() {
            let weight = u16::from(w);
            let pos = nodes
                .get(..nlen)
                .and_then(|live| live.iter().position(|n| n.weight > weight))
                .unwrap_or(nlen);
            nodes.get_mut(pos..=nlen).ok_or(BAD)?.rotate_right(1);
            let node = nodes.get_mut(pos).ok_or(BAD)?;
            node.weight = weight;
            node.sym = Some(u8::try_from(sym).map_err(|_| BAD)?);
            nlen = nlen.wrapping_add(1);
        }

        let mut low = 0usize;
        let leaves = nlen;
        for _ in 1..leaves {
            let a = *nodes.get(low).ok_or(BAD)?;
            let b = *nodes.get(low.wrapping_add(1)).ok_or(BAD)?;
            let parent = Node {
                weight: a.weight.wrapping_add(b.weight),
                sym: None,
                ch0: low,
                ch1: low.wrapping_add(1),
            };
            low = low.wrapping_add(2);
            let pos = nodes
                .get(low..nlen)
                .and_then(|rest| rest.iter().position(|n| n.weight >= parent.weight))
                .map_or(nlen, |p| p.wrapping_add(low));
            nodes.get_mut(pos..=nlen).ok_or(BAD)?.rotate_right(1);
            *nodes.get_mut(pos).ok_or(BAD)? = parent;
            nlen = nlen.wrapping_add(1);
        }

        let mut huff = Self::default();
        let mut stack = [(0usize, 0u16, 0u8); MAX_NODES];
        *stack.first_mut().ok_or(BAD)? = (nlen.checked_sub(1).ok_or(BAD)?, 0, 0);
        let mut depth = 1usize;
        while let Some(top) = depth.checked_sub(1) {
            depth = top;
            let (pos, code, len) = *stack.get(top).ok_or(BAD)?;
            let node = nodes.get(pos).ok_or(BAD)?;
            match node.sym {
                Some(sym) => {
                    let sym = usize::from(sym);
                    *huff.codes.get_mut(sym).ok_or(BAD)? = code;
                    *huff.bits.get_mut(sym).ok_or(BAD)? = len;
                }
                None => {
                    let code = code.wrapping_shl(1);
                    let len = len.wrapping_add(1);
                    *stack.get_mut(depth).ok_or(BAD)? = (node.ch1, code | 1, len);
                    *stack.get_mut(depth.wrapping_add(1)).ok_or(BAD)? = (node.ch0, code, len);
                    depth = depth.wrapping_add(2);
                }
            }
        }
        let (code0, bits0) = (huff.codes[0], huff.bits[0]);
        for i in nlen..MAX_ELEMS {
            if let (Some(c), Some(b)) = (huff.codes.get_mut(i), huff.bits.get_mut(i)) {
                *c = code0;
                *b = bits0;
            }
        }
        Ok(huff)
    }

    pub(crate) fn read(&self, br: &mut BitReader<'_>) -> Result<u8> {
        let peek = br.peek16();
        for (sym, (&code, &bits)) in self.codes.iter().zip(self.bits.iter()).enumerate() {
            if peek.wrapping_shr(16u32.wrapping_sub(u32::from(bits))) == u32::from(code) {
                br.skip(u32::from(bits))?;
                return u8::try_from(sym).map_err(|_| Error::InvalidData("Huffman symbol"));
            }
        }
        Err(Error::InvalidData("Huffman code"))
    }
}
