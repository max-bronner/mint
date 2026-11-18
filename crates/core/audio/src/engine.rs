use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use crate::device::{Device, DeviceFormat, DeviceStats, Source};
use crate::mixer::{
    Admission, Clip, EventKey, Listener, Mixer, MixerConfig, MixerStats, Play, Rolloff, VoiceId,
    MAX_BUSES,
};
use crate::tap::Tap;

#[derive(Debug, Default)]
pub struct PendingClip(OnceLock<Option<Arc<Clip>>>);

impl PendingClip {
    pub fn fill(&self, clip: Option<Arc<Clip>>) {
        let _ = self.0.set(clip);
    }

    #[must_use]
    pub fn get(&self) -> Option<&Option<Arc<Clip>>> {
        self.0.get()
    }
}

enum Command {
    Play(Box<Play>),
    PlayPending(Box<Play>, Arc<PendingClip>),
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

impl Command {
    fn label(&self) -> String {
        match self {
            Command::Play(p) => format!(
                "play key {} bus {} stream {} gain {:.3} fade-in {} ms",
                p.key.0, p.bus, p.stream, p.gain, p.fade_in_ms
            ),
            Command::PlayPending(p, _) => format!(
                "play-pending key {} bus {} stream {}",
                p.key.0, p.bus, p.stream
            ),
            Command::Stop(id) => format!("stop {}", id.0),
            Command::FadeOut(id, ms) => format!("fade-out {} {ms} ms", id.0),
            Command::Pause(id, ms) => format!("pause {} {ms} ms", id.0),
            Command::Resume(id, ms) => format!("resume {} {ms} ms", id.0),
            Command::PauseAll(bus) => format!("pause-all except {bus:?}"),
            Command::ResumeAll => "resume-all".to_string(),
            Command::StopOwner(owner) => format!("stop-owner {owner}"),
            Command::StopKey(key) => format!("stop-key {}", key.0),
            Command::StopOwnerKey(owner, key) => format!("stop-owner-key {owner} {}", key.0),
            Command::MoveOwners(_) => "move-owners".to_string(),
            Command::StopAll => "stop-all".to_string(),
            Command::Listener(_) => "listener".to_string(),
            Command::BusGain(bus, gain) => format!("bus {bus} gain {gain:.4}"),
            Command::Master(gain) => format!("master {gain:.4}"),
            Command::Pool(gain, exempt) => format!("pool {gain:.4} except {exempt:?}"),
            Command::Rolloff(rolloff) => format!("rolloff {}", rolloff.label()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    None,
    Paused,
    PausedByAll,
}

struct Waiting {
    request: Box<Play>,
    clip: Option<Arc<PendingClip>>,
    hold: Hold,
}

struct MixerSource {
    mixer: Mixer,
    rx: mpsc::Receiver<Command>,
    stats: Arc<Mutex<MixerStats>>,
    waiting: Vec<Waiting>,
    tap: Option<Tap>,
}

impl MixerSource {
    fn new(
        mixer: Mixer,
        rx: mpsc::Receiver<Command>,
        stats: Arc<Mutex<MixerStats>>,
        name: &str,
    ) -> Self {
        Self {
            mixer,
            rx,
            stats,
            waiting: Vec::new(),
            tap: Tap::from_env(name),
        }
    }

    fn forget(&mut self, gone: impl Fn(&Play) -> bool) {
        self.waiting.retain(|waiting| !gone(&waiting.request));
    }

    fn hold(&mut self, which: impl Fn(&Waiting) -> bool, hold: Hold) {
        for waiting in self.waiting.iter_mut().filter(|w| which(w)) {
            waiting.hold = hold;
        }
    }

    fn apply(&mut self, command: Command) {
        if let Some(tap) = &mut self.tap {
            if !matches!(command, Command::Listener(_) | Command::MoveOwners(_)) {
                tap.notes.push(command.label());
            }
        }
        match command {
            Command::Play(request) => {
                if self.waiting.iter().any(|w| w.request.key == request.key) {
                    self.waiting.push(Waiting {
                        request,
                        clip: None,
                        hold: Hold::None,
                    });
                } else {
                    self.mixer.play(*request);
                }
            }
            Command::PlayPending(request, clip) => self.waiting.push(Waiting {
                request,
                clip: Some(clip),
                hold: Hold::None,
            }),
            Command::Stop(id) => {
                self.forget(|p| p.id == Some(id));
                self.mixer.stop(id);
            }
            Command::FadeOut(id, ms) => {
                self.forget(|p| p.id == Some(id));
                self.mixer.fade_out(id, ms);
            }
            Command::Pause(id, ms) => {
                self.hold(|w| w.request.id == Some(id), Hold::Paused);
                self.mixer.pause(id, ms);
            }
            Command::Resume(id, ms) => {
                self.hold(|w| w.request.id == Some(id), Hold::None);
                self.mixer.resume(id, ms);
            }
            Command::PauseAll(excluded_bus) => {
                self.hold(
                    |w| w.hold == Hold::None && Some(w.request.bus) != excluded_bus,
                    Hold::PausedByAll,
                );
                self.mixer.pause_all(excluded_bus);
            }
            Command::ResumeAll => {
                self.hold(|w| w.hold == Hold::PausedByAll, Hold::None);
                self.mixer.resume_all();
            }
            Command::StopOwner(owner) => {
                self.forget(|p| p.owner == Some(owner));
                self.mixer.stop_owner(owner);
            }
            Command::StopKey(key) => {
                self.forget(|p| p.key == key);
                self.mixer.stop_key(key);
            }
            Command::StopOwnerKey(owner, key) => {
                self.forget(|p| p.owner == Some(owner) && p.key == key);
                self.mixer.stop_owner_key(owner, key);
            }
            Command::MoveOwners(moves) => {
                for Waiting { request, .. } in &mut self.waiting {
                    let (Some(owner), Some(spatial)) = (request.owner, request.spatial.as_mut())
                    else {
                        continue;
                    };
                    if let Ok(found) = moves.binary_search_by_key(&owner, |(o, _)| *o) {
                        if let Some((_, position)) = moves.get(found) {
                            spatial.position = *position;
                        }
                    }
                }
                self.mixer.move_owners(&moves);
            }
            Command::StopAll => {
                self.waiting.clear();
                self.mixer.stop_all();
            }
            Command::Listener(listener) => self.mixer.set_listener(listener),
            Command::BusGain(bus, gain) => self.mixer.set_bus_gain(bus, gain),
            Command::Master(gain) => self.mixer.set_master_gain(gain),
            Command::Pool(gain, exempt) => self.mixer.set_pool_gain(gain, exempt),
            Command::Rolloff(rolloff) => self.mixer.set_rolloff(rolloff),
        }
    }

    fn start_arrived(&mut self) {
        if self.waiting.is_empty() {
            return;
        }
        let mut blocked: Vec<EventKey> = Vec::new();
        for mut waiting in std::mem::take(&mut self.waiting) {
            let key = waiting.request.key;
            let arrived = match &waiting.clip {
                None => Some(true),
                Some(clip) => match clip.get() {
                    None => None,
                    Some(Some(decoded)) => {
                        waiting.request.clip = Arc::clone(decoded);
                        Some(true)
                    }
                    Some(None) => Some(false),
                },
            };
            match arrived {
                None => {
                    blocked.push(key);
                    self.waiting.push(waiting);
                }
                Some(_) if blocked.contains(&key) => self.waiting.push(waiting),
                Some(true) => {
                    let hold = waiting.hold;
                    if let Admission::Started(id) = self.mixer.play(*waiting.request) {
                        match hold {
                            Hold::None => {}
                            Hold::Paused => self.mixer.pause(id, 0),
                            Hold::PausedByAll => self.mixer.pause_as_by_all(id),
                        }
                    }
                }
                Some(false) => {}
            }
        }
    }
}

impl Source for MixerSource {
    fn render(&mut self, out: &mut [f32], format: DeviceFormat) {
        while let Ok(command) = self.rx.try_recv() {
            self.apply(command);
        }
        self.start_arrived();
        match &mut self.tap {
            Some(tap) => {
                let streams_before = self.mixer.stats().active_stream;
                tap.stem.resize(out.len(), 0.0);
                self.mixer.render_split(
                    out,
                    Some(&mut tap.stem),
                    format.channels,
                    format.sample_rate,
                );
                let streams = self.mixer.stats().active_stream;
                if streams != streams_before {
                    tap.notes
                        .push(format!("streams {streams_before} -> {streams}"));
                }
                tap.send(format, out);
            }
            None => self.mixer.render(out, format.channels, format.sample_rate),
        }
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
        Self::open_named(config, "engine")
    }

    #[must_use]
    pub fn open_named(config: MixerConfig, name: &str) -> Self {
        let (tx, rx) = mpsc::channel();
        let stats = Arc::new(Mutex::new(MixerStats::default()));
        let source = MixerSource::new(Mixer::new(config), rx, Arc::clone(&stats), name);
        crate::loopback::start_from_env();

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

    pub fn play_pending(&self, mut request: Play, clip: Arc<PendingClip>) -> VoiceId {
        let id = VoiceId(self.next_id.fetch_add(1, Ordering::Relaxed));
        request.id = Some(id);
        let _ = self.tx.send(Command::PlayPending(Box::new(request), clip));
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

    #[must_use]
    pub fn device_stats(&self) -> DeviceStats {
        self.device.as_ref().map(|d| d.stats()).unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;
    use mint_audio_format::Pcm16;

    fn source() -> (MixerSource, mpsc::Sender<Command>) {
        let (tx, rx) = mpsc::channel();
        let stats = Arc::new(Mutex::new(MixerStats::default()));
        let mixer = Mixer::new(MixerConfig::default());
        (MixerSource::new(mixer, rx, stats, "test"), tx)
    }

    fn pending(id: u64, key: u32) -> (Command, Arc<PendingClip>) {
        let clip = Arc::new(PendingClip::default());
        let mut request = Play::new(Arc::new(Clip::empty()), EventKey(key));
        request.id = Some(VoiceId(id));
        (
            Command::PlayPending(Box::new(request), Arc::clone(&clip)),
            clip,
        )
    }

    fn tone() -> Arc<Clip> {
        Arc::new(Clip::from_pcm16(Pcm16 {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![8_000; 44_100],
        }))
    }

    fn mix(source: &mut MixerSource) -> MixerStats {
        let mut out = vec![0.0; 2 * 441];
        source.render(&mut out, DeviceFormat::CD);
        source.mixer.stats()
    }

    #[test]
    fn a_pending_request_starts_when_its_clip_arrives_and_never_once_stopped() {
        let (mut source, tx) = source();
        let (first, first_clip) = pending(1, 1);
        let (second, second_clip) = pending(2, 2);
        tx.send(first).unwrap();
        tx.send(second).unwrap();
        tx.send(Command::Stop(VoiceId(2))).unwrap();
        assert_eq!(mix(&mut source).started, 0, "nothing has arrived");

        first_clip.fill(Some(tone()));
        second_clip.fill(Some(tone()));
        assert_eq!(
            mix(&mut source).started,
            1,
            "the stopped request stays stopped"
        );
        assert!(source.mixer.is_live(VoiceId(1)));
        assert!(!source.mixer.is_live(VoiceId(2)));
        assert!(source.waiting.is_empty());
    }

    fn peak(source: &mut MixerSource) -> f32 {
        let mut out = vec![0.0; 2 * 441];
        source.render(&mut out, DeviceFormat::CD);
        out.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn a_click_still_decoding_when_the_pause_menu_opens_plays_when_it_closes() {
        let (mut source, tx) = source();
        let (click, clip) = pending(1, 1);
        tx.send(click).unwrap();
        tx.send(Command::PauseAll(Some(5))).unwrap();
        clip.fill(Some(tone()));
        assert_eq!(peak(&mut source), 0.0, "held while the match is paused");
        assert_eq!(source.mixer.stats().started, 1);
        tx.send(Command::ResumeAll).unwrap();
        assert!(peak(&mut source) > 0.1, "heard once the pause ends");
    }

    #[test]
    fn a_ready_request_waits_behind_a_decoding_one_of_its_event() {
        let (mut source, tx) = source();
        let interrupting = |clip: Arc<Clip>, id: u64| {
            let mut request = Play::new(clip, EventKey(7));
            request.id = Some(VoiceId(id));
            request.limit = 1;
            request.interrupt = true;
            Box::new(request)
        };
        let clip = Arc::new(PendingClip::default());
        tx.send(Command::PlayPending(
            interrupting(Arc::new(Clip::empty()), 1),
            Arc::clone(&clip),
        ))
        .unwrap();
        tx.send(Command::Play(interrupting(tone(), 2))).unwrap();
        assert_eq!(mix(&mut source).started, 0, "the newer waits its turn");
        clip.fill(Some(tone()));
        assert_eq!(mix(&mut source).started, 2);
        assert!(
            source.mixer.is_live(VoiceId(2)),
            "the newer replaced the older"
        );
        assert!(!source.mixer.is_live(VoiceId(1)));
    }

    #[test]
    fn a_decoding_loop_starts_once_and_carries_on() {
        let (mut source, tx) = source();
        let (Command::PlayPending(mut request, clip), _) = pending(1, 3) else {
            unreachable!()
        };
        request.looping = true;
        let pending_clip = Arc::clone(&clip);
        tx.send(Command::PlayPending(request, clip)).unwrap();
        pending_clip.fill(Some(tone()));
        for _ in 0..300 {
            assert!(peak(&mut source) > 0.1);
        }
        assert_eq!(source.mixer.stats().started, 1);
        assert_eq!(source.mixer.active(), 1);
    }
}
