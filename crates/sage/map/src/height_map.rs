use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Boundary {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct HeightMapData {
    pub width: u32,
    pub height: u32,
    pub border_size: u32,
    pub boundaries: Vec<Boundary>,
    pub samples: Vec<u16>,
}

impl HeightMapData {
    #[must_use]
    pub fn sample(&self, x: u32, y: u32) -> Option<u16> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = usize::try_from(y)
            .ok()?
            .checked_mul(usize::try_from(self.width).ok()?)?;
        let index = index.checked_add(usize::try_from(x).ok()?)?;
        self.samples.get(index).copied()
    }
}

pub(crate) fn read_height_map(body: &[u8], chunk: &RawChunk) -> Result<HeightMapData> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let width = r.u32()?;
    let height = r.u32()?;
    let border_size = r.u32()?;

    let boundary_count = r.u32()?;
    let mut boundaries = Vec::new();
    for _ in 0..boundary_count {
        let x = r.i32()?;
        let y = r.i32()?;
        boundaries.push(Boundary { x, y });
    }

    let sample_count = r.u32()?;
    let expected = width.checked_mul(height);
    if expected != Some(sample_count) {
        return Err(Error::HeightMapMismatch {
            declared_samples: sample_count,
            width,
            height,
        });
    }

    let mut samples = Vec::new();
    for _ in 0..sample_count {
        samples.push(r.u16()?);
    }

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: "HeightMapData",
            offset: r.position(),
            remaining: r.remaining(),
        });
    }

    Ok(HeightMapData {
        width,
        height,
        border_size,
        boundaries,
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::{height_map_payload, MapBuilder};

    fn one_height_map_chunk(payload: &[u8]) -> (Vec<u8>, RawChunk) {
        let mut b = MapBuilder::new();
        b.chunk("HeightMapData", 5, payload);
        let body = b.build();
        let (_names, mut chunks) = parse_top_level(&body).expect("valid container");
        (body, chunks.remove(0))
    }

    #[test]
    fn boundary_is_read_not_derived() {
        let payload = height_map_payload(10, 10, 1, &[(3, 4)], &[0u16; 100]);
        let (body, chunk) = one_height_map_chunk(&payload);

        let hm = read_height_map(&body, &chunk).expect("valid heightmap");
        assert_eq!(hm.boundaries, vec![Boundary { x: 3, y: 4 }]);
        assert_ne!(
            hm.boundaries.first().copied(),
            Some(Boundary { x: 8, y: 8 })
        );
    }

    #[test]
    fn sample_count_mismatch_is_an_error() {
        let payload = height_map_payload(2, 2, 0, &[], &[0u16; 3]);
        let (body, chunk) = one_height_map_chunk(&payload);

        let err = read_height_map(&body, &chunk).unwrap_err();
        assert!(matches!(err, Error::HeightMapMismatch { .. }));
    }

    #[test]
    fn sample_accessor_is_bounds_checked() {
        let payload = height_map_payload(2, 2, 0, &[], &[1, 2, 3, 4]);
        let (body, chunk) = one_height_map_chunk(&payload);

        let hm = read_height_map(&body, &chunk).expect("valid");
        assert_eq!(hm.sample(1, 0), Some(2));
        assert_eq!(hm.sample(0, 1), Some(3));
        assert_eq!(hm.sample(2, 0), None);
        assert_eq!(hm.sample(0, 2), None);
    }
}
