use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const SKYBOX_CHUNK: &str = "SkyboxSettings";

const SKYBOX_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct SkyboxSettings {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub scale: f32,
    pub rotation: f32,
    pub texture_scheme: String,
}

pub(crate) fn read_skybox(body: &[u8], chunk: &RawChunk) -> Result<SkyboxSettings> {
    if chunk.version != SKYBOX_VERSION {
        return Err(Error::UnknownSkyboxVersion {
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let x = r.f32()?;
    let y = r.f32()?;
    let z = r.f32()?;
    let scale = r.f32()?;
    let rotation = r.f32()?;
    let texture_scheme = r.ascii_string()?;

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: SKYBOX_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(SkyboxSettings {
        x,
        y,
        z,
        scale,
        rotation,
        texture_scheme,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn chunk_at(len: usize, version: u16) -> RawChunk {
        RawChunk {
            name: SKYBOX_CHUNK.to_owned(),
            version,
            body: 0,
            len,
        }
    }

    fn payload(scheme: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for value in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        let len = u16::try_from(scheme.len()).unwrap();
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(scheme.as_bytes());
        out
    }

    #[test]
    fn every_field_comes_off_its_own_offset() {
        let body = payload("DefaultSky");
        let sky = read_skybox(&body, &chunk_at(body.len(), 1)).unwrap();
        assert_eq!(sky.x, 1.0);
        assert_eq!(sky.y, 2.0);
        assert_eq!(sky.z, 3.0);
        assert_eq!(sky.scale, 4.0);
        assert_eq!(sky.rotation, 5.0);
        assert_eq!(sky.texture_scheme, "DefaultSky");
    }

    #[test]
    fn trailing_bytes_are_an_error_not_a_partial_decode() {
        let mut body = payload("DefaultSky");
        body.push(0);
        let err = read_skybox(&body, &chunk_at(body.len(), 1)).unwrap_err();
        assert!(matches!(err, Error::TrailingBytes { .. }));
    }

    #[test]
    fn an_unknown_version_is_refused_rather_than_guessed_at() {
        let body = payload("DefaultSky");
        for version in [0u16, 2, u16::MAX] {
            let err = read_skybox(&body, &chunk_at(body.len(), version)).unwrap_err();
            assert!(
                matches!(err, Error::UnknownSkyboxVersion { .. }),
                "{version}"
            );
        }
    }

    #[test]
    fn truncation_never_panics() {
        let body = payload("DefaultSky");
        for cut in 0..body.len() {
            assert!(
                read_skybox(&body[..cut], &chunk_at(cut, 1)).is_err(),
                "prefix of {cut} bytes decoded as a whole chunk"
            );
        }
        assert!(read_skybox(&body, &chunk_at(body.len(), 1)).is_ok());
    }
}
