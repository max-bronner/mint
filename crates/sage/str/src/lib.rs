#![forbid(unsafe_code)]
#![deny(clippy::arithmetic_side_effects)]

mod csf;
mod placeholder;
mod str_file;
mod table;
mod text;

pub use csf::parse_csf;
pub use placeholder::{format_placeholder, Placeholder};
pub use str_file::parse_str;
pub use table::StringTable;
pub use text::decode_cp1252;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringRecord {
    pub label: String,
    pub text: String,
    pub speech: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    #[error(".str line {line}: label {label:?} has no END before end of file")]
    UnterminatedRecord { line: usize, label: String },

    #[error(".str line {line}: quoted string has no closing quote before end of file")]
    UnterminatedString { line: usize },

    #[error(".csf is {len} bytes, too short for the 24-byte header")]
    CsfHeaderTruncated { len: usize },

    #[error(".csf does not start with the expected \" FSC\" magic")]
    CsfBadMagic,

    #[error(".csf offset {offset:#x}: tag {tag:?} is not \" LBL\", \" RTS\" or \"WRTS\"")]
    CsfBadTag { offset: usize, tag: [u8; 4] },

    #[error(".csf offset {offset:#x}: {what} of {len} needs more than the {left} bytes remaining")]
    CsfTruncated {
        offset: usize,
        what: &'static str,
        len: usize,
        left: usize,
    },
}
