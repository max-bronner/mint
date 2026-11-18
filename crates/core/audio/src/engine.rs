use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use crate::device::{Device, DeviceFormat, Source};
use crate::mixer::{
    EventKey, Listener, Mixer, MixerConfig, MixerStats, Play, Rolloff, VoiceId, MAX_BUSES,
};

enum Command {
    Play(Box<Play>),
    Stop(VoiceId),
    FadeOut(VoiceId, u32),
    Pause(VoiceId, u32),
    Resume(VoiceId, u32),
    PauseAll(Option<u8>),
    ResumeAll,
    StopOwner(u64),
    StopKey(EventKey),
    StopOwnerKey(u64, EventKey),
    MoveOwners(Vec<(u64, [f32; 3])>),
    StopAll,
    Listener(Listener),
    BusGain(u8, f32),
    Master(f32),
    Pool(f32, Option<u8>),
    Rolloff(Rolloff),
}

struct MixerSource {
    mixer: Mixer,
    rx: mpsc::Receiver<Command>,
    stats: Arc<Mutex<MixerStats>>,
}

impl Source for MixerSource {
    fn render(&mut self, out: &mut [f32], format: DeviceFormat) {
        while let Ok(command) = self.rx.try_recv() {
            match command {
                Command::Play(request) => {
                    self.mixer.play(*request);
                }
                Command::Stop(id) => self.mixer.stop(id),
                Command::FadeOut(id, ms) => self.mixer.fade_out(id, ms),
                Command::Pause(id, ms) => self.mixer.pause(id, ms),
                Command::Resume(id, ms) => self.mixer.resume(id, ms),
                Command::PauseAll(excluded_bus) => self.mixer.pause_all(excluded_bus),
                Command::ResumeAll => self.mixer.resume_all(),
                Command::StopOwner(owner) => self.mixer.stop_owner(owner),
                Command::StopKey(key) => self.mixer.stop_key(key),
                Command::StopOwnerKey(owner, key) => self.mixer.stop_owner_key(owner, key),
                Command::MoveOwners(moves) => self.mixer.move_owners(&moves),
                Command::StopAll => self.mixer.stop_all(),
                Command::Listener(listener) => self.mixer.set_listener(listener),
                Command::BusGain(bus, gain) => self.mixer.set_bus_gain(bus, gain),
                Command::Master(gain) => self.mixer.set_master_gain(gain),
                Command::Pool(gain, exempt) => self.mixer.set_pool_gain(gain, exempt),
                Command::Rolloff(rolloff) => self.mixer.set_rolloff(rolloff),
            }
        }
        self.mixer.render(out, format.channels, format.sample_rate);
        if let Ok(mut shared) = self.stats.lock() {
            *shared = self.mixer.stats();
        }
    }
}

pub struct AudioEngine {
    tx: mpsc::Sender<Command>,
    stats: Arc<Mutex<MixerStats>>,
    device: Option<Box<dyn Device>>,
    diagnostic: String,
    next_id: AtomicU64,
}

impl AudioEngine {
    #[must_use]
    pub fn open(config: MixerConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        let stats = Arc::new(Mutex::new(MixerStats::default()));
        let source = MixerSource {
            mixer: Mixer::new(config),
            rx,
            stats: Arc::clone(&stats),
        };

        match open_default(Box::new(source)) {
            Ok(device) => {
                let diagnostic = device.describe();
                Self {
                    tx,
                    stats,
                    device: Some(device),
                    diagnostic,
                    next_id: AtomicU64::new(1),
                }
            }
            Err(message) => Self::silent_with(message),
        }
    }

    #[must_use]
    pub fn silent() -> Self {
        Self::silent_with("audio disabled by the caller".to_string())
    }

    fn silent_with(reason: String) -> Self {
        let (tx, _rx) = mpsc::channel();
        Self {
            tx,
            stats: Arc::new(Mutex::new(MixerStats::default())),
            device: None,
            diagnostic: reason,
            next_id: AtomicU64::new(1),
        }
    }

    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.device.is_none()
    }

    #[must_use]
    pub fn device(&self) -> Option<&dyn Device> {
        self.device.as_deref()
    }

    #[must_use]
    pub fn format(&self) -> Option<DeviceFormat> {
        self.device.as_ref().map(|d| d.format())
    }

    #[must_use]
    pub fn describe(&self) -> &str {
        &self.diagnostic
    }

    pub fn play(&self, mut request: Play) -> VoiceId {
        let id = VoiceId(self.next_id.fetch_add(1, Ordering::Relaxed));
        request.id = Some(id);
        let _ = self.tx.send(Command::Play(Box::new(request)));
        id
    }

    pub fn fade_out(&self, id: VoiceId, ms: u32) {
        let _ = self.tx.send(Command::FadeOut(id, ms));
    }

    pub fn pause(&self, id: VoiceId, ms: u32) {
        let _ = self.tx.send(Command::Pause(id, ms));
    }

    pub fn resume(&self, id: VoiceId, ms: u32) {
        let _ = self.tx.send(Command::Resume(id, ms));
    }

    pub fn pause_all(&self, excluded_bus: Option<u8>) {
        let _ = self.tx.send(Command::PauseAll(excluded_bus));
    }

    pub fn resume_all(&self) {
        let _ = self.tx.send(Command::ResumeAll);
    }

    pub fn stop(&self, id: VoiceId) {
        let _ = self.tx.send(Command::Stop(id));
    }

    pub fn stop_owner(&self, owner: u64) {
        let _ = self.tx.send(Command::StopOwner(owner));
    }

    pub fn stop_key(&self, key: EventKey) {
        let _ = self.tx.send(Command::StopKey(key));
    }

    pub fn stop_owner_key(&self, owner: u64, key: EventKey) {
        let _ = self.tx.send(Command::StopOwnerKey(owner, key));
    }

    pub fn move_owners(&self, moves: Vec<(u64, [f32; 3])>) {
        let _ = self.tx.send(Command::MoveOwners(moves));
    }

    pub fn stop_all(&self) {
        let _ = self.tx.send(Command::StopAll);
    }

    pub fn set_listener(&self, listener: Listener) {
        let _ = self.tx.send(Command::Listener(listener));
    }

    pub fn set_bus_gain(&self, bus: u8, gain: f32) {
        let _ = self.tx.send(Command::BusGain(bus, gain));
    }

    pub fn set_master_gain(&self, gain: f32) {
        let _ = self.tx.send(Command::Master(gain));
    }

    pub fn set_pool_gain(&self, gain: f32, exempt_bus: Option<u8>) {
        let _ = self.tx.send(Command::Pool(gain, exempt_bus));
    }

    pub fn set_rolloff(&self, rolloff: Rolloff) {
        let _ = self.tx.send(Command::Rolloff(rolloff));
    }

    #[must_use]
    pub fn stats(&self) -> MixerStats {
        self.stats
            .lock()
            .map(|s| *s)
            .unwrap_or_else(|poisoned| *poisoned.into_inner())
    }
}

#[cfg(windows)]
fn open_default(source: Box<dyn Source>) -> Result<Box<dyn Device>, String> {
    crate::wasapi::open(source)
        .map(|device| Box::new(device) as Box<dyn Device>)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn open_default(_source: Box<dyn Source>) -> Result<Box<dyn Device>, String> {
    Err("no audio backend is implemented for this platform yet".to_string())
}

pub const BUSES: usize = MAX_BUSES;
