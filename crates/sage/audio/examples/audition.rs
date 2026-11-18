use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mint_audio::device::DeviceFormat;
use mint_audio::{offline, Listener, Mixer, MixerConfig, Play, Priority, Rolloff, Spatial};
use mint_sage_audio::{load_library, AudioLibrary, EventRequest, Outcome, SageAudio};
use mint_sage_big::BigArchive;
use mint_vfs::Vfs;

const ORBIT_SECONDS: usize = 8;

const REQUESTS_PER_SECOND: usize = 4;

fn main() -> ExitCode {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut event: Option<String> = None;
    let mut wav: Option<PathBuf> = None;
    let mut rolloff = Rolloff::default();
    let mut rate: Option<u32> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--install" => match args.next() {
                Some(path) => roots.push(PathBuf::from(path)),
                None => return usage("--install wants a path"),
            },
            "--event" => event = args.next(),
            "--rolloff" => {
                rolloff = match args.next().as_deref() {
                    Some("linear") => Rolloff::Linear,
                    Some("inverse" | "inverse-clamped") => Rolloff::InverseClamped,
                    Some("inverse-normalised") => Rolloff::InverseNormalised,
                    _ => return usage("--rolloff wants inverse, inverse-normalised or linear"),
                }
            }
            "--wav" => match args.next() {
                Some(path) => wav = Some(PathBuf::from(path)),
                None => return usage("--wav wants a path"),
            },
            "--rate" => match args.next().and_then(|r| r.parse::<u32>().ok()) {
                Some(hz) if hz > 0 => rate = Some(hz),
                _ => return usage("--rate wants a positive sample rate in Hz"),
            },
            other if other.starts_with("--") => return usage("unknown option"),
            other => roots.push(PathBuf::from(other)),
        }
    }
    if roots.is_empty() {
        return usage("at least one install directory is needed");
    }

    let mut vfs = Vfs::new();
    for root in &roots {
        for path in mint_sage_big::archives_under(root) {
            if let Ok(archive) = BigArchive::open(&path) {
                vfs.mount(Arc::new(archive));
            }
        }
    }
    println!("{} paths mounted", vfs.len());

    let library = match load_library(&vfs) {
        Ok(library) => library,
        Err(error) => {
            eprintln!("could not read the audio INI: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("{} audio definitions", library.len());

    let name = match event.or_else(|| first_playable(&library, &vfs)) {
        Some(name) => name,
        None => {
            eprintln!("no event named and none found that resolves to a readable file");
            return ExitCode::FAILURE;
        }
    };
    let Some(def) = library.get(&name).cloned() else {
        eprintln!("`{name}` is not a defined audio event");
        return ExitCode::FAILURE;
    };
    let min_range = def.min_range.unwrap_or(160.0);
    let max_range = def.max_range.unwrap_or(640.0);
    println!(
        "auditioning `{}`: {} variant(s), priority {}, limit {}, range {min_range:.0}..{max_range:.0}, \
         duration {} ms (header only, no decode)",
        def.name,
        def.sounds.len(),
        def.priority.as_str(),
        def.limit,
        library.event_duration_ms(&vfs, &name).unwrap_or(0)
    );

    for path in library.variant_paths(&def) {
        match vfs
            .read(&path)
            .ok()
            .as_deref()
            .map(mint_audio_format::probe_wav)
        {
            Some(Ok(info)) => println!(
                "  {path}: {} Hz, {} channel(s), {} bit, tag {}, {} ms",
                info.format.sample_rate,
                info.format.channels,
                info.format.bits_per_sample,
                info.format.raw_tag,
                info.duration_ms()
            ),
            Some(Err(error)) => println!("  {path}: will not probe - {error}"),
            None => println!("  {path}: not in any mounted archive"),
        }
    }

    match wav {
        Some(path) => render_to_file(
            &vfs, &library, &name, min_range, max_range, rolloff, rate, &path,
        ),
        None => play_live(&vfs, library, &name, min_range, max_range, rolloff),
    }
}

fn play_live(
    vfs: &Vfs,
    library: AudioLibrary,
    name: &str,
    min_range: f32,
    max_range: f32,
    rolloff: Rolloff,
) -> ExitCode {
    let mut audio = SageAudio::open(library);
    audio.set_rolloff(rolloff);
    println!("{}", audio.readout());
    if audio.is_silent() {
        eprintln!("no output device - use --wav to render the same mix to a file instead");
        return ExitCode::FAILURE;
    }

    let radius = (min_range + max_range) * 0.5;
    let started = Instant::now();
    let mut fired = 0u32;
    let interval = Duration::from_millis(1000 / (REQUESTS_PER_SECOND.max(1) as u64));
    let orbit = ORBIT_SECONDS as f32;
    while started.elapsed().as_secs_f32() < orbit {
        let t = started.elapsed().as_secs_f32() / orbit;
        let (x, y) = circle(t);
        let position = [x * radius, y * radius, 0.0];
        audio.set_listener(Listener {
            position: [0.0, 0.0, 0.0],
            right: [1.0, 0.0, 0.0],
            gain: 1.0,
        });
        let outcome = audio.play(vfs, &EventRequest::at(name, position));
        if outcome == Outcome::Queued {
            fired = fired.saturating_add(1);
        }
        std::thread::sleep(interval);
    }
    std::thread::sleep(Duration::from_millis(1500));

    println!("{fired} request(s) queued over {ORBIT_SECONDS}s");
    println!("{}", audio.readout());
    ExitCode::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn render_to_file(
    vfs: &Vfs,
    library: &AudioLibrary,
    name: &str,
    min_range: f32,
    max_range: f32,
    rolloff: Rolloff,
    rate: Option<u32>,
    path: &PathBuf,
) -> ExitCode {
    let Some(def) = library.get(name) else {
        eprintln!("`{name}` is not a defined audio event");
        return ExitCode::FAILURE;
    };
    let Some(variant) = library.variant_paths(def).into_iter().next() else {
        eprintln!("`{name}` names no file");
        return ExitCode::FAILURE;
    };
    let Ok(bytes) = vfs.read(&variant) else {
        eprintln!("{variant} is not in any mounted archive");
        return ExitCode::FAILURE;
    };
    let clip = match mint_audio::Clip::from_wav(&bytes) {
        Ok(clip) => Arc::new(clip),
        Err(error) => {
            eprintln!("{variant}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let format = match rate {
        Some(sample_rate) => DeviceFormat {
            sample_rate,
            ..DeviceFormat::CD
        },
        None => DeviceFormat::CD,
    };
    println!(
        "source {} Hz {} ch -> output {} Hz {} ch, resample step {:.6}",
        clip.sample_rate(),
        clip.channels(),
        format.sample_rate,
        format.channels,
        f64::from(clip.sample_rate()) / f64::from(format.sample_rate.max(1))
    );
    let mut mixer = Mixer::new(MixerConfig {
        rolloff,
        ..MixerConfig::default()
    });
    let radius = (min_range + max_range) * 0.5;
    let total_frames = usize::try_from(format.sample_rate)
        .unwrap_or(0)
        .saturating_mul(ORBIT_SECONDS);
    let chunk = 480;
    let mut out: Vec<f32> = Vec::with_capacity(total_frames * 2);
    let mut buffer = vec![0.0f32; chunk * 2];
    let mut done = 0usize;
    let mut next_request = 0usize;
    let step = usize::try_from(format.sample_rate)
        .unwrap_or(0)
        .checked_div(REQUESTS_PER_SECOND)
        .unwrap_or(1);

    mixer.set_listener(Listener {
        position: [0.0, 0.0, 0.0],
        right: [1.0, 0.0, 0.0],
        gain: 1.0,
    });
    while done < total_frames {
        if done >= next_request {
            let t = frame_fraction(done, total_frames);
            let (x, y) = circle(t);
            let mut request = Play::new(Arc::clone(&clip), mint_audio::EventKey(0));
            request.priority = Priority::High;
            request.limit = def.limit;
            request.spatial = Some(Spatial {
                position: [x * radius, y * radius, 0.0],
                min_range,
                max_range,
                min_gain: 0.0,
            });
            mixer.play(request);
            next_request = next_request.saturating_add(step.max(1));
        }
        let want = chunk.min(total_frames.saturating_sub(done));
        let slice = buffer.get_mut(..want * 2).unwrap_or(&mut []);
        mixer.render(slice, format.channels, format.sample_rate);
        out.extend_from_slice(slice);
        done = done.saturating_add(want);
    }

    let peak = out.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    let file = offline::to_wav(&out, format);
    if let Err(error) = std::fs::write(path, &file) {
        eprintln!("could not write {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "wrote {} - {}s, {} bytes, peak {peak:.3}, rolloff {}",
        path.display(),
        ORBIT_SECONDS,
        file.len(),
        rolloff.label()
    );
    println!("{}", mixer.stats().summary());
    if peak <= 0.0 {
        eprintln!("the mix is silent, which means nothing above this worked");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn frame_fraction(done: usize, total: usize) -> f32 {
    let total = total.max(1);
    let permille = done.saturating_mul(1000).checked_div(total).unwrap_or(0);
    let permille = u16::try_from(permille.min(1000)).unwrap_or(1000);
    f32::from(permille) / 1000.0
}

fn circle(t: f32) -> (f32, f32) {
    let u = (t * 4.0).rem_euclid(4.0);
    if u < 1.0 {
        (1.0 - u * 2.0, 1.0)
    } else if u < 2.0 {
        (-1.0, 1.0 - (u - 1.0) * 2.0)
    } else if u < 3.0 {
        ((u - 2.0) * 2.0 - 1.0, -1.0)
    } else {
        (1.0, (u - 3.0) * 2.0 - 1.0)
    }
}

fn first_playable(library: &AudioLibrary, vfs: &Vfs) -> Option<String> {
    for (_, def) in library.index().events() {
        if !def.has_audio() {
            continue;
        }
        for path in library.variant_paths(def) {
            if path.ends_with(".wav") && vfs.contains(&path) {
                return Some(def.name.clone());
            }
        }
    }
    None
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    eprintln!(
        "usage: audition <install dir> [--install <dir>] [--event <name>]          [--wav <file>] [--rate <hz>] [--rolloff linear|inverse]"
    );
    ExitCode::FAILURE
}
