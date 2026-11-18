use std::collections::BTreeMap;
use std::sync::Arc;

use mint_app::RtsCamera;
use mint_render::Vec3;
use mint_sage_audio::{Audience, AudioLibrary, EventRequest, Outcome, Rolloff, SageAudio};
use mint_sage_model::ObjectIndex;
use mint_sage_scene::Conditions;
use mint_vfs::Vfs;

use crate::render_state::UnitView;

const VOICE_SELECT: &str = "VoiceSelect";
const VOICE_SELECT_UNDER_CONSTRUCTION: &str = "VoiceSelectUnderConstruction";
const VOICE_SELECT_BATTLE: &str = "VoiceSelectBattle";
const VOICE_MOVE: &str = "VoiceMove";
const VOICE_ATTACK: &str = "VoiceAttack";
const VOICE_CREATED: &str = "VoiceCreated";
const VOICE_FULLY_CREATED: &str = "VoiceFullyCreated";
const SOUND_IMPACT: &str = "SoundImpact";
const SOUND_IMPACT_CYCLONIC: &str = "SoundImpactCyclonic";
const SOUND_ON_DAMAGED: &str = "SoundOnDamaged";
const SOUND_ON_REALLY_DAMAGED: &str = "SoundOnReallyDamaged";
const VOICE_FEAR: &str = "VoiceFear";
const FEAR_FRACTION: f32 = 0.35;
const SOUND_MOVE_START: &str = "SoundMoveStart";
const SOUND_MOVE_START_DAMAGED: &str = "SoundMoveStartDamaged";
const SOUND_MOVE_LOOP: &str = "SoundMoveLoop";
const SOUND_MOVE_LOOP_DAMAGED: &str = "SoundMoveLoopDamaged";

const UNDER_CONSTRUCTION: &str = "UnderConstruction";
const UNDER_REPAIR_FROM_RUBBLE: &str = "UnderRepairFromRubble";
const UNDER_REPAIR_FROM_DAMAGE: &str = "UnderRepairFromDamage";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Flags {
    selected: bool,
    dying: bool,
    moving: bool,
    steady: bool,
    damaged: bool,
    really_damaged: bool,
    rubble: bool,
    building: bool,
    rebuild: bool,
    repairing: bool,
    kind: Option<u16>,
    entity: Option<mint_sim::Entity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Start { id: u32, field: &'static str },
    Stop { id: u32, flags: Flags },
}

fn transitions(previous: &[(u32, Flags)], now: &[(u32, Flags)]) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut p = previous.iter().peekable();
    let mut n = now.iter().peekable();
    loop {
        match (p.peek(), n.peek()) {
            (Some(&&(pid, pflags)), Some(&&(nid, nflags))) => match pid.cmp(&nid) {
                std::cmp::Ordering::Less => {
                    if pflags.building {
                        changes.push(Change::Stop {
                            id: pid,
                            flags: pflags,
                        });
                    }
                    p.next();
                }
                std::cmp::Ordering::Greater => {
                    if nflags.building {
                        changes.push(Change::Start {
                            id: nid,
                            field: construction_loop_field(nflags),
                        });
                    }
                    n.next();
                }
                std::cmp::Ordering::Equal => {
                    let same_owner = pflags.entity == nflags.entity;
                    if !same_owner && pflags.building {
                        changes.push(Change::Stop {
                            id: pid,
                            flags: pflags,
                        });
                    }
                    if nflags.building && !(same_owner && pflags.building) {
                        changes.push(Change::Start {
                            id: nid,
                            field: construction_loop_field(nflags),
                        });
                    } else if same_owner && pflags.building && !nflags.building {
                        changes.push(Change::Stop {
                            id: pid,
                            flags: pflags,
                        });
                    }
                    p.next();
                    n.next();
                }
            },
            (Some(&&(pid, pflags)), None) => {
                if pflags.building {
                    changes.push(Change::Stop {
                        id: pid,
                        flags: pflags,
                    });
                }
                p.next();
            }
            (None, Some(&&(nid, nflags))) => {
                if nflags.building {
                    changes.push(Change::Start {
                        id: nid,
                        field: construction_loop_field(nflags),
                    });
                }
                n.next();
            }
            (None, None) => break,
        }
    }
    changes
}

fn construction_loop_field(flags: Flags) -> &'static str {
    if flags.repairing {
        UNDER_REPAIR_FROM_DAMAGE
    } else if flags.rebuild {
        UNDER_REPAIR_FROM_RUBBLE
    } else {
        UNDER_CONSTRUCTION
    }
}

fn select_voice_field(conditions: Conditions) -> &'static str {
    if conditions.contains(Conditions::ENGAGED) {
        VOICE_SELECT_BATTLE
    } else if conditions.contains(Conditions::ACTIVELY_BEING_CONSTRUCTED)
        || conditions.contains(Conditions::PARTIALLY_CONSTRUCTED)
        || conditions.contains(Conditions::AWAITING_CONSTRUCTION)
    {
        VOICE_SELECT_UNDER_CONSTRUCTION
    } else {
        VOICE_SELECT
    }
}

struct WithAnnouncer<'a> {
    world: &'a dyn crate::music_script::MusicWorld,
    announcer: &'a crate::announcer::Announcer,
}

impl crate::music_script::MusicWorld for WithAnnouncer<'_> {
    fn local_side(&self) -> Option<&str> {
        self.world.local_side()
    }
    fn home_base(&self, who: crate::music_script::Who) -> Option<[f32; 2]> {
        self.world.home_base(who)
    }
    fn units_beyond(&self, who: crate::music_script::Who, at: [f32; 2], distance: f32) -> u32 {
        self.world.units_beyond(who, at, distance)
    }
    fn has_science(&self, who: crate::music_script::Who, science: &str) -> bool {
        self.world.has_science(who, science)
    }
    fn objects_with_condition(
        &self,
        who: crate::music_script::Who,
        condition: &str,
    ) -> Option<u32> {
        self.world.objects_with_condition(who, condition)
    }
    fn units_near(&self, who: crate::music_script::Who, at: [f32; 2], distance: f32) -> u32 {
        self.world.units_near(who, at, distance)
    }
    fn eva_last_played(&self, event: &str) -> Option<Option<(u32, [f32; 2])>> {
        self.announcer.last_played(event)
    }
    fn reached_level_cap(&self, who: crate::music_script::Who) -> bool {
        self.world.reached_level_cap(who)
    }
}

const EVA_PREFIX: &str = "EVA:";

fn pick_damaged_or_plain(
    plain: &'static str,
    damaged: &'static str,
    damaged_now: bool,
    damaged_declared: bool,
) -> &'static str {
    if damaged_now && damaged_declared {
        damaged
    } else {
        plain
    }
}

fn pick_by_total<K: PartialEq + Clone>(entries: impl IntoIterator<Item = (K, u32)>) -> Option<K> {
    let mut totals: Vec<(K, u32)> = Vec::new();
    for (key, weight) in entries {
        match totals.iter_mut().find(|(k, _)| *k == key) {
            Some((_, total)) => *total = total.saturating_add(weight),
            None => totals.push((key, weight)),
        }
    }
    let mut best: Option<(K, u32)> = None;
    for (key, total) in totals {
        let replace = best.as_ref().is_none_or(|(_, b)| total > *b);
        if replace {
            best = Some((key, total));
        }
    }
    best.map(|(key, _)| key)
}

pub(crate) fn owner_id(entity: mint_sim::Entity) -> u64 {
    (u64::from(entity.index()) << 32) | u64::from(entity.generation())
}

pub(crate) fn sound_lengths(vfs: &Vfs, names: &[String]) -> BTreeMap<String, u32> {
    let Ok(library) = mint_sage_audio::load_library(vfs) else {
        return BTreeMap::new();
    };
    names
        .iter()
        .filter(|name| library.get(name).is_some())
        .map(|name| {
            let ms = library.event_duration_ms(vfs, name).unwrap_or(0);
            (
                name.to_ascii_lowercase(),
                u32::try_from(ms).unwrap_or(u32::MAX),
            )
        })
        .collect()
}

fn load_weather_sounds(vfs: &Vfs) -> BTreeMap<String, mint_sage_ini::WeatherSound> {
    let path = mint_sage_scene::snow_present::WEATHER_INI;
    let Ok(bytes) = vfs.read(path) else {
        println!("audio: {path} did not load - no weather sound");
        return BTreeMap::new();
    };
    let table = mint_sage_ini::BlockTable::corpus_path();
    match mint_sage_ini::Document::parse(path, &String::from_utf8_lossy(&bytes), &table) {
        Ok(document) => mint_sage_ini::weather_sounds(std::slice::from_ref(&document)),
        Err(error) => {
            println!("audio: {path} did not parse ({error}) - no weather sound");
            BTreeMap::new()
        }
    }
}

fn weather_kind_token(kind: mint_sage_modules::WeatherKind) -> &'static str {
    match kind {
        mint_sage_modules::WeatherKind::None => "NONE",
        mint_sage_modules::WeatherKind::Cloudy => "CLOUDY",
        mint_sage_modules::WeatherKind::Rainy => "RAINY",
        mint_sage_modules::WeatherKind::CloudyRainy => "CLOUDYRAINY",
        mint_sage_modules::WeatherKind::Sunny => "SUNNY",
    }
}

fn weather_sound_for(
    sounds: &BTreeMap<String, mint_sage_ini::WeatherSound>,
    kind: mint_sage_modules::WeatherKind,
) -> Option<&str> {
    sounds
        .get(weather_kind_token(kind))
        .and_then(|sound| sound.sound.as_deref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WeatherSoundChange<'a> {
    None,
    Stop,
    Play(&'a str),
}

fn weather_sound_change<'a>(
    cached: mint_sage_modules::WeatherKind,
    kind: mint_sage_modules::WeatherKind,
    holding: bool,
    sounds: &'a BTreeMap<String, mint_sage_ini::WeatherSound>,
) -> WeatherSoundChange<'a> {
    if cached == kind {
        return WeatherSoundChange::None;
    }
    match weather_sound_for(sounds, kind) {
        Some(name) => WeatherSoundChange::Play(name),
        None if holding => WeatherSoundChange::Stop,
        None => WeatherSoundChange::None,
    }
}

fn load_music_scripts(
    vfs: &Vfs,
    library: &AudioLibrary,
) -> Option<crate::music_script::MusicScripts> {
    let name = &library.settings().music_script_library_name;
    if name.is_empty() {
        println!("music: AudioSettings names no MusicScriptLibraryName - no in-game music");
        return None;
    }
    let path = name.replace('\\', "/").to_ascii_lowercase();
    let tree = vfs
        .read(&path)
        .ok()
        .and_then(|bytes| mint_sage_map::read_script_library(&bytes).ok().flatten());
    let Some(tree) = tree else {
        println!("music: {path} did not load - no in-game music");
        return None;
    };
    let scripts =
        crate::music_script::MusicScripts::new(tree, mint_sage_audio::ClientRng::from_clock());
    println!("music: {} script(s) from {path}", scripts.len());
    Some(scripts)
}

fn load_eva(
    vfs: &Vfs,
    macros: &mint_sage_ini::MacroTable,
) -> (
    mint_sage_ini::eva::EvaDefs,
    mint_sage_ini::eva::JumpSettings,
) {
    let table = mint_sage_ini::table::BlockTable::corpus_path();
    let mut documents = Vec::new();
    for path in [
        "data/ini/default/eva.ini",
        "data/ini/eva.ini",
        "data/ini/scoredkillevaannouncer.ini",
    ] {
        let Ok(bytes) = vfs.read(path) else {
            println!("eva: {path} did not load");
            continue;
        };
        match mint_sage_ini::Document::parse(path, &String::from_utf8_lossy(&bytes), &table) {
            Ok(document) => documents.push(document),
            Err(error) => println!("eva: {path} did not parse ({error})"),
        }
    }
    let defs = mint_sage_ini::eva::EvaDefs::read(&documents, macros);
    if !defs.malformed.is_empty() {
        println!(
            "eva: {} field value(s) did not parse and were left at their default",
            defs.malformed.len()
        );
        for error in &defs.malformed {
            crate::diag::diag!("eva: {error}");
        }
    }
    let jump_settings = mint_sage_ini::eva::read_jump_settings(&documents);
    (defs, jump_settings)
}

pub(crate) fn names_a_sound(event: &str) -> bool {
    !event.eq_ignore_ascii_case("NoSound")
}

fn looping_of(audio: &SageAudio, event: &str, overrides: &mint_sage_audio::Overrides) -> bool {
    audio
        .effective_def(event, overrides)
        .is_some_and(|def| def.control.has(mint_sage_ini::audio::ControlFlags::LOOP))
}

fn delay_of(
    audio: &SageAudio,
    event: &str,
    overrides: &mint_sage_audio::Overrides,
) -> Option<(u32, u32)> {
    audio.effective_def(event, overrides)?.delay
}

pub(crate) struct GameAudio {
    audio: SageAudio,
    vfs: Arc<Vfs>,
    previous: Vec<(u32, Flags)>,
    scratch: Vec<(u32, Flags)>,
    voices: BTreeMap<(u16, &'static str), String>,
    enabled: bool,
    music_scripts: Option<crate::music_script::MusicScripts>,
    announcer: crate::announcer::Announcer,
    jump: crate::radar_jump::JumpList,
    line_lengths: BTreeMap<String, u32>,
    ambients: crate::ambient::Emitters,
    names: Vec<String>,
    unit_sounds: BTreeMap<(u16, String), String>,
    crowd_keys: BTreeMap<u16, String>,
    charge: crate::voice_chain::ChargeClocks,
    charge_timeouts: Vec<u64>,
    custom_ambients: BTreeMap<u64, crate::ambient::Custom>,
    enter_state_playing: BTreeMap<u32, String>,
    stream_markers: BTreeMap<u64, (String, [f32; 3])>,
    stream_states: BTreeMap<String, crate::ambient_stream::Stream>,
    stream_voices: BTreeMap<String, mint_sage_audio::VoiceId>,
    max_ambient_streams: u32,
    weather_sounds: BTreeMap<String, mint_sage_ini::WeatherSound>,
    weather_cached: mint_sage_modules::WeatherKind,
    weather_voice: Option<mint_sage_audio::VoiceId>,
    script: script_ops::ScriptState,
}

impl GameAudio {
    pub(crate) fn open(
        vfs: Arc<Vfs>,
        object_index: Option<&ObjectIndex>,
        templates: Vec<String>,
        rules: &crate::rules::RotwkRules,
        macros: &mint_sage_ini::MacroTable,
    ) -> Self {
        let library = match mint_sage_audio::load_library(&vfs) {
            Ok(library) => library,
            Err(error) => {
                println!("audio: could not read the audio INI ({error}) - running silent");
                AudioLibrary::default()
            }
        };
        let events = library.len();
        let audio = SageAudio::open_named(library, "match");

        let mut voices = BTreeMap::new();
        let mut unit_sounds = BTreeMap::new();
        let mut crowd_keys = BTreeMap::new();
        if let Some(index) = object_index {
            for (slot, name) in templates.iter().enumerate() {
                let Ok(kind) = u16::try_from(slot) else {
                    continue;
                };
                if let Some(key) = index.crowd_response_key(name) {
                    crowd_keys.insert(kind, key.to_string());
                }
                for field in [
                    VOICE_SELECT,
                    VOICE_SELECT_UNDER_CONSTRUCTION,
                    VOICE_SELECT_BATTLE,
                    VOICE_MOVE,
                    VOICE_ATTACK,
                    VOICE_CREATED,
                    VOICE_FULLY_CREATED,
                    SOUND_IMPACT,
                    SOUND_IMPACT_CYCLONIC,
                    SOUND_ON_DAMAGED,
                    SOUND_ON_REALLY_DAMAGED,
                    VOICE_FEAR,
                    SOUND_MOVE_START,
                    SOUND_MOVE_START_DAMAGED,
                    SOUND_MOVE_LOOP,
                    SOUND_MOVE_LOOP_DAMAGED,
                    crate::ambient::SOUND_AMBIENT,
                    crate::ambient::SOUND_AMBIENT_DAMAGED,
                    crate::ambient::SOUND_AMBIENT_REALLY_DAMAGED,
                    crate::ambient::SOUND_AMBIENT_RUBBLE,
                    crate::ambient::SOUND_AMBIENT_BATTLE,
                ]
                .into_iter()
                .chain(crate::voice_chain::OBJECT_FIELDS)
                {
                    if let Some(event) = index.sound(name, field).filter(|e| names_a_sound(e)) {
                        voices.insert((kind, field), event.to_string());
                    }
                }
                for (key, event) in index.unit_specific_sounds(name).into_iter().flatten() {
                    if names_a_sound(event) {
                        unit_sounds.insert((kind, key.clone()), event.clone());
                    }
                }
                for field in [
                    UNDER_CONSTRUCTION,
                    UNDER_REPAIR_FROM_RUBBLE,
                    UNDER_REPAIR_FROM_DAMAGE,
                ] {
                    if let Some(event) = index.unit_specific_sound(name, field) {
                        voices.insert((kind, field), event.to_string());
                    }
                }
            }
        }

        let damage_voices = voices
            .keys()
            .filter(|(_, field)| *field == SOUND_ON_DAMAGED || *field == SOUND_ON_REALLY_DAMAGED)
            .count();
        println!(
            "audio: {events} events, {} template voices resolved ({damage_voices} damage-rung), {}",
            voices.len(),
            audio.readout()
        );
        let music_scripts = load_music_scripts(&vfs, audio.library());
        let weather_sounds = load_weather_sounds(&vfs);
        println!(
            "weather: {} WeatherData record(s) resolved",
            weather_sounds.len()
        );
        let (eva_defs, jump_settings) = load_eva(&vfs, macros);
        let announcer =
            crate::announcer::Announcer::new(&eva_defs, object_index, &templates, rules);
        println!(
            "eva: {}, jump list {} units / {} ms",
            announcer.readout(),
            jump_settings.min_distance_between_jump_to_events,
            jump_settings.max_milliseconds_before_resetting_last_jump_to
        );
        let jump = crate::radar_jump::JumpList::new(
            jump_settings.min_distance_between_jump_to_events,
            jump_settings.max_milliseconds_before_resetting_last_jump_to,
        );
        Self {
            audio,
            vfs,
            previous: Vec::new(),
            scratch: Vec::new(),
            voices,
            enabled: true,
            music_scripts,
            announcer,
            jump,
            line_lengths: BTreeMap::new(),
            ambients: crate::ambient::Emitters::default(),
            names: templates,
            unit_sounds,
            crowd_keys,
            charge: crate::voice_chain::ChargeClocks::default(),
            charge_timeouts: Vec::new(),
            custom_ambients: BTreeMap::new(),
            enter_state_playing: BTreeMap::new(),
            stream_markers: BTreeMap::new(),
            stream_states: BTreeMap::new(),
            stream_voices: BTreeMap::new(),
            max_ambient_streams: 2,
            weather_sounds,
            weather_cached: mint_sage_modules::WeatherKind::None,
            weather_voice: None,
            script: script_ops::ScriptState::default(),
        }
    }

    pub(crate) fn settings(&self) -> mint_sage_ini::audio::AudioSettingsDef {
        self.audio.library().settings().clone()
    }

    pub(crate) fn event_facts(
        &self,
        event: &str,
        overrides: &mint_sage_audio::Overrides,
    ) -> Option<crate::ambient::EventFacts> {
        let def = self.audio.effective_def(event, overrides)?;
        Some(crate::ambient::EventFacts {
            permanent: def.control.has(mint_sage_ini::audio::ControlFlags::LOOP)
                && def.loop_count.unwrap_or(0) == 0,
            stream: def.kind == mint_sage_ini::audio::AudioEventKind::Ambient,
        })
    }

    pub(crate) fn place_ambients(&mut self, placed: Vec<(u32, [f32; 3], crate::ambient::Placed)>) {
        let (mut started, mut disabled, mut streams, mut streams_waiting) =
            (0usize, 0usize, 0usize, 0usize);
        for (index, at, placement) in placed {
            if self
                .event_facts(&placement.event, &placement.overrides)
                .is_some_and(|facts| facts.stream)
            {
                if placement.enabled {
                    streams = streams.saturating_add(1);
                    self.stream_markers.insert(
                        crate::ambient::PLACED | u64::from(index),
                        (placement.event, at),
                    );
                } else {
                    streams_waiting = streams_waiting.saturating_add(1);
                }
                continue;
            }
            if !placement.enabled {
                disabled = disabled.saturating_add(1);
                continue;
            }
            started = started.saturating_add(1);
            self.ambients.set(
                crate::ambient::PLACED | u64::from(index),
                Some((placement.event, placement.overrides)),
                at,
                true,
                |event, overrides| looping_of(&self.audio, event, overrides),
            );
        }
        println!(
            "ambient: {started} map placement(s) sound, {disabled} wait for a script, {streams} ambient stream(s) marked for selection, {streams_waiting} ambient stream(s) wait for a script"
        );
    }

    pub(crate) fn bind_custom_ambients(
        &mut self,
        placed: Vec<(mint_sim::Entity, crate::ambient::Custom)>,
        objects: usize,
    ) {
        let (silenced, own, waiting) = placed.iter().fold((0, 0, 0), |(a, b, c), (_, custom)| {
            (
                a + usize::from(custom.silenced),
                b + usize::from(custom.sound.is_some()),
                c + usize::from(!custom.silenced && !custom.enabled),
            )
        });
        println!(
            "ambient: {} of {objects} map-placed object(s) carry their own ambient properties: {own} own sound, {silenced} silenced, {waiting} wait for a script",
            placed.len()
        );
        self.custom_ambients = placed
            .into_iter()
            .map(|(entity, custom)| (owner_id(entity), custom))
            .collect();
    }

    fn update_ambients(
        &mut self,
        units: &[UnitView],
        ability_loops: &[(mint_sim::Entity, std::sync::Arc<str>)],
    ) {
        let mut moves: Vec<(u64, [f32; 3])> = Vec::with_capacity(units.len());
        for unit in units {
            let owner = owner_id(unit.entity);
            let at = [unit.position.x, unit.position.y, unit.ground.max(0.0)];
            moves.push((owner, at));
            let rung = crate::ambient::Rung::of(unit);
            let silenced = self.script.disabled_objects.contains(&owner);
            let wanted = crate::ambient::choose(self.custom_ambients.get(&owner), rung, || {
                let kind = unit.kind?;
                crate::ambient::fields(rung, crate::ambient::under_construction(unit.conditions))
                    .iter()
                    .find_map(|field| self.voices.get(&(kind, *field)))
                    .cloned()
            });
            let wanted = wanted.filter(|_| !silenced);
            let wanted = match wanted {
                Some((event, overrides))
                    if self
                        .event_facts(&event, &overrides)
                        .is_some_and(|facts| facts.stream) =>
                {
                    self.stream_markers.insert(owner, (event, at));
                    None
                }
                other => {
                    self.stream_markers.remove(&owner);
                    other
                }
            };
            let visible = matches!(
                unit.visibility,
                mint_sage_shroud::ObjectVisibility::Clear
                    | mint_sage_shroud::ObjectVisibility::PartialClear
            );
            if let Some(old) = self
                .ambients
                .set(owner, wanted, at, visible, |event, overrides| {
                    looping_of(&self.audio, event, overrides)
                })
            {
                crate::diag::diag!("ambient: {owner:#x} stops {old}");
                self.audio.stop_event_for(owner, &old);
            }
            let battle_owner = owner | crate::ambient::BATTLE;
            moves.push((battle_owner, at));
            let battle_wanted = if crate::ambient::battle(unit.conditions) {
                unit.kind
                    .and_then(|kind| {
                        self.voices
                            .get(&(kind, crate::ambient::SOUND_AMBIENT_BATTLE))
                    })
                    .cloned()
                    .map(|event| (event, mint_sage_audio::Overrides::default()))
            } else {
                None
            };
            if let Some(old) = self.ambients.set(
                battle_owner,
                battle_wanted,
                at,
                visible,
                |event, overrides| looping_of(&self.audio, event, overrides),
            ) {
                crate::diag::diag!("ambient: {battle_owner:#x} stops battle {old}");
                self.audio.stop_event_for(battle_owner, &old);
            }
            let moving_owner = owner | crate::ambient::MOVING;
            moves.push((moving_owner, at));
            let moving_wanted = if unit.moving {
                let damaged_now = unit.conditions.contains(Conditions::REALLYDAMAGED)
                    || unit.conditions.contains(Conditions::RUBBLE);
                unit.kind.and_then(|kind| {
                    let declared = self.voices.contains_key(&(kind, SOUND_MOVE_LOOP_DAMAGED));
                    let field = pick_damaged_or_plain(
                        SOUND_MOVE_LOOP,
                        SOUND_MOVE_LOOP_DAMAGED,
                        damaged_now,
                        declared,
                    );
                    self.voices
                        .get(&(kind, field))
                        .cloned()
                        .map(|event| (event, mint_sage_audio::Overrides::default()))
                })
            } else {
                None
            };
            if let Some(old) = self.ambients.set(
                moving_owner,
                moving_wanted,
                at,
                visible,
                |event, overrides| looping_of(&self.audio, event, overrides),
            ) {
                crate::diag::diag!("ambient: {moving_owner:#x} stops move loop {old}");
                self.audio.stop_event_for(moving_owner, &old);
            }
            let loop_owner = owner | crate::ambient::ABILITY_LOOP;
            moves.push((loop_owner, at));
            let loop_wanted = ability_loops
                .iter()
                .find(|(entity, _)| *entity == unit.entity)
                .map(|(_, event)| (event.to_string(), mint_sage_audio::Overrides::default()));
            if let Some(old) = self
                .ambients
                .set(loop_owner, loop_wanted, at, visible, |_, _| true)
            {
                crate::diag::diag!("ambient: {loop_owner:#x} stops ability loop {old}");
                self.audio.stop_event_for(loop_owner, &old);
            }
        }
        moves.sort_unstable_by_key(|(owner, _)| *owner);
        moves.dedup_by_key(|(owner, _)| *owner);
        let seen: Vec<u64> = moves.iter().map(|(owner, _)| *owner).collect();
        self.charge.retain(&seen);
        for (owner, old) in self.ambients.retain_units(&seen) {
            self.audio.stop_event_for(owner, &old);
        }
        self.stream_markers.retain(|owner, _| {
            owner & crate::ambient::PLACED != 0 || seen.binary_search(owner).is_ok()
        });
        let vfs = Arc::clone(&self.vfs);
        for (owner, emitter) in &mut self.ambients.map {
            if emitter.phase == crate::ambient::Phase::Done {
                continue;
            }
            emitter.cooldown = emitter.cooldown.saturating_sub(1);
            let ready = emitter.cooldown == 0;
            let reach = self
                .audio
                .reach(&emitter.event, emitter.at, &emitter.overrides);
            match crate::ambient::step(emitter.phase, emitter.looping, reach, ready) {
                crate::ambient::Action::Play => {
                    let delay = delay_of(&self.audio, &emitter.event, &emitter.overrides);
                    let mut overrides = emitter.overrides;
                    if delay.is_some() {
                        overrides.looping = Some(false);
                    }
                    let request = EventRequest::at(&emitter.event, emitter.at)
                        .by(*owner)
                        .visible(emitter.visible)
                        .with(overrides);
                    let outcome = self.audio.play(&vfs, &request);
                    crate::diag::diag!(
                        "ambient: {owner:#x} {} -> {}",
                        emitter.event,
                        outcome.label()
                    );
                    emitter.phase =
                        crate::ambient::after_play(emitter.looping, outcome, delay.is_some());
                    if emitter.phase == crate::ambient::Phase::Waiting {
                        if let Some((low, high)) = delay {
                            let clip_ms = self
                                .audio
                                .library()
                                .event_duration_ms(&vfs, &emitter.event)
                                .and_then(|ms| u32::try_from(ms).ok())
                                .unwrap_or(0);
                            let draw_ms = self.audio.random_delay_ms(low, high);
                            emitter.cooldown = mint_sim::LogicFrames::from_engine_milliseconds(
                                clip_ms.saturating_add(draw_ms),
                            )
                            .get();
                        }
                    }
                }
                crate::ambient::Action::Stop => {
                    crate::diag::diag!("ambient: {owner:#x} {} out of reach", emitter.event);
                    self.audio.stop_event_for(*owner, &emitter.event);
                    emitter.phase = crate::ambient::Phase::Waiting;
                }
                crate::ambient::Action::None => {}
            }
        }
        self.audio.move_owners(moves);
        self.update_ambient_streams(&vfs);
    }

    fn update_ambient_streams(&mut self, vfs: &Arc<Vfs>) {
        let settings = self.audio.library().settings().clone();
        let mut positions: BTreeMap<&str, (f32, [f32; 3])> = BTreeMap::new();
        let mut markers = Vec::with_capacity(self.stream_markers.len());
        for (event, at) in self.stream_markers.values() {
            let loudness =
                self.audio
                    .effective_volume(event, *at, &mint_sage_audio::Overrides::default());
            markers.push((event.clone(), loudness));
            positions
                .entry(event.as_str())
                .and_modify(|(level, pos)| {
                    if loudness > *level {
                        *level = loudness;
                        *pos = *at;
                    }
                })
                .or_insert((loudness, *at));
        }
        let dt_ms = 1_000.0 / f32::from(u16::try_from(mint_sim::SIMULATION_HZ).unwrap_or(30));
        #[allow(clippy::cast_precision_loss)]
        let fade_ms = settings.time_to_fade_audio_ms as f32;
        let changes = crate::ambient_stream::tick(
            &mut self.stream_states,
            &markers,
            self.max_ambient_streams,
            settings.ambient_stream_hysteresis,
            settings.min_sample_volume,
            fade_ms,
            dt_ms,
        );
        for name in &changes.started {
            let Some((_, at)) = positions.get(name.as_str()).copied() else {
                continue;
            };
            let request = EventRequest::at(name, at);
            let (outcome, id) = self.audio.play_tracked(vfs, &request);
            crate::diag::diag!("ambient: stream {name} starts -> {}", outcome.label());
            if let Some(id) = id {
                self.stream_voices.insert(name.clone(), id);
            }
        }
        for (name, ms) in &changes.fading {
            if let Some(id) = self.stream_voices.get(name) {
                crate::diag::diag!("ambient: stream {name} displaced, fades over {ms}ms");
                self.audio.fade_out(*id, *ms);
            }
        }
        for name in &changes.stopped {
            self.stream_voices.remove(name);
            crate::diag::diag!("ambient: stream {name} stopped");
        }
    }

    pub(crate) fn music_tick(&mut self, world: &dyn crate::music_script::MusicWorld) {
        let Some(scripts) = self.music_scripts.as_mut() else {
            return;
        };
        let world = WithAnnouncer {
            world,
            announcer: &self.announcer,
        };
        let commands = scripts.update(&world);
        let vfs = Arc::clone(&self.vfs);
        for command in commands {
            if self.script.owns_music {
                crate::diag::diag!(
                    "music: script asks {command:?} (held: a map script owns the channel)"
                );
                continue;
            }
            crate::diag::diag!("music: script asks {command:?}");
            match command {
                crate::music_script::MusicCommand::Replace(request) => {
                    self.audio.music_replace(&vfs, request);
                }
                crate::music_script::MusicCommand::Push(request) => {
                    self.audio.music_push(&vfs, request);
                }
                crate::music_script::MusicCommand::Pop { fade_out, fade_in } => {
                    self.audio.music_pop(fade_out, fade_in);
                }
            }
        }
    }

    pub(crate) fn announce(
        &mut self,
        facts: &crate::announcer::AnnouncerFacts<'_>,
        ground: &dyn Fn(f32, f32) -> f32,
    ) {
        let combat = facts.rules.combat_facts(facts.world);
        let vfs = Arc::clone(&self.vfs);
        let enabled = self.is_enabled() && self.script.eva_on;
        let audio = &mut self.audio;
        let lengths = &mut self.line_lengths;
        let mut play = |line: &mint_sage_audio::EvaLine| -> Option<u32> {
            if !enabled {
                return None;
            }
            let at = [line.at[0], line.at[1], ground(line.at[0], line.at[1])];
            let outcome = audio.play(&vfs, &EventRequest::at(&line.sound, at));
            if outcome != Outcome::Queued {
                crate::diag::diag!("eva: {} did not play ({})", line.sound, outcome.label());
                return None;
            }
            let length = *lengths.entry(line.sound.clone()).or_insert_with(|| {
                audio
                    .library()
                    .event_duration_ms(&vfs, &line.sound)
                    .and_then(|ms| u32::try_from(ms).ok())
                    .unwrap_or(0)
            });
            Some(length)
        };
        if let Some(point) = self.announcer.observe(facts, combat, &mut play) {
            self.jump.push(point);
        }
    }

    pub(crate) fn push_jump(&mut self, point: [f32; 2]) {
        self.jump.push(point);
    }

    pub(crate) fn jump_press(&mut self, now: std::time::Duration) -> Option<[f32; 2]> {
        self.jump.press(now)
    }

    pub(crate) fn music_frame(&mut self, dt: f32) {
        self.step_script_fade(dt);
        let vfs = Arc::clone(&self.vfs);
        for event in self.audio.music_update(&vfs, f64::from(dt.max(0.0))) {
            crate::ui_audio::log_music(&event);
            if let mint_sage_audio::MusicEvent::Finished {
                notify: Some(flag), ..
            } = &event
            {
                if let Some(scripts) = self.music_scripts.as_mut() {
                    scripts.set_flag(flag, true);
                }
            }
        }
    }

    pub(crate) fn readout(&self) -> String {
        self.audio.readout()
    }

    pub(crate) fn hitch_counters(&self) -> (u64, u64) {
        (
            self.audio.device_stats().underruns,
            self.audio.bank_stats().load_us,
        )
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled && !self.audio.is_silent()
    }

    pub(crate) fn set_pool_gain(&self, gain: f32) {
        self.audio.set_pool_gain(gain);
    }

    pub(crate) fn pause(&self) {
        self.audio.pause_all();
    }

    pub(crate) fn resume(&self) {
        self.audio.resume_all();
    }

    pub(crate) fn set_submix_gain(&mut self, submix: mint_sage_ini::audio::Submix, gain: f32) {
        let mut gain = gain.clamp(0.0, 1.0);
        if submix == mint_sage_ini::audio::Submix::Music {
            self.script.music_slider = gain;
            gain *= self.script.music_gain;
        }
        self.audio.set_submix_gain(submix, gain);
    }

    pub(crate) fn cycle_rolloff(&mut self) {
        let next = self.audio.rolloff().next();
        self.audio.set_rolloff(next);
        let rung = Rolloff::ALL
            .iter()
            .position(|mode| *mode == next)
            .map_or(0, |index| index.saturating_add(1));
        println!(
            "audio rolloff: {} [{rung} of {}]",
            next.label(),
            Rolloff::ALL.len()
        );
    }

    pub(crate) fn update_weather(&mut self, kind: mint_sage_modules::WeatherKind) {
        let holding = self.weather_voice.is_some();
        let change = weather_sound_change(self.weather_cached, kind, holding, &self.weather_sounds);
        if change == WeatherSoundChange::None {
            return;
        }
        self.weather_cached = kind;
        if let Some(voice) = self.weather_voice.take() {
            self.audio.fade_out(voice, 0);
        }
        if let WeatherSoundChange::Play(sound) = change {
            let vfs = Arc::clone(&self.vfs);
            let (outcome, id) = self.audio.play_tracked(&vfs, &EventRequest::new(sound));
            crate::diag::diag!("weather: {kind:?} starts {sound} ({})", outcome.label());
            self.weather_voice = id;
        }
    }

    pub(crate) fn set_listener(&mut self, camera: &RtsCamera, ground: f32) {
        let view = camera.camera();
        let forward = view.target - view.eye;
        let right = forward.cross(view.up);
        self.audio.set_camera(
            [view.eye.x, view.eye.y, view.eye.z],
            [view.target.x, view.target.y, ground],
            [right.x, right.y, right.z],
        );
    }

    fn report(&self, field: &str, event: &str, at: Vec3, outcome: Outcome) {
        let listener = self.audio.listener();
        let dx = at.x - listener.position[0];
        let dy = at.y - listener.position[1];
        let dz = at.z - listener.position[2];
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        let settings = self.audio.library().settings();
        let library = self.audio.library();
        let def = library.get(event).or_else(|| {
            library
                .multisound(event)
                .and_then(|multi| multi.subsounds.first())
                .and_then(|first| library.get(&first.name))
        });
        let (volume, floor, range) = def.map_or((1.0, 0.0, (0.0, 0.0)), |def| {
            (
                def.volume.unwrap_or(1.0),
                def.min_volume.unwrap_or(0.0),
                (
                    def.min_range.unwrap_or(settings.global_min_range),
                    def.max_range.unwrap_or(settings.global_max_range),
                ),
            )
        });
        let spatial = mint_sage_audio::Spatial {
            position: [at.x, at.y, at.z],
            min_range: range.0,
            max_range: range.1,
            min_gain: floor,
        };
        let level =
            mint_sage_audio::spatial_level(self.audio.rolloff(), volume, &spatial, distance);
        println!(
            "audio voice: {field} {event} -> {}; Volume {volume:.2} MinVolume {floor:.2} range {:.0}..{:.0}, {distance:.0} from the microphone: level {level:.3} x zoom gain {:.3}",
            outcome.label(),
            range.0,
            range.1,
            listener.gain
        );
    }

    pub(crate) fn play(&mut self, event: &str, at: Option<Vec3>, owner: Option<u64>) -> Outcome {
        if !self.is_enabled() {
            return Outcome::UnknownEvent;
        }
        let mut request = match at {
            Some(position) => EventRequest::at(event, [position.x, position.y, position.z]),
            None => EventRequest::new(event),
        };
        if let Some(owner) = owner {
            request = request.by(owner);
        }
        let vfs = Arc::clone(&self.vfs);
        self.audio.play(&vfs, &request)
    }

    pub(crate) fn play_misc(
        &mut self,
        field: &str,
        at: Option<Vec3>,
        audience: Option<Audience>,
    ) -> Outcome {
        if !self.is_enabled() {
            return Outcome::UnknownEvent;
        }
        let Some(event) = self
            .audio
            .library()
            .misc(field)
            .filter(|name| !name.eq_ignore_ascii_case("NoSound"))
            .map(str::to_string)
        else {
            crate::diag::diag!("ui sound: {field} names no sound");
            return Outcome::UnknownEvent;
        };
        let mut request = match at {
            Some(position) => EventRequest::at(&event, [position.x, position.y, position.z]),
            None => EventRequest::new(&event),
        };
        if let Some(audience) = audience {
            request = request.heard_as(audience);
        }
        let vfs = Arc::clone(&self.vfs);
        let outcome = self.audio.play(&vfs, &request);
        crate::diag::diag!("ui sound: {field} plays {event} ({})", outcome.label());
        outcome
    }

    pub(crate) fn play_animation_sound(
        &mut self,
        event: &str,
        at: Vec3,
        owner: u64,
        range_cap: Option<f32>,
    ) -> Outcome {
        if !self.is_enabled() {
            return Outcome::UnknownEvent;
        }
        if let Some(cap) = range_cap {
            let listener = self.audio.listener();
            let dx = at.x - listener.position[0];
            let dy = at.y - listener.position[1];
            let dz = at.z - listener.position[2];
            if (dx * dx + dy * dy + dz * dz).sqrt() > cap {
                return Outcome::UnknownEvent;
            }
        }
        let request = EventRequest::at(event, [at.x, at.y, at.z]).by(owner);
        let vfs = Arc::clone(&self.vfs);
        self.audio.play(&vfs, &request)
    }

    pub(crate) fn observe(
        &mut self,
        units: &[UnitView],
        ability_loops: &[(mint_sim::Entity, std::sync::Arc<str>)],
        audience: impl Fn(mint_sim::PlayerId) -> Audience,
    ) {
        if !self.is_enabled() {
            self.remember(units);
            return;
        }

        let mut newly_selected: Vec<&UnitView> = Vec::new();
        for unit in units {
            let id = unit.entity.index();
            let now = flags_for(unit);
            let before = self.flags_of(id);
            let owner = owner_id(unit.entity);

            if now.dying && !before.dying {
                self.audio.stop_owner(owner);
                continue;
            }
            if now.really_damaged && !before.really_damaged {
                self.play_for(unit, SOUND_ON_REALLY_DAMAGED, audience(unit.owner));
            } else if now.damaged && !before.damaged {
                self.play_for(unit, SOUND_ON_DAMAGED, audience(unit.owner));
            }
            if before.steady && !now.steady && unit.health.is_some_and(|h| h > 0.0) {
                self.play_for(unit, VOICE_FEAR, audience(unit.owner));
            }
            if now.moving && !before.moving {
                let damaged_now = now.really_damaged || now.rubble;
                self.play_damaged_or_plain(
                    unit,
                    SOUND_MOVE_START,
                    SOUND_MOVE_START_DAMAGED,
                    damaged_now,
                    audience(unit.owner),
                );
            }
            if now.selected && !before.selected {
                newly_selected.push(unit);
            }
        }
        if !newly_selected.is_empty() {
            self.play_select_vote(&newly_selected, &audience);
        }

        self.scratch.clear();
        for unit in units {
            self.scratch.push((unit.entity.index(), flags_for(unit)));
        }
        self.scratch.sort_by_key(|(id, _)| *id);

        for change in transitions(&self.previous, &self.scratch) {
            match change {
                Change::Start { id, field } => {
                    if let Some(unit) = units.iter().find(|u| u.entity.index() == id) {
                        self.play_for(unit, field, audience(unit.owner));
                    }
                }
                Change::Stop { flags, .. } => {
                    let Some(owner) = flags.entity.map(owner_id) else {
                        continue;
                    };
                    let field = construction_loop_field(flags);
                    if let Some(event) = flags.kind.and_then(|kind| self.voices.get(&(kind, field)))
                    {
                        self.audio.stop_event_for(owner, event);
                        crate::diag::diag!("sound field: {field} {event} -> stopped");
                    }
                }
            }
        }

        std::mem::swap(&mut self.previous, &mut self.scratch);
        self.update_ambients(units, ability_loops);
    }

    pub(crate) fn play_voice(
        &mut self,
        event: &crate::voice::VoiceEvent,
        at: Option<(Vec3, u16, bool, Audience)>,
        target: Option<crate::voice_chain::TargetFacts>,
        move_facts: Option<crate::voice_chain::MoveVoiceFacts>,
        now: u64,
    ) -> Outcome {
        use crate::voice::VoiceKind;
        if !self.is_enabled() {
            return Outcome::UnknownEvent;
        }
        let Some((at, template, visible, audience)) = at else {
            return Outcome::UnknownEvent;
        };
        if matches!(event.kind, VoiceKind::StealthBack) && audience != Audience::Local {
            return Outcome::NotForLocal;
        }
        if matches!(event.kind, VoiceKind::SpecialPower | VoiceKind::ObjectSound) {
            let Some(sound) = &event.sound else {
                return Outcome::UnknownEvent;
            };
            let owner = owner_id(event.entity);
            return self.finish_play(event.kind.field(), sound, at, owner, visible, audience, now);
        }
        let chain = self.chain_for(event, target, move_facts, now, false);
        let owner = owner_id(event.entity);
        let Some((slot, name)) = self.resolve_chain(template, &chain, None) else {
            crate::diag::diag!(
                "sound field: {:?} - no field of its chain names a sound",
                event.kind
            );
            return Outcome::UnknownEvent;
        };
        self.finish_play(&slot, &name, at, owner, visible, audience, now)
    }

    pub(crate) fn play_unplaced(
        &mut self,
        event: &crate::voice::VoiceEvent,
        audience: Audience,
    ) -> Outcome {
        use crate::voice::VoiceKind;
        if !self.is_enabled()
            || !matches!(event.kind, VoiceKind::SpecialPower | VoiceKind::ObjectSound)
        {
            return Outcome::UnknownEvent;
        }
        let Some(sound) = &event.sound else {
            return Outcome::UnknownEvent;
        };
        let request = EventRequest::new(sound)
            .by(owner_id(event.entity))
            .heard_as(audience);
        let vfs = Arc::clone(&self.vfs);
        let outcome = self.audio.play(&vfs, &request);
        crate::diag::diag!("sound field: {} unplaced ({})", sound, outcome.label());
        outcome
    }

    pub(crate) fn play_enter_state_voice(
        &mut self,
        event: &crate::voice::VoiceEvent,
        at: Option<(Vec3, u16, bool, Audience)>,
        target: Option<crate::voice_chain::TargetFacts>,
        move_facts: Option<crate::voice_chain::MoveVoiceFacts>,
        now: u64,
    ) -> Outcome {
        use crate::voice::VoiceKind;
        if !self.is_enabled() || !matches!(event.kind, VoiceKind::Move | VoiceKind::Attack) {
            return Outcome::UnknownEvent;
        }
        let Some((at, template, visible, audience)) = at else {
            return Outcome::UnknownEvent;
        };
        let chain = self.chain_for(event, target, move_facts, now, true);
        let Some((slot, name)) = self.resolve_chain(template, &chain, None) else {
            return Outcome::UnknownEvent;
        };
        let owner_entity = event.entity.index();
        if self.enter_state_playing.get(&owner_entity) == Some(&name) {
            return Outcome::NotForLocal;
        }
        let owner = owner_id(event.entity);
        let outcome = self.finish_play(&slot, &name, at, owner, visible, audience, now);
        if outcome == Outcome::Queued {
            self.enter_state_playing.insert(owner_entity, name);
        }
        outcome
    }

    fn chain_for(
        &self,
        event: &crate::voice::VoiceEvent,
        target: Option<crate::voice_chain::TargetFacts>,
        move_facts: Option<crate::voice_chain::MoveVoiceFacts>,
        now: u64,
        enter_state: bool,
    ) -> Vec<crate::voice_chain::Slot> {
        use crate::voice::VoiceKind;
        let named = target.and_then(|facts| {
            let name = self.names.get(usize::from(facts.kind))?;
            Some((name.as_str(), facts))
        });
        match event.kind {
            VoiceKind::Attack => {
                let owner = owner_id(event.entity);
                let due = self.charge.due(owner, event.was_attacking, now);
                crate::voice_chain::attack(named, due, enter_state)
            }
            VoiceKind::Move => crate::voice_chain::move_order(
                self.camp_holds(move_facts),
                move_facts.is_some_and(|facts| facts.higher_ground),
                move_facts.is_some_and(|facts| facts.engaged),
                enter_state,
            ),
            VoiceKind::Enter => crate::voice_chain::enter(named),
            kind => vec![crate::voice_chain::Slot::Field(kind.field())],
        }
    }

    fn camp_holds(&self, facts: Option<crate::voice_chain::MoveVoiceFacts>) -> bool {
        let Some(facts) = facts else {
            return false;
        };
        let settings = self.audio.library().settings();
        facts.campness_start <= settings.voice_move_to_camp_max_campness_at_start_point
            && facts.campness_goal >= settings.voice_move_to_camp_min_campness_at_end_point
    }

    pub(crate) fn play_voice_vote(
        &mut self,
        event: &crate::voice::VoiceEvent,
        templates: &[u16],
        at: Option<(Vec3, u16, bool, Audience)>,
        target: Option<crate::voice_chain::TargetFacts>,
        move_facts: Option<crate::voice_chain::MoveVoiceFacts>,
        now: u64,
    ) -> Outcome {
        if !self.is_enabled() {
            return Outcome::UnknownEvent;
        }
        let Some((at, _, visible, audience)) = at else {
            return Outcome::UnknownEvent;
        };
        if templates.is_empty() {
            return Outcome::UnknownEvent;
        }
        let owner = owner_id(event.entity);
        let threshold = self.crowd_threshold(templates);
        let records = templates.iter().filter_map(|&template| {
            let chain = self.chain_for(event, target, move_facts, now, false);
            self.resolve_chain(template, &chain, threshold)
                .map(|record| (record, 1))
        });
        let Some((slot, name)) = pick_by_total(records) else {
            crate::diag::diag!(
                "sound field: {:?} - no field of any voting template's chain names a sound",
                event.kind
            );
            return Outcome::UnknownEvent;
        };
        self.finish_play(&slot, &name, at, owner, visible, audience, now)
    }

    fn crowd_threshold(
        &self,
        templates: &[u16],
    ) -> Option<&mint_sage_ini::audio::CrowdThresholdDef> {
        let library = self.audio.library();
        let winner = pick_by_total(templates.iter().filter_map(|template| {
            let name = self.crowd_keys.get(template)?;
            let weight = library.crowd_response(name)?.weight;
            Some((name.as_str(), weight))
        }))?;
        let count = u32::try_from(templates.len()).unwrap_or(u32::MAX);
        library.crowd_response(winner)?.threshold_for(count)
    }

    fn resolve_chain(
        &self,
        template: u16,
        chain: &[crate::voice_chain::Slot],
        threshold: Option<&mint_sage_ini::audio::CrowdThresholdDef>,
    ) -> Option<(String, String)> {
        chain.iter().find_map(|slot| {
            if let Some(value) = threshold.and_then(|t| t.fields.get(slot.label())) {
                return Some((slot.label().to_owned(), value.clone()));
            }
            let event = match slot {
                crate::voice_chain::Slot::Field(field) => self.voices.get(&(template, *field)),
                crate::voice_chain::Slot::Unit(key) => {
                    self.unit_sounds.get(&(template, key.clone()))
                }
            }?;
            Some((slot.label().to_owned(), event.clone()))
        })
    }

    fn finish_play(
        &mut self,
        slot: &str,
        name: &str,
        at: Vec3,
        owner: u64,
        visible: bool,
        audience: Audience,
        now: u64,
    ) -> Outcome {
        if let Some(eva) = name.strip_prefix(EVA_PREFIX) {
            if audience == Audience::Local {
                self.announcer
                    .report_voice(eva, mint_sim::Frame::new(now), [at.x, at.y, at.z]);
            }
            return Outcome::Queued;
        }
        let request = EventRequest::at(name, [at.x, at.y, at.z])
            .by(owner)
            .visible(visible)
            .heard_as(audience);
        let vfs = Arc::clone(&self.vfs);
        let outcome = self.audio.play(&vfs, &request);
        if outcome != Outcome::NotForLocal {
            self.report(slot, name, at, outcome);
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn report_fx_eva_event(
        &mut self,
        rules: &crate::rules::RotwkRules,
        world: &mint_sim::World,
        particle_scene: &mint_sage_scene::ParticleScene,
        list_name: &str,
        on_object: bool,
        caller: mint_sim::Entity,
        local: mint_sim::PlayerId,
        frame: mint_sim::Frame,
        at: [f32; 3],
    ) {
        let Some(list) = particle_scene.fx_list(list_name) else {
            return;
        };
        let Some(nugget) = list
            .nuggets
            .iter()
            .find_map(|nugget| nugget.eva_event.as_ref())
        else {
            return;
        };
        let Some((_, _, visible, owner)) = rules.voice_target(world, caller, local) else {
            return;
        };
        let name = crate::fx_present::eva_event_name(
            nugget,
            on_object,
            list.play_even_if_shrouded,
            visible,
            owner,
            local,
            |a, b| rules.allied(a, b),
        );
        if let Some(name) = name {
            self.announcer.report_voice(name, frame, at);
        }
    }

    pub(crate) fn attack_ordered(&mut self, entity: mint_sim::Entity, template: u16, now: u64) {
        let timeout = self
            .charge_timeouts
            .get(usize::from(template))
            .copied()
            .unwrap_or(0);
        self.charge.restart(owner_id(entity), now, timeout);
    }

    pub(crate) fn set_charge_timeouts(&mut self, frames: Vec<u64>) {
        self.charge_timeouts = frames;
    }

    fn play_for(&mut self, unit: &UnitView, field: &'static str, audience: Audience) -> Outcome {
        let Some(kind) = unit.kind else {
            return Outcome::UnknownEvent;
        };
        let Some(event) = self.voices.get(&(kind, field)).cloned() else {
            return Outcome::UnknownEvent;
        };
        self.play_named_for(unit, field, &event, audience)
    }

    fn play_named_for(
        &mut self,
        unit: &UnitView,
        field: &str,
        event: &str,
        audience: Audience,
    ) -> Outcome {
        let at = Vec3::new(unit.position.x, unit.position.y, unit.ground.max(0.0));
        let request = EventRequest::at(event, [at.x, at.y, at.z])
            .by(owner_id(unit.entity))
            .visible(matches!(
                unit.visibility,
                mint_sage_shroud::ObjectVisibility::Clear
                    | mint_sage_shroud::ObjectVisibility::PartialClear
            ))
            .heard_as(audience);
        let vfs = Arc::clone(&self.vfs);
        let outcome = self.audio.play(&vfs, &request);
        if matches!(
            field,
            VOICE_SELECT | VOICE_SELECT_UNDER_CONSTRUCTION | VOICE_SELECT_BATTLE
        ) && outcome != Outcome::NotForLocal
        {
            self.report(field, event, at, outcome);
        } else {
            crate::diag::diag!("sound field: {field} {event} -> {}", outcome.label());
        }
        outcome
    }

    fn play_select_vote(
        &mut self,
        units: &[&UnitView],
        audience: &impl Fn(mint_sim::PlayerId) -> Audience,
    ) {
        let templates: Vec<u16> = units.iter().filter_map(|unit| unit.kind).collect();
        let threshold = self.crowd_threshold(&templates);
        let resolved: Vec<(&UnitView, String)> = units
            .iter()
            .filter_map(|&unit| {
                let name = self.resolve_select(unit.kind?, unit.conditions, threshold)?;
                Some((unit, name))
            })
            .collect();
        let records = resolved.iter().map(|(_, name)| (name.clone(), 1));
        let Some(name) = pick_by_total(records) else {
            return;
        };
        let Some(&(unit, _)) = resolved.iter().find(|(_, candidate)| *candidate == name) else {
            return;
        };
        self.play_named_for(unit, VOICE_SELECT, &name, audience(unit.owner));
    }

    fn resolve_select(
        &self,
        kind: u16,
        conditions: Conditions,
        threshold: Option<&mint_sage_ini::audio::CrowdThresholdDef>,
    ) -> Option<String> {
        let field = select_voice_field(conditions);
        if field == VOICE_SELECT {
            if let Some(value) = threshold.and_then(|t| t.fields.get(VOICE_SELECT)) {
                return Some(value.clone());
            }
            return self.voices.get(&(kind, VOICE_SELECT)).cloned();
        }
        if let Some(value) = self.voices.get(&(kind, field)) {
            return Some(value.clone());
        }
        if let Some(value) = threshold.and_then(|t| t.fields.get(VOICE_SELECT)) {
            return Some(value.clone());
        }
        self.voices.get(&(kind, VOICE_SELECT)).cloned()
    }

    fn play_damaged_or_plain(
        &mut self,
        unit: &UnitView,
        plain: &'static str,
        damaged: &'static str,
        damaged_now: bool,
        audience: Audience,
    ) -> Outcome {
        let declared = unit
            .kind
            .is_some_and(|kind| self.voices.contains_key(&(kind, damaged)));
        let field = pick_damaged_or_plain(plain, damaged, damaged_now, declared);
        self.play_for(unit, field, audience)
    }

    fn flags_of(&self, id: u32) -> Flags {
        match self.previous.binary_search_by_key(&id, |(key, _)| *key) {
            Ok(index) => self.previous.get(index).map_or(Flags::default(), |e| e.1),
            Err(_) => Flags::default(),
        }
    }

    fn remember(&mut self, units: &[UnitView]) {
        self.scratch.clear();
        for unit in units {
            self.scratch.push((unit.entity.index(), flags_for(unit)));
        }
        self.scratch.sort_by_key(|(id, _)| *id);
        std::mem::swap(&mut self.previous, &mut self.scratch);
    }
}

fn flags_for(unit: &UnitView) -> Flags {
    Flags {
        selected: unit.selected,
        dying: unit.dying,
        moving: unit.moving,
        steady: unit.health.is_none_or(|health| health >= FEAR_FRACTION),
        damaged: unit.conditions.contains(Conditions::DAMAGED),
        really_damaged: unit.conditions.contains(Conditions::REALLYDAMAGED),
        rubble: unit.conditions.contains(Conditions::RUBBLE),
        building: unit
            .conditions
            .contains(Conditions::ACTIVELY_BEING_CONSTRUCTED)
            || unit.under_repair,
        rebuild: unit.build_total.is_some(),
        repairing: unit.under_repair,
        kind: unit.kind,
        entity: Some(unit.entity),
    }
}

pub(crate) fn play_animation_sounds(audio: &mut GameAudio, units: &mut mint_sage_scene::UnitScene) {
    if !audio.is_enabled() {
        units.take_sound_events().for_each(drop);
        return;
    }
    let events: Vec<(u32, String, Option<f32>)> = units.take_sound_events().collect();
    for (id, event, range_cap) in events {
        let Some(drawable) = units.drawable_by_id(id) else {
            continue;
        };
        let at = drawable.placement().transform_point(Vec3::ZERO);
        audio.play_animation_sound(&event, at, u64::from(id), range_cap);
    }
}

mod script_ops;
#[cfg(test)]
mod tests;
