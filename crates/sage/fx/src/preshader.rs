use crate::read::{u32_at, word_offset, words};
use crate::shader::{parse_ctab, Constant, FOURCC_CTAB};
use crate::{Error, Result};

const FOURCC_CLIT: u32 = 0x5449_4C43;
const FOURCC_FXLC: u32 = 0x434C_5846;

pub const TABLE_LITERAL: u32 = 1;
pub const TABLE_INPUT: u32 = 2;
pub const TABLE_OUTPUT: u32 = 4;
pub const TABLE_TEMP: u32 = 7;

const MAX_TEMPS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operand {
    pub table: u32,
    pub offset: u32,
    pub index: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Op {
    pub opcode: u32,
    pub components: u32,
    pub scalar: bool,
    pub inputs: Vec<Operand>,
    pub output: Operand,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Preshader {
    pub literals: Vec<f64>,
    pub constants: Vec<Constant>,
    pub ops: Vec<Op>,
}

#[must_use]
pub fn op_name(opcode: u32) -> &'static str {
    match opcode {
        0x100 => "mov",
        0x101 => "neg",
        0x103 => "rcp",
        0x104 => "frc",
        0x105 => "exp",
        0x106 => "log",
        0x107 => "rsq",
        0x108 => "sin",
        0x109 => "cos",
        0x10a => "asin",
        0x10b => "acos",
        0x10c => "atan",
        0x130 => "floor",
        0x131 => "ceil",
        0x200 => "min",
        0x201 => "max",
        0x202 => "lt",
        0x203 => "ge",
        0x204 => "add",
        0x205 => "mul",
        0x206 => "atan2",
        0x208 => "div",
        0x300 => "cmp",
        0x500 => "dot",
        _ => "?",
    }
}

impl Preshader {
    pub fn parse(body: &[u8]) -> Result<Self> {
        let tokens = words(body);
        let mut out = Self {
            literals: Vec::new(),
            constants: Vec::new(),
            ops: Vec::new(),
        };
        let mut at = 1usize;
        while let Some(&token) = tokens.get(at) {
            if token & 0xFFFF == 0xFFFF {
                break;
            }
            if token & 0xFFFF != 0xFFFE {
                return Err(Error::Unsupported {
                    what: "preshader token",
                    value: token,
                    offset: word_offset(at),
                });
            }
            let len = ((token >> 16) & 0x7FFF) as usize;
            let start = at.saturating_add(1);
            let end = start.saturating_add(len);
            let block = tokens.get(start..end).ok_or(Error::Truncated {
                offset: word_offset(at),
            })?;
            let bytes = body
                .get(start.saturating_mul(4)..end.saturating_mul(4))
                .ok_or(Error::Truncated {
                    offset: word_offset(at),
                })?;
            match block.first().copied() {
                Some(FOURCC_CLIT) => {
                    let count = u32_at(bytes, 4)? as usize;
                    let doubles = bytes.get(8..).unwrap_or_default();
                    out.literals = doubles
                        .chunks_exact(8)
                        .take(count)
                        .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8])))
                        .collect();
                }
                Some(FOURCC_CTAB) => {
                    out.constants = parse_ctab(bytes.get(4..).unwrap_or_default())?;
                }
                Some(FOURCC_FXLC) => out.ops = parse_fxlc(block.get(1..).unwrap_or_default())?,
                _ => {}
            }
            at = end;
        }
        Ok(out)
    }

    pub fn run(&self, inputs: &[f64], outputs: &mut [f32]) -> Result<()> {
        let mut temps: Vec<f64> = Vec::new();
        for (n, op) in self.ops.iter().enumerate() {
            let arity = match op.opcode {
                0x100..=0x131 => 1,
                0x200..=0x208 | 0x500 => 2,
                0x300 => 3,
                other => {
                    return Err(Error::Unsupported {
                        what: "preshader opcode",
                        value: other,
                        offset: n,
                    })
                }
            };
            if op.inputs.len() != arity {
                return Err(Error::Unsupported {
                    what: "preshader arity",
                    value: op.opcode,
                    offset: n,
                });
            }
            let fetch = |operand: &Operand, component: u32, temps: &[f64], outputs: &[f32]| {
                let mut offset = operand.offset;
                if let Some((table, index_offset)) = operand.index {
                    let index = read(table, index_offset, &self.literals, inputs, temps, outputs);
                    offset = offset.saturating_add(f64_to_u32_index(index).saturating_mul(4));
                }
                read(
                    operand.table,
                    offset.saturating_add(component),
                    &self.literals,
                    inputs,
                    temps,
                    outputs,
                )
            };
            if let (0x500, [left, right]) = (op.opcode, op.inputs.as_slice()) {
                let mut sum = 0.0;
                for j in 0..op.components {
                    sum += fetch(left, j, &temps, outputs) * fetch(right, j, &temps, outputs);
                }
                write(op.output, 0, sum, &mut temps, outputs)?;
                continue;
            }
            for j in 0..op.components {
                let mut args = [0.0f64; 3];
                for (k, (input, arg)) in op.inputs.iter().zip(args.iter_mut()).enumerate() {
                    let component = if op.scalar && k == 0 { 0 } else { j };
                    *arg = fetch(input, component, &temps, outputs);
                }
                let [a, b, c] = args;
                let value = match op.opcode {
                    0x100 => a,
                    0x101 => -a,
                    0x103 => 1.0 / a,
                    0x104 => a - a.floor(),
                    0x105 => mint_math::f64::exp(a),
                    0x106 => mint_math::f64::ln(a),
                    0x107 => {
                        let v = a.abs();
                        if v == 0.0 {
                            f64::INFINITY
                        } else {
                            1.0 / v.sqrt()
                        }
                    }
                    0x108 => mint_math::f64::sin(a),
                    0x109 => mint_math::f64::cos(a),
                    0x10a => mint_math::f64::asin(a),
                    0x10b => mint_math::f64::acos(a),
                    0x10c => mint_math::f64::atan(a),
                    0x130 => a.floor(),
                    0x131 => a.ceil(),
                    0x200 => a.min(b),
                    0x201 => a.max(b),
                    0x202 => f64::from(u8::from(a < b)),
                    0x203 => f64::from(u8::from(a >= b)),
                    0x204 => a + b,
                    0x205 => a * b,
                    0x206 => mint_math::f64::atan2(a, b),
                    0x208 => a / b,
                    0x300 => {
                        if a >= 0.0 {
                            b
                        } else {
                            c
                        }
                    }
                    other => {
                        return Err(Error::Unsupported {
                            what: "preshader opcode",
                            value: other,
                            offset: n,
                        })
                    }
                };
                write(op.output, j, value, &mut temps, outputs)?;
            }
        }
        Ok(())
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_to_u32_index(value: f64) -> u32 {
    value.max(0.0).min(f64::from(u32::MAX)) as u32
}

fn read(
    table: u32,
    offset: u32,
    literals: &[f64],
    inputs: &[f64],
    temps: &[f64],
    outputs: &[f32],
) -> f64 {
    let at = offset as usize;
    match table {
        TABLE_LITERAL => literals.get(at).copied().unwrap_or(0.0),
        TABLE_INPUT => inputs.get(at).copied().unwrap_or(0.0),
        TABLE_OUTPUT => outputs.get(at).copied().map_or(0.0, f64::from),
        TABLE_TEMP => temps.get(at).copied().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn write(
    output: Operand,
    component: u32,
    value: f64,
    temps: &mut Vec<f64>,
    outputs: &mut [f32],
) -> Result<()> {
    let at = output.offset.saturating_add(component) as usize;
    match output.table {
        TABLE_TEMP => {
            if at >= MAX_TEMPS {
                return Err(Error::CountTooLarge {
                    count: output.offset,
                    offset: 0,
                });
            }
            if temps.len() <= at {
                temps.resize(at.saturating_add(1), 0.0);
            }
            if let Some(slot) = temps.get_mut(at) {
                *slot = value;
            }
        }
        TABLE_OUTPUT => {
            if let Some(slot) = outputs.get_mut(at) {
                #[allow(clippy::cast_possible_truncation)]
                {
                    *slot = value as f32;
                }
            }
        }
        other => {
            return Err(Error::Unsupported {
                what: "preshader output table",
                value: other,
                offset: 0,
            })
        }
    }
    Ok(())
}

fn parse_fxlc(tokens: &[u32]) -> Result<Vec<Op>> {
    let count = *tokens.first().ok_or(Error::Truncated { offset: 0 })? as usize;
    if count > tokens.len() {
        return Err(Error::CountTooLarge {
            count: u32::try_from(count).unwrap_or(u32::MAX),
            offset: 0,
        });
    }
    let mut at = 1usize;
    let next = |at: &mut usize| -> Result<u32> {
        let value = tokens.get(*at).copied().ok_or(Error::Truncated {
            offset: word_offset(*at),
        })?;
        *at = at.saturating_add(1);
        Ok(value)
    };
    let mut ops = Vec::with_capacity(count);
    for _ in 0..count {
        let raw = next(&mut at)?;
        let input_count = next(&mut at)? as usize;
        if input_count > 8 {
            return Err(Error::CountTooLarge {
                count: u32::try_from(input_count).unwrap_or(u32::MAX),
                offset: word_offset(at),
            });
        }
        let mut operands = Vec::with_capacity(input_count.saturating_add(1));
        for _ in 0..=input_count {
            let relative = next(&mut at)?;
            let index = match relative {
                0 => None,
                1 => Some((next(&mut at)?, next(&mut at)?)),
                other => {
                    return Err(Error::Unsupported {
                        what: "preshader relative flag",
                        value: other,
                        offset: word_offset(at),
                    })
                }
            };
            let table = next(&mut at)?;
            let offset = next(&mut at)?;
            operands.push(Operand {
                table,
                offset,
                index,
            });
        }
        let output = operands.pop().ok_or(Error::Truncated {
            offset: word_offset(at),
        })?;
        ops.push(Op {
            opcode: (raw >> 20) & 0x7FF,
            components: raw & 0xFFFF,
            scalar: raw & 0x8000_0000 != 0,
            inputs: operands,
            output,
        });
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn one_mov_from_literal() -> Vec<u32> {
        let literal: [u8; 8] = 2.0f64.to_le_bytes();
        let lit_lo = u32::from_le_bytes(literal[0..4].try_into().unwrap_or([0; 4]));
        let lit_hi = u32::from_le_bytes(literal[4..8].try_into().unwrap_or([0; 4]));
        let clit_block = [FOURCC_CLIT, 1, lit_lo, lit_hi];
        let fxlc_block = [
            FOURCC_FXLC,
            1,
            0x1000_0001,
            1,
            0,
            TABLE_LITERAL,
            0,
            0,
            TABLE_OUTPUT,
            0,
        ];
        let mut tokens = vec![0u32];
        tokens.push((u32::try_from(clit_block.len()).unwrap_or(0) << 16) | 0xFFFE);
        tokens.extend_from_slice(&clit_block);
        tokens.push((u32::try_from(fxlc_block.len()).unwrap_or(0) << 16) | 0xFFFE);
        tokens.extend_from_slice(&fxlc_block);
        tokens.push(0xFFFF);
        tokens
    }

    #[test]
    fn a_literal_copied_to_the_output_register_runs_correctly() {
        let tokens = one_mov_from_literal();
        let body = le_bytes(&tokens);
        let pre = Preshader::parse(&body).expect("hand-built preshader must parse");
        assert_eq!(pre.literals, vec![2.0]);
        assert_eq!(pre.ops.len(), 1);
        let mut out = [0.0f32; 4];
        pre.run(&[], &mut out).expect("a bare mov must not fail");
        assert_eq!(out[0], 2.0);
    }

    #[test]
    fn truncating_a_valid_preshader_errors_but_never_panics() {
        let body = le_bytes(&one_mov_from_literal());
        for n in 0..=body.len() {
            let _ = Preshader::parse(&body[..n]);
        }
    }

    #[test]
    fn flipping_a_byte_in_a_valid_preshader_errors_but_never_panics() {
        let body = le_bytes(&one_mov_from_literal());
        for i in 0..body.len() {
            for bit in 0..8u8 {
                let mut mutated = body.clone();
                if let Some(byte) = mutated.get_mut(i) {
                    *byte ^= 1 << bit;
                }
                let _ = Preshader::parse(&mutated);
            }
        }
    }

    #[test]
    fn a_declared_op_count_past_the_tokens_present_is_rejected_up_front() {
        let fxlc_block = [FOURCC_FXLC, 1_000_000];
        let tokens = [
            0u32,
            (u32::try_from(fxlc_block.len()).unwrap_or(0) << 16) | 0xFFFE,
            fxlc_block[0],
            fxlc_block[1],
            0xFFFF,
        ];
        let body = le_bytes(&tokens);
        match Preshader::parse(&body) {
            Err(Error::CountTooLarge { count, .. }) => assert_eq!(count, 1_000_000),
            other => panic!("expected CountTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn an_op_with_more_than_eight_inputs_is_rejected() {
        let fxlc_block = [FOURCC_FXLC, 1, 0x1000_0001, 9];
        let tokens = [
            0u32,
            (u32::try_from(fxlc_block.len()).unwrap_or(0) << 16) | 0xFFFE,
            fxlc_block[0],
            fxlc_block[1],
            fxlc_block[2],
            fxlc_block[3],
            0xFFFF,
        ];
        let body = le_bytes(&tokens);
        assert!(matches!(
            Preshader::parse(&body),
            Err(Error::CountTooLarge { .. })
        ));
    }
}
