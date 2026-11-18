use mint_bytes::Truncated;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum SaveError {
    #[error(transparent)]
    Truncated(#[from] Truncated),
    #[error("not an untagged save file: bad header")]
    BadHeader,
    #[error("missing block marker at offset {offset}")]
    BadBlockMarker { offset: usize },
    #[error("block end {end} at offset {offset} is outside its parent")]
    BlockEnd { offset: usize, end: u32 },
    #[error("{count} bytes after the end token at offset {offset}")]
    TrailingBytes { offset: usize, count: usize },
    #[error("blocks nest deeper than {0}")]
    NestingTooDeep(usize),
    #[error("block name of {0} bytes is too long")]
    NameTooLong(usize),
    #[error("{what}: unsupported version {version}")]
    UnsupportedVersion { what: &'static str, version: u8 },
    #[error("bool of {value} at offset {offset}")]
    InvalidBool { offset: usize, value: u8 },
    #[error("non-canonical string length at offset {offset}")]
    NonCanonicalLength { offset: usize },
    #[error("{what}: {count} unread bytes at offset {offset}")]
    NotExact {
        what: &'static str,
        offset: usize,
        count: usize,
    },
    #[error("no {0} block before the end token")]
    MissingBlock(&'static str),
    #[error("{0}: in-game save, not a between-missions save")]
    NotBetweenMissions(&'static str),
}

pub type Result<T> = core::result::Result<T, SaveError>;
