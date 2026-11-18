use crate::preshader::Preshader;
use crate::read::{slice_at, u32_at, word_offset, words};
use crate::{Error, Result};

pub(crate) const FOURCC_CTAB: u32 = 0x4241_5443;
const FOURCC_PRES: u32 = 0x5345_5250;

const COMMENT: u32 = 0xFFFE;
const END: u32 = 0xFFFF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Pixel,
}

pub mod reg {
    pub const TEMP: u32 = 0;
    pub const INPUT: u32 = 1;
    pub const CONST: u32 = 2;
    pub const ADDR_OR_TEXTURE: u32 = 3;
    pub const RASTOUT: u32 = 4;
    pub const ATTROUT: u32 = 5;
    pub const TEXCRDOUT: u32 = 6;
    pub const CONSTINT: u32 = 7;
    pub const COLOROUT: u32 = 8;
    pub const DEPTHOUT: u32 = 9;
    pub const SAMPLER: u32 = 10;
    pub const CONSTBOOL: u32 = 14;
    pub const LOOP: u32 = 15;
    pub const MISCTYPE: u32 = 17;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register {
    pub kind: u32,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dst {
    pub reg: Register,
    pub mask: u8,
    pub modifiers: u8,
    pub shift: i8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Src {
    pub reg: Register,
    pub swizzle: [u8; 4],
    pub modifier: u8,
    pub relative: Option<(Register, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decl {
    pub usage: u8,
    pub usage_index: u8,
    pub texture_type: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub opcode: u32,
    pub controls: u8,
    pub dst: Option<Dst>,
    pub src: Vec<Src>,
    pub decl: Option<Decl>,
    pub literal: Vec<u32>,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constant {
    pub name: String,
    pub register_set: u16,
    pub register_index: u16,
    pub register_count: u16,
    pub class: u16,
    pub rows: u16,
    pub columns: u16,
    pub elements: u16,
    pub members: Vec<(String, u16, u16, u16, u16)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shader {
    pub stage: Stage,
    pub major: u8,
    pub minor: u8,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub preshader: Option<Preshader>,
}

#[must_use]
pub fn opcode_name(opcode: u32) -> &'static str {
    match opcode {
        0 => "nop",
        1 => "mov",
        2 => "add",
        3 => "sub",
        4 => "mad",
        5 => "mul",
        6 => "rcp",
        7 => "rsq",
        8 => "dp3",
        9 => "dp4",
        10 => "min",
        11 => "max",
        12 => "slt",
        13 => "sge",
        14 => "exp",
        15 => "log",
        16 => "lit",
        17 => "dst",
        18 => "lrp",
        19 => "frc",
        20 => "m4x4",
        21 => "m4x3",
        22 => "m3x4",
        23 => "m3x3",
        24 => "m3x2",
        25 => "call",
        26 => "callnz",
        27 => "loop",
        28 => "ret",
        29 => "endloop",
        30 => "label",
        31 => "dcl",
        32 => "pow",
        33 => "crs",
        34 => "sgn",
        35 => "abs",
        36 => "nrm",
        37 => "sincos",
        38 => "rep",
        39 => "endrep",
        40 => "if",
        41 => "ifc",
        42 => "else",
        43 => "endif",
        44 => "break",
        45 => "breakc",
        46 => "mova",
        47 => "defb",
        48 => "defi",
        64 => "texcoord",
        65 => "texkill",
        66 => "texld",
        67 => "texbem",
        68 => "texbeml",
        69 => "texreg2ar",
        70 => "texreg2gb",
        71 => "texm3x2pad",
        72 => "texm3x2tex",
        73 => "texm3x3pad",
        74 => "texm3x3tex",
        76 => "texm3x3spec",
        77 => "texm3x3vspec",
        78 => "expp",
        79 => "logp",
        80 => "cnd",
        81 => "def",
        82 => "texreg2rgb",
        83 => "texdp3tex",
        84 => "texm3x2depth",
        85 => "texdp3",
        86 => "texm3x3",
        87 => "texdepth",
        88 => "cmp",
        89 => "bem",
        90 => "dp2add",
        91 => "dsx",
        92 => "dsy",
        93 => "texldd",
        94 => "setsampler",
        95 => "texldl",
        96 => "breakp",
        _ => "?",
    }
}

fn operands(opcode: u32, stage: Stage, major: u8, minor: u8) -> Option<(bool, usize)> {
    let ps14 = stage == Stage::Pixel && major == 1 && minor >= 4;
    Some(match opcode {
        0 | 28 | 29 | 39 | 42 | 43 | 44 | 0xFFFD => (false, 0),
        1 | 6 | 7 | 14 | 15 | 16 | 19 | 35 | 36 | 46 | 78 | 79 | 91 | 92 => (true, 1),
        2 | 3 | 5 | 8..=13 | 17 | 20..=24 | 32 | 33 | 89 => (true, 2),
        4 | 18 | 80 | 88 | 90 => (true, 3),
        34 => (true, if major >= 3 { 1 } else { 3 }),
        37 => (true, if major >= 3 { 1 } else { 3 }),
        25 | 30 | 38 | 40 | 96 => (false, 1),
        26 | 27 | 41 | 45 => (false, 2),
        64 => (true, usize::from(ps14)),
        65 | 87 => (true, 0),
        66 if stage == Stage::Pixel && major == 1 => (true, usize::from(ps14)),
        66 | 95 => (true, 2),
        93 => (true, 4),
        67 | 68 | 69 | 70 | 71 | 72 | 73 | 74 | 77 | 82 | 83 | 84 | 85 | 86 => (true, 1),
        76 => (true, 2),
        _ => return None,
    })
}

fn register(token: u32) -> Register {
    Register {
        kind: ((token >> 28) & 0x7) | (((token >> 11) & 0x3) << 3),
        index: token & 0x7FF,
    }
}

fn dst_of(token: u32) -> Dst {
    let shift = ((token >> 24) & 0xF) as u8;
    Dst {
        reg: register(token),
        mask: ((token >> 16) & 0xF) as u8,
        modifiers: ((token >> 20) & 0xF) as u8,
        #[allow(clippy::cast_possible_truncation)]
        shift: if shift >= 8 {
            i16::from(shift).saturating_sub(16) as i8
        } else {
            shift as i8
        },
    }
}

fn swizzle(token: u32) -> [u8; 4] {
    [
        ((token >> 16) & 3) as u8,
        ((token >> 18) & 3) as u8,
        ((token >> 20) & 3) as u8,
        ((token >> 22) & 3) as u8,
    ]
}

impl Shader {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let tokens = words(bytes);
        let version = *tokens.first().ok_or(Error::Truncated { offset: 0 })?;
        let stage = match version >> 16 {
            0xFFFE => Stage::Vertex,
            0xFFFF => Stage::Pixel,
            _ => return Err(Error::NotAShader { token: version }),
        };
        let major = ((version >> 8) & 0xFF) as u8;
        let minor = (version & 0xFF) as u8;
        let mut shader = Self {
            stage,
            major,
            minor,
            instructions: Vec::new(),
            constants: Vec::new(),
            preshader: None,
        };
        let mut at = 1usize;
        let token_at = |i: usize| {
            tokens.get(i).copied().ok_or(Error::Truncated {
                offset: word_offset(i),
            })
        };
        loop {
            let token = token_at(at)?;
            let opcode = token & 0xFFFF;
            if opcode == END {
                break;
            }
            if opcode == COMMENT {
                let len = ((token >> 16) & 0x7FFF) as usize;
                let start = at.saturating_add(1);
                let body_bytes = slice_at(bytes, start.saturating_mul(4), len.saturating_mul(4))?;
                shader.comment(body_bytes)?;
                at = start.saturating_add(len);
                continue;
            }
            let length = ((token >> 24) & 0xF) as usize;
            let offset = at;
            at = at.saturating_add(1);
            let mut instruction = Instruction {
                opcode,
                controls: ((token >> 16) & 0xFF) as u8,
                dst: None,
                src: Vec::new(),
                decl: None,
                literal: Vec::new(),
                offset: word_offset(offset),
            };
            match opcode {
                31 => {
                    let usage = token_at(at)?;
                    instruction.decl = Some(Decl {
                        usage: (usage & 0x1F) as u8,
                        usage_index: ((usage >> 16) & 0xF) as u8,
                        texture_type: ((usage >> 27) & 0xF) as u8,
                    });
                    instruction.dst = Some(dst_of(token_at(at.saturating_add(1))?));
                    at = at.saturating_add(2);
                }
                81 | 48 => {
                    instruction.dst = Some(dst_of(token_at(at)?));
                    for i in 1..=4 {
                        instruction.literal.push(token_at(at.saturating_add(i))?);
                    }
                    at = at.saturating_add(5);
                }
                47 => {
                    instruction.dst = Some(dst_of(token_at(at)?));
                    instruction.literal.push(token_at(at.saturating_add(1))?);
                    at = at.saturating_add(2);
                }
                _ => {
                    let (has_dst, sources) =
                        operands(opcode, stage, major, minor).ok_or(Error::Unsupported {
                            what: "shader opcode",
                            value: opcode,
                            offset: word_offset(offset),
                        })?;
                    if has_dst {
                        let d = token_at(at)?;
                        at = at.saturating_add(1);
                        if d & (1 << 13) != 0 {
                            return Err(Error::Unsupported {
                                what: "relative addressing on a destination",
                                value: opcode,
                                offset: word_offset(offset),
                            });
                        }
                        instruction.dst = Some(dst_of(d));
                    }
                    for _ in 0..sources {
                        let s = token_at(at)?;
                        at = at.saturating_add(1);
                        let mut src = Src {
                            reg: register(s),
                            swizzle: swizzle(s),
                            modifier: ((s >> 24) & 0xF) as u8,
                            relative: None,
                        };
                        if s & (1 << 13) != 0 {
                            if major >= 2 {
                                let r = token_at(at)?;
                                at = at.saturating_add(1);
                                src.relative = Some((register(r), ((r >> 16) & 3) as u8));
                            } else {
                                src.relative = Some((
                                    Register {
                                        kind: reg::ADDR_OR_TEXTURE,
                                        index: 0,
                                    },
                                    0,
                                ));
                            }
                        }
                        instruction.src.push(src);
                    }
                }
            }
            if major >= 2 && at != offset.saturating_add(1).saturating_add(length) {
                return Err(Error::LengthMismatch {
                    opcode,
                    offset: word_offset(offset),
                });
            }
            shader.instructions.push(instruction);
        }
        Ok(shader)
    }

    fn comment(&mut self, body: &[u8]) -> Result<()> {
        match u32_at(body, 0)? {
            FOURCC_CTAB => self.constants = parse_ctab(body.get(4..).unwrap_or_default())?,
            FOURCC_PRES => {
                self.preshader = Some(Preshader::parse(body.get(4..).unwrap_or_default())?);
            }
            _ => {}
        }
        Ok(())
    }

    #[must_use]
    pub fn constant_at(&self, set: u16, index: u32) -> Option<&Constant> {
        self.constants.iter().find(|c| {
            c.register_set == set
                && index >= u32::from(c.register_index)
                && index < u32::from(c.register_index).saturating_add(u32::from(c.register_count))
        })
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    mint_bytes::u16_at(bytes, offset).ok_or(Error::Truncated { offset })
}

fn table_string(table: &[u8], offset: u32) -> String {
    let rest = table.get(offset as usize..).unwrap_or_default();
    let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
    String::from_utf8_lossy(rest.get(..end).unwrap_or_default()).into_owned()
}

pub(crate) fn parse_ctab(table: &[u8]) -> Result<Vec<Constant>> {
    let count = u32_at(table, 12)?;
    let info = u32_at(table, 16)? as usize;
    if count as usize > table.len() / 20 {
        return Err(Error::CountTooLarge { count, offset: 12 });
    }
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let at = info.saturating_add(i.saturating_mul(20));
        let name = table_string(table, u32_at(table, at)?);
        let type_info = u32_at(table, at.saturating_add(12))? as usize;
        let member_count = u16_at(table, type_info.saturating_add(10))?;
        let member_info = u32_at(table, type_info.saturating_add(12))? as usize;
        let mut members = Vec::new();
        for m in 0..usize::from(member_count).min(table.len() / 8) {
            let m_at = member_info.saturating_add(m.saturating_mul(8));
            let m_name = table_string(table, u32_at(table, m_at)?);
            let m_type = u32_at(table, m_at.saturating_add(4))? as usize;
            members.push((
                m_name,
                u16_at(table, m_type)?,
                u16_at(table, m_type.saturating_add(4))?,
                u16_at(table, m_type.saturating_add(6))?,
                u16_at(table, m_type.saturating_add(8))?,
            ));
        }
        out.push(Constant {
            name,
            register_set: u16_at(table, at.saturating_add(4))?,
            register_index: u16_at(table, at.saturating_add(6))?,
            register_count: u16_at(table, at.saturating_add(8))?,
            class: u16_at(table, type_info)?,
            rows: u16_at(table, type_info.saturating_add(4))?,
            columns: u16_at(table, type_info.saturating_add(6))?,
            elements: u16_at(table, type_info.saturating_add(8))?,
            members,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn one_mov() -> Vec<u32> {
        vec![
            0xFFFE_0200,
            0x0200_0001,
            0x000F_0000,
            0x20E4_0000,
            0x0000_FFFF,
        ]
    }

    #[test]
    fn a_hand_built_mov_parses_to_one_instruction() {
        let bytes = le_bytes(&one_mov());
        let shader = Shader::parse(&bytes).expect("hand-built shader must parse");
        assert_eq!(shader.stage, Stage::Vertex);
        assert_eq!(shader.major, 2);
        assert_eq!(shader.instructions.len(), 1);
        assert_eq!(shader.instructions[0].opcode, 1);
    }

    #[test]
    fn an_opcode_this_reader_has_no_operand_count_for_is_named_in_the_error() {
        let bytes = le_bytes(&[0xFFFE_0200, 0x0000_00C8, 0x0000_FFFF]);
        match Shader::parse(&bytes) {
            Err(Error::Unsupported { what, value, .. }) => {
                assert_eq!(what, "shader opcode");
                assert_eq!(value, 0xC8);
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_tag_that_is_neither_vertex_nor_pixel_is_refused() {
        let bytes = le_bytes(&[0x1234_5678, 0x0000_FFFF]);
        assert!(matches!(
            Shader::parse(&bytes),
            Err(Error::NotAShader { .. })
        ));
    }

    #[test]
    fn truncating_a_valid_shader_errors_but_never_panics() {
        let bytes = le_bytes(&one_mov());
        for n in 0..=bytes.len() {
            let _ = Shader::parse(&bytes[..n]);
        }
    }

    #[test]
    fn flipping_a_byte_in_a_valid_shader_errors_but_never_panics() {
        let bytes = le_bytes(&one_mov());
        for i in 0..bytes.len() {
            for bit in 0..8u8 {
                let mut mutated = bytes.clone();
                if let Some(byte) = mutated.get_mut(i) {
                    *byte ^= 1 << bit;
                }
                let _ = Shader::parse(&mutated);
            }
        }
    }

    #[test]
    fn a_declared_instruction_length_that_disagrees_with_its_operands_is_rejected() {
        let mut tokens = one_mov();
        if let Some(op) = tokens.get_mut(1) {
            *op = 0x0100_0001;
        }
        let bytes = le_bytes(&tokens);
        assert!(matches!(
            Shader::parse(&bytes),
            Err(Error::LengthMismatch { .. })
        ));
    }
}
