use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const TRIGGER_AREAS_CHUNK: &str = "TriggerAreas";

#[derive(Debug, Clone, PartialEq)]
pub struct TriggerArea {
    pub name: String,
    pub layer_name: String,
    pub id: u32,
    pub points: Vec<(f32, f32)>,
}

pub(crate) fn read_trigger_areas(body: &[u8], chunk: &RawChunk) -> Result<Vec<TriggerArea>> {
    if chunk.version != 1 {
        return Err(Error::UnknownTriggerAreaVersion {
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let count = r.u32()?;
    let mut areas = Vec::new();
    for _ in 0..count {
        let name = r.ascii_string()?;
        let layer_name = r.ascii_string()?;
        let id = r.u32()?;
        let point_count = r.u32()?;
        let mut points = Vec::new();
        for _ in 0..point_count {
            let x = r.f32()?;
            let y = r.f32()?;
            points.push((x, y));
        }
        let _trailing = r.u32()?;
        areas.push(TriggerArea {
            name,
            layer_name,
            id,
            points,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: TRIGGER_AREAS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(areas)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ascii(field: &str) -> Vec<u8> {
        let mut out = u16::try_from(field.len()).unwrap().to_le_bytes().to_vec();
        out.extend_from_slice(field.as_bytes());
        out
    }

    fn two_areas() -> Vec<u8> {
        let mut payload = 2u32.to_le_bytes().to_vec();
        for (name, points) in [
            (
                "AREA - Reveal",
                vec![(0.0f32, 0.0), (10.0, 0.0), (0.0, 10.0)],
            ),
            ("Second", vec![(5.0, 5.0), (6.0, 6.0)]),
        ] {
            payload.extend_from_slice(&ascii(name));
            payload.extend_from_slice(&ascii(""));
            payload.extend_from_slice(&7u32.to_le_bytes());
            payload.extend_from_slice(&u32::try_from(points.len()).unwrap().to_le_bytes());
            for (x, y) in points {
                let (x, y): (f32, f32) = (x, y);
                payload.extend_from_slice(&x.to_le_bytes());
                payload.extend_from_slice(&y.to_le_bytes());
            }
            payload.extend_from_slice(&0u32.to_le_bytes());
        }
        payload
    }

    fn chunk(len: usize) -> RawChunk {
        RawChunk {
            name: TRIGGER_AREAS_CHUNK.into(),
            version: 1,
            body: 0,
            len,
        }
    }

    #[test]
    fn each_area_reads_its_name_and_polygon() {
        let payload = two_areas();
        let areas = read_trigger_areas(&payload, &chunk(payload.len())).unwrap();
        assert_eq!(areas.len(), 2);
        assert_eq!(areas[0].name, "AREA - Reveal");
        assert_eq!(areas[0].points, vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]);
        assert_eq!(areas[1].points, vec![(5.0, 5.0), (6.0, 6.0)]);
    }

    #[test]
    fn a_version_other_than_1_is_refused_rather_than_guessed_at() {
        let payload = two_areas();
        let mut bad = chunk(payload.len());
        bad.version = 2;
        assert!(matches!(
            read_trigger_areas(&payload, &bad),
            Err(Error::UnknownTriggerAreaVersion { version: 2 })
        ));
    }

    #[test]
    fn truncation_errors_rather_than_panicking() {
        let payload = two_areas();
        for len in 0..payload.len() {
            assert!(
                read_trigger_areas(&payload[..len], &chunk(len)).is_err(),
                "prefix {len}"
            );
        }
    }

    #[test]
    fn corruption_never_panics() {
        let payload = two_areas();
        for at in 0..payload.len() {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = payload.clone();
                bad[at] ^= flip;
                let _ = read_trigger_areas(&bad, &chunk(bad.len()));
            }
        }
    }
}
