use crate::reader::Reader;
use crate::{Array, ChunkType, Element, Error, Result};

const HEADER_SIZE: usize = 8;

const HAS_SUBCHUNKS: u32 = 0x8000_0000;

pub const MAX_CHUNK_DEPTH: u32 = 32;

#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    kind: ChunkType,
    has_children: bool,
    payload: &'a [u8],
    offset: usize,
    depth: u32,
}

impl<'a> Chunk<'a> {
    #[must_use]
    pub fn kind(&self) -> ChunkType {
        self.kind
    }

    #[must_use]
    pub fn has_children(&self) -> bool {
        self.has_children
    }

    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }

    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    #[must_use]
    pub fn depth(&self) -> u32 {
        self.depth
    }

    #[must_use]
    pub fn reader(&self) -> Reader<'a> {
        Reader::new(self.payload)
    }

    pub fn children(&self) -> Result<Chunks<'a>> {
        if !self.has_children {
            return Ok(Chunks::empty());
        }
        let depth = self.depth.saturating_add(1);
        if depth >= MAX_CHUNK_DEPTH {
            return Err(Error::TooDeep {
                offset: self.offset,
                max: MAX_CHUNK_DEPTH,
            });
        }
        Ok(Chunks {
            data: self.payload,
            pos: 0,
            base: self.offset.saturating_add(HEADER_SIZE),
            depth,
            done: false,
        })
    }

    pub fn array<T: Element>(&self, what: &'static str) -> Result<Array<'a, T>> {
        Array::new(what, self.payload)
    }

    #[must_use]
    pub fn text(&self) -> String {
        crate::reader::decode_name(self.payload)
    }
}

#[derive(Debug, Clone)]
pub struct Chunks<'a> {
    data: &'a [u8],
    pos: usize,
    base: usize,
    depth: u32,
    done: bool,
}

impl<'a> Chunks<'a> {
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            base: 0,
            depth: 0,
            done: false,
        }
    }

    fn empty() -> Self {
        Self {
            data: &[],
            pos: 0,
            base: 0,
            depth: 0,
            done: true,
        }
    }

    fn read_next(&mut self) -> Option<Result<Chunk<'a>>> {
        let remaining = self.data.len().saturating_sub(self.pos);
        if remaining == 0 {
            return None;
        }
        let header_offset = self.base.saturating_add(self.pos);
        let short_header = || {
            Some(Err(Error::UnexpectedEof {
                offset: header_offset,
                needed: HEADER_SIZE,
                available: remaining,
            }))
        };

        let mut reader = Reader::new(self.data.get(self.pos..).unwrap_or_default());
        let (Ok(raw_kind), Ok(raw_size)) = (reader.u32(), reader.u32()) else {
            return short_header();
        };
        let kind = ChunkType(raw_kind);

        let has_children = raw_size & HAS_SUBCHUNKS != 0;
        let masked = raw_size & !HAS_SUBCHUNKS;
        let overrun = || {
            Some(Err(Error::ChunkOverrun {
                kind,
                offset: header_offset,
                declared: usize::try_from(masked).unwrap_or(usize::MAX),
                available: remaining.saturating_sub(HEADER_SIZE),
            }))
        };
        let Ok(size) = usize::try_from(masked) else {
            return overrun();
        };

        let body_start = self.pos.saturating_add(HEADER_SIZE);
        let Some(body_end) = body_start.checked_add(size) else {
            return overrun();
        };
        let Some(payload) = self.data.get(body_start..body_end) else {
            return overrun();
        };

        self.pos = body_end;
        Some(Ok(Chunk {
            kind,
            has_children,
            payload,
            offset: header_offset,
            depth: self.depth,
        }))
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let item = self.read_next();
        if matches!(item, None | Some(Err(_))) {
            self.done = true;
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::chunk;

    fn descend(c: &Chunk<'_>) -> Result<u32> {
        let mut deepest = c.depth();
        for kid in c.children()? {
            deepest = deepest.max(descend(&kid?)?);
        }
        Ok(deepest)
    }

    #[test]
    fn walks_siblings_and_children() {
        let leaf = chunk(ChunkType::VERTICES, false, &[1, 2, 3, 4]);
        let container = chunk(ChunkType::MESH, true, &leaf);
        let mut file = container.clone();
        file.extend_from_slice(&chunk(ChunkType::HLOD, false, &[]));

        let top: Vec<_> = Chunks::new(&file)
            .collect::<Result<Vec<_>>>()
            .expect("valid");
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].kind(), ChunkType::MESH);
        assert!(top[0].has_children());
        assert_eq!(top[1].kind(), ChunkType::HLOD);

        let kids: Vec<_> = top[0]
            .children()
            .expect("depth ok")
            .collect::<Result<Vec<_>>>()
            .expect("valid");
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].kind(), ChunkType::VERTICES);
        assert_eq!(kids[0].payload(), &[1, 2, 3, 4]);
        assert_eq!(kids[0].depth(), 1);
        assert_eq!(kids[0].offset(), 8);
    }

    #[test]
    fn a_size_past_eof_is_an_error_not_a_read() {
        let mut file = Vec::new();
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
        file.extend_from_slice(&[0u8; 16]);
        let err = Chunks::new(&file).next().and_then(Result::err);
        assert!(matches!(err, Some(Error::ChunkOverrun { .. })), "{err:?}");
    }

    #[test]
    fn truncated_header_is_an_error() {
        for len in 1..HEADER_SIZE {
            let file = vec![0u8; len];
            let err = Chunks::new(&file).next().and_then(Result::err);
            assert!(matches!(err, Some(Error::UnexpectedEof { .. })), "{err:?}");
        }
    }

    #[test]
    fn zero_length_chunks_terminate() {
        let mut file = Vec::new();
        for _ in 0..1000 {
            file.extend_from_slice(&chunk(ChunkType::VERTICES, false, &[]));
        }
        assert_eq!(Chunks::new(&file).count(), 1000);
    }

    #[test]
    fn a_leaf_marked_as_a_container_yields_an_error_not_a_panic() {
        let file = chunk(ChunkType::VERTICES, true, &[0xAA; 13]);
        let top = Chunks::new(&file)
            .next()
            .and_then(Result::ok)
            .expect("outer header is well formed");
        assert!(top.has_children());
        let kids: Result<Vec<_>> = top.children().expect("depth ok").collect();
        assert!(kids.is_err(), "{kids:?}");
    }

    #[test]
    fn a_subchunk_larger_than_its_parent_is_an_error() {
        let mut inner = Vec::new();
        inner.extend_from_slice(&ChunkType::VERTICES.0.to_le_bytes());
        inner.extend_from_slice(&64u32.to_le_bytes());
        inner.extend_from_slice(&[0u8; 8]);
        let file = chunk(ChunkType::MESH, true, &inner);

        let top = Chunks::new(&file)
            .next()
            .and_then(Result::ok)
            .expect("outer header is well formed");
        let kids: Result<Vec<_>> = top.children().expect("depth ok").collect();
        assert!(
            matches!(kids, Err(Error::ChunkOverrun { .. })),
            "{kids:?}, a child may not out-size its parent"
        );
    }

    #[test]
    fn depth_bomb_is_refused() {
        let mut payload = Vec::new();
        for _ in 0..MAX_CHUNK_DEPTH.saturating_add(8) {
            payload = chunk(ChunkType::MESH, true, &payload);
        }
        let top = Chunks::new(&payload)
            .next()
            .and_then(Result::ok)
            .expect("outermost chunk parses");
        assert!(matches!(descend(&top), Err(Error::TooDeep { .. })));
    }

    #[test]
    fn nesting_just_under_the_cap_is_accepted() {
        let mut payload = chunk(ChunkType::VERTICES, false, &[]);
        for _ in 0..MAX_CHUNK_DEPTH.saturating_sub(1) {
            payload = chunk(ChunkType::MESH, true, &payload);
        }
        let top = Chunks::new(&payload)
            .next()
            .and_then(Result::ok)
            .expect("outermost chunk parses");
        assert_eq!(descend(&top), Ok(MAX_CHUNK_DEPTH - 1));
    }

    #[test]
    fn iteration_stops_after_the_first_bad_header() {
        let mut file = chunk(ChunkType::VERTICES, false, &[0; 4]);
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0x0000_FFFFu32.to_le_bytes());
        file.extend_from_slice(&chunk(ChunkType::VERTICES, false, &[]));
        let results: Vec<_> = Chunks::new(&file).collect();
        assert_eq!(results.len(), 2);
        assert!(results.last().is_some_and(Result::is_err));
    }

    #[test]
    fn empty_input_yields_no_chunks() {
        assert_eq!(Chunks::new(&[]).count(), 0);
    }
}
