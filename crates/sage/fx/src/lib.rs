#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

pub mod constants;
pub mod effect;
pub mod preshader;
mod read;
pub mod shader;
pub mod wgsl;

pub use effect::{Effect, Parameter, Pass, State, Technique};
pub use preshader::Preshader;
pub use shader::{Shader, Stage};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a D3DX9 effect: tag {tag:#010x}")]
    NotAnEffect { tag: u32 },
    #[error("not a D3D9 shader: version token {token:#010x}")]
    NotAShader { token: u32 },
    #[error("truncated at byte {offset}")]
    Truncated { offset: usize },
    #[error("count {count} at byte {offset} exceeds the data present")]
    CountTooLarge { count: u32, offset: usize },
    #[error("type nesting too deep at byte {offset}")]
    TooDeep { offset: usize },
    #[error("opcode {opcode} at byte {offset}: declared length disagrees with its operands")]
    LengthMismatch { opcode: u32, offset: usize },
    #[error("unsupported {what} {value} (at {offset})")]
    Unsupported {
        what: &'static str,
        value: u32,
        offset: usize,
    },
}

pub type Result<T> = core::result::Result<T, Error>;
