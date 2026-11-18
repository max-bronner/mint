use super::*;
use crate::table::BlockTable;

fn parse(text: &str) -> Document {
    Document::parse("audio-test.ini", text, &BlockTable::audio_path())
        .expect("the fixture must parse")
}

fn index(text: &str) -> AudioIndex {
    let document = parse(text);
    assert!(document.is_balanced(), "{:?}", document.diagnostics());
    let mut index = AudioIndex::new();
    index.insert_document(&document);
    index
}

#[test]
fn a_plain_audio_event_reads_every_field_it_carries() {
    let index = index(
        "AudioEvent GondorSoldierVoiceAttack\n\
         \x20 Sounds = guarch_voiatta guarch_voiattb\n\
         \x20 Limit = 3\n\
         \x20 Volume = 110\n\
         \x20 MinVolume = 40\n\
         \x20 MinRange = 250\n\
         \x20 MaxRange = 700\n\
         \x20 Priority = high\n\
         \x20 Type = world shrouded everyone voice\n\
         \x20 Control = interrupt\n\
         \x20 SubmixSlider = voice\n\
         End\n",
    );
    let def = index.get("gondorsoldiervoiceattack").expect("by name");
    assert_eq!(def.kind, AudioEventKind::Sound);
    assert_eq!(def.sounds.len(), 2);
    assert_eq!(def.sounds[0].name, "guarch_voiatta");
    assert_eq!(def.sounds[0].weight, DEFAULT_SOUND_WEIGHT);
    assert_eq!(def.limit, 3);
    assert!((def.volume.unwrap() - 1.10).abs() < 1e-6);
    assert!((def.min_volume.unwrap() - 0.40).abs() < 1e-6);
    assert_eq!(def.min_range, Some(250.0));
    assert_eq!(def.max_range, Some(700.0));
    assert_eq!(def.priority, AudioPriority::High);
    assert!(def.types.is_positional());
    assert!(def.types.has(TypeFlags::SHROUDED));
    assert!(def.types.has(TypeFlags::VOICE));
    assert!(def.control.has(ControlFlags::INTERRUPT));
    assert_eq!(def.submix, Some(Submix::Voice));
    assert!(!def.unresolved_range);
}

#[test]
fn a_weighted_sound_list_keeps_its_weights() {
    let index = index("AudioEvent Weighted\n  Sounds = one two:2000 three:200\nEnd\n");
    let def = index.get("weighted").unwrap();
    assert_eq!(def.sounds.len(), 3);
    assert_eq!(def.sounds[0].weight, 1000);
    assert_eq!(def.sounds[1].name, "two");
    assert_eq!(def.sounds[1].weight, 2000);
    assert_eq!(def.sounds[2].weight, 200);
    assert_eq!(def.total_weight(), 3200);
}

#[test]
fn an_event_that_names_no_sounds_is_still_a_definition() {
    let index = index("AudioEvent Silent\n  Sounds =\n  Limit = 1\nEnd\n");
    let def = index.get("silent").unwrap();
    assert!(def.sounds.is_empty());
    assert!(!def.has_audio());
    assert_eq!(def.limit, 1);
    assert_eq!(index.len(), 1);
}

#[test]
fn the_four_streamed_kinds_carry_a_filename_with_its_extension() {
    let index = index(
        "MusicTrack Game_Silent\n  Filename = Silence60.mp3\nEnd\n\
         DialogEvent Narrator01\n  Filename = narr01.mp3\n  Volume = 100\nEnd\n\
         AmbientStream AmbientShireStream\n  Filename = WAShire_ambien1.mp3\n  Volume = 100\n  Type = everyone\nEnd\n\
         StreamedSound TestWeatherLoop\n  Filename = weatherloop.mp3\n  Control = Loop\n  Priority = High\nEnd\n",
    );
    assert_eq!(index.len(), 4);
    let music = index.get("game_silent").unwrap();
    assert_eq!(music.kind, AudioEventKind::Music);
    assert_eq!(music.filename.as_deref(), Some("Silence60.mp3"));
    assert!(music.kind.is_streamed());
    assert_eq!(music.effective_submix(), Submix::Music);
    assert_eq!(
        index.get("narrator01").unwrap().kind,
        AudioEventKind::Dialog
    );
    assert_eq!(
        index.get("ambientshirestream").unwrap().effective_submix(),
        Submix::Ambient
    );
    let stream = index.get("testweatherloop").unwrap();
    assert_eq!(stream.kind, AudioEventKind::Stream);
    assert!(stream.kind.is_streamed());
    assert_eq!(stream.filename.as_deref(), Some("weatherloop.mp3"));
    assert!(stream.control.has(ControlFlags::LOOP));
    assert_eq!(stream.priority, AudioPriority::High);
}

#[test]
fn an_unexpanded_macro_range_is_flagged_rather_than_guessed() {
    let index =
        index("AudioEvent Chatter\n  MinRange = 100\n  MaxRange = BATTLE_CHATTER_RANGE\nEnd\n");
    let def = index.get("chatter").unwrap();
    assert_eq!(def.min_range, Some(100.0));
    assert_eq!(def.max_range, None);
    assert!(def.unresolved_range, "the caller must be able to see this");
}

#[test]
fn the_rotwk_control_vocabulary_parses_and_so_does_generals() {
    let index = index(
        "AudioEvent Rotwk\n  Control = loop fade_on_start fade_on_kill sequential interrupt\nEnd\n\
         AudioEvent Generals\n  Control = random all postdelay\nEnd\n",
    );
    let rotwk = index.get("rotwk").unwrap().control;
    for bit in [
        ControlFlags::LOOP,
        ControlFlags::FADE_ON_START,
        ControlFlags::FADE_ON_KILL,
        ControlFlags::SEQUENTIAL,
        ControlFlags::INTERRUPT,
    ] {
        assert!(rotwk.has(bit), "{:#b}", rotwk.0);
    }
    let generals = index.get("generals").unwrap().control;
    assert!(generals.has(ControlFlags::RANDOM));
    assert!(generals.has(ControlFlags::ALL));
    assert!(generals.has(ControlFlags::POSTDELAY));
}

#[test]
fn a_type_list_with_the_bit_modifier_prefix_still_parses() {
    let index = index("AudioEvent Modified\n  Type = +DEFAULT world\nEnd\n");
    assert!(index.get("modified").unwrap().types.is_positional());
}

#[test]
fn priority_falls_back_to_normal_only_when_no_default_definition_was_read() {
    let index = index("AudioEvent Plain\n  Sounds = a\nEnd\n");
    assert_eq!(index.get("plain").unwrap().priority, AudioPriority::Normal);
    assert!(index.default_for(AudioEventKind::Sound).is_none());
}

#[test]
fn audio_settings_reads_the_shipped_values_and_both_percent_spellings() {
    let index = index(
        "AudioSettings\n\
         \x20 AudioRoot = Data\\Audio\n\
         \x20 SoundsFolder = Sounds\n\
         \x20 MusicFolder = Tracks\n\
         \x20 StreamingFolder = Speech\n\
         \x20 AmbientStreamFolder = AmbientStreams\n\
         \x20 SoundsExtension = wav\n\
         \x20 SampleCount2D = 4\n\
         \x20 SampleCount3D = 25\n\
         \x20 StreamCount = 3\n\
         \x20 MinSampleVolume = 2\n\
         \x20 AudioFootprintInBytes = 8388608\n\
         \x20 DefaultSoundVolume = 70%\n\
         \x20 DefaultMusicVolume = 70\n\
         End\n",
    );
    assert!(index.saw_settings());
    let settings = index.settings();
    assert_eq!(settings.sample_count_2d, 4);
    assert_eq!(settings.sample_count_3d, 25);
    assert_eq!(settings.stream_count, 3);
    assert_eq!(settings.audio_footprint_in_bytes, 8_388_608);
    assert!((settings.min_sample_volume - 0.02).abs() < 1e-6);
    assert!((settings.default_sound_volume - 0.70).abs() < 1e-6);
    assert!(
        (settings.default_music_volume - 0.70).abs() < 1e-6,
        "got {}",
        settings.default_music_volume
    );
    assert_eq!(
        settings.folder_for(AudioEventKind::Sound),
        "Data/Audio/Sounds"
    );
    assert_eq!(
        settings.folder_for(AudioEventKind::Music),
        "Data/Audio/Tracks"
    );
}

#[test]
fn a_renamed_sounds_folder_moves_every_path() {
    let index = index("AudioSettings\n  AudioRoot = Mod\\Sfx\n  SoundsFolder = Wav\nEnd\n");
    assert_eq!(
        index.settings().folder_for(AudioEventKind::Sound),
        "Mod/Sfx/Wav"
    );
}

#[test]
fn an_index_with_no_settings_block_still_has_the_shipped_defaults() {
    let index = index("AudioEvent A\n  Sounds = a\nEnd\n");
    assert!(!index.saw_settings());
    assert_eq!(index.settings().sample_count_3d, 25);
    assert_eq!(index.settings().sounds_extension, "wav");
}

#[test]
fn the_last_definition_of_a_name_wins() {
    let index =
        index("AudioEvent Shared\n  Volume = 50\nEnd\nAudioEvent Shared\n  Volume = 90\nEnd\n");
    assert_eq!(index.len(), 1);
    assert_eq!(index.definitions(), 2);
    assert_eq!(index.redefinitions(), 1);
    assert!((index.get("shared").unwrap().volume.unwrap() - 0.90).abs() < 1e-6);
}

#[test]
fn misc_audio_maps_an_engine_sound_name_to_an_event_name() {
    let index =
        index("MiscAudio\n  NoCanDoSound = NoCanDoSound\n  BuildingDisabled = NoSound\nEnd\n");
    assert_eq!(index.misc("nocandosound"), Some("NoCanDoSound"));
    assert_eq!(index.misc("BuildingDisabled"), Some("NoSound"));
    assert_eq!(index.misc("NotThere"), None);
}

#[test]
fn delay_and_pitch_shift_read_their_two_values() {
    let index = index(
        "AudioEvent Varied\n  Delay = 0 1000\n  PitchShift = -5 5\n  VolumeShift = -10\nEnd\n",
    );
    let def = index.get("varied").unwrap();
    assert_eq!(def.delay, Some((0, 1000)));
    let (low, high) = def.pitch_shift.unwrap();
    assert!((low + 0.05).abs() < 1e-6 && (high - 0.05).abs() < 1e-6);
    assert!((def.volume_shift.unwrap() + 0.10).abs() < 1e-6);
}

#[test]
fn a_block_that_is_not_an_audio_block_is_ignored_rather_than_mangled() {
    let mut index = AudioIndex::new();
    let document = parse("AudioEvent Real\n  Sounds = a\nEnd\n");
    index.insert_document(&document);
    assert_eq!(index.len(), 1);
    let other = Block {
        keyword: "Object".to_string(),
        args: vec!["GondorArcher".to_string()],
        children: Vec::new(),
        file: "x".into(),
        line: 1,
    };
    index.insert_block(&other);
    assert_eq!(index.len(), 1);
}

#[test]
fn a_nameless_event_is_dropped_rather_than_keyed_on_nothing() {
    let index = index("AudioEvent\n  Sounds = a\nEnd\n");
    assert!(index.is_empty());
}

#[test]
fn an_event_inherits_default_sound_effect_field_by_field() {
    let index = index(
        "AudioEvent DefaultSoundEffect\n\
         \x20 Volume = 100\n\
         \x20 Priority = high\n\
         \x20 MinVolume = 0\n\
         \x20 Limit = 4\n\
         \x20 MinRange = 160.000\n\
         \x20 MaxRange = 640.000\n\
         \x20 PlayPercent = 100\n\
         End\n\
         AudioEvent Inherits\n  Sounds = thump\nEnd\n\
         AudioEvent Overrides\n  Sounds = crash\n  Priority = lowest\n  MaxRange = 2000\nEnd\n",
    );
    assert!(index.default_for(AudioEventKind::Sound).is_some());

    let inherits = index.get("inherits").unwrap();
    assert_eq!(inherits.priority, AudioPriority::High);
    assert_eq!(inherits.limit, 4);
    assert_eq!(inherits.min_range, Some(160.0));
    assert_eq!(inherits.max_range, Some(640.0));
    assert!((inherits.volume.unwrap() - 1.0).abs() < 1e-6);
    assert!((inherits.play_percent.unwrap() - 1.0).abs() < 1e-6);
    assert_eq!(inherits.sounds.len(), 1);
    assert_eq!(inherits.sounds[0].name, "thump");

    let overrides = index.get("overrides").unwrap();
    assert_eq!(overrides.priority, AudioPriority::Lowest);
    assert_eq!(overrides.max_range, Some(2000.0));
    assert_eq!(
        overrides.min_range,
        Some(160.0),
        "fields the block does not set still inherit"
    );
}

#[test]
fn a_default_read_after_a_definition_does_not_reach_backwards() {
    let index = index(
        "AudioEvent Early\n  Sounds = a\nEnd\n\
         AudioEvent DefaultSoundEffect\n  Priority = critical\nEnd\n\
         AudioEvent Late\n  Sounds = b\nEnd\n",
    );
    assert_eq!(index.get("early").unwrap().priority, AudioPriority::Normal);
    assert_eq!(index.get("late").unwrap().priority, AudioPriority::Critical);
}

#[test]
fn a_bare_type_list_replaces_and_a_modified_one_adds() {
    let index = index(
        "AudioEvent DefaultSoundEffect\n  Type = ui player\nEnd\n\
         AudioEvent Replaces\n  Type = world\nEnd\n\
         AudioEvent Adds\n  Type = +world\nEnd\n\
         AudioEvent Removes\n  Type = -ui\nEnd\n",
    );
    let replaces = index.get("replaces").unwrap().types;
    assert!(replaces.has(TypeFlags::WORLD));
    assert!(!replaces.has(TypeFlags::UI), "a bare list replaces");

    let adds = index.get("adds").unwrap().types;
    assert!(adds.has(TypeFlags::WORLD) && adds.has(TypeFlags::UI));

    let removes = index.get("removes").unwrap().types;
    assert!(!removes.has(TypeFlags::UI) && removes.has(TypeFlags::PLAYER));
}

#[test]
fn an_empty_sounds_line_clears_an_inherited_list() {
    let index = index(
        "AudioEvent DefaultSoundEffect\n  Sounds = fallback\nEnd\n\
         AudioEvent Quiet\n  Sounds =\nEnd\n",
    );
    assert!(index.get("quiet").unwrap().sounds.is_empty());
}

#[test]
fn a_define_resolves_a_range_that_would_otherwise_be_unresolved() {
    let text = "#define BATTLE_CHATTER_RANGE 700\n\
                AudioEvent Chatter\n  MaxRange = BATTLE_CHATTER_RANGE\nEnd\n";
    let document = parse(text);

    let mut without = AudioIndex::new();
    without.insert_document(&document);
    assert!(without.get("chatter").unwrap().unresolved_range);

    let mut with = AudioIndex::new();
    with.insert_defines(document.defines());
    assert_eq!(with.macros(), 1);
    with.insert_document(&document);
    let def = with.get("chatter").unwrap();
    assert_eq!(def.max_range, Some(700.0));
    assert!(!def.unresolved_range);
}

#[test]
fn only_the_value_that_does_not_read_is_refused() {
    let text = "#define CHEER_MIN 1000\n#define CHEER_MAX 3000\n\
                AudioEvent Cheer\n\
                \x20 Delay = CHEER_MIN #ADD( CHEER_MAX 5000 )\n\
                \x20 VolumeSliderMultiplier = Slider:Voice Multiplier:70\n\
                \x20 SubmixSlider = None\n\
                \x20 Volume = loud\n\
                End\n";
    let document = parse(text);
    let mut index = AudioIndex::new();
    index.insert_defines(document.defines());
    index.insert_document(&document);
    let refused: Vec<String> = index
        .value_errors()
        .take()
        .iter()
        .map(|e| e.field.clone())
        .collect();
    assert_eq!(refused, ["Volume"]);
}

#[test]
fn a_multisound_reads_its_weighted_subsounds_and_both_control_flags() {
    let index = index(
        "Multisound PlaylistA\n\
         \x20 Control = PLAY_ONE LOOP\n\
         \x20 Subsounds = TrackOne:3000 TrackTwo\n\
         \x20 Subsounds = TrackThree\n\
         End\n\
         Multisound PlaylistB\n\
         \x20 Control = PLAY_ONE\n\
         \x20 Subsounds = TrackOne\n\
         End\n",
    );
    let a = index.multisound("playlista").expect("read");
    let names: Vec<(&str, u32)> = a
        .subsounds
        .iter()
        .map(|v| (v.name.as_str(), v.weight))
        .collect();
    assert_eq!(
        names,
        [
            ("TrackOne", 3000),
            ("TrackTwo", DEFAULT_SOUND_WEIGHT),
            ("TrackThree", DEFAULT_SOUND_WEIGHT)
        ]
    );
    assert!(a.play_one && a.looping);
    let b = index.multisound("PlaylistB").expect("read");
    assert!(b.play_one && !b.looping);
}

#[test]
fn a_crowd_response_threshold_picks_by_the_largest_minimum_at_or_below_the_count() {
    let index = index(
        "CrowdResponse TestCrowd\n\
         \x20 Weight = 1\n\
         \x20 Threshold 5\n\
         \x20   VoiceAttack = FiveAttack\n\
         \x20 End\n\
         \x20 Threshold 20\n\
         \x20   VoiceAttack = TwentyAttack\n\
         \x20 End\n\
         End\n",
    );
    let crowd = index.crowd_response("testcrowd").expect("read");
    assert_eq!(crowd.weight, 1);
    assert!(crowd.threshold_for(4).is_none());
    assert_eq!(
        crowd
            .threshold_for(5)
            .and_then(|t| t.fields.get("VoiceAttack")),
        Some(&"FiveAttack".to_string())
    );
    assert_eq!(
        crowd
            .threshold_for(19)
            .and_then(|t| t.fields.get("VoiceAttack")),
        Some(&"FiveAttack".to_string())
    );
    assert_eq!(
        crowd
            .threshold_for(20)
            .and_then(|t| t.fields.get("VoiceAttack")),
        Some(&"TwentyAttack".to_string())
    );
}

#[test]
fn a_crowd_response_refuses_an_eva_sound_and_a_duplicate_minimum() {
    let index = index(
        "CrowdResponse BadCrowd\n\
         \x20 Threshold 5\n\
         \x20   VoiceAttack = EVA:SomeEvent\n\
         \x20 End\n\
         \x20 Threshold 5\n\
         \x20   VoiceMove = AgainFive\n\
         \x20 End\n\
         End\n",
    );
    let crowd = index.crowd_response("badcrowd").expect("read");
    assert_eq!(
        crowd.thresholds.len(),
        1,
        "the duplicate minimum is refused"
    );
    assert!(
        !index.value_errors().is_empty(),
        "an EVA: sound and a duplicate minimum are both refused values"
    );
}

#[test]
fn a_crowd_response_threshold_reads_its_nested_unit_specific_sounds() {
    let index = index(
        "CrowdResponse TestCrowd\n\
         \x20 Threshold 5\n\
         \x20   VoiceAttack = FiveAttack\n\
         \x20   UnitSpecificSounds\n\
         \x20     TankTurretMove = M\n\
         \x20   End\n\
         \x20 End\n\
         End\n",
    );
    let crowd = index.crowd_response("testcrowd").expect("read");
    let threshold = crowd.threshold_for(5).expect("the threshold read");
    assert_eq!(
        threshold.unit_specific_sounds.get("TankTurretMove"),
        Some(&"M".to_string())
    );
}

#[test]
fn an_unknown_field_errors_at_the_crowd_response_and_threshold_levels() {
    let index = index(
        "CrowdResponse BadCrowd\n\
         \x20 SomeInventedField = 1\n\
         \x20 Threshold 5\n\
         \x20   SomeOtherInventedField = 1\n\
         \x20 End\n\
         End\n",
    );
    assert_eq!(
        index.value_errors().len(),
        2,
        "an unknown field at the CrowdResponse level and one at the Threshold level both error"
    );
}
