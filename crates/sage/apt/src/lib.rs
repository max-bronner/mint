#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod button;
mod bytecode;
mod const_file;
mod dat;
mod display_list;
mod frame;
mod import;
mod movie;
mod reader;
mod ru;

pub use button::{hit_bounds, parse_button_actions, records_for_state, ButtonAction, ButtonState};
pub use bytecode::{
    decode_block, opcode_name, preload_register_base, Block, DecodeLimits, Function, Instruction,
    Op, Param,
};
pub use const_file::{parse_const, ConstFile, ConstItem};
pub use dat::{parse_dat, standalone_texture_path, DatEntry, TextureLocation};
pub use display_list::{
    accumulate_timeline, DisplayList, DisplayObject, FrameEvents, TimelineFrame,
};
pub use frame::{
    parse_clip_actions, parse_frames, ActionRef, BackgroundColor, ClipAction, Frame, FrameItem,
    FrameLabel, InitAction, PlaceObject, PlaceObjectFlags, Placement, RemoveObject,
};
pub use import::{resolve_import, ImportError, MovieSet, ResolvedImport};
pub use movie::{
    parse_characters, parse_exports, parse_imports, parse_movie, ru_path, shared_atlas_path,
    AptMovie, Button, ButtonRecord, Character, Export, Glyph, Import, Movie, Rect, Sprite,
    TextRecord, Transform, Vector2, CHARACTER_SIGNATURE,
};
pub use ru::{parse_ru, Entry, RuFile, Run, Style};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("offset arithmetic overflowed")]
    Overflow,

    #[error("read at offset {offset:#x} needs more than the {len} bytes present")]
    OutOfBounds { offset: usize, len: usize },

    #[error("string at offset {offset:#x} has no NUL terminator before end of file")]
    UnterminatedString { offset: usize },

    #[error(".const is {len} bytes, too short for the 20-byte header")]
    ConstHeaderTruncated { len: usize },

    #[error(".const does not start with the expected \"Apt constant file\" magic")]
    ConstBadMagic,

    #[error(".const item {index} has type {ty}, expected 1 (string) or 4 (number)")]
    ConstBadItemType { index: usize, ty: u32 },

    #[error("apt offset {offset:#x}: type {ty} (expected 9, the Movie record)")]
    NotAMovieRecord { offset: usize, ty: u32 },

    #[error("apt offset {offset:#x}: signature {signature:#010x} (expected 0x09876543)")]
    BadSignature { offset: usize, signature: u32 },

    #[error(".ru line {line}: {reason}")]
    RuSyntax { line: usize, reason: String },

    #[error("apt offset {offset:#x}: opcode {opcode:#04x} is outside the known instruction table")]
    UnknownOpcode { offset: u32, opcode: u8 },

    #[error("function body should end at {end:#x} but decoding landed at {landed:#x}")]
    FunctionBodyMisclose { end: u32, landed: u32 },

    #[error("bytecode limit reached: {what} (limit {limit})")]
    BytecodeLimit { what: &'static str, limit: usize },

    #[error(".dat line {line}: {reason}")]
    DatSyntax { line: usize, reason: String },
}
