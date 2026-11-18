use super::*;

fn flat(frames: usize, value: i16, rate: u32) -> Arc<Clip> {
    Arc::new(Clip::from_pcm16(Pcm16 {
        sample_rate: rate,
        channels: 1,
        samples: vec![value; frames],
    }))
}

fn flat_stereo(frames: usize, l: i16, r: i16, rate: u32) -> Arc<Clip> {
    let mut samples = Vec::with_capacity(frames * 2);
    for _ in 0..frames {
        samples.push(l);
        samples.push(r);
    }
    Arc::new(Clip::from_pcm16(Pcm16 {
        sample_rate: rate,
        channels: 2,
        samples,
    }))
}

fn tiny_budget() -> MixerConfig {
    MixerConfig {
        voices_2d: 2,
        voices_3d: 2,
        voices_stream: 1,
        min_sample_volume: 0.02,
        rolloff: Rolloff::Linear,
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0e-4
}

#[test]
fn a_mono_clip_reaches_both_output_channels() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(64, 16384, 44100);
    assert!(matches!(
        mixer.play(Play::new(clip, EventKey(1))),
        Admission::Started(_)
    ));

    let mut out = vec![0.0f32; 32];
    mixer.render(&mut out, 2, 44100);
    let want = 0.5 * 0.5f32.sqrt();
    assert!(near(out[0], want), "left {} want {want}", out[0]);
    assert!(near(out[1], want), "right {} want {want}", out[1]);
}

#[test]
fn a_non_looping_clip_stops_at_its_end() {
    let mut mixer = Mixer::new(MixerConfig::default());
    mixer.play(Play::new(flat(4, 32767, 44100), EventKey(1)));
    let mut out = vec![0.0f32; 64];
    mixer.render(&mut out, 2, 44100);
    assert_eq!(mixer.active(), 0);
    assert_eq!(mixer.stats().finished, 1);
    assert!(near(out[62], 0.0));
}

#[test]
fn a_looping_clip_keeps_going_past_its_end() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut request = Play::new(flat(4, 32767, 44100), EventKey(1));
    request.looping = true;
    mixer.play(request);
    let mut out = vec![0.0f32; 64];
    mixer.render(&mut out, 2, 44100);
    assert_eq!(mixer.active(), 1);
    assert!(out[62].abs() > 0.1, "still sounding: {}", out[62]);
}

#[test]
fn a_clip_at_half_the_device_rate_lasts_twice_as_long() {
    let mut mixer = Mixer::new(MixerConfig::default());
    mixer.play(Play::new(flat(10, 32767, 22050), EventKey(1)));
    let mut out = vec![0.0f32; 2 * 19];
    mixer.render(&mut out, 2, 44100);
    assert_eq!(mixer.active(), 1, "10 source frames should fill 20 output");

    let mut rest = vec![0.0f32; 2 * 4];
    mixer.render(&mut rest, 2, 44100);
    assert_eq!(mixer.active(), 0);
}

#[test]
fn panning_puts_a_sound_on_the_side_it_is_on() {
    let mut mixer = Mixer::new(MixerConfig::default());
    mixer.set_listener(Listener {
        position: [0.0, 0.0, 0.0],
        right: [1.0, 0.0, 0.0],
        gain: 1.0,
    });
    let mut request = Play::new(flat(64, 32767, 44100), EventKey(1));
    request.spatial = Some(Spatial {
        position: [10.0, 0.0, 0.0],
        min_range: 100.0,
        max_range: 1000.0,
        min_gain: 0.0,
    });
    mixer.play(request);

    let mut out = vec![0.0f32; 8];
    mixer.render(&mut out, 2, 44100);
    assert!(
        out[1] > out[0],
        "right {} should beat left {}",
        out[1],
        out[0]
    );
    assert!(near(out[0], 0.0), "hard right leaves nothing on the left");
}

#[test]
fn spatial_level_is_min_over_distance_floored_by_min_gain() {
    let at = |min_gain: f32, gain: f32, d: f32| {
        let spatial = Spatial {
            position: [0.0; 3],
            min_range: 160.0,
            max_range: 640.0,
            min_gain,
        };
        spatial_level(Rolloff::InverseClamped, gain, &spatial, d)
    };
    let cases = [
        (0.0, 1.0, 100.0, 1.0),
        (0.0, 1.0, 320.0, 0.5),
        (0.0, 1.0, 400.0, 0.4),
        (0.0, 1.0, 640.0, 0.0),
        (1.05, 1.15, 400.0, 1.05),
        (1.05, 1.15, 5000.0, 1.05),
        (1.05, 1.15, 100.0, 1.15),
        (0.9, 0.5, 5000.0, 0.5),
    ];
    for (min_gain, gain, d, want) in cases {
        let got = at(min_gain, gain, d);
        assert!(
            near(got, want),
            "floor {min_gain} gain {gain} at {d}: {got}, want {want}"
        );
    }
}

#[test]
fn a_floored_sound_beyond_max_range_is_still_started() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut request = Play::new(flat(64, 32767, 44100), EventKey(1));
    request.spatial = Some(Spatial {
        position: [0.0, 5000.0, 0.0],
        min_range: 100.0,
        max_range: 900.0,
        min_gain: 0.5,
    });
    assert!(matches!(mixer.play(request), Admission::Started(_)));
}

#[test]
fn beyond_max_range_is_never_started() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut request = Play::new(flat(64, 32767, 44100), EventKey(1));
    request.spatial = Some(Spatial {
        position: [0.0, 5000.0, 0.0],
        min_range: 100.0,
        max_range: 900.0,
        min_gain: 0.0,
    });
    assert_eq!(
        mixer.play(request),
        Admission::Culled(CullReason::OutOfRange)
    );
    assert_eq!(mixer.stats().culled_range, 1);
}

#[test]
fn below_min_sample_volume_is_never_started() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut request = Play::new(flat(64, 32767, 44100), EventKey(1));
    request.gain = 0.001;
    assert_eq!(mixer.play(request), Admission::Culled(CullReason::TooQuiet));
    assert_eq!(mixer.stats().culled_quiet, 1);
}

#[test]
fn a_stereo_file_cannot_be_positioned() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut request = Play::new(flat_stereo(64, 32767, 32767, 44100), EventKey(1));
    request.spatial = Some(Spatial {
        position: [0.0, 0.0, 0.0],
        min_range: 100.0,
        max_range: 900.0,
        min_gain: 0.0,
    });
    assert_eq!(
        mixer.play(request),
        Admission::Culled(CullReason::StereoPositional)
    );
    let plain = Play::new(flat_stereo(64, 32767, 32767, 44100), EventKey(1));
    assert!(matches!(mixer.play(plain), Admission::Started(_)));
}

#[test]
fn the_event_limit_counts_by_event_not_by_emitter() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);
    let mut admitted = 0;
    for emitter in 0..10u64 {
        let mut request = Play::new(Arc::clone(&clip), EventKey(7));
        request.limit = 3;
        request.owner = Some(emitter);
        if matches!(mixer.play(request), Admission::Started(_)) {
            admitted += 1;
        }
    }
    assert_eq!(admitted, 3);
    assert_eq!(mixer.stats().culled_limit, 7);
}

#[test]
fn an_interrupting_request_replaces_the_oldest_instance() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);
    let mut first = Play::new(Arc::clone(&clip), EventKey(7));
    first.limit = 1;
    let Admission::Started(oldest) = mixer.play(first) else {
        panic!("the first instance must start")
    };

    let mut second = Play::new(Arc::clone(&clip), EventKey(7));
    second.limit = 1;
    second.interrupt = true;
    let Admission::Started(newer) = mixer.play(second) else {
        panic!("an interrupting request must start")
    };
    assert_ne!(oldest, newer);
    assert_eq!(mixer.active(), 1, "the older one was replaced, not added");
}

#[test]
fn one_voice_per_emitter_unless_it_interrupts() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);

    let mut speaking = Play::new(Arc::clone(&clip), EventKey(1));
    speaking.owner = Some(42);
    speaking.exclusive_owner = true;
    assert!(matches!(mixer.play(speaking), Admission::Started(_)));

    let mut second = Play::new(Arc::clone(&clip), EventKey(2));
    second.owner = Some(42);
    second.exclusive_owner = true;
    assert_eq!(mixer.play(second), Admission::Culled(CullReason::OwnerBusy));

    let mut urgent = Play::new(Arc::clone(&clip), EventKey(3));
    urgent.owner = Some(42);
    urgent.exclusive_owner = true;
    urgent.interrupt = true;
    assert!(matches!(mixer.play(urgent), Admission::Started(_)));
    assert_eq!(mixer.active(), 1);
}

#[test]
fn a_higher_priority_sound_evicts_a_lower_one_and_never_the_reverse() {
    let mut mixer = Mixer::new(tiny_budget());
    let clip = flat(4096, 32767, 44100);

    for key in 0..2u32 {
        let mut low = Play::new(Arc::clone(&clip), EventKey(key));
        low.priority = Priority::Low;
        assert!(matches!(mixer.play(low), Admission::Started(_)));
    }
    assert_eq!(mixer.active(), 2, "the 2D budget is full");

    let mut critical = Play::new(Arc::clone(&clip), EventKey(9));
    critical.priority = Priority::Critical;
    assert!(matches!(mixer.play(critical), Admission::Started(_)));
    assert_eq!(mixer.stats().evicted, 1);
    assert_eq!(mixer.active(), 2);

    let mut another_low = Play::new(Arc::clone(&clip), EventKey(10));
    another_low.priority = Priority::Low;
    assert_eq!(
        mixer.play(another_low),
        Admission::Culled(CullReason::NoFreeVoice),
        "equal priority must not evict"
    );
}

#[test]
fn a_lowest_priority_sound_never_evicts_anything() {
    let mut mixer = Mixer::new(tiny_budget());
    let clip = flat(4096, 32767, 44100);
    for key in 0..2u32 {
        let mut low = Play::new(Arc::clone(&clip), EventKey(key));
        low.priority = Priority::Low;
        mixer.play(low);
    }
    let mut lowest = Play::new(clip, EventKey(9));
    lowest.priority = Priority::Lowest;
    assert_eq!(
        mixer.play(lowest),
        Admission::Culled(CullReason::NoFreeVoice)
    );
    assert_eq!(mixer.stats().evicted, 0);
}

#[test]
fn the_two_and_three_dimensional_budgets_are_counted_separately() {
    let mut mixer = Mixer::new(tiny_budget());
    let clip = flat(4096, 32767, 44100);
    for key in 0..2u32 {
        assert!(matches!(
            mixer.play(Play::new(Arc::clone(&clip), EventKey(key))),
            Admission::Started(_)
        ));
    }
    for key in 2..4u32 {
        let mut request = Play::new(Arc::clone(&clip), EventKey(key));
        request.spatial = Some(Spatial {
            position: [0.0, 0.0, 0.0],
            min_range: 100.0,
            max_range: 900.0,
            min_gain: 0.0,
        });
        assert!(matches!(mixer.play(request), Admission::Started(_)));
    }
    assert_eq!(mixer.active(), 4);
    assert_eq!(mixer.stats().active_2d, 2);
    assert_eq!(mixer.stats().active_3d, 2);
}

#[test]
fn a_hundred_requests_against_a_budget_of_four_start_four() {
    let mut mixer = Mixer::new(MixerConfig {
        voices_2d: 4,
        voices_3d: 25,
        ..MixerConfig::default()
    });
    let clip = flat(44100, 32767, 44100);
    for key in 0..100u32 {
        mixer.play(Play::new(Arc::clone(&clip), EventKey(key)));
    }
    assert_eq!(mixer.stats().active_2d, 4);
    assert_eq!(mixer.stats().started + mixer.stats().culled(), 100);
}

#[test]
fn stopping_by_key_stops_every_instance() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);
    for _ in 0..3 {
        mixer.play(Play::new(Arc::clone(&clip), EventKey(5)));
    }
    mixer.play(Play::new(clip, EventKey(6)));
    assert_eq!(mixer.active(), 4);
    mixer.stop_key(EventKey(5));
    assert_eq!(mixer.active(), 1);
}

#[test]
fn stopping_by_owner_stops_every_instance() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);
    for key in 0..3u32 {
        let mut request = Play::new(Arc::clone(&clip), EventKey(key));
        request.owner = Some(11);
        mixer.play(request);
    }
    mixer.play(Play::new(clip, EventKey(99)));
    mixer.stop_owner(11);
    assert_eq!(mixer.active(), 1);
}

#[test]
fn stopping_one_owners_event_leaves_its_other_sounds_and_other_owners() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let clip = flat(4096, 32767, 44100);
    for (owner, key) in [(11, 5), (11, 6), (12, 5)] {
        let mut request = Play::new(Arc::clone(&clip), EventKey(key));
        request.owner = Some(owner);
        mixer.play(request);
    }
    mixer.stop_owner_key(11, EventKey(5));
    assert_eq!(mixer.active(), 2);
}

#[test]
fn an_empty_clip_is_refused_rather_than_dividing_by_zero() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let empty = Arc::new(Clip::from_pcm16(Pcm16 {
        sample_rate: 44100,
        channels: 1,
        samples: Vec::new(),
    }));
    assert_eq!(
        mixer.play(Play::new(empty, EventKey(1))),
        Admission::Culled(CullReason::Empty)
    );
}

#[test]
fn rendering_into_an_empty_or_odd_buffer_does_nothing_bad() {
    let mut mixer = Mixer::new(MixerConfig::default());
    mixer.play(Play::new(flat(64, 32767, 44100), EventKey(1)));
    let mut nothing: Vec<f32> = Vec::new();
    mixer.render(&mut nothing, 2, 44100);
    let mut odd = vec![0.0f32; 7];
    mixer.render(&mut odd, 2, 44100);
    let mut zero_rate = vec![0.0f32; 8];
    mixer.render(&mut zero_rate, 2, 0);
    let mut zero_channels = vec![0.0f32; 8];
    mixer.render(&mut zero_channels, 0, 44100);
}

#[test]
fn the_summary_line_names_every_counter() {
    let mut mixer = Mixer::new(tiny_budget());
    mixer.play(Play::new(flat(64, 32767, 44100), EventKey(1)));
    let line = mixer.stats().summary();
    for token in ["voices", "started", "culled", "range", "budget", "stereo"] {
        assert!(line.contains(token), "{line} is missing {token}");
    }
}

#[test]
fn a_bus_gain_of_zero_silences_only_that_bus() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let mut quiet = Play::new(flat(64, 32767, 44100), EventKey(1));
    quiet.bus = 1;
    let mut loud = Play::new(flat(64, 32767, 44100), EventKey(2));
    loud.bus = 2;
    mixer.play(quiet);
    mixer.play(loud);
    mixer.set_bus_gain(1, 0.0);

    let mut out = vec![0.0f32; 4];
    mixer.render(&mut out, 2, 44100);
    let want = 0.5f32.sqrt();
    assert!(near(out[0], want), "{} want {want}", out[0]);
}

#[test]
fn a_stream_counts_against_the_stream_budget_not_the_2d_one() {
    let mut mixer = Mixer::new(tiny_budget());
    let clip = flat(64, 32767, 1000);
    for key in 0..2u32 {
        mixer.play(Play::new(Arc::clone(&clip), EventKey(key)));
    }
    let mut music = Play::new(Arc::clone(&clip), EventKey(9));
    music.stream = true;
    assert!(matches!(mixer.play(music.clone()), Admission::Started(_)));
    assert_eq!(
        (mixer.stats().active_2d, mixer.stats().active_stream),
        (2, 1)
    );
    assert_eq!(
        mixer.play(music),
        Admission::Culled(CullReason::NoFreeVoice),
        "one stream channel, already taken"
    );
}

#[test]
fn a_clip_with_repeats_plays_that_many_more_times_then_ends() {
    let mut mixer = Mixer::new(tiny_budget());
    let mut play = Play::new(flat(10, 32767, 1000), EventKey(1));
    play.repeats = 2;
    mixer.play(play);
    let mut out = vec![0.0f32; 2 * 29];
    mixer.render(&mut out, 2, 1000);
    assert_eq!(mixer.active(), 1, "29 of 30 frames played");
    assert!(out.iter().all(|s| *s > 0.0), "no gap between the plays");
    let mut rest = vec![0.0f32; 2 * 4];
    mixer.render(&mut rest, 2, 1000);
    assert_eq!(mixer.active(), 0);
}

#[test]
fn a_paused_voice_holds_its_place_and_a_faded_one_ends() {
    let mut mixer = Mixer::new(tiny_budget());
    let mut play = Play::new(flat(100, 32767, 1000), EventKey(1));
    play.stream = true;
    let id = VoiceId(77);
    play.id = Some(id);
    mixer.play(play);
    let mut out = vec![0.0f32; 2 * 10];
    mixer.render(&mut out, 2, 1000);
    mixer.pause(id, 0);
    let mut held = vec![0.0f32; 2 * 500];
    mixer.render(&mut held, 2, 1000);
    assert!(held.iter().all(|s| *s == 0.0), "silent while held");
    assert!(mixer.is_live(id), "held, not ended");
    mixer.resume(id, 0);
    let mut back = vec![0.0f32; 2 * 89];
    mixer.render(&mut back, 2, 1000);
    assert!(mixer.is_live(id), "90 of 100 frames played");
    mixer.fade_out(id, 5);
    let mut tail = vec![0.0f32; 2 * 8];
    mixer.render(&mut tail, 2, 1000);
    assert!(!mixer.is_live(id), "the fade-out ended it");
}

#[test]
fn pause_all_holds_every_voice_but_the_excluded_bus_and_resume_all_undoes_exactly_that() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let world_id = VoiceId(1);
    let mut world_sound = Play::new(flat(100, 32767, 1000), EventKey(1));
    world_sound.id = Some(world_id);
    let music_id = VoiceId(2);
    let mut music = Play::new(flat(100, 32767, 1000), EventKey(2));
    music.bus = 4;
    music.id = Some(music_id);
    mixer.play(world_sound);
    mixer.play(music);

    mixer.pause_all(Some(4));
    let mut held = vec![0.0f32; 2 * 20];
    mixer.render(&mut held, 2, 1000);
    assert!(held.iter().any(|s| *s != 0.0), "music keeps playing");
    assert!(mixer.is_live(world_id), "held, not ended");

    let click_id = VoiceId(3);
    let mut click = Play::new(flat(5, 32767, 1000), EventKey(3));
    click.id = Some(click_id);
    mixer.play(click);
    let mut during = vec![0.0f32; 2 * 5];
    mixer.render(&mut during, 2, 1000);
    assert!(
        during.iter().any(|s| *s != 0.0),
        "a sound queued while held still plays"
    );
    assert!(!mixer.is_live(click_id), "it ran to the end on schedule");

    mixer.resume_all();
    let mut resumed = vec![0.0f32; 2 * 10];
    mixer.render(&mut resumed, 2, 1000);
    assert!(
        resumed.iter().any(|s| *s != 0.0),
        "the held world sound is audible again"
    );
    assert!(mixer.is_live(world_id));
}

#[test]
fn resume_all_leaves_an_individually_paused_voice_held() {
    let mut mixer = Mixer::new(MixerConfig::default());
    let music_id = VoiceId(1);
    let mut music = Play::new(flat(100, 32767, 1000), EventKey(1));
    music.id = Some(music_id);
    mixer.play(music);

    mixer.pause(music_id, 0);
    mixer.pause_all(None);
    mixer.resume_all();

    let mut out = vec![0.0f32; 2 * 10];
    mixer.render(&mut out, 2, 1000);
    assert!(
        out.iter().all(|s| *s == 0.0),
        "individually paused before pause_all, so resume_all must not wake it"
    );
    assert!(mixer.is_live(music_id), "held, not ended");
}

#[test]
fn a_moved_emitter_carries_its_voice_with_it() {
    let mut mixer = Mixer::new(MixerConfig::default());
    mixer.set_listener(Listener {
        position: [0.0, 0.0, 0.0],
        right: [1.0, 0.0, 0.0],
        gain: 1.0,
    });
    let mut request = Play::new(flat(64, 32767, 44100), EventKey(1));
    request.owner = Some(9);
    request.looping = true;
    request.spatial = Some(Spatial {
        position: [10.0, 0.0, 0.0],
        min_range: 100.0,
        max_range: 1000.0,
        min_gain: 0.0,
    });
    mixer.play(request);
    mixer.move_owners(&[(4, [10.0, 0.0, 0.0]), (9, [-10.0, 0.0, 0.0])]);

    let mut out = vec![0.0f32; 8];
    mixer.render(&mut out, 2, 44100);
    assert!(
        out[0] > out[1],
        "left {} should beat right {}",
        out[0],
        out[1]
    );
}
