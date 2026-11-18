use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const POST_EFFECTS_CHUNK: &str = "PostEffectsChunk";

const POST_EFFECTS_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct PostEffect {
    pub name: String,
    pub blend_factor: f32,
    pub lookup_image: String,
}

pub(crate) fn read_post_effects(body: &[u8], chunk: &RawChunk) -> Result<Vec<PostEffect>> {
    if chunk.version != POST_EFFECTS_VERSION {
        return Err(Error::Malformed {
            name: POST_EFFECTS_CHUNK,
            reason: "a version other than 1",
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let count = r.u8()?;
    let mut effects = Vec::new();
    for _ in 0..count {
        let name = r.ascii_string()?;
        let blend_factor = r.f32()?;
        let lookup_image = r.ascii_string()?;
        effects.push(PostEffect {
            name,
            blend_factor,
            lookup_image,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: POST_EFFECTS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(effects)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn chunk(len: usize) -> RawChunk {
        RawChunk {
            name: POST_EFFECTS_CHUNK.to_owned(),
            version: 1,
            body: 0,
            len,
        }
    }

    fn string(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&u16::try_from(text.len()).unwrap().to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }

    #[test]
    fn one_effect_reads_exactly() {
        let mut body = vec![1u8];
        string(&mut body, "SomeEffect");
        body.extend_from_slice(&0.25f32.to_le_bytes());
        string(&mut body, "lut.tga");
        let effects = read_post_effects(&body, &chunk(body.len())).unwrap();
        assert_eq!(
            effects,
            vec![PostEffect {
                name: "SomeEffect".to_owned(),
                blend_factor: 0.25,
                lookup_image: "lut.tga".to_owned(),
            }]
        );
    }

    #[test]
    fn a_trailing_byte_is_an_error() {
        let body = vec![0u8, 7];
        assert!(read_post_effects(&body, &chunk(body.len())).is_err());
    }
}
