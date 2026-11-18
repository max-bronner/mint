use std::sync::Arc;

use mint_audio::device::{DeviceFormat, Source};
use mint_audio::{
    offline, AudioEngine, Clip, EventKey, Listener, Mixer, MixerConfig, Play, Spatial,
};

struct MixerSource(Mixer);

impl Source for MixerSource {
    fn render(&mut self, out: &mut [f32], format: DeviceFormat) {
        self.0.render(out, format.channels, format.sample_rate);
    }
}

fn square_wav(frames: usize, period: usize, rate: u32, amplitude: i16) -> Vec<u8> {
    let mut data = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let value = if (frame / period.max(1)) % 2 == 0 {
            amplitude
        } else {
            -amplitude
        };
        data.extend_from_slice(&value.to_le_bytes());
    }

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

#[test]
fn a_wav_goes_in_and_a_playable_wav_comes_out() {
    let bytes = square_wav(4410, 50, 44_100, 20_000);
    let clip = Arc::new(Clip::from_wav(&bytes).expect("the synthetic file must decode"));
    assert_eq!(clip.frames(), 4410);
    assert_eq!(clip.duration_ms(), 100);

    let mut source = MixerSource(Mixer::new(MixerConfig::default()));
    source.0.play(Play::new(clip, EventKey(1)));

    let format = DeviceFormat::CD;
    let mixed = offline::render(&mut source, format, 4410);
    let file = offline::to_wav(&mixed, format);

    let info = mint_audio_format::probe_wav(&file).expect("the mix must be a readable WAV");
    assert_eq!(info.format.sample_rate, 44_100);
    assert_eq!(info.format.channels, 2);
    assert_eq!(info.frames, 4410);
    assert_eq!(info.duration_ms(), 100);

    let decoded = mint_audio_format::decode_wav(&file).expect("and must decode");
    let peak = decoded.samples.iter().map(|s| s.abs()).max().unwrap_or(0);
    assert!(
        peak > 8_000,
        "the mix should not be near-silent: peak {peak}"
    );
}

#[test]
fn chunked_rendering_matches_one_call() {
    let bytes = square_wav(2000, 37, 22_050, 30_000);
    let clip = Arc::new(Clip::from_wav(&bytes).unwrap());
    let format = DeviceFormat::CD;

    let mut whole = MixerSource(Mixer::new(MixerConfig::default()));
    whole.0.play(Play::new(Arc::clone(&clip), EventKey(1)));
    let a = offline::render(&mut whole, format, 8000);

    let mut pieces = MixerSource(Mixer::new(MixerConfig::default()));
    pieces.0.play(Play::new(clip, EventKey(1)));
    let b = offline::render_chunked(&mut pieces, format, 8000, 480);

    assert_eq!(a.len(), b.len());
    for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            (x - y).abs() < 1.0e-6,
            "sample {index} differs: {x} against {y}"
        );
    }
}

#[test]
fn the_mix_survives_a_device_at_an_unrelated_rate() {
    let bytes = square_wav(4410, 50, 44_100, 20_000);
    let clip = Arc::new(Clip::from_wav(&bytes).unwrap());
    let format = DeviceFormat {
        sample_rate: 48_000,
        channels: 2,
    };
    let mut source = MixerSource(Mixer::new(MixerConfig::default()));
    source.0.play(Play::new(clip, EventKey(1)));
    let mixed = offline::render_chunked(&mut source, format, 4800, 240);

    assert!(mixed.iter().all(|s| s.is_finite()), "no NaNs");
    let peak = mixed.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    assert!(peak > 0.25, "peak {peak}");
}

#[test]
fn a_moving_listener_changes_which_side_a_sound_is_on() {
    let bytes = square_wav(44_100, 50, 44_100, 30_000);
    let clip = Arc::new(Clip::from_wav(&bytes).unwrap());
    let mut source = MixerSource(Mixer::new(MixerConfig::default()));
    let mut request = Play::new(clip, EventKey(1));
    request.looping = true;
    request.spatial = Some(Spatial {
        position: [200.0, 0.0, 0.0],
        min_range: 100.0,
        max_range: 2000.0,
        min_gain: 0.0,
    });
    source.0.play(request);

    let format = DeviceFormat::CD;
    let right = offline::render(&mut source, format, 128);
    source.0.set_listener(Listener {
        position: [400.0, 0.0, 0.0],
        right: [1.0, 0.0, 0.0],
        gain: 1.0,
    });
    let left = offline::render(&mut source, format, 128);

    let energy = |buffer: &[f32], channel: usize| -> f32 {
        buffer
            .iter()
            .skip(channel)
            .step_by(2)
            .map(|s| s.abs())
            .sum()
    };
    assert!(
        energy(&right, 1) > energy(&right, 0),
        "emitter to the right of the origin should be louder on the right"
    );
    assert!(
        energy(&left, 0) > energy(&left, 1),
        "after the listener passes it, louder on the left"
    );
}

#[test]
fn a_silent_engine_accepts_every_call_and_reports_why() {
    let engine = AudioEngine::silent();
    assert!(engine.is_silent());
    assert!(engine.device().is_none());
    assert!(engine.format().is_none());
    assert!(!engine.describe().is_empty());

    let bytes = square_wav(100, 10, 44_100, 1000);
    let clip = Arc::new(Clip::from_wav(&bytes).unwrap());
    engine.play(Play::new(clip, EventKey(1)));
    engine.set_listener(Listener::default());
    engine.set_bus_gain(0, 0.5);
    engine.set_master_gain(0.5);
    engine.stop_key(EventKey(1));
    engine.stop_owner(1);
    engine.stop_all();
    assert_eq!(engine.stats().started, 0);
}
