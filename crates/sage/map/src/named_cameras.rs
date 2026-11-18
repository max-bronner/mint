use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const NAMED_CAMERAS_CHUNK: &str = "NamedCameras";

const ROLL_VERSION: u16 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct NamedCamera {
    pub name: String,
    pub look_at: [f32; 3],
    pub pitch: f32,
    pub roll: f32,
    pub yaw: f32,
    pub zoom: f32,
    pub fov: f32,
    pub ground: f32,
}

pub(crate) fn read_named_cameras(body: &[u8], chunk: &RawChunk) -> Result<Vec<NamedCamera>> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let has_roll = chunk.version >= ROLL_VERSION;
    let declared = r.u32()?;
    let item_size: usize = if has_roll { 38 } else { 34 };
    let wanted = usize::try_from(declared).unwrap_or(usize::MAX);
    if wanted.saturating_mul(item_size) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: NAMED_CAMERAS_CHUNK,
            offset: r.position(),
            count: declared,
            item_size,
            available: r.remaining(),
        });
    }
    let mut cameras = Vec::new();
    for _ in 0..wanted {
        let look_at = [r.f32()?, r.f32()?, r.f32()?];
        let name = r.ascii_string()?;
        let pitch = r.f32()?;
        let roll = if has_roll { r.f32()? } else { 0.0 };
        cameras.push(NamedCamera {
            name,
            look_at,
            pitch,
            roll,
            yaw: r.f32()?,
            zoom: r.f32()?,
            fov: r.f32()?,
            ground: r.f32()?,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: NAMED_CAMERAS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(cameras)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn payload(name: &str, with_roll: bool) -> Vec<u8> {
        let mut out = 1u32.to_le_bytes().to_vec();
        for v in [1.0f32, 2.0, 3.0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        let fields: &[f32] = if with_roll {
            &[4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        } else {
            &[4.0, 6.0, 7.0, 8.0, 9.0]
        };
        for v in fields {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    fn chunk(len: usize, version: u16) -> RawChunk {
        RawChunk {
            name: NAMED_CAMERAS_CHUNK.to_owned(),
            version,
            body: 0,
            len,
        }
    }

    #[test]
    fn every_field_comes_off_its_own_offset_and_roll_needs_version_two() {
        let v2 = payload("CamTest", true);
        let cams = read_named_cameras(&v2, &chunk(v2.len(), 2)).unwrap();
        assert_eq!(
            cams,
            vec![NamedCamera {
                name: "CamTest".to_owned(),
                look_at: [1.0, 2.0, 3.0],
                pitch: 4.0,
                roll: 5.0,
                yaw: 6.0,
                zoom: 7.0,
                fov: 8.0,
                ground: 9.0,
            }]
        );
        let v1 = payload("CamTest", false);
        let cams = read_named_cameras(&v1, &chunk(v1.len(), 1)).unwrap();
        assert_eq!((cams[0].roll, cams[0].yaw, cams[0].ground), (0.0, 6.0, 9.0));
    }

    #[test]
    fn truncated_or_overlong_payload_is_an_error() {
        let body = payload("CamTest", true);
        for cut in 0..body.len() {
            assert!(read_named_cameras(&body[..cut], &chunk(cut, 2)).is_err());
        }
        let mut long = body;
        long.push(0);
        assert!(matches!(
            read_named_cameras(&long, &chunk(long.len(), 2)),
            Err(Error::TrailingBytes { .. })
        ));
    }

    #[test]
    fn a_count_the_bytes_cannot_hold_allocates_nothing() {
        let body = u32::MAX.to_le_bytes().to_vec();
        assert!(matches!(
            read_named_cameras(&body, &chunk(4, 2)),
            Err(Error::CountExceedsPayload { .. })
        ));
    }
}
