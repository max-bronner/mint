use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::container::{header_bytes, HEADER_LEN};
use crate::game_state::{GameStateBlock, CHUNK_GAME_STATE};
use crate::xfer::Reader;
use crate::{Result, SaveError, END_TOKEN};

const DEFAULT_NAME_LIMIT: u32 = 99_999_999;

pub fn read_info(bytes: &[u8]) -> Result<GameStateBlock> {
    let mut r = Reader::new(bytes, 0);
    let head = r.bytes(HEADER_LEN).map_err(|_| SaveError::BadHeader)?;
    if head != header_bytes() {
        return Err(SaveError::BadHeader);
    }
    loop {
        let name_len = r.u8()?;
        let name = r.bytes(usize::from(name_len))?;
        if name == END_TOKEN {
            return Err(SaveError::MissingBlock("CHUNK_GameState"));
        }
        let mut inner = r.nested()?;
        if name == CHUNK_GAME_STATE {
            let offset = inner.position();
            let payload = inner.bytes(inner.remaining())?;
            return GameStateBlock::decode_info(payload, offset);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveDate(pub [u16; 8]);

impl SaveDate {
    fn key(self) -> [u16; 7] {
        let [y, mo, d, _weekday, h, mi, s, ms] = self.0;
        [y, mo, d, h, mi, s, ms]
    }

    #[must_use]
    pub fn is_newer_than(self, other: Self) -> bool {
        self.key() > other.key()
    }
}

pub fn sort_newest_first<T>(items: &mut [T], date: impl Fn(&T) -> SaveDate) {
    items.sort_by(|a, b| {
        let (a, b) = (date(a), date(b));
        if a.is_newer_than(b) {
            Ordering::Less
        } else if b.is_newer_than(a) {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTab {
    Campaign,
    Skirmish,
    WotrSingle,
    WotrMulti,
}

impl SaveTab {
    #[must_use]
    pub fn extension_type(self) -> u32 {
        match self {
            Self::Campaign => 1,
            Self::Skirmish => 2,
            Self::WotrSingle => 4,
            Self::WotrMulti => 6,
        }
    }

    #[must_use]
    pub fn extension(self) -> &'static str {
        extension(self.extension_type())
    }

    #[must_use]
    pub fn mint_extension(self) -> &'static str {
        mint_extension(self.extension_type())
    }

    #[must_use]
    pub fn lists_file(self, file_name: &str) -> bool {
        let name = file_name.to_ascii_lowercase();
        [self.extension(), self.mint_extension()]
            .iter()
            .any(|ext| name.ends_with(&ext.to_ascii_lowercase()))
    }
}

#[must_use]
pub fn tab_accepts(tab: SaveTab, save_file_type: u32) -> bool {
    match tab {
        SaveTab::Campaign => matches!(save_file_type, 0 | 1 | 8),
        SaveTab::Skirmish => save_file_type == 2,
        SaveTab::WotrSingle => matches!(save_file_type, 3 | 4),
        SaveTab::WotrMulti => save_file_type == 6,
    }
}

#[must_use]
pub fn extension(save_type: u32) -> &'static str {
    match save_type {
        2 => ".BfME2Skirmish",
        3 | 4 => ".BfME2WotR",
        6 => ".BfME2WotRMP",
        _ => ".BfME2Campaign",
    }
}

#[must_use]
pub fn mint_extension(save_type: u32) -> &'static str {
    match save_type {
        2 => ".mintSkirmish",
        3 | 4 => ".mintWotR",
        6 => ".mintWotRMP",
        _ => ".mintCampaign",
    }
}

#[must_use]
pub fn has_retail_extension(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    [0, 2, 3, 6]
        .iter()
        .any(|&t| name.ends_with(&extension(t).to_ascii_lowercase()))
}

#[must_use]
pub fn is_scratch_map_name(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    [".map", ".wak", ".lws"].iter().any(|e| name.ends_with(e))
}

#[must_use]
pub fn default_save_name(format: &str, extension: &str, existing_files: &[&str]) -> String {
    let taken: BTreeSet<String> = existing_files.iter().map(|f| f.to_lowercase()).collect();
    let tries = u32::try_from(taken.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1)
        .min(DEFAULT_NAME_LIMIT.saturating_sub(1));
    for n in 1..=tries {
        let stem = format.replacen("%d", &n.to_string(), 1);
        if !taken.contains(&format!("{stem}{extension}").to_lowercase()) {
            return stem;
        }
    }
    "0".to_owned()
}
