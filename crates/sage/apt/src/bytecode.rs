use crate::reader::{checked_add, checked_mul, u32_at};
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub offset: u32,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub register: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub flags: u32,
    pub is_function2: bool,
    pub trailing_magic: (u32, u32),
    pub body_start: u32,
    pub body_size: u32,
    pub body_end_exact: bool,
    pub interior_end: bool,
    pub body: Vec<Instruction>,
}

impl Function {
    #[must_use]
    pub fn swf_flags(&self) -> u32 {
        self.flags >> 16
    }

    #[must_use]
    pub fn register_count(&self) -> u32 {
        self.flags & 0xFF
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub start: u32,
    pub end: u32,
    pub code: Vec<Instruction>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    pub max_instructions: usize,
    pub max_nesting: u32,
    pub max_string_len: usize,
    pub max_list_len: usize,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_instructions: 200_000,
            max_nesting: 16,
            max_string_len: 8192,
            max_list_len: 65_536,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Bare,
    Aligned4,
    Aligned8,
    AlignedStr,
    DefineFn,
    DefineFn2,
    Byte,
    Short,
    Word,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(missing_docs)]
pub enum Op {
    End,
    NextFrame,
    PrevFrame,
    Play,
    Stop,
    ToggleQuality,
    StopSounds,
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    LessThan,
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    StringEquals,
    StringLength,
    Substring,
    Pop,
    ToInteger,
    GetVariable,
    SetVariable,
    SetTargetExpression,
    StringConcat,
    GetProperty,
    SetProperty,
    DuplicateClip,
    RemoveClip,
    Trace,
    StartDragMovie,
    StopDragMovie,
    StringCompare,
    Throw,
    CastOp,
    ImplementsOp,
    Random,
    MbLength,
    Ord,
    Chr,
    GetTimer,
    MbSubstring,
    MbOrd,
    MbChr,
    Delete,
    Delete2,
    DefineLocal,
    CallFunction,
    Return,
    Modulo,
    New,
    Var,
    InitArray,
    InitObject,
    TypeOf,
    TargetPath,
    Enumerate,
    NewAdd,
    NewLessThan,
    NewEquals,
    ToNumber,
    ToString,
    Dup,
    Swap,
    GetMember,
    SetMember,
    Increment,
    Decrement,
    CallMethod,
    NewMethod,
    InstanceOf,
    Enumerate2,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
    ShiftRight2,
    StrictEquals,
    Greater,
    StringGreater,
    Extends,
    EaAction56,
    EaAction58,
    EaPushZero,
    EaPushOne,
    EaCallFunctionPop,
    EaCallFunction,
    EaCallMethodPop,
    EaCallMethod,
    EaPushThis,
    EaPushGlobal,
    EaZeroVariable,
    EaPushTrue,
    EaPushFalse,
    EaPushNull,
    EaPushUndefined,
    EaAction77,
    WaitForFrame,
    WaitForFrameExpression,
    GetUrl2,
    CallFrame,
    Try,
    GotoFrame(i32),
    SetRegister(u32),
    With(i32),
    BranchAlways(i32),
    BranchIfTrue(i32),
    GotoExpression(i32),
    EaBranchIfFalse(i32),
    GetUrl { url: String, target: String },
    ConstantPool(Vec<u32>),
    PushData(Vec<u32>),
    SetTarget(String),
    GotoLabel(String),
    EaPushString(String),
    EaGetStringVar(String),
    EaGetStringMember(String),
    EaSetStringVar(String),
    EaSetStringMember(String),
    EaPushConstant(u8),
    EaPushWordConstant(u16),
    EaPushValueOfVar(u8),
    EaGetNamedMember(u8),
    EaCallNamedFunctionPop(u8),
    EaCallNamedFunction(u8),
    EaCallNamedMethodPop(u8),
    EaCallNamedMethod(u8),
    EaPushByte(i8),
    EaPushShort(i16),
    EaPushFloat(f32),
    EaPushLong(u32),
    EaPushRegister(u8),
    DefineFunction(usize),
    DefineFunction2(usize),
}

impl Op {
    #[must_use]
    pub fn opcode(&self) -> u8 {
        match *self {
            Op::End => 0x00,
            Op::NextFrame => 0x04,
            Op::PrevFrame => 0x05,
            Op::Play => 0x06,
            Op::Stop => 0x07,
            Op::ToggleQuality => 0x08,
            Op::StopSounds => 0x09,
            Op::Add => 0x0A,
            Op::Subtract => 0x0B,
            Op::Multiply => 0x0C,
            Op::Divide => 0x0D,
            Op::Equals => 0x0E,
            Op::LessThan => 0x0F,
            Op::LogicalAnd => 0x10,
            Op::LogicalOr => 0x11,
            Op::LogicalNot => 0x12,
            Op::StringEquals => 0x13,
            Op::StringLength => 0x14,
            Op::Substring => 0x15,
            Op::Pop => 0x17,
            Op::ToInteger => 0x18,
            Op::GetVariable => 0x1C,
            Op::SetVariable => 0x1D,
            Op::SetTargetExpression => 0x20,
            Op::StringConcat => 0x21,
            Op::GetProperty => 0x22,
            Op::SetProperty => 0x23,
            Op::DuplicateClip => 0x24,
            Op::RemoveClip => 0x25,
            Op::Trace => 0x26,
            Op::StartDragMovie => 0x27,
            Op::StopDragMovie => 0x28,
            Op::StringCompare => 0x29,
            Op::Throw => 0x2A,
            Op::CastOp => 0x2B,
            Op::ImplementsOp => 0x2C,
            Op::Random => 0x30,
            Op::MbLength => 0x31,
            Op::Ord => 0x32,
            Op::Chr => 0x33,
            Op::GetTimer => 0x34,
            Op::MbSubstring => 0x35,
            Op::MbOrd => 0x36,
            Op::MbChr => 0x37,
            Op::Delete => 0x3A,
            Op::Delete2 => 0x3B,
            Op::DefineLocal => 0x3C,
            Op::CallFunction => 0x3D,
            Op::Return => 0x3E,
            Op::Modulo => 0x3F,
            Op::New => 0x40,
            Op::Var => 0x41,
            Op::InitArray => 0x42,
            Op::InitObject => 0x43,
            Op::TypeOf => 0x44,
            Op::TargetPath => 0x45,
            Op::Enumerate => 0x46,
            Op::NewAdd => 0x47,
            Op::NewLessThan => 0x48,
            Op::NewEquals => 0x49,
            Op::ToNumber => 0x4A,
            Op::ToString => 0x4B,
            Op::Dup => 0x4C,
            Op::Swap => 0x4D,
            Op::GetMember => 0x4E,
            Op::SetMember => 0x4F,
            Op::Increment => 0x50,
            Op::Decrement => 0x51,
            Op::CallMethod => 0x52,
            Op::NewMethod => 0x53,
            Op::InstanceOf => 0x54,
            Op::Enumerate2 => 0x55,
            Op::EaAction56 => 0x56,
            Op::EaAction58 => 0x58,
            Op::EaPushZero => 0x59,
            Op::EaPushOne => 0x5A,
            Op::EaCallFunctionPop => 0x5B,
            Op::EaCallFunction => 0x5C,
            Op::EaCallMethodPop => 0x5D,
            Op::EaCallMethod => 0x5E,
            Op::BitwiseAnd => 0x60,
            Op::BitwiseOr => 0x61,
            Op::BitwiseXor => 0x62,
            Op::ShiftLeft => 0x63,
            Op::ShiftRight => 0x64,
            Op::ShiftRight2 => 0x65,
            Op::StrictEquals => 0x66,
            Op::Greater => 0x67,
            Op::StringGreater => 0x68,
            Op::Extends => 0x69,
            Op::EaPushThis => 0x70,
            Op::EaPushGlobal => 0x71,
            Op::EaZeroVariable => 0x72,
            Op::EaPushTrue => 0x73,
            Op::EaPushFalse => 0x74,
            Op::EaPushNull => 0x75,
            Op::EaPushUndefined => 0x76,
            Op::EaAction77 => 0x77,
            Op::GotoFrame(_) => 0x81,
            Op::GetUrl { .. } => 0x83,
            Op::SetRegister(_) => 0x87,
            Op::ConstantPool(_) => 0x88,
            Op::WaitForFrame => 0x8A,
            Op::SetTarget(_) => 0x8B,
            Op::GotoLabel(_) => 0x8C,
            Op::WaitForFrameExpression => 0x8D,
            Op::DefineFunction2(_) => 0x8E,
            Op::Try => 0x8F,
            Op::With(_) => 0x94,
            Op::PushData(_) => 0x96,
            Op::BranchAlways(_) => 0x99,
            Op::GetUrl2 => 0x9A,
            Op::DefineFunction(_) => 0x9B,
            Op::BranchIfTrue(_) => 0x9D,
            Op::CallFrame => 0x9E,
            Op::GotoExpression(_) => 0x9F,
            Op::EaPushString(_) => 0xA1,
            Op::EaPushConstant(_) => 0xA2,
            Op::EaPushWordConstant(_) => 0xA3,
            Op::EaGetStringVar(_) => 0xA4,
            Op::EaGetStringMember(_) => 0xA5,
            Op::EaSetStringVar(_) => 0xA6,
            Op::EaSetStringMember(_) => 0xA7,
            Op::EaPushValueOfVar(_) => 0xAE,
            Op::EaGetNamedMember(_) => 0xAF,
            Op::EaCallNamedFunctionPop(_) => 0xB0,
            Op::EaCallNamedFunction(_) => 0xB1,
            Op::EaCallNamedMethodPop(_) => 0xB2,
            Op::EaCallNamedMethod(_) => 0xB3,
            Op::EaPushFloat(_) => 0xB4,
            Op::EaPushByte(_) => 0xB5,
            Op::EaPushShort(_) => 0xB6,
            Op::EaPushLong(_) => 0xB7,
            Op::EaBranchIfFalse(_) => 0xB8,
            Op::EaPushRegister(_) => 0xB9,
        }
    }
}

#[must_use]
pub fn opcode_name(opcode: u8) -> Option<&'static str> {
    Some(match opcode {
        0x00 => "End",
        0x04 => "NextFrame",
        0x05 => "PrevFrame",
        0x06 => "Play",
        0x07 => "Stop",
        0x08 => "ToggleQuality",
        0x09 => "StopSounds",
        0x0A => "Add",
        0x0B => "Subtract",
        0x0C => "Multiply",
        0x0D => "Divide",
        0x0E => "Equals",
        0x0F => "LessThan",
        0x10 => "LogicalAnd",
        0x11 => "LogicalOr",
        0x12 => "LogicalNot",
        0x13 => "StringEquals",
        0x14 => "StringLength",
        0x15 => "Substring",
        0x17 => "Pop",
        0x18 => "ToInteger",
        0x1C => "GetVariable",
        0x1D => "SetVariable",
        0x20 => "SetTargetExpression",
        0x21 => "StringConcat",
        0x22 => "GetProperty",
        0x23 => "SetProperty",
        0x24 => "DuplicateClip",
        0x25 => "RemoveClip",
        0x26 => "Trace",
        0x27 => "StartDragMovie",
        0x28 => "StopDragMovie",
        0x29 => "StringCompare",
        0x2A => "Throw",
        0x2B => "CastOp",
        0x2C => "ImplementsOp",
        0x30 => "Random",
        0x31 => "MbLength",
        0x32 => "Ord",
        0x33 => "Chr",
        0x34 => "GetTimer",
        0x35 => "MbSubstring",
        0x36 => "MbOrd",
        0x37 => "MbChr",
        0x3A => "Delete",
        0x3B => "Delete2",
        0x3C => "DefineLocal",
        0x3D => "CallFunction",
        0x3E => "Return",
        0x3F => "Modulo",
        0x40 => "New",
        0x41 => "Var",
        0x42 => "InitArray",
        0x43 => "InitObject",
        0x44 => "TypeOf",
        0x45 => "TargetPath",
        0x46 => "Enumerate",
        0x47 => "NewAdd",
        0x48 => "NewLessThan",
        0x49 => "NewEquals",
        0x4A => "ToNumber",
        0x4B => "ToString",
        0x4C => "Dup",
        0x4D => "Swap",
        0x4E => "GetMember",
        0x4F => "SetMember",
        0x50 => "Increment",
        0x51 => "Decrement",
        0x52 => "CallMethod",
        0x53 => "NewMethod",
        0x54 => "InstanceOf",
        0x55 => "Enumerate2",
        0x56 => "EA_ACTION56",
        0x58 => "EA_ACTION58",
        0x59 => "EA_PushZero",
        0x5A => "EA_PushOne",
        0x5B => "EA_CallFunctionPop",
        0x5C => "EA_CallFunction",
        0x5D => "EA_CallMethodPop",
        0x5E => "EA_CallMethod",
        0x60 => "BitwiseAnd",
        0x61 => "BitwiseOr",
        0x62 => "BitwiseXor",
        0x63 => "ShiftLeft",
        0x64 => "ShiftRight",
        0x65 => "ShiftRight2",
        0x66 => "StrictEquals",
        0x67 => "Greater",
        0x68 => "StringGreater",
        0x69 => "Extends",
        0x70 => "EA_PushThis",
        0x71 => "EA_PushGlobal",
        0x72 => "EA_ZeroVariable",
        0x73 => "EA_PushTrue",
        0x74 => "EA_PushFalse",
        0x75 => "EA_PushNull",
        0x76 => "EA_PushUndefined",
        0x77 => "EA_ACTION77",
        0x81 => "GotoFrame",
        0x83 => "GetUrl",
        0x87 => "SetRegister",
        0x88 => "ConstantPool",
        0x8A => "WaitForFrame",
        0x8B => "SetTarget",
        0x8C => "GotoLabel",
        0x8D => "WaitForFrameExpression",
        0x8E => "DefineFunction2",
        0x8F => "Try",
        0x94 => "With",
        0x96 => "PushData",
        0x99 => "BranchAlways",
        0x9A => "GetUrl2",
        0x9B => "DefineFunction",
        0x9D => "BranchIfTrue",
        0x9E => "CallFrame",
        0x9F => "GotoExpression",
        0xA1 => "EA_PushString",
        0xA2 => "EA_PushConstant",
        0xA3 => "EA_PushWordConstant",
        0xA4 => "EA_GetStringVar",
        0xA5 => "EA_GetStringMember",
        0xA6 => "EA_SetStringVar",
        0xA7 => "EA_SetStringMember",
        0xAE => "EA_PushValueOfVar",
        0xAF => "EA_GetNamedMember",
        0xB0 => "EA_CallNamedFunctionPop",
        0xB1 => "EA_CallNamedFunction",
        0xB2 => "EA_CallNamedMethodPop",
        0xB3 => "EA_CallNamedMethod",
        0xB4 => "EA_PushFloat",
        0xB5 => "EA_PushByte",
        0xB6 => "EA_PushShort",
        0xB7 => "EA_PushLong",
        0xB8 => "EA_BranchIfFalse",
        0xB9 => "EA_PushRegister",
        _ => return None,
    })
}

fn shape(opcode: u8) -> Option<Shape> {
    opcode_name(opcode)?;
    Some(match opcode {
        0x81 | 0x87 | 0x94 | 0x99 | 0x9D | 0x9F | 0xB8 => Shape::Aligned4,
        0x83 | 0x88 | 0x96 => Shape::Aligned8,
        0x8B | 0x8C | 0xA1 | 0xA4 | 0xA5 | 0xA6 | 0xA7 => Shape::AlignedStr,
        0x8E => Shape::DefineFn2,
        0x9B => Shape::DefineFn,
        0xA2 | 0xAE | 0xAF | 0xB0 | 0xB1 | 0xB2 | 0xB3 | 0xB5 | 0xB9 => Shape::Byte,
        0xA3 | 0xB6 => Shape::Short,
        0xB4 | 0xB7 => Shape::Word,
        _ => Shape::Bare,
    })
}

#[must_use]
pub fn preload_register_base(flags: u32) -> u32 {
    const PRELOAD_BITS: [u32; 6] = [0x01, 0x04, 0x10, 0x40, 0x80, 0x100];
    let mut base: u32 = 1;
    for bit in PRELOAD_BITS {
        if flags & bit != 0 {
            base = base.saturating_add(1);
        }
    }
    base
}

pub fn decode_block(apt: &[u8], start: u32, limits: &DecodeLimits) -> Result<Block> {
    let mut decoder = Decoder {
        apt,
        limits,
        functions: Vec::new(),
        instructions: 0,
    };
    let (code, end) = decoder.decode_run(start as usize, None, 0)?;
    Ok(Block {
        start,
        end: u32::try_from(end).unwrap_or(u32::MAX),
        code,
        functions: decoder.functions,
    })
}

struct Decoder<'a> {
    apt: &'a [u8],
    limits: &'a DecodeLimits,
    functions: Vec<Function>,
    instructions: usize,
}

impl Decoder<'_> {
    fn decode_run(
        &mut self,
        mut pos: usize,
        body_end: Option<usize>,
        depth: u32,
    ) -> Result<(Vec<Instruction>, usize)> {
        if depth > self.limits.max_nesting {
            return Err(Error::BytecodeLimit {
                what: "function nesting depth",
                limit: self.limits.max_nesting as usize,
            });
        }
        let mut code = Vec::new();
        loop {
            if let Some(limit) = body_end {
                if pos == limit {
                    return Ok((code, pos));
                }
                if pos > limit {
                    return Err(Error::FunctionBodyMisclose {
                        end: u32::try_from(limit).unwrap_or(u32::MAX),
                        landed: u32::try_from(pos).unwrap_or(u32::MAX),
                    });
                }
            }
            self.instructions = checked_add(self.instructions, 1)?;
            if self.instructions > self.limits.max_instructions {
                return Err(Error::BytecodeLimit {
                    what: "instructions in one block",
                    limit: self.limits.max_instructions,
                });
            }
            let offset = pos;
            let opcode = *self.apt.get(pos).ok_or(Error::OutOfBounds {
                offset: pos,
                len: self.apt.len(),
            })?;
            let shape = shape(opcode).ok_or(Error::UnknownOpcode {
                offset: u32::try_from(pos).unwrap_or(u32::MAX),
                opcode,
            })?;
            pos = checked_add(pos, 1)?;
            let op = self.decode_operand(opcode, shape, &mut pos, depth)?;
            let is_end = matches!(op, Op::End);
            code.push(Instruction {
                offset: u32::try_from(offset).unwrap_or(u32::MAX),
                op,
            });
            if is_end && body_end.is_none() {
                return Ok((code, pos));
            }
        }
    }

    fn decode_operand(
        &mut self,
        opcode: u8,
        shape: Shape,
        pos: &mut usize,
        depth: u32,
    ) -> Result<Op> {
        match shape {
            Shape::Bare => bare_op(opcode),
            Shape::Aligned4 => {
                let value = self.aligned_word(pos)?;
                #[allow(clippy::cast_possible_wrap)]
                let signed = value as i32;
                Ok(match opcode {
                    0x81 => Op::GotoFrame(signed),
                    0x87 => Op::SetRegister(value),
                    0x94 => Op::With(signed),
                    0x99 => Op::BranchAlways(signed),
                    0x9D => Op::BranchIfTrue(signed),
                    0x9F => Op::GotoExpression(signed),
                    _ => Op::EaBranchIfFalse(signed),
                })
            }
            Shape::AlignedStr => {
                let ptr = self.aligned_word(pos)?;
                let text = self.string_at(ptr)?;
                Ok(match opcode {
                    0x8B => Op::SetTarget(text),
                    0x8C => Op::GotoLabel(text),
                    0xA1 => Op::EaPushString(text),
                    0xA4 => Op::EaGetStringVar(text),
                    0xA5 => Op::EaGetStringMember(text),
                    0xA6 => Op::EaSetStringVar(text),
                    _ => Op::EaSetStringMember(text),
                })
            }
            Shape::Aligned8 => {
                let first = self.aligned_word(pos)?;
                let second = read_word(self.apt, pos)?;
                match opcode {
                    0x83 => Ok(Op::GetUrl {
                        url: self.string_at(first)?,
                        target: self.string_at(second)?,
                    }),
                    0x88 => Ok(Op::ConstantPool(self.index_list(first, second)?)),
                    _ => Ok(Op::PushData(self.index_list(first, second)?)),
                }
            }
            Shape::Byte => {
                let value = *self.apt.get(*pos).ok_or(Error::OutOfBounds {
                    offset: *pos,
                    len: self.apt.len(),
                })?;
                *pos = checked_add(*pos, 1)?;
                Ok(match opcode {
                    0xA2 => Op::EaPushConstant(value),
                    0xAE => Op::EaPushValueOfVar(value),
                    0xAF => Op::EaGetNamedMember(value),
                    0xB0 => Op::EaCallNamedFunctionPop(value),
                    0xB1 => Op::EaCallNamedFunction(value),
                    0xB2 => Op::EaCallNamedMethodPop(value),
                    0xB3 => Op::EaCallNamedMethod(value),
                    0xB5 => Op::EaPushByte(i8::from_le_bytes([value])),
                    _ => Op::EaPushRegister(value),
                })
            }
            Shape::Short => {
                let lo = *self.apt.get(*pos).ok_or(Error::OutOfBounds {
                    offset: *pos,
                    len: self.apt.len(),
                })?;
                let hi = *self
                    .apt
                    .get(checked_add(*pos, 1)?)
                    .ok_or(Error::OutOfBounds {
                        offset: *pos,
                        len: self.apt.len(),
                    })?;
                *pos = checked_add(*pos, 2)?;
                let value = u16::from_le_bytes([lo, hi]);
                Ok(if opcode == 0xA3 {
                    Op::EaPushWordConstant(value)
                } else {
                    Op::EaPushShort(i16::from_le_bytes([lo, hi]))
                })
            }
            Shape::Word => {
                let value = read_word(self.apt, pos)?;
                Ok(if opcode == 0xB4 {
                    Op::EaPushFloat(f32::from_bits(value))
                } else {
                    Op::EaPushLong(value)
                })
            }
            Shape::DefineFn | Shape::DefineFn2 => {
                let is_function2 = shape == Shape::DefineFn2;
                let name_ptr = self.aligned_word(pos)?;
                let count = read_word(self.apt, pos)?;
                let flags = if is_function2 {
                    read_word(self.apt, pos)?
                } else {
                    0
                };
                let args_ptr = read_word(self.apt, pos)?;
                let size = read_word(self.apt, pos)?;
                let magic = (
                    u32_at(self.apt, *pos)?,
                    u32_at(self.apt, checked_add(*pos, 4)?)?,
                );
                *pos = checked_add(*pos, 8)?;

                let name = self.string_at(name_ptr)?;
                let params = self.params(args_ptr, count, is_function2)?;
                let body_start = *pos;
                let body_end = checked_add(body_start, size as usize)?;
                if body_end > self.apt.len() {
                    return Err(Error::OutOfBounds {
                        offset: body_start,
                        len: self.apt.len(),
                    });
                }
                let (body, landed) = if size == 0 {
                    (Vec::new(), body_start)
                } else {
                    #[allow(clippy::arithmetic_side_effects)]
                    self.decode_run(body_start, Some(body_end), depth + 1)?
                };
                if landed != body_end {
                    return Err(Error::FunctionBodyMisclose {
                        end: u32::try_from(body_end).unwrap_or(u32::MAX),
                        landed: u32::try_from(landed).unwrap_or(u32::MAX),
                    });
                }
                *pos = body_end;
                let interior_end = body
                    .iter()
                    .take(body.len().saturating_sub(1))
                    .any(|i| matches!(i.op, Op::End));
                let index = self.functions.len();
                self.functions.push(Function {
                    name,
                    params,
                    flags,
                    is_function2,
                    trailing_magic: magic,
                    body_start: u32::try_from(body_start).unwrap_or(u32::MAX),
                    body_size: size,
                    body_end_exact: true,
                    interior_end,
                    body,
                });
                Ok(if is_function2 {
                    Op::DefineFunction2(index)
                } else {
                    Op::DefineFunction(index)
                })
            }
        }
    }

    fn aligned_word(&self, pos: &mut usize) -> Result<u32> {
        let aligned = align4(*pos)?;
        *pos = aligned;
        read_word(self.apt, pos)
    }

    fn string_at(&self, ptr: u32) -> Result<String> {
        let offset = ptr as usize;
        let rest = self.apt.get(offset..).ok_or(Error::OutOfBounds {
            offset,
            len: self.apt.len(),
        })?;
        let window = rest.get(..self.limits.max_string_len).unwrap_or(rest);
        let nul = window
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::UnterminatedString { offset })?;
        let text = window.get(..nul).ok_or(Error::OutOfBounds {
            offset,
            len: self.apt.len(),
        })?;
        Ok(String::from_utf8_lossy(text).into_owned())
    }

    fn index_list(&self, count: u32, ptr: u32) -> Result<Vec<u32>> {
        let count = count as usize;
        if count > self.limits.max_list_len {
            return Err(Error::BytecodeLimit {
                what: "operand list length",
                limit: self.limits.max_list_len,
            });
        }
        let mut out = Vec::with_capacity(count.min(self.apt.len().saturating_div(4)));
        for i in 0..count {
            let at = checked_add(ptr as usize, checked_mul(i, 4)?)?;
            out.push(u32_at(self.apt, at)?);
        }
        Ok(out)
    }

    fn params(&self, ptr: u32, count: u32, is_function2: bool) -> Result<Vec<Param>> {
        let count = count as usize;
        if count > self.limits.max_list_len {
            return Err(Error::BytecodeLimit {
                what: "function argument count",
                limit: self.limits.max_list_len,
            });
        }
        let stride = if is_function2 { 8 } else { 4 };
        let mut out = Vec::with_capacity(count.min(self.apt.len()));
        for i in 0..count {
            let at = checked_add(ptr as usize, checked_mul(i, stride)?)?;
            let (register, name_ptr) = if is_function2 {
                (
                    u32_at(self.apt, at)?,
                    u32_at(self.apt, checked_add(at, 4)?)?,
                )
            } else {
                (0, u32_at(self.apt, at)?)
            };
            out.push(Param {
                register,
                name: self.string_at(name_ptr)?,
            });
        }
        Ok(out)
    }
}

fn align4(pos: usize) -> Result<usize> {
    let bumped = checked_add(pos, 3)?;
    Ok(bumped & !3usize)
}

fn read_word(apt: &[u8], pos: &mut usize) -> Result<u32> {
    let value = u32_at(apt, *pos)?;
    *pos = checked_add(*pos, 4)?;
    Ok(value)
}

fn bare_op(opcode: u8) -> Result<Op> {
    Ok(match opcode {
        0x00 => Op::End,
        0x04 => Op::NextFrame,
        0x05 => Op::PrevFrame,
        0x06 => Op::Play,
        0x07 => Op::Stop,
        0x08 => Op::ToggleQuality,
        0x09 => Op::StopSounds,
        0x0A => Op::Add,
        0x0B => Op::Subtract,
        0x0C => Op::Multiply,
        0x0D => Op::Divide,
        0x0E => Op::Equals,
        0x0F => Op::LessThan,
        0x10 => Op::LogicalAnd,
        0x11 => Op::LogicalOr,
        0x12 => Op::LogicalNot,
        0x13 => Op::StringEquals,
        0x14 => Op::StringLength,
        0x15 => Op::Substring,
        0x17 => Op::Pop,
        0x18 => Op::ToInteger,
        0x1C => Op::GetVariable,
        0x1D => Op::SetVariable,
        0x20 => Op::SetTargetExpression,
        0x21 => Op::StringConcat,
        0x22 => Op::GetProperty,
        0x23 => Op::SetProperty,
        0x24 => Op::DuplicateClip,
        0x25 => Op::RemoveClip,
        0x26 => Op::Trace,
        0x27 => Op::StartDragMovie,
        0x28 => Op::StopDragMovie,
        0x29 => Op::StringCompare,
        0x2A => Op::Throw,
        0x2B => Op::CastOp,
        0x2C => Op::ImplementsOp,
        0x30 => Op::Random,
        0x31 => Op::MbLength,
        0x32 => Op::Ord,
        0x33 => Op::Chr,
        0x34 => Op::GetTimer,
        0x35 => Op::MbSubstring,
        0x36 => Op::MbOrd,
        0x37 => Op::MbChr,
        0x3A => Op::Delete,
        0x3B => Op::Delete2,
        0x3C => Op::DefineLocal,
        0x3D => Op::CallFunction,
        0x3E => Op::Return,
        0x3F => Op::Modulo,
        0x40 => Op::New,
        0x41 => Op::Var,
        0x42 => Op::InitArray,
        0x43 => Op::InitObject,
        0x44 => Op::TypeOf,
        0x45 => Op::TargetPath,
        0x46 => Op::Enumerate,
        0x47 => Op::NewAdd,
        0x48 => Op::NewLessThan,
        0x49 => Op::NewEquals,
        0x4A => Op::ToNumber,
        0x4B => Op::ToString,
        0x4C => Op::Dup,
        0x4D => Op::Swap,
        0x4E => Op::GetMember,
        0x4F => Op::SetMember,
        0x50 => Op::Increment,
        0x51 => Op::Decrement,
        0x52 => Op::CallMethod,
        0x53 => Op::NewMethod,
        0x54 => Op::InstanceOf,
        0x55 => Op::Enumerate2,
        0x56 => Op::EaAction56,
        0x58 => Op::EaAction58,
        0x59 => Op::EaPushZero,
        0x5A => Op::EaPushOne,
        0x5B => Op::EaCallFunctionPop,
        0x5C => Op::EaCallFunction,
        0x5D => Op::EaCallMethodPop,
        0x5E => Op::EaCallMethod,
        0x60 => Op::BitwiseAnd,
        0x61 => Op::BitwiseOr,
        0x62 => Op::BitwiseXor,
        0x63 => Op::ShiftLeft,
        0x64 => Op::ShiftRight,
        0x65 => Op::ShiftRight2,
        0x66 => Op::StrictEquals,
        0x67 => Op::Greater,
        0x68 => Op::StringGreater,
        0x69 => Op::Extends,
        0x70 => Op::EaPushThis,
        0x71 => Op::EaPushGlobal,
        0x72 => Op::EaZeroVariable,
        0x73 => Op::EaPushTrue,
        0x74 => Op::EaPushFalse,
        0x75 => Op::EaPushNull,
        0x76 => Op::EaPushUndefined,
        0x77 => Op::EaAction77,
        0x8A => Op::WaitForFrame,
        0x8D => Op::WaitForFrameExpression,
        0x8F => Op::Try,
        0x9A => Op::GetUrl2,
        0x9E => Op::CallFrame,
        other => {
            return Err(Error::UnknownOpcode {
                offset: 0,
                opcode: other,
            })
        }
    })
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;

    fn buffer(prefix: usize, block: &[u8]) -> (Vec<u8>, u32) {
        let mut bytes = vec![0xEE; prefix];
        bytes.extend_from_slice(block);
        (bytes, prefix as u32)
    }

    #[test]
    fn a_bare_stop_block_decodes_to_stop_then_end() {
        let (apt, start) = buffer(3, &[0x07, 0x00]);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code.len(), 2);
        assert_eq!(block.code[0].op, Op::Stop);
        assert_eq!(block.code[1].op, Op::End);
        assert_eq!(block.end, start + 2);
    }

    #[test]
    fn a_word_operand_aligns_on_the_absolute_file_address() {
        let mut apt = vec![0xEE; 5];
        apt.push(0x81);
        apt.extend_from_slice(&[0xAA, 0xAA]);
        apt.extend_from_slice(&7u32.to_le_bytes());
        apt.push(0x00);
        let block = decode_block(&apt, 5, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code[0].op, Op::GotoFrame(7));
        assert_eq!(block.code[1].op, Op::End);
    }

    #[test]
    fn byte_operands_are_not_aligned() {
        let (apt, start) = buffer(1, &[0xB5, 5, 0xB5, 6, 0x00]);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code[0].op, Op::EaPushByte(5));
        assert_eq!(block.code[1].op, Op::EaPushByte(6));
        assert_eq!(block.code[2].op, Op::End);
    }

    #[test]
    fn push_byte_and_short_operands_are_signed() {
        let (apt, start) = buffer(1, &[0xB5, 0xFF, 0xB6, 0x37, 0xFF, 0x00]);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code[0].op, Op::EaPushByte(-1));
        assert_eq!(block.code[1].op, Op::EaPushShort(-201));
    }

    #[test]
    fn an_unknown_opcode_is_an_error_not_a_skip() {
        let (apt, start) = buffer(0, &[0x01, 0x00]);
        assert!(matches!(
            decode_block(&apt, start, &DecodeLimits::default()),
            Err(Error::UnknownOpcode { opcode: 0x01, .. })
        ));
    }

    fn define_function2_buffer(declared_size: u32) -> (Vec<u8>, u32) {
        define_function2_body(declared_size, &[0x07, 0x00])
    }

    fn define_function2_body(declared_size: u32, body: &[u8]) -> (Vec<u8>, u32) {
        let mut apt = Vec::new();
        apt.extend_from_slice(b"fn\0");
        apt.extend_from_slice(b"arg\0");
        while apt.len() < 16 {
            apt.push(0);
        }
        let args_at = apt.len() as u32;
        apt.extend_from_slice(&2u32.to_le_bytes());
        apt.extend_from_slice(&3u32.to_le_bytes());
        let start = apt.len() as u32;
        apt.push(0x8E);
        apt.extend_from_slice(&[0, 0, 0]);
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.extend_from_slice(&1u32.to_le_bytes());
        apt.extend_from_slice(&0x0040_0003u32.to_le_bytes());
        apt.extend_from_slice(&args_at.to_le_bytes());
        apt.extend_from_slice(&declared_size.to_le_bytes());
        apt.extend_from_slice(&0x9876_5432u32.to_le_bytes());
        apt.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        apt.extend_from_slice(body);
        apt.push(0x00);
        (apt, start)
    }

    #[test]
    fn define_function2_reads_its_header_arguments_and_inline_body() {
        let (apt, start) = define_function2_buffer(2);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code[0].op, Op::DefineFunction2(0));
        assert_eq!(block.code[1].op, Op::End);
        let f = &block.functions[0];
        assert_eq!(f.name, "fn");
        assert_eq!(
            f.params,
            vec![Param {
                register: 2,
                name: "arg".into()
            }]
        );
        assert_eq!(f.flags, 0x0040_0003);
        assert_eq!(f.swf_flags(), 0x40);
        assert_eq!(f.register_count(), 3);
        assert!(f.is_function2);
        assert_eq!(f.body_size, 2);
        assert_eq!(f.trailing_magic, (0x9876_5432, 0x1234_5678));
        assert!(f.body_end_exact);
        assert_eq!(f.body.len(), 2);
        assert_eq!(f.body[0].op, Op::Stop);
        assert_eq!(preload_register_base(f.swf_flags()), 2);
    }

    #[test]
    fn a_function_body_that_does_not_close_on_its_declared_size_is_an_error() {
        let (apt, start) = define_function2_body(1, &[0xB5, 0x05, 0x00]);
        assert!(matches!(
            decode_block(&apt, start, &DecodeLimits::default()),
            Err(Error::FunctionBodyMisclose { .. })
        ));
    }

    #[test]
    fn a_function_body_is_decoded_to_its_declared_size_not_its_first_end() {
        let mut apt = vec![0u8; 16];
        let start = 16u32;
        apt.push(0x9B);
        apt.extend_from_slice(&[0, 0, 0]);
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.extend_from_slice(&3u32.to_le_bytes());
        apt.extend_from_slice(&[0u8; 8]);
        apt.extend_from_slice(&[0x00, 0x07, 0x00]);
        apt.push(0x00);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        let f = &block.functions[0];
        assert_eq!(f.body.len(), 3);
        assert!(f.interior_end);
        assert_eq!(
            block.code.len(),
            2,
            "the enclosing block resumes after the body"
        );
    }

    #[test]
    fn constant_pool_reads_its_out_of_line_index_array() {
        let mut apt = Vec::new();
        apt.extend_from_slice(&11u32.to_le_bytes());
        apt.extend_from_slice(&22u32.to_le_bytes());
        let start = apt.len() as u32;
        apt.push(0x88);
        apt.extend_from_slice(&[0, 0, 0]);
        apt.extend_from_slice(&2u32.to_le_bytes());
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.push(0x00);
        let block = decode_block(&apt, start, &DecodeLimits::default()).unwrap();
        assert_eq!(block.code[0].op, Op::ConstantPool(vec![11, 22]));
    }

    #[test]
    fn a_huge_operand_list_count_errors_rather_than_allocating() {
        let mut apt = vec![0u8; 4];
        let start = 4u32;
        apt.push(0x88);
        apt.extend_from_slice(&[0, 0, 0]);
        apt.extend_from_slice(&u32::MAX.to_le_bytes());
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.push(0x00);
        assert!(decode_block(&apt, start, &DecodeLimits::default()).is_err());
    }

    #[test]
    fn a_block_with_no_end_hits_the_instruction_cap_rather_than_running_forever() {
        let apt = vec![0x07; 4096];
        let limits = DecodeLimits {
            max_instructions: 64,
            ..DecodeLimits::default()
        };
        assert!(matches!(
            decode_block(&apt, 0, &limits),
            Err(Error::BytecodeLimit { .. })
        ));
    }

    #[test]
    fn an_unterminated_string_operand_errors() {
        let mut apt = vec![0xAA; 8];
        let start = 8u32;
        apt.push(0xA1);
        apt.extend_from_slice(&[0, 0, 0]);
        apt.extend_from_slice(&0u32.to_le_bytes());
        apt.push(0x00);
        let limits = DecodeLimits {
            max_string_len: 4,
            ..DecodeLimits::default()
        };
        assert!(matches!(
            decode_block(&apt, start, &limits),
            Err(Error::UnterminatedString { .. })
        ));
    }

    #[test]
    fn every_prefix_of_a_valid_block_errors_rather_than_panicking() {
        let (apt, start) = define_function2_buffer(2);
        for len in 0..apt.len() {
            let _ = decode_block(&apt[..len], start, &DecodeLimits::default());
        }
    }

    #[test]
    fn corrupting_any_byte_errors_or_decodes_but_never_panics() {
        let (apt, start) = define_function2_buffer(2);
        for i in 0..apt.len() {
            for bit in 0..8 {
                let mut corrupt = apt.clone();
                corrupt[i] ^= 1 << bit;
                let _ = decode_block(&corrupt, start, &DecodeLimits::default());
            }
        }
    }

    #[test]
    fn opcode_round_trips_through_the_name_table() {
        for opcode in 0u8..=255 {
            if opcode_name(opcode).is_none() {
                continue;
            }
            if let Ok(op) = bare_op(opcode) {
                assert_eq!(op.opcode(), opcode, "bare opcode {opcode:#04x}");
            }
        }
    }

    #[test]
    fn every_named_opcode_has_a_shape() {
        for opcode in 0u8..=255 {
            assert_eq!(
                opcode_name(opcode).is_some(),
                shape(opcode).is_some(),
                "opcode {opcode:#04x}"
            );
        }
    }

    #[test]
    fn the_name_table_has_exactly_the_135_entries_aptconverter_lists() {
        let named = (0u8..=255).filter(|&o| opcode_name(o).is_some()).count();
        assert_eq!(named, 135);
        let ea = (0u8..=255)
            .filter(|&o| opcode_name(o).is_some_and(|n| n.starts_with("EA_")))
            .count();
        assert_eq!(ea, 35);
    }
}
