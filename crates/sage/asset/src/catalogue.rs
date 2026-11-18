use std::cmp::Ordering;
use std::path::Path;

use mint_bytes::{Cursor, Truncated};

use crate::{AssetKind, Error, Result};

const MAGIC: [u8; 4] = *b"ALAE";

pub const VERSION: u32 = 0x0000_0102;

const HEADER_LEN: usize = 16;

pub const MAX_FILE_LEN: u64 = 268_435_456;

const MIN_RECORD_LEN: u64 = 11;

const MIN_ENTRY_LEN: u64 = 13;

const MIN_DEPENDENCY_LEN: u64 = 4;

const MIN_REFERENCE_LEN: u64 = 1;

const MAX_RESERVE: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEntry {
    pub name: String,
    pub kind: AssetKind,
    pub offset: u32,
    pub size: u32,
}

impl AssetEntry {
    #[must_use]
    pub fn byte_range(&self) -> Option<std::ops::Range<u64>> {
        if self.offset == 0 && self.size == 0 {
            return None;
        }
        let start = u64::from(self.offset);
        Some(start..start.saturating_add(u64::from(self.size)))
    }

    pub fn slice<'a>(&self, file: &'a [u8]) -> Result<&'a [u8]> {
        let file_len = u64::try_from(file.len()).unwrap_or(u64::MAX);
        let Some(range) = self.byte_range() else {
            return Ok(file);
        };
        let start = usize::try_from(range.start).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end).unwrap_or(usize::MAX);
        file.get(start..end).ok_or_else(|| Error::EntryOutOfBounds {
            entry: self.name.clone(),
            offset: self.offset,
            end: range.end,
            file_len,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRecord {
    pub name: String,
    pub timestamp: u64,
    pub entries: Vec<AssetEntry>,
}

impl AssetRecord {
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&AssetEntry> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyRecord {
    pub file: String,
    pub sub_asset: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Catalogue {
    label: String,
    version: u32,
    records: Vec<AssetRecord>,
    by_name: Vec<usize>,
    dependencies: Vec<DependencyRecord>,
    by_owner: Vec<usize>,
    trailing: u64,
}

impl Catalogue {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let len = std::fs::metadata(path)
            .map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        if len > MAX_FILE_LEN {
            return Err(Error::FileTooLarge {
                path: path.to_path_buf(),
                len,
                limit: MAX_FILE_LEN,
            });
        }
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&bytes, &path.display().to_string())
    }

    pub fn parse(bytes: &[u8], label: &str) -> Result<Self> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::HeaderTruncated {
                catalogue: label.to_owned(),
                len: bytes.len(),
            });
        }

        let mut reader = Reader::new(bytes, label);
        let magic: [u8; 4] = reader
            .take(4, "the magic number")?
            .try_into()
            .unwrap_or([0; 4]);
        if magic != MAGIC {
            return Err(Error::NotCatalogue {
                catalogue: label.to_owned(),
                magic,
            });
        }

        let version = reader.u32("the version")?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion {
                catalogue: label.to_owned(),
                version,
                known: VERSION,
            });
        }

        let record_count_at = reader.offset();
        let record_count = reader.u32("the asset-record count")?;
        let dependency_count_at = reader.offset();
        let dependency_count = reader.u32("the dependency-record count")?;

        reader.check_count(
            u64::from(record_count),
            MIN_RECORD_LEN,
            "asset records",
            record_count_at,
        )?;
        let mut records =
            Vec::with_capacity(reader.reserve(u64::from(record_count), MIN_RECORD_LEN));
        for _ in 0..record_count {
            records.push(read_record(&mut reader)?);
        }

        reader.check_count(
            u64::from(dependency_count),
            MIN_DEPENDENCY_LEN,
            "dependency records",
            dependency_count_at,
        )?;
        let mut dependencies =
            Vec::with_capacity(reader.reserve(u64::from(dependency_count), MIN_DEPENDENCY_LEN));
        for _ in 0..dependency_count {
            dependencies.push(read_dependency(&mut reader)?);
        }

        let trailing = reader.remaining_u64();

        let by_name = sorted_index(records.len(), |a, b| {
            compare_ignoring_case(name_at(&records, a), name_at(&records, b))
        });
        let by_owner = sorted_index(dependencies.len(), |a, b| {
            owner_at(&dependencies, a).cmp_ignoring_case(&owner_at(&dependencies, b))
        });

        Ok(Self {
            label: label.to_owned(),
            version,
            records,
            by_name,
            dependencies,
            by_owner,
            trailing,
        })
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub fn records(&self) -> &[AssetRecord] {
        &self.records
    }

    #[must_use]
    pub fn dependency_records(&self) -> &[DependencyRecord] {
        &self.dependencies
    }

    #[must_use]
    pub fn trailing_bytes(&self) -> u64 {
        self.trailing
    }

    #[must_use]
    pub fn record(&self, name: &str) -> Option<&AssetRecord> {
        let found = last_matching(&self.by_name, |&index| {
            compare_ignoring_case(name_at(&self.records, index), name)
        })?;
        self.records.get(found)
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.record(name).is_some()
    }

    #[must_use]
    pub fn dependencies(&self, file: &str, sub_asset: &str) -> Option<&DependencyRecord> {
        let wanted = Owner { file, sub_asset };
        let found = last_matching(&self.by_owner, |&index| {
            owner_at(&self.dependencies, index).cmp_ignoring_case(&wanted)
        })?;
        self.dependencies.get(found)
    }

    pub fn check_references(&self) -> Result<()> {
        let mut known: Vec<String> = Vec::new();
        for record in &self.records {
            known.push(record.name.to_ascii_lowercase());
            for entry in &record.entries {
                known.push(entry.name.to_ascii_lowercase());
            }
        }
        known.sort_unstable();
        known.dedup();

        for dependency in &self.dependencies {
            let owner_exists = self
                .record(&dependency.file)
                .is_some_and(|record| record.entry(&dependency.sub_asset).is_some());
            if !owner_exists {
                return Err(Error::UnknownDependencyOwner {
                    catalogue: self.label.clone(),
                    file: dependency.file.clone(),
                    sub_asset: dependency.sub_asset.clone(),
                });
            }
            for reference in &dependency.references {
                let lowered = reference.to_ascii_lowercase();
                if known.binary_search(&lowered).is_err() {
                    return Err(Error::DanglingReference {
                        catalogue: self.label.clone(),
                        file: dependency.file.clone(),
                        sub_asset: dependency.sub_asset.clone(),
                        reference: reference.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

struct Owner<'a> {
    file: &'a str,
    sub_asset: &'a str,
}

impl Owner<'_> {
    fn cmp_ignoring_case(&self, other: &Owner<'_>) -> Ordering {
        compare_ignoring_case(self.file, other.file)
            .then_with(|| compare_ignoring_case(self.sub_asset, other.sub_asset))
    }
}

fn name_at(records: &[AssetRecord], index: usize) -> &str {
    records.get(index).map_or("", |record| record.name.as_str())
}

fn owner_at(dependencies: &[DependencyRecord], index: usize) -> Owner<'_> {
    dependencies.get(index).map_or(
        Owner {
            file: "",
            sub_asset: "",
        },
        |record| Owner {
            file: &record.file,
            sub_asset: &record.sub_asset,
        },
    )
}

fn compare_ignoring_case(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(b.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn sorted_index(len: usize, mut compare: impl FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut index: Vec<usize> = (0..len).collect();
    index.sort_by(|&a, &b| compare(a, b));
    index
}

fn last_matching(index: &[usize], mut compare: impl FnMut(&usize) -> Ordering) -> Option<usize> {
    let start = index.partition_point(|item| compare(item) == Ordering::Less);
    let mut found = None;
    for item in index.iter().skip(start) {
        if compare(item) == Ordering::Equal {
            found = Some(*item);
        } else {
            break;
        }
    }
    found
}

fn read_record(reader: &mut Reader<'_>) -> Result<AssetRecord> {
    let name = reader.name("an asset record's name")?;
    let low = reader.u32("an asset record's timestamp")?;
    let high = reader.u32("an asset record's timestamp")?;
    let timestamp = u64::from(high).wrapping_shl(32) | u64::from(low);

    let count_at = reader.offset();
    let count = reader.u16("an asset record's entry count")?;
    reader.check_count(u64::from(count), MIN_ENTRY_LEN, "entries", count_at)?;

    let mut entries = Vec::with_capacity(reader.reserve(u64::from(count), MIN_ENTRY_LEN));
    for _ in 0..count {
        let name = reader.name("an entry's name")?;
        let kind = AssetKind::from_tag(reader.u32("an entry's kind tag")?);
        let offset = reader.u32("an entry's offset")?;
        let size = reader.u32("an entry's size")?;
        entries.push(AssetEntry {
            name,
            kind,
            offset,
            size,
        });
    }

    Ok(AssetRecord {
        name,
        timestamp,
        entries,
    })
}

fn read_dependency(reader: &mut Reader<'_>) -> Result<DependencyRecord> {
    let file = reader.name("a dependency record's file name")?;
    let sub_asset = reader.name("a dependency record's sub-asset name")?;

    let count_at = reader.offset();
    let count = reader.u16("a dependency record's reference count")?;
    reader.check_count(u64::from(count), MIN_REFERENCE_LEN, "references", count_at)?;

    let mut references = Vec::with_capacity(reader.reserve(u64::from(count), MIN_REFERENCE_LEN));
    for _ in 0..count {
        references.push(reader.name("a referenced name")?);
    }

    Ok(DependencyRecord {
        file,
        sub_asset,
        references,
    })
}

struct Reader<'a> {
    cursor: Cursor<'a>,
    label: &'a str,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], label: &'a str) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            label,
        }
    }

    fn offset(&self) -> u64 {
        u64::try_from(self.cursor.position()).unwrap_or(u64::MAX)
    }

    fn remaining_u64(&self) -> u64 {
        u64::try_from(self.cursor.remaining()).unwrap_or(u64::MAX)
    }

    fn truncated(&self, e: Truncated, what: &'static str) -> Error {
        Error::Truncated {
            catalogue: self.label.to_owned(),
            what,
            offset: u64::try_from(e.offset).unwrap_or(u64::MAX),
            needed: u64::try_from(e.needed).unwrap_or(u64::MAX),
            available: u64::try_from(e.available).unwrap_or(u64::MAX),
        }
    }

    fn take(&mut self, len: usize, what: &'static str) -> Result<&'a [u8]> {
        match self.cursor.take(len) {
            Ok(slice) => Ok(slice),
            Err(e) => Err(self.truncated(e, what)),
        }
    }

    fn u8(&mut self, what: &'static str) -> Result<u8> {
        match self.cursor.u8() {
            Ok(value) => Ok(value),
            Err(e) => Err(self.truncated(e, what)),
        }
    }

    fn u16(&mut self, what: &'static str) -> Result<u16> {
        match self.cursor.u16() {
            Ok(value) => Ok(value),
            Err(e) => Err(self.truncated(e, what)),
        }
    }

    fn u32(&mut self, what: &'static str) -> Result<u32> {
        match self.cursor.u32() {
            Ok(value) => Ok(value),
            Err(e) => Err(self.truncated(e, what)),
        }
    }

    fn name(&mut self, what: &'static str) -> Result<String> {
        let start = self.offset();
        let len = self.u8(what)?;
        if len == 0 {
            return Err(Error::EmptyName {
                catalogue: self.label.to_owned(),
                what,
                offset: start,
            });
        }
        let bytes = self.take(usize::from(len), what)?;
        Ok(bytes.iter().map(|&byte| byte as char).collect())
    }

    fn check_count(
        &self,
        count: u64,
        min_item_len: u64,
        what: &'static str,
        offset: u64,
    ) -> Result<()> {
        let available = self.remaining_u64();
        let needed = count.saturating_mul(min_item_len);
        if needed > available {
            return Err(Error::CountTooLarge {
                catalogue: self.label.to_owned(),
                what,
                count,
                needed,
                available,
                offset,
            });
        }
        Ok(())
    }

    fn reserve(&self, count: u64, min_item_len: u64) -> usize {
        let by_bytes = self.remaining_u64().checked_div(min_item_len).unwrap_or(0);
        let believable = count.min(by_bytes).min(MAX_RESERVE);
        usize::try_from(believable).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
