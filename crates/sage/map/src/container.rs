use std::collections::HashMap;

use crate::reader::Reader;
use crate::{Error, Result};

const CHUNK_HEADER_LEN: usize = 10;

#[derive(Debug)]
pub(crate) struct NameTable {
    names: HashMap<u32, String>,
}

impl NameTable {
    pub(crate) fn parse(r: &mut Reader<'_>) -> Result<Self> {
        let count = r.u32()?;
        let mut names = HashMap::new();
        for _ in 0..count {
            let len = usize::from(r.u8()?);
            let raw = r.take(len)?;
            let name: String = raw.iter().map(|&b| char::from(b)).collect();
            let index = r.u32()?;
            names.insert(index, name);
        }
        Ok(Self { names })
    }

    pub(crate) fn resolve(&self, index: u32, offset: usize) -> Result<String> {
        self.names
            .get(&index)
            .cloned()
            .ok_or(Error::UnknownNameIndex { offset, index })
    }

    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: impl IntoIterator<Item = (u32, String)>) -> Self {
        Self {
            names: pairs.into_iter().collect(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct RawChunk {
    pub(crate) name: String,
    pub(crate) version: u16,
    pub(crate) body: usize,
    pub(crate) len: usize,
}

impl RawChunk {
    pub(crate) fn end(&self) -> Result<usize> {
        self.body.checked_add(self.len).ok_or(Error::ChunkOverrun {
            name: self.name.clone(),
            offset: self.body,
            declared: self.len,
            available: 0,
        })
    }

    pub(crate) fn payload<'a>(&self, body_bytes: &'a [u8]) -> Result<&'a [u8]> {
        let end = self.end()?;
        let available = body_bytes
            .len()
            .saturating_sub(self.body.min(body_bytes.len()));
        body_bytes.get(self.body..end).ok_or(Error::ChunkOverrun {
            name: self.name.clone(),
            offset: self.body,
            declared: self.len,
            available,
        })
    }
}

const MAGIC: &[u8; 4] = b"CkMp";

pub(crate) fn parse_top_level(body: &[u8]) -> Result<(NameTable, Vec<RawChunk>)> {
    let mut r = Reader::new(body);
    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(Error::BadMagic);
    }
    let names = NameTable::parse(&mut r)?;
    let start = r.position();
    let chunks = read_chunks(body, start, body.len(), &names)?;
    Ok((names, chunks))
}

pub(crate) fn read_chunks(
    body: &[u8],
    start: usize,
    end: usize,
    names: &NameTable,
) -> Result<Vec<RawChunk>> {
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        let region = body.get(at..end).ok_or(Error::UnexpectedEof {
            offset: at,
            needed: CHUNK_HEADER_LEN,
            available: 0,
        })?;
        let mut r = Reader::new_at(region, at);
        let index = r.u32()?;
        let version = r.u16()?;
        let len = r.u32()?;
        let name = names.resolve(index, at)?;
        let header_len = r.position().saturating_sub(at);
        let chunk_body = at.saturating_add(header_len);
        let len = usize::try_from(len).unwrap_or(usize::MAX);
        let chunk = RawChunk {
            name,
            version,
            body: chunk_body,
            len,
        };
        let next = chunk.end()?;
        if next > end {
            return Err(Error::ChunkOverrun {
                name: chunk.name,
                offset: chunk.body,
                declared: chunk.len,
                available: end.saturating_sub(chunk.body),
            });
        }
        at = next;
        out.push(chunk);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::MapBuilder;

    #[test]
    fn walks_flat_top_level_chunks() {
        let mut b = MapBuilder::new();
        b.chunk("WorldInfo", 1, &[0, 0]);
        b.chunk("HeightMapData", 5, &[]);
        let body = b.build();

        let mut r = Reader::new(&body);
        let _magic = r.take(4).expect("magic");
        let names = NameTable::parse(&mut r).expect("name table");
        let chunks = read_chunks(&body, r.position(), body.len(), &names).expect("walk");

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].name, "WorldInfo");
        assert_eq!(chunks[1].name, "HeightMapData");
    }

    #[test]
    fn unknown_name_index_is_an_error_not_a_panic() {
        let mut body = Vec::new();
        body.extend_from_slice(&99u32.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        let names = NameTable {
            names: HashMap::new(),
        };
        let err = read_chunks(&body, 0, body.len(), &names).unwrap_err();
        assert!(matches!(err, Error::UnknownNameIndex { .. }));
    }

    #[test]
    fn overrunning_length_is_an_error_not_a_read_past_end() {
        let mut body = Vec::new();
        body.extend_from_slice(&1u32.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let mut names = HashMap::new();
        names.insert(1, "X".to_string());
        let names = NameTable { names };
        let err = read_chunks(&body, 0, body.len(), &names).unwrap_err();
        assert!(matches!(err, Error::ChunkOverrun { .. }));
    }
}
