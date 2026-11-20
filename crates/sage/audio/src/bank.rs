use std::collections::{BTreeMap, BTreeSet};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use mint_audio::{Clip, PendingClip};
use mint_vfs::{Mount, Vfs, VfsPath};

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
    pub load_us: u64,
    pub max_load_us: u64,
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

#[derive(Debug, Clone)]
pub enum Lookup {
    Ready(Arc<Clip>),
    Pending(Arc<PendingClip>),
    Absent,
}

type Job = (String, Arc<dyn Mount>, VfsPath, Arc<PendingClip>);

type Done = (String, Result<Arc<Clip>, String>);

#[derive(Debug)]
struct Decoder {
    jobs: mpsc::Sender<Job>,
    done: mpsc::Receiver<Done>,
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
    load_us: u64,
    max_load_us: u64,
    pending: BTreeMap<String, Arc<PendingClip>>,
    decoder: Option<Decoder>,
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
        self.collect();
        if let Some(clip) = self.clips.get(path) {
            self.hits = self.hits.saturating_add(1);
            return Some(Arc::clone(clip));
        }
        if self.missing.contains(path) || self.undecodable.contains_key(path) {
            return None;
        }

        let started = Instant::now();
        let Ok(bytes) = vfs.read(path) else {
            self.missing.insert(path.to_string());
            return None;
        };
        let decoded = decode(path, &bytes);
        self.charge(started);
        self.loaded = self.loaded.saturating_add(1);
        match decoded {
            Ok(clip) => {
                let clip = Arc::new(clip);
                self.admit(path.to_string(), Arc::clone(&clip));
                Some(clip)
            }
            Err(error) => {
                self.undecodable.insert(path.to_string(), error);
                None
            }
        }
    }

    pub fn request(&mut self, vfs: &Vfs, path: &str) -> Lookup {
        self.collect();
        if let Some(clip) = self.clips.get(path) {
            self.hits = self.hits.saturating_add(1);
            return Lookup::Ready(Arc::clone(clip));
        }
        if let Some(pending) = self.pending.get(path) {
            self.hits = self.hits.saturating_add(1);
            return Lookup::Pending(Arc::clone(pending));
        }
        if self.missing.contains(path) || self.undecodable.contains_key(path) {
            return Lookup::Absent;
        }
        let started = Instant::now();
        let Some((mount, vfs_path)) = vfs.source(path) else {
            self.missing.insert(path.to_string());
            return Lookup::Absent;
        };
        let pending = Arc::new(PendingClip::default());
        let job = (path.to_string(), mount, vfs_path, Arc::clone(&pending));
        let sent = self
            .decoder()
            .is_some_and(|decoder| decoder.jobs.send(job).is_ok());
        if !sent {
            self.charge(started);
            return match self.get_or_load(vfs, path) {
                Some(clip) => Lookup::Ready(clip),
                None => Lookup::Absent,
            };
        }
        self.charge(started);
        self.loaded = self.loaded.saturating_add(1);
        self.pending.insert(path.to_string(), Arc::clone(&pending));
        Lookup::Pending(pending)
    }

    pub fn settle(&mut self, timeout: std::time::Duration) {
        let until = Instant::now() + timeout;
        while !self.pending.is_empty() {
            let Some(decoder) = self.decoder.as_ref() else {
                return;
            };
            let left = until.saturating_duration_since(Instant::now());
            match decoder.done.recv_timeout(left) {
                Ok(done) => self.accept(done),
                Err(_) => return,
            }
        }
    }

    fn collect(&mut self) {
        let finished: Vec<Done> = match &self.decoder {
            Some(decoder) if !self.pending.is_empty() => decoder.done.try_iter().collect(),
            _ => return,
        };
        for done in finished {
            self.accept(done);
        }
    }

    fn accept(&mut self, (path, decoded): Done) {
        self.pending.remove(&path);
        match decoded {
            Ok(clip) => self.admit(path, clip),
            Err(error) => {
                self.undecodable.insert(path, error);
            }
        }
    }

    fn admit(&mut self, path: String, clip: Arc<Clip>) {
        let size = clip_bytes(&clip);
        self.make_room(size);
        self.bytes = self.bytes.saturating_add(size);
        self.order.push(path.clone());
        self.clips.insert(path, clip);
    }

    fn charge(&mut self, started: Instant) {
        let took = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.load_us = self.load_us.saturating_add(took);
        self.max_load_us = self.max_load_us.max(took);
    }

    fn decoder(&mut self) -> Option<&Decoder> {
        if self.decoder.is_none() {
            let (jobs, job_rx) = mpsc::channel::<Job>();
            let (done_tx, done) = mpsc::channel::<Done>();
            std::thread::Builder::new()
                .name("mint-sound-decode".to_string())
                .spawn(move || {
                    while let Ok((path, mount, vfs_path, pending)) = job_rx.recv() {
                        let decoded = mount
                            .read(&vfs_path)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| decode(&path, &bytes))
                            .map(Arc::new);
                        pending.fill(decoded.as_ref().ok().map(Arc::clone));
                        if done_tx.send((path, decoded)).is_err() {
                            break;
                        }
                    }
                })
                .ok()?;
            self.decoder = Some(Decoder { jobs, done });
        }
        self.decoder.as_ref()
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
            load_us: self.load_us,
            max_load_us: self.max_load_us,
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

fn decode(path: &str, bytes: &[u8]) -> Result<Clip, String> {
    if path.ends_with(".mp3") {
        mint_audio::decode_compressed(bytes).map_err(|e| e.to_string())
    } else {
        Clip::from_wav(bytes).map_err(|e| e.to_string())
    }
}

fn clip_bytes(clip: &Clip) -> usize {
    clip.frames()
        .saturating_mul(usize::from(clip.channels()))
        .saturating_mul(BYTES_PER_SAMPLE)
}
