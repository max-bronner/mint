use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use mint_audio::Clip;
use mint_vfs::Vfs;

pub const MIN_BUDGET_BYTES: usize = 256 * 1024 * 1024;

const BYTES_PER_SAMPLE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BankStats {
    pub clips: usize,
    pub bytes: usize,
    pub budget: usize,
    pub loaded: u64,
    pub hits: u64,
    pub missing: usize,
    pub undecodable: usize,
    pub evicted: u64,
}

impl BankStats {
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "bank {} clips, {:.1} MiB of {:.0} MiB | loaded {} hits {} evicted {} | \
             missing {} undecodable {}",
            self.clips,
            self.bytes as f64 / (1024.0 * 1024.0),
            self.budget as f64 / (1024.0 * 1024.0),
            self.loaded,
            self.hits,
            self.evicted,
            self.missing,
            self.undecodable,
        )
    }
}

#[derive(Debug, Default)]
pub struct SoundBank {
    clips: BTreeMap<String, Arc<Clip>>,
    order: Vec<String>,
    missing: BTreeSet<String>,
    undecodable: BTreeMap<String, String>,
    bytes: usize,
    budget: usize,
    loaded: u64,
    hits: u64,
    evicted: u64,
}

impl SoundBank {
    #[must_use]
    pub fn new(declared_budget: u64) -> Self {
        let declared = usize::try_from(declared_budget).unwrap_or(usize::MAX);
        Self {
            budget: declared.max(MIN_BUDGET_BYTES),
            ..Self::default()
        }
    }

    pub fn get_or_load(&mut self, vfs: &Vfs, path: &str) -> Option<Arc<Clip>> {
        if let Some(clip) = self.clips.get(path) {
            self.hits = self.hits.saturating_add(1);
            return Some(Arc::clone(clip));
        }
        if self.missing.contains(path) || self.undecodable.contains_key(path) {
            return None;
        }

        let Ok(bytes) = vfs.read(path) else {
            self.missing.insert(path.to_string());
            return None;
        };
        let decoded = if path.ends_with(".mp3") {
            mint_audio::decode_compressed(&bytes).map_err(|e| e.to_string())
        } else {
            Clip::from_wav(&bytes).map_err(|e| e.to_string())
        };
        let clip = match decoded {
            Ok(clip) => Arc::new(clip),
            Err(error) => {
                self.undecodable.insert(path.to_string(), error);
                return None;
            }
        };

        let size = clip_bytes(&clip);
        self.make_room(size);
        self.bytes = self.bytes.saturating_add(size);
        self.loaded = self.loaded.saturating_add(1);
        self.order.push(path.to_string());
        self.clips.insert(path.to_string(), Arc::clone(&clip));
        Some(clip)
    }

    pub fn missing_paths(&self) -> impl Iterator<Item = &str> {
        self.missing.iter().map(String::as_str)
    }

    pub fn undecodable_paths(&self) -> impl Iterator<Item = (&str, &str)> {
        self.undecodable
            .iter()
            .map(|(path, reason)| (path.as_str(), reason.as_str()))
    }

    #[must_use]
    pub fn stats(&self) -> BankStats {
        BankStats {
            clips: self.clips.len(),
            bytes: self.bytes,
            budget: self.budget,
            loaded: self.loaded,
            hits: self.hits,
            missing: self.missing.len(),
            undecodable: self.undecodable.len(),
            evicted: self.evicted,
        }
    }

    fn make_room(&mut self, wanted: usize) {
        if self.bytes.saturating_add(wanted) <= self.budget {
            return;
        }
        let mut kept: Vec<String> = Vec::with_capacity(self.order.len());
        let order = std::mem::take(&mut self.order);
        for path in order {
            if self.bytes.saturating_add(wanted) <= self.budget {
                kept.push(path);
                continue;
            }
            let evictable = self
                .clips
                .get(&path)
                .is_some_and(|clip| Arc::strong_count(clip) == 1);
            if evictable {
                if let Some(clip) = self.clips.remove(&path) {
                    self.bytes = self.bytes.saturating_sub(clip_bytes(&clip));
                    self.evicted = self.evicted.saturating_add(1);
                }
            } else {
                kept.push(path);
            }
        }
        self.order = kept;
    }
}

fn clip_bytes(clip: &Clip) -> usize {
    clip.frames()
        .saturating_mul(usize::from(clip.channels()))
        .saturating_mul(BYTES_PER_SAMPLE)
}
