use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const ENVIRONMENT_CHUNK: &str = "EnvironmentData";

const ENVIRONMENT_MIN_VERSION: u16 = 2;

const ENVIRONMENT_MAX_VERSION: u16 = 3;

const WATER_ALPHA_FROM_VERSION: u16 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentData {
    pub version: u16,
    pub water_max_alpha_depth: Option<f32>,
    pub deep_water_alpha: Option<f32>,
    pub is_macro_texture_stretched: bool,
    pub macro_texture: String,
    pub cloud_texture: String,
}

pub(crate) fn read_environment(body: &[u8], chunk: &RawChunk) -> Result<EnvironmentData> {
    if chunk.version < ENVIRONMENT_MIN_VERSION || chunk.version > ENVIRONMENT_MAX_VERSION {
        return Err(Error::UnknownEnvironmentVersion {
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let (water_max_alpha_depth, deep_water_alpha) = if chunk.version >= WATER_ALPHA_FROM_VERSION {
        (Some(r.f32()?), Some(r.f32()?))
    } else {
        (None, None)
    };
    let is_macro_texture_stretched = match r.u8()? {
        0 => false,
        1 => true,
        _ => {
            return Err(Error::Malformed {
                name: ENVIRONMENT_CHUNK,
                reason: "the isMacroTextureStretched byte was neither 0 nor 1",
            })
        }
    };
    let macro_texture = r.ascii_string()?;
    let cloud_texture = r.ascii_string()?;

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: ENVIRONMENT_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(EnvironmentData {
        version: chunk.version,
        water_max_alpha_depth,
        deep_water_alpha,
        is_macro_texture_stretched,
        macro_texture,
        cloud_texture,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn chunk_at(len: usize, version: u16) -> RawChunk {
        RawChunk {
            name: ENVIRONMENT_CHUNK.to_owned(),
            version,
            body: 0,
            len,
        }
    }

    fn payload_v3(depth: f32, alpha: f32, stretched: u8, a: &str, b: &str) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&depth.to_le_bytes());
        out.extend_from_slice(&alpha.to_le_bytes());
        out.push(stretched);
        for name in [a, b] {
            let len = u16::try_from(name.len()).unwrap();
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        out
    }

    #[test]
    fn a_version_three_chunk_yields_both_water_fields() {
        let body = payload_v3(3.0, 0.75, 0, "TSNoiseUrb.tga", "TSCloudMed.tga");
        let env = read_environment(&body, &chunk_at(body.len(), 3)).unwrap();
        assert_eq!(env.water_max_alpha_depth, Some(3.0));
        assert_eq!(env.deep_water_alpha, Some(0.75));
        assert_eq!(env.macro_texture, "TSNoiseUrb.tga");
        assert_eq!(env.cloud_texture, "TSCloudMed.tga");
    }

    #[test]
    fn a_version_two_chunk_has_no_water_fields_rather_than_zeroed_ones() {
        let full = payload_v3(0.0, 0.0, 1, "a.tga", "b.tga");
        let body = full[8..].to_vec();
        let env = read_environment(&body, &chunk_at(body.len(), 2)).unwrap();
        assert_eq!(env.water_max_alpha_depth, None);
        assert!(env.is_macro_texture_stretched);
    }

    #[test]
    fn trailing_bytes_are_an_error_not_a_partial_decode() {
        let mut body = payload_v3(1.0, 1.0, 0, "a", "b");
        body.push(0);
        let err = read_environment(&body, &chunk_at(body.len(), 3)).unwrap_err();
        assert!(matches!(err, Error::TrailingBytes { .. }));
    }

    #[test]
    fn an_unknown_version_is_refused_rather_than_guessed_at() {
        let body = payload_v3(1.0, 1.0, 0, "a", "b");
        let err = read_environment(&body, &chunk_at(body.len(), 4)).unwrap_err();
        assert!(matches!(err, Error::UnknownEnvironmentVersion { .. }));
    }

    #[test]
    fn truncation_never_panics() {
        let body = payload_v3(1.0, 1.0, 0, "a.tga", "b.tga");
        for cut in 0..=body.len() {
            let _ = read_environment(&body[..cut], &chunk_at(cut, 3));
        }
    }
}
