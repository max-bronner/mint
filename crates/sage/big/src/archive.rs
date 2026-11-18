use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use mint_vfs::{Mount, VfsPath};

use crate::{Error, Result};

const HEADER_LEN: u64 = 16;

const MIN_ENTRY_LEN: u64 = 9;

const MAX_RESERVE: u64 = 4096;

const MAX_NAME_LEN: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BigFormat {
    Big4,
    BigF,
}

impl fmt::Display for BigFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Big4 => "BIG4",
            Self::BigF => "BIGF",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BigEntry {
    pub name: String,
    pub path: VfsPath,
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug)]
pub struct BigArchive {
    path: PathBuf,
    label: String,
    format: BigFormat,
    declared_size: u32,
    data_offset: u32,
    file_len: u64,
    table_end: u64,
    entries: Vec<BigEntry>,
    index: BTreeMap<VfsPath, usize>,
}

impl BigArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let file_len = file
            .metadata()
            .map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?
            .len();

        let mut reader = BufReader::new(file);
        let header = read_header(&mut reader, &path, file_len)?;

        let format = match &header.magic {
            b"BIG4" => BigFormat::Big4,
            b"BIGF" => BigFormat::BigF,
            magic => {
                return Err(Error::NotArchive {
                    path,
                    magic: *magic,
                })
            }
        };

        let available = file_len.saturating_sub(HEADER_LEN);
        let needed = u64::from(header.count).saturating_mul(MIN_ENTRY_LEN);
        if needed > available {
            return Err(Error::EntryCountTooLarge {
                path,
                count: header.count,
                needed,
                available,
            });
        }

        let (entries, table_end) = read_table(&mut reader, &path, &header, file_len)?;

        let mut index = BTreeMap::new();
        for (position, entry) in entries.iter().enumerate() {
            index.insert(entry.path.clone(), position);
        }

        Ok(Self {
            label: path.display().to_string(),
            path,
            format,
            declared_size: header.declared_size,
            data_offset: header.data_offset,
            file_len,
            table_end,
            entries,
            index,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn format(&self) -> BigFormat {
        self.format
    }

    #[must_use]
    pub fn declared_size(&self) -> u32 {
        self.declared_size
    }

    #[must_use]
    pub fn data_offset(&self) -> u32 {
        self.data_offset
    }

    #[must_use]
    pub fn entry_table_end(&self) -> u64 {
        self.table_end
    }

    #[must_use]
    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    #[must_use]
    pub fn entries(&self) -> &[BigEntry] {
        &self.entries
    }

    #[must_use]
    pub fn entry(&self, path: &VfsPath) -> Option<&BigEntry> {
        let position = *self.index.get(path)?;
        self.entries.get(position)
    }

    pub fn read_entry(&self, entry: &BigEntry) -> Result<Vec<u8>> {
        let end = u64::from(entry.offset).saturating_add(u64::from(entry.size));
        if end > self.file_len {
            return Err(Error::EntryOutOfBounds {
                path: self.path.clone(),
                name: entry.name.clone(),
                offset: entry.offset,
                end,
                file_len: self.file_len,
            });
        }

        let mut file = File::open(&self.path).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        file.seek(SeekFrom::Start(u64::from(entry.offset)))
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;

        let size = usize::try_from(entry.size).unwrap_or(usize::MAX);
        let mut bytes = vec![0u8; size];
        file.read_exact(&mut bytes).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(bytes)
    }

    pub fn read(&self, path: &VfsPath) -> Result<Vec<u8>> {
        let entry = self.entry(path).ok_or_else(|| Error::NotFound {
            path: self.path.clone(),
            name: path.to_string(),
        })?;
        self.read_entry(entry)
    }
}

impl Mount for BigArchive {
    fn label(&self) -> &str {
        &self.label
    }

    fn entries(&self) -> Vec<VfsPath> {
        self.index.keys().cloned().collect()
    }

    fn read(&self, path: &VfsPath) -> mint_vfs::Result<Vec<u8>> {
        let entry = self
            .entry(path)
            .ok_or_else(|| mint_vfs::Error::NotFound { path: path.clone() })?;
        self.read_entry(entry)
            .map_err(|source| mint_vfs::Error::Mount {
                mount: self.label.clone(),
                path: path.clone(),
                source: Box::new(source),
            })
    }
}

struct Header {
    magic: [u8; 4],
    declared_size: u32,
    count: u32,
    data_offset: u32,
}

fn read_header(reader: &mut impl Read, path: &Path, file_len: u64) -> Result<Header> {
    let mut bytes = [0u8; 16];
    reader.read_exact(&mut bytes).map_err(|source| {
        if source.kind() == ErrorKind::UnexpectedEof {
            Error::HeaderTruncated {
                path: path.to_path_buf(),
                len: file_len,
            }
        } else {
            Error::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;

    let field = |at: usize| -> [u8; 4] {
        at.checked_add(4)
            .and_then(|end| bytes.get(at..end))
            .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
            .unwrap_or([0; 4])
    };

    Ok(Header {
        magic: field(0),
        declared_size: u32::from_le_bytes(field(4)),
        count: u32::from_be_bytes(field(8)),
        data_offset: u32::from_be_bytes(field(12)),
    })
}

fn read_table(
    reader: &mut BufReader<File>,
    path: &Path,
    header: &Header,
    file_len: u64,
) -> Result<(Vec<BigEntry>, u64)> {
    let per_entry_max = file_len
        .saturating_sub(HEADER_LEN)
        .checked_div(MIN_ENTRY_LEN)
        .unwrap_or(0);
    let believable = u64::from(header.count).min(per_entry_max).min(MAX_RESERVE);
    let mut entries = Vec::with_capacity(usize::try_from(believable).unwrap_or(0));

    let mut position = HEADER_LEN;
    let mut name_bytes: Vec<u8> = Vec::with_capacity(64);

    for ordinal in 0..header.count {
        let mut fixed = [0u8; 8];
        reader.read_exact(&mut fixed).map_err(|source| {
            if source.kind() == ErrorKind::UnexpectedEof {
                Error::EntryTableTruncated {
                    path: path.to_path_buf(),
                    entry: ordinal,
                    count: header.count,
                    offset: position,
                }
            } else {
                Error::Io {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        position = position.saturating_add(8);

        let offset = u32::from_be_bytes(take4(&fixed, 0));
        let size = u32::from_be_bytes(take4(&fixed, 4));

        name_bytes.clear();
        let limit = u64::try_from(MAX_NAME_LEN)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let read = reader
            .by_ref()
            .take(limit)
            .read_until(0, &mut name_bytes)
            .map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
        let name_start = position;
        position = position.saturating_add(u64::try_from(read).unwrap_or(0));

        if name_bytes.last() != Some(&0) {
            let hit_limit = u64::try_from(read).unwrap_or(0) >= limit;
            return Err(if hit_limit {
                Error::UnterminatedName {
                    path: path.to_path_buf(),
                    entry: ordinal,
                    offset: name_start,
                    limit: MAX_NAME_LEN,
                }
            } else {
                Error::EntryTableTruncated {
                    path: path.to_path_buf(),
                    entry: ordinal,
                    count: header.count,
                    offset: position,
                }
            });
        }
        name_bytes.pop();

        if name_bytes.is_empty() {
            return Err(Error::EmptyName {
                path: path.to_path_buf(),
                entry: ordinal,
            });
        }

        let name: String = name_bytes.iter().map(|&byte| byte as char).collect();

        let end = u64::from(offset).saturating_add(u64::from(size));
        if end > file_len {
            return Err(Error::EntryOutOfBounds {
                path: path.to_path_buf(),
                name,
                offset,
                end,
                file_len,
            });
        }

        entries.push(BigEntry {
            path: VfsPath::new(&name),
            name,
            offset,
            size,
        });
    }

    Ok((entries, position))
}

fn take4(bytes: &[u8; 8], at: usize) -> [u8; 4] {
    at.checked_add(4)
        .and_then(|end| bytes.get(at..end))
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
        .unwrap_or([0; 4])
}

#[cfg(test)]
mod tests;
