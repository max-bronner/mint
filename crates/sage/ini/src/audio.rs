use std::collections::BTreeMap;

use crate::diagnostic::ValueErrors;
use crate::tree::{Block, Define, Document, Field};
use crate::value::{
    is_unexpanded_macro_call, parse_float, parse_float_or_expression, parse_float_or_macro,
    parse_int, parse_percent, resolve_percent, split_tag, MacroTable,
};

pub const AUDIO_EVENT_KEYWORD: &str = "AudioEvent";
pub const DIALOG_EVENT_KEYWORD: &str = "DialogEvent";
pub const MUSIC_TRACK_KEYWORD: &str = "MusicTrack";
pub const AMBIENT_STREAM_KEYWORD: &str = "AmbientStream";
pub const STREAMED_SOUND_KEYWORD: &str = "StreamedSound";
pub const AUDIO_SETTINGS_KEYWORD: &str = "AudioSettings";
pub const MISC_AUDIO_KEYWORD: &str = "MiscAudio";
pub const MULTISOUND_KEYWORD: &str = "Multisound";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MultisoundDef {
    pub name: String,
    pub subsounds: Vec<SoundVariant>,
    pub play_one: bool,
    pub looping: bool,
}

#[must_use]
pub fn read_multisound(block: &Block, errors: &ValueErrors) -> MultisoundDef {
    let mut def = MultisoundDef {
        name: block.name().unwrap_or_default().to_string(),
        ..MultisoundDef::default()
    };
    for field in block.fields() {
        if field.keyword.eq("Subsounds") {
            let context = def.name.clone();
            def.subsounds.extend(variants(&field.args, |token| {
                errors.value(
                    &context,
                    field,
                    token,
                    "a subsound with a whole-number weight",
                );
            }));
        } else if field.keyword.eq("Control") {
            let has = |flag: &str| field.args.iter().any(|a| a.eq_ignore_ascii_case(flag));
            def.play_one = has("PLAY_ONE");
            def.looping = has("LOOP");
        }
    }
    def
}

pub const CROWD_RESPONSE_KEYWORD: &str = "CrowdResponse";

pub const CROWD_RESPONSE_FIELDS: [&str; 5] = [
    "VoiceAttack",
    "VoiceAttackCharge",
    "VoiceMove",
    "VoiceSelect",
    "VoiceGarrison",
];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrowdThresholdDef {
    pub minimum: u32,
    pub fields: BTreeMap<&'static str, String>,
    pub unit_specific_sounds: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrowdResponseDef {
    pub name: String,
    pub weight: u32,
    pub thresholds: Vec<CrowdThresholdDef>,
}

impl CrowdResponseDef {
    #[must_use]
    pub fn threshold_for(&self, count: u32) -> Option<&CrowdThresholdDef> {
        self.thresholds
            .iter()
            .filter(|t| t.minimum <= count)
            .max_by_key(|t| t.minimum)
    }
}

#[must_use]
pub fn read_crowd_response(block: &Block, errors: &ValueErrors) -> CrowdResponseDef {
    let mut def = CrowdResponseDef {
        name: block.name().unwrap_or_default().to_string(),
        ..CrowdResponseDef::default()
    };
    for field in block.fields() {
        if !field.keyword.eq("Weight") {
            errors.field(&def.name, field, "a CrowdResponse field (Weight)");
        }
    }
    if let Some(field) = block.field("Weight") {
        match field
            .value()
            .and_then(parse_int)
            .and_then(|v| u32::try_from(v).ok())
        {
            Some(weight) => def.weight = weight,
            None => errors.field(&def.name, field, "a non-negative whole number"),
        }
    }
    let mut seen_minimums = std::collections::BTreeSet::new();
    for threshold_block in block.blocks_named("Threshold") {
        let minimum = threshold_block
            .name()
            .and_then(parse_int)
            .and_then(|v| u32::try_from(v).ok());
        let Some(minimum) = minimum else {
            errors.at(
                &threshold_block.file,
                threshold_block.line,
                &def.name,
                "Threshold",
                threshold_block.name().unwrap_or_default(),
                "a non-negative whole number",
            );
            continue;
        };
        if !seen_minimums.insert(minimum) {
            errors.at(
                &threshold_block.file,
                threshold_block.line,
                &def.name,
                "Threshold",
                &minimum.to_string(),
                "a minimum declared only once per crowd",
            );
            continue;
        }
        let mut threshold = CrowdThresholdDef {
            minimum,
            ..CrowdThresholdDef::default()
        };
        for field in threshold_block.fields() {
            let Some(name) = CROWD_RESPONSE_FIELDS
                .iter()
                .copied()
                .find(|candidate| field.keyword.eq(*candidate))
            else {
                errors.field(&def.name, field, "a crowd-response voice field");
                continue;
            };
            let Some(value) = field.value() else {
                errors.field(&def.name, field, "a sound event name");
                continue;
            };
            if value.starts_with("EVA:") {
                errors.field(&def.name, field, "a sound event, not an announcer event");
                continue;
            }
            threshold.fields.insert(name, value.to_string());
        }
        if let Some(sounds) = threshold_block.blocks_named("UnitSpecificSounds").last() {
            for field in sounds.fields() {
                if let Some(value) = field.value() {
                    threshold
                        .unit_specific_sounds
                        .entry(field.keyword.clone())
                        .or_insert_with(|| value.to_string());
                }
            }
        }
        def.thresholds.push(threshold);
    }
    def
}

pub const DEFAULT_DEFINITION_NAMES: [(&str, AudioEventKind); 4] = [
    ("DefaultSoundEffect", AudioEventKind::Sound),
    ("DefaultDialog", AudioEventKind::Dialog),
    ("DefaultMusicTrack", AudioEventKind::Music),
    ("DefaultAmbientStream", AudioEventKind::Ambient),
];

pub const DEFAULT_SOUND_WEIGHT: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum AudioEventKind {
    #[default]
    Sound,
    Dialog,
    Music,
    Ambient,
    Stream,
}

impl AudioEventKind {
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            AudioEventKind::Sound => AUDIO_EVENT_KEYWORD,
            AudioEventKind::Dialog => DIALOG_EVENT_KEYWORD,
            AudioEventKind::Music => MUSIC_TRACK_KEYWORD,
            AudioEventKind::Ambient => AMBIENT_STREAM_KEYWORD,
            AudioEventKind::Stream => STREAMED_SOUND_KEYWORD,
        }
    }

    #[must_use]
    pub fn is_streamed(self) -> bool {
        !matches!(self, AudioEventKind::Sound)
    }

    fn from_keyword(keyword: &str) -> Option<Self> {
        if keyword.eq(AUDIO_EVENT_KEYWORD) {
            Some(AudioEventKind::Sound)
        } else if keyword.eq(DIALOG_EVENT_KEYWORD) {
            Some(AudioEventKind::Dialog)
        } else if keyword.eq(MUSIC_TRACK_KEYWORD) {
            Some(AudioEventKind::Music)
        } else if keyword.eq(AMBIENT_STREAM_KEYWORD) {
            Some(AudioEventKind::Ambient)
        } else if keyword.eq(STREAMED_SOUND_KEYWORD) {
            Some(AudioEventKind::Stream)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum AudioPriority {
    Lowest,
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl AudioPriority {
    pub const NAMES: [(&'static str, AudioPriority); 5] = [
        ("lowest", AudioPriority::Lowest),
        ("low", AudioPriority::Low),
        ("normal", AudioPriority::Normal),
        ("high", AudioPriority::High),
        ("critical", AudioPriority::Critical),
    ];

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(name, _)| token.eq_ignore_ascii_case(name))
            .map(|(_, value)| *value)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AudioPriority::Lowest => "lowest",
            AudioPriority::Low => "low",
            AudioPriority::Normal => "normal",
            AudioPriority::High => "high",
            AudioPriority::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeFlags(pub u16);

impl TypeFlags {
    pub const UI: u16 = 1 << 0;
    pub const WORLD: u16 = 1 << 1;
    pub const SHROUDED: u16 = 1 << 2;
    pub const GLOBAL: u16 = 1 << 3;
    pub const VOICE: u16 = 1 << 4;
    pub const PLAYER: u16 = 1 << 5;
    pub const ALLIES: u16 = 1 << 6;
    pub const ENEMIES: u16 = 1 << 7;
    pub const EVERYONE: u16 = 1 << 8;

    pub const NAMES: [(&'static str, u16); 9] = [
        ("ui", Self::UI),
        ("world", Self::WORLD),
        ("shrouded", Self::SHROUDED),
        ("global", Self::GLOBAL),
        ("voice", Self::VOICE),
        ("player", Self::PLAYER),
        ("allies", Self::ALLIES),
        ("enemies", Self::ENEMIES),
        ("everyone", Self::EVERYONE),
    ];

    #[must_use]
    pub fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    #[must_use]
    pub fn is_positional(self) -> bool {
        self.has(Self::WORLD)
    }

    #[must_use]
    pub fn parse(args: &[String]) -> Self {
        let mut bits = 0u16;
        for token in args {
            let token = token.trim_start_matches(['+', '-']);
            if let Some((_, bit)) = Self::NAMES
                .iter()
                .find(|(name, _)| token.eq_ignore_ascii_case(name))
            {
                bits |= bit;
            }
        }
        Self(bits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ControlFlags(pub u16);

impl ControlFlags {
    pub const LOOP: u16 = 1 << 0;
    pub const INTERRUPT: u16 = 1 << 1;
    pub const SEQUENTIAL: u16 = 1 << 2;
    pub const FADE_ON_START: u16 = 1 << 3;
    pub const FADE_ON_KILL: u16 = 1 << 4;
    pub const RANDOM: u16 = 1 << 5;
    pub const ALL: u16 = 1 << 6;
    pub const POSTDELAY: u16 = 1 << 7;

    pub const NAMES: [(&'static str, u16); 8] = [
        ("loop", Self::LOOP),
        ("interrupt", Self::INTERRUPT),
        ("sequential", Self::SEQUENTIAL),
        ("fade_on_start", Self::FADE_ON_START),
        ("fade_on_kill", Self::FADE_ON_KILL),
        ("random", Self::RANDOM),
        ("all", Self::ALL),
        ("postdelay", Self::POSTDELAY),
    ];

    #[must_use]
    pub fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    #[must_use]
    pub fn parse(args: &[String]) -> Self {
        let mut bits = 0u16;
        for token in args {
            let token = token.trim_start_matches(['+', '-']);
            if let Some((_, bit)) = Self::NAMES
                .iter()
                .find(|(name, _)| token.eq_ignore_ascii_case(name))
            {
                bits |= bit;
            }
        }
        Self(bits)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Submix {
    #[default]
    SoundFx,
    Voice,
    Ambient,
    Movie,
    Music,
    Unscaled,
}

impl Submix {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if token.eq_ignore_ascii_case("voice") {
            Some(Submix::Voice)
        } else if token.eq_ignore_ascii_case("soundfx") {
            Some(Submix::SoundFx)
        } else if token.eq_ignore_ascii_case("ambient") {
            Some(Submix::Ambient)
        } else if token.eq_ignore_ascii_case("movie") {
            Some(Submix::Movie)
        } else if token.eq_ignore_ascii_case("music") {
            Some(Submix::Music)
        } else if token.eq_ignore_ascii_case("none") {
            Some(Submix::Unscaled)
        } else {
            None
        }
    }

    #[must_use]
    pub fn bus(self) -> u8 {
        match self {
            Submix::SoundFx => 0,
            Submix::Voice => 1,
            Submix::Ambient => 2,
            Submix::Movie => 3,
            Submix::Music => 4,
            Submix::Unscaled => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundVariant {
    pub name: String,
    pub weight: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioEventDef {
    pub name: String,
    pub kind: AudioEventKind,
    pub sounds: Vec<SoundVariant>,
    pub attack: Vec<SoundVariant>,
    pub decay: Vec<SoundVariant>,
    pub filename: Option<String>,
    pub volume: Option<f32>,
    pub min_volume: Option<f32>,
    pub volume_shift: Option<f32>,
    pub pitch_shift: Option<(f32, f32)>,
    pub delay: Option<(u32, u32)>,
    pub limit: u16,
    pub loop_count: Option<u32>,
    pub priority: AudioPriority,
    pub types: TypeFlags,
    pub control: ControlFlags,
    pub min_range: Option<f32>,
    pub max_range: Option<f32>,
    pub low_pass_cutoff: Option<f32>,
    pub submix: Option<Submix>,
    pub volume_slider_multipliers: Vec<(Submix, f32)>,
    pub play_percent: Option<f32>,
    pub unresolved_range: bool,
}

impl AudioEventDef {
    #[must_use]
    pub fn effective_submix(&self) -> Submix {
        match (self.submix, self.kind) {
            (Some(submix), _) => submix,
            (None, AudioEventKind::Music) => Submix::Music,
            (None, AudioEventKind::Ambient) => Submix::Ambient,
            (None, _) => Submix::SoundFx,
        }
    }

    #[must_use]
    pub fn has_audio(&self) -> bool {
        !self.sounds.is_empty() || self.filename.is_some()
    }

    #[must_use]
    pub fn total_weight(&self) -> u64 {
        self.sounds
            .iter()
            .map(|variant| u64::from(variant.weight))
            .sum()
    }
}

#[must_use]
pub fn read_audio_event(block: &Block, errors: &ValueErrors) -> Option<AudioEventDef> {
    read_audio_event_with(block, None, &MacroTable::new(), errors)
}

#[must_use]
pub fn read_audio_event_with(
    block: &Block,
    base: Option<&AudioEventDef>,
    macros: &MacroTable,
    errors: &ValueErrors,
) -> Option<AudioEventDef> {
    let kind = AudioEventKind::from_keyword(&block.keyword)?;
    let mut def = match base {
        Some(base) => AudioEventDef {
            name: block.name().unwrap_or_default().to_string(),
            kind,
            ..base.clone()
        },
        None => AudioEventDef {
            name: block.name().unwrap_or_default().to_string(),
            kind,
            ..AudioEventDef::default()
        },
    };

    let context = def.name.clone();
    let refuse = |field: &Field, expected: &'static str| errors.field(&context, field, expected);
    let percent = |field: &Field| {
        let value = resolve_percent(&field.args, macros);
        if value.is_none() {
            refuse(field, "a percentage");
        }
        value
    };
    let two_numbers = |field: &Field| {
        let value = pair(&field.args, macros);
        if value.is_none() {
            refuse(field, "one or two numbers");
        }
        value
    };
    let list = |field: &Field| {
        variants(&field.args, |token| {
            errors.value(&context, field, token, "a sound with a whole-number weight");
        })
    };
    for field in block.fields() {
        let keyword = field.keyword.as_str();
        if keyword.eq("Sounds") {
            def.sounds = list(field);
        } else if keyword.eq("Attack") {
            def.attack = list(field);
        } else if keyword.eq("Decay") {
            def.decay = list(field);
        } else if keyword.eq("Filename") {
            def.filename = field.value().map(str::to_string);
        } else if keyword.eq("Volume") {
            def.volume = percent(field);
        } else if keyword.eq("MinVolume") {
            def.min_volume = percent(field);
        } else if keyword.eq("VolumeShift") {
            def.volume_shift = percent(field);
        } else if keyword.eq("VolumeSliderMultiplier") {
            match slider_multiplier(&field.args) {
                Some(multiplier) => def.volume_slider_multipliers.push(multiplier),
                None => refuse(field, "`Slider:<submix> Multiplier:<number>`"),
            }
        } else if keyword.eq("PlayPercent") {
            def.play_percent = percent(field);
        } else if keyword.eq("PitchShift") {
            def.pitch_shift = two_numbers(field).map(|(a, b)| (a / 100.0, b / 100.0));
        } else if keyword.eq("Delay") {
            def.delay = two_numbers(field).map(|(a, b)| (clamp_ms(a), clamp_ms(b)));
        } else if keyword.eq("Limit") {
            match field
                .value()
                .and_then(parse_int)
                .and_then(|v| u16::try_from(v).ok())
            {
                Some(limit) => def.limit = limit,
                None => refuse(field, "a whole number from 0 to 65535"),
            }
        } else if keyword.eq("LoopCount") {
            def.loop_count = field
                .value()
                .and_then(parse_int)
                .and_then(|v| u32::try_from(v).ok());
            if def.loop_count.is_none() {
                refuse(field, "a non-negative whole number");
            }
        } else if keyword.eq("Priority") {
            match field.value().and_then(AudioPriority::parse) {
                Some(priority) => def.priority = priority,
                None => refuse(field, "a priority"),
            }
        } else if keyword.eq("Type") {
            def.types = TypeFlags(apply_bits(def.types.0, &field.args, &TypeFlags::NAMES));
        } else if keyword.eq("Control") {
            def.control =
                ControlFlags(apply_bits(def.control.0, &field.args, &ControlFlags::NAMES));
        } else if keyword.eq("MinRange") || keyword.eq("MaxRange") {
            let slot = if keyword.eq("MinRange") {
                &mut def.min_range
            } else {
                &mut def.max_range
            };
            match range(field, macros) {
                Range::Value(value) => *slot = Some(value),
                Range::Unresolved => def.unresolved_range = true,
                Range::Invalid => refuse(field, "a number or a macro"),
            }
        } else if keyword.eq("LowPassCutoff") {
            def.low_pass_cutoff = field.value().and_then(parse_float);
            if def.low_pass_cutoff.is_none() {
                refuse(field, "a number");
            }
        } else if keyword.eq("SubmixSlider") {
            def.submix = field.value().and_then(Submix::parse);
            if def.submix.is_none() {
                refuse(field, "a submix");
            }
        }
    }

    Some(def)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettingsDef {
    pub audio_root: String,
    pub sounds_folder: String,
    pub music_folder: String,
    pub streaming_folder: String,
    pub ambient_stream_folder: String,
    pub sounds_extension: String,
    pub music_script_library_name: String,
    pub sample_count_2d: u32,
    pub sample_count_3d: u32,
    pub stream_count: u32,
    pub output_rate: u32,
    pub output_channels: u32,
    pub min_sample_volume: f32,
    pub global_min_range: f32,
    pub global_max_range: f32,
    pub audio_footprint_in_bytes: u64,
    pub time_to_fade_audio_ms: u32,
    pub mixahead_latency_during_movies_ms: u32,
    pub ambient_stream_hysteresis: f32,
    pub default_sound_volume: f32,
    pub default_voice_volume: f32,
    pub default_ambient_volume: f32,
    pub default_movie_volume: f32,
    pub default_music_volume: f32,
    pub microphone: MicrophoneDef,
    pub voice_move_to_camp_max_campness_at_start_point: f32,
    pub voice_move_to_camp_min_campness_at_end_point: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MicrophoneDef {
    pub preferred_fraction_camera_to_ground: Option<f32>,
    pub min_distance_to_camera: Option<f32>,
    pub max_distance_to_camera: Option<f32>,
    pub pull_towards_look_at: Option<f32>,
    pub zoom_min_distance: Option<f32>,
    pub zoom_max_distance: Option<f32>,
    pub zoom_sound_volume_amount: Option<f32>,
}

impl Default for AudioSettingsDef {
    fn default() -> Self {
        Self {
            audio_root: "Data\\Audio".to_string(),
            sounds_folder: "Sounds".to_string(),
            music_folder: "Tracks".to_string(),
            streaming_folder: "Speech".to_string(),
            ambient_stream_folder: "AmbientStreams".to_string(),
            sounds_extension: "wav".to_string(),
            music_script_library_name: String::new(),
            sample_count_2d: 4,
            sample_count_3d: 25,
            stream_count: 3,
            output_rate: 44_100,
            output_channels: 2,
            min_sample_volume: 0.02,
            global_min_range: 5_000.0,
            global_max_range: 5_000_000.0,
            audio_footprint_in_bytes: 8_388_608,
            time_to_fade_audio_ms: 2_000,
            mixahead_latency_during_movies_ms: 700,
            ambient_stream_hysteresis: 0.10,
            default_sound_volume: 0.70,
            default_voice_volume: 0.70,
            default_ambient_volume: 0.50,
            default_movie_volume: 0.70,
            default_music_volume: 0.70,
            microphone: MicrophoneDef::default(),
            voice_move_to_camp_max_campness_at_start_point: 0.0,
            voice_move_to_camp_min_campness_at_end_point: 5_000.0,
        }
    }
}

impl AudioSettingsDef {
    #[must_use]
    pub fn folder_for(&self, kind: AudioEventKind) -> String {
        let leaf = match kind {
            AudioEventKind::Dialog => &self.streaming_folder,
            AudioEventKind::Music => &self.music_folder,
            AudioEventKind::Ambient => &self.ambient_stream_folder,
            AudioEventKind::Sound | AudioEventKind::Stream => &self.sounds_folder,
        };
        let root = self.audio_root.replace('\\', "/");
        format!("{}/{}", root.trim_end_matches('/'), leaf.trim_matches('/'))
    }
}

#[must_use]
pub fn read_audio_settings(block: &Block, errors: &ValueErrors) -> AudioSettingsDef {
    let mut settings = AudioSettingsDef::default();
    let text = |key: &str| block.field(key).and_then(Field::value).map(str::to_string);
    let number = |key: &str| settings_value(block, key, errors, "a number", parse_float);
    let count = |key: &str| {
        settings_value(block, key, errors, "a non-negative whole number", |t| {
            parse_int(t).and_then(|v| u32::try_from(v).ok())
        })
    };
    let fraction = |key: &str| settings_value(block, key, errors, "a percentage", parse_percent);

    if let Some(v) = text("AudioRoot") {
        settings.audio_root = v;
    }
    if let Some(v) = text("SoundsFolder") {
        settings.sounds_folder = v;
    }
    if let Some(v) = text("MusicFolder") {
        settings.music_folder = v;
    }
    if let Some(v) = text("StreamingFolder") {
        settings.streaming_folder = v;
    }
    if let Some(v) = text("AmbientStreamFolder") {
        settings.ambient_stream_folder = v;
    }
    if let Some(v) = text("SoundsExtension") {
        settings.sounds_extension = v;
    }
    if let Some(v) = text("MusicScriptLibraryName") {
        settings.music_script_library_name = v.trim_matches('"').to_string();
    }
    if let Some(v) = count("SampleCount2D") {
        settings.sample_count_2d = v;
    }
    if let Some(v) = count("SampleCount3D") {
        settings.sample_count_3d = v;
    }
    if let Some(v) = count("StreamCount") {
        settings.stream_count = v;
    }
    if let Some(v) = count("OutputRate") {
        settings.output_rate = v;
    }
    if let Some(v) = count("OutputChannels") {
        settings.output_channels = v;
    }
    if let Some(v) = number("MinSampleVolume") {
        settings.min_sample_volume = v / 100.0;
    }
    if let Some(v) = number("GlobalMinRange") {
        settings.global_min_range = v;
    }
    if let Some(v) = number("GlobalMaxRange") {
        settings.global_max_range = v;
    }
    if let Some(v) = settings_value(
        block,
        "AudioFootprintInBytes",
        errors,
        "a non-negative whole number",
        |t| parse_int(t).and_then(|v| u64::try_from(v).ok()),
    ) {
        settings.audio_footprint_in_bytes = v;
    }
    if let Some(v) = count("TimeToFadeAudio") {
        settings.time_to_fade_audio_ms = v;
    }
    if let Some(v) = count("MixaheadLatencyDuringMovies") {
        settings.mixahead_latency_during_movies_ms = v;
    }
    if let Some(v) = number("AmbientStreamHysteresisVolume") {
        settings.ambient_stream_hysteresis = v / 100.0;
    }
    if let Some(v) = fraction("DefaultSoundVolume") {
        settings.default_sound_volume = v;
    }
    if let Some(v) = fraction("DefaultVoiceVolume") {
        settings.default_voice_volume = v;
    }
    if let Some(v) = fraction("DefaultAmbientVolume") {
        settings.default_ambient_volume = v;
    }
    if let Some(v) = fraction("DefaultMovieVolume") {
        settings.default_movie_volume = v;
    }
    if let Some(v) = fraction("DefaultMusicVolume") {
        settings.default_music_volume = v;
    }
    settings.microphone = MicrophoneDef {
        preferred_fraction_camera_to_ground: fraction("MicrophonePreferredFractionCameraToGround"),
        min_distance_to_camera: number("MicrophoneMinDistanceToCamera"),
        max_distance_to_camera: number("MicrophoneMaxDistanceToCamera"),
        pull_towards_look_at: fraction("MicrophonePullTowardsTerrainLookAtPointPercent"),
        zoom_min_distance: number("ZoomMinDistance"),
        zoom_max_distance: number("ZoomMaxDistance"),
        zoom_sound_volume_amount: fraction("ZoomSoundVolumePercentageAmount"),
    };
    if let Some(v) = number("VoiceMoveToCampMaxCampnessAtStartPoint") {
        settings.voice_move_to_camp_max_campness_at_start_point = v;
    }
    if let Some(v) = number("VoiceMoveToCampMinCampnessAtEndPoint") {
        settings.voice_move_to_camp_min_campness_at_end_point = v;
    }
    settings
}

fn settings_value<T>(
    block: &Block,
    key: &str,
    errors: &ValueErrors,
    expected: &'static str,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    let field = block.field(key)?;
    let value = field.value().and_then(parse);
    if value.is_none() {
        errors.field(AUDIO_SETTINGS_KEYWORD, field, expected);
    }
    value
}

#[derive(Debug, Clone, Default)]
pub struct AudioIndex {
    events: BTreeMap<String, AudioEventDef>,
    multisounds: BTreeMap<String, MultisoundDef>,
    crowd_responses: BTreeMap<String, CrowdResponseDef>,
    misc: BTreeMap<String, String>,
    settings: AudioSettingsDef,
    defaults: [Option<AudioEventDef>; 5],
    macros: MacroTable,
    definitions: usize,
    redefinitions: usize,
    saw_settings: bool,
    errors: ValueErrors,
}

impl AudioIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_defines(&mut self, defines: &[Define]) {
        self.macros.insert_defines(defines);
    }

    #[must_use]
    pub fn macros(&self) -> usize {
        self.macros.len()
    }

    pub fn insert_document(&mut self, document: &Document) {
        for block in document.roots() {
            self.insert_block(block);
        }
    }

    pub fn insert_block(&mut self, block: &Block) {
        if block.keyword.eq(AUDIO_SETTINGS_KEYWORD) {
            self.settings = read_audio_settings(block, &self.errors);
            self.saw_settings = true;
            return;
        }
        if block.keyword.eq(MISC_AUDIO_KEYWORD) {
            for field in block.fields() {
                if let Some(value) = field.value() {
                    self.misc
                        .insert(field.keyword.to_ascii_lowercase(), value.to_string());
                }
            }
            return;
        }
        if block.keyword.eq(MULTISOUND_KEYWORD) {
            let def = read_multisound(block, &self.errors);
            if !def.name.is_empty() {
                self.multisounds.insert(def.name.to_ascii_lowercase(), def);
            }
            return;
        }
        if block.keyword.eq(CROWD_RESPONSE_KEYWORD) {
            let def = read_crowd_response(block, &self.errors);
            if !def.name.is_empty() {
                self.crowd_responses
                    .insert(def.name.to_ascii_lowercase(), def);
            }
            return;
        }
        let Some(kind) = AudioEventKind::from_keyword(&block.keyword) else {
            return;
        };
        let base = self.defaults.get(kind as usize).and_then(Option::as_ref);
        let Some(def) = read_audio_event_with(block, base, &self.macros, &self.errors) else {
            return;
        };
        if def.name.is_empty() {
            return;
        }
        if let Some((_, default_kind)) = DEFAULT_DEFINITION_NAMES
            .iter()
            .find(|(name, _)| def.name.eq_ignore_ascii_case(name))
        {
            if *default_kind == kind {
                if let Some(slot) = self.defaults.get_mut(kind as usize) {
                    *slot = Some(def.clone());
                }
            }
        }
        self.definitions = self.definitions.saturating_add(1);
        if self
            .events
            .insert(def.name.to_ascii_lowercase(), def)
            .is_some()
        {
            self.redefinitions = self.redefinitions.saturating_add(1);
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AudioEventDef> {
        self.events.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn multisound(&self, name: &str) -> Option<&MultisoundDef> {
        self.multisounds.get(&name.to_ascii_lowercase())
    }

    #[must_use]
    pub fn crowd_response(&self, name: &str) -> Option<&CrowdResponseDef> {
        self.crowd_responses.get(&name.to_ascii_lowercase())
    }

    pub fn events(&self) -> impl Iterator<Item = (&str, &AudioEventDef)> {
        self.events.iter().map(|(k, v)| (k.as_str(), v))
    }

    #[must_use]
    pub fn settings(&self) -> &AudioSettingsDef {
        &self.settings
    }

    #[must_use]
    pub fn default_for(&self, kind: AudioEventKind) -> Option<&AudioEventDef> {
        self.defaults.get(kind as usize).and_then(Option::as_ref)
    }

    #[must_use]
    pub fn saw_settings(&self) -> bool {
        self.saw_settings
    }

    #[must_use]
    pub fn misc(&self, key: &str) -> Option<&str> {
        self.misc.get(&key.to_ascii_lowercase()).map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    #[must_use]
    pub fn definitions(&self) -> usize {
        self.definitions
    }

    #[must_use]
    pub fn redefinitions(&self) -> usize {
        self.redefinitions
    }

    #[must_use]
    pub const fn value_errors(&self) -> &ValueErrors {
        &self.errors
    }
}

fn apply_bits(current: u16, args: &[String], names: &[(&str, u16)]) -> u16 {
    let modifying = args
        .first()
        .is_some_and(|t| t.starts_with('+') || t.starts_with('-'));
    let mut bits = if modifying { current } else { 0 };
    for token in args {
        let remove = token.starts_with('-');
        let name = token.trim_start_matches(['+', '-']);
        if let Some((_, bit)) = names
            .iter()
            .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
        {
            if remove {
                bits &= !bit;
            } else {
                bits |= bit;
            }
        }
    }
    bits
}

fn variants(args: &[String], mut refuse: impl FnMut(&str)) -> Vec<SoundVariant> {
    let mut out = Vec::new();
    for token in args {
        if token.is_empty() {
            continue;
        }
        let (name, weight) = match token.split_once(':') {
            Some((name, weight)) => (
                name,
                match weight.parse::<u32>() {
                    Ok(weight) if weight > 0 => weight,
                    Ok(_) => DEFAULT_SOUND_WEIGHT,
                    Err(_) => {
                        refuse(token);
                        DEFAULT_SOUND_WEIGHT
                    }
                },
            ),
            None => (token.as_str(), DEFAULT_SOUND_WEIGHT),
        };
        if name.is_empty() {
            continue;
        }
        out.push(SoundVariant {
            name: name.to_string(),
            weight,
        });
    }
    out
}

fn slider_multiplier(args: &[String]) -> Option<(Submix, f32)> {
    let [slider, multiplier] = args else {
        return None;
    };
    let slider = split_tag(slider)
        .filter(|(tag, _)| tag.eq_ignore_ascii_case("Slider"))
        .and_then(|(_, name)| Submix::parse(name))?;
    let multiplier = split_tag(multiplier)
        .filter(|(tag, _)| tag.eq_ignore_ascii_case("Multiplier"))
        .and_then(|(_, value)| parse_float(value))?;
    Some((slider, multiplier))
}

fn pair(args: &[String], macros: &MacroTable) -> Option<(f32, f32)> {
    let mut tokens = args.iter();
    let mut next = || -> Option<Option<f32>> {
        let token = tokens.next()?;
        if !is_unexpanded_macro_call(token) {
            return Some(parse_float_or_macro(token, macros));
        }
        let mut call = token.clone();
        while !call.ends_with(')') {
            let Some(more) = tokens.next() else {
                return Some(None);
            };
            call.push(' ');
            call.push_str(more);
        }
        Some(parse_float_or_expression(&call, macros))
    };
    let first = next()??;
    let second = match next() {
        None => first,
        Some(value) => value?,
    };
    match next() {
        None => Some((first, second)),
        Some(_) => None,
    }
}

fn clamp_ms(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f32::from(u16::MAX) * 16.0 {
        return u32::MAX;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        value as u32
    }
}

enum Range {
    Value(f32),
    Unresolved,
    Invalid,
}

fn range(field: &Field, macros: &MacroTable) -> Range {
    let Some(token) = field.value() else {
        return Range::Invalid;
    };
    if let Some(value) = parse_float_or_macro(token, macros) {
        return Range::Value(value);
    }
    let identifier = token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if identifier || is_unexpanded_macro_call(token) {
        Range::Unresolved
    } else {
        Range::Invalid
    }
}

#[cfg(test)]
mod tests;
