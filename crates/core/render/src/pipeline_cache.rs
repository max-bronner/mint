use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{Error, Result};

const MAGIC: [u8; 8] = *b"mintPLC1";

const HEADER_LEN: usize = 16;

const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PipelineCacheState {
    Off,
    Empty,
    Loaded { bytes: usize },
    Ignored { reason: &'static str },
}

pub(crate) struct DiskCache {
    pub(crate) cache: wgpu::PipelineCache,
    path: PathBuf,
    on_disk: Option<(usize, u64)>,
}

pub(crate) fn open(
    device: &wgpu::Device,
    key: Option<&str>,
    dir: Option<&Path>,
) -> (Option<DiskCache>, PipelineCacheState) {
    let (Some(key), Some(dir)) = (key, dir) else {
        return (None, PipelineCacheState::Off);
    };
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) {
        return (None, PipelineCacheState::Off);
    }
    let path = dir.join(format!("{key}.bin"));
    let (payload, mut state) = match read_bounded(&path) {
        Ok(Some(file)) => match unseal(&file) {
            Ok(payload) => {
                let state = PipelineCacheState::Loaded {
                    bytes: payload.len(),
                };
                (Some(payload.to_vec()), state)
            }
            Err(reason) => (None, PipelineCacheState::Ignored { reason }),
        },
        Ok(None) => (None, PipelineCacheState::Empty),
        Err(reason) => (None, PipelineCacheState::Ignored { reason }),
    };
    let mut cache = payload
        .as_deref()
        .and_then(|data| create(device, Some(data)));
    if payload.is_some() && cache.is_none() {
        state = PipelineCacheState::Ignored {
            reason: "the driver refused it (made for another device)",
        };
    }
    if cache.is_none() {
        cache = create(device, None);
    }
    let on_disk = payload.as_deref().map(|data| (data.len(), checksum(data)));
    match cache {
        Some(cache) => (
            Some(DiskCache {
                cache,
                path,
                on_disk,
            }),
            state,
        ),
        None => (None, PipelineCacheState::Off),
    }
}

fn create(device: &wgpu::Device, data: Option<&[u8]>) -> Option<wgpu::PipelineCache> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    #[allow(unsafe_code)]
    let cache = unsafe {
        device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("mint pipeline cache"),
            data,
            fallback: true,
        })
    };
    let refused = crate::renderer::pop_scope(scope, device).ok().flatten();
    refused.is_none().then_some(cache)
}

impl DiskCache {
    pub(crate) fn save(&mut self) -> Result<bool> {
        let Some(data) = self.cache.get_data() else {
            return Ok(false);
        };
        let stamp = (data.len(), checksum(&data));
        if self.on_disk == Some(stamp) {
            return Ok(false);
        }
        let io = |e| Error::backend("saving the pipeline cache", e);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        let temporary = self.path.with_extension("tmp");
        std::fs::write(&temporary, seal(&data)).map_err(io)?;
        std::fs::rename(&temporary, &self.path).map_err(io)?;
        self.on_disk = Some(stamp);
        Ok(true)
    }
}

fn read_bounded(path: &Path) -> std::result::Result<Option<Vec<u8>>, &'static str> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("unreadable"),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable")?;
    if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_FILE_BYTES) {
        return Err("larger than any cache mint writes");
    }
    Ok(Some(bytes))
}

pub(crate) fn seal(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len().saturating_add(HEADER_LEN));
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&checksum(payload).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub(crate) fn unseal(file: &[u8]) -> std::result::Result<&[u8], &'static str> {
    let (header, payload) = file
        .split_at_checked(HEADER_LEN)
        .ok_or("truncated header")?;
    let field = |range: std::ops::Range<usize>| -> [u8; 8] {
        header
            .get(range)
            .and_then(|bytes| bytes.try_into().ok())
            .unwrap_or_default()
    };
    if field(0..8) != MAGIC {
        return Err("not a mint pipeline cache");
    }
    if u64::from_le_bytes(field(8..16)) != checksum(payload) {
        return Err("checksum does not match (damaged, truncated or extended)");
    }
    Ok(payload)
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_payload_opens_to_itself() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        assert_eq!(unseal(&seal(&payload)), Ok(payload.as_slice()));
        assert_eq!(unseal(&seal(&[])), Ok(&[][..]));
    }

    #[test]
    fn a_damaged_file_is_refused() {
        let payload: Vec<u8> = (0..200u8).collect();
        let sealed = seal(&payload);
        for cut in [0, 7, HEADER_LEN - 1, HEADER_LEN, sealed.len() - 1] {
            assert!(unseal(&sealed[..cut]).is_err(), "cut at {cut}");
        }
        let mut extended = sealed.clone();
        extended.push(0);
        assert!(unseal(&extended).is_err());
        let mut flipped = sealed.clone();
        flipped[HEADER_LEN + 100] ^= 1;
        assert!(unseal(&flipped).is_err());
        let mut foreign = sealed;
        foreign[0] = b'M';
        assert!(unseal(&foreign).is_err());
    }
}
