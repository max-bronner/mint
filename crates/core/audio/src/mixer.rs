use std::sync::Arc;

use mint_audio_format::{decode_wav, Pcm16};

pub const MAX_BUSES: usize = 8;

const I16_SCALE: f32 = 1.0 / 32768.0;

const PAN_DEADZONE: f32 = 1.0e-3;

const DECLICK_MS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

impl Clip {
    pub fn from_wav(bytes: &[u8]) -> Result<Self, mint_audio_format::Error> {
        Ok(Self::from_pcm16(decode_wav(bytes)?))
    }

    #[must_use]
    pub fn from_pcm16(pcm: Pcm16) -> Self {
        Self {
            sample_rate: pcm.sample_rate.max(1),
            channels: pcm.channels.max(1),
            samples: pcm.samples,
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            sample_rate: 1,
            channels: 1,
            samples: Vec::new(),
        }
    }

    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples
            .len()
            .checked_div(usize::from(self.channels))
            .unwrap_or(0)
    }

    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        u64::try_from(self.frames())
            .unwrap_or(u64::MAX)
            .saturating_mul(1000)
            .checked_div(u64::from(self.sample_rate))
            .unwrap_or(0)
    }

    fn sample(&self, frame: usize, channel: u16) -> Option<i16> {
        let index = frame
            .checked_mul(usize::from(self.channels))?
            .checked_add(usize::from(channel))?;
        self.samples.get(index).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    Lowest,
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl Priority {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Priority::Lowest => "lowest",
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct EventKey(pub u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spatial {
    pub position: [f32; 3],
    pub min_range: f32,
    pub max_range: f32,
    pub min_gain: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub position: [f32; 3],
    pub right: [f32; 3],
    pub gain: f32,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            right: [1.0, 0.0, 0.0],
            gain: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rolloff {
    #[default]
    InverseClamped,
    InverseNormalised,
    Linear,
}

impl Rolloff {
    pub const ALL: [Rolloff; 3] = [
        Rolloff::InverseClamped,
        Rolloff::InverseNormalised,
        Rolloff::Linear,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Rolloff::InverseClamped => "inverse-clamped",
            Rolloff::InverseNormalised => "inverse-normalised",
            Rolloff::Linear => "linear",
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Rolloff::InverseClamped => Rolloff::InverseNormalised,
            Rolloff::InverseNormalised => Rolloff::Linear,
            Rolloff::Linear => Rolloff::InverseClamped,
        }
    }

    #[must_use]
    pub fn gain(self, distance: f32, min_range: f32, max_range: f32) -> f32 {
        let min = min_range.max(0.0);
        let max = max_range.max(min);
        if distance <= min {
            return 1.0;
        }
        if distance >= max || max <= min {
            return 0.0;
        }
        match self {
            Rolloff::InverseClamped => min / distance,
            Rolloff::Linear => (max - distance) / (max - min),
            Rolloff::InverseNormalised => {
                if min <= 0.0 {
                    return (max - distance) / max;
                }
                let floor = min / max;
                ((min / distance) - floor) / (1.0 - floor)
            }
        }
    }
}

#[must_use]
pub fn spatial_level(rolloff: Rolloff, gain: f32, spatial: &Spatial, distance: f32) -> f32 {
    let faded = gain * rolloff.gain(distance, spatial.min_range, spatial.max_range);
    faded.max(spatial.min_gain.min(gain))
}

#[derive(Debug, Clone)]
pub struct Play {
    pub clip: Arc<Clip>,
    pub key: EventKey,
    pub priority: Priority,
    pub bus: u8,
    pub gain: f32,
    pub pitch: f32,
    pub looping: bool,
    pub limit: u16,
    pub owner: Option<u64>,
    pub exclusive_owner: bool,
    pub interrupt: bool,
    pub spatial: Option<Spatial>,
    pub ignore_distance_cull: bool,
    pub repeats: u32,
    pub stream: bool,
    pub fade_in_ms: u32,
    pub id: Option<VoiceId>,
}

impl Play {
    #[must_use]
    pub fn new(clip: Arc<Clip>, key: EventKey) -> Self {
        Self {
            clip,
            key,
            priority: Priority::Normal,
            bus: 0,
            gain: 1.0,
            pitch: 1.0,
            looping: false,
            limit: 0,
            owner: None,
            exclusive_owner: false,
            interrupt: false,
            spatial: None,
            ignore_distance_cull: false,
            repeats: 0,
            stream: false,
            fade_in_ms: 0,
            id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Flat,
    Positional,
    Stream,
}

impl Channel {
    fn of(stream: bool, positional: bool) -> Self {
        match (stream, positional) {
            (true, _) => Channel::Stream,
            (false, true) => Channel::Positional,
            (false, false) => Channel::Flat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtSilence {
    Continue,
    Stop,
    Pause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CullReason {
    OutOfRange,
    TooQuiet,
    OwnerBusy,
    EventLimit,
    NoFreeVoice,
    StereoPositional,
    Empty,
}

impl CullReason {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            CullReason::OutOfRange => "out of range",
            CullReason::TooQuiet => "below MinSampleVolume",
            CullReason::OwnerBusy => "emitter already speaking",
            CullReason::EventLimit => "event limit reached",
            CullReason::NoFreeVoice => "no free voice",
            CullReason::StereoPositional => "stereo file requested positionally",
            CullReason::Empty => "clip is empty",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Started(VoiceId),
    Culled(CullReason),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixerConfig {
    pub voices_2d: usize,
    pub voices_3d: usize,
    pub voices_stream: usize,
    pub min_sample_volume: f32,
    pub rolloff: Rolloff,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            voices_2d: 4,
            voices_3d: 25,
            voices_stream: 3,
            min_sample_volume: 0.02,
            rolloff: Rolloff::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MixerStats {
    pub active_2d: usize,
    pub active_3d: usize,
    pub active_stream: usize,
    pub peak_2d: usize,
    pub peak_3d: usize,
    pub started: u64,
    pub finished: u64,
    pub evicted: u64,
    pub culled_range: u64,
    pub culled_quiet: u64,
    pub culled_owner: u64,
    pub culled_limit: u64,
    pub culled_budget: u64,
    pub culled_stereo: u64,
    pub culled_empty: u64,
}

impl MixerStats {
    #[must_use]
    pub fn culled(&self) -> u64 {
        self.culled_range
            .saturating_add(self.culled_quiet)
            .saturating_add(self.culled_owner)
            .saturating_add(self.culled_limit)
            .saturating_add(self.culled_budget)
            .saturating_add(self.culled_stereo)
            .saturating_add(self.culled_empty)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "voices {}/{}/{} 2D+3D+stream (peak {}/{}) | started {} finished {} evicted {} | \
             culled {} = range {} quiet {} owner {} limit {} budget {} stereo {} empty {}",
            self.active_2d,
            self.active_3d,
            self.active_stream,
            self.peak_2d,
            self.peak_3d,
            self.started,
            self.finished,
            self.evicted,
            self.culled(),
            self.culled_range,
            self.culled_quiet,
            self.culled_owner,
            self.culled_limit,
            self.culled_budget,
            self.culled_stereo,
            self.culled_empty,
        )
    }
}

#[derive(Debug)]
struct Voice {
    id: VoiceId,
    clip: Arc<Clip>,
    key: EventKey,
    priority: Priority,
    bus: u8,
    gain: f32,
    pitch: f32,
    looping: bool,
    owner: Option<u64>,
    exclusive_owner: bool,
    spatial: Option<Spatial>,
    position: f64,
    order: u64,
    channel: Channel,
    repeats: u32,
    fade: f32,
    fade_target: f32,
    fade_rate: f32,
    at_silence: AtSilence,
    paused: bool,
    bulk_paused: bool,
    applied: Option<[f32; 2]>,
}

#[derive(Debug)]
pub struct Mixer {
    config: MixerConfig,
    voices: Vec<Voice>,
    releasing: Vec<Voice>,
    listener: Listener,
    bus_gain: [f32; MAX_BUSES],
    master: f32,
    pool_gain: f32,
    pool_exempt_bus: Option<u8>,
    next_id: u64,
    stats: MixerStats,
}

impl Mixer {
    #[must_use]
    pub fn new(config: MixerConfig) -> Self {
        let capacity = config.voices_2d.saturating_add(config.voices_3d);
        Self {
            config,
            voices: Vec::with_capacity(capacity),
            releasing: Vec::new(),
            listener: Listener::default(),
            bus_gain: [1.0; MAX_BUSES],
            master: 1.0,
            pool_gain: 1.0,
            pool_exempt_bus: None,
            next_id: 1,
            stats: MixerStats::default(),
        }
    }

    #[must_use]
    pub fn config(&self) -> MixerConfig {
        self.config
    }

    pub fn set_rolloff(&mut self, rolloff: Rolloff) {
        self.config.rolloff = rolloff;
    }

    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
    }

    #[must_use]
    pub fn listener(&self) -> Listener {
        self.listener
    }

    pub fn set_bus_gain(&mut self, bus: u8, gain: f32) {
        if let Some(slot) = self.bus_gain.get_mut(usize::from(bus)) {
            *slot = gain.max(0.0);
        }
    }

    pub fn set_master_gain(&mut self, gain: f32) {
        self.master = gain.max(0.0);
    }

    pub fn set_pool_gain(&mut self, gain: f32, exempt_bus: Option<u8>) {
        self.pool_gain = gain.max(0.0);
        self.pool_exempt_bus = exempt_bus;
    }

    #[must_use]
    pub fn master_gain(&self) -> f32 {
        self.master
    }

    #[must_use]
    pub fn stats(&self) -> MixerStats {
        self.stats
    }

    #[must_use]
    pub fn active(&self) -> usize {
        self.voices.len()
    }

    pub fn play(&mut self, request: Play) -> Admission {
        if request.clip.frames() == 0 {
            return self.refuse(CullReason::Empty);
        }

        let positional = request.spatial.is_some();
        let channel = Channel::of(request.stream, positional);
        if positional && !request.stream && request.clip.channels() > 1 {
            return self.refuse(CullReason::StereoPositional);
        }

        if let Some(spatial) = request.spatial {
            let distance = distance(self.listener.position, spatial.position);
            let floored = spatial.min_gain.min(request.gain) >= self.config.min_sample_volume;
            if distance >= spatial.max_range && !request.ignore_distance_cull && !floored {
                return self.refuse(CullReason::OutOfRange);
            }
            let attenuated = spatial_level(self.config.rolloff, request.gain, &spatial, distance);
            if attenuated < self.config.min_sample_volume {
                return self.refuse(CullReason::TooQuiet);
            }
        } else if request.gain < self.config.min_sample_volume {
            return self.refuse(CullReason::TooQuiet);
        }

        if request.exclusive_owner {
            if let Some(owner) = request.owner {
                if let Some(index) = self
                    .voices
                    .iter()
                    .position(|v| v.exclusive_owner && v.owner == Some(owner))
                {
                    if !request.interrupt {
                        return self.refuse(CullReason::OwnerBusy);
                    }
                    self.retire(index, false);
                }
            }
        }

        if request.limit > 0 {
            let matching = self
                .voices
                .iter()
                .filter(|v| v.key == request.key && v.channel == channel)
                .count();
            if matching >= usize::from(request.limit) {
                let oldest = self
                    .voices
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.key == request.key && v.channel == channel)
                    .min_by_key(|(_, v)| v.order)
                    .map(|(i, _)| i);
                match (request.interrupt, oldest) {
                    (true, Some(index)) => self.retire(index, false),
                    _ => return self.refuse(CullReason::EventLimit),
                }
            }
        }

        let budget = match channel {
            Channel::Flat => self.config.voices_2d,
            Channel::Positional => self.config.voices_3d,
            Channel::Stream => self.config.voices_stream,
        };
        let in_use = self.voices.iter().filter(|v| v.channel == channel).count();
        if in_use >= budget {
            if request.priority == Priority::Lowest {
                return self.refuse(CullReason::NoFreeVoice);
            }
            let victim = self
                .voices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.channel == channel)
                .min_by_key(|(_, v)| (v.priority, v.order))
                .filter(|(_, v)| v.priority < request.priority)
                .map(|(i, _)| i);
            match victim {
                Some(index) => self.retire(index, true),
                None => return self.refuse(CullReason::NoFreeVoice),
            }
        }

        let id = request.id.unwrap_or(VoiceId(self.next_id));
        self.next_id = self.next_id.saturating_add(1);
        let fading_in = request.fade_in_ms > 0;
        self.voices.push(Voice {
            id,
            clip: Arc::clone(&request.clip),
            key: request.key,
            priority: request.priority,
            bus: request.bus,
            gain: request.gain.max(0.0),
            pitch: if request.pitch > 0.0 {
                request.pitch
            } else {
                1.0
            },
            looping: request.looping,
            owner: request.owner,
            exclusive_owner: request.exclusive_owner,
            spatial: request.spatial,
            position: 0.0,
            order: self.next_id,
            channel,
            repeats: request.repeats,
            fade: if fading_in { 0.0 } else { 1.0 },
            fade_target: 1.0,
            fade_rate: fade_rate(request.fade_in_ms),
            at_silence: AtSilence::Continue,
            paused: false,
            bulk_paused: false,
            applied: None,
        });
        self.stats.started = self.stats.started.saturating_add(1);
        self.recount();
        Admission::Started(id)
    }

    pub fn stop(&mut self, id: VoiceId) {
        if let Some(index) = self.voices.iter().position(|v| v.id == id) {
            self.retire(index, false);
            self.recount();
        }
    }

    pub fn fade_out(&mut self, id: VoiceId, ms: u32) {
        if ms == 0 {
            self.stop(id);
            return;
        }
        self.fade_to(id, 0.0, ms, AtSilence::Stop);
    }

    pub fn pause(&mut self, id: VoiceId, ms: u32) {
        self.fade_to(id, 0.0, ms, AtSilence::Pause);
    }

    pub fn resume(&mut self, id: VoiceId, ms: u32) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.paused = false;
            voice.bulk_paused = false;
        }
        self.fade_to(id, 1.0, ms, AtSilence::Continue);
    }

    pub fn pause_all(&mut self, excluded_bus: Option<u8>) {
        let ids: Vec<VoiceId> = self
            .voices
            .iter()
            .filter(|voice| Some(voice.bus) != excluded_bus && !voice.paused)
            .map(|voice| voice.id)
            .collect();
        for id in ids {
            self.pause_as_by_all(id);
        }
    }

    pub fn pause_as_by_all(&mut self, id: VoiceId) {
        self.pause(id, 0);
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.bulk_paused = true;
        }
    }

    pub fn resume_all(&mut self) {
        let ids: Vec<VoiceId> = self
            .voices
            .iter()
            .filter(|voice| voice.bulk_paused)
            .map(|voice| voice.id)
            .collect();
        for id in ids {
            self.resume(id, 0);
        }
    }

    fn fade_to(&mut self, id: VoiceId, target: f32, ms: u32, at_silence: AtSilence) {
        if let Some(voice) = self.voices.iter_mut().find(|v| v.id == id) {
            voice.fade_target = target;
            voice.at_silence = at_silence;
            let silent = voice.applied.is_none() || voice.paused;
            if ms == 0 && silent {
                voice.fade = target;
                voice.fade_rate = 0.0;
                if target <= 0.0 && at_silence == AtSilence::Pause {
                    voice.paused = true;
                }
            } else {
                voice.fade_rate = fade_rate(ms.max(DECLICK_MS));
            }
        }
    }

    #[must_use]
    pub fn is_live(&self, id: VoiceId) -> bool {
        self.voices.iter().any(|v| v.id == id)
    }

    pub fn stop_owner(&mut self, owner: u64) {
        let mut index = 0;
        while index < self.voices.len() {
            if self
                .voices
                .get(index)
                .is_some_and(|v| v.owner == Some(owner))
            {
                self.retire(index, false);
            } else {
                index = index.saturating_add(1);
            }
        }
        self.recount();
    }

    pub fn stop_key(&mut self, key: EventKey) {
        let mut index = 0;
        while index < self.voices.len() {
            if self.voices.get(index).is_some_and(|v| v.key == key) {
                self.retire(index, false);
            } else {
                index = index.saturating_add(1);
            }
        }
        self.recount();
    }

    pub fn stop_owner_key(&mut self, owner: u64, key: EventKey) {
        let mut index = 0;
        while index < self.voices.len() {
            if self
                .voices
                .get(index)
                .is_some_and(|v| v.owner == Some(owner) && v.key == key)
            {
                self.retire(index, false);
            } else {
                index = index.saturating_add(1);
            }
        }
        self.recount();
    }

    pub fn move_owners(&mut self, moves: &[(u64, [f32; 3])]) {
        for voice in &mut self.voices {
            let (Some(owner), Some(spatial)) = (voice.owner, voice.spatial.as_mut()) else {
                continue;
            };
            if let Ok(index) = moves.binary_search_by_key(&owner, |(id, _)| *id) {
                if let Some((_, position)) = moves.get(index) {
                    spatial.position = *position;
                }
            }
        }
    }

    pub fn stop_all(&mut self) {
        let finished = u64::try_from(self.voices.len()).unwrap_or(u64::MAX);
        for voice in std::mem::take(&mut self.voices) {
            self.release(voice);
        }
        self.stats.finished = self.stats.finished.saturating_add(finished);
        self.recount();
    }

    pub fn render(&mut self, out: &mut [f32], channels: u16, sample_rate: u32) {
        self.render_split(out, None, channels, sample_rate);
    }

    pub fn render_split(
        &mut self,
        out: &mut [f32],
        mut streams: Option<&mut [f32]>,
        channels: u16,
        sample_rate: u32,
    ) {
        out.fill(0.0);
        if let Some(stem) = streams.as_deref_mut() {
            stem.fill(0.0);
        }
        let channels = usize::from(channels.max(1));
        let frames = out.len().checked_div(channels).unwrap_or(0);
        if frames == 0 || sample_rate == 0 {
            return;
        }

        let mut bus_gain = self.bus_gain;
        for (bus, gain) in bus_gain.iter_mut().enumerate() {
            if self.pool_exempt_bus.map(usize::from) != Some(bus) {
                *gain *= self.pool_gain;
            }
        }
        let env = Environment {
            listener: self.listener,
            rolloff: self.config.rolloff,
            master: self.master,
            bus_gain,
            channels,
            frames,
            sample_rate,
        };

        let mut index = 0;
        while index < self.voices.len() {
            let ended = {
                let Some(voice) = self.voices.get_mut(index) else {
                    break;
                };
                match streams.as_deref_mut() {
                    Some(stem) if voice.channel == Channel::Stream => {
                        render_voice(voice, stem, &env)
                    }
                    _ => render_voice(voice, out, &env),
                }
            };
            if ended {
                self.voices.remove(index);
                self.stats.finished = self.stats.finished.saturating_add(1);
            } else {
                index = index.saturating_add(1);
            }
        }
        self.releasing.retain_mut(|voice| {
            let ended = match streams.as_deref_mut() {
                Some(stem) if voice.channel == Channel::Stream => render_voice(voice, stem, &env),
                _ => render_voice(voice, out, &env),
            };
            !ended
        });
        if let Some(stem) = streams {
            for (mixed, streamed) in out.iter_mut().zip(stem.iter()) {
                *mixed += *streamed;
            }
        }
        self.recount();
    }

    fn refuse(&mut self, reason: CullReason) -> Admission {
        let slot = match reason {
            CullReason::OutOfRange => &mut self.stats.culled_range,
            CullReason::TooQuiet => &mut self.stats.culled_quiet,
            CullReason::OwnerBusy => &mut self.stats.culled_owner,
            CullReason::EventLimit => &mut self.stats.culled_limit,
            CullReason::NoFreeVoice => &mut self.stats.culled_budget,
            CullReason::StereoPositional => &mut self.stats.culled_stereo,
            CullReason::Empty => &mut self.stats.culled_empty,
        };
        *slot = slot.saturating_add(1);
        Admission::Culled(reason)
    }

    fn retire(&mut self, index: usize, evicted: bool) {
        if index >= self.voices.len() {
            return;
        }
        let voice = self.voices.remove(index);
        self.release(voice);
        self.stats.finished = self.stats.finished.saturating_add(1);
        if evicted {
            self.stats.evicted = self.stats.evicted.saturating_add(1);
        }
    }

    fn release(&mut self, mut voice: Voice) {
        if voice.applied.is_none() || voice.paused || voice.fade <= 0.0 {
            return;
        }
        let declick = fade_rate(DECLICK_MS);
        voice.fade_rate = if voice.fade_target <= 0.0 {
            voice.fade_rate.max(declick)
        } else {
            declick
        };
        voice.fade_target = 0.0;
        voice.at_silence = AtSilence::Stop;
        self.releasing.push(voice);
    }

    fn recount(&mut self) {
        let count = |channel| self.voices.iter().filter(|v| v.channel == channel).count();
        let three_d = count(Channel::Positional);
        let two_d = count(Channel::Flat);
        self.stats.active_stream = count(Channel::Stream);
        self.stats.active_2d = two_d;
        self.stats.active_3d = three_d;
        self.stats.peak_2d = self.stats.peak_2d.max(two_d);
        self.stats.peak_3d = self.stats.peak_3d.max(three_d);
    }
}

struct Environment {
    listener: Listener,
    rolloff: Rolloff,
    master: f32,
    bus_gain: [f32; MAX_BUSES],
    channels: usize,
    frames: usize,
    sample_rate: u32,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn render_voice(voice: &mut Voice, out: &mut [f32], env: &Environment) -> bool {
    let clip_frames = voice.clip.frames();
    if clip_frames == 0 {
        return true;
    }
    if voice.paused {
        return false;
    }
    let fade_step = voice.fade_rate / env.sample_rate.max(1) as f32;
    let clip_frames_f = clip_frames as f64;

    let bus = env
        .bus_gain
        .get(usize::from(voice.bus))
        .copied()
        .unwrap_or(1.0);
    let mut left = voice.gain * bus * env.master;
    let mut right = left;

    if let Some(spatial) = voice.spatial {
        let d = distance(env.listener.position, spatial.position);
        let level =
            spatial_level(env.rolloff, voice.gain, &spatial, d) * env.listener.gain.max(0.0);
        let pan = pan_of(&env.listener, spatial.position);
        left = level * bus * env.master * (0.5 * (1.0 - pan)).max(0.0).sqrt();
        right = level * bus * env.master * (0.5 * (1.0 + pan)).max(0.0).sqrt();
    } else if voice.clip.channels() == 1 {
        let centre = 0.5f32.sqrt();
        left *= centre;
        right *= centre;
    }

    let step = f64::from(voice.clip.sample_rate()) / f64::from(env.sample_rate.max(1))
        * f64::from(voice.pitch);
    let stereo_source = voice.clip.channels() >= 2;

    let target = [left, right];
    let from = voice.applied.unwrap_or(target);
    voice.applied = Some(target);
    let frames_f = env.frames as f32;
    let ramp = [
        (target[0] - from[0]) / frames_f,
        (target[1] - from[1]) / frames_f,
    ];

    for frame in 0..env.frames {
        if voice.position >= clip_frames_f {
            if voice.looping {
                voice.position %= clip_frames_f;
            } else if voice.repeats > 0 {
                voice.repeats = voice.repeats.saturating_sub(1);
                voice.position %= clip_frames_f;
            } else {
                return true;
            }
        }
        if voice.fade != voice.fade_target {
            voice.fade = if voice.fade < voice.fade_target {
                (voice.fade + fade_step).min(voice.fade_target)
            } else {
                (voice.fade - fade_step).max(voice.fade_target)
            };
            if voice.fade <= 0.0 {
                match voice.at_silence {
                    AtSilence::Stop => return true,
                    AtSilence::Pause => {
                        voice.paused = true;
                        return false;
                    }
                    AtSilence::Continue => {}
                }
            }
        }

        let base = voice.position as usize;
        let frac = (voice.position - base as f64) as f32;
        let next = base.saturating_add(1);

        let (l, r) = if stereo_source {
            (
                interpolate(&voice.clip, base, next, 0, frac, voice.looping),
                interpolate(&voice.clip, base, next, 1, frac, voice.looping),
            )
        } else {
            let m = interpolate(&voice.clip, base, next, 0, frac, voice.looping);
            (m, m)
        };

        let along = frame.saturating_add(1) as f32;
        let at = frame.saturating_mul(env.channels);
        if let Some(slot) = out.get_mut(at) {
            *slot += l * (from[0] + ramp[0] * along) * voice.fade;
        }
        if env.channels >= 2 {
            if let Some(slot) = out.get_mut(at.saturating_add(1)) {
                *slot += r * (from[1] + ramp[1] * along) * voice.fade;
            }
        }

        voice.position += step;
    }

    !voice.looping && voice.repeats == 0 && voice.position >= clip_frames_f
}

#[allow(clippy::cast_precision_loss)]
fn fade_rate(ms: u32) -> f32 {
    if ms == 0 {
        0.0
    } else {
        1000.0 / ms as f32
    }
}

fn interpolate(
    clip: &Clip,
    base: usize,
    next: usize,
    channel: u16,
    frac: f32,
    looping: bool,
) -> f32 {
    let a = clip.sample(base, channel).unwrap_or(0);
    let b = match clip.sample(next, channel) {
        Some(v) => v,
        None if looping => clip.sample(0, channel).unwrap_or(0),
        None => a,
    };
    let a = f32::from(a) * I16_SCALE;
    let b = f32::from(b) * I16_SCALE;
    a + (b - a) * frac
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn pan_of(listener: &Listener, position: [f32; 3]) -> f32 {
    let dx = position[0] - listener.position[0];
    let dy = position[1] - listener.position[1];
    let dz = position[2] - listener.position[2];
    let length = (dx * dx + dy * dy + dz * dz).sqrt();
    if length < PAN_DEADZONE {
        return 0.0;
    }
    let rx = listener.right[0];
    let ry = listener.right[1];
    let rz = listener.right[2];
    let right_len = (rx * rx + ry * ry + rz * rz).sqrt();
    if right_len < PAN_DEADZONE {
        return 0.0;
    }
    ((dx * rx + dy * ry + dz * rz) / (length * right_len)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests;
