use std::path::PathBuf;
use std::sync::Arc;

use mint_sage_ini::audio::{AudioEventKind, AudioIndex};
use mint_sage_ini::table::BlockTable;
use mint_sage_ini::Document;
use mint_vfs::{DirMount, Vfs};

use crate::library::AudioLibrary;
use crate::rng::ClientRng;
use crate::runtime::{EventRequest, Outcome, SageAudio};

fn wav(frames: usize, rate: u32, amplitude: i16) -> Vec<u8> {
    let data: Vec<u8> = (0..frames).flat_map(|_| amplitude.to_le_bytes()).collect();
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * 2).to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());

    let mut body = b"WAVE".to_vec();
    for (id, payload) in [(b"fmt ", &fmt), (b"data", &data)] {
        body.extend_from_slice(id);
        body.extend_from_slice(&u32::try_from(payload.len()).unwrap_or(0).to_le_bytes());
        body.extend_from_slice(payload);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&u32::try_from(body.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

struct Fixture {
    _root: PathBuf,
    vfs: Vfs,
}

impl Fixture {
    fn new(files: &[(&str, Vec<u8>)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "mint-sage-audio-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let sounds = root.join("data").join("audio").join("sounds");
        std::fs::create_dir_all(&sounds).unwrap();
        for (name, bytes) in files {
            let path = if name.contains('/') {
                root.join(name)
            } else {
                sounds.join(name)
            };
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, bytes).unwrap();
        }
        let mut vfs = Vfs::new();
        vfs.mount(Arc::new(DirMount::new(&root).unwrap()));
        Self { _root: root, vfs }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._root);
    }
}

fn library(ini: &str) -> AudioLibrary {
    let document = Document::parse("audio.ini", ini, &BlockTable::audio_path()).unwrap();
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = AudioIndex::new();
    index.insert_document(&document);
    AudioLibrary::from_index(index)
}

fn audio(ini: &str) -> SageAudio {
    let mut audio = SageAudio::silent(library(ini));
    audio.set_rng(ClientRng::seeded(0xC0FFEE));
    audio
}

#[test]
fn a_path_is_built_from_the_settings_and_nothing_else() {
    let shipped = library(
        "AudioSettings\n  AudioRoot = Data\\Audio\n  SoundsFolder = Sounds\n  SoundsExtension = wav\nEnd\n\
         AudioEvent Thud\n  Sounds = thud01\nEnd\n",
    );
    let def = shipped.get("thud").unwrap();
    assert_eq!(
        shipped.variant_path(def, "thud01").as_deref(),
        Some("data/audio/sounds/thud01.wav")
    );

    let renamed = library(
        "AudioSettings\n  AudioRoot = Mod\\Sfx\n  SoundsFolder = Wav\n  SoundsExtension = snd\nEnd\n\
         AudioEvent Thud\n  Sounds = thud01\nEnd\n",
    );
    let def = renamed.get("thud").unwrap();
    assert_eq!(
        renamed.variant_path(def, "thud01").as_deref(),
        Some("mod/sfx/wav/thud01.snd")
    );
}

#[test]
fn a_streamed_kind_keeps_the_extension_its_filename_carries() {
    let library = library("MusicTrack Theme\n  Filename = theme.mp3\nEnd\n");
    let def = library.get("theme").unwrap();
    assert_eq!(def.kind, AudioEventKind::Music);
    assert_eq!(
        library.variant_path(def, "").as_deref(),
        Some("data/audio/tracks/theme.mp3")
    );
}

#[test]
fn a_streamed_sound_resolves_in_the_sounds_folder() {
    let library = library(
        "StreamedSound Drizzle
  Filename = drizzle.wav
End
",
    );
    let def = library.get("drizzle").unwrap();
    assert_eq!(
        library.variant_path(def, "").as_deref(),
        Some("data/audio/sounds/drizzle.wav")
    );
}

#[test]
fn an_event_plays_when_its_file_is_there() {
    let fixture = Fixture::new(&[("thud01.wav", wav(4410, 44100, 20000))]);
    let mut audio = audio("AudioEvent Thud\n  Sounds = thud01\n  Type = ui player\nEnd\n");
    assert_eq!(
        audio.play(&fixture.vfs, &EventRequest::new("Thud")),
        Outcome::Queued
    );
    assert_eq!(audio.request_stats().queued, 1);
    assert_eq!(audio.bank_stats().loaded, 1);
}

#[test]
fn a_missing_file_does_not_play_and_is_remembered() {
    let fixture = Fixture::new(&[]);
    let mut audio = audio("AudioEvent Absent\n  Sounds = nothere\nEnd\n");
    for _ in 0..1_000 {
        assert_eq!(
            audio.play(&fixture.vfs, &EventRequest::new("Absent")),
            Outcome::MissingFile
        );
    }
    assert_eq!(audio.request_stats().missing, 1_000);
    assert_eq!(audio.bank_stats().missing, 1, "one distinct path");
    assert_eq!(audio.bank_stats().loaded, 0);
}

#[test]
fn an_unknown_event_name_is_refused_not_guessed() {
    let fixture = Fixture::new(&[]);
    let mut audio = audio("AudioEvent Real\n  Sounds = a\nEnd\n");
    assert_eq!(
        audio.play(&fixture.vfs, &EventRequest::new("NotAnEvent")),
        Outcome::UnknownEvent
    );
    assert_eq!(audio.request_stats().unknown, 1);
}

#[test]
fn an_event_naming_no_sounds_is_refused_rather_than_erroring() {
    let fixture = Fixture::new(&[]);
    let mut audio = audio("AudioEvent Silent\n  Sounds =\nEnd\n");
    assert_eq!(
        audio.play(&fixture.vfs, &EventRequest::new("Silent")),
        Outcome::NoVariants
    );
}

#[test]
fn player_allies_and_enemies_bits_decide_who_hears_an_owned_sound() {
    use crate::runtime::Audience::{Ally, Enemy, Local};
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio(
        "AudioEvent Mine\n  Sounds = thud01\n  Type = world player voice\nEnd\n\
         AudioEvent Chatter\n  Sounds = thud01\n  Type = world allies enemies\nEnd\n\
         AudioEvent Loud\n  Sounds = thud01\n  Type = world everyone\nEnd\n",
    );
    let cases = [
        ("Mine", Local, Outcome::Queued),
        ("Mine", Ally, Outcome::NotForLocal),
        ("Mine", Enemy, Outcome::NotForLocal),
        ("Chatter", Local, Outcome::NotForLocal),
        ("Chatter", Ally, Outcome::Queued),
        ("Chatter", Enemy, Outcome::Queued),
        ("Loud", Enemy, Outcome::Queued),
    ];
    for (event, audience, want) in cases {
        let request = EventRequest::at(event, [0.0, 0.0, 0.0]).heard_as(audience);
        assert_eq!(
            audio.play(&fixture.vfs, &request),
            want,
            "{event} as {audience:?}"
        );
    }
}

#[test]
fn a_multisound_plays_every_subsound_or_with_play_one_just_one() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio(
        "AudioEvent Voice\n  Sounds = thud01\nEnd\n\
         AudioEvent Foley\n  Sounds = thud01\nEnd\n\
         Multisound Both\n  Subsounds = Voice Foley\nEnd\n\
         Multisound Either\n  Control = PLAY_ONE\n  Subsounds = Voice Foley\nEnd\n\
         Multisound Loop\n  Subsounds = Loop\nEnd\n",
    );
    for (event, queued) in [("Both", 2), ("Either", 1), ("Loop", 0)] {
        let before = audio.request_stats().queued;
        let outcome = audio.play(&fixture.vfs, &EventRequest::new(event));
        let played = audio.request_stats().queued - before;
        assert_eq!(played, queued, "{event}: {outcome:?}");
    }
}

#[test]
fn a_stop_by_a_multisound_name_reaches_its_subsounds() {
    let audio = audio(
        "AudioEvent Wheel
  Sounds = thud01
  Control = loop
End
         AudioEvent Rattle
  Sounds = thud01
  Control = loop
End
         AudioEvent Creak
  Sounds = thud01
End
         Multisound Inner
  Subsounds = Rattle Creak
End
         Multisound Cart
  Subsounds = Wheel Inner
End
         Multisound Echo
  Subsounds = Echo Wheel
End
",
    );
    let key = |name: &str| audio.library().key(name).unwrap();
    let mut cart = audio.played_keys("Cart");
    cart.sort();
    let mut want = vec![key("Wheel"), key("Rattle"), key("Creak")];
    want.sort();
    assert_eq!(cart, want);
    assert_eq!(audio.played_keys("Wheel"), vec![key("Wheel")]);
    assert_eq!(audio.played_keys("Echo"), vec![key("Wheel")]);
    assert!(audio.played_keys("Nothing").is_empty());
}

#[test]
fn a_shrouded_event_is_refused_where_the_local_player_cannot_see() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio(
        "AudioEvent Hidden\n  Sounds = thud01\n  Type = world shrouded\n  MaxRange = 1000\nEnd\n",
    );
    let request = EventRequest::at("Hidden", [0.0, 0.0, 0.0]);
    assert_eq!(
        audio.play(&fixture.vfs, &request.clone().visible(false)),
        Outcome::Shrouded
    );
    assert_eq!(
        audio.play(&fixture.vfs, &request.visible(true)),
        Outcome::Queued
    );
}

#[test]
fn a_critical_event_ignores_the_shroud_and_the_distance() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio(
        "AudioEvent Doom\n  Sounds = thud01\n  Type = world shrouded\n  Priority = critical\n  MaxRange = 100\nEnd\n",
    );
    let far_and_hidden = EventRequest::at("Doom", [100_000.0, 0.0, 0.0]).visible(false);
    assert_eq!(audio.play(&fixture.vfs, &far_and_hidden), Outcome::Queued);
    assert_eq!(audio.request_stats().shrouded, 0);
}

#[test]
fn play_percent_refuses_roughly_its_share() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio("AudioEvent Maybe\n  Sounds = thud01\n  PlayPercent = 50\nEnd\n");
    let mut queued = 0;
    for _ in 0..2_000 {
        if audio.play(&fixture.vfs, &EventRequest::new("Maybe")) == Outcome::Queued {
            queued += 1;
        }
    }
    assert!(
        (700..1_300).contains(&queued),
        "queued {queued} of 2000 at PlayPercent = 50"
    );
}

#[test]
fn play_percent_of_one_hundred_always_plays() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio("AudioEvent Always\n  Sounds = thud01\n  PlayPercent = 100\nEnd\n");
    for _ in 0..200 {
        assert_eq!(
            audio.play(&fixture.vfs, &EventRequest::new("Always")),
            Outcome::Queued
        );
    }
}

#[test]
fn sequential_control_steps_through_the_variants_in_order() {
    let fixture = Fixture::new(&[
        ("a.wav", wav(441, 44100, 1000)),
        ("b.wav", wav(441, 44100, 2000)),
        ("c.wav", wav(441, 44100, 3000)),
    ]);
    let mut audio = audio("AudioEvent Steps\n  Sounds = a b c\n  Control = sequential\nEnd\n");
    for _ in 0..6 {
        assert_eq!(
            audio.play(&fixture.vfs, &EventRequest::new("Steps")),
            Outcome::Queued
        );
    }
    assert_eq!(audio.bank_stats().loaded, 3);
    assert_eq!(audio.bank_stats().hits, 3);
}

#[test]
fn a_weighted_variant_list_favours_the_heavy_entry() {
    let fixture = Fixture::new(&[
        ("common.wav", wav(441, 44100, 1000)),
        ("rare.wav", wav(441, 44100, 2000)),
    ]);
    let mut audio = audio("AudioEvent Weighted\n  Sounds = common:9000 rare:1000\nEnd\n");
    for _ in 0..500 {
        audio.play(&fixture.vfs, &EventRequest::new("Weighted"));
    }
    assert_eq!(audio.bank_stats().loaded, 2);
    assert_eq!(audio.bank_stats().hits, 498);
}

#[test]
fn a_silent_engine_still_runs_every_gate() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio("AudioEvent Thud\n  Sounds = thud01\nEnd\n");
    assert!(audio.is_silent());
    assert_eq!(
        audio.play(&fixture.vfs, &EventRequest::new("Thud")),
        Outcome::Queued
    );
    let line = audio.readout();
    for token in ["audio:", "rolloff", "requests", "voices", "bank"] {
        assert!(line.contains(token), "{line} is missing {token}");
    }
}

#[test]
fn duration_comes_from_the_header_and_takes_the_longest_variant() {
    let fixture = Fixture::new(&[
        ("short.wav", wav(44_100, 44_100, 1000)),
        ("long.wav", wav(88_200, 44_100, 1000)),
    ]);
    let library = library("AudioEvent Pair\n  Sounds = short long\nEnd\n");
    assert_eq!(
        library.event_duration_ms(&fixture.vfs, "Pair"),
        Some(2_000),
        "the deadline must not depend on the variant chosen"
    );
    assert_eq!(library.event_duration_ms(&fixture.vfs, "NotAnEvent"), None);
}

#[test]
fn duration_adds_the_attack_and_decay_to_the_body() {
    let fixture = Fixture::new(&[
        ("lead.wav", wav(22_050, 44_100, 1000)),
        ("body.wav", wav(88_200, 44_100, 1000)),
        ("tail.wav", wav(44_100, 44_100, 1000)),
    ]);
    let library = library(
        "AudioEvent Framed
  Attack = lead
  Sounds = body
  Decay = tail
End
",
    );
    assert_eq!(
        library.event_duration_ms(&fixture.vfs, "Framed"),
        Some(3_500)
    );
}

#[test]
fn duration_is_the_same_answer_every_time_it_is_asked() {
    let fixture = Fixture::new(&[("tone.wav", wav(22_050, 44_100, 1000))]);
    let library = library("AudioEvent Tone\n  Sounds = tone\nEnd\n");
    let first = library.event_duration_ms(&fixture.vfs, "Tone");
    assert_eq!(first, Some(500));
    for _ in 0..100 {
        assert_eq!(library.event_duration_ms(&fixture.vfs, "Tone"), first);
    }
}

#[test]
fn keys_are_assigned_in_sorted_name_order() {
    let library =
        library("AudioEvent Zebra\n  Sounds = z\nEnd\nAudioEvent Apple\n  Sounds = a\nEnd\n");
    let apple = library.key("Apple").unwrap();
    let zebra = library.key("ZEBRA").unwrap();
    assert!(apple.0 < zebra.0);
    assert_eq!(library.name(apple), Some("Apple"));
    assert_eq!(library.name(zebra), Some("Zebra"));
}

#[test]
fn is_audio_document_names_the_eight_files_and_nothing_else() {
    for path in [
        "data/ini/soundeffects.ini",
        "data/ini/default/soundeffects.ini",
        "data/ini/VOICE.INI",
        "data/ini/audiosettings.ini",
    ] {
        assert!(super::is_audio_document(path), "{path}");
    }
    for path in ["data/ini/object.ini", "data/ini/weapon.ini", "sounds.txt"] {
        assert!(!super::is_audio_document(path), "{path}");
    }
}

fn settled(audio: &mut SageAudio, vfs: &Vfs, dt: f64) -> Vec<crate::music::MusicEvent> {
    audio.music_settle(std::time::Duration::from_secs(5));
    audio.music_update(vfs, dt)
}

fn finished_flags(events: &[crate::music::MusicEvent]) -> Vec<Option<String>> {
    events
        .iter()
        .filter_map(|e| match e {
            crate::music::MusicEvent::Finished { notify, .. } => Some(notify.clone()),
            _ => None,
        })
        .collect()
}

const ONE_SECOND_TRACKS: &str = "MusicTrack TuneA
  Filename = tunea.wav
End
     MusicTrack TuneB
  Filename = tuneb.wav
End
";

fn music_fixture() -> Fixture {
    Fixture::new(&[
        ("data/audio/tracks/tunea.wav", wav(1000, 1000, 9000)),
        ("data/audio/tracks/tuneb.wav", wav(1000, 1000, 9000)),
    ])
}

#[test]
fn a_finite_music_request_finishes_after_its_plays_and_raises_its_flag() {
    let fixture = music_fixture();
    let mut audio = audio(ONE_SECOND_TRACKS);
    let request = crate::music::MusicRequest {
        times: Some(2),
        notify: Some("TuneDone".to_string()),
        ..crate::music::MusicRequest::forever("TuneA")
    };
    audio.music_replace(&fixture.vfs, request);
    let started = settled(&mut audio, &fixture.vfs, 0.0);
    assert!(
        matches!(
            started.first(),
            Some(crate::music::MusicEvent::Started { .. })
        ),
        "{started:?}"
    );
    assert!(finished_flags(&audio.music_update(&fixture.vfs, 1.5)).is_empty());
    assert_eq!(
        finished_flags(&audio.music_update(&fixture.vfs, 0.6)),
        [Some("TuneDone".to_string())]
    );
    assert!(!audio.music().is_playing());
}

#[test]
fn a_pushed_request_holds_the_music_beneath_and_a_pop_resumes_it() {
    let fixture = music_fixture();
    let mut audio = audio(ONE_SECOND_TRACKS);
    audio.music_replace(&fixture.vfs, crate::music::MusicRequest::forever("TuneA"));
    settled(&mut audio, &fixture.vfs, 0.0);
    let over = crate::music::MusicRequest {
        times: Some(1),
        ..crate::music::MusicRequest::forever("TuneB")
    };
    audio.music_push(&fixture.vfs, over);
    settled(&mut audio, &fixture.vfs, 0.0);
    assert!(
        audio.music().describe().contains("held TuneA"),
        "{}",
        audio.music().describe()
    );
    audio.music_update(&fixture.vfs, 1.1);
    audio.music_pop(false, true);
    let line = audio.music().describe();
    assert!(line.contains("playing TuneA"), "{line}");
    assert_eq!(audio.music().current(), Some("TuneA"));
}

#[test]
fn a_looping_play_one_multisound_picks_again_when_its_pick_ends() {
    let fixture = music_fixture();
    let mut audio = audio(&format!(
        "{ONE_SECOND_TRACKS}Multisound Playlist
  Control = PLAY_ONE LOOP
  Subsounds = TuneA TuneB
End
         Multisound Single
  Control = PLAY_ONE
  Subsounds = TuneA TuneB
End
"
    ));
    for (name, picks) in [("Playlist", 3), ("Single", 1)] {
        audio.music_replace(&fixture.vfs, crate::music::MusicRequest::forever(name));
        let mut starts = 0;
        for _ in 0..5 {
            let events = settled(&mut audio, &fixture.vfs, 1.01);
            starts += events
                .iter()
                .filter(|e| matches!(e, crate::music::MusicEvent::Started { .. }))
                .count();
        }
        assert_eq!(starts, picks, "{name}");
    }
}

#[test]
fn reach_follows_the_listener_and_a_placements_own_range() {
    let audio = audio(
        "AudioEvent Brook\n  Sounds = brook01\n  Type = world\n  MinRange = 10\n  MaxRange = 200\nEnd\n",
    );
    let none = crate::runtime::Overrides::default();
    let near = audio.reach("Brook", [50.0, 0.0, 0.0], &none);
    let far = audio.reach("Brook", [500.0, 0.0, 0.0], &none);
    assert_eq!(near, crate::runtime::Reach::Audible);
    assert_eq!(far, crate::runtime::Reach::OutOfRange);
    let wider = crate::runtime::Overrides {
        max_range: Some(1000.0),
        ..crate::runtime::Overrides::default()
    };
    assert_eq!(
        audio.reach("Brook", [500.0, 0.0, 0.0], &wider),
        crate::runtime::Reach::Audible,
        "a placement's own MaxRange replaces the definition's"
    );
    assert_eq!(
        audio.reach("Nothing", [0.0, 0.0, 0.0], &none),
        crate::runtime::Reach::Unknown
    );
}

#[test]
fn overrides_replace_the_loop_flag_and_volume_for_one_request() {
    let audio = audio("AudioEvent Mill\n  Sounds = mill01\n  Control = loop\n  Volume = 80\nEnd\n");
    let plain = audio
        .effective_def("Mill", &crate::runtime::Overrides::default())
        .unwrap();
    assert!(plain.control.has(mint_sage_ini::audio::ControlFlags::LOOP));
    let changed = audio
        .effective_def(
            "Mill",
            &crate::runtime::Overrides {
                looping: Some(false),
                volume: Some(0.25),
                ..crate::runtime::Overrides::default()
            },
        )
        .unwrap();
    assert!(!changed
        .control
        .has(mint_sage_ini::audio::ControlFlags::LOOP));
    assert_eq!(changed.volume, Some(0.25));
}

#[test]
fn random_delay_ms_draws_within_bounds_and_varies() {
    let mut audio = audio("AudioEvent Mill\n  Sounds = mill01\nEnd\n");
    let draws: Vec<u32> = (0..32).map(|_| audio.random_delay_ms(1000, 2500)).collect();
    assert!(
        draws.iter().all(|ms| (1000..=2500).contains(ms)),
        "{draws:?}"
    );
    assert!(
        draws.iter().any(|ms| *ms != draws[0]),
        "a fixed low bound would never vary: {draws:?}"
    );
}

#[test]
fn a_disabled_event_is_refused_until_enabled() {
    let fixture = Fixture::new(&[("thud01.wav", wav(441, 44100, 20000))]);
    let mut audio = audio(
        "AudioEvent Thud\n  Sounds = thud01\nEnd\nAudioEvent Knock\n  Sounds = thud01\nEnd\n\
         Multisound Both\n  Subsounds = Thud Knock\nEnd\n",
    );
    let play =
        |audio: &mut SageAudio, name: &str| audio.play(&fixture.vfs, &EventRequest::new(name));
    audio.disable_event("thud");
    assert_eq!(play(&mut audio, "Thud"), Outcome::Disabled);
    assert_eq!(play(&mut audio, "Knock"), Outcome::Queued);
    assert_eq!(play(&mut audio, "Both"), Outcome::Queued);
    audio.enable_event("Thud");
    assert_eq!(play(&mut audio, "Thud"), Outcome::Queued);
    audio.disable_event("Thud");
    audio.disable_event("Knock");
    audio.enable_all_events();
    assert_eq!(play(&mut audio, "Knock"), Outcome::Queued);
    assert_eq!(
        audio.request_stats().disabled,
        2,
        "the direct play and the multisound's nested one"
    );
}
