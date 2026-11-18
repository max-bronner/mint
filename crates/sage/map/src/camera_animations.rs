use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const CAMERA_ANIMATIONS_CHUNK: &str = "CameraAnimationList";

const INTERPOLATION_VERSION: u16 = 2;
const START_OFFSET_VERSION: u16 = 3;
const FREE_KIND: &[u8; 4] = b"eerf";
const LOOK_KIND: &[u8; 4] = b"kool";
const STEP_TAG: &[u8; 4] = b"pets";
const LINEAR_TAG: &[u8; 4] = b"enil";
const MIN_ANIMATION_BYTES: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
    CatmullRom,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key<T> {
    pub frame: u32,
    pub interpolation: Interpolation,
    pub value: T,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeKey {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub lens: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LookKey {
    pub position: [f32; 3],
    pub roll: f32,
    pub lens: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CameraTrack {
    Free(Vec<Key<FreeKey>>),
    Look {
        camera: Vec<Key<LookKey>>,
        target: Vec<Key<[f32; 3]>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraAnimation {
    pub name: String,
    pub num_frames: u32,
    pub start_offset: u32,
    pub track: CameraTrack,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CameraAnimations {
    pub animations: Vec<CameraAnimation>,
    pub stopped_at_unknown_kind: bool,
}

fn key_bytes(version: u16, values: usize) -> usize {
    let tag = if version >= INTERPOLATION_VERSION {
        4
    } else {
        0
    };
    4_usize
        .saturating_add(tag)
        .saturating_add(values.saturating_mul(4))
}

fn read_key_header(r: &mut Reader<'_>, version: u16) -> Result<(u32, Interpolation)> {
    let frame = r.u32()?;
    if version < INTERPOLATION_VERSION {
        return Ok((frame, Interpolation::CatmullRom));
    }
    let interpolation = match r.take(4)? {
        t if t == STEP_TAG => Interpolation::Step,
        t if t == LINEAR_TAG => Interpolation::Linear,
        _ => Interpolation::CatmullRom,
    };
    Ok((frame, interpolation))
}

fn read_track<T>(
    r: &mut Reader<'_>,
    version: u16,
    values: usize,
    mut value: impl FnMut(&mut Reader<'_>) -> Result<T>,
) -> Result<Vec<Key<T>>> {
    let declared = r.u32()?;
    let count = usize::try_from(declared).unwrap_or(usize::MAX);
    let item_size = key_bytes(version, values);
    if count.saturating_mul(item_size) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: CAMERA_ANIMATIONS_CHUNK,
            offset: r.position(),
            count: declared,
            item_size,
            available: r.remaining(),
        });
    }
    let mut keys = Vec::new();
    for _ in 0..count {
        let (frame, interpolation) = read_key_header(r, version)?;
        keys.push(Key {
            frame,
            interpolation,
            value: value(r)?,
        });
    }
    Ok(keys)
}

fn floats<const N: usize>(r: &mut Reader<'_>) -> Result<[f32; N]> {
    let mut out = [0.0; N];
    for slot in &mut out {
        *slot = r.f32()?;
    }
    Ok(out)
}

pub(crate) fn read_camera_animations(body: &[u8], chunk: &RawChunk) -> Result<CameraAnimations> {
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let version = chunk.version;
    let declared = r.u32()?;
    let count = usize::try_from(declared).unwrap_or(usize::MAX);
    if count.saturating_mul(MIN_ANIMATION_BYTES) > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk: CAMERA_ANIMATIONS_CHUNK,
            offset: r.position(),
            count: declared,
            item_size: MIN_ANIMATION_BYTES,
            available: r.remaining(),
        });
    }
    let mut out = CameraAnimations::default();
    for _ in 0..count {
        let kind = r.take(4)?;
        let free = kind == FREE_KIND;
        if !free && kind != LOOK_KIND {
            out.stopped_at_unknown_kind = true;
            return Ok(out);
        }
        let name = r.ascii_string()?;
        let num_frames = r.u32()?;
        let start_offset = if version >= START_OFFSET_VERSION {
            r.u32()?
        } else {
            0
        };
        let track = if free {
            CameraTrack::Free(read_track(&mut r, version, 8, |r| {
                Ok(FreeKey {
                    position: floats(r)?,
                    rotation: floats(r)?,
                    lens: r.f32()?,
                })
            })?)
        } else {
            let camera = read_track(&mut r, version, 5, |r| {
                Ok(LookKey {
                    position: floats(r)?,
                    roll: r.f32()?,
                    lens: r.f32()?,
                })
            })?;
            let target = read_track(&mut r, version, 3, floats)?;
            CameraTrack::Look { camera, target }
        };
        out.animations.push(CameraAnimation {
            name,
            num_frames,
            start_offset,
            track,
        });
    }
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: CAMERA_ANIMATIONS_CHUNK,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&u16::try_from(s.len()).unwrap().to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn u32s(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.to_le_bytes());
    }

    fn f32s(out: &mut Vec<u8>, values: &[f32]) {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }

    fn key(out: &mut Vec<u8>, version: u16, frame: u32, tag: &[u8; 4], values: &[f32]) {
        u32s(out, frame);
        if version >= 2 {
            out.extend_from_slice(tag);
        }
        f32s(out, values);
    }

    fn sample(version: u16) -> Vec<u8> {
        let mut out = Vec::new();
        u32s(&mut out, 2);
        out.extend_from_slice(b"eerf");
        string(&mut out, "Flyby");
        u32s(&mut out, 90);
        if version >= 3 {
            u32s(&mut out, 7);
        }
        u32s(&mut out, 2);
        key(
            &mut out,
            version,
            0,
            b"mtac",
            &[1., 2., 3., 0., 0., 0., 1., 0.8],
        );
        key(
            &mut out,
            version,
            45,
            b"enil",
            &[4., 5., 6., 0., 0., 0., 1., 0.9],
        );
        out.extend_from_slice(b"kool");
        string(&mut out, "Orbit");
        u32s(&mut out, 30);
        if version >= 3 {
            u32s(&mut out, 0);
        }
        u32s(&mut out, 1);
        key(&mut out, version, 0, b"pets", &[1., 2., 3., 0.1, 0.7]);
        u32s(&mut out, 1);
        key(&mut out, version, 0, b"pets", &[9., 8., 7.]);
        out
    }

    fn read(bytes: &[u8], version: u16) -> Result<CameraAnimations> {
        let chunk = RawChunk {
            name: CAMERA_ANIMATIONS_CHUNK.into(),
            version,
            body: 0,
            len: bytes.len(),
        };
        read_camera_animations(bytes, &chunk)
    }

    #[test]
    fn each_version_frames_its_own_gates() {
        for version in [1, 2, 3] {
            let list = read(&sample(version), version).unwrap();
            assert_eq!(list.animations.len(), 2, "v{version}");
            assert!(!list.stopped_at_unknown_kind);
            assert_eq!(
                list.animations[0].start_offset,
                if version == 3 { 7 } else { 0 }
            );
            let CameraTrack::Free(keys) = &list.animations[0].track else {
                panic!("free expected");
            };
            let expected = match version {
                1 => [Interpolation::CatmullRom; 2],
                _ => [Interpolation::CatmullRom, Interpolation::Linear],
            };
            assert_eq!(
                keys.iter().map(|k| k.interpolation).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(keys[1].value.position, [4., 5., 6.]);
        }
    }

    #[test]
    fn look_tracks_read_camera_and_target_keys() {
        let list = read(&sample(3), 3).unwrap();
        let CameraTrack::Look { camera, target } = &list.animations[1].track else {
            panic!("look expected");
        };
        assert_eq!(camera[0].interpolation, Interpolation::Step);
        assert_eq!(camera[0].value.roll, 0.1);
        assert_eq!(target[0].value, [9., 8., 7.]);
    }

    #[test]
    fn an_unknown_kind_ends_the_list_with_what_was_read() {
        let mut bytes = sample(3);
        let second = bytes.windows(4).position(|w| w == b"kool").unwrap();
        bytes[second..second + 4].copy_from_slice(b"????");
        let list = read(&bytes, 3).unwrap();
        assert_eq!(list.animations.len(), 1);
        assert!(list.stopped_at_unknown_kind);
    }

    #[test]
    fn trailing_bytes_and_oversized_counts_are_refused() {
        let mut bytes = sample(3);
        bytes.push(0);
        assert!(matches!(read(&bytes, 3), Err(Error::TrailingBytes { .. })));
        let mut bytes = sample(3);
        bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read(&bytes, 3),
            Err(Error::CountExceedsPayload { .. })
        ));
    }

    #[test]
    fn truncation_errors_and_corruption_never_panics() {
        let bytes = sample(3);
        for len in 0..bytes.len() {
            assert!(read(&bytes[..len], 3).is_err(), "prefix {len}");
        }
        for at in 0..bytes.len() {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = bytes.clone();
                bad[at] ^= flip;
                let _ = read(&bad, 3);
            }
        }
    }
}
