use mint_audio::{
    AudioEngine, Clip, EventKey, Listener, MixerConfig, MixerStats, Play, Priority, Rolloff,
    Spatial, VoiceId,
};
use mint_sage_ini::audio::{
    AudioEventDef, AudioEventKind, AudioPriority, ControlFlags, Submix, TypeFlags,
};
use mint_vfs::Vfs;
use std::sync::Arc;

use crate::bank::{BankStats, Lookup, SoundBank};
use crate::library::AudioLibrary;
use crate::music::{Context, MusicEvent, MusicRequest, MusicSystem};
use crate::rng::ClientRng;

#[derive(Debug, Clone)]
pub struct EventRequest<'a> {
    pub event: &'a str,
    pub position: Option<[f32; 3]>,
    pub owner: Option<u64>,
    pub shroud_clear: bool,
    pub audience: Option<Audience>,
    pub overrides: Overrides,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Overrides {
    pub looping: Option<bool>,
    pub volume: Option<f32>,
    pub min_volume: Option<f32>,
    pub min_range: Option<f32>,
    pub max_range: Option<f32>,
    pub priority: Option<AudioPriority>,
}

impl Overrides {
    pub fn apply(&self, def: &mut AudioEventDef) {
        if let Some(looping) = self.looping {
            if looping {
                def.control.0 |= ControlFlags::LOOP;
            } else {
                def.control.0 &= !ControlFlags::LOOP;
            }
        }
        if let Some(volume) = self.volume {
            def.volume = Some(volume);
        }
        if let Some(min_volume) = self.min_volume {
            def.min_volume = Some(min_volume);
        }
        if let Some(min_range) = self.min_range {
            def.min_range = Some(min_range);
        }
        if let Some(max_range) = self.max_range {
            def.max_range = Some(max_range);
        }
        if let Some(priority) = self.priority {
            def.priority = priority;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Audible,
    OutOfRange,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Local,
    Ally,
    Enemy,
}

impl<'a> EventRequest<'a> {
    #[must_use]
    pub fn new(event: &'a str) -> Self {
        Self {
            event,
            position: None,
            owner: None,
            shroud_clear: true,
            audience: None,
            overrides: Overrides::default(),
        }
    }

    #[must_use]
    pub fn at(event: &'a str, position: [f32; 3]) -> Self {
        Self {
            event,
            position: Some(position),
            owner: None,
            shroud_clear: true,
            audience: None,
            overrides: Overrides::default(),
        }
    }

    #[must_use]
    pub fn by(mut self, owner: u64) -> Self {
        self.owner = Some(owner);
        self
    }

    #[must_use]
    pub fn visible(mut self, shroud_clear: bool) -> Self {
        self.shroud_clear = shroud_clear;
        self
    }

    #[must_use]
    pub fn heard_as(mut self, audience: Audience) -> Self {
        self.audience = Some(audience);
        self
    }

    #[must_use]
    pub fn with(mut self, overrides: Overrides) -> Self {
        self.overrides = overrides;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    Queued,
    UnknownEvent,
    NoVariants,
    MissingFile,
    PlayPercent,
    Shrouded,
    NotForLocal,
    Disabled,
}

impl Outcome {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Outcome::Queued => "queued",
            Outcome::UnknownEvent => "unknown event",
            Outcome::NoVariants => "no variants",
            Outcome::MissingFile => "file missing or undecodable",
            Outcome::PlayPercent => "PlayPercent roll failed",
            Outcome::Shrouded => "shrouded",
            Outcome::NotForLocal => "not for the local player",
            Outcome::Disabled => "disabled by a script",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RequestStats {
    pub queued: u64,
    pub unknown: u64,
    pub no_variants: u64,
    pub missing: u64,
    pub play_percent: u64,
    pub shrouded: u64,
    pub not_for_local: u64,
    pub disabled: u64,
}

impl RequestStats {
    #[must_use]
    pub fn total(&self) -> u64 {
        self.queued
            .saturating_add(self.unknown)
            .saturating_add(self.no_variants)
            .saturating_add(self.missing)
            .saturating_add(self.play_percent)
            .saturating_add(self.shrouded)
            .saturating_add(self.not_for_local)
            .saturating_add(self.disabled)
    }

    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "requests {} = queued {} unknown {} no-variants {} missing {} play-percent {} shrouded {} not-local {} disabled {}",
            self.total(),
            self.queued,
            self.unknown,
            self.no_variants,
            self.missing,
            self.play_percent,
            self.shrouded,
            self.not_for_local,
            self.disabled,
        )
    }
}

pub struct SageAudio {
    engine: AudioEngine,
    library: AudioLibrary,
    bank: SoundBank,
    rng: ClientRng,
    cursor: std::collections::BTreeMap<u32, usize>,
    stats: RequestStats,
    rolloff: Rolloff,
    listener: Listener,
    music: MusicSystem,
    disabled: std::collections::BTreeSet<String>,
    placeholder: Arc<Clip>,
}

impl SageAudio {
    #[must_use]
    pub fn open(library: AudioLibrary) -> Self {
        Self::open_named(library, "engine")
    }

    #[must_use]
    pub fn open_named(library: AudioLibrary, name: &str) -> Self {
        let engine = AudioEngine::open_named(mixer_config(&library), name);
        Self::with_engine(engine, library)
    }

    #[must_use]
    pub fn silent(library: AudioLibrary) -> Self {
        Self::with_engine(AudioEngine::silent(), library)
    }

    #[must_use]
    pub fn with_engine(engine: AudioEngine, library: AudioLibrary) -> Self {
        let settings = library.settings();
        let bank = SoundBank::new(settings.audio_footprint_in_bytes);
        engine.set_bus_gain(Submix::SoundFx.bus(), settings.default_sound_volume);
        engine.set_bus_gain(Submix::Voice.bus(), settings.default_voice_volume);
        engine.set_bus_gain(Submix::Ambient.bus(), settings.default_ambient_volume);
        engine.set_bus_gain(Submix::Movie.bus(), settings.default_movie_volume);
        engine.set_bus_gain(Submix::Music.bus(), settings.default_music_volume);
        let music = MusicSystem::new(settings.time_to_fade_audio_ms);
        Self {
            engine,
            library,
            bank,
            music,
            rng: ClientRng::from_clock(),
            cursor: std::collections::BTreeMap::new(),
            stats: RequestStats::default(),
            rolloff: Rolloff::default(),
            listener: Listener::default(),
            disabled: std::collections::BTreeSet::new(),
            placeholder: Arc::new(Clip::empty()),
        }
    }

    pub fn set_rng(&mut self, rng: ClientRng) {
        self.rng = rng;
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn random_delay_ms(&mut self, low: u32, high: u32) -> u32 {
        self.rng.next_range(low as f32, high as f32).round() as u32
    }

    #[must_use]
    pub fn library(&self) -> &AudioLibrary {
        &self.library
    }

    #[must_use]
    pub fn bank(&self) -> &SoundBank {
        &self.bank
    }

    #[must_use]
    pub fn mixer_stats(&self) -> MixerStats {
        self.engine.stats()
    }

    #[must_use]
    pub fn device_stats(&self) -> mint_audio::DeviceStats {
        self.engine.device_stats()
    }

    #[must_use]
    pub fn request_stats(&self) -> RequestStats {
        self.stats
    }

    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.engine.is_silent()
    }

    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
        self.engine.set_listener(listener);
    }

    pub fn set_camera(&mut self, eye: [f32; 3], look_at: [f32; 3], right: [f32; 3]) {
        let listener =
            crate::microphone::place(&self.library.settings().microphone, eye, look_at, right);
        self.set_listener(listener);
    }

    #[must_use]
    pub fn listener(&self) -> Listener {
        self.listener
    }

    pub fn set_master_gain(&self, gain: f32) {
        self.engine.set_master_gain(gain);
    }

    pub fn set_pool_gain(&self, gain: f32) {
        self.engine
            .set_pool_gain(gain, Some(Submix::Unscaled.bus()));
    }

    pub fn set_submix_gain(&self, submix: Submix, gain: f32) {
        self.engine.set_bus_gain(submix.bus(), gain);
    }

    #[must_use]
    pub fn rolloff(&self) -> Rolloff {
        self.rolloff
    }

    pub fn set_rolloff(&mut self, rolloff: Rolloff) {
        self.rolloff = rolloff;
        self.engine.set_rolloff(rolloff);
    }

    pub fn stop_event(&self, name: &str) {
        for key in self.played_keys(name) {
            self.engine.stop_key(key);
        }
    }

    pub fn disable_event(&mut self, name: &str) {
        self.stop_event(name);
        self.disabled.insert(name.to_ascii_lowercase());
    }

    pub fn enable_event(&mut self, name: &str) {
        self.disabled.remove(&name.to_ascii_lowercase());
    }

    pub fn enable_all_events(&mut self) {
        self.disabled.clear();
    }

    fn is_disabled(&self, name: &str) -> bool {
        !self.disabled.is_empty() && self.disabled.contains(&name.to_ascii_lowercase())
    }

    pub fn stop_event_for(&self, owner: u64, name: &str) {
        for key in self.played_keys(name) {
            self.engine.stop_owner_key(owner, key);
        }
    }

    pub(crate) fn played_keys(&self, name: &str) -> Vec<EventKey> {
        let mut keys = Vec::new();
        let mut pending: Vec<(&str, u8)> = vec![(name, 0)];
        while let Some((name, depth)) = pending.pop() {
            if let Some(key) = self.library.key(name) {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            } else if depth < MAX_MULTISOUND_DEPTH {
                if let Some(multi) = self.library.multisound(name) {
                    let next = depth.saturating_add(1);
                    pending.extend(multi.subsounds.iter().map(|s| (s.name.as_str(), next)));
                }
            }
        }
        keys
    }

    pub fn move_owners(&self, moves: Vec<(u64, [f32; 3])>) {
        self.engine.move_owners(moves);
    }

    pub fn stop_owner(&self, owner: u64) {
        self.engine.stop_owner(owner);
    }

    pub fn stop_all(&self) {
        self.engine.stop_all();
    }

    pub fn pause_all(&self) {
        self.engine
            .pause_all(Some(mint_sage_ini::audio::Submix::Music.bus()));
    }

    pub fn resume_all(&self) {
        self.engine.resume_all();
    }

    pub fn play(&mut self, vfs: &Vfs, request: &EventRequest<'_>) -> Outcome {
        self.play_tracked(vfs, request).0
    }

    pub fn play_tracked(
        &mut self,
        vfs: &Vfs,
        request: &EventRequest<'_>,
    ) -> (Outcome, Option<VoiceId>) {
        if self.is_disabled(request.event) {
            self.stats.disabled = self.stats.disabled.saturating_add(1);
            return (Outcome::Disabled, None);
        }
        if self.is_music(request.event, 0) {
            self.music_replace(vfs, MusicRequest::forever(request.event));
            return (Outcome::Queued, None);
        }
        self.play_nested(vfs, request, 0)
    }

    pub fn decoded_duration_ms(&mut self, vfs: &Vfs, name: &str) -> Option<u64> {
        let mut names = vec![name.to_owned()];
        if self.library.get(name).is_none() {
            names = self
                .library
                .multisound(name)?
                .subsounds
                .iter()
                .map(|sub| sub.name.clone())
                .collect();
        }
        let mut longest = None;
        for event in names {
            let Some(def) = self.library.get(&event).cloned() else {
                continue;
            };
            for path in self.library.variant_paths(&def) {
                if let Some(clip) = self.bank.get_or_load(vfs, &path) {
                    longest = longest.max(Some(clip.duration_ms()));
                }
            }
        }
        longest
    }

    pub fn fade_out(&self, id: VoiceId, ms: u32) {
        self.engine.fade_out(id, ms);
    }

    #[must_use]
    pub fn is_music(&self, name: &str, depth: u8) -> bool {
        if let Some(def) = self.library.get(name) {
            return def.kind == AudioEventKind::Music;
        }
        depth < MAX_MULTISOUND_DEPTH
            && self
                .library
                .multisound(name)
                .and_then(|multi| multi.subsounds.first())
                .is_some_and(|first| self.is_music(&first.name, depth.saturating_add(1)))
    }

    pub fn music_replace(&mut self, vfs: &Vfs, request: MusicRequest) {
        let mut ctx = Context {
            library: &self.library,
            vfs,
            rng: &mut self.rng,
        };
        self.music.replace(&self.engine, request, &mut ctx);
    }

    pub fn music_push(&mut self, vfs: &Vfs, request: MusicRequest) {
        let mut ctx = Context {
            library: &self.library,
            vfs,
            rng: &mut self.rng,
        };
        self.music.push(&self.engine, request, &mut ctx);
    }

    pub fn music_pop(&mut self, fade_out: bool, fade_in: bool) {
        self.music.pop(&self.engine, fade_out, fade_in);
    }

    pub fn music_stop(&mut self, fade: bool) {
        self.music.stop(&self.engine, fade);
    }

    pub fn music_update(&mut self, vfs: &Vfs, dt: f64) -> Vec<MusicEvent> {
        let mut ctx = Context {
            library: &self.library,
            vfs,
            rng: &mut self.rng,
        };
        self.music.update(&self.engine, dt, &mut ctx)
    }

    #[must_use]
    pub fn music(&self) -> &MusicSystem {
        &self.music
    }

    pub fn music_settle(&mut self, timeout: std::time::Duration) {
        self.music.settle(timeout);
    }

    fn resolve_def(&self, name: &str) -> Option<&AudioEventDef> {
        let mut name = name;
        for _ in 0..=MAX_MULTISOUND_DEPTH {
            if let Some(def) = self.library.get(name) {
                return Some(def);
            }
            name = &self.library.multisound(name)?.subsounds.first()?.name;
        }
        None
    }

    #[must_use]
    pub fn effective_def(&self, name: &str, overrides: &Overrides) -> Option<AudioEventDef> {
        let mut def = self.resolve_def(name)?.clone();
        overrides.apply(&mut def);
        Some(def)
    }

    #[must_use]
    pub fn effective_volume(&self, name: &str, position: [f32; 3], overrides: &Overrides) -> f32 {
        let Some(def) = self.resolve_def(name) else {
            return 0.0;
        };
        let priority = overrides.priority.unwrap_or(def.priority);
        let exempt = def.types.has(TypeFlags::GLOBAL) || priority == AudioPriority::Critical;
        let gain = overrides.volume.or(def.volume).unwrap_or(1.0);
        if exempt || !def.types.is_positional() {
            return gain;
        }
        let settings = self.library.settings();
        let min_range = overrides
            .min_range
            .or(def.min_range)
            .unwrap_or(settings.global_min_range);
        let spatial = Spatial {
            position,
            min_range,
            max_range: overrides
                .max_range
                .or(def.max_range)
                .unwrap_or(settings.global_max_range)
                .max(min_range),
            min_gain: overrides
                .min_volume
                .or(def.min_volume)
                .unwrap_or(0.0)
                .max(0.0),
        };
        let listener = self.listener.position;
        let d = [
            position[0] - listener[0],
            position[1] - listener[1],
            position[2] - listener[2],
        ];
        let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        mint_audio::mixer::spatial_level(self.rolloff, gain, &spatial, distance)
    }

    #[must_use]
    pub fn reach(&self, name: &str, position: [f32; 3], overrides: &Overrides) -> Reach {
        let Some(def) = self.resolve_def(name) else {
            return Reach::Unknown;
        };
        let priority = overrides.priority.unwrap_or(def.priority);
        let exempt = def.types.has(TypeFlags::GLOBAL) || priority == AudioPriority::Critical;
        if exempt || !def.types.is_positional() {
            return Reach::Audible;
        }
        let settings = self.library.settings();
        let min_range = overrides
            .min_range
            .or(def.min_range)
            .unwrap_or(settings.global_min_range);
        let spatial = Spatial {
            position,
            min_range,
            max_range: overrides
                .max_range
                .or(def.max_range)
                .unwrap_or(settings.global_max_range)
                .max(min_range),
            min_gain: overrides
                .min_volume
                .or(def.min_volume)
                .unwrap_or(0.0)
                .max(0.0),
        };
        let gain = overrides.volume.or(def.volume).unwrap_or(1.0);
        let floor = settings.min_sample_volume.max(0.0);
        if spatial.min_gain.min(gain) >= floor {
            return Reach::Audible;
        }
        let listener = self.listener.position;
        let d = [
            position[0] - listener[0],
            position[1] - listener[1],
            position[2] - listener[2],
        ];
        let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if distance >= spatial.max_range
            || mint_audio::mixer::spatial_level(self.rolloff, gain, &spatial, distance) < floor
        {
            Reach::OutOfRange
        } else {
            Reach::Audible
        }
    }

    fn play_nested(
        &mut self,
        vfs: &Vfs,
        request: &EventRequest<'_>,
        depth: u8,
    ) -> (Outcome, Option<VoiceId>) {
        if self.library.key(request.event).is_none() && depth < MAX_MULTISOUND_DEPTH {
            if let Some(multi) = self.library.multisound(request.event).cloned() {
                let chosen: Vec<&str> = if multi.play_one {
                    let weights: Vec<u32> = multi.subsounds.iter().map(|s| s.weight).collect();
                    self.rng
                        .next_weighted(&weights)
                        .and_then(|i| multi.subsounds.get(i))
                        .map(|s| s.name.as_str())
                        .into_iter()
                        .collect()
                } else {
                    multi.subsounds.iter().map(|s| s.name.as_str()).collect()
                };
                let mut result = (Outcome::NoVariants, None);
                for sub in chosen {
                    let nested = EventRequest {
                        event: sub,
                        ..request.clone()
                    };
                    let this = self.play_nested(vfs, &nested, depth.saturating_add(1));
                    if result.0 != Outcome::Queued {
                        result = this;
                    }
                }
                return result;
            }
        }
        if self.is_disabled(request.event) {
            self.stats.disabled = self.stats.disabled.saturating_add(1);
            return (Outcome::Disabled, None);
        }
        let Some(key) = self.library.key(request.event) else {
            self.stats.unknown = self.stats.unknown.saturating_add(1);
            return (Outcome::UnknownEvent, None);
        };
        let Some(mut def) = self.library.get(request.event).cloned() else {
            self.stats.unknown = self.stats.unknown.saturating_add(1);
            return (Outcome::UnknownEvent, None);
        };
        request.overrides.apply(&mut def);

        if def.kind != AudioEventKind::Music && !heard_locally(def.types, request.audience) {
            self.stats.not_for_local = self.stats.not_for_local.saturating_add(1);
            return (Outcome::NotForLocal, None);
        }

        let exempt = def.types.has(TypeFlags::GLOBAL) || def.priority == AudioPriority::Critical;
        let positional = def.types.is_positional() && request.position.is_some();

        if positional && !exempt && def.types.has(TypeFlags::SHROUDED) && !request.shroud_clear {
            self.stats.shrouded = self.stats.shrouded.saturating_add(1);
            return (Outcome::Shrouded, None);
        }

        if let Some(chance) = def.play_percent {
            if chance < 1.0 && self.rng.next_unit() >= chance {
                self.stats.play_percent = self.stats.play_percent.saturating_add(1);
                return (Outcome::PlayPercent, None);
            }
        }

        let Some(path) = self.choose_path(key, &def) else {
            self.stats.no_variants = self.stats.no_variants.saturating_add(1);
            return (Outcome::NoVariants, None);
        };

        let (clip, pending) = match self.bank.request(vfs, &path) {
            Lookup::Ready(clip) => (clip, None),
            Lookup::Pending(pending) => (Arc::clone(&self.placeholder), Some(pending)),
            Lookup::Absent => {
                self.stats.missing = self.stats.missing.saturating_add(1);
                return (Outcome::MissingFile, None);
            }
        };

        let mut play = Play::new(clip, key);
        play.priority = map_priority(def.priority);
        play.bus = def.effective_submix().bus();
        play.limit = def.limit;
        play.looping = def.control.has(ControlFlags::LOOP);
        play.interrupt = def.control.has(ControlFlags::INTERRUPT);
        play.owner = request.owner;
        play.exclusive_owner = def.types.has(TypeFlags::VOICE);
        play.ignore_distance_cull = exempt;
        play.stream = def.kind.is_streamed();

        let base = def.volume.unwrap_or(1.0);
        let shift = def.volume_shift.unwrap_or(0.0).min(0.0);
        play.gain = (base + self.rng.next_range(shift, 0.0)).max(0.0);

        if let Some((low, high)) = def.pitch_shift {
            play.pitch = (1.0 + self.rng.next_range(low, high)).max(0.01);
        }

        if positional {
            if let Some(position) = request.position {
                let settings = self.library.settings();
                let (min_range, max_range) = if def.types.has(TypeFlags::GLOBAL) {
                    (settings.global_min_range, settings.global_max_range)
                } else {
                    (
                        def.min_range.unwrap_or(settings.global_min_range),
                        def.max_range.unwrap_or(settings.global_max_range),
                    )
                };
                play.spatial = Some(Spatial {
                    position,
                    min_range,
                    max_range: max_range.max(min_range),
                    min_gain: def.min_volume.unwrap_or(0.0).max(0.0),
                });
            }
        }

        let id = match pending {
            Some(pending) => self.engine.play_pending(play, pending),
            None => self.engine.play(play),
        };
        self.stats.queued = self.stats.queued.saturating_add(1);
        (Outcome::Queued, Some(id))
    }

    fn choose_path(&mut self, key: EventKey, def: &AudioEventDef) -> Option<String> {
        if def.kind != AudioEventKind::Sound {
            return self.library.variant_path(def, "");
        }
        if def.sounds.is_empty() {
            return None;
        }
        let index = if def.control.has(ControlFlags::SEQUENTIAL) {
            let slot = self.cursor.entry(key.0).or_insert(0);
            let index = *slot % def.sounds.len();
            *slot = slot.wrapping_add(1);
            index
        } else {
            let weights: Vec<u32> = def.sounds.iter().map(|v| v.weight).collect();
            self.rng.next_weighted(&weights)?
        };
        let variant = def.sounds.get(index)?;
        self.library.variant_path(def, &variant.name)
    }

    #[must_use]
    pub fn readout(&self) -> String {
        format!(
            "audio: {} | rolloff {} | {} | {} | {} | {}",
            self.engine.describe(),
            self.rolloff.label(),
            self.stats.summary(),
            self.engine.stats().summary(),
            self.bank.stats().summary(),
            self.engine.device_stats().summary(),
        )
    }

    #[must_use]
    pub fn bank_stats(&self) -> BankStats {
        self.bank.stats()
    }
}

const MAX_MULTISOUND_DEPTH: u8 = 4;

fn heard_locally(types: TypeFlags, audience: Option<Audience>) -> bool {
    let gated = TypeFlags::PLAYER | TypeFlags::ALLIES | TypeFlags::ENEMIES | TypeFlags::EVERYONE;
    if !types.has(gated) || types.has(TypeFlags::EVERYONE) {
        return true;
    }
    let Some(audience) = audience else {
        return true;
    };
    if types.has(TypeFlags::PLAYER) {
        return audience == Audience::Local;
    }
    if types.has(TypeFlags::ALLIES) && audience == Audience::Ally {
        return true;
    }
    types.has(TypeFlags::ENEMIES) && audience == Audience::Enemy
}

fn mixer_config(library: &AudioLibrary) -> MixerConfig {
    let settings = library.settings();
    MixerConfig {
        voices_2d: usize::try_from(settings.sample_count_2d)
            .unwrap_or(4)
            .max(1),
        voices_3d: usize::try_from(settings.sample_count_3d)
            .unwrap_or(25)
            .max(1),
        voices_stream: usize::try_from(settings.stream_count).unwrap_or(3).max(1),
        min_sample_volume: settings.min_sample_volume.max(0.0),
        rolloff: Rolloff::default(),
    }
}

fn map_priority(priority: AudioPriority) -> Priority {
    match priority {
        AudioPriority::Lowest => Priority::Lowest,
        AudioPriority::Low => Priority::Low,
        AudioPriority::Normal => Priority::Normal,
        AudioPriority::High => Priority::High,
        AudioPriority::Critical => Priority::Critical,
    }
}
